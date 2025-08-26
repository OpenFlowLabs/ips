use crate::actions::{Manifest, Property};
use crate::fmri::Fmri;
use crate::image::Image;
use crate::solver::{SolverError, SolverProblemKind};
use miette::Diagnostic;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
#[error("solver advice error: {message}")]
#[diagnostic(
    code(ips::solver_advice_error::generic),
    help("Ensure the image catalogs are built and accessible.")
)]
pub struct AdviceError {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdviceIssue {
    pub path: Vec<String>,
    pub stem: String,
    pub constraint_release: Option<String>,
    pub constraint_branch: Option<String>,
    pub details: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AdviceReport {
    pub issues: Vec<AdviceIssue>,
}

#[derive(Debug, Default, Clone)]
pub struct AdviceOptions {
    pub max_depth: usize,            // 0 = unlimited
    pub dependency_cap: usize,       // 0 = unlimited per node
}

#[derive(Default)]
struct Ctx {
    // caches
    catalog_cache: HashMap<String, Vec<(String, Fmri)>>, // stem -> [(publisher, fmri)]
    manifest_cache: HashMap<String, Manifest>,            // fmri string -> manifest
    lock_cache: HashMap<String, Option<String>>,          // stem -> incorporated release
    candidate_cache: HashMap<(String, Option<String>, Option<String>, Option<String>), Option<Fmri>>, // (stem, rel, branch, publisher)
    publisher_filter: Option<String>,
    cap: usize,
}

impl Ctx {
    fn new(publisher_filter: Option<String>, cap: usize) -> Self {
        Self { publisher_filter, cap, ..Default::default() }
    }
}

pub fn advise_from_error(image: &Image, err: &SolverError, opts: AdviceOptions) -> Result<AdviceReport, AdviceError> {
    let mut report = AdviceReport::default();
    let Some(problem) = err.problem() else {
        return Ok(report);
    };

    match &problem.kind {
        SolverProblemKind::NoCandidates { stem, release, branch } => {
            // Advise directly on the missing root
            let mut ctx = Ctx::new(None, opts.dependency_cap);
            let details = build_missing_detail(image, &mut ctx, stem, release.as_deref(), branch.as_deref());
            report.issues.push(AdviceIssue {
                path: vec![stem.clone()],
                stem: stem.clone(),
                constraint_release: release.clone(),
                constraint_branch: branch.clone(),
                details,
            });
            Ok(report)
        }
        SolverProblemKind::Unsolvable => {
            // Fall back to analyzing roots and traversing dependencies to find a missing candidate leaf.
            let mut ctx = Ctx::new(None, opts.dependency_cap);
            for root in &problem.roots {
                let root_fmri = match find_best_candidate(image, &mut ctx, &root.stem, root.version_req.as_deref(), root.branch.as_deref()) {
                    Ok(Some(f)) => f,
                    _ => {
                        // Missing root candidate
                        let details = build_missing_detail(image, &mut ctx, &root.stem, root.version_req.as_deref(), root.branch.as_deref());
                        report.issues.push(AdviceIssue {
                            path: vec![root.stem.clone()],
                            stem: root.stem.clone(),
                            constraint_release: root.version_req.clone(),
                            constraint_branch: root.branch.clone(),
                            details,
                        });
                        continue;
                    }
                };

                // Depth-first traversal looking for missing candidates
                let mut path = vec![root.stem.clone()];
                let mut seen = std::collections::HashSet::new();
                advise_recursive(image, &mut ctx, &root_fmri, &mut path, 1, opts.max_depth, &mut seen, &mut report)?;
            }
            Ok(report)
        }
    }
}

fn advise_recursive(
    image: &Image,
    ctx: &mut Ctx,
    fmri: &Fmri,
    path: &mut Vec<String>,
    depth: usize,
    max_depth: usize,
    seen: &mut std::collections::HashSet<String>,
    report: &mut AdviceReport,
) -> Result<(), AdviceError> {
    if max_depth != 0 && depth > max_depth { return Ok(()); }
    let manifest = get_manifest_cached(image, ctx, fmri)?;

    let mut processed = 0usize;
    for dep in manifest.dependencies.iter().filter(|d| d.dependency_type == "require" || d.dependency_type == "incorporate") {
        let Some(df) = &dep.fmri else { continue; };
        let dep_stem = df.stem().to_string();
        let (rel, br) = extract_constraint(&dep.optional);

        if ctx.cap != 0 && processed >= ctx.cap { break; }
        processed += 1;

        match find_best_candidate(image, ctx, &dep_stem, rel.as_deref(), br.as_deref())? {
            Some(next) => {
                if !seen.contains(&dep_stem) {
                    seen.insert(dep_stem.clone());
                    path.push(dep_stem.clone());
                    advise_recursive(image, ctx, &next, path, depth + 1, max_depth, seen, report)?;
                    path.pop();
                }
            }
            None => {
                let details = build_missing_detail(image, ctx, &dep_stem, rel.as_deref(), br.as_deref());
                report.issues.push(AdviceIssue {
                    path: path.clone(),
                    stem: dep_stem.clone(),
                    constraint_release: rel.clone(),
                    constraint_branch: br.clone(),
                    details,
                });
            }
        }
    }
    Ok(())
}

fn extract_constraint(optional: &[Property]) -> (Option<String>, Option<String>) {
    let mut release: Option<String> = None;
    let mut branch: Option<String> = None;
    for p in optional {
        match p.key.as_str() {
            "release" => release = Some(p.value.clone()),
            "branch" => branch = Some(p.value.clone()),
            _ => {}
        }
    }
    (release, branch)
}

fn build_missing_detail(image: &Image, ctx: &mut Ctx, stem: &str, release: Option<&str>, branch: Option<&str>) -> String {
    let mut available: Vec<String> = Vec::new();
    if let Ok(list) = query_catalog_cached_mut(image, ctx, stem) {
        for (pubname, fmri) in list {
            if let Some(ref pfilter) = ctx.publisher_filter { if &pubname != pfilter { continue; } }
            if fmri.stem() != stem { continue; }
            let ver = fmri.version();
            if ver.is_empty() { continue; }
            available.push(ver);
        }
    }
    available.sort();
    available.dedup();

    let available_str = if available.is_empty() { "<none>".to_string() } else { available.join(", ") };
    let lock = get_incorporated_release_cached(image, ctx, stem).ok().flatten();

    match (release, branch, lock.as_deref()) {
        (Some(r), Some(b), Some(lr)) => format!("Required release={}, branch={} not found. Image incorporation lock release={} may constrain candidates. Available versions: {}", r, b, lr, available_str),
        (Some(r), Some(b), None) => format!("Required release={}, branch={} not found. Available versions: {}", r, b, available_str),
        (Some(r), None, Some(lr)) => format!("Required release={} not found. Image incorporation lock release={} present. Available versions: {}", r, lr, available_str),
        (Some(r), None, None) => format!("Required release={} not found. Available versions: {}", r, available_str),
        (None, Some(b), Some(lr)) => format!("Required branch={} not found. Image incorporation lock release={} present. Available versions: {}", b, lr, available_str),
        (None, Some(b), None) => format!("Required branch={} not found. Available versions: {}", b, available_str),
        (None, None, Some(lr)) => format!("No candidates matched. Image incorporation lock release={} present. Available versions: {}", lr, available_str),
        (None, None, None) => format!("No candidates matched. Available versions: {}", available_str),
    }
}

fn find_best_candidate(
    image: &Image,
    ctx: &mut Ctx,
    stem: &str,
    req_release: Option<&str>,
    req_branch: Option<&str>,
) -> Result<Option<Fmri>, AdviceError> {
    let key = (
        stem.to_string(),
        req_release.map(|s| s.to_string()),
        req_branch.map(|s| s.to_string()),
        ctx.publisher_filter.clone(),
    );
    if let Some(cached) = ctx.candidate_cache.get(&key) { return Ok(cached.clone()); }

    let lock_release = if req_release.is_none() { get_incorporated_release_cached(image, ctx, stem).ok().flatten() } else { None };

    let mut candidates: Vec<(String, Fmri)> = Vec::new();
    for (pubf, pfmri) in query_catalog_cached(image, ctx, stem)? {
        if let Some(ref pfilter) = ctx.publisher_filter { if &pubf != pfilter { continue; } }
        if pfmri.stem() != stem { continue; }
        let ver = pfmri.version();
        if ver.is_empty() { continue; }
        let rel = version_release(&ver);
        let br = version_branch(&ver);
        if let Some(req_r) = req_release { if Some(req_r) != rel.as_deref() { continue; } } else if let Some(lock_r) = lock_release.as_deref() { if Some(lock_r) != rel.as_deref() { continue; } }
        if let Some(req_b) = req_branch { if Some(req_b) != br.as_deref() { continue; } }
        candidates.push((ver.clone(), pfmri.clone()));
    }

    candidates.sort_by(|a, b| a.0.cmp(&b.0));
    let res = candidates.pop().map(|x| x.1);
    ctx.candidate_cache.insert(key, res.clone());
    Ok(res)
}

fn version_release(version: &str) -> Option<String> {
    version.split_once(',').map(|(rel, _)| rel.to_string())
}

fn version_branch(version: &str) -> Option<String> {
    if let Some((_, rest)) = version.split_once(',') { return rest.split_once('-').map(|(b, _)| b.to_string()); }
    None
}

fn query_catalog_cached(
    image: &Image,
    ctx: &Ctx,
    stem: &str,
) -> Result<Vec<(String, Fmri)>, AdviceError> {
    if let Some(v) = ctx.catalog_cache.get(stem) { return Ok(v.clone()); }
    let mut tmp = Ctx { catalog_cache: ctx.catalog_cache.clone(), ..Default::default() };
    query_catalog_cached_mut(image, &mut tmp, stem)
}

fn query_catalog_cached_mut(
    image: &Image,
    ctx: &mut Ctx,
    stem: &str,
) -> Result<Vec<(String, Fmri)>, AdviceError> {
    if let Some(v) = ctx.catalog_cache.get(stem) { return Ok(v.clone()); }
    let mut out = Vec::new();
    let res = image.query_catalog(Some(stem)).map_err(|e| AdviceError{ message: format!("Failed to query catalog for {}: {}", stem, e) })?;
    for p in res { out.push((p.publisher, p.fmri)); }
    ctx.catalog_cache.insert(stem.to_string(), out.clone());
    Ok(out)
}

fn get_manifest_cached(image: &Image, ctx: &mut Ctx, fmri: &Fmri) -> Result<Manifest, AdviceError> {
    let key = fmri.to_string();
    if let Some(m) = ctx.manifest_cache.get(&key) { return Ok(m.clone()); }
    let manifest_opt = image.get_manifest_from_catalog(fmri).map_err(|e| AdviceError { message: format!("Failed to load manifest for {}: {}", fmri.to_string(), e) })?;
    let manifest = manifest_opt.unwrap_or_else(Manifest::new);
    ctx.manifest_cache.insert(key, manifest.clone());
    Ok(manifest)
}

fn get_incorporated_release_cached(image: &Image, ctx: &mut Ctx, stem: &str) -> Result<Option<String>, AdviceError> {
    if let Some(v) = ctx.lock_cache.get(stem) { return Ok(v.clone()); }
    let v = image.get_incorporated_release(stem).map_err(|e| AdviceError{ message: format!("Failed to read incorporation lock for {}: {}", stem, e) })?;
    ctx.lock_cache.insert(stem.to_string(), v.clone());
    Ok(v)
}
