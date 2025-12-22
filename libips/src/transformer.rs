//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

use regex::Regex;
use std::collections::HashMap;

use miette::Diagnostic;
use thiserror::Error;

use crate::actions::{Facet, Manifest, Property, Transform};

// Programmatic AST for transform instructions
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransformTarget {
    Attr,
    File,
    Dir,
    Link,
    License,
    Dependency,
    User,
    Group,
    Driver,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchType {
    Key,
    Value,
    Path,
    Facet,
    Any,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Add,
    Default,
    Delete,
    Drop,
    Edit,
    Emit,
    Set,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformRule {
    pub target: TransformTarget,
    pub match_type: MatchType,
    pub pattern: Option<String>,
    pub op: Operation,
    pub value: Option<String>,
    pub attribute: Option<String>,
    pub emit_action: Option<String>,
    pub extra: std::collections::HashMap<String, String>,
}

impl TransformRule {
    pub fn new(target: TransformTarget, op: Operation) -> Self {
        Self {
            target,
            match_type: MatchType::Any,
            pattern: None,
            op,
            value: None,
            attribute: None,
            emit_action: None,
            extra: HashMap::new(),
        }
    }
    pub fn with_match_type(mut self, mt: MatchType) -> Self {
        self.match_type = mt;
        self
    }
    pub fn with_pattern(mut self, pat: impl Into<String>) -> Self {
        self.pattern = Some(pat.into());
        self
    }
    pub fn with_value(mut self, val: impl Into<String>) -> Self {
        self.value = Some(val.into());
        self
    }
    pub fn with_attribute(mut self, attr: impl Into<String>) -> Self {
        self.attribute = Some(attr.into());
        self
    }
    pub fn with_emit_action(mut self, act: impl Into<String>) -> Self {
        self.emit_action = Some(act.into());
        self
    }
}

fn target_to_str(t: &TransformTarget) -> &'static str {
    match t {
        TransformTarget::Attr => "attr",
        TransformTarget::File => "file",
        TransformTarget::Dir => "dir",
        TransformTarget::Link => "link",
        TransformTarget::License => "license",
        TransformTarget::Dependency => "dependency",
        TransformTarget::User => "user",
        TransformTarget::Group => "group",
        TransformTarget::Driver => "driver",
    }
}

fn str_to_target(s: &str) -> Option<TransformTarget> {
    Some(match s {
        "attr" => TransformTarget::Attr,
        "file" => TransformTarget::File,
        "dir" => TransformTarget::Dir,
        "link" => TransformTarget::Link,
        "license" => TransformTarget::License,
        "dependency" => TransformTarget::Dependency,
        "user" => TransformTarget::User,
        "group" => TransformTarget::Group,
        "driver" => TransformTarget::Driver,
        _ => return None,
    })
}

fn mt_to_str(mt: &MatchType) -> &'static str {
    match mt {
        MatchType::Key => "key",
        MatchType::Value => "value",
        MatchType::Path => "path",
        MatchType::Facet => "facet",
        MatchType::Any => "",
    }
}

fn str_to_mt(s: &str) -> Option<MatchType> {
    Some(match s {
        "key" => MatchType::Key,
        "value" => MatchType::Value,
        "path" => MatchType::Path,
        "facet" => MatchType::Facet,
        "" => MatchType::Any,
        _ => return None,
    })
}

fn op_to_str(op: &Operation) -> &'static str {
    match op {
        Operation::Add => "add",
        Operation::Default => "default",
        Operation::Delete => "delete",
        Operation::Drop => "drop",
        Operation::Edit => "edit",
        Operation::Emit => "emit",
        Operation::Set => "set",
    }
}

fn str_to_op(s: &str) -> Option<Operation> {
    Some(match s {
        "add" => Operation::Add,
        "default" => Operation::Default,
        "delete" => Operation::Delete,
        "drop" => Operation::Drop,
        "edit" => Operation::Edit,
        "emit" => Operation::Emit,
        "set" => Operation::Set,
        _ => return None,
    })
}

