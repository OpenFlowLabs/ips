use std::fs::{self, File as FsFile};
use std::io::{self, Write};
use std::os::unix::fs as unix_fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use miette::Diagnostic;
use thiserror::Error;
use tracing::info;

use crate::actions::{Link as LinkAction, Manifest};
use crate::actions::{Dir as DirAction, File as FileAction};

#[derive(Error, Debug, Diagnostic)]
pub enum InstallerError {
    #[error("I/O error while operating on {path}")]
    #[diagnostic(code(ips::installer_error::io))]
    Io {
        #[source]
        source: io::Error,
        path: PathBuf,
    },

    #[error("Absolute paths are forbidden in actions: {path}")]
    #[diagnostic(code(ips::installer_error::absolute_path_forbidden), help("Provide paths relative to the image root"))]
    AbsolutePathForbidden { path: String },

    #[error("Path escapes image root via traversal: {rel}")]
    #[diagnostic(code(ips::installer_error::path_outside_image), help("Remove '..' components that escape the image root"))]
    PathTraversalOutsideImage { rel: String },

    #[error("Unsupported or not yet implemented action: {action} ({reason})")]
    #[diagnostic(code(ips::installer_error::unsupported_action))]
    UnsupportedAction { action: &'static str, reason: String },
}

fn parse_mode(mode: &str, default: u32) -> u32 {
    if mode.is_empty() || mode.eq("0") {
        return default;
    }
    // Accept strings like "0755" or "755"
    let trimmed = mode.trim_start_matches('0');
    u32::from_str_radix(if trimmed.is_empty() { "0" } else { trimmed }, 8).unwrap_or(default)
}

/// Join a manifest-provided path (must be relative) under image_root.
/// - Rejects absolute paths
/// - Rejects traversal that would escape the image root
pub fn safe_join(image_root: &Path, rel: &str) -> Result<PathBuf, InstallerError> {
    if rel.is_empty() {
        return Ok(image_root.to_path_buf());
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(InstallerError::AbsolutePathForbidden {
            path: rel.to_string(),
        });
    }

    let mut stack: Vec<PathBuf> = Vec::new();
    for c in rel_path.components() {
        match c {
            Component::CurDir => {}
            Component::Normal(seg) => stack.push(PathBuf::from(seg)),
            Component::ParentDir => {
                if stack.pop().is_none() {
                    return Err(InstallerError::PathTraversalOutsideImage {
                        rel: rel.to_string(),
                    });
                }
            }
            // Prefixes shouldn't appear on Unix; treat conservatively
            Component::Prefix(_) | Component::RootDir => {
                return Err(InstallerError::AbsolutePathForbidden {
                    path: rel.to_string(),
                })
            }
        }
    }

    let mut out = PathBuf::from(image_root);
    for seg in stack {
        out.push(seg);
    }
    Ok(out)
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub enum ActionOrder {
    Dir = 0,
    File = 1,
    Link = 2,
    Other = 3,
}

#[derive(Clone)]
pub struct ApplyOptions {
    pub dry_run: bool,
    /// Optional progress callback. If set, library will emit coarse-grained progress events.
    pub progress: Option<ProgressCallback>,
    /// Emit numeric progress every N items per phase. 0 disables periodic progress.
    pub progress_interval: usize,
}

impl std::fmt::Debug for ApplyOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApplyOptions")
            .field("dry_run", &self.dry_run)
            .field("progress", &self.progress.as_ref().map(|_| "Some(callback)"))
            .field("progress_interval", &self.progress_interval)
            .finish()
    }
}

impl Default for ApplyOptions {
    fn default() -> Self {
        Self { dry_run: false, progress: None, progress_interval: 0 }
    }
}

