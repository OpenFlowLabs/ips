use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use clap::{ArgAction, Parser, ValueEnum};
use miette::{Diagnostic, IntoDiagnostic, Result};
use thiserror::Error;
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

use libips::image::Image;

#[derive(Parser, Debug)]
#[command(name = "pkgtree", version, about = "Analyze IPS package dependency trees, detect cycles, and advise on failing installs", long_about = None)]
struct Cli {
    /// Path to an IPS image (root containing var/pkg)
    #[arg(short = 'I', long = "image", env = "IPS_IMAGE")]
    image_path: PathBuf,

    /// Publisher to analyze (default: all publishers in the image)
    #[arg(short = 'P', long)]
    publisher: Option<String>,

    /// Only analyze packages whose stem or FMRI contains this substring (case sensitive)
    #[arg(short = 'n', long)]
    package: Option<String>,

    /// Output format for graph mode
    #[arg(short = 'F', long = "format", default_value_t = OutputFormat::Tree)]
    format: OutputFormat,

    /// Maximum depth to print for the tree (0 = unlimited)
    #[arg(short = 'd', long = "max-depth", default_value_t = 0)]
    max_depth: usize,

    /// Detect and report dependency cycles across the analyzed set (graph mode)
    #[arg(short = 'c', long = "detect-cycles", action = ArgAction::SetTrue)]
    detect_cycles: bool,

    /// Emit suggestions to break detected cycles (graph mode)
    #[arg(short = 's', long = "suggest", action = ArgAction::SetTrue)]
    suggest: bool,

    /// Find packages whose dependencies reference missing stems (dangling)
    #[arg(long = "find-dangling", action = ArgAction::SetTrue)]
    find_dangling: bool,

    /// Advise on an install for the given package stem (advisor mode)
    #[arg(long = "advise-install")]
    advise_install: Option<String>,

    /// Analyze a pkg6 solver error text file and suggest fixes (targeted mode)
    #[arg(long = "analyze-solver-error")]
    solver_error_file: Option<PathBuf>,

    /// Maximum recursion depth for advisor mode (default: 2)
    #[arg(long = "advice-depth", default_value_t = 2)]
    advice_depth: usize,

    /// Maximum number of dependencies processed per package in advisor mode (0 = unlimited)
    #[arg(long = "advice-cap", default_value_t = 400)]
    advice_cap: usize,

    /// Increase log verbosity (use multiple times)
    #[arg(short = 'v', long = "verbose", action = ArgAction::Count)]
    verbose: u8,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Tree,
    Json,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Tree => write!(f, "tree"),
            OutputFormat::Json => write!(f, "json"),
        }
    }
}

#[derive(Error, Debug, Diagnostic)]
#[error("pkgtree error: {0}")]
#[diagnostic(
    code(ips::pkgtree_error),
    help("See logs with RUST_LOG=pkgtree=debug for more details.")
)]
struct PkgTreeError(String);

#[derive(Debug, Clone)]
struct Edge {
    to: String,       // target stem
    dep_type: String, // dependency type (e.g., require, incorporate, optional, etc.)
}

#[derive(Debug, Default, Clone)]
struct Graph {
    // stem -> edges
    adj: HashMap<String, Vec<Edge>>,
}

impl Graph {
    fn add_edge(&mut self, from: String, to: String, dep_type: String) {
        self.adj
            .entry(from)
            .or_default()
            .push(Edge { to, dep_type });
    }

    fn stems(&self) -> impl Iterator<Item = &String> {
        self.adj.keys()
    }
}

#[derive(Debug, Clone)]
struct Cycle {
    nodes: Vec<String>, // ordered stems forming the cycle, first == last for readability
    edges: Vec<String>, // edge types along the cycle
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup tracing
    let env_filter = match cli.verbose {
        0 => EnvFilter::from_default_env().add_directive("pkgtree=info".parse().unwrap()),
        1 => EnvFilter::from_default_env().add_directive("pkgtree=debug".parse().unwrap()),
        _ => EnvFilter::from_default_env().add_directive("pkgtree=trace".parse().unwrap()),
    };
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    // Load image
    let image = Image::load(&cli.image_path).map_err(|e| {
        PkgTreeError(format!(
            "Failed to load image at {:?}: {}",
            cli.image_path, e
        ))
    })?;

    // Targeted analysis of solver error file has top priority if provided
    if let Some(err_path) = &cli.solver_error_file {
        analyze_solver_error(&image, cli.publisher.as_deref(), err_path)?;
        return Ok(());
    }

    // Advisor mode has priority if requested
    if let Some(root) = &cli.advise_install {
        let mut ctx = AdviceContext::new(cli.publisher.clone(), cli.advice_cap);
        run_advisor(&image, &mut ctx, root, cli.advice_depth)?;
        return Ok(());
    }

    // Dangling dependency scan has priority over graph mode
    if cli.find_dangling {
        run_dangling_scan(
            &image,
            cli.publisher.as_deref(),
            cli.package.as_deref(),
            cli.format,
        )?;
        return Ok(());
    }