impl From<TransformRule> for Transform {
    fn from(r: TransformRule) -> Self {
        let mut t = Transform::default();
        t.transform_type = target_to_str(&r.target).to_string();
        t.match_type = mt_to_str(&r.match_type).to_string();
        if let Some(p) = r.pattern {
            t.pattern = p;
        }
        t.operation = op_to_str(&r.op).to_string();
        if let Some(v) = r.value {
            t.value = v;
        }
        let mut props = HashMap::new();
        if let Some(a) = r.attribute {
            props.insert(
                "attribute".to_string(),
                Property {
                    key: "attribute".to_string(),
                    value: a,
                },
            );
        }
        if let Some(e) = r.emit_action {
            props.insert(
                "emit_action".to_string(),
                Property {
                    key: "emit_action".to_string(),
                    value: e,
                },
            );
        }
        for (k, v) in r.extra {
            props.insert(k.clone(), Property { key: k, value: v });
        }
        t.properties = props;
        t
    }
}

impl std::convert::TryFrom<Transform> for TransformRule {
    type Error = TransformError;
    fn try_from(t: Transform) -> Result<Self> {
        let target = str_to_target(&t.transform_type).ok_or_else(|| {
            TransformError(format!("unknown transform_type: {}", t.transform_type))
        })?;
        let match_type = str_to_mt(&t.match_type).unwrap_or_else(|| {
            // Default based on target when empty
            match target {
                TransformTarget::Attr => MatchType::Key,
                TransformTarget::File | TransformTarget::Dir | TransformTarget::Link => {
                    MatchType::Path
                }
                _ => MatchType::Any,
            }
        });
        let op = str_to_op(&t.operation)
            .ok_or_else(|| TransformError(format!("unknown operation: {}", t.operation)))?;
        let attribute = t.properties.get("attribute").map(|p| p.value.clone());
        let emit_action = t.properties.get("emit_action").map(|p| p.value.clone());
        let mut extra = HashMap::new();
        for (k, p) in &t.properties {
            if k != "attribute" && k != "emit_action" {
                extra.insert(k.clone(), p.value.clone());
            }
        }
        Ok(TransformRule {
            target,
            match_type,
            pattern: if t.pattern.is_empty() {
                None
            } else {
                Some(t.pattern)
            },
            op,
            value: if t.value.is_empty() {
                None
            } else {
                Some(t.value)
            },
            attribute,
            emit_action,
            extra,
        })
    }
}

/// Parse rules as AST
pub fn parse_rules_ast(text: &str) -> Result<Vec<TransformRule>> {
    let ts = parse_rules(text)?;
    let mut out = Vec::with_capacity(ts.len());
    for t in ts {
        out.push(TransformRule::try_from(t)?);
    }
    Ok(out)
}

#[derive(Debug, Error, Diagnostic)]
#[error("transformer error: {0}")]
#[diagnostic(
    code(ips::transformer_error),
    help("Check the transformer rules format and inputs")
)]
pub struct TransformError(String);

pub type Result<T> = std::result::Result<T, TransformError>;

/// Parse textual transform rules from a simple line-oriented format.
/// Supported syntaxes:
/// 1) Plain:  `transform key=value key=value ...` where keys include:
///    - type, match_type, pattern, operation, value, attribute, emit_action
/// 2) Legacy: `<transform ACTION key=value ... -> ACTION_TEXT>`
///    We will parse key=value pairs; ACTION is mapped to `type` if not provided.
///    ACTION_TEXT is attached as `emit_action`.
pub fn parse_rules(text: &str) -> Result<Vec<Transform>> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("transform ") {
            out.push(parse_plain_transform_line(line)?);
        } else if line.starts_with("<transform ") && line.ends_with('>') {
            out.push(parse_legacy_transform_line(line)?);
        } else {
            // ignore unknown lines to be permissive
        }
    }
    Ok(out)
}

