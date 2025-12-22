//  This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
//  If a copy of the MPL was not distributed with this file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::actions::{Dependency as ManifestDependency, Manifest};
use crate::fmri::Fmri;
use crate::repository::ReadableRepository;
use miette::Diagnostic;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error as StdError;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, warn};

pub type Result<T> = std::result::Result<T, DependError>;

#[derive(Error, Debug, Diagnostic)]
#[error("Dependency generation error: {message}")]
#[diagnostic(code(ips::depend_error), help("Review inputs and file types"))]
pub struct DependError {
    pub message: String,
    #[source]
    pub source: Option<Box<dyn StdError + Send + Sync>>, // keep library crate simple
}

impl DependError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }
    fn with_source(message: impl Into<String>, source: Box<dyn StdError + Send + Sync>) -> Self {
        Self {
            message: message.into(),
            source: Some(source),
        }
    }
}

/// Options controlling dependency generation
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GenerateOptions {
    /// Colon-separated runpath override to be applied to all actions (manifest-level).
    /// If it contains the PD_DEFAULT_RUNPATH token, default runpaths will be inserted at that position.
    pub runpath: Option<String>,
    /// Regex patterns to bypass dependency generation (skip matching actions entirely).
    pub bypass_patterns: Vec<String>,
    /// Proto directory base; used to locate local files when only manifest relative paths are known.
    pub proto_dir: Option<PathBuf>,
}

/// Token name used to splice in the analyzer default runpaths.
pub const PD_DEFAULT_RUNPATH: &str = "PD_DEFAULT_RUNPATH";

