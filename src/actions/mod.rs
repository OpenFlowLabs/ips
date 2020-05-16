use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::error;
use std::fmt;
use regex::Regex;
use regex::RegexSet;

pub struct Attr {
    pub Key: String,
    pub Values: Vec<String>,
}

pub struct Manifest {
    pub Attributes: Vec<Attr>,
}

impl Manifest {
    pub fn new() -> Manifest {
        return Manifest{
            Attributes: Vec::new(),
        };
    }
}

#[derive(Debug)]
pub enum ManifestError {
    EmptyVec,
    // We will defer to the parse error implementation for their error.
    // Supplying extra info requires adding more data to the type.
    Read(std::io::Error),
    Regex(regex::Error),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ManifestError::EmptyVec =>
                write!(f, "please use a vector with at least one element"),
            // This is a wrapper, so defer to the underlying types' implementation of `fmt`.
            ManifestError::Read(ref e) => e.fmt(f),
            ManifestError::Regex(ref e) => e.fmt(f),
        }
    }
}

impl error::Error for ManifestError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            ManifestError::EmptyVec => None,
            // The cause is the underlying implementation error type. Is implicitly
            // cast to the trait object `&error::Error`. This works because the
            // underlying type already implements the `Error` trait.
            ManifestError::Read(ref e) => Some(e),
            ManifestError::Regex(ref e) => Some(e),
        }
    }
}

pub fn ParseManifestFile(filename: String) -> Result<Manifest, ManifestError> {
    let mut manifest = Manifest::new();
    let f = match File::open(filename) {
        Ok(file) => file,
        Err(e) => return Err(ManifestError::Read(e)),
    };
    let file = BufReader::new(&f);
    for lineRead in file.lines() {
        let line = match lineRead {
            Ok(l) => l,
            Err(e) => return Err(ManifestError::Read(e)),
        };
        if isAttrLine(&line) {
            match ParseAttrLine(line) {
                Ok(attr) => manifest.Attributes.push(attr),
                Err(e) => return Err(e)
            }
        }
    } 
    return Ok(manifest);
}

pub fn ParseManifestString(manifest: String) -> Result<Manifest, ManifestError> {
    let mut m = Manifest::new();
    for line in manifest.lines() {
        if isAttrLine(&String::from(line)) {
            let attr = match ParseAttrLine(String::from(line)) {
                Ok(attr) => m.Attributes.push(attr),
                Err(e) => return Err(e)
            };
        }
    }
    return Ok(m)
}

fn isAttrLine(line: &String) -> bool {
    if line.trim().starts_with("set ") {
        return true;
    }
    return false;
}

pub fn ParseAttrLine(line: String) -> Result<Attr, ManifestError> {
    let name_regex = match Regex::new(r"name=([^ ]+) value=") {
        Ok(re) => re,
        Err(e) => return Err(ManifestError::Regex(e))
    };
    let mut name = String::new();
    for cap in name_regex.captures_iter(line.trim_start()) {
        name = String::from(&cap[1]);
    }

    let mut values = Vec::new();
    let value_no_space_regex = match Regex::new(r#"value="(.+)""#) {
        Ok(re) => re,
        Err(e) => return Err(ManifestError::Regex(e)),
    };

    let value_space_regex = match Regex::new(r#"value=([^"][^ ]+[^"])"#) {
        Ok(re) => re,
        Err(e) => return Err(ManifestError::Regex(e)),
    };

    for cap in value_no_space_regex.captures_iter(line.trim_start()) {
        values.push(String::from(&cap[1]));
    }

    for cap in value_space_regex.captures_iter(line.trim_start()) {
        values.push(String::from(&cap[1]));
    }

    Ok(Attr{
        Key: name,
        Values: values,
    })
}