/// Apply a set of transform rules onto a manifest.
///
/// Convention for Transform fields:
/// - transform_type: target kind ("attr", "file", "dir", "link", "license", "dependency")
/// - match_type: what to match within the action. Supported:
///     - "key": for attr, matches Attr.key using regex in pattern
///     - "value": for attr, matches any Attr.values using regex in pattern
///     - "path": for file/dir/link: matches path field using regex in pattern
///     - "facet": for file facet name; requires property `attribute` with facet name, pattern matches facet value
/// - pattern: regex (unanchored)
/// - operation: one of add, default, delete, drop, edit, emit, set
/// - value: operation value or replacement string
/// - properties:
///     - attribute: name of attribute/facet to operate on (for attr and facet operations)
///     - emit_action: full action line to emit when operation=="emit"
pub fn apply(manifest: &mut Manifest, rules: &[Transform]) -> Result<()> {
    for rule in rules {
        let re = Regex::new(&rule.pattern)
            .map_err(|e| TransformError(format!("invalid regex '{}': {}", rule.pattern, e)))?;
        match rule.transform_type.as_str() {
            "attr" => apply_on_attrs(manifest, &re, rule)?,
            "file" => apply_on_files(manifest, &re, rule)?,
            "dir" => apply_on_dirs(manifest, &re, rule)?,
            "link" => apply_on_links(manifest, &re, rule)?,
            "license" => apply_on_licenses(manifest, &re, rule)?,
            "dependency" => apply_on_dependencies(manifest, &re, rule)?,
            "group" | "user" | "driver" => { /* not implemented */ }
            other => return Err(TransformError(format!("unknown transform_type: {}", other))),
        }
    }
    Ok(())
}

fn strip_quotes(s: &str) -> String {
    let t = s.trim();
    if (t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\'')) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