/// Intermediate file-level dependency representation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileDepKind {
    Elf {
        /// The base filename needed (from DT_NEEDED)
        base_name: String,
        /// Directories searched to find the base_name
        run_paths: Vec<String>,
        /// Installed path of the object declaring the dependency
        installed_path: String,
    },
    Script {
        /// The base filename of the interpreter (e.g., python3, sh)
        base_name: String,
        /// Directories searched to find the interpreter
        run_paths: Vec<String>,
        /// Installed path of the script declaring the dependency
        installed_path: String,
    },
    Python {
        /// Candidate module file basenames (e.g., foo.py, foo.so)
        base_names: Vec<String>,
        /// Directories searched for Python modules for the selected version
        run_paths: Vec<String>,
        /// Installed path of the script/module declaring the dependency
        installed_path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDep {
    pub kind: FileDepKind,
}

/// Convert manifest file actions into FileDep entries (ELF only for now).
pub fn generate_file_dependencies_from_manifest(
    manifest: &Manifest,
    opts: &GenerateOptions,
) -> Result<Vec<FileDep>> {
    let mut out = Vec::new();
    let bypass = compile_bypass(&opts.bypass_patterns)?;

    for f in &manifest.files {
        // Determine installed path (manifests typically do not start with '/').
        let installed_path = if f.path.starts_with('/') {
            f.path.clone()
        } else {
            format!("/{}", f.path)
        };

        if should_bypass(&installed_path, &bypass) {
            debug!(
                "bypassing dependency generation for {} per patterns",
                installed_path
            );
            continue;
        }

        // Try to find the local file to analyze: prefer explicit original-path property; if it's relative, resolve against proto_dir.
        let local_path = match f.get_original_path() {
            Some(op) => {
                let p = PathBuf::from(&op);
                if p.is_absolute() {
                    p
                } else if let Some(base) = &opts.proto_dir {
                    let cand = base.join(op.trim_start_matches('/'));
                    if cand.exists() {
                        cand
                    } else {
                        // Fallback to proto_dir + installed_path
                        base.join(installed_path.trim_start_matches('/'))
                    }
                } else {
                    // Relative without proto_dir: try as-is (may be relative to CWD)
                    PathBuf::from(op)
                }
            }
            None => match &opts.proto_dir {
                Some(base) => base.join(installed_path.trim_start_matches('/')),
                None => continue, // no local file to analyze; skip
            },
        };

        // Read local bytes once
        if let Ok(bytes) = fs::read(&local_path) {
            // ELF check
            if bytes.len() >= 4 && &bytes[0..4] == b"\x7FELF" {
                let mut deps = process_elf(&bytes, &installed_path, opts);
                out.append(&mut deps);
                continue;
            }

            // Script shebang check
            if let Some(interp) = parse_shebang(&bytes) {
                // Optional: ensure executable; if mode missing, assume executable
                let exec_ok = is_executable_mode(&f.mode);
                if !exec_ok {
                    // Not executable; skip script dependency
                    continue;
                }
                // Normalize /bin -> /usr/bin
                let interp_path = normalize_bin_path(&interp);
                if !interp_path.starts_with('/') {
                    warn!(
                        "Script shebang for {} specifies non-absolute interpreter: {}",
                        installed_path, interp_path
                    );
                } else {
                    // Derive dir and base name
                    let (dir, base) = split_dir_base(&interp_path);
                    if let Some(dir) = dir {
                        out.push(FileDep {
                            kind: FileDepKind::Script {
                                base_name: base.to_string(),
                                run_paths: vec![dir.to_string()],
                                installed_path: installed_path.clone(),
                            },
                        });
                        // If Python interpreter, perform Python analysis
                        if interp_path.contains("python") {
                            if let Some((maj, min)) =
                                infer_python_version_from_paths(&installed_path, Some(&interp_path))
                            {
                                let mut pydeps =
                                    process_python(&bytes, &installed_path, (maj, min), opts);
                                out.append(&mut pydeps);
                            }
                        }
                    }
                }
            } else {
                // If no shebang or non-exec, but file is under usr/lib/pythonX.Y/, analyze as module
                if let Some((maj, min)) = infer_python_version_from_paths(&installed_path, None) {
                    let mut pydeps = process_python(&bytes, &installed_path, (maj, min), opts);
                    out.append(&mut pydeps);
                }
            }

            // SMF manifest detection: extract exec paths
            if looks_like_smf_manifest(&bytes) {
                for exec_path in extract_smf_execs(&bytes) {
                    if exec_path.starts_with('/') {
                        let (dir, base) = split_dir_base(&exec_path);
                        if let Some(dir) = dir {
                            out.push(FileDep {
                                kind: FileDepKind::Script {
                                    base_name: base.to_string(),
                                    run_paths: vec![dir.to_string()],
                                    installed_path: installed_path.clone(),
                                },
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(out)
}

/// Insert default runpaths into provided runpaths based on PD_DEFAULT_RUNPATH token
fn insert_default_runpath(
    defaults: &[String],
    provided: &[String],
) -> std::result::Result<Vec<String>, DependError> {
    let mut out = Vec::new();
    let mut token_count = 0;
    for p in provided {
        if p == PD_DEFAULT_RUNPATH {
            token_count += 1;
            if token_count > 1 {
                return Err(DependError::new(
                    "Multiple PD_DEFAULT_RUNPATH tokens in runpath override",
                ));
            }
            out.extend_from_slice(defaults);
        } else {
            out.push(p.clone());
        }
    }
    if token_count == 0 {
        // Override replaces defaults
        Ok(provided.to_vec())
    } else {
        Ok(out)
    }
}

fn compile_bypass(patterns: &[String]) -> Result<Vec<Regex>> {
    let mut out = Vec::new();
    for p in patterns {
        out.push(Regex::new(p).map_err(|e| {
            DependError::with_source(format!("invalid bypass pattern: {}", p), Box::new(e))
        })?);
    }
    Ok(out)
}

fn should_bypass(path: &str, patterns: &[Regex]) -> bool {
    patterns.iter().any(|re| re.is_match(path))
}

fn process_elf(bytes: &[u8], installed_path: &str, opts: &GenerateOptions) -> Vec<FileDep> {
    let mut out = Vec::new();
    match goblin::elf::Elf::parse(bytes) {
        Ok(elf) => {
            // DT_NEEDED entries
            let mut needed: Vec<String> = elf.libraries.iter().map(|s| s.to_string()).collect();
            if needed.is_empty() {
                return out;
            }

            // Default runpaths
            let mut defaults: Vec<String> = vec!["/lib".into(), "/usr/lib".into()];
            // crude bitness check: presence of 64-bit elf class
            if elf.is_64 {
                defaults.push("/lib/64".into());
                defaults.push("/usr/lib/64".into());
            }

            // DT_RUNPATH
            let mut runpaths: Vec<String> = Vec::new();
            if !elf.runpaths.is_empty() {
                for rp in &elf.runpaths {
                    for seg in rp.split(':') {
                        if !seg.is_empty() {
                            runpaths.push(seg.to_string());
                        }
                    }
                }
            }

            // Merge with defaults using PD_DEFAULT_RUNPATH semantics if caller provided runpath override
            let effective = if let Some(ref rp) = opts.runpath {
                let provided: Vec<String> = rp.split(':').map(|s| s.to_string()).collect();
                match insert_default_runpath(&defaults, &provided) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("{}", e.message);
                        provided
                    }
                }
            } else {
                // If no override, prefer DT_RUNPATH if present else defaults
                if runpaths.is_empty() {
                    defaults.clone()
                } else {
                    runpaths.clone()
                }
            };

            // Expand $ORIGIN
            let origin = Path::new(installed_path)
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "/".to_string());
            let expanded: Vec<String> = effective
                .into_iter()
                .map(|p| p.replace("$ORIGIN", &origin))
                .collect();

            // Emit FileDep for each DT_NEEDED base name
            for bn in needed.drain(..) {
                out.push(FileDep {
                    kind: FileDepKind::Elf {
                        base_name: bn,
                        run_paths: expanded.clone(),
                        installed_path: installed_path.to_string(),
                    },
                });
            }
        }
        Err(err) => warn!("ELF parse error for {}: {}", installed_path, err),
    }
    out
}

/// Resolve file-level dependencies into manifest Dependency actions by consulting a repository.
pub fn resolve_dependencies<R: ReadableRepository>(
    repo: &R,
    publisher: Option<&str>,
    file_deps: &[FileDep],
) -> Result<Vec<ManifestDependency>> {
    // Build a mapping from path -> providers (FMRIs)
    let path_map = build_path_provider_map(repo, publisher)?;

    let mut deps: Vec<ManifestDependency> = Vec::new();

    for fd in file_deps {
        match &fd.kind {
            FileDepKind::Elf {
                base_name,
                run_paths,
                ..
            } => {
                let mut providers: Vec<Fmri> = Vec::new();
                for dir in run_paths {
                    let full = normalize_join(dir, base_name);
                    if let Some(list) = path_map.get(&full) {
                        for f in list {
                            if !providers.contains(f) {
                                providers.push(f.clone());
                            }
                        }
                    }
                }
                if providers.len() == 1 {
                    let fmri = providers.remove(0);
                    deps.push(ManifestDependency {
                        fmri: Some(fmri),
                        dependency_type: "require".to_string(),
                        predicate: None,
                        root_image: String::new(),
                        optional: Vec::new(),
                        facets: HashMap::new(),
                    });
                } else if providers.len() > 1 {
                    // Our model lacks a group for require-any; emit one per FMRI
                    for fmri in providers.into_iter() {
                        deps.push(ManifestDependency {
                            fmri: Some(fmri),
                            dependency_type: "require-any".to_string(),
                            predicate: None,
                            root_image: String::new(),
                            optional: Vec::new(),
                            facets: HashMap::new(),
                        });
                    }
                } else {
                    // unresolved -> skip for now; future: emit analysis warnings
                }
            }
            FileDepKind::Script {
                base_name,
                run_paths,
                ..
            } => {
                let mut providers: Vec<Fmri> = Vec::new();
                for dir in run_paths {
                    let full = normalize_join(dir, base_name);
                    if let Some(list) = path_map.get(&full) {
                        for f in list {
                            if !providers.contains(f) {
                                providers.push(f.clone());
                            }
                        }
                    }
                }
                if providers.len() == 1 {
                    let fmri = providers.remove(0);
                    deps.push(ManifestDependency {
                        fmri: Some(fmri),
                        dependency_type: "require".to_string(),
                        predicate: None,
                        root_image: String::new(),
                        optional: Vec::new(),
                        facets: HashMap::new(),
                    });
                } else if providers.len() > 1 {
                    for fmri in providers.into_iter() {
                        deps.push(ManifestDependency {
                            fmri: Some(fmri),
                            dependency_type: "require-any".to_string(),
                            predicate: None,
                            root_image: String::new(),
                            optional: Vec::new(),
                            facets: HashMap::new(),
                        });
                    }
                } else {
                }
            }
            FileDepKind::Python {
                base_names,
                run_paths,
                ..
            } => {
                let mut providers: Vec<Fmri> = Vec::new();
                for dir in run_paths {
                    for base in base_names {
                        let full = normalize_join(dir, base);
                        if let Some(list) = path_map.get(&full) {
                            for f in list {
                                if !providers.contains(f) {
                                    providers.push(f.clone());
                                }
                            }
                        }
                    }
                }
                if providers.len() == 1 {
                    let fmri = providers.remove(0);
                    deps.push(ManifestDependency {
                        fmri: Some(fmri),
                        dependency_type: "require".to_string(),
                        predicate: None,
                        root_image: String::new(),
                        optional: Vec::new(),
                        facets: HashMap::new(),
                    });
                } else if providers.len() > 1 {
                    for fmri in providers.into_iter() {
                        deps.push(ManifestDependency {
                            fmri: Some(fmri),
                            dependency_type: "require-any".to_string(),
                            predicate: None,
                            root_image: String::new(),
                            optional: Vec::new(),
                            facets: HashMap::new(),
                        });
                    }
                } else {
                }
            }
        }
    }

    Ok(deps)
}

fn normalize_join(dir: &str, base: &str) -> String {
    if dir.ends_with('/') {
        format!("{}{}", dir.trim_end_matches('/'), format!("/{}", base))
    } else {
        format!("{}/{}", dir, base)
    }
}

fn build_path_provider_map<R: ReadableRepository>(
    repo: &R,
    publisher: Option<&str>,
) -> Result<HashMap<String, Vec<Fmri>>> {
    // Ask repo to show contents for all packages (files only)
    let contents = repo
        .show_contents(publisher, None, Some(&["file".to_string()]))
        .map_err(|e| DependError::with_source("Repository show_contents failed", Box::new(e)))?;

    let mut map: HashMap<String, Vec<Fmri>> = HashMap::new();
    for pc in contents {
        let fmri = match pc.package_id.parse::<Fmri>() {
            Ok(f) => f,
            Err(e) => {
                warn!(
                    "Skipping package with invalid FMRI {}: {}",
                    pc.package_id, e
                );
                continue;
            }
        };
        if let Some(files) = pc.files {
            for p in files {
                // Ensure leading slash
                let key = if p.starts_with('/') {
                    p
                } else {
                    format!("/{}", p)
                };
                map.entry(key).or_default().push(fmri.clone());
            }
        }
    }
    Ok(map)
}

// --- Helpers for script processing ---
fn parse_shebang(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 || bytes[0] != b'#' || bytes[1] != b'!' {
        return None;
    }
    // Extract first line after #!
    let mut end = bytes.len();
    for (i, b) in bytes.iter().enumerate().skip(2) {
        if *b == b'\n' || *b == b'\r' {
            end = i;
            break;
        }
    }
    let line = &bytes[2..end];
    let text = String::from_utf8_lossy(line);
    let s = text.trim();
    if s.is_empty() {
        return None;
    }
    // First token is interpreter path
    let mut parts = s.split_whitespace();
    parts.next().map(|p| p.to_string())
}

fn is_executable_mode(mode_str: &str) -> bool {
    // If mode is empty or unparsable, assume executable to avoid missing deps
    let ms = mode_str.trim();
    if ms.is_empty() {
        return true;
    }
    // Accept strings like "0755" or "755"
    match u32::from_str_radix(ms.trim_start_matches('0'), 8) {
        Ok(bits) => bits & 0o111 != 0,
        Err(_) => true,
    }
}

fn normalize_bin_path(path: &str) -> String {
    if path.starts_with("/bin/") {
        path.replacen("/bin/", "/usr/bin/", 1)
    } else {
        path.to_string()
    }
}

fn split_dir_base(path: &str) -> (Option<&str>, &str) {
    if let Some(idx) = path.rfind('/') {
        if idx == 0 {
            return (Some("/"), &path[1..]);
        }
        (Some(&path[..idx]), &path[idx + 1..])
    } else {
        (None, path)
    }
}

fn looks_like_smf_manifest(bytes: &[u8]) -> bool {
    // Very lightweight detection: SMF manifests are XML files with a <service_bundle ...> root
    // We do a lossy UTF-8 conversion and look for the tag to avoid a full XML parser.
    let text = String::from_utf8_lossy(bytes);
    text.contains("<service_bundle")
}

// --- Python helpers ---
fn infer_python_version_from_paths(
    installed_path: &str,
    shebang_path: Option<&str>,
) -> Option<(u8, u8)> {
    // Prefer version implied by installed path under /usr/lib/pythonX.Y
    if let Ok(re) = Regex::new(r"^/usr/lib/python(\d+)\.(\d+)(/|$)") {
        if let Some(c) = re.captures(installed_path) {
            if let (Some(ma), Some(mi)) = (c.get(1), c.get(2)) {
                if let (Ok(maj), Ok(min)) = (ma.as_str().parse::<u8>(), mi.as_str().parse::<u8>()) {
                    return Some((maj, min));
                }
            }
        }
    }
    // Else, try to infer from shebang interpreter path (e.g., /usr/bin/python3.11)
    if let Some(sb) = shebang_path {
        if let Ok(re) = Regex::new(r"python(\d+)\.(\d+)") {
            if let Some(c) = re.captures(sb) {
                if let (Some(ma), Some(mi)) = (c.get(1), c.get(2)) {
                    if let (Ok(maj), Ok(min)) =
                        (ma.as_str().parse::<u8>(), mi.as_str().parse::<u8>())
                    {
                        return Some((maj, min));
                    }
                }
            }
        }
    }
    None
}

fn compute_python_runpaths(version: (u8, u8), opts: &GenerateOptions) -> Vec<String> {
    let (maj, min) = version;
    let base = format!("/usr/lib/python{}.{}", maj, min);
    let defaults = vec![
        base.clone(),
        format!("{}/vendor-packages", base),
        format!("{}/site-packages", base),
        format!("{}/lib-dynload", base),
    ];
    if let Some(ref rp) = opts.runpath {
        let provided: Vec<String> = rp.split(':').map(|s| s.to_string()).collect();
        insert_default_runpath(&defaults, &provided).unwrap_or_else(|_| provided)
    } else {
        defaults
    }
}

fn collect_python_imports(src: &str) -> Vec<String> {
    let mut mods = Vec::new();
    // Regex for 'import x[.y][, z]' - handle only first module per line for simplicity
    if let Ok(re_imp) = Regex::new(r"(?m)^\s*import\s+([A-Za-z_][A-Za-z0-9_\.]*)") {
        for cap in re_imp.captures_iter(src) {
            if let Some(m) = cap.get(1) {
                let name = m.as_str().split('.').next().unwrap_or("").to_string();
                if !name.is_empty() && !mods.contains(&name) {
                    mods.push(name);
                }
            }
        }
    }
    // Regex for 'from x.y import ...'
    if let Ok(re_from) = Regex::new(r"(?m)^\s*from\s+([A-Za-z_][A-Za-z0-9_\.]*)\s+import\s+") {
        for cap in re_from.captures_iter(src) {
            if let Some(m) = cap.get(1) {
                let name = m.as_str().split('.').next().unwrap_or("").to_string();
                if !name.is_empty() && !mods.contains(&name) {
                    mods.push(name);
                }
            }
        }
    }
    mods
}

fn process_python(
    bytes: &[u8],
    installed_path: &str,
    version: (u8, u8),
    opts: &GenerateOptions,
) -> Vec<FileDep> {
    let text = String::from_utf8_lossy(bytes);
    let imports = collect_python_imports(&text);
    if imports.is_empty() {
        return Vec::new();
    }
    // Base names to search: module.py and module.so
    let mut base_names: Vec<String> = Vec::new();
    for m in imports {
        let py = format!("{}.py", m);
        let so = format!("{}.so", m);
        if !base_names.contains(&py) {
            base_names.push(py);
        }
        if !base_names.contains(&so) {
            base_names.push(so);
        }
    }
    let run_paths = compute_python_runpaths(version, opts);
    vec![FileDep {
        kind: FileDepKind::Python {
            base_names,
            run_paths,
            installed_path: installed_path.to_string(),
        },
    }]
}

// --- SMF helpers ---
fn extract_smf_execs(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    // Match exec="..." or exec='...'
    if let Ok(re) = Regex::new(r#"exec\s*=\s*\"([^\"]+)\"|exec\s*=\s*'([^']+)'"#) {
        for cap in re.captures_iter(&text) {
            let m = cap.get(1).or_else(|| cap.get(2));
            if let Some(v) = m {
                let val = v.as_str().to_string();
                if !out.contains(&val) {
                    out.push(val);
                }
            }
        }
    }
    out
}
