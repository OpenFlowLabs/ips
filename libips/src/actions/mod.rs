//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

// Source https://docs.oracle.com/cd/E23824_01/html/E21796/pkg-5.html

use crate::digest::Digest;
use crate::fmri::Fmri;
use crate::payload::{Payload, PayloadError};
use diff::Diff;
use miette::Diagnostic;
use pest::Parser;
use pest_derive::Parser;
use serde::{Deserialize, Serialize};
use std::clone::Clone;
use std::collections::{HashMap, HashSet};
use std::fs::read_to_string;
use std::path::Path;
use std::result::Result as StdResult;
use std::str::FromStr;
use thiserror::Error;

type Result<T> = StdResult<T, ActionError>;

#[derive(Debug, Error, Diagnostic)]
pub enum ActionError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    PayloadError(#[from] PayloadError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    FileError(#[from] FileError),

    #[error("value {0} is not a boolean")]
    #[diagnostic(
        code(ips::action_error::invalid_boolean),
        help("Boolean values must be 'true', 'false', 't', or 'f'.")
    )]
    NotBooleanValue(String),

    #[error(transparent)]
    #[diagnostic(code(ips::action_error::io))]
    IOError(#[from] std::io::Error),

    #[error(transparent)]
    #[diagnostic(code(ips::action_error::parser))]
    ParserError(#[from] pest::error::Error<Rule>),
}

pub trait FacetedAction {
    // Add a facet to the action if the facet is already present, the function returns false.
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
    pub fn new(kind: ActionKind) -> Action {
        Action {
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
        self.facets.insert(facet.name.clone(), facet).is_none()
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        self.facets.remove(&facet.name) == Some(facet)
    }
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
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
            props.push(Property {
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
        self.facets.insert(facet.name.clone(), facet).is_none()
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        self.facets.remove(&facet.name) == Some(facet)
    }
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
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
            }
            None => return Err(FileError::FilePathIsNoStringError)?,
        }

        //TODO group owner mode

        Ok(f)
    }

    pub fn get_original_path(&self) -> Option<String> {
        for p in &self.properties {
            if p.key.as_str() == "original-path" {
                return Some(p.value.clone());
            }
        }
        None
    }
}