fn tokenize_kv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut quote_char: char = '"';
    for c in line.chars() {
        match c {
            '"' | '\'' => {
                if in_quotes && c == quote_char {
                    in_quotes = false;
                    cur.push(c);
                } else if !in_quotes {
                    in_quotes = true;
                    quote_char = c;
                    cur.push(c);
                } else {
                    cur.push(c);
                }
            }
            ' ' | '\t' if !in_quotes => {
                if !cur.is_empty() {
                    out.push(cur.clone());
                    cur.clear();
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_plain_transform_line(line: &str) -> Result<Transform> {
    // line starts with "transform "; parse key=value tokens
    let rest = line.trim_start_matches("transform ").trim();
    let tokens = tokenize_kv(rest);
    let mut t = Transform::default();
    for tok in tokens {
        if let Some(eq) = tok.find('=') {
            let (k, v) = (&tok[..eq], &tok[eq + 1..]);
            let key = k.trim();
            let val = strip_quotes(v);
            match key {
                "type" => t.transform_type = val,
                "match_type" => t.match_type = val,
                "pattern" => t.pattern = val,
                "operation" => t.operation = val,
                "value" => t.value = val,
                "attribute" | "emit_action" => {
                    t.properties.insert(
                        key.to_string(),
                        Property {
                            key: key.to_string(),
                            value: val,
                        },
                    );
                }
                _ => {
                    t.properties.insert(
                        key.to_string(),
                        Property {
                            key: key.to_string(),
                            value: val,
                        },
                    );
                }
            }
        }
    }
    if t.transform_type.is_empty() {
        return Err(TransformError("missing type".into()));
    }
    if t.operation.is_empty() {
        return Err(TransformError("missing operation".into()));
    }
    if t.pattern.is_empty() && t.operation != "emit" {
        return Err(TransformError("missing pattern".into()));
    }
    if t.match_type.is_empty() {
        t.match_type = match t.transform_type.as_str() {
            "attr" => "key".into(),
            "file" | "dir" | "link" => "path".into(),
            _ => "".into(),
        };
    }
    Ok(t)
}

fn map_action_to_type(action: &str) -> String {
    match action {
        "set" => "attr".into(),
        "file" => "file".into(),
        "dir" => "dir".into(),
        "hardlink" | "link" => "link".into(),
        "license" => "license".into(),
        "depend" => "dependency".into(),
        "user" => "user".into(),
        "group" => "group".into(),
        "driver" => "driver".into(),
        _ => action.to_string(),
    }
}

fn parse_legacy_transform_line(line: &str) -> Result<Transform> {
    // <transform ACTION key=value ... -> ACTION_TEXT>
    let inner = line
        .trim_start_matches("<transform ")
        .trim_end_matches('>')
        .trim();
    let (left, right) = inner
        .rsplit_once("->")
        .ok_or_else(|| TransformError("invalid legacy transform: missing '->'".into()))?;
    let action_text = right.trim();
    let mut iter = left.split_whitespace();
    let action = iter
        .next()
        .ok_or_else(|| TransformError("legacy transform missing action".into()))?;
    let rest = iter.collect::<Vec<_>>().join(" ");
    let mut t = Transform::default();
    t.transform_type = map_action_to_type(action);
    for tok in tokenize_kv(&rest) {
        if let Some(eq) = tok.find('=') {
            let key = &tok[..eq];
            let val = strip_quotes(&tok[eq + 1..]);
            match key {
                "type" => t.transform_type = val,
                "match_type" => t.match_type = val,
                "pattern" => t.pattern = val,
                "operation" => t.operation = val,
                "value" => t.value = val,
                _ => {
                    t.properties.insert(
                        key.to_string(),
                        Property {
                            key: key.to_string(),
                            value: val,
                        },
                    );
                }
            }
        }
    }
    // attach emit_action if present
    if !action_text.is_empty() {
        t.properties.insert(
            "emit_action".to_string(),
            Property {
                key: "emit_action".to_string(),
                value: action_text.to_string(),
            },
        );
    }
    if t.match_type.is_empty() {
        t.match_type = match t.transform_type.as_str() {
            "attr" => "key".into(),
            "file" | "dir" | "link" => "path".into(),
            _ => "".into(),
        };
    }
    Ok(t)
}

fn prop<'a>(rule: &'a Transform, key: &str) -> Option<&'a str> {
    rule.properties.get(key).map(|p| p.value.as_str())
}

fn map_backrefs(s: &str) -> String {
    // Convert \1, \12 style backrefs to Rust regex ${1}, ${12} style to avoid ambiguity
    let mut out = String::with_capacity(s.len() + 8);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            // Collect consecutive digits following the backslash
            let mut digits = String::new();
            while let Some(&d) = chars.peek() {
                if d.is_ascii_digit() {
                    digits.push(d);
                    chars.next();
                } else {
                    break;
                }
            }
            if !digits.is_empty() {
                out.push_str("${");
                out.push_str(&digits);
                out.push('}');
                continue;
            } else {
                // Not a backref, keep the backslash
                out.push('\\');
                continue;
            }
        }
        out.push(c);
    }
    out
}