    // Graph mode
    // Query catalog (filtered if --package provided)
    let mut pkgs = if let Some(ref needle) = cli.package {
        image
            .query_catalog(Some(needle.as_str()))
            .map_err(|e| PkgTreeError(format!("Failed to query catalog: {}", e)))?
    } else {
        image
            .query_catalog(None)
            .map_err(|e| PkgTreeError(format!("Failed to query catalog: {}", e)))?
    };

    // Filter by publisher if specified
    if let Some(pubname) = &cli.publisher {
        pkgs.retain(|p| p.publisher == *pubname);
    }

    // Select starting set by package substring if requested
    let filter_substr = cli.package.clone();

    // Build dependency graph from manifests
    let mut graph = Graph::default();

    for p in &pkgs {
        // If filter is set and neither stem nor fmri string contains it, skip
        if let Some(ref needle) = filter_substr {
            let stem = p.fmri.stem().to_string();
            let fmri_str = p.fmri.to_string();
            if !stem.contains(needle) && !fmri_str.contains(needle) {
                continue;
            }
        }

        // Get manifest
        match image.get_manifest_from_catalog(&p.fmri) {
            Ok(Some(manifest)) => {
                let from_stem = p.fmri.stem().to_string();
                for dep in manifest.dependencies {
                    if dep.dependency_type != "require" && dep.dependency_type != "incorporate" {
                        continue;
                    }
                    if let Some(dep_fmri) = dep.fmri {
                        let to_stem = dep_fmri.stem().to_string();
                        graph.add_edge(from_stem.clone(), to_stem, dep.dependency_type.clone());
                    }
                }
            }
            Ok(None) => {
                warn!(fmri=%p.fmri.to_string(), "Manifest not found in catalog");
            }
            Err(err) => {
                warn!(error=%format!("{}", err), fmri=%p.fmri.to_string(), "Failed to get manifest from catalog");
            }
        }
    }