/// Progress event emitted by apply_manifest when a callback is provided.
#[derive(Debug, Clone, Copy)]
pub enum ProgressEvent {
    StartingPhase { phase: &'static str, total: usize },
    Progress { phase: &'static str, current: usize, total: usize },
    FinishedPhase { phase: &'static str, total: usize },
}

pub type ProgressCallback = Arc<dyn Fn(ProgressEvent) + Send + Sync + 'static>;

/// Apply a manifest to the filesystem rooted at image_root.
/// This function enforces ordering: directories, then files, then links, then others (no-ops for now).
pub fn apply_manifest(image_root: &Path, manifest: &Manifest, opts: &ApplyOptions) -> Result<(), InstallerError> {
    let emit = |evt: ProgressEvent, cb: &Option<ProgressCallback>| {
        if let Some(cb) = cb.as_ref() { (cb)(evt); }
    };

    // Directories first
    let total_dirs = manifest.directories.len();
    if total_dirs > 0 { emit(ProgressEvent::StartingPhase { phase: "directories", total: total_dirs }, &opts.progress); }
    let mut i = 0usize;
    for d in &manifest.directories {
        apply_dir(image_root, d, opts)?;
        i += 1;
        if opts.progress_interval > 0 && (i % opts.progress_interval == 0 || i == total_dirs) {
            emit(ProgressEvent::Progress { phase: "directories", current: i, total: total_dirs }, &opts.progress);
        }
    }
    if total_dirs > 0 { emit(ProgressEvent::FinishedPhase { phase: "directories", total: total_dirs }, &opts.progress); }

    // Files next
    let total_files = manifest.files.len();
    if total_files > 0 { emit(ProgressEvent::StartingPhase { phase: "files", total: total_files }, &opts.progress); }
    i = 0;
    for f_action in &manifest.files {
        apply_file(image_root, f_action, opts)?;
        i += 1;
        if opts.progress_interval > 0 && (i % opts.progress_interval == 0 || i == total_files) {
            emit(ProgressEvent::Progress { phase: "files", current: i, total: total_files }, &opts.progress);
        }
    }
    if total_files > 0 { emit(ProgressEvent::FinishedPhase { phase: "files", total: total_files }, &opts.progress); }

    // Links
    let total_links = manifest.links.len();
    if total_links > 0 { emit(ProgressEvent::StartingPhase { phase: "links", total: total_links }, &opts.progress); }
    i = 0;
    for l in &manifest.links {
        apply_link(image_root, l, opts)?;
        i += 1;
        if opts.progress_interval > 0 && (i % opts.progress_interval == 0 || i == total_links) {
            emit(ProgressEvent::Progress { phase: "links", current: i, total: total_links }, &opts.progress);
        }
    }
    if total_links > 0 { emit(ProgressEvent::FinishedPhase { phase: "links", total: total_links }, &opts.progress); }

    // Other action kinds are ignored for now and left for future extension.
    Ok(())
}

fn apply_dir(image_root: &Path, d: &DirAction, opts: &ApplyOptions) -> Result<(), InstallerError> {
    let full = safe_join(image_root, &d.path)?;
    info!(?full, "creating directory");
    if opts.dry_run {
        return Ok(());
    }

    fs::create_dir_all(&full).map_err(|e| InstallerError::Io {
        source: e,
        path: full.clone(),
    })?;

    // Set permissions if provided
    let mode = parse_mode(&d.mode, 0o755);
    let perm = fs::Permissions::from_mode(mode);
    fs::set_permissions(&full, perm).map_err(|e| InstallerError::Io {
        source: e,
        path: full.clone(),
    })?;

    Ok(())
}

fn ensure_parent(image_root: &Path, p: &str, opts: &ApplyOptions) -> Result<(), InstallerError> {
    let full = safe_join(image_root, p)?;
    if let Some(parent) = full.parent() {
        if opts.dry_run {
            return Ok(());
        }
        fs::create_dir_all(parent).map_err(|e| InstallerError::Io {
            source: e,
            path: parent.to_path_buf(),
        })?;
    }
    Ok(())
}

fn apply_file(image_root: &Path, f: &FileAction, opts: &ApplyOptions) -> Result<(), InstallerError> {
    let full = safe_join(image_root, &f.path)?;

    // Ensure parent exists (directories should already be applied, but be robust)
    ensure_parent(image_root, &f.path, opts)?;

    info!(?full, "creating file (payload handling TBD)");
    if opts.dry_run {
        return Ok(());
    }

    // For now, write empty content as a scaffold. Payload fetching/integration will follow later.
    let mut file = FsFile::create(&full).map_err(|e| InstallerError::Io {
        source: e,
        path: full.clone(),
    })?;
    file.write_all(&[]).map_err(|e| InstallerError::Io {
        source: e,
        path: full.clone(),
    })?;

    // Set permissions if provided
    let mode = parse_mode(&f.mode, 0o644);
    let perm = fs::Permissions::from_mode(mode);
    fs::set_permissions(&full, perm).map_err(|e| InstallerError::Io {
        source: e,
        path: full.clone(),
    })?;

    Ok(())
}

fn apply_link(image_root: &Path, l: &LinkAction, opts: &ApplyOptions) -> Result<(), InstallerError> {
    let link_path = safe_join(image_root, &l.path)?;

    // Determine link type (default to symlink). If properties contain type=hard, create hard link.
    let mut is_hard = false;
    if let Some(prop) = l.properties.get("type") {
        let v = prop.value.to_ascii_lowercase();
        if v == "hard" || v == "hardlink" {
            is_hard = true;
        }
    }

    // Target may be relative; keep it as-is for symlink. For hard links, target must resolve under image_root.
    if opts.dry_run {
        return Ok(());
    }

    if is_hard {
        // Hard link needs a resolved, safe target within the image.
        let target_full = safe_join(image_root, &l.target)?;
        fs::hard_link(&target_full, &link_path).map_err(|e| InstallerError::Io {
            source: e,
            path: link_path.clone(),
        })?;
    } else {
        // Symlink: require non-absolute target to avoid embedding full host paths
        if Path::new(&l.target).is_absolute() {
            return Err(InstallerError::AbsolutePathForbidden { path: l.target.clone() });
        }
        // Create relative symlink as provided (do not convert to absolute to avoid embedding full paths)
        #[cfg(target_family = "unix")]
        {
            unix_fs::symlink(&l.target, &link_path).map_err(|e| InstallerError::Io {
                source: e,
                path: link_path.clone(),
            })?;
        }
        #[cfg(not(target_family = "unix"))]
        {
            return Err(InstallerError::UnsupportedAction {
                action: "link",
                reason: "symlink not supported on this platform".to_string(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_rejects_absolute() {
        let root = Path::new("/tmp/image");
        let err = safe_join(root, "/etc/passwd").unwrap_err();
        match err {
            InstallerError::AbsolutePathForbidden { .. } => {}
            _ => panic!("expected AbsolutePathForbidden"),
        }
    }

    #[test]
    fn safe_join_rejects_escape() {
        let root = Path::new("/tmp/image");
        let err = safe_join(root, "../../etc").unwrap_err();
        match err {
            InstallerError::PathTraversalOutsideImage { .. } => {}
            _ => panic!("expected PathTraversalOutsideImage"),
        }
    }

    #[test]
    fn safe_join_ok() {
        let root = Path::new("/tmp/image");
        let p = safe_join(root, "etc/pkg").unwrap();
        assert!(p.starts_with(root));
        assert!(p.ends_with("pkg"));
    }
}
