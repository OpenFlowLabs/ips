//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

// Source https://docs.oracle.com/cd/E23824_01/html/E21796/pkg-5.html

use regex::{RegexSet, Regex};
use std::collections::{HashMap};
use std::fs::{File as OsFile, read_to_string};
use std::io::BufRead;
use std::io::BufReader;
use crate::payload::Payload;
use std::clone::Clone;
use crate::digest::Digest;
use std::str::FromStr;
use std::path::{Path};
use std::fmt;
use crate::errors::Result;
use pest::Parser;

pub trait FacetedAction {
    // Add a facet to the action if the facet is already present the function returns false.
    fn add_facet(&mut self, facet: Facet) -> bool;

    // Remove a facet from the action.
    fn remove_facet(&mut self, facet: Facet) -> bool;
}

#[derive(Debug, Default)]
pub struct Action {
    kind: ActionKind,
    payload: Payload,
    payload_string: String,
    properties: Vec<Property>,
    facets: HashMap<String, Facet>,
}

impl Action {
    pub fn new(kind: ActionKind) -> Action{
        Action{
            kind,
            payload: Payload::default(),
            payload_string: String::new(),
            properties: Vec::new(),
            facets: HashMap::new(),
        }
    }
}

impl FacetedAction for Action {
    fn add_facet(&mut self, facet: Facet) -> bool {
        return self.facets.insert(facet.name.clone(), facet.clone()) == None
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        return self.facets.remove(&facet.name) == Some(facet)
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Dir {
    pub path: String,
    pub group: String,
    pub owner: String,
    pub mode: String, //TODO implement as bitmask
    pub revert_tag: String,
    pub salvage_from: String,
    pub facets: HashMap<String, Facet>,
}

impl From<Action> for Dir {
    fn from(act: Action) -> Self {
        let mut dir = Dir::default();
        for prop in act.properties {
            match prop.key.as_str() {
                "path" => dir.path = prop.value,
                "owner" => dir.owner = prop.value,
                "group" => dir.group = prop.value,
                "mode" => dir.mode = prop.value,
                "revert-tag" => dir.revert_tag = prop.value,
                "salvage-from" => dir.salvage_from = prop.value,
                _ => {
                    if is_facet(prop.key.clone()) {
                        dir.add_facet(Facet::from_key_value(prop.key, prop.value));
                    }
                }
            }
        }
        dir
    }
}

impl FacetedAction for Dir {
    fn add_facet(&mut self, facet: Facet) -> bool {
        return self.facets.insert(facet.name.clone(), facet.clone()) == None
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        return self.facets.remove(&facet.name) == Some(facet)
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct File {
    pub payload: Option<Payload>,
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
    pub facets: HashMap<String, Facet>,
}

impl File {
    pub fn read_from_path(p: &Path) -> Result<File> {
        let mut f = File::default();
        match p.to_str() {
            Some(str) => {
                f.path = str.to_string();
                f.payload = Some(Payload::compute_payload(p)?);
            },
            None => return Err(FileError::FilePathIsNoStringError)?,
        }

        //TODO group owner mode

        Ok(f)
    }
}

impl From<Action> for File {
    fn from(act: Action) -> Self {
        let mut file = File::default();
        let mut p = act.payload.clone();
        if !act.payload_string.is_empty() {
            if act.payload_string.contains("/") {
                file.properties.push(Property{
                    key: "original-path".to_string(),
                    value: act.payload_string
                });
            } else {
                p.primary_identifier = Digest::from_str(&act.payload_string).unwrap();
            }
        }
        for prop in act.properties {
            match prop.key.as_str() {
                "path" => file.path = prop.value,
                "owner" => file.owner = prop.value,
                "group" => file.group = prop.value,
                "mode" => file.mode = prop.value,
                "revert-tag" => file.revert_tag = prop.value,
                "original_name" => file.original_name = prop.value,
                "sysattr" => file.sys_attr = prop.value,
                "overlay" => file.overlay = match string_to_bool(&prop.value) {
                    Ok(b) => b,
                    _ => false,
                },
                "preserve" => file.preserve = match string_to_bool(&prop.value) {
                    Ok(b) => b,
                    _ => false,
                },
                "chash" | "pkg.content-hash" => p.additional_identifiers.push(Digest::from_str(&prop.value).unwrap()),
                _ => {
                    if is_facet(prop.key.clone()) {
                        file.add_facet(Facet::from_key_value(prop.key, prop.value));
                    } else {
                        file.properties.push(prop.clone());
                    }
                }
            }
        }
        file
    }
}

impl FacetedAction for File {
    fn add_facet(&mut self, facet: Facet) -> bool {
        return self.facets.insert(facet.name.clone(), facet.clone()) == None
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        return self.facets.remove(&facet.name) == Some(facet)
    }
}

#[derive(Debug, Fail)]
pub enum FileError {
    #[fail(display = "file path is not a string")]
    FilePathIsNoStringError,
}

//TODO implement multiple FMRI for require-any
#[derive(Debug, Default, PartialEq)]
pub struct Dependency {
    pub fmri: String, //TODO make FMRI
    pub dependency_type: String, //TODO make enum
    pub predicate: String,  //TODO make FMRI
    pub root_image: String, //TODO make boolean
    pub optional: Vec<Property>,
    pub facets: HashMap<String, Facet>,
}

impl From<Action> for Dependency {
    fn from(act: Action) -> Self {
        let mut dep = Dependency::default();
        for prop in act.properties {
            match prop.key.as_str() {
                "fmri" => dep.fmri = prop.value,
                "type" => dep.dependency_type = prop.value,
                "predicate" => dep.predicate = prop.value,
                "root-image" => dep.root_image = prop.value,
                _ => {
                    if is_facet(prop.key.clone()) {
                        dep.add_facet(Facet::from_key_value(prop.key, prop.value));
                    } else {
                        dep.optional.push(prop.clone());
                    }
                }
            }
        }
        dep
    }
}

impl FacetedAction for Dependency {
    fn add_facet(&mut self, facet: Facet) -> bool {
        return self.facets.insert(facet.name.clone(), facet.clone()) == None
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        return self.facets.remove(&facet.name) == Some(facet)
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Default, Clone)]
pub struct Facet {
    pub name: String,
    pub value: String,
}

impl Facet {
    fn from_key_value(key: String, value: String) -> Facet {
        Facet{
            name: get_facet_key(key),
            value,
        }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Attr {
    pub key: String,
    pub values: Vec<String>,
    pub properties: HashMap<String, Property>,
}

impl From<Action> for Attr {
    fn from(act: Action) -> Self {
        let mut attr = Attr::default();
        for prop in act.properties {
            match prop.key.as_str() {
                "name" => attr.key = prop.value,
                "value" => attr.values.push(prop.value),
                _ => {
                    attr.properties.insert(prop.key.clone(), Property{
                        key: prop.key,
                        value: prop.value
                    });
                }
            }
        }
        attr
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Default, Clone)]
pub struct Property {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Default, PartialEq)]
pub struct Manifest {
    pub attributes: Vec<Attr>,
    pub directories: Vec<Dir>,
    pub files: Vec<File>,
    pub dependencies: Vec<Dependency>,
}

impl Manifest {
    pub fn new() -> Manifest {
        return Manifest {
            attributes: Vec::new(),
            directories: Vec::new(),
            files: Vec::new(),
            dependencies: Vec::new(),
        };
    }

    pub fn add_file(&mut self, f: File) {
        self.files.push(f);
    }

    fn add_action(&mut self, act: Action) {
        match act.kind {
            ActionKind::Attr => {
                self.attributes.push(act.into());
            }
            ActionKind::Dir => {
                self.directories.push(act.into());
            }
            ActionKind::File => {
                self.files.push(act.into());
            }
            ActionKind::Dependency => {
                self.dependencies.push(act.into());
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
            ActionKind::Transform => {

            }
            ActionKind::Unknown{action} => (),
        }
    }

    pub fn parse_file(f: String) -> Result<Manifest> {
        let content = read_to_string(Path::new(&f))?;
        Manifest::parse_string(content)
    }

    pub fn parse_string(content: String) -> Result<Manifest> {
        let mut m = Manifest::new();

        let pairs = ManifestParser::parse(Rule::manifest, &content)?;

        for p in pairs {
            match p.as_rule() {
                Rule::manifest => {
                    for manifest in p.clone().into_inner() {
                        match manifest.as_rule() {
                            Rule::action => {
                                let mut act = Action::default();
                                for action in manifest.clone().into_inner() {
                                    match action.as_rule() {
                                        Rule::action_name => {
                                            act.kind = get_action_kind(action.as_str());
                                        }
                                        Rule::payload => {
                                            act.payload_string = action.as_str().clone().into();
                                        }
                                        Rule::property => {
                                            let mut property = Property::default();
                                            for prop in action.clone().into_inner() {
                                                match prop.as_rule() {
                                                    Rule::property_name => {
                                                        property.key = prop.as_str().clone().into();
                                                    }
                                                    Rule::property_value => {
                                                        property.value = prop.as_str().clone().into();
                                                    }
                                                    _ => panic!("unexpected rule {:?} inside action expected property_name or property_value", prop.as_rule())
                                                }
                                            }
                                            act.properties.push(property);
                                        }
                                        Rule::EOI => (),
                                        _ => panic!("unexpected rule {:?} inside action expected payload, property, action_name", action.as_rule()),
                                    }
                                }
                                m.add_action(act);
                            }
                            Rule::EOI => (),
                            _ => panic!("unexpected rule {:?} inside manifest expected action", manifest.as_rule()),
                        }
                    }
                }
                _ => panic!("unexpected rule {:?} inside pair expected manifest", p.as_rule()),
            }
        }

        Ok(m)
    }
}

#[derive(Debug)]
pub enum ActionKind {
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
    Transform,
}

impl Default for ActionKind {
    fn default() -> Self { ActionKind::Unknown {action: String::new()} }
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

#[derive(Parser)]
#[grammar = "actions/manifest.pest"]
struct ManifestParser;

fn get_action_kind(act: &str) -> ActionKind {
    return match act {
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
        "<transform" => ActionKind::Transform,
        _ => ActionKind::Unknown{action: act.into()},
    }
}

fn is_facet(s: String) -> bool {
    s.starts_with("facet.")
}

fn get_facet_key(facet_string: String) -> String {
    match facet_string.find(".") {
        Some(idx) => {
            facet_string.clone().split_off(idx+1)
        },
        None => facet_string.clone()
    }
}

pub fn parse_manifest_file(filename: String) -> Result<Manifest> {
    let mut m = Manifest::new();
    let f = OsFile::open(filename)?;

    let file = BufReader::new(&f);

    for (line_nr, line_read) in file.lines().enumerate() {
        let line = line_read?;
        if !line.starts_with("#") {
            handle_manifest_line(&mut m, line.trim_start(), line_nr)?;
        }
    }

    return Ok(m);
}

pub fn parse_manifest_string(manifest: String) -> Result<Manifest> {
    let mut m = Manifest::new();
    for (line_nr, line) in manifest.lines().enumerate() {
        handle_manifest_line(&mut m, line.trim_start(), line_nr)?;
    }
    return Ok(m);
}

fn handle_manifest_line(manifest: &mut Manifest, line: &str, line_nr: usize) -> Result<()> {
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
            manifest.dependencies.push(parse_depend_action(String::from(line),line_nr)?);
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
        ActionKind::Transform => {

        }
        ActionKind::Unknown{action} => {
            if !action.is_empty() {
                Err(ManifestError::UnknownAction {action, line: line_nr})?;
            }
        }
    }
    Ok(())
}

fn add_facet_to_action<T: FacetedAction>(action: &mut T, facet_string: String, line: String, line_nr: usize) -> Result<()> {
    let mut facet_key = match facet_string.find(".") {
        Some(idx) => {
            facet_string.clone().split_off(idx+1)
        },
        None => return Err(ManifestError::InvalidAction{action: line, line: line_nr, message: String::from("separation dot not found but string contains facet.")})?
    };

    let value = match facet_key.find("=") {
        Some(idx) => {
            facet_key.split_off(idx+1)
        },
        None => return Err(ManifestError::InvalidAction{action: line, line: line_nr, message: String::from("no value present for facet")})?
    };

    facet_key.truncate(facet_key.len() - 1);

    if !action.add_facet(Facet{name: clean_string_value(facet_key.as_str()), value: clean_string_value(value.as_str())}) {
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
        "<transform" => ActionKind::Transform,
        _ => ActionKind::Unknown{action: act},
    }
}

fn clean_string_value(orig: &str) -> String {
    return String::from(orig).trim_end().replace(&['"', '\\'][..], "")
}

fn string_to_bool(orig: &str) -> Result<bool> {
    match &String::from(orig).trim().to_lowercase()[..] {
        "true" => Ok(true),
        "false" => Ok(false),
        "t" => Ok(true),
        "f" => Ok(false),
        _ => Err(failure::err_msg("not a boolean like value"))
    }
}

fn parse_depend_action(line: String, line_nr: usize) -> Result<Dependency> {
    let mut act = Dependency::default();
    let regex_set = RegexSet::new(&[
        r#"([^ ]+)=([^"][^ ]+[^"])"#,
        r#"([^ ]+)="(.+)"#
    ])?;

    for (pat, _) in regex_set.matches(line.trim_start()).into_iter().map(|match_idx| (&regex_set.patterns()[match_idx], match_idx)) {
        let regex = Regex::new(&pat)?;
        for cap in regex.captures_iter(line.clone().trim_start()) {
            let full_cap_idx = 0;
            let key_cap_idx = 1;
            let val_cap_idx = 2;

            match &cap[key_cap_idx] {
                "fmri" => act.fmri = clean_string_value(&cap[val_cap_idx]),
                "type" => act.dependency_type = clean_string_value(&cap[val_cap_idx]),
                "predicate" => act.predicate = clean_string_value(&cap[val_cap_idx]),
                "root-image" => act.root_image = clean_string_value(&cap[val_cap_idx]),
                _ => {
                    let key_val_string = String::from(&cap[full_cap_idx]);
                    if key_val_string.contains("facet.") {
                        match add_facet_to_action(&mut act, key_val_string, line.clone(), line_nr) {
                            Ok(_) => continue,
                            Err(e) => return Err(e)?,
                        }
                    } else {
                        act.optional.push(Property{key: clean_string_value(&cap[key_cap_idx]), value: clean_string_value(&cap[val_cap_idx])});
                    }
                }
            }
        }
    }

    Ok(act)
}

fn parse_file_action(line: String, line_nr: usize) -> Result<File> {
    let mut act = File::default();
    let regex_set = RegexSet::new(&[
        r"file ([a-zA-Z0-9]+) ",
        r#"([^ ]+)=([^"][^ ]+[^"])"#,
        r#"([^ ]+)="(.+)"#
    ])?;

    let mut p = Payload::default();

    for (pat, idx) in regex_set.matches(line.trim_start()).into_iter().map(|match_idx| (&regex_set.patterns()[match_idx], match_idx)) {
        let regex = Regex::new(&pat)?;

        for cap in regex.captures_iter(line.clone().trim_start()) {
            if idx == 0 {
                p.primary_identifier = Digest::from_str(&cap[1])?;
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
                    Err(e) => return Err(ManifestError::InvalidAction {action: line, line: line_nr, message: e.to_string()})?
                },
                "preserve" => act.preserve = match string_to_bool(&cap[val_cap_idx]) {
                    Ok(b) => b,
                    Err(e) => return Err(ManifestError::InvalidAction {action: line, line: line_nr, message: e.to_string()})?
                },
                "chash" | "pkg.content-hash" => p.additional_identifiers.push(match Digest::from_str(clean_string_value(&cap[val_cap_idx]).as_str()) {
                    Ok(d) => d,
                    Err(e) => return Err(e)?
                }),
                _ => {
                    let key_val_string = String::from(&cap[full_cap_idx]);
                    if key_val_string.contains("facet.") {
                        match add_facet_to_action(&mut act, key_val_string, line.clone(), line_nr) {
                            Ok(_) => continue,
                            Err(e) => return Err(e)?,
                        }
                    } else {
                        act.properties.push(Property{key: clean_string_value(&cap[key_cap_idx]), value: clean_string_value(&cap[val_cap_idx])});
                    }
                }
            }
        }
    }

    act.payload = Some(p);

    Ok(act)
}

fn parse_dir_action(line: String, line_nr: usize) -> Result<Dir> {
    let mut act = Dir::default();
    let regex_set = RegexSet::new(&[
        r#"([^ ]+)=([^"][^ ]+[^"])"#,
        r#"([^ ]+)="(.+)"#
    ])?;

    for pat in regex_set.matches(line.trim_start()).into_iter().map(|match_idx| &regex_set.patterns()[match_idx]) {
        let regex = Regex::new(&pat)?;

        for cap in regex.captures_iter(line.clone().trim_start()) {
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
                        match add_facet_to_action(&mut act, key_val_string, line.clone(), line_nr) {
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

fn parse_attr_action(line: String) -> Result<Attr> {
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

    let mut properties = HashMap::new();
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

        properties.insert(String::from(cap[1].trim()), Property {
            key: String::from(cap[1].trim()),
            value: String::from(cap[2].trim()),
        });
    }

    for cap in optionals_regex_no_quotes.captures_iter(line.trim_start()) {
        if cap[1].trim().starts_with("name") || cap[1].trim().starts_with("value") {
            continue;
        }

        properties.insert(String::from(cap[1].trim()), Property {
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
