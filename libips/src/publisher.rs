//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use std::path::{Path, PathBuf};
use std::fs;

use miette::Diagnostic;
use thiserror::Error;

use crate::actions::{File as FileAction, Manifest, Transform as TransformAction};
use crate::repository::{ReadableRepository, RepositoryError, WritableRepository};
use crate::repository::file_backend::{FileBackend, Transaction};
use crate::transformer;

/// Error type for high-level publishing operations
#[derive(Debug, Error, Diagnostic)]
pub enum PublisherError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Repository(#[from] RepositoryError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Transform(#[from] transformer::TransformError),

    #[error("I/O error: {0}")]
    #[diagnostic(code(ips::publisher_error::io), help("Check the path and permissions"))]
    Io(String),

    #[error("invalid root path: {0}")]
    #[diagnostic(code(ips::publisher_error::invalid_root_path), help("Ensure the directory exists and is readable"))]
    InvalidRoot(String),
}

pub type Result<T> = std::result::Result<T, PublisherError>;

/// High-level Publisher client that keeps a repository handle and an open transaction.
///
/// This is intended to simplify software build/publish flows: instantiate once with a
/// repository path and publisher, then build/transform manifests and publish.
pub struct PublisherClient {
    backend: FileBackend,
    publisher: String,
    tx: Option<Transaction>,
    transform_rules: Vec<transformer::TransformRule>,
}

impl PublisherClient {
    /// Open an existing repository located at `path` with a selected `publisher`.
    pub fn open<P: AsRef<Path>>(path: P, publisher: impl Into<String>) -> Result<Self> {
        let backend = FileBackend::open(path)?;
        Ok(Self { backend, publisher: publisher.into(), tx: None, transform_rules: Vec::new() })
    }

    /// Open a transaction if not already open and return whether a new transaction was created.
    pub fn open_transaction(&mut self) -> Result<bool> {
        if self.tx.is_none() {
            let tx = self.backend.begin_transaction()?;
            self.tx = Some(tx);
            return Ok(true);
        }
        Ok(false)
    }

    /// Build a new Manifest from a directory tree. Paths in the manifest are relative to `root`.
    pub fn build_manifest_from_dir(&mut self, root: &Path) -> Result<Manifest> {
        if !root.exists() {
            return Err(PublisherError::InvalidRoot(root.display().to_string()));
        }
        let mut manifest = Manifest::new();
        let root = root.canonicalize().map_err(|_| PublisherError::InvalidRoot(root.display().to_string()))?;

        let walker = walkdir::WalkDir::new(&root).into_iter().filter_map(|e| e.ok());
        // Ensure a transaction is open
        if self.tx.is_none() {
            self.open_transaction()?;
        }
        let tx = self.tx.as_mut().expect("transaction must be open");

        for entry in walker {
            let p = entry.path();
            if p.is_file() {
                // Create a File action from the absolute path
                let mut f = FileAction::read_from_path(p).map_err(RepositoryError::from)?;
                // Set path to be relative to root
                let rel: PathBuf = p
                    .strip_prefix(&root)
                    .map_err(RepositoryError::from)?
                    .to_path_buf();
                f.path = rel.to_string_lossy().to_string();
                // Add into manifest and stage via transaction
                manifest.add_file(f.clone());
                tx.add_file(f, p)?;
            }
        }
        Ok(manifest)
    }

    /// Make a new empty manifest
    pub fn new_empty_manifest(&self) -> Manifest {
        Manifest::new()
    }

    /// Transform a manifest with a user-supplied rule function
    pub fn transform_manifest<F>(&self, mut manifest: Manifest, rule: F) -> Manifest
    where
        F: FnOnce(&mut Manifest),
    {
        rule(&mut manifest);
        manifest
    }

    /// Add a single AST transform rule
    pub fn add_transform_rule(&mut self, rule: transformer::TransformRule) {
        self.transform_rules.push(rule);
    }

    /// Add multiple AST transform rules
    pub fn add_transform_rules(&mut self, rules: Vec<transformer::TransformRule>) {
        self.transform_rules.extend(rules);
    }

    /// Clear all configured transform rules
    pub fn clear_transform_rules(&mut self) {
        self.transform_rules.clear();
    }

    /// Load transform rules from raw text (returns number of rules added)
    pub fn load_transform_rules_from_text(&mut self, text: &str) -> Result<usize> {
        let rules = transformer::parse_rules_ast(text)?;
        let n = rules.len();
        self.transform_rules.extend(rules);
        Ok(n)
    }

    /// Load transform rules from a file (returns number of rules added)
    pub fn load_transform_rules_from_file<P: AsRef<Path>>(&mut self, path: P) -> Result<usize> {
        let p = path.as_ref();
        let content = fs::read_to_string(p).map_err(|e| PublisherError::Io(e.to_string()))?;
        self.load_transform_rules_from_text(&content)
    }

    /// Publish the given manifest. If no transaction is open, one will be opened.
    /// The transaction will be updated with the provided manifest and committed.
    /// If `rebuild_metadata` is true, repository metadata (catalog/index) will be rebuilt.
    pub fn publish(&mut self, mut manifest: Manifest, rebuild_metadata: bool) -> Result<()> {
        // Apply configured transform rules (if any)
        if !self.transform_rules.is_empty() {
            let rules: Vec<TransformAction> = self
                .transform_rules
                .clone()
                .into_iter()
                .map(Into::into)
                .collect();
            transformer::apply(&mut manifest, &rules)?;
        }

        // Ensure transaction exists
        if self.tx.is_none() {
            self.open_transaction()?;
        }

        // Take ownership of the transaction, update and commit
        let mut tx = self.tx.take().expect("transaction must be open");
        tx.set_publisher(&self.publisher);
        tx.update_manifest(manifest);
        tx.commit()?;
        // Optionally rebuild repo metadata for the publisher
        if rebuild_metadata {
            self.backend.rebuild(Some(&self.publisher), false, false)?;
        }
        Ok(())
    }
}
