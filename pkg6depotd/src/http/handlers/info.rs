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
use chrono::{NaiveDateTime, Utc, TimeZone, Datelike, Timelike};
use libips::actions::Property;
use std::fs;
use std::io::Read as _;

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
    // Parse version components for Version, Build Release, Branch, and Packaging Date
    let version_full = fmri.version();
    let mut version_core = version_full.clone();
    let mut build_release: Option<String> = None;
    let mut branch: Option<String> = None;
    let mut ts_str: Option<String> = None;

    if let Some((core, rest)) = version_full.split_once(',') {
        version_core = core.to_string();
        if let Some((rel_branch, ts)) = rest.split_once(':') {
            ts_str = Some(ts.to_string());
            if let Some((rel, br)) = rel_branch.split_once('-') {
                if !rel.is_empty() { build_release = Some(rel.to_string()); }
                if !br.is_empty() { branch = Some(br.to_string()); }
            } else {
                // No branch
                if !rel_branch.is_empty() { build_release = Some(rel_branch.to_string()); }
            }
        } else {
            // No timestamp
            if let Some((rel, br)) = rest.split_once('-') {
                if !rel.is_empty() { build_release = Some(rel.to_string()); }
                if !br.is_empty() { branch = Some(br.to_string()); }
            } else if !rest.is_empty() {
                build_release = Some(rest.to_string());
            }
        }
    }

    out.push_str(&format!("Version: {}\n", version_core));
    if let Some(rel) = build_release { out.push_str(&format!("Build Release: {}\n", rel)); }
    if let Some(br) = branch { out.push_str(&format!("Branch: {}\n", br)); }
    if let Some(ts) = ts_str.and_then(|s| format_packaging_date(&s)) {
        out.push_str(&format!("Packaging Date: {}\n", ts));
    }
    // Compute sizes from manifest file actions (pkg.size and pkg.csize)
    let (total_size, total_csize) = compute_sizes(&manifest);
    out.push_str(&format!("Size: {}\n", human_bytes(total_size)));
    out.push_str(&format!("Compressed Size: {}\n", human_bytes(total_csize)));
    // Construct a correct FMRI string:
    // Always use the publisher from the URL segment, and format as
    // pkg://<publisher>/<name>@<version>
    let name = fmri.stem();
    let version = fmri.version();
    if version.is_empty() {
        out.push_str(&format!("FMRI: pkg://{}/{}\n", publisher, name));
    } else {
        out.push_str(&format!("FMRI: pkg://{}/{}@{}\n", publisher, name, version));
    }
    
    // License
    // Print actual license text content from repository instead of hash.
    out.push_str("\nLicense:\n");
    let mut first = true;
    for license in &manifest.licenses {
        if !first { out.push('\n'); }
        first = false;

        // Optional license name header for readability
        if let Some(name_prop) = license.properties.get("license") {
            if !name_prop.value.is_empty() {
                out.push_str(&format!("Name: {}\n", name_prop.value));
            }
        }

        // Resolve file by digest payload
        let digest = license.payload.trim();
        if !digest.is_empty() {
            match resolve_license_text(&repo, &publisher, digest) {
                Some(text) => {
                    out.push_str(&text);
                    if !text.ends_with('\n') { out.push('\n'); }
                }
                None => {
                    // Fallback: show the digest if content could not be resolved
                    out.push_str(&format!("<license content unavailable for digest {}>\n", digest));
                }
            }
        }
    }
    
    Ok((
        [(header::CONTENT_TYPE, "text/plain")],
        out
    ).into_response())
}

// Try to read and decode the license text for a given digest from the repository.
// - Prefer publisher-scoped file path; fallback to global location.
// - If content appears to be gzip-compressed (magic 1f 8b), decompress.
// - Decode as UTF-8 (lossy) and enforce a maximum output size to avoid huge responses.
fn resolve_license_text(repo: &DepotRepo, publisher: &str, digest: &str) -> Option<String> {
    let path = repo.get_file_path(publisher, digest)?;
    let bytes = fs::read(&path).ok()?;

    // Detect gzip magic
    let mut data: Vec<u8> = bytes;
    if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
        // Attempt to gunzip
        let mut decoder = flate2::read::GzDecoder::new(&data[..]);
        let mut decompressed = Vec::new();
        match decoder.read_to_end(&mut decompressed) {
            Ok(_) => data = decompressed,
            Err(_) => {
                // Leave as-is if decompression fails
            }
        }
    }

    // Limit output size to 256 KiB
    const MAX_LICENSE_BYTES: usize = 256 * 1024;
    let truncated = data.len() > MAX_LICENSE_BYTES;
    if truncated {
        data.truncate(MAX_LICENSE_BYTES);
    }

    let mut text = String::from_utf8_lossy(&data).to_string();
    if truncated {
        if !text.ends_with('\n') { text.push('\n'); }
        text.push_str("...[truncated]\n");
    }
    Some(text)
}

fn find_attr(manifest: &Manifest, key: &str) -> Option<String> {
    for attr in &manifest.attributes {
        if attr.key == key {
             return attr.values.first().cloned();
        }
    }
    None
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    }
}

fn format_packaging_date(ts: &str) -> Option<String> {
    // Expect formats like YYYYMMDDThhmmssZ or with fractional seconds before Z
    let clean_ts = if let Some((base, _frac)) = ts.split_once('.') { format!("{}Z", base) } else { ts.to_string() };
    let ndt = NaiveDateTime::parse_from_str(&clean_ts, "%Y%m%dT%H%M%SZ").ok()?;
    let dt_utc = Utc.from_utc_datetime(&ndt);
    let month = month_name(dt_utc.month() as u32);
    let day = dt_utc.day();
    let year = dt_utc.year();
    let hour24 = dt_utc.hour();
    let (ampm, hour12) = if hour24 == 0 { ("AM", 12) } else if hour24 < 12 { ("AM", hour24) } else if hour24 == 12 { ("PM", 12) } else { ("PM", hour24 - 12) };
    let minute = dt_utc.minute();
    let second = dt_utc.second();
    Some(format!("{} {:02}, {} at {:02}:{:02}:{:02} {}", month, day, year, hour12, minute, second, ampm))
}

// Sum pkg.size (uncompressed) and pkg.csize (compressed) over all file actions
fn compute_sizes(manifest: &Manifest) -> (u128, u128) {
    let mut size: u128 = 0;
    let mut csize: u128 = 0;

    for file in &manifest.files {
        for Property { key, value } in &file.properties {
            if key == "pkg.size" {
                if let Ok(v) = value.parse::<u128>() { size = size.saturating_add(v); }
            } else if key == "pkg.csize" {
                if let Ok(v) = value.parse::<u128>() { csize = csize.saturating_add(v); }
            }
        }
    }

    (size, csize)
}

fn human_bytes(bytes: u128) -> String {
    // Use binary (IEC-like) units for familiarity; format with two decimals for KB and above
    const KIB: u128 = 1024;
    const MIB: u128 = 1024 * 1024;
    const GIB: u128 = 1024 * 1024 * 1024;
    const TIB: u128 = 1024 * 1024 * 1024 * 1024;

    if bytes < KIB {
        return format!("{} B", bytes);
    } else if bytes < MIB {
        return format!("{:.2} KB", (bytes as f64) / (KIB as f64));
    } else if bytes < GIB {
        return format!("{:.2} MB", (bytes as f64) / (MIB as f64));
    } else if bytes < TIB {
        return format!("{:.2} GB", (bytes as f64) / (GIB as f64));
    } else {
        return format!("{:.2} TB", (bytes as f64) / (TIB as f64));
    }
}
