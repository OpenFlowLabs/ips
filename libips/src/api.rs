//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

//! High-level, struct-first APIs for forge/pkgdev integration.
//!
//! These facades wrap existing libips modules to provide a stable API surface
//! for building, transforming, linting, resolving, and publishing IPS packages
//! entirely in memory using typed structures.
//!
//! See doc/forge_docs/ips_integration.md for an overview of the end-to-end flow.
//!
//! Quickstart (ignore): Build, lint, resolve, and publish
//! ```ignore
//! use libips::api as ips;
//! use std::path::Path;
//!
//! // 1) Build a Manifest from a prototype directory and base metadata
//! let proto = Path::new("/path/to/proto");
//! let mut manifest = ips::ManifestBuilder::from_prototype_dir(proto)?
//!     .with_base_metadata(ips::BaseMeta {
//!         fmri: Some(ips::Fmri::parse("pkg://pub/example@1.0")?),
//!         summary: Some("Example package".into()),
//!         classification: Some("org.opensolaris.category.2008:Applications/Other".into()),
//!         upstream_url: Some("https://example.com".into()),
//!         source_url: Some("https://example.com/src.tar.gz".into()),
//!         license: Some("MIT".into()),
//!     })
//!     .build();
//!
//! // 2) Generate dependencies with a repository (for FMRI mapping)
//! let mut backend = libips::repository::file_backend::FileBackend::open(Path::new("/repo"))?;
//! manifest = ips::DependencyGenerator::generate_with_repo(&mut backend, Some("pub"), proto, &manifest, ips::DependGenerateOptions::default())?;
//!
//! // 3) Lint and optionally filter rules
//! let mut lint_cfg = ips::LintConfig::default();
//! lint_cfg.disabled_rules = vec!["manifest.summary".into()];
//! let diags = ips::lint::lint_manifest(&manifest, &lint_cfg)?;
//! assert!(diags.is_empty(), "Diagnostics: {:?}", diags);
//!
//! // 4) Publish
//! let repo = ips::Repository::open(Path::new("/repo"))?;
//! if !repo.has_publisher("pub")? { repo.add_publisher("pub")?; }
//! let client = ips::PublisherClient::new(repo, "pub");
//! let mut txn = client.begin()?;
//! txn.add_payload_dir(proto)?;
//! txn.add_manifest(&manifest);
//! txn.commit()?;
//! # Ok::<(), libips::api::IpsError>(())
//! ```

use std::path::{Path, PathBuf};

use miette::Diagnostic;
use thiserror::Error;
use walkdir::WalkDir;

pub use crate::actions::Manifest;
// Core typed manifest
use crate::actions::{
    Attr, Dependency as DependAction, File as FileAction, License as LicenseAction,
    Link as LinkAction, Property,
};
pub use crate::depend::{FileDep, GenerateOptions as DependGenerateOptions};
pub use crate::fmri::Fmri;
// For BaseMeta
use crate::repository::file_backend::{FileBackend, Transaction};
use crate::repository::{
    ReadableRepository, RepositoryError, RepositoryVersion, WritableRepository,
};
use crate::transformer;
pub use crate::transformer::TransformRule;

/// Unified error type for API-level operations
#[derive(Debug, Error, Diagnostic)]
pub enum IpsError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Repository(Box<RepositoryError>),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Transform(#[from] transformer::TransformError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Depend(#[from] crate::depend::DependError),

    #[error("I/O error: {0}")]
    #[diagnostic(code(ips::api_error::io), help("Check file paths and permissions"))]
    Io(String),

    #[error("Unimplemented feature: {feature}")]
    #[diagnostic(
        code(ips::api_error::unimplemented),
        help("See doc/forge_docs/ips_integration.md for roadmap.")
    )]
    Unimplemented { feature: &'static str },
}

/// Base package metadata used by ManifestBuilder.
///
/// Fields are optional to support incremental construction. At minimum,
/// providing `fmri` and `summary` is recommended.
///
/// Example:
/// ```
/// use libips::api::{BaseMeta, Fmri};
/// let meta = BaseMeta {
///     fmri: Some(Fmri::parse("pkg://pub/example@1.0").unwrap()),
///     summary: Some("Example".into()),
///     classification: Some("org.opensolaris.category.2008:Applications/Other".into()),
///     upstream_url: Some("https://example.com".into()),
///     source_url: Some("https://example.com/src.tar.gz".into()),
///     license: Some("MIT".into()),
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct BaseMeta {
    pub fmri: Option<Fmri>,
    pub summary: Option<String>,
    pub classification: Option<String>,
    pub upstream_url: Option<String>,
    pub source_url: Option<String>,
    pub license: Option<String>,
}

