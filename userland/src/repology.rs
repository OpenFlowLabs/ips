extern crate reqwest;

use anyhow::Result;
use reqwest::*;
use semver::Version;
use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://repology.org/api/v1/";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Package {
    repo: String,
    name: Option<String>,
    version: String,
    sub_repo: Option<String>,
    orig_version: Option<String>,
    status: Option<String>,
    summary: Option<String>,
    family: Option<String>,
    categories: Option<Vec<String>>,
    licenses: Option<Vec<String>>,
    maintainers: Option<Vec<String>>,
    www: Option<Vec<String>>,
    downloads: Option<Vec<String>>,
}

pub fn project(package: &str) -> Result<Vec<Package>> {
    let url = Url::parse(&format!("{}/project/{}", BASE_URL, package))?;

    let json = reqwest::blocking::get(url)?.json::<Vec<Package>>()?;

    Ok(json)
}

pub fn find_newest_version(package: &str) -> Result<String> {
    let pkgs = project(package)?;
    let version_res: Result<Vec<Version>> = pkgs
        .iter()
        .map(|p| -> Result<Version> {
            let v = Version::parse(&p.version);
            if v.is_ok() {
                return Ok(v?);
            }
            Ok(Version::new(0, 0, 1))
        })
        .collect();

    let mut versions = version_res?;

    versions.sort();

    Ok(versions.last().unwrap().to_string())
}
