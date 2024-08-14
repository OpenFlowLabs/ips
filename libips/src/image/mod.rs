mod properties;

use std::collections::HashMap;
use std::fs::File;
use properties::*;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImageError {
    //Implement derives for IO error and serde_json error
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ImageError>;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Image {
    path: PathBuf,
    props: Vec<ImageProperty>,
    version: i32,
    variants: HashMap<String, String>,
    mediators: HashMap<String, String>,
}

impl Image {
    pub fn new<P: Into<PathBuf>>(path: P) -> Image {
        Image{
            path: path.into(),
            version: 5,
            variants: HashMap::new(),
            mediators: HashMap::new(),
            props: vec![],
        }
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Image> {
        let path = path.as_ref();

        //TODO: Parse the old INI format of pkg5
        //TODO once root images are implemented, look for metadata under sub directory var/pkg
        let props_path = path.join("pkg6.image.json");
        let mut f = File::open(props_path)?;
        Ok(serde_json::from_reader(&mut f)?)
    }

    pub fn open_default<P: AsRef<Path>>(path: P) -> Image {
        if let Ok(img) = Image::open(path.as_ref()) {
            img
        } else {
            Image::new(path.as_ref())
        }
    }
}