    // If no nodes were added (e.g., filter too narrow), try building graph for all packages to support cycle analysis
    if graph.adj.is_empty() && filter_substr.is_some() {
        info!(
            "No packages matched filter for dependency graph; analyzing full catalog for cycles/tree context."
        );
        for p in &pkgs {
            match image.get_manifest_from_catalog(&p.fmri) {
                Ok(Some(manifest)) => {
                    let from_stem = p.fmri.stem().to_string();
                    for dep in manifest.dependencies {
                        if dep.dependency_type != "require" && dep.dependency_type != "incorporate"
                        {
                            continue;
                        }
                        if let Some(dep_fmri) = dep.fmri {
                            let to_stem = dep_fmri.stem().to_string();
                            graph.add_edge(from_stem.clone(), to_stem, dep.dependency_type.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Determine roots for tree printing
    let roots: Vec<String> = if let Some(ref needle) = filter_substr {
        let mut r = HashSet::new();
        for k in graph.adj.keys() {
            if k.contains(needle) {
                r.insert(k.clone());
            }
        }
        r.into_iter().collect()
    } else {
        graph.adj.keys().cloned().collect()
    };

    // Optionally detect cycles
    let mut cycles: Vec<Cycle> = Vec::new();
    if cli.detect_cycles {
        cycles = detect_cycles(&graph);
    }

    match cli.format {
        OutputFormat::Tree => {
            print_trees(&graph, &roots, cli.max_depth);
            if cli.detect_cycles {
                print_cycles(&cycles);
                if cli.suggest {
                    print_suggestions(&cycles, &graph);
                }
            }
        }
        OutputFormat::Json => {
            use serde::Serialize;
            #[derive(Serialize)]
            struct JsonEdge {
                from: String,
                to: String,
                dep_type: String,
            }
            #[derive(Serialize)]
            struct JsonCycle {
                nodes: Vec<String>,
                edges: Vec<String>,
            }
            #[derive(Serialize)]
            struct Payload {
                edges: Vec<JsonEdge>,
                cycles: Vec<JsonCycle>,
            }

            let mut edges = Vec::new();
            for (from, es) in &graph.adj {
                for e in es {
                    edges.push(JsonEdge {
                        from: from.clone(),
                        to: e.to.clone(),
                        dep_type: e.dep_type.clone(),
                    });
                }
            }
            let cycles_json = cycles
                .iter()
                .map(|c| JsonCycle {
                    nodes: c.nodes.clone(),
                    edges: c.edges.clone(),
                })
                .collect();
            let payload = Payload {
                edges,
                cycles: cycles_json,
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&payload).into_diagnostic()?
            );
        }
    }

    Ok(())
}

// ---------- Advisor mode ----------

#[derive(Debug, Clone)]
struct DepConstraint {
    release: Option<String>,
    branch: Option<String>,
}

#[derive(Debug, Clone)]
struct AdviceIssue {
    path: Vec<String>, // path from root to the missing dependency stem
    stem: String,      // the missing stem
    constraint: DepConstraint,
    details: String, // human description
}

#[derive(Default)]
struct AdviceContext {
    publisher: Option<String>,
    advice_cap: usize,
    // caches
    catalog_cache: HashMap<String, Vec<(String, libips::fmri::Fmri)>>, // stem -> [(publisher, fmri)]
    manifest_cache: HashMap<String, libips::actions::Manifest>,        // fmri string -> manifest
    lock_cache: HashMap<String, Option<String>>,                       // stem -> release lock
    candidate_cache: HashMap<
        (String, Option<String>, Option<String>, Option<String>),
        Option<libips::fmri::Fmri>,
    >, // (stem, rel, branch, publisher)
}

impl AdviceContext {
    fn new(publisher: Option<String>, advice_cap: usize) -> Self {
        AdviceContext {
            publisher,
            advice_cap,
            ..Default::default()
        }
    }
}

fn run_advisor(
    image: &Image,
    ctx: &mut AdviceContext,
    root_stem: &str,
    max_depth: usize,
) -> Result<()> {
    info!("Advisor analyzing installability for root: {}", root_stem);

    // Find best candidate for root
    let root_fmri = match find_best_candidate(image, ctx, root_stem, None, None) {
        Ok(Some(fmri)) => fmri,
        Ok(None) => {
            println!(
                "No candidates found for root package '{}'.\n- Suggestion: run 'pkg6 refresh' to update catalogs.\n- Ensure publisher{} contains the package.",
                root_stem,
                ctx.publisher
                    .as_ref()
                    .map(|p| format!(" '{}')", p))
                    .unwrap_or_else(|| "".to_string())
            );
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    debug!("Chosen root FMRI: {}", root_fmri.to_string());

    // Traverse dependencies up to depth and collect issues
    let mut issues: Vec<AdviceIssue> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut path: Vec<String> = vec![root_stem.to_string()];
    advise_recursive(
        image,
        ctx,
        &root_fmri,
        &mut path,
        1,
        max_depth,
        &mut seen,
        &mut issues,
    )?;

    // Print summary
    if issues.is_empty() {
        println!(
            "No immediate missing dependencies detected up to depth {} for root '{}'.\nIf installs still fail, try running with higher --advice-depth or check solver logs.",
            max_depth, root_stem
        );
    } else {
        println!("Found {} installability issue(s):", issues.len());
        for (i, iss) in issues.iter().enumerate() {
            let constraint_str = format!(
                "{}{}",
                iss.constraint
                    .release
                    .as_ref()
                    .map(|r| format!("release={} ", r))
                    .unwrap_or_default(),
                iss.constraint
                    .branch
                    .as_ref()
                    .map(|b| format!("branch={}", b))
                    .unwrap_or_default(),
            )
            .trim()
            .to_string();
            println!(
                "  {}. {}\n     - Path: {}\n     - Constraint: {}\n     - Details: {}",
                i + 1,
                format!("No viable candidates for '{}'", iss.stem),
                iss.path.join(" -> "),
                if constraint_str.is_empty() {
                    "<none>".to_string()
                } else {
                    constraint_str
                },
                iss.details,
            );

            // Suggestions
            println!("     - Suggestions:");
            println!(
                "       • Add or publish a matching package for '{}'{}{}.",
                iss.stem,
                iss.constraint
                    .release
                    .as_ref()
                    .map(|r| format!(" (release={})", r))
                    .unwrap_or_default(),
                iss.constraint
                    .branch
                    .as_ref()
                    .map(|b| format!(" (branch={})", b))
                    .unwrap_or_default()
            );
            println!(
                "       • Alternatively, relax the dependency constraint in the requiring package to match available releases."
            );
            if let Some(lock) = get_incorporated_release_cached(image, ctx, &iss.stem)
                .ok()
                .flatten()
            {
                println!(
                    "       • Incorporation lock present for '{}': release={}. Consider updating the incorporation to allow the required release, or align the dependency.",
                    iss.stem, lock
                );
            }
            println!("       • Ensure catalogs are up to date: 'pkg6 refresh'.");
        }
    }

    Ok(())
}

fn advise_recursive(
    image: &Image,
    ctx: &mut AdviceContext,
    fmri: &libips::fmri::Fmri,
    path: &mut Vec<String>,
    depth: usize,
    max_depth: usize,
    seen: &mut HashSet<String>,
    issues: &mut Vec<AdviceIssue>,
) -> Result<()> {
    if max_depth != 0 && depth > max_depth {
        return Ok(());
    }

    // Load manifest of the current FMRI (cached)
    let manifest = get_manifest_cached(image, ctx, fmri)?;

    let mut processed = 0usize;
    let mut constrained = Vec::new();
    let mut unconstrained = Vec::new();
    for dep in manifest.dependencies {
        if dep.dependency_type != "require" && dep.dependency_type != "incorporate" {
            continue;
        }
        let has_fmri = dep.fmri.is_some();
        if !has_fmri {
            continue;
        }
        let c = extract_constraint(&dep.optional);
        if c.release.is_some() || c.branch.is_some() {
            constrained.push((dep, c));
        } else {
            unconstrained.push((dep, c));
        }
    }
    for (dep, constraint) in constrained.into_iter().chain(unconstrained.into_iter()) {
        if ctx.advice_cap != 0 && processed >= ctx.advice_cap {
            debug!(
                "Dependency processing for {} truncated at cap {}",
                fmri.stem(),
                ctx.advice_cap
            );
            break;
        }
        processed += 1;

        let dep_stem = dep.fmri.unwrap().stem().to_string();

        debug!(
            "Checking dependency to '{}' with constraint {:?}",
            dep_stem,
            (&constraint.release, &constraint.branch)
        );

        match find_best_candidate(
            image,
            ctx,
            &dep_stem,
            constraint.release.as_deref(),
            constraint.branch.as_deref(),
        )? {
            Some(next_fmri) => {
                // Continue recursion if not seen and depth allows
                if !seen.contains(&dep_stem) {
                    seen.insert(dep_stem.clone());
                    path.push(dep_stem.clone());
                    advise_recursive(
                        image,
                        ctx,
                        &next_fmri,
                        path,
                        depth + 1,
                        max_depth,
                        seen,
                        issues,
                    )?;
                    path.pop();
                }
            }
            None => {
                let details = build_missing_detail(image, ctx, &dep_stem, &constraint);
                issues.push(AdviceIssue {
                    path: path.clone(),
                    stem: dep_stem.clone(),
                    constraint: constraint.clone(),
                    details,
                });
            }
        }
    }

    Ok(())
}

fn extract_constraint(optional: &[libips::actions::Property]) -> DepConstraint {
    let mut release: Option<String> = None;
    let mut branch: Option<String> = None;
    for p in optional {
        match p.key.as_str() {
            "release" => release = Some(p.value.clone()),
            "branch" => branch = Some(p.value.clone()),
            _ => {}
        }
    }
    DepConstraint { release, branch }
}

fn build_missing_detail(
    image: &Image,
    ctx: &mut AdviceContext,
    stem: &str,
    constraint: &DepConstraint,
) -> String {
    // List available releases/branches for informational purposes
    let mut available: Vec<String> = Vec::new();
    if let Ok(list) = query_catalog_cached_mut(image, ctx, stem) {
        for (pubname, fmri) in list {
            if let Some(ref pfilter) = ctx.publisher {
                if &pubname != pfilter {
                    continue;
                }
            }
            if fmri.stem() != stem {
                continue;
            }
            let ver = fmri.version();
            if ver.is_empty() {
                continue;
            }
            available.push(ver);
        }
    }
    let mut available: Vec<String> = available.into_iter().collect();
    available.sort();
    available.dedup();

    let available_str = if available.is_empty() {
        "<none>".to_string()
    } else {
        available.join(", ")
    };

    let lock = get_incorporated_release_cached(image, ctx, stem)
        .ok()
        .flatten();

    match (&constraint.release, &constraint.branch, lock) {
        (Some(r), Some(b), Some(lr)) => format!(
            "Required release={}, branch={} not found. Incorporation lock release={} may also constrain candidates. Available versions: {}",
            r, b, lr, available_str
        ),
        (Some(r), Some(b), None) => format!(
            "Required release={}, branch={} not found. Available versions: {}",
            r, b, available_str
        ),
        (Some(r), None, Some(lr)) => format!(
            "Required release={} not found. Incorporation lock release={} present. Available versions: {}",
            r, lr, available_str
        ),
        (Some(r), None, None) => format!(
            "Required release={} not found. Available versions: {}",
            r, available_str
        ),
        (None, Some(b), Some(lr)) => format!(
            "Required branch={} not found. Incorporation lock release={} present. Available versions: {}",
            b, lr, available_str
        ),
        (None, Some(b), None) => format!(
            "Required branch={} not found. Available versions: {}",
            b, available_str
        ),
        (None, None, Some(lr)) => format!(
            "No candidates matched. Incorporation lock release={} present. Available versions: {}",
            lr, available_str
        ),
        (None, None, None) => format!(
            "No candidates matched. Available versions: {}",
            available_str
        ),
    }
}

fn find_best_candidate(
    image: &Image,
    ctx: &mut AdviceContext,
    stem: &str,
    req_release: Option<&str>,
    req_branch: Option<&str>,
) -> Result<Option<libips::fmri::Fmri>> {
    let key = (
        stem.to_string(),
        req_release.map(|s| s.to_string()),
        req_branch.map(|s| s.to_string()),
        ctx.publisher.clone(),
    );
    if let Some(cached) = ctx.candidate_cache.get(&key) {
        return Ok(cached.clone());
    }

    let mut candidates: Vec<(String, libips::fmri::Fmri)> = Vec::new();

    // Prefer matching release from incorporation lock, unless explicit req_release provided
    let lock_release = if req_release.is_none() {
        get_incorporated_release_cached(image, ctx, stem)
            .ok()
            .flatten()
    } else {
        None
    };

    for (pubf, pfmri) in query_catalog_cached(image, ctx, stem)? {
        if let Some(ref pfilter) = ctx.publisher {
            if &pubf != pfilter {
                continue;
            }
        }
        if pfmri.stem() != stem {
            continue;
        }
        let ver = pfmri.version();
        if ver.is_empty() {
            continue;
        }

        // Parse version string to extract release and branch heuristically: release,branch-rest
        let rel = version_release(&ver);
        let br = version_branch(&ver);

        if let Some(req_r) = req_release {
            if Some(req_r) != rel.as_deref() {
                continue;
            }
        } else if let Some(lock_r) = lock_release.as_deref() {
            if Some(lock_r) != rel.as_deref() {
                continue;
            }
        }

        if let Some(req_b) = req_branch {
            if Some(req_b) != br.as_deref() {
                continue;
            }
        }

        candidates.push((ver.clone(), pfmri.clone()));
    }

    // Choose the lexicographically max version string (approximate latest)
    candidates.sort_by(|a, b| a.0.cmp(&b.0));
    let res = candidates.pop().map(|x| x.1);
    ctx.candidate_cache.insert(key, res.clone());
    Ok(res)
}

fn version_release(version: &str) -> Option<String> {
    // Format like: "1.35,5.11-2023.0.0.0:TS" => release before comma
    version.split_once(',').map(|(rel, _)| rel.to_string())
}

fn version_branch(version: &str) -> Option<String> {
    // Format like: "1.35,5.11-2023.0.0.0:TS" => branch between "," and "-"
    if let Some((_, rest)) = version.split_once(',') {
        return rest.split_once('-').map(|(b, _)| b.to_string());
    }
    None
}

// ---------- Caching helpers ----------

fn query_catalog_cached(
    image: &Image,
    ctx: &AdviceContext,
    stem: &str,
) -> Result<Vec<(String, libips::fmri::Fmri)>> {
    if let Some(v) = ctx.catalog_cache.get(stem) {
        return Ok(v.clone());
    }
    // We don't have mutable borrow on ctx here; clone and return, caller will populate cache through a mutable wrapper.
    // To keep code simple, provide a small wrapper that fills the cache when needed.
    // We'll implement a separate function that has mutable ctx.
    let mut tmp_ctx = AdviceContext {
        catalog_cache: ctx.catalog_cache.clone(),
        ..Default::default()
    };
    query_catalog_cached_mut(image, &mut tmp_ctx, stem)
}

fn query_catalog_cached_mut(
    image: &Image,
    ctx: &mut AdviceContext,
    stem: &str,
) -> Result<Vec<(String, libips::fmri::Fmri)>> {
    if let Some(v) = ctx.catalog_cache.get(stem) {
        return Ok(v.clone());
    }
    let mut out = Vec::new();
    for p in image
        .query_catalog(Some(stem))
        .map_err(|e| PkgTreeError(format!("Failed to query catalog for {}: {}", stem, e)))?
    {
        out.push((p.publisher, p.fmri));
    }
    ctx.catalog_cache.insert(stem.to_string(), out.clone());
    Ok(out)
}

fn get_manifest_cached(
    image: &Image,
    ctx: &mut AdviceContext,
    fmri: &libips::fmri::Fmri,
) -> Result<libips::actions::Manifest> {
    let key = fmri.to_string();
    if let Some(m) = ctx.manifest_cache.get(&key) {
        return Ok(m.clone());
    }
    let manifest_opt = image.get_manifest_from_catalog(fmri).map_err(|e| {
        PkgTreeError(format!(
            "Failed to load manifest for {}: {}",
            fmri.to_string(),
            e
        ))
    })?;
    let manifest = manifest_opt.unwrap_or_else(|| libips::actions::Manifest::new());
    ctx.manifest_cache.insert(key, manifest.clone());
    Ok(manifest)
}

fn get_incorporated_release_cached(
    image: &Image,
    ctx: &mut AdviceContext,
    stem: &str,
) -> Result<Option<String>> {
    if let Some(v) = ctx.lock_cache.get(stem) {
        return Ok(v.clone());
    }
    let v = image.get_incorporated_release(stem)?;
    ctx.lock_cache.insert(stem.to_string(), v.clone());
    Ok(v)
}

// ---------- Graph mode helpers ----------

fn print_trees(graph: &Graph, roots: &[String], max_depth: usize) {
    // Print a tree for each root
    let mut printed = HashSet::new();
    for r in roots {
        if printed.contains(r) {
            continue;
        }
        printed.insert(r.clone());
        println!("{}", r);
        let mut path = Vec::new();
        let mut seen = HashSet::new();
        print_tree_rec(graph, r, 1, max_depth, &mut path, &mut seen);
        println!("");
    }
}

fn print_tree_rec(
    graph: &Graph,
    node: &str,
    depth: usize,
    max_depth: usize,
    path: &mut Vec<String>,
    _seen: &mut HashSet<String>,
) {
    if max_depth != 0 && depth > max_depth {
        return;
    }
    path.push(node.to_string());

    if let Some(edges) = graph.adj.get(node) {
        for e in edges {
            let last = if path.contains(&e.to) { " (cycle)" } else { "" };
            println!("{}└─ {} [{}]{}", "  ".repeat(depth), e.to, e.dep_type, last);
            if !path.contains(&e.to) {
                print_tree_rec(graph, &e.to, depth + 1, max_depth, path, _seen);
            }
        }
    }

    path.pop();
}

fn detect_cycles(graph: &Graph) -> Vec<Cycle> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = Vec::new();
    let mut cycles = Vec::new();

    for node in graph.stems().cloned().collect::<Vec<_>>() {
        if !visited.contains(&node) {
            dfs_cycles(graph, &node, &mut visited, &mut stack, &mut cycles);
        }
    }

    dedup_cycles(cycles)
}

fn dfs_cycles(
    graph: &Graph,
    node: &str,
    visited: &mut HashSet<String>,
    stack: &mut Vec<String>,
    cycles: &mut Vec<Cycle>,
) {
    visited.insert(node.to_string());
    stack.push(node.to_string());

    if let Some(edges) = graph.adj.get(node) {
        for e in edges {
            let to = &e.to;
            if let Some(pos) = stack.iter().position(|n| n == to) {
                // Found a cycle: stack[pos..] -> to
                let mut cycle_nodes = stack[pos..].to_vec();
                cycle_nodes.push(to.clone());
                let mut cycle_edges = Vec::new();
                for i in pos..stack.len() {
                    let from = &stack[i];
                    let to2 = if i + 1 < stack.len() {
                        &stack[i + 1]
                    } else {
                        to
                    };
                    if let Some(es2) = graph.adj.get(from) {
                        if let Some(edge) = es2.iter().find(|ed| &ed.to == to2) {
                            cycle_edges.push(edge.dep_type.clone());
                        } else {
                            cycle_edges.push("unknown".to_string());
                        }
                    }
                }
                cycles.push(Cycle {
                    nodes: cycle_nodes,
                    edges: cycle_edges,
                });
            } else if !visited.contains(to) {
                dfs_cycles(graph, to, visited, stack, cycles);
            }
        }
    }

    stack.pop();
}

fn dedup_cycles(mut cycles: Vec<Cycle>) -> Vec<Cycle> {
    // Normalize cycles so that smallest node lexicographically is first, and ensure start==end
    for c in cycles.iter_mut() {
        if c.nodes.first() != c.nodes.last() && !c.nodes.is_empty() {
            c.nodes.push(c.nodes.first().unwrap().clone());
        }
        // rotate to minimal node position (excluding the duplicate last element when comparing)
        if c.nodes.len() > 1 {
            let inner = &c.nodes[..c.nodes.len() - 1];
            if let Some((min_idx, _)) = inner.iter().enumerate().min_by_key(|(_, n)| *n) {
                c.nodes.rotate_left(min_idx);
                c.edges.rotate_left(min_idx);
            }
        }
    }
    // Deduplicate by string key
    let mut seen = HashSet::new();
    cycles.retain(|c| {
        let key = c.nodes.join("->");
        if seen.contains(&key) {
            false
        } else {
            seen.insert(key);
            true
        }
    });
    cycles
}

fn print_cycles(cycles: &[Cycle]) {
    if cycles.is_empty() {
        println!("No dependency cycles detected.");
        return;
    }
    println!("Detected {} cycle(s):", cycles.len());
    for (i, c) in cycles.iter().enumerate() {
        println!("  {}. {}", i + 1, c.nodes.join(" -> "));
    }
}

fn print_suggestions(cycles: &[Cycle], graph: &Graph) {
    if cycles.is_empty() {
        return;
    }
    println!("\nSuggestions to break cycles (heuristic):");
    for (i, c) in cycles.iter().enumerate() {
        // Prefer breaking an 'incorporate' edge if present, otherwise any edge
        let mut suggested: Option<(String, String)> = None; // (from, to)
        'outer: for w in c.nodes.windows(2) {
            let from = &w[0];
            let to = &w[1];
            if let Some(es) = graph.adj.get(from) {
                for e in es {
                    if &e.to == to {
                        if e.dep_type == "incorporate" {
                            suggested = Some((from.clone(), to.clone()));
                            break 'outer;
                        }
                        if suggested.is_none() {
                            suggested = Some((from.clone(), to.clone()));
                        }
                    }
                }
            }
        }
        if let Some((from, to)) = suggested {
            println!(
                "  {}. Consider relaxing/removing edge {} -> {} (preferably if it's an incorporation).",
                i + 1,
                from,
                to
            );
        } else {
            println!(
                "  {}. Consider relaxing one edge along the cycle: {}",
                i + 1,
                c.nodes.join(" -> ")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_simple_cycle() {
        let mut g = Graph::default();
        g.add_edge("A".to_string(), "B".to_string(), "require".to_string());
        g.add_edge("B".to_string(), "C".to_string(), "require".to_string());
        g.add_edge("C".to_string(), "A".to_string(), "incorporate".to_string());
        let cycles = detect_cycles(&g);
        assert!(!cycles.is_empty());
    }

    #[test]
    fn version_parsing_helpers() {
        let v = "1.35,5.11-2023.0.0.0:20230723T105730Z";
        assert_eq!(version_release(v).as_deref(), Some("1.35"));
        assert_eq!(version_branch(v).as_deref(), Some("5.11"));
    }
}

// ---------- Dangling dependency scan ----------
fn run_dangling_scan(
    image: &Image,
    publisher: Option<&str>,
    package_filter: Option<&str>,
    format: OutputFormat,
) -> Result<()> {
    // Query full catalog once
    let mut pkgs = image
        .query_catalog(None)
        .map_err(|e| PkgTreeError(format!("Failed to query catalog: {}", e)))?;

    // Build set of available non-obsolete stems AND an index of available (release, branch) pairs per stem,
    // honoring publisher filter
    let mut available_stems: HashSet<String> = HashSet::new();
    let mut available_index: HashMap<String, Vec<(String, Option<String>)>> = HashMap::new();
    for p in &pkgs {
        if let Some(pubf) = publisher {
            if p.publisher != pubf {
                continue;
            }
        }
        if p.obsolete {
            continue;
        }
        let stem = p.fmri.stem().to_string();
        available_stems.insert(stem.clone());
        let ver = p.fmri.version();
        if !ver.is_empty() {
            if let Some(rel) = version_release(&ver) {
                let br = version_branch(&ver);
                available_index.entry(stem).or_default().push((rel, br));
            }
        }
    }

    // Filter the list of requiring packages we'll scan
    if let Some(pubf) = publisher {
        pkgs.retain(|p| p.publisher == pubf);
    }
    pkgs.retain(|p| !p.obsolete);
    if let Some(needle) = package_filter {
        pkgs.retain(|p| p.fmri.stem().contains(needle) || p.fmri.to_string().contains(needle));
    }

    // Map of requiring package fmri string -> Vec<missing_stems>
    let mut dangling: HashMap<String, Vec<String>> = HashMap::new();

    for p in &pkgs {
        let fmri = &p.fmri;
        match image.get_manifest_from_catalog(fmri) {
            Ok(Some(man)) => {
                let mut missing_for_pkg: Vec<String> = Vec::new();
                for dep in man.dependencies {
                    if dep.dependency_type != "require" && dep.dependency_type != "incorporate" {
                        continue;
                    }
                    let Some(df) = dep.fmri else {
                        continue;
                    };
                    let stem = df.stem().to_string();

                    // Extract version/branch constraints if any (from optional properties)
                    let mut c = extract_constraint(&dep.optional);
                    // Also merge constraints from the dependency FMRI's version string if not provided in optional
                    let df_ver_str = df.version();
                    if !df_ver_str.is_empty() {
                        if c.release.is_none() {
                            c.release = version_release(&df_ver_str);
                        }
                        if c.branch.is_none() {
                            c.branch = version_branch(&df_ver_str);
                        }
                    }

                    // Helper to check availability against constraints
                    let satisfies = |stem: &str, rel: Option<&str>, br: Option<&str>| -> bool {
                        if let Some(list) = available_index.get(stem) {
                            if let (Some(rreq), Some(breq)) = (rel, br) {
                                return list
                                    .iter()
                                    .any(|(r, b)| r == rreq && b.as_deref() == Some(breq));
                            } else if let Some(rreq) = rel {
                                return list.iter().any(|(r, _)| r == rreq);
                            } else if let Some(breq) = br {
                                return list.iter().any(|(_, b)| b.as_deref() == Some(breq));
                            } else {
                                return true; // no constraint: stem existing already confirmed elsewhere
                            }
                        }
                        false
                    };

                    let mut mark_missing: Option<String> = None;
                    if !available_stems.contains(&stem) {
                        mark_missing = Some(stem.clone());
                    } else if c.release.is_some() || c.branch.is_some() {
                        if !satisfies(&stem, c.release.as_deref(), c.branch.as_deref()) {
                            // Include constraint context in output for maintainers
                            let mut ctx = String::new();
                            if let Some(r) = &c.release {
                                ctx.push_str(&format!("release={} ", r));
                            }
                            if let Some(b) = &c.branch {
                                ctx.push_str(&format!("branch={}", b));
                            }
                            let ctx = ctx.trim().to_string();
                            if ctx.is_empty() {
                                mark_missing = Some(stem.clone());
                            } else {
                                mark_missing = Some(format!("{} [required {}]", stem, ctx));
                            }
                        }
                    }

                    if let Some(m) = mark_missing {
                        missing_for_pkg.push(m);
                    }
                }
                if !missing_for_pkg.is_empty() {
                    missing_for_pkg.sort();
                    missing_for_pkg.dedup();
                    dangling.insert(fmri.to_string(), missing_for_pkg);
                }
            }
            Ok(None) => {
                warn!(pkg=%fmri.to_string(), "Manifest not found in catalog while scanning dangling deps");
            }
            Err(e) => {
                warn!(pkg=%fmri.to_string(), error=%format!("{}", e), "Failed to read manifest while scanning dangling deps");
            }
        }
    }

    // Output
    match format {
        OutputFormat::Tree => {
            if dangling.is_empty() {
                println!("No dangling dependencies detected.");
            } else {
                println!(
                    "Found {} package(s) with dangling dependencies:",
                    dangling.len()
                );
                let mut keys: Vec<String> = dangling.keys().cloned().collect();
                keys.sort();
                for k in keys {
                    println!("- {}:", k);
                    if let Some(list) = dangling.get(&k) {
                        for m in list {
                            println!("   • {}", m);
                        }
                    }
                }
            }
        }
        OutputFormat::Json => {
            use serde::Serialize;
            #[derive(Serialize)]
            struct DanglingJson {
                package_fmri: String,
                missing_stems: Vec<String>,
            }
            let mut out: Vec<DanglingJson> = Vec::new();
            for (pkg, miss) in dangling.into_iter() {
                out.push(DanglingJson {
                    package_fmri: pkg,
                    missing_stems: miss,
                });
            }
            out.sort_by(|a, b| a.package_fmri.cmp(&b.package_fmri));
            println!("{}", serde_json::to_string_pretty(&out).into_diagnostic()?);
        }
    }

    Ok(())
}

// ---------- Targeted analysis: parse pkg6 solver error text ----------
fn analyze_solver_error(image: &Image, publisher: Option<&str>, err_path: &PathBuf) -> Result<()> {
    let text = std::fs::read_to_string(err_path).map_err(|e| {
        PkgTreeError(format!(
            "Failed to read solver error file {:?}: {}",
            err_path, e
        ))
    })?;

    // Build a stack based on indentation before the tree bullet "└─".
    let mut stack: Vec<String> = Vec::new();
    let mut captured_path: Vec<String> = Vec::new();
    let mut failing_leaf: Option<String> = None;

    for line in text.lines() {
        if let Some(idx) = line.find("└") {
            // Count spaces before the bullet to infer depth (~3 spaces per level in our output)
            let indent = line[..idx].chars().filter(|c| *c == ' ').count();
            let level = indent / 3; // heuristic

            // Extract node text after "└─ "
            let bullet = "└─ ";
            let start = match line.find(bullet) {
                Some(p) => p + bullet.len(),
                None => continue,
            };
            let mut node_full = line[start..].trim().to_string();
            // Remove trailing diagnostic phrases for leaf line
            if let Some(pos) = node_full.find("for which no candidates were found") {
                node_full = node_full[..pos].trim().trim_end_matches(',').to_string();
            }

            if level >= stack.len() {
                stack.push(node_full.clone());
            } else {
                stack.truncate(level);
                stack.push(node_full.clone());
            }

            if line.contains("for which no candidates were found") {
                failing_leaf = Some(node_full.clone());
                captured_path = stack.clone();
                break;
            }
        }
    }

    if failing_leaf.is_none() {
        println!(
            "Could not find a 'for which no candidates were found' leaf in the provided solver error file."
        );
        return Ok(());
    }

    let leaf = failing_leaf.unwrap();

    // Extract stem and constraints from the leaf node text.
    let (stem, constraint) = parse_leaf_node(&leaf);

    // Prepare context and produce detailed suggestion
    let mut ctx = AdviceContext::new(publisher.map(|s| s.to_string()), 0);
    let details = build_missing_detail(image, &mut ctx, &stem, &constraint);

    // Build a readable path using stems
    let path_stems: Vec<String> = captured_path
        .into_iter()
        .map(|n| stem_from_node(&n))
        .collect();

    println!("Found 1 installability issue (from solver error):");
    let constraint_str = format!(
        "{}{}",
        constraint
            .release
            .as_ref()
            .map(|r| format!("release={} ", r))
            .unwrap_or_default(),
        constraint
            .branch
            .as_ref()
            .map(|b| format!("branch={}", b))
            .unwrap_or_default(),
    )
    .trim()
    .to_string();
    println!(
        "  1. No viable candidates for '{}'\n     - Path: {}\n     - Constraint: {}\n     - Details: {}",
        stem,
        path_stems.join(" -> "),
        if constraint_str.is_empty() {
            "<none>".to_string()
        } else {
            constraint_str
        },
        details,
    );
    println!("     - Suggestions:");
    println!(
        "       • Add or publish a matching package for '{}'{}{}.",
        stem,
        constraint
            .release
            .as_ref()
            .map(|r| format!(" (release={})", r))
            .unwrap_or_default(),
        constraint
            .branch
            .as_ref()
            .map(|b| format!(" (branch={})", b))
            .unwrap_or_default()
    );
    println!(
        "       • Alternatively, relax the dependency constraint in the requiring package to match available releases."
    );
    if let Some(lock) = get_incorporated_release_cached(image, &mut ctx, &stem)
        .ok()
        .flatten()
    {
        println!(
            "       • Incorporation lock present for '{}': release={}. Consider updating the incorporation to allow the required release, or align the dependency.",
            stem, lock
        );
    }
    println!("       • Ensure catalogs are up to date: 'pkg6 refresh'.");

    Ok(())
}

fn stem_from_node(node: &str) -> String {
    // Node may be like: "pkg://...@ver would require" or "archiver/gnu-tar branch=5.11, which ..." or just a stem
    let first = node.split_whitespace().next().unwrap_or("");
    if first.starts_with("pkg://") {
        if let Ok(fmri) = libips::fmri::Fmri::parse(first) {
            return fmri.stem().to_string();
        }
    }
    // If it contains '@' (FMRI without scheme), parse via Fmri::parse
    if first.contains('@') {
        if let Ok(fmri) = libips::fmri::Fmri::parse(first) {
            return fmri.stem().to_string();
        }
    }
    // Otherwise assume it's a stem token
    first.trim_end_matches(',').to_string()
}

fn parse_leaf_node(node: &str) -> (String, DepConstraint) {
    let core = node
        .split("for which")
        .next()
        .unwrap_or(node)
        .trim()
        .trim_end_matches(',')
        .to_string();
    let mut release: Option<String> = None;
    let mut branch: Option<String> = None;

    // Find release=
    if let Some(p) = core.find("release=") {
        let rest = &core[p + "release=".len()..];
        let end = rest
            .find(|c: char| c == ' ' || c == ',')
            .unwrap_or(rest.len());
        release = Some(rest[..end].to_string());
    }
    // Find branch=
    if let Some(p) = core.find("branch=") {
        let rest = &core[p + "branch=".len()..];
        let end = rest
            .find(|c: char| c == ' ' || c == ',')
            .unwrap_or(rest.len());
        branch = Some(rest[..end].to_string());
    }

    // Stem is first token
    let stem = stem_from_node(&core);
    (stem, DepConstraint { release, branch })
}
