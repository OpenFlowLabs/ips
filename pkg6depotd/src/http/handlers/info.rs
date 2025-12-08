use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    http::header,
};
use std::sync::Arc;
use crate::repo::DepotRepo;
use crate::errors::DepotError;
use libips::fmri::Fmri;
use std::str::FromStr;
use libips::actions::Manifest;

pub async fn get_info(
    State(repo): State<Arc<DepotRepo>>,
    Path((publisher, fmri_str)): Path<(String, String)>,
) -> Result<Response, DepotError> {
    let fmri = Fmri::from_str(&fmri_str).map_err(|e| DepotError::Repo(libips::repository::RepositoryError::Other(e.to_string())))?;
    
    let content = repo.get_manifest_text(&publisher, &fmri)?;
    
    let manifest = match serde_json::from_str::<Manifest>(&content) {
        Ok(m) => m,
        Err(_) => Manifest::parse_string(content).map_err(|e| DepotError::Repo(libips::repository::RepositoryError::Other(e.to_string())))?,
    };
    
    let mut out = String::new();
    out.push_str(&format!("Name: {}\n", fmri.name));
    
    if let Some(summary) = find_attr(&manifest, "pkg.summary") {
        out.push_str(&format!("Summary: {}\n", summary));
    }
    out.push_str(&format!("Publisher: {}\n", publisher));
    out.push_str(&format!("Version: {}\n", fmri.version()));
    out.push_str(&format!("FMRI: pkg://{}/{}\n", publisher, fmri));
    
    // License
    // License might be an action (License action) or attribute.
    // Usually it's license actions.
    // For M2 minimal parity, we can skip detailed license text or just say empty if not found.
    // depot.txt sample shows "License:" empty line if none?
    out.push_str("\nLicense:\n");
    for license in &manifest.licenses {
        out.push_str(&format!("{}\n", license.payload));
    }
    
    Ok((
        [(header::CONTENT_TYPE, "text/plain")],
        out
    ).into_response())
}

fn find_attr(manifest: &Manifest, key: &str) -> Option<String> {
    for attr in &manifest.attributes {
        if attr.key == key {
             return attr.values.first().cloned();
        }
    }
    None
}