fn apply_on_attrs(manifest: &mut Manifest, re: &Regex, rule: &Transform) -> Result<()> {
    let attr_name = prop(rule, "attribute");
    let mut to_drop: Vec<usize> = Vec::new();
    let mut pending_emits: Vec<String> = Vec::new();

    for (idx, a) in manifest.attributes.iter_mut().enumerate() {
        let matches = match rule.match_type.as_str() {
            "key" => re.is_match(&a.key),
            "value" => a.values.iter().any(|v| re.is_match(v)),
            _ => {
                if let Some(target) = attr_name {
                    // match_type unspecified or custom: match attribute name equals target
                    a.key == target
                        && (re.is_match(&a.key) || a.values.iter().any(|v| re.is_match(v)))
                } else {
                    re.is_match(&a.key) || a.values.iter().any(|v| re.is_match(v))
                }
            }
        };
        if !matches {
            continue;
        }
        match rule.operation.as_str() {
            "add" => {
                if let Some(val) = Some(rule.value.as_str()).filter(|s| !s.is_empty()) {
                    a.values.push(val.to_string());
                }
            }
            "default" => {
                if a.values.is_empty() {
                    if !rule.value.is_empty() {
                        a.values.push(rule.value.clone());
                    }
                }
            }
            "delete" => {
                // delete values matching regex (unanchored)
                a.values.retain(|v| !re.is_match(v));
            }
            "drop" => {
                to_drop.push(idx);
            }
            "edit" => {
                let rep = map_backrefs(rule.value.as_str());
                for v in &mut a.values {
                    let new = re.replace(v, rep.as_str()).to_string();
                    *v = new;
                }
            }
            "set" => {
                a.values.clear();
                a.values.push(rule.value.clone());
            }
            "emit" => {
                // defer emit until after loop to avoid nested mutable borrow
                if let Some(line) = prop(rule, "emit_action") {
                    pending_emits.push(line.to_string());
                } else {
                    return Err(TransformError(
                        "emit operation on attr requires 'emit_action' property".into(),
                    ));
                }
            }
            other => return Err(TransformError(format!("unknown operation: {}", other))),
        }
    }
    // Drop in reverse order
    for idx in to_drop.into_iter().rev() {
        if idx < manifest.attributes.len() {
            manifest.attributes.remove(idx);
        }
    }
    // Now process deferred emits
    for line in pending_emits {
        emit_action_into_manifest(manifest, &line)?;
    }
    Ok(())
}

