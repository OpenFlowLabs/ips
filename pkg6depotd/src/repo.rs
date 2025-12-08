use std::path::PathBuf;
use libips::repository::{FileBackend, ReadableRepository};
use crate::config::Config;
use crate::errors::{Result, DepotError};
use libips::fmri::Fmri;
use std::sync::Mutex;

pub struct DepotRepo {
    pub backend: Mutex<FileBackend>,
    pub root: PathBuf,
}

impl DepotRepo {
    pub fn new(config: &Config) -> Result<Self> {
        let root = config.repository.root.clone();
        let backend = FileBackend::open(&root).map_err(DepotError::Repo)?;
        Ok(Self { backend: Mutex::new(backend), root })
    }

    pub fn get_catalog_path(&self, publisher: &str) -> PathBuf {
        FileBackend::construct_catalog_path(&self.root, publisher)
    }

    pub fn get_file_path(&self, publisher: &str, hash: &str) -> Option<PathBuf> {
         let cand_pub = FileBackend::construct_file_path_with_publisher(&self.root, publisher, hash);
         if cand_pub.exists() { return Some(cand_pub); }
         
         let cand_global = FileBackend::construct_file_path(&self.root, hash);
         if cand_global.exists() { return Some(cand_global); }
         
         None
    }
    
    pub fn get_manifest_text(&self, publisher: &str, fmri: &Fmri) -> Result<String> {
        let backend = self.backend.lock().map_err(|e| DepotError::Server(format!("Lock poisoned: {}", e)))?;
        backend.fetch_manifest_text(publisher, fmri).map_err(DepotError::Repo)
    }
}
