//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use regex::{Regex, Captures};
use std::collections::HashSet;
use std::error;
use std::fmt;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;

#[derive(Debug, Default)]
pub struct Dir {
    pub path: String,
    pub group: String,
    pub owner: String,
    pub mode: String, //TODO implement as bitmask
}

#[derive(Debug, Default)]
pub struct Attr {
    pub key: String,
    pub values: Vec<String>,
    pub properties: HashSet<Property>,
}

#[derive(Hash, Eq, PartialEq, Debug)]
pub struct Property {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Default)]
pub struct Manifest {
    pub attributes: Vec<Attr>,
}

impl Manifest {
    pub fn new() -> Manifest {
        return Manifest {
            attributes: Vec::new(),
        };
    }
}

enum ActionKind {
    Attr,
    Dir,
    File,
    Dependency,
    User,
    Group,
    Driver,
    License,
    Link,
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
            ManifestError::EmptyVec => write!(f, "please use a vector with at least one element"),
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

pub fn parse_manifest_file(filename: String) -> Result<Manifest, ManifestError> {
    let mut manifest = Manifest::new();
    let f = match File::open(filename) {
        Ok(file) => file,
        Err(e) => return Err(ManifestError::Read(e)),
    };
    let file = BufReader::new(&f);
    for line_read in file.lines() {
        let line = match line_read {
            Ok(l) => l,
            Err(e) => return Err(ManifestError::Read(e)),
        };
        if is_attr_action(&line) {
            match parse_attr_action(line) {
                Ok(attr) => manifest.attributes.push(attr),
                Err(e) => return Err(e),
            }
        }
    }
    return Ok(manifest);
}

pub fn parse_manifest_string(manifest: String) -> Result<Manifest, ManifestError> {
    let mut m = Manifest::new();
    for line in manifest.lines() {
        if is_attr_action(&String::from(line)) {
            match parse_attr_action(String::from(line)) {
                Ok(attr) => m.attributes.push(attr),
                Err(e) => return Err(e),
            };
        }
    }
    return Ok(m);
}

fn is_attr_action(line: &String) -> bool {
    if line.trim().starts_with("set ") {
        return true;
    }
    return false;
}

pub fn parse_attr_action(line: String) -> Result<Attr, ManifestError> {
    // Do a full line match to see if we can fast path this.
    // This also catches values with spaces, that have not been properly escaped.
    // Note: values with spaces must be properly escaped or the rest here will fail. Strings with
    //  unescaped spaces are never valid but sadly present in the wild.
    // Fast path will fail if a value has multiple values or a '=' sign in the values
    let full_line_regex = match Regex::new(r"^set name=([^ ]+) value=(.+)$") {
        Ok(re) => re,
        Err(e) => return Err(ManifestError::Regex(e)),
    };

    if full_line_regex.is_match(line.trim_start()) {
        match full_line_regex.captures(line.trim_start()) {
            Some(captures) => {
                let mut fast_path_fail = false;
                let mut val = String::from(&captures[2]);

                if val.contains("=") {
                    fast_path_fail = true;
                }

                if val.contains("value=") {
                    fast_path_fail = true;
                }

                if val.contains("name=") {
                    fast_path_fail = true;
                }

                val = val.replace(&['"', '\\'][..], "");

                if !fast_path_fail{
                    return Ok(Attr{
                        key: String::from(&captures[1]),
                        values: vec![val],
                        ..Attr::default()
                    });
                }
            }
            None => (),
        };
    }


    //Todo move regex initialisation out of for loop into static area
    let name_regex = match Regex::new(r"name=([^ ]+) value=") {
        Ok(re) => re,
        Err(e) => return Err(ManifestError::Regex(e)),
    };
    let mut key = String::new();
    for cap in name_regex.captures_iter(line.trim_start()) {
        key = String::from(&cap[1]);
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

    let mut properties = HashSet::new();
    let optionals_regex_no_quotes = match Regex::new(r#"([^ ]+)=([^"][^ ]+[^"])"#) {
        Ok(re) => re,
        Err(e) => return Err(ManifestError::Regex(e)),
    };

    let optionals_regex_quotes = match Regex::new(r#"([^ ]+)=([^"][^ ]+[^"])"#) {
        Ok(re) => re,
        Err(e) => return Err(ManifestError::Regex(e)),
    };

    for cap in value_no_space_regex.captures_iter(line.trim_start()) {
        values.push(String::from(cap[1].trim()));
    }

    for cap in value_space_regex.captures_iter(line.trim_start()) {
        values.push(String::from(cap[1].trim()));
    }

    for cap in optionals_regex_quotes.captures_iter(line.trim_start()) {
        if cap[1].trim().starts_with("name") || cap[1].trim().starts_with("value") {
            continue;
        }

        properties.insert(Property {
            key: String::from(cap[1].trim()),
            value: String::from(cap[2].trim()),
        });
    }

    for cap in optionals_regex_no_quotes.captures_iter(line.trim_start()) {
        if cap[1].trim().starts_with("name") || cap[1].trim().starts_with("value") {
            continue;
        }

        properties.insert(Property {
            key: String::from(cap[1].trim()),
            value: String::from(cap[2].trim()),
        });
    }

    Ok(Attr {
        key,
        values,
        properties,
    })
}
