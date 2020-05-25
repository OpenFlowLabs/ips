//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use regex::{RegexSet, Regex};
use std::collections::HashSet;
use std::fs::File as OsFile;
use std::io::BufRead;
use std::io::BufReader;
use failure::Error;
use crate::payload::Payload;
use std::clone::Clone;

trait FacetedAction {
    // Add a facet to the action if the facet is already present the function returns false.
    fn add_facet(&mut self, facet: Facet) -> bool;

    // Remove a facet from the action.
    fn remove_facet(&mut self, facet: Facet) -> bool;
}

#[derive(Debug)]
pub struct Action {
    kind: ActionKind,
    payload_reference: String,
    properties: Vec<Property>,
    facets: HashSet<Facet>,
}

impl FacetedAction for Action {
    fn add_facet(&mut self, facet: Facet) -> bool {
        return self.facets.insert(facet)
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        return self.facets.remove(&facet)
    }
}

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

impl FacetedAction for Dir {
    fn add_facet(&mut self, facet: Facet) -> bool {
        return self.facets.insert(facet)
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        return self.facets.remove(&facet)
    }
}

#[derive(Debug, Default)]
pub struct File {
    pub payload: Payload,
    pub path: String,
    pub group: String,
    pub owner: String,
    pub mode: String, //TODO implement as bitmask
    pub preserve: bool,
    pub overlay: bool,
    pub original_name: String,
    pub revert_tag: String,
    pub sys_attr: String,
    pub properties: Vec<Property>,
    pub facets: HashSet<Facet>,
}

impl FacetedAction for File {
    fn add_facet(&mut self, facet: Facet) -> bool {
        return self.facets.insert(facet)
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        return self.facets.remove(&facet)
    }
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
    pub files: Vec<File>,
}

impl Manifest {
    pub fn new() -> Manifest {
        return Manifest {
            attributes: Vec::new(),
            directories: Vec::new(),
            files: Vec::new(),
        };
    }
}

#[derive(Debug)]
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
    let f = OsFile::open(filename)?;

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
            manifest.files.push(parse_file_action(String::from(line), line_nr)?);
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

fn add_facet_to_action<T: FacetedAction>(action: &mut T, facet_string: String, line: String, line_nr: usize) -> Result<(), ManifestError> {
    let facet_key = match facet_string.find(".") {
        Some(idx) => {
            facet_string.clone().split_off(idx+1)
        },
        None => return Err(ManifestError::InvalidAction{action: line, line: line_nr, message: String::from("separation dot not found but string contains facet.")})?
    };

    let value = match facet_string.find("=") {
        Some(idx) => {
            facet_string.clone().split_off(idx+1)
        },
        None => return Err(ManifestError::InvalidAction{action: line, line: line_nr, message: String::from("no value present for facet")})?
    };

    if !action.add_facet(Facet{name: facet_key, value}) {
        return Err(ManifestError::InvalidAction{action: line, line: line_nr, message: String::from("double declaration of facet")})?
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

fn clean_string_value(orig: &str) -> String {
    return String::from(orig).trim_end().replace(&['"', '\\'][..], "")
}

fn string_to_bool(orig: &str) -> Result<bool, String> {
    match &String::from(orig).trim().to_lowercase()[..] {
        "true" => Ok(true),
        "false" => Ok(false),
        "t" => Ok(true),
        "f" => Ok(false),
        _ => Err(String::from("not a boolean like value"))
    }
}

fn parse_file_action(line: String, line_nr: usize) -> Result<File, Error> {
    let mut act = File::default();
    let regex_set = RegexSet::new(&[
        r"file ([a-zA-Z0-9]+) ",
        r#"([^ ]+)=([^"][^ ]+[^"])"#,
        r#"([^ ]+)="(.+)"#
    ])?;

    for (pat, idx) in regex_set.matches(line.trim_start()).into_iter().map(|match_idx| (&regex_set.patterns()[match_idx], match_idx)) {
        let regex = Regex::new(&pat)?;

        for cap in regex.captures_iter(line.trim_start()) {
            if idx == 0 {
                act.payload = String::from(&cap[1]);
                continue;
            }

            let full_cap_idx = 0;
            let key_cap_idx = 1;
            let val_cap_idx = 2;

            match &cap[key_cap_idx] {
                "path" => act.path = clean_string_value(&cap[val_cap_idx]),
                "owner" => act.owner = clean_string_value(&cap[val_cap_idx]),
                "group" => act.group = clean_string_value(&cap[val_cap_idx]),
                "mode" => act.mode = clean_string_value(&cap[val_cap_idx]),
                "revert-tag" => act.revert_tag = clean_string_value(&cap[val_cap_idx]),
                "original_name" => act.original_name = clean_string_value(&cap[val_cap_idx]),
                "sysattr" => act.sys_attr = clean_string_value(&cap[val_cap_idx]),
                "overlay" => act.overlay = match string_to_bool(&cap[val_cap_idx]) {
                    Ok(b) => b,
                    Err(e) => return Err(ManifestError::InvalidAction {action: line, line: line_nr, message: e})?
                },
                "preserve" => act.preserve = match string_to_bool(&cap[val_cap_idx]) {
                    Ok(b) => b,
                    Err(e) => return Err(ManifestError::InvalidAction {action: line, line: line_nr, message: e})?
                },
                _ => {
                    let key_val_string = clean_string_value(&cap[full_cap_idx]);

                    if key_val_string.contains("facet.") {
                        match add_facet_to_action(&mut act, key_val_string, line, line_nr) {
                            Ok(_) => continue,
                            Err(e) => return Err(e)?,
                        }
                    }

                    let mut key = key_val_string.clone();
                    let value = match key.find("=") {
                        Some(idx) => {
                            key.split_off(idx+1)
                        },
                        None => return Err(ManifestError::InvalidAction{action: line, line: line_nr, message: String::from("no value present for facet")})?
                    };
                    key = key.replace("=", "");
                    act.properties.push(Property{key, value});
                }
            }
        }
    }


    Ok(act)
}

fn parse_dir_action(line: String, line_nr: usize) -> Result<Dir, Error> {
    let mut act = Dir::default();
    let regex_set = RegexSet::new(&[
        r#"([^ ]+)=([^"][^ ]+[^"])"#,
        r#"([^ ]+)="(.+)"#
    ])?;

    for pat in regex_set.matches(line.trim_start()).into_iter().map(|match_idx| &regex_set.patterns()[match_idx]) {
        let regex = Regex::new(&pat)?;

        for cap in regex.captures_iter(line.trim_start()) {
            let full_cap_idx = 0;
            let key_cap_idx = 1;
            let val_cap_idx = 2;


            match &cap[key_cap_idx] {
                "path" => act.path = clean_string_value(&cap[val_cap_idx]),
                "owner" => act.owner = clean_string_value(&cap[val_cap_idx]),
                "group" => act.group = clean_string_value(&cap[val_cap_idx]),
                "mode" => act.mode = clean_string_value(&cap[val_cap_idx]),
                "revert-tag" => act.revert_tag = clean_string_value(&cap[val_cap_idx]),
                "salvage-from" => act.salvage_from = clean_string_value(&cap[val_cap_idx]),
                _ => {
                    let key_val_string = clean_string_value(&cap[full_cap_idx]);
                    if key_val_string.contains("facet.") {
                        match add_facet_to_action(&mut act, key_val_string, line, line_nr) {
                            Ok(_) => continue,
                            Err(e) => return Err(e)?,
                        }
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
                //TODO knock out single quotes somehow without

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