/// Build or enrich typed manifests using a fluent builder.
///
/// Example (no_run):
/// ```no_run
/// use libips::api as ips;
/// let mut builder = ips::ManifestBuilder::new();
/// let fmri = ips::Fmri::parse("pkg://pub/name@1.0").unwrap();
/// let summary = String::from("A summary");
/// let classification = "Applications/Other";
/// let project_url = String::from("https://example.com");
/// let source_url = String::from("https://example.com/src.tar.gz");
/// let license_file_name = "license.txt";
/// let license_name = "MIT";
/// builder.add_set("pkg.fmri", &fmri.to_string());
/// builder.add_set("pkg.summary", &summary);
/// builder.add_set(
///     "info.classification",
///     &format!("org.opensolaris.category.2008:{}", classification),
/// );
/// builder.add_set("info.upstream-url", &project_url);
/// builder.add_set("info.source-url", &source_url);
/// builder.add_license(&license_file_name, &license_name);
/// let manifest = builder.build();
/// # Ok::<(), ips::IpsError>(())
/// ```
///
/// Another style using with_base_metadata:
/// Example (no_run):
/// ```no_run
/// use libips::api as ips;
/// use std::path::Path;
/// let proto = Path::new("/proto");
/// let mut manifest = ips::ManifestBuilder::new()
///     .with_base_metadata(ips::BaseMeta {
///         fmri: Some(ips::Fmri::parse("pkg://pub/name@1.0").unwrap()),
///         summary: Some("Summary".into()),
///         classification: None,
///         upstream_url: None,
///         source_url: None,
///         license: None,
///     })
///     .build();
/// # Ok::<(), ips::IpsError>(())
/// ```
pub struct ManifestBuilder {
    manifest: Manifest,
}

impl ManifestBuilder {
    /// Add a simple set (attribute) action: set name=<key> value=<value>
    /// Returns self for chaining.
    pub fn add_set<K: Into<String>, V: ToString>(&mut self, key: K, value: V) -> &mut Self {
        self.manifest.attributes.push(Attr {
            key: key.into(),
            values: vec![value.to_string()],
            properties: Default::default(),
        });
        self
    }

    /// Add a license action, equivalent to: license path=<path> license=<license_name>
    pub fn add_license(&mut self, path: &str, license_name: &str) -> &mut Self {
        let mut props = std::collections::HashMap::new();
        props.insert(
            "path".to_string(),
            Property {
                key: "path".to_string(),
                value: path.to_string(),
            },
        );
        props.insert(
            "license".to_string(),
            Property {
                key: "license".to_string(),
                value: license_name.to_string(),
            },
        );
        self.manifest.licenses.push(LicenseAction {
            payload: String::new(),
            properties: props,
        });
        self
    }

    /// Add a link action
    pub fn add_link(&mut self, path: &str, target: &str) -> &mut Self {
        self.manifest.links.push(LinkAction {
            path: path.to_string(),
            target: target.to_string(),
            properties: Default::default(),
        });
        self
    }

    /// Add a dependency action with a type and an FMRI string (name or full FMRI).
    /// If FMRI parsing fails, the dependency is added without an fmri (will be flagged by lint).
    pub fn add_depend(&mut self, dep_type: &str, fmri_str: &str) -> &mut Self {
        let fmri = Fmri::parse(fmri_str).ok();
        let mut d = DependAction::default();
        d.dependency_type = dep_type.to_string();
        d.fmri = fmri;
        self.manifest.dependencies.push(d);
        self
    }
    /// Start a new empty builder
    pub fn new() -> Self {
        Self {
            manifest: Manifest::new(),
        }
    }

