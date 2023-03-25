use std::{
    fmt::Display,
    path::{Path, PathBuf},
    result::Result as StdResult,
};
use thiserror::Error;
use url::{ParseError, Url};

type Result<T> = StdResult<T, SourceError>;

#[derive(Debug, Error)]
pub enum SourceError {
    #[error("can't create source from url: {0}")]
    CantCreateSource(String),
    #[error("can not parse source url: {0}")]
    UrlParseError(#[from] ParseError),
}

#[derive(Debug, Clone)]
pub struct Source {
    pub url: Url,
    pub local_name: PathBuf,
}

impl Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.local_name.display())
    }
}

impl Source {
    pub fn new<P: AsRef<Path>>(url_string: &str, local_base: P) -> Result<Source> {
        let url = Url::parse(url_string)?;
        let path = url.path().to_owned();
        let path_vec: Vec<_> = path.split('/').collect();
        match path_vec.last() {
            Some(local_name) => Ok(Source {
                url,
                local_name: local_base.as_ref().join(local_name),
            }),
            None => Err(SourceError::CantCreateSource(url.into()))?,
        }
    }
}
