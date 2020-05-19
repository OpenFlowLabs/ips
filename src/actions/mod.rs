//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use regex::{RegexSet, Regex};
use std::collections::HashSet;
use std::error;
use std::fmt;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use failure::Error;

#[derive(Debug, Default)]
pub struct Dir {
    pub path: String,
    pub group: String,
    pub owner: String,
    pub mode: String, //TODO implement as bitmask
    pub revert_tag: String,
    pub salvage_from: String,
    pub facets: HashSet<Facet>,
}

#[derive(Hash, Eq, PartialEq, Debug, Default)]
pub struct Facet {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Default)]
pub struct Attr {
    pub key: String,
    pub values: Vec<String>,
    pub properties: HashSet<Property>,
}

#[derive(Hash, Eq, PartialEq, Debug, Default)]
pub struct Property {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Default)]
pub struct Manifest {
    pub attributes: Vec<Attr>,
    pub directories: Vec<Dir>,
}

impl Manifest {
    pub fn new() -> Manifest {
        return Manifest {
            attributes: Vec::new(),
            directories: Vec::new(),
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
    Legacy,
    Unknown{action: String},
}

//TODO Multierror and no failure for these cases
#[derive(Debug, Fail)]
pub enum ManifestError {
    #[fail(display = "unknown action {} at line {}", action, line)]
    UnknownAction {
        line: usize,
        action: String,
    },
    #[fail(display = "action string \"{}\" at line {} is invalid: {}", action, line, message)]
    InvalidAction {
        line: usize,
        action: String,
        message: String,
    },
}

pub fn parse_manifest_file(filename: String) -> Result<Manifest, Error> {
    let mut m = Manifest::new();
    let f = File::open(filename)?;

    let file = BufReader::new(&f);

    for (line_nr, line_read) in file.lines().enumerate() {
        handle_manifest_line(&mut m, line_read?.trim_start(), line_nr)?;
    }

    return Ok(m);
}

pub fn parse_manifest_string(manifest: String) -> Result<Manifest, Error> {
    let mut m = Manifest::new();
    for (line_nr, line) in manifest.lines().enumerate() {
        handle_manifest_line(&mut m, line.trim_start(), line_nr)?;
    }
    return Ok(m);
}

fn handle_manifest_line(manifest: &mut Manifest, line: &str, line_nr: usize) -> Result<(), Error> {
    match determine_action_kind(&line) {
        ActionKind::Attr => {
            manifest.attributes.push(parse_attr_action(String::from(line))?);
        }
        ActionKind::Dir => {
            manifest.directories.push(parse_dir_action(String::from(line), line_nr)?);
        }
        ActionKind::File => {

        }
        ActionKind::Dependency => {

        }
        ActionKind::User => {

        }
        ActionKind::Group => {

        }
        ActionKind::Driver => {

        }
        ActionKind::License => {

        }
        ActionKind::Link => {

        }
        ActionKind::Legacy => {

        }
        ActionKind::Unknown{action} => {
            Err(ManifestError::UnknownAction {action, line: line_nr})?;
        }
    }
    Ok(())
}

fn determine_action_kind(line: &str) -> ActionKind {
    let mut act = String::new();
    for c in line.trim_start().chars() {
        if c == ' ' {
            break
        }
        act.push(c)
    }

    return match act.as_str() {
        "set" => ActionKind::Attr,
        "depend" => ActionKind::Dependency,
        "dir" => ActionKind::Dir,
        "file" => ActionKind::File,
        "license" => ActionKind::License,
        "hardlink" => ActionKind::Link,
        "link" => ActionKind::Link,
        "driver" => ActionKind::Driver,
        "group" => ActionKind::Group,
        "user" => ActionKind::User,
        "legacy" => ActionKind::Legacy,
        _ => ActionKind::Unknown{action: act},
    }
}

fn parse_dir_action(line: String, line_nr: usize) -> Result<Dir, Error> {
    let mut act = Dir::default();
    let regex = Regex::new(r#"(([^ ]+)=([^"][^ ]+[^"])|([^ ]+)=([^"][^ ]+[^"]))"#)?;

    for cap in regex.captures_iter(line.trim_start()) {
        match &cap[1] {
            "path" => act.path = String::from(&cap[2]).replace(&['"', '\\'][..], ""),
            "owner" => act.owner = String::from(&cap[2]).replace(&['"', '\\'][..], ""),
            "group" => act.group = String::from(&cap[2]).replace(&['"', '\\'][..], ""),
            "mode" => act.mode = String::from(&cap[2]).replace(&['"', '\\'][..], ""),
            "revert-tag" => act.revert_tag = String::from(&cap[2]).replace(&['"', '\\'][..], ""),
            "salvage-from" => act.salvage_from = String::from(&cap[2]).replace(&['"', '\\'][..], ""),
            _ => {
                let key_val_string = String::from(&cap[1]).replace(&['"', '\\'][..], "");
                if key_val_string.contains("facet.") {
                    let key = match key_val_string.find(".") {
                        Some(idx) => {
                            key_val_string.clone().split_off(idx+1)
                        },
                        None => return Err(ManifestError::InvalidAction{action: line, line: line_nr, message: String::from("separation dot not found but string contains facet.")})?
                    };

                    let value = match key_val_string.find("=") {
                        Some(idx) => {
                            key_val_string.clone().split_off(idx+1)
                        },
                        None => return Err(ManifestError::InvalidAction{action: line, line: line_nr, message: String::from("no value present for facet")})?
                    };

                    if !act.facets.insert(Facet{name: key, value: value}) {
                        return Err(ManifestError::InvalidAction{action: line, line: line_nr, message: String::from("double declaration of facet")})?
                    }
                }
            }
        }
    }

    Ok(act)
}

fn parse_attr_action(line: String) -> Result<Attr, Error> {
    // Do a full line match to see if we can fast path this.
    // This also catches values with spaces, that have not been properly escaped.
    // Note: values with spaces must be properly escaped or the rest here will fail. Strings with
    //  unescaped spaces are never valid but sadly present in the wild.
    // Fast path will fail if a value has multiple values or a '=' sign in the values
    let full_line_regex = Regex::new(r"^set name=([^ ]+) value=(.+)$")?;

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
                //TODO knock out single quotes somehow

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
    let name_regex = Regex::new(r"name=([^ ]+) value=")?;
    let mut key = String::new();
    for cap in name_regex.captures_iter(line.trim_start()) {
        key = String::from(&cap[1]);
    }

    let mut values = Vec::new();
    let value_no_space_regex = Regex::new(r#"value="(.+)""#)?;

    let value_space_regex = Regex::new(r#"value=([^"][^ ]+[^"])"#)?;

    let mut properties = HashSet::new();
    let optionals_regex_no_quotes = Regex::new(r#"([^ ]+)=([^"][^ ]+[^"])"#)?;

    let optionals_regex_quotes = Regex::new(r#"([^ ]+)=([^"][^ ]+[^"])"#)?;

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
