extern crate reqwest;

use reqwest::*;
use crate::errors::Result as EResult;
use semver::Version;
use serde::{Serialize, Deserialize};

const BASE_URL: &str = "https://repology.org/api/v1/";

#[derive(Debug, Clone)]
pub struct RepologyClient {
    client: reqwest::Client,
}

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
    downloads: Option<Vec<String>>
}

pub fn project(package: &str) -> Result<Vec<Package>> {

    let url = Url::parse(&format!("{}/project/{}",BASE_URL, package)).unwrap();

    let json = reqwest::blocking::get(url)?
        .json::<Vec<Package>>()?;

    return Ok(json);
}

pub fn find_newest_version(package: &str) -> EResult<String> {
    let pkgs = project(package)?;
    let version_res: EResult<Vec<Version>> = pkgs.iter().map(|p| -> EResult<Version> {
        let v = Version::parse(&p.version);
        if v.is_ok() {
            return Ok(v?);
        }
        Ok(Version::new(0,0,1))
    }).collect();

    let mut versions = version_res?;

    versions.sort();

    Ok(versions.last().unwrap().to_string())
}