    /// Convenience: construct a Manifest directly by scanning a prototype directory.
    /// Paths in the manifest are stored relative to `proto`.
    pub fn from_prototype_dir(proto: &Path) -> Result<Manifest, IpsError> {
        if !proto.exists() {
            return Err(IpsError::Io(format!(
                "prototype directory does not exist: {}",
                proto.display()
            )));
        }
        let root = proto.canonicalize().map_err(|e| {
            IpsError::Io(format!("failed to canonicalize {}: {}", proto.display(), e))
        })?;

        let mut m = Manifest::new();
        for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_file() {
                // Build File action from absolute path
                let mut f = FileAction::read_from_path(p).map_err(RepositoryError::from)?;
                // Store path relative to root
                let rel = p
                    .strip_prefix(&root)
                    .map_err(RepositoryError::from)?
                    .to_string_lossy()
                    .to_string();
                f.path = rel;
                m.add_file(f);
            }
        }
        Ok(m)
    }

    /// Add base metadata to the manifest using typed fields.
    pub fn with_base_metadata(mut self, meta: BaseMeta) -> Self {
        // Helper to push an attribute set action
        let mut push_attr = |key: &str, val: String| {
            self.manifest.attributes.push(Attr {
                key: key.to_string(),
                values: vec![val],
                properties: Default::default(),
            });
        };

        if let Some(fmri) = meta.fmri {
            push_attr("pkg.fmri", fmri.to_string());
        }
        if let Some(s) = meta.summary {
            push_attr("pkg.summary", s);
        }
        if let Some(c) = meta.classification {
            push_attr("info.classification", c);
        }
        if let Some(u) = meta.upstream_url {
            push_attr("info.upstream-url", u);
        }
        if let Some(su) = meta.source_url {
            push_attr("info.source-url", su);
        }
        if let Some(l) = meta.license {
            // Represent base license via an attribute named 'license'; callers may add dedicated license actions separately
            self.manifest.attributes.push(Attr {
                key: "license".to_string(),
                values: vec![l],
                properties: Default::default(),
            });
        }
        self
    }

    /// Apply typed transform rules to the manifest (in place)
    pub fn apply_rules(mut self, rules: &[TransformRule]) -> Result<Self, IpsError> {
        let rules: Vec<crate::actions::Transform> = rules.iter().cloned().map(Into::into).collect();
        transformer::apply(&mut self.manifest, &rules)?;
        Ok(self)
    }

    /// Finalize and return the Manifest
    pub fn build(self) -> Manifest {
        self.manifest
    }
}

/// Minimal repository facade backed by an on-disk file repository.
///
/// Example (no_run):
/// ```no_run
/// use libips::api::Repository;
/// use std::path::Path;
/// let repo_path = Path::new("/repo");
/// // Create if needed
/// let _ = Repository::create(repo_path);
/// // Open and ensure publisher
/// let repo = Repository::open(repo_path)?;
/// if !repo.has_publisher("pub")? { repo.add_publisher("pub")?; }
/// # Ok::<(), libips::api::IpsError>(())
/// ```
pub struct Repository {
    path: PathBuf,
}

