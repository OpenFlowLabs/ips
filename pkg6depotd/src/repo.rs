use crate::config::Config;
use crate::errors::{DepotError, Result};
use libips::fmri::Fmri;
use libips::repository::{FileBackend, ReadableRepository, IndexEntry};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct DepotRepo {
    pub backend: Mutex<FileBackend>,
    pub root: PathBuf,
    pub cache_max_age: u64,
}

impl DepotRepo {
    pub fn new(config: &Config) -> Result<Self> {
        let root = config.repository.root.clone();
        let backend = FileBackend::open(&root).map_err(DepotError::Repo)?;
        let cache_max_age = config.server.cache_max_age.unwrap_or(3600);
        Ok(Self {
            backend: Mutex::new(backend),
            root,
            cache_max_age,
        })
    }

    pub fn search(&self, publisher: Option<&str>, query: &str, case_sensitive: bool) -> Result<Vec<IndexEntry>> {
        let backend = self
            .backend
            .lock()
            .map_err(|e| DepotError::Server(format!("Lock poisoned: {}", e)))?;
        backend
            .search_detailed(query, publisher, None, case_sensitive)
            .map_err(DepotError::Repo)
    }

    pub fn get_catalog_path(&self, publisher: &str) -> PathBuf {
        FileBackend::construct_catalog_path(&self.root, publisher)
    }

    pub fn get_file_path(&self, publisher: &str, hash: &str) -> Option<PathBuf> {
        let cand_pub = FileBackend::construct_file_path_with_publisher(&self.root, publisher, hash);
        if cand_pub.exists() {
            return Some(cand_pub);
        }

        let cand_global = FileBackend::construct_file_path(&self.root, hash);
        if cand_global.exists() {
            return Some(cand_global);
        }

        None
    }

    pub fn get_manifest_text(&self, publisher: &str, fmri: &Fmri) -> Result<String> {
        let backend = self
            .backend
            .lock()
            .map_err(|e| DepotError::Server(format!("Lock poisoned: {}", e)))?;
        backend
            .fetch_manifest_text(publisher, fmri)
            .map_err(DepotError::Repo)
    }

    pub fn get_manifest_path(&self, publisher: &str, fmri: &Fmri) -> Option<PathBuf> {
        let version = fmri.version();
        if version.is_empty() {
            return None;
        }
        let path =
            FileBackend::construct_manifest_path(&self.root, publisher, fmri.stem(), &version);
        if path.exists() {
            return Some(path);
        }
        // Fallbacks similar to lib logic
        let encoded_stem = url_encode_filename(fmri.stem());
        let encoded_version = url_encode_filename(&version);
        let alt1 = self
            .root
            .join("pkg")
            .join(&encoded_stem)
            .join(&encoded_version);
        if alt1.exists() {
            return Some(alt1);
        }
        let alt2 = self
            .root
            .join("publisher")
            .join(publisher)
            .join("pkg")
            .join(&encoded_stem)
            .join(&encoded_version);
        if alt2.exists() {
            return Some(alt2);
        }
        None
    }

    pub fn cache_max_age(&self) -> u64 {
        self.cache_max_age
    }

    pub fn get_catalog_file_path(&self, publisher: &str, filename: &str) -> Result<PathBuf> {
        let backend = self
            .backend
            .lock()
            .map_err(|e| DepotError::Server(format!("Lock poisoned: {}", e)))?;
        backend
            .get_catalog_file_path(publisher, filename)
            .map_err(DepotError::Repo)
    }

    pub fn get_info(&self) -> Result<libips::repository::RepositoryInfo> {
        let backend = self
            .backend
            .lock()
            .map_err(|e| DepotError::Server(format!("Lock poisoned: {}", e)))?;
        backend.get_info().map_err(DepotError::Repo)
    }
}

// Local percent-encoding for filenames similar to lib's private helper.
fn url_encode_filename(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push('+'),
            _ => {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).as_bytes() {
                    result.push('%');
                    result.push_str(&format!("{:02X}", b));
                }
            }
        }
    }
    result
}