fn apply_on_files(manifest: &mut Manifest, re: &Regex, rule: &Transform) -> Result<()> {
    let attr = prop(rule, "attribute");
    let mut to_drop: Vec<usize> = Vec::new();
    let mut pending_emits: Vec<String> = Vec::new();

    for (idx, f) in manifest.files.iter_mut().enumerate() {
        let matches = match rule.match_type.as_str() {
            "path" => re.is_match(&f.path),
            "facet" => {
                if let Some(name) = attr {
                    match_facet(&f.facets, name, re)
                } else {
                    false
                }
            }
            _ => re.is_match(&f.path),
        };
        if !matches {
            continue;
        }

        match rule.operation.as_str() {
            "drop" => to_drop.push(idx),
            "emit" => {
                if let Some(line) = prop(rule, "emit_action") {
                    pending_emits.push(line.to_string());
                } else {
                    return Err(TransformError(
                        "emit operation requires 'emit_action'".into(),
                    ));
                }
            }
            op => {
                // operations on facets
                if let Some(name) = attr {
                    apply_facet_op(&mut f.facets, name, re, op, &rule.value)?;
                } else {
                    // fallback: edit path via set/edit (not altering stored payload); minimal implementation
                    match op {
                        "set" => {
                            f.path = rule.value.clone();
                        }
                        "edit" => {
                            let rep = map_backrefs(rule.value.as_str());
                            f.path = re.replace(&f.path, rep.as_str()).to_string();
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    for idx in to_drop.into_iter().rev() {
        if idx < manifest.files.len() {
            manifest.files.remove(idx);
        }
    }
    // process deferred emits after mutation
    for line in pending_emits {
        emit_action_into_manifest(manifest, &line)?;
    }
    Ok(())
}

fn apply_on_dirs(manifest: &mut Manifest, re: &Regex, rule: &Transform) -> Result<()> {
    // Only support drop on directories by path for now (minimal)
    if rule.operation == "drop" {
        manifest.directories.retain(|d| !re.is_match(&d.path));
        Ok(())
    } else {
        Ok(())
    }
}

fn apply_on_links(manifest: &mut Manifest, re: &Regex, rule: &Transform) -> Result<()> {
    if rule.operation == "drop" {
        manifest.links.retain(|l| !re.is_match(&l.path));
    }
    Ok(())
}

fn apply_on_licenses(manifest: &mut Manifest, re: &Regex, rule: &Transform) -> Result<()> {
    if rule.operation == "drop" {
        manifest.licenses.retain(|l| !re.is_match(&l.payload));
    }
    Ok(())
}

fn apply_on_dependencies(manifest: &mut Manifest, re: &Regex, rule: &Transform) -> Result<()> {
    if rule.operation == "drop" {
        manifest.dependencies.retain(|d| {
            let fmri_str = d.fmri.as_ref().map(|f| f.to_string()).unwrap_or_default();
            !re.is_match(&fmri_str)
        });
    }
    Ok(())
}

fn match_facet(facets: &HashMap<String, Facet>, name: &str, re: &Regex) -> bool {
    facets
        .get(name)
        .map(|f| re.is_match(&f.value))
        .unwrap_or(false)
}

fn apply_facet_op(
    facets: &mut HashMap<String, Facet>,
    name: &str,
    re: &Regex,
    op: &str,
    val: &str,
) -> Result<()> {
    match op {
        "add" => {
            facets.insert(
                name.to_string(),
                Facet {
                    name: name.to_string(),
                    value: val.to_string(),
                },
            );
        }
        "default" => {
            facets.entry(name.to_string()).or_insert(Facet {
                name: name.to_string(),
                value: val.to_string(),
            });
        }
        "delete" => {
            if let Some(f) = facets.get(name) {
                if re.is_match(&f.value) {
                    facets.remove(name);
                }
            }
        }
        "edit" => {
            if let Some(f) = facets.get_mut(name) {
                let rep = map_backrefs(val);
                let new = re.replace(&f.value, rep.as_str()).to_string();
                f.value = new;
            }
        }
        "set" => {
            if let Some(f) = facets.get_mut(name) {
                f.value = val.to_string();
            } else {
                facets.insert(
                    name.to_string(),
                    Facet {
                        name: name.to_string(),
                        value: val.to_string(),
                    },
                );
            }
        }
        other => {
            return Err(TransformError(format!(
                "unsupported facet operation: {}",
                other
            )));
        }
    }
    Ok(())
}

fn emit_action_into_manifest(manifest: &mut Manifest, action_line: &str) -> Result<()> {
    let m = Manifest::parse_string(format!("{}\n", action_line))
        .map_err(|e| TransformError(e.to_string()))?;
    // merge m into manifest
    manifest.attributes.extend(m.attributes);
    manifest.directories.extend(m.directories);
    manifest.files.extend(m.files);
    manifest.dependencies.extend(m.dependencies);
    manifest.licenses.extend(m.licenses);
    manifest.links.extend(m.links);
    manifest.users.extend(m.users);
    manifest.groups.extend(m.groups);
    manifest.drivers.extend(m.drivers);
    manifest.legacies.extend(m.legacies);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::{Attr, File};

    #[test]
    fn add_default_set_attr() {
        let mut m = Manifest::new();
        m.attributes.push(Attr {
            key: "pkg.summary".into(),
            values: vec![],
            properties: Default::default(),
        });
        let rules = parse_rules("transform type=attr match_type=key pattern=pkg\\.summary operation=default value=Hello").unwrap();
        apply(&mut m, &rules).unwrap();
        assert_eq!(m.attributes[0].values, vec!["Hello".to_string()]);

        let rules = parse_rules(
            "transform type=attr match_type=key pattern=pkg\\.summary operation=add value=World",
        )
        .unwrap();
        apply(&mut m, &rules).unwrap();
        assert_eq!(
            m.attributes[0].values,
            vec!["Hello".to_string(), "World".to_string()]
        );

        let rules = parse_rules(
            "transform type=attr match_type=key pattern=pkg\\.summary operation=set value=Only",
        )
        .unwrap();
        apply(&mut m, &rules).unwrap();
        assert_eq!(m.attributes[0].values, vec!["Only".to_string()]);
    }

    #[test]
    fn drop_file_by_path_and_emit() {
        let mut m = Manifest::new();
        m.files.push(File {
            path: "bin/ls".into(),
            ..Default::default()
        });
        m.files.push(File {
            path: "bin/cp".into(),
            ..Default::default()
        });
        let rules = parse_rules("transform type=file match_type=path pattern=bin/ls operation=drop value=\ntransform type=file match_type=path pattern=bin/cp operation=emit value= attribute=ignored emit_action=\"set name=pkg.summary value=added\"").unwrap();
        apply(&mut m, &rules).unwrap();
        assert_eq!(m.files.len(), 1);
        assert_eq!(m.attributes.len(), 1);
    }

    #[test]
    fn edit_file_facet() {
        let mut m = Manifest::new();
        let mut f = File {
            path: "usr/bin/foo".into(),
            ..Default::default()
        };
        f.facets.insert(
            "variant.arch".into(),
            Facet {
                name: "variant.arch".into(),
                value: "i386".into(),
            },
        );
        m.files.push(f);
        let rules = parse_rules("transform type=file match_type=facet pattern=i386 operation=edit value=amd64 attribute=variant.arch").unwrap();
        apply(&mut m, &rules).unwrap();
        assert_eq!(m.files[0].facets["variant.arch"].value, "amd64");
    }

    #[test]
    fn backrefs_in_attr_edit() {
        // Set an attribute with a value like "name-123" and use two capture groups
        let mut m = Manifest::new();
        m.attributes.push(Attr {
            key: "some.attr".into(),
            values: vec!["abc-123".into()],
            properties: Default::default(),
        });
        let rules = parse_rules("transform type=attr match_type=value pattern=\"([a-z]+)-(\\d+)\" operation=edit value=\"\\1_\\2\"").unwrap();
        apply(&mut m, &rules).unwrap();
        assert_eq!(m.attributes[0].values[0], "abc_123");
    }

    #[test]
    fn parse_rules_ast_plain() {
        let rules = parse_rules_ast(
            "transform type=attr match_type=key pattern=pkg\\.summary operation=set value=Hello",
        )
        .unwrap();
        assert_eq!(rules.len(), 1);
        let r = &rules[0];
        match r.target {
            TransformTarget::Attr => {}
            _ => panic!("wrong target"),
        }
        match r.match_type {
            MatchType::Key => {}
            _ => panic!("wrong match type"),
        }
        assert_eq!(r.pattern.as_deref(), Some("pkg\\.summary"));
        match r.op {
            Operation::Set => {}
            _ => panic!("wrong op"),
        }
        assert_eq!(r.value.as_deref(), Some("Hello"));
    }

    #[test]
    fn programmatic_rule_apply_edit_with_backrefs() {
        // Prepare manifest
        let mut m = Manifest::new();
        m.attributes.push(Attr {
            key: "some.attr".into(),
            values: vec!["foo-123".into()],
            properties: Default::default(),
        });
        // Build TransformRule programmatically
        let rule = TransformRule::new(TransformTarget::Attr, Operation::Edit)
            .with_match_type(MatchType::Value)
            .with_pattern("([a-z]+)-(\\d+)")
            .with_value("\\1_\\2");
        // Convert to existing Transform and apply
        let t: Transform = rule.clone().into();
        apply(&mut m, &[t]).unwrap();
        assert_eq!(m.attributes[0].values[0], "foo_123");
        // Round-trip conversion back to AST
        let t2: Transform = rule.clone().into();
        let r2 = TransformRule::try_from(t2).unwrap();
        // target/op remain the same
        match r2.target {
            TransformTarget::Attr => {}
            _ => panic!("target changed"),
        }
        match r2.op {
            Operation::Edit => {}
            _ => panic!("op changed"),
        }
    }
}
