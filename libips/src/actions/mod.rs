//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

// Source https://docs.oracle.com/cd/E23824_01/html/E21796/pkg-5.html

use std::collections::{HashMap};
use std::fs::{read_to_string};
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
        let mut props = act.properties;
        if !act.payload_string.is_empty() {
            let p_str = split_property(act.payload_string);
            props.push(Property{
                key: p_str.0,
                value: p_str.1,
            })
        }
        for prop in props {
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
        let mut props = act.properties;
        if !act.payload_string.is_empty() {
            if act.payload_string.contains("/") {
                if act.payload_string.contains("=") {
                    let p_str = split_property(act.payload_string);
                    props.push(Property{
                        key: p_str.0,
                        value: p_str.1,
                    })
                } else {
                    file.properties.push(Property{
                        key: "original-path".to_string(),
                        value: act.payload_string.replace("\"", "")
                    });
                }
            } else {
                p.primary_identifier = Digest::from_str(&act.payload_string).unwrap();
            }
        }
        for prop in props {
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
                        file.properties.push(Property{
                            key: prop.key,
                            value: prop.value,
                        });
                    }
                }
            }
        }
        if p.primary_identifier.hash.is_empty() {
            file.payload = None;
        } else {
            file.payload = Some(p);
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
        let mut props = act.properties;
        if !act.payload_string.is_empty() {
            let p_str = split_property(act.payload_string);
            props.push(Property{
                key: p_str.0,
                value: p_str.1,
            })
        }
        for prop in props {
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
        let mut props = act.properties;
        if !act.payload_string.is_empty() {
            let p_str = split_property(act.payload_string);
            props.push(Property{
                key: p_str.0,
                value: p_str.1,
            })
        }
        for prop in props {
            match prop.key.as_str() {
                "name" => attr.key = prop.value,
                "value" => attr.values.push(prop.value),
                _ => {
                    attr.properties.insert(prop.key.clone(), Property{
                        key: prop.key,
                        value: prop.value,
                    });
                }
            }
        }
        attr
    }
}

#[derive(Eq, PartialEq, Debug, Default, Clone)]
pub struct License {
    pub payload: String,
    pub properties: HashMap<String, Property>,
}

impl From<Action> for License {
    fn from(act: Action) -> Self {
        let mut license = License::default();
        if !act.payload_string.is_empty() {
            license.payload = act.payload_string;
        }
        for prop in act.properties {
            match prop.key.as_str() {
                _ => {
                    license.properties.insert(prop.key.clone(), Property{
                        key: prop.key,
                        value: prop.value,
                    });
                }
            }
        }
        license
    }
}

#[derive(Eq, PartialEq, Debug, Default, Clone)]
pub struct Link {
    pub path: String,
    pub target: String,
    pub properties: HashMap<String, Property>,
}

impl From<Action> for Link {
    fn from(act: Action) -> Self {
        let mut link = Link::default();
        let mut props = act.properties;
        if !act.payload_string.is_empty() {
            let p_str = split_property(act.payload_string);
            props.push(Property{
                key: p_str.0,
                value: p_str.1,
            })
        }
        for prop in props {
            match prop.key.as_str() {
                "path" => link.path = prop.value,
                "target" => link.target = prop.value,
                _ => {
                    link.properties.insert(prop.key.clone(), Property{
                        key: prop.key,
                        value: prop.value,
                    });
                }
            }
        }
        link
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
    pub licenses: Vec<License>,
    pub links: Vec<Link>
}

impl Manifest {
    pub fn new() -> Manifest {
        return Manifest {
            attributes: Vec::new(),
            directories: Vec::new(),
            files: Vec::new(),
            dependencies: Vec::new(),
            licenses: Vec::new(),
            links: Vec::new(),
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
                todo!()
            }
            ActionKind::Group => {
                todo!()
            }
            ActionKind::Driver => {
                todo!()
            }
            ActionKind::License => {
                self.licenses.push(act.into());
            }
            ActionKind::Link => {
                self.links.push(act.into());
            }
            ActionKind::Legacy => {
                todo!()
            }
            ActionKind::Transform => {
                todo!()
            }
            ActionKind::Unknown{action} => {
                panic!("action {:?} not known", action)
            },
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
                                                        let str_val: String =  prop.as_str().clone().into();
                                                        property.value = str_val
                                                            .replace("\"", "")
                                                            .replace("\\", "");
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
                            Rule::comment_string => (),
                            Rule::transform => (),
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

fn split_property(property_string: String) -> (String, String) {
    match property_string.find("=") {
        Some(_) => {
            let v :Vec <_> = property_string.split("=").collect();
            (
                String::from(v[0]),
                String::from(v[1])
                    .replace("\"", "")
                    .replace("\\", "")
            )
        },
        None => (property_string.clone(), String::new())
    }
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
