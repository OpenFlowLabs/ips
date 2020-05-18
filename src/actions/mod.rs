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
use failure::Error;

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
}

pub fn parse_manifest_file(filename: String) -> Result<Manifest, Error> {
    let mut m = Manifest::new();
    let f = File::open(filename)?;

    let file = BufReader::new(&f);

    for (line_nr, line_read) in file.lines().enumerate() {
        let line = line_read?;
        match determine_action_kind(&line) {
            ActionKind::Attr => {
                let attr = parse_attr_action(String::from(line))?;
                m.attributes.push(attr)
            }
            ActionKind::Dir => {

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
    }

    return Ok(m);
}

pub fn parse_manifest_string(manifest: String) -> Result<Manifest, Error> {
    let mut m = Manifest::new();
    for (line_nr, line) in manifest.lines().enumerate() {
        match determine_action_kind(&line) {
            ActionKind::Attr => {
                let attr = parse_attr_action(String::from(line))?;
                m.attributes.push(attr)
            }
            ActionKind::Dir => {

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
    }
    return Ok(m);
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

pub fn parse_attr_action(line: String) -> Result<Attr, Error> {
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