impl From<Action> for File {
    fn from(act: Action) -> Self {
        let mut file = File::default();
        let mut p = act.payload.clone();
        let mut props = act.properties;
        if !act.payload_string.is_empty() {
            if act.payload_string.contains('/') {
                if act.payload_string.contains('=') {
                    let p_str = split_property(act.payload_string);
                    props.push(Property {
                        key: p_str.0,
                        value: p_str.1,
                    })
                } else {
                    file.properties.push(Property {
                        key: "original-path".to_string(),
                        value: act.payload_string.replace(['\"', '\\'], ""),
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
                "overlay" => {
                    file.overlay = match string_to_bool(&prop.value) {
                        Ok(b) => b,
                        _ => false,
                    }
                }
                "preserve" => {
                    file.preserve = match string_to_bool(&prop.value) {
                        Ok(b) => b,
                        _ => false,
                    }
                }
                "chash" | "pkg.content-hash" => p
                    .additional_identifiers
                    .push(Digest::from_str(&prop.value).unwrap()),
                _ => {
                    if is_facet(prop.key.clone()) {
                        file.add_facet(Facet::from_key_value(prop.key, prop.value));
                    } else {
                        file.properties.push(Property {
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
        self.facets.insert(facet.name.clone(), facet).is_none()
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        self.facets.remove(&facet.name) == Some(facet)
    }
}

#[derive(Debug, Error, Diagnostic)]
pub enum FileError {
    #[error("file path is not a string")]
    #[diagnostic(
        code(ips::action_error::file::invalid_path),
        help("The file path must be convertible to a valid string.")
    )]
    FilePathIsNoStringError,
}

//TODO implement multiple FMRI for require-any
#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Dependency {
    pub fmri: Option<Fmri>,      // FMRI of the dependency
    pub dependency_type: String, //TODO make enum
    pub predicate: Option<Fmri>, // FMRI for conditional dependencies
    pub root_image: String,      //TODO make boolean
    pub optional: Vec<Property>,
    pub facets: HashMap<String, Facet>,
}

impl From<Action> for Dependency {
    fn from(act: Action) -> Self {
        let mut dep = Dependency::default();
        let mut props = act.properties;
        if !act.payload_string.is_empty() {
            let p_str = split_property(act.payload_string);
            props.push(Property {
                key: p_str.0,
                value: p_str.1,
            })
        }
        for prop in props {
            match prop.key.as_str() {
                "fmri" => match Fmri::parse(&prop.value) {
                    Ok(fmri) => dep.fmri = Some(fmri),
                    Err(err) => {
                        eprintln!("Error parsing FMRI '{}': {}", prop.value, err);
                        dep.fmri = None;
                    }
                },
                "type" => dep.dependency_type = prop.value,
                "predicate" => match Fmri::parse(&prop.value) {
                    Ok(fmri) => dep.predicate = Some(fmri),
                    Err(err) => {
                        eprintln!("Error parsing predicate FMRI '{}': {}", prop.value, err);
                        dep.predicate = None;
                    }
                },
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
        self.facets.insert(facet.name.clone(), facet).is_none()
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        self.facets.remove(&facet.name) == Some(facet)
    }
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Facet {
    pub name: String,
    pub value: String,
}

impl Facet {
    fn from_key_value(key: String, value: String) -> Facet {
        Facet {
            name: get_facet_key(key),
            value,
        }
    }
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
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
            props.push(Property {
                key: p_str.0,
                value: p_str.1,
            })
        }
        for prop in props {
            match prop.key.as_str() {
                "name" => attr.key = prop.value,
                "value" => attr.values.push(prop.value),
                _ => {
                    attr.properties.insert(
                        prop.key.clone(),
                        Property {
                            key: prop.key,
                            value: prop.value,
                        },
                    );
                }
            }
        }
        attr
    }
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
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
            let key = prop.key.as_str();
            {
                license.properties.insert(
                    key.to_owned(),
                    Property {
                        key: prop.key,
                        value: prop.value,
                    },
                );
            }
        }
        license
    }
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
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
            props.push(Property {
                key: p_str.0,
                value: p_str.1,
            })
        }
        for prop in props {
            match prop.key.as_str() {
                "path" => link.path = prop.value,
                "target" => link.target = prop.value,
                _ => {
                    link.properties.insert(
                        prop.key.clone(),
                        Property {
                            key: prop.key,
                            value: prop.value,
                        },
                    );
                }
            }
        }
        link
    }
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct User {
    pub username: String,
    pub uid: String,
    pub group: String,
    pub home_dir: String,
    pub login_shell: String,
    pub password: String,
    pub services: HashSet<String>,
    pub gcos_field: String,
    pub properties: HashMap<String, Property>,
    pub facets: HashMap<String, Facet>,
}

impl From<Action> for User {
    fn from(act: Action) -> Self {
        let mut user = User::default();
        let mut props = act.properties;
        if !act.payload_string.is_empty() {
            let p_str = split_property(act.payload_string);
            props.push(Property {
                key: p_str.0,
                value: p_str.1,
            })
        }
        for prop in props {
            match prop.key.as_str() {
                "username" => user.username = prop.value,
                "uid" => user.uid = prop.value,
                "group" => user.group = prop.value,
                "home-dir" => user.home_dir = prop.value,
                "login-shell" => user.login_shell = prop.value,
                "password" => user.password = prop.value,
                "gcos-field" => user.gcos_field = prop.value,
                "ftpuser" => {
                    // Parse ftpuser property into services
                    match string_to_bool(&prop.value) {
                        // If it's a boolean value (backward compatibility)
                        Ok(true) => {
                            user.services.insert("ftp".to_string());
                        }
                        Ok(false) => {} // No services if false
                        // If the value not a boolean, treat as a comma-separated list of services
                        _ => {
                            for service in prop.value.split(',') {
                                let service = service.trim();
                                if !service.is_empty() {
                                    user.services.insert(service.to_string());
                                }
                            }
                        }
                    }
                }
                _ => {
                    if is_facet(prop.key.clone()) {
                        user.add_facet(Facet::from_key_value(prop.key, prop.value));
                    } else {
                        user.properties.insert(
                            prop.key.clone(),
                            Property {
                                key: prop.key,
                                value: prop.value,
                            },
                        );
                    }
                }
            }
        }
        user
    }
}

impl FacetedAction for User {
    fn add_facet(&mut self, facet: Facet) -> bool {
        self.facets.insert(facet.name.clone(), facet).is_none()
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        self.facets.remove(&facet.name) == Some(facet)
    }
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Group {
    pub groupname: String,
    pub gid: String,
    pub properties: HashMap<String, Property>,
    pub facets: HashMap<String, Facet>,
}

impl From<Action> for Group {
    fn from(act: Action) -> Self {
        let mut group = Group::default();
        let mut props = act.properties;
        if !act.payload_string.is_empty() {
            let p_str = split_property(act.payload_string);
            props.push(Property {
                key: p_str.0,
                value: p_str.1,
            })
        }
        for prop in props {
            match prop.key.as_str() {
                "groupname" => group.groupname = prop.value,
                "gid" => group.gid = prop.value,
                _ => {
                    if is_facet(prop.key.clone()) {
                        group.add_facet(Facet::from_key_value(prop.key, prop.value));
                    } else {
                        group.properties.insert(
                            prop.key.clone(),
                            Property {
                                key: prop.key,
                                value: prop.value,
                            },
                        );
                    }
                }
            }
        }
        group
    }
}

impl FacetedAction for Group {
    fn add_facet(&mut self, facet: Facet) -> bool {
        self.facets.insert(facet.name.clone(), facet).is_none()
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        self.facets.remove(&facet.name) == Some(facet)
    }
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Driver {
    pub name: String,
    pub class: String,
    pub perms: String,
    pub clone_perms: String,
    pub alias: String,
    pub properties: HashMap<String, Property>,
    pub facets: HashMap<String, Facet>,
}

impl From<Action> for Driver {
    fn from(act: Action) -> Self {
        let mut driver = Driver::default();
        let mut props = act.properties;
        if !act.payload_string.is_empty() {
            let p_str = split_property(act.payload_string);
            props.push(Property {
                key: p_str.0,
                value: p_str.1,
            })
        }
        for prop in props {
            match prop.key.as_str() {
                "name" => driver.name = prop.value,
                "class" => driver.class = prop.value,
                "perms" => driver.perms = prop.value,
                "clone_perms" => driver.clone_perms = prop.value,
                "alias" => driver.alias = prop.value,
                _ => {
                    if is_facet(prop.key.clone()) {
                        driver.add_facet(Facet::from_key_value(prop.key, prop.value));
                    } else {
                        driver.properties.insert(
                            prop.key.clone(),
                            Property {
                                key: prop.key,
                                value: prop.value,
                            },
                        );
                    }
                }
            }
        }
        driver
    }
}

impl FacetedAction for Driver {
    fn add_facet(&mut self, facet: Facet) -> bool {
        self.facets.insert(facet.name.clone(), facet).is_none()
    }

    fn remove_facet(&mut self, facet: Facet) -> bool {
        self.facets.remove(&facet.name) == Some(facet)
    }
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Legacy {
    pub arch: String,
    pub category: String,
    pub desc: String,
    pub hotline: String,
    pub name: String,
    pub pkg: String,
    pub vendor: String,
    pub version: String,
    pub properties: HashMap<String, Property>,
}

impl From<Action> for Legacy {
    fn from(act: Action) -> Self {
        let mut legacy = Legacy::default();
        let mut props = act.properties;
        if !act.payload_string.is_empty() {
            let p_str = split_property(act.payload_string);
            props.push(Property {
                key: p_str.0,
                value: p_str.1,
            })
        }
        for prop in props {
            match prop.key.as_str() {
                "arch" => legacy.arch = prop.value,
                "category" => legacy.category = prop.value,
                "desc" => legacy.desc = prop.value,
                "hotline" => legacy.hotline = prop.value,
                "name" => legacy.name = prop.value,
                "pkg" => legacy.pkg = prop.value,
                "vendor" => legacy.vendor = prop.value,
                "version" => legacy.version = prop.value,
                _ => {
                    legacy.properties.insert(
                        prop.key.clone(),
                        Property {
                            key: prop.key,
                            value: prop.value,
                        },
                    );
                }
            }
        }
        legacy
    }
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Transform {
    pub transform_type: String,
    pub pattern: String,
    pub match_type: String,
    pub operation: String,
    pub value: String,
    pub properties: HashMap<String, Property>,
}

impl From<Action> for Transform {
    fn from(act: Action) -> Self {
        let mut transform = Transform::default();
        let mut props = act.properties;
        if !act.payload_string.is_empty() {
            let p_str = split_property(act.payload_string);
            props.push(Property {
                key: p_str.0,
                value: p_str.1,
            })
        }
        for prop in props {
            match prop.key.as_str() {
                "type" => transform.transform_type = prop.value,
                "pattern" => transform.pattern = prop.value,
                "match_type" => transform.match_type = prop.value,
                "operation" => transform.operation = prop.value,
                "value" => transform.value = prop.value,
                _ => {
                    transform.properties.insert(
                        prop.key.clone(),
                        Property {
                            key: prop.key,
                            value: prop.value,
                        },
                    );
                }
            }
        }
        transform
    }
}

#[derive(Hash, Eq, PartialEq, Debug, Default, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Property {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize, Serialize, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
pub struct Manifest {
    pub attributes: Vec<Attr>,
    pub directories: Vec<Dir>,
    pub files: Vec<File>,
    pub dependencies: Vec<Dependency>,
    pub licenses: Vec<License>,
    pub links: Vec<Link>,
    pub users: Vec<User>,
    pub groups: Vec<Group>,
    pub drivers: Vec<Driver>,
    pub legacies: Vec<Legacy>,
    pub transforms: Vec<Transform>,
}

impl Manifest {
    pub fn new() -> Manifest {
        Manifest {
            attributes: Vec::new(),
            directories: Vec::new(),
            files: Vec::new(),
            dependencies: Vec::new(),
            licenses: Vec::new(),
            links: Vec::new(),
            users: Vec::new(),
            groups: Vec::new(),
            drivers: Vec::new(),
            legacies: Vec::new(),
            transforms: Vec::new(),
        }
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
                self.users.push(act.into());
            }
            ActionKind::Group => {
                self.groups.push(act.into());
            }
            ActionKind::Driver => {
                self.drivers.push(act.into());
            }
            ActionKind::License => {
                self.licenses.push(act.into());
            }
            ActionKind::Link => {
                self.links.push(act.into());
            }
            ActionKind::Legacy => {
                self.legacies.push(act.into());
            }
            ActionKind::Transform => {
                self.transforms.push(act.into());
            }
            ActionKind::Unknown { action } => {
                panic!("action {:?} not known", action)
            }
        }
    }

    pub fn parse_file<P: AsRef<Path>>(f: P) -> Result<Manifest> {
        let content = read_to_string(f)?;

        // Try to parse as JSON first
        match serde_json::from_str::<Manifest>(&content) {
            Ok(manifest) => Ok(manifest),
            Err(_) => {
                // If JSON parsing fails, fall back to string format
                Manifest::parse_string(content)
            }
        }
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
                                            act.payload_string = action.as_str().to_owned();
                                        }
                                        Rule::property => {
                                            let mut property = Property::default();
                                            for prop in action.clone().into_inner() {
                                                match prop.as_rule() {
                                                    Rule::property_name => {
                                                        property.key = prop.as_str().to_owned();
                                                    }
                                                    Rule::property_value => {
                                                        let str_val: String =  prop.as_str().to_owned();
                                                        property.value = str_val
                                                            .replace(['\"', '\\'], "");
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
                            Rule::transform => (),
                            _ => panic!(
                                "unexpected rule {:?} inside manifest expected action",
                                manifest.as_rule()
                            ),
                        }
                    }
                }
                Rule::WHITESPACE => (),
                _ => panic!(
                    "unexpected rule {:?} inside pair expected manifest",
                    p.as_rule()
                ),
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
    Unknown { action: String },
    Transform,
}

impl Default for ActionKind {
    fn default() -> Self {
        ActionKind::Unknown {
            action: String::new(),
        }
    }
}

//TODO Multierror and no failure for these cases
#[derive(Debug, Error, Diagnostic)]
pub enum ManifestError {
    #[error("unknown action {action:?} at line {line:?}")]
    #[diagnostic(
        code(ips::action_error::manifest::unknown_action),
        help("Check the action name and make sure it's one of the supported action types.")
    )]
    UnknownAction { line: usize, action: String },

    #[error("action string \"{action:?}\" at line {line:?} is invalid: {message:?}")]
    #[diagnostic(
        code(ips::action_error::manifest::invalid_action),
        help("Check the action format and fix the syntax error.")
    )]
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
    match act {
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
        _ => ActionKind::Unknown { action: act.into() },
    }
}

fn is_facet(s: String) -> bool {
    s.starts_with("facet.")
}

fn get_facet_key(facet_string: String) -> String {
    match facet_string.find('.') {
        Some(idx) => facet_string.clone().split_off(idx + 1),
        None => facet_string.clone(),
    }
}

fn split_property(property_string: String) -> (String, String) {
    match property_string.find('=') {
        Some(_) => {
            let v: Vec<_> = property_string.split('=').collect();
            (
                String::from(v[0]),
                String::from(v[1]).replace(['\"', '\\'], ""),
            )
        }
        None => (property_string.clone(), String::new()),
    }
}

fn string_to_bool(orig: &str) -> Result<bool> {
    match &String::from(orig).trim().to_lowercase()[..] {
        "true" => Ok(true),
        "false" => Ok(false),
        "t" => Ok(true),
        "f" => Ok(false),
        _ => Err(ActionError::NotBooleanValue(orig.to_owned())),
    }
}