impl Repository {
    pub fn open(path: &Path) -> Result<Self, IpsError> {
        // Validate by opening backend
        let _ = FileBackend::open(path)?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    pub fn create(path: &Path) -> Result<Self, IpsError> {
        let _ = FileBackend::create(path, RepositoryVersion::default())?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    pub fn has_publisher(&self, name: &str) -> Result<bool, IpsError> {
        let backend = FileBackend::open(&self.path)?;
        let info = backend.get_info()?;
        Ok(info.publishers.iter().any(|p| p.name == name))
    }

    pub fn add_publisher(&self, name: &str) -> Result<(), IpsError> {
        let mut backend = FileBackend::open(&self.path)?;
        backend.add_publisher(name)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// High-level publishing client for starting repository transactions.
///
/// Example (no_run):
/// ```no_run
/// use libips::api as ips;
/// use std::path::Path;
/// let repo = ips::Repository::open(Path::new("/repo"))?;
/// let client = ips::PublisherClient::new(repo, "pub");
/// let mut tx = client.begin()?;
/// // Add payloads and manifests, then commit
/// # Ok::<(), ips::IpsError>(())
/// ```
pub struct PublisherClient {
    repo: Repository,
    publisher: String,
}

impl PublisherClient {
    pub fn new(repo: Repository, publisher: impl Into<String>) -> Self {
        Self {
            repo,
            publisher: publisher.into(),
        }
    }

    /// Begin a new transaction
    pub fn begin(&self) -> Result<Txn, IpsError> {
        let backend = FileBackend::open(self.repo.path())?;
        let tx = backend.begin_transaction()?; // returns Transaction bound to repo path
        Ok(Txn {
            backend_path: self.repo.path().to_path_buf(),
            tx,
            publisher: self.publisher.clone(),
        })
    }
}

/// Transaction wrapper exposing add_payload_dir/add_manifest/commit.
///
/// Start a transaction via PublisherClient::begin, add payload directories and manifests,
/// then commit to publish.
///
/// Example (no_run):
/// ```no_run
/// use libips::api as ips;
/// use std::path::Path;
/// let repo = ips::Repository::open(Path::new("/repo"))?;
/// let client = ips::PublisherClient::new(repo, "pub");
/// let mut tx = client.begin()?;
/// tx.add_payload_dir(Path::new("/proto"))?;
/// tx.add_manifest(&ips::Manifest::new());
/// tx.commit()?;
/// # Ok::<(), ips::IpsError>(())
/// ```
pub struct Txn {
    backend_path: PathBuf,
    tx: Transaction,
    publisher: String,
}

impl Txn {
    /// Add all files from the given payload/prototype directory
    pub fn add_payload_dir(&mut self, dir: &Path) -> Result<(), IpsError> {
        let root = dir.canonicalize().map_err(|e| {
            IpsError::Io(format!("failed to canonicalize {}: {}", dir.display(), e))
        })?;
        for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_file() {
                let mut f = FileAction::read_from_path(p).map_err(RepositoryError::from)?;
                let rel = p
                    .strip_prefix(&root)
                    .map_err(RepositoryError::from)?
                    .to_string_lossy()
                    .to_string();
                f.path = rel;
                self.tx.add_file(f, p)?;
            }
        }
        Ok(())
    }

    /// Merge the provided manifest into the transaction's manifest
    pub fn add_manifest(&mut self, manifest: &Manifest) {
        self.tx.update_manifest(manifest.clone());
    }

    /// Commit the transaction to the repository for the preselected publisher
    pub fn commit(mut self) -> Result<(), IpsError> {
        self.tx.set_publisher(&self.publisher);
        self.tx.commit()?;
        // Rebuild metadata (catalog and index)
        let backend = FileBackend::open(&self.backend_path)?;
        backend.rebuild(Some(&self.publisher), false, false)?;
        Ok(())
    }
}

/// Dependency generation facade.
///
/// Use this to compute file-level dependencies and resolve them to package
/// FMRIs using a repository.
///
/// Example: generate dependencies with a repository (no_run)
/// ```no_run
/// use libips::api as ips;
/// use std::path::Path;
/// use libips::repository::{FileBackend, ReadableRepository};
/// let proto = Path::new("/proto");
/// let mut backend = FileBackend::open(Path::new("/repo"))?;
/// let manifest = ips::Manifest::new();
/// let manifest = ips::DependencyGenerator::generate_with_repo(
///     &mut backend,
///     Some("pub"),
///     proto,
///     &manifest,
///     ips::DependGenerateOptions::default(),
/// )?;
/// # Ok::<(), ips::IpsError>(())
/// ```
pub struct DependencyGenerator;

impl DependencyGenerator {
    /// Compute file-level dependencies for the given manifest, using `proto` as base for local file resolution.
    /// This is a helper for callers that want to inspect raw file deps before mapping them to package FMRIs.
    pub fn file_deps(
        proto: &Path,
        manifest: &Manifest,
        mut opts: DependGenerateOptions,
    ) -> Result<Vec<FileDep>, IpsError> {
        if opts.proto_dir.is_none() {
            opts.proto_dir = Some(proto.to_path_buf());
        }
        let deps = crate::depend::generate_file_dependencies_from_manifest(manifest, &opts)?;
        Ok(deps)
    }

    /// Generate dependencies and return a new manifest with Depend actions injected.
    /// Intentionally not implemented in this facade: mapping raw file dependencies to package FMRIs
    /// requires repository/catalog context. Call `generate_with_repo` instead.
    pub fn generate(_proto: &Path, _manifest: &Manifest) -> Result<Manifest, IpsError> {
        Err(IpsError::Unimplemented {
            feature: "DependencyGenerator::generate (use generate_with_repo)",
        })
    }

    /// Generate dependencies using a repository to resolve file-level deps into package FMRIs.
    pub fn generate_with_repo<R: ReadableRepository>(
        repo: &mut R,
        publisher: Option<&str>,
        proto: &Path,
        manifest: &Manifest,
        mut opts: DependGenerateOptions,
    ) -> Result<Manifest, IpsError> {
        if opts.proto_dir.is_none() {
            opts.proto_dir = Some(proto.to_path_buf());
        }
        let file_deps = crate::depend::generate_file_dependencies_from_manifest(manifest, &opts)?;
        let deps = crate::depend::resolve_dependencies(repo, publisher, &file_deps)?;
        let mut out = manifest.clone();
        out.dependencies.extend(deps);
        Ok(out)
    }
}

/// Cross-manifest dependency resolver.
///
/// This helper fills missing publisher/version on dependency FMRIs either by
/// inspecting peer manifests in-memory or by querying a repository.
///
/// Examples (no_run):
/// ```no_run
/// use libips::api as ips;
/// // Peer-manifest resolve
/// let mut manifests: Vec<ips::Manifest> = vec![]; // populate with manifests that depend on each other
/// ips::Resolver::resolve(&mut manifests)?;
/// # Ok::<(), ips::IpsError>(())
/// ```
/// ```no_run
/// use libips::api as ips;
/// use std::path::Path;
/// // Repository-backed resolve
/// use libips::repository::{FileBackend, ReadableRepository};
/// let backend = FileBackend::open(Path::new("/repo"))?;
/// let mut manifests: Vec<ips::Manifest> = vec![]; // populate
/// ips::Resolver::resolve_with_repo(&backend, Some("pub"), &mut manifests)?;
/// # Ok::<(), ips::IpsError>(())
/// ```
pub struct Resolver;

impl Resolver {
    /// Best-effort peer-manifest resolver.
    /// Note: For production resolution against published packages, prefer resolve_with_repo().
    pub fn resolve(manifests: &mut [Manifest]) -> Result<(), IpsError> {
        // Build a map from package name (stem) to full FMRI from the provided manifests
        use std::collections::HashMap;
        let mut providers: HashMap<String, Fmri> = HashMap::new();
        for m in manifests.iter() {
            if let Some(f) = manifest_fmri(m) {
                providers.insert(f.stem().to_string(), f);
            }
        }

        // For each manifest dependency that has an FMRI with missing publisher/version,
        // fill in from providers if there is a matching manifest by name.
        for m in manifests.iter_mut() {
            for dep in &mut m.dependencies {
                if let Some(ref mut f) = dep.fmri {
                    // Only attempt if version is missing
                    if f.version.is_none() {
                        if let Some(p) = providers.get(f.stem()) {
                            // Fill publisher if missing and version from provider
                            if f.publisher.is_none() {
                                f.publisher = p.publisher.clone();
                            }
                            if f.version.is_none() {
                                f.version = p.version.clone();
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Resolve dependency FMRIs using a repository of already-published packages.
    /// For each dependency with a name-only FMRI (missing version), if exactly one
    /// package with that name exists in the given publisher, fill in publisher and version.
    /// If multiple or zero matches are found, the dependency is left unchanged.
    pub fn resolve_with_repo<R: ReadableRepository>(
        repo: &R,
        publisher: Option<&str>,
        manifests: &mut [Manifest],
    ) -> Result<(), IpsError> {
        for m in manifests.iter_mut() {
            for dep in &mut m.dependencies {
                if let Some(ref mut f) = dep.fmri {
                    if f.version.is_none() {
                        // Query repository for this package name
                        let pkgs = repo.list_packages(publisher, Some(&f.name))?;
                        let matches: Vec<&crate::repository::PackageInfo> =
                            pkgs.iter().filter(|pi| pi.fmri.name == f.name).collect();
                        if matches.len() == 1 {
                            let fmri = &matches[0].fmri;
                            if f.publisher.is_none() {
                                f.publisher = fmri.publisher.clone();
                            }
                            if f.version.is_none() {
                                f.version = fmri.version.clone();
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

// Helper: extract the package FMRI from a manifest's attributes
fn manifest_fmri(manifest: &Manifest) -> Option<Fmri> {
    for attr in &manifest.attributes {
        if attr.key == "pkg.fmri" {
            if let Some(val) = attr.values.first() {
                if let Ok(f) = Fmri::parse(val) {
                    return Some(f);
                }
            }
        }
    }
    None
}

/// Lint facade providing a typed, extensible rule engine with enable/disable controls.
///
/// Configure which rules to run, override severities, and pass rule-specific parameters.
///
/// Example: disable a rule and run lint (no_run)
/// ```no_run
/// use libips::api as ips;
/// let mut cfg = ips::LintConfig::default();
/// cfg.disabled_rules = vec!["manifest.summary".into()];
/// let mut m = ips::Manifest::new();
/// m.attributes.push(libips::actions::Attr{ key: "pkg.fmri".into(), values: vec!["pkg://pub/name@1.0".into()], properties: Default::default() });
/// let diags = ips::lint::lint_manifest(&m, &cfg)?;
/// assert!(diags.is_empty());
/// # Ok::<(), ips::IpsError>(())
/// ```
#[derive(Debug, Clone, Default)]
pub struct LintConfig {
    pub reference_repos: Vec<PathBuf>,
    pub rulesets: Vec<String>,
    // Rule configurability
    pub disabled_rules: Vec<String>,       // rule IDs to disable
    pub enabled_only: Option<Vec<String>>, // if Some, only these rule IDs run
    pub severity_overrides: std::collections::HashMap<String, lint::LintSeverity>,
    pub rule_params: std::collections::HashMap<String, std::collections::HashMap<String, String>>, // rule_id -> (key->val)
}

pub mod lint {
    use super::*;
    use miette::Diagnostic;
    use thiserror::Error;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum LintSeverity {
        Error,
        Warning,
        Info,
    }

    #[derive(Debug, Error, Diagnostic)]
    pub enum LintIssue {
        #[error("Manifest is missing pkg.fmri or it is invalid")]
        #[diagnostic(
            code(ips::lint_error::missing_fmri),
            help("Add a valid set name=pkg.fmri value=... attribute")
        )]
        MissingOrInvalidFmri,

        #[error("Manifest has multiple pkg.fmri attributes")]
        #[diagnostic(
            code(ips::lint_error::duplicate_fmri),
            help("Ensure only one pkg.fmri set action is present")
        )]
        DuplicateFmri,

        #[error("Manifest is missing pkg.summary")]
        #[diagnostic(
            code(ips::lint_error::missing_summary),
            help("Add a set name=pkg.summary value=... attribute")
        )]
        MissingSummary,

        #[error("Dependency is missing FMRI or name")]
        #[diagnostic(
            code(ips::lint_error::dependency_missing_fmri),
            help("Each depend action should include a valid fmri (name or full fmri)")
        )]
        DependencyMissingFmri,

        #[error("Dependency type is missing")]
        #[diagnostic(
            code(ips::lint_error::dependency_missing_type),
            help("Set depend type (e.g., require, incorporate, optional)")
        )]
        DependencyMissingType,
    }

    pub trait LintRule {
        fn id(&self) -> &'static str;
        fn description(&self) -> &'static str;
        fn default_severity(&self) -> LintSeverity {
            LintSeverity::Error
        }
        /// Run this rule against the manifest. Implementors may ignore `config` (prefix with `_`) if not needed.
        /// The config carries enable/disable lists, severity overrides and rule-specific parameters for extensibility.
        fn check(&self, manifest: &Manifest, config: &LintConfig) -> Vec<miette::Report>;
    }

    struct RuleManifestFmri;
    impl LintRule for RuleManifestFmri {
        fn id(&self) -> &'static str {
            "manifest.fmri"
        }
        fn description(&self) -> &'static str {
            "Validate pkg.fmri presence/uniqueness/parse"
        }
        fn check(&self, manifest: &Manifest, _config: &LintConfig) -> Vec<miette::Report> {
            let mut diags = Vec::new();
            let mut fmri_attr_count = 0usize;
            let mut fmri_text: Option<String> = None;
            for attr in &manifest.attributes {
                if attr.key == "pkg.fmri" {
                    fmri_attr_count += 1;
                    if let Some(v) = attr.values.first() {
                        fmri_text = Some(v.clone());
                    }
                }
            }
            if fmri_attr_count > 1 {
                diags.push(miette::Report::new(LintIssue::DuplicateFmri));
            }
            match (fmri_attr_count, fmri_text) {
                (0, _) => diags.push(miette::Report::new(LintIssue::MissingOrInvalidFmri)),
                (_, Some(txt)) => {
                    if crate::fmri::Fmri::parse(&txt).is_err() {
                        diags.push(miette::Report::new(LintIssue::MissingOrInvalidFmri));
                    }
                }
                (_, None) => diags.push(miette::Report::new(LintIssue::MissingOrInvalidFmri)),
            }
            diags
        }
    }

    struct RuleManifestSummary;
    impl LintRule for RuleManifestSummary {
        fn id(&self) -> &'static str {
            "manifest.summary"
        }
        fn description(&self) -> &'static str {
            "Validate pkg.summary presence"
        }
        fn check(&self, manifest: &Manifest, _config: &LintConfig) -> Vec<miette::Report> {
            let mut diags = Vec::new();
            let has_summary = manifest
                .attributes
                .iter()
                .any(|a| a.key == "pkg.summary" && a.values.iter().any(|v| !v.trim().is_empty()));
            if !has_summary {
                diags.push(miette::Report::new(LintIssue::MissingSummary));
            }
            diags
        }
    }

    struct RuleDependencyFields;
    impl LintRule for RuleDependencyFields {
        fn id(&self) -> &'static str {
            "depend.fields"
        }
        fn description(&self) -> &'static str {
            "Validate basic dependency fields"
        }
        fn check(&self, manifest: &Manifest, _config: &LintConfig) -> Vec<miette::Report> {
            let mut diags = Vec::new();
            for dep in &manifest.dependencies {
                let fmri_ok = dep
                    .fmri
                    .as_ref()
                    .map(|f| !f.name.trim().is_empty())
                    .unwrap_or(false);
                if !fmri_ok {
                    diags.push(miette::Report::new(LintIssue::DependencyMissingFmri));
                }
                if dep.dependency_type.trim().is_empty() {
                    diags.push(miette::Report::new(LintIssue::DependencyMissingType));
                }
            }
            diags
        }
    }

    fn default_rules() -> Vec<Box<dyn LintRule>> {
        vec![
            Box::new(RuleManifestFmri),
            Box::new(RuleManifestSummary),
            Box::new(RuleDependencyFields),
        ]
    }

    fn rule_enabled(rule_id: &str, cfg: &LintConfig) -> bool {
        if let Some(only) = &cfg.enabled_only {
            let set: std::collections::HashSet<&str> = only.iter().map(|s| s.as_str()).collect();
            return set.contains(rule_id);
        }
        let disabled: std::collections::HashSet<&str> =
            cfg.disabled_rules.iter().map(|s| s.as_str()).collect();
        !disabled.contains(rule_id)
    }

    /// Lint a manifest and return diagnostics. Does not fail the call; diagnostics are returned as reports.
    ///
    /// Example (no_run):
    /// ```no_run
    /// use libips::api as ips;
    /// let mut m = ips::Manifest::new();
    /// m.attributes.push(libips::actions::Attr{ key: "pkg.fmri".into(), values: vec!["pkg://pub/name@1.0".into()], properties: Default::default() });
    /// let cfg = ips::LintConfig::default();
    /// let diags = ips::lint::lint_manifest(&m, &cfg)?;
    /// assert!(diags.is_empty());
    /// # Ok::<(), ips::IpsError>(())
    /// ```
    pub fn lint_manifest(
        manifest: &Manifest,
        config: &LintConfig,
    ) -> Result<Vec<miette::Report>, IpsError> {
        let mut diags: Vec<miette::Report> = Vec::new();
        for rule in default_rules().into_iter() {
            if rule_enabled(rule.id(), config) {
                diags.extend(rule.check(manifest, config).into_iter());
            }
        }
        Ok(diags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{Attr, Dependency as ManifestDependency};

    fn make_manifest_with_fmri(fmri_str: &str) -> Manifest {
        let mut m = Manifest::new();
        m.attributes.push(Attr {
            key: "pkg.fmri".into(),
            values: vec![fmri_str.to_string()],
            properties: Default::default(),
        });
        m
    }

    #[test]
    fn resolver_fills_version_and_publisher_from_peer_manifest() {
        // Provider manifest: pkgA with publisher and version
        let provider = make_manifest_with_fmri("pkg://pub/pkgA@1.0");

        // Consumer manifest with dependency on pkgA without version/publisher
        let mut consumer = make_manifest_with_fmri("pkg://pub/consumer@0.1");
        let dep_fmri = Fmri::parse("pkgA").unwrap();
        consumer.dependencies.push(ManifestDependency {
            fmri: Some(dep_fmri),
            dependency_type: "require".to_string(),
            predicate: None,
            root_image: String::new(),
            optional: Vec::new(),
            facets: Default::default(),
        });

        let mut manifests = vec![provider, consumer];
        Resolver::resolve(&mut manifests).unwrap();

        // After resolve, the consumer's first dependency should have version and publisher set
        let consumer_after = &manifests[1];
        let dep = &consumer_after.dependencies[0];
        let fmri = dep.fmri.as_ref().unwrap();
        assert_eq!(fmri.name, "pkgA");
        assert_eq!(fmri.publisher.as_deref(), Some("pub"));
        assert!(
            fmri.version.is_some(),
            "expected version to be filled from provider"
        );
        assert_eq!(fmri.version.as_ref().unwrap().to_string(), "1.0");
    }

    #[test]
    fn resolver_uses_repository_for_provider() {
        use crate::repository::RepositoryVersion;
        use crate::repository::file_backend::FileBackend;

        // Create a temporary repository and add a publisher
        let tmp = tempfile::tempdir().unwrap();
        let repo_path = tmp.path().join("repo");
        let mut backend = FileBackend::create(&repo_path, RepositoryVersion::default()).unwrap();
        backend.add_publisher("pub").unwrap();

        // Publish provider package pkgA@1.0
        let mut provider = Manifest::new();
        provider.attributes.push(Attr {
            key: "pkg.fmri".into(),
            values: vec!["pkg://pub/pkgA@1.0".to_string()],
            properties: Default::default(),
        });
        let mut tx = backend.begin_transaction().unwrap();
        tx.update_manifest(provider);
        tx.set_publisher("pub");
        tx.commit().unwrap();
        backend.rebuild(Some("pub"), false, false).unwrap();

        // Build consumer with name-only dependency
        let mut consumer = make_manifest_with_fmri("pkg://pub/consumer@0.1");
        let dep_fmri = Fmri::parse("pkgA").unwrap();
        consumer.dependencies.push(ManifestDependency {
            fmri: Some(dep_fmri),
            dependency_type: "require".to_string(),
            predicate: None,
            root_image: String::new(),
            optional: Vec::new(),
            facets: Default::default(),
        });

        let mut manifests = vec![consumer];
        Resolver::resolve_with_repo(&backend, Some("pub"), &mut manifests).unwrap();
        let dep = &manifests[0].dependencies[0];
        let fmri = dep.fmri.as_ref().unwrap();
        assert_eq!(fmri.publisher.as_deref(), Some("pub"));
        assert_eq!(fmri.version.as_ref().unwrap().to_string(), "1.0");
    }

    #[test]
    fn lint_reports_missing_fmri_and_summary() {
        let m = Manifest::new();
        let cfg = LintConfig::default();
        let diags = lint::lint_manifest(&m, &cfg).unwrap();
        assert!(!diags.is_empty());
    }

    #[test]
    fn lint_accepts_valid_manifest() {
        let mut m = Manifest::new();
        m.attributes.push(Attr {
            key: "pkg.fmri".into(),
            values: vec!["pkg://pub/name@1.0".to_string()],
            properties: Default::default(),
        });
        m.attributes.push(Attr {
            key: "pkg.summary".into(),
            values: vec!["A package".to_string()],
            properties: Default::default(),
        });
        let cfg = LintConfig::default();
        let diags = lint::lint_manifest(&m, &cfg).unwrap();
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn lint_disable_summary_rule() {
        // Manifest with valid fmri but missing summary
        let mut m = Manifest::new();
        m.attributes.push(Attr {
            key: "pkg.fmri".into(),
            values: vec!["pkg://pub/name@1.0".to_string()],
            properties: Default::default(),
        });

        // Disable the summary rule; expect no diagnostics
        let mut cfg = LintConfig::default();
        cfg.disabled_rules = vec!["manifest.summary".to_string()];
        let diags = lint::lint_manifest(&m, &cfg).unwrap();
        // fmri is valid, dependencies empty, summary rule disabled => no diags
        assert!(
            diags.is_empty(),
            "expected no diagnostics when summary rule disabled, got: {:?}",
            diags
        );
    }

    #[test]
    fn builder_add_set_license_link_depend() {
        // add_set with Fmri and strings
        let fmri = Fmri::parse("pkg://pub/example@1.0").unwrap();
        let mut b = ManifestBuilder::new();
        b.add_set("pkg.fmri", &fmri);
        b.add_set("pkg.summary", "Summary");
        b.add_set("info.upstream-url", "https://example.com");
        b.add_license("LICENSE", "MIT");
        b.add_link("usr/bin/foo", "../libexec/foo");
        b.add_depend("require", "pkg://pub/dep@1.2");
        let m = b.build();

        // Validate attributes include fmri and summary
        assert!(m.attributes.iter().any(|a| {
            a.key == "pkg.fmri"
                && a.values
                    .first()
                    .map(|v| v == &fmri.to_string())
                    .unwrap_or(false)
        }));
        assert!(
            m.attributes.iter().any(|a| a.key == "pkg.summary"
                && a.values.first().map(|v| v == "Summary").unwrap_or(false))
        );

        // Validate license
        assert_eq!(m.licenses.len(), 1);
        let lic = &m.licenses[0];
        assert_eq!(
            lic.properties.get("path").map(|p| p.value.as_str()),
            Some("LICENSE")
        );
        assert_eq!(
            lic.properties.get("license").map(|p| p.value.as_str()),
            Some("MIT")
        );

        // Validate link
        assert_eq!(m.links.len(), 1);
        let ln = &m.links[0];
        assert_eq!(ln.path, "usr/bin/foo");
        assert_eq!(ln.target, "../libexec/foo");

        // Validate dependency
        assert_eq!(m.dependencies.len(), 1);
        let dep = &m.dependencies[0];
        assert_eq!(dep.dependency_type, "require");
        let df = dep.fmri.as_ref().expect("dep fmri parsed");
        assert_eq!(df.publisher.as_deref(), Some("pub"));
        assert_eq!(df.version.as_ref().unwrap().to_string(), "1.2");
    }
}

impl From<RepositoryError> for IpsError {
    fn from(err: RepositoryError) -> Self {
        Self::Repository(Box::new(err))
    }
}
