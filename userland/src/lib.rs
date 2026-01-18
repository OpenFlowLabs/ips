mod component;
pub mod repology;

use anyhow::{Context, Result, anyhow};
use lazy_static::lazy_static;
use pest::Parser;
use pest::iterators::Pairs;
use pest_derive::Parser;
use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::fs::{canonicalize, read_to_string};
use std::io::Error as IOError;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub use component::*;

#[derive(Parser)]
#[grammar = "makefile.pest"]
struct MakefileParser;

#[derive(Debug, Default, Clone)]
pub struct Makefile {
    path: PathBuf,
    variables: HashMap<String, MakefileVariable>,
    includes: Vec<String>,
    // targets: HashMap<String, String>,
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct MakefileVariable {
    pub key: String,
    pub values: Vec<String>,
    pub mode: VariableMode,
}

#[derive(Debug, PartialEq, Clone, Default)]
pub enum VariableMode {
    Add,
    #[default]
    Set,
}

#[derive(Error, Debug)]
pub enum ParserError {
    #[error("cannot parse {file}: {reason}")]
    MakefileReadError {
        file: PathBuf,
        reason: anyhow::Error,
    },
    #[error("could not find include {0}")]
    IncludeNotFound(String, #[source] IOError),
}

impl Makefile {
    pub fn parse_single_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = read_to_string(&path).context(format!(
            "cannot read {0} to string",
            path.as_ref().display()
        ))?;
        let mut m =
            parse_string(content).context(format!("cannot parse {0}", path.as_ref().display()))?;
        m.path = path.as_ref().into();
        Ok(m)
    }

    pub fn parse_string(content: String) -> Result<Self> {
        parse_string(content)
    }

    pub fn parse_all(&self) -> Result<Self> {
        let includes = self.parse_included_makefiles()?;
        let mut final_makefile = Self::default();

        for incl in includes {
            if incl.has_includes() {
                let final_include = incl.parse_all()?;
                final_makefile.merge(&final_include)?;
            } else {
                final_makefile.merge(&incl)?;
            }
        }

        final_makefile.merge(self)?;
        Ok(final_makefile)
    }

    pub fn merge(&mut self, other: &Self) -> Result<()> {
        for (key, var) in other.variables.clone() {
            match var.mode {
                VariableMode::Add => {
                    if let Some(v) = self.variables.get(&key) {
                        let mut new_variable = v.clone();
                        for val in var.values {
                            new_variable.values.push(val);
                        }
                        new_variable.mode = var.mode;
                        self.variables.insert(key, new_variable);
                    } else {
                        self.variables.insert(key, var);
                    }
                }
                VariableMode::Set => {
                    self.variables.insert(key, var);
                }
            }
        }

        Ok(())
    }

    pub fn get(&self, var_name: &str) -> Option<String> {
        if let Some(var) = self.variables.get(var_name) {
            let vars_resolved = self.resolve_nested_variables(var);
            Some(vars_to_string(&vars_resolved))
        } else {
            None
        }
    }

    pub fn get_includes(&self) -> Option<Vec<String>> {
        if !self.includes.is_empty() {
            Some(self.includes.clone())
        } else {
            None
        }
    }

    pub fn has_includes(&self) -> bool {
        !self.includes.is_empty()
    }

    pub fn parse_included_makefiles(&self) -> Result<Vec<Self>> {
        if let Some(includes) = self.get_includes() {
            let mut included_makefiles: Vec<Makefile> = Vec::new();

            let dir_of_makefile = self.path.parent();
            if let Some(d) = dir_of_makefile {
                env::set_current_dir(d)?;
            } else {
                env::set_current_dir("/")?;
            };

            for incl in includes {
                let incl_path = canonicalize(&incl)
                    .map_err(|err| anyhow!(ParserError::IncludeNotFound(incl, err)))?;
                let m = Self::parse_single_file(incl_path)?;
                included_makefiles.push(m);
            }

            Ok(included_makefiles)
        } else {
            Ok(Vec::new())
        }
    }

    fn resolve_nested_variables(&self, var: &MakefileVariable) -> Vec<String> {
        // Make a mutable copy of the variables so we can replace nested variables with their final strings
        let mut vars_copy = var.values.clone();

        // Logic to resolve all the nested Variables when we access them.
        for (i, maybe_nested_var) in var.values.iter().enumerate() {
            lazy_static! {
                static ref VARRE: Regex = Regex::new(r"(?P<var_name>\$\(.+?\))").unwrap();
            }
            for captures in VARRE.captures_iter(maybe_nested_var) {
                if let Some(nested_var) = captures.name("var_name") {
                    let nested_var_name = nested_var.as_str().replace("$(", "").replace(')', "");
                    if let Some(resolved_nested_var) = self.get(&nested_var_name) {
                        let mut new_string = vars_copy[i].clone();
                        new_string =
                            new_string.replacen(nested_var.as_str(), &resolved_nested_var, 1);
                        vars_copy[i] = new_string;
                    }
                }
            }
        }
        vars_copy
    }

    pub fn get_first_value_of_variable_by_name(&self, var_name: &str) -> Option<String> {
        if let Some(var) = self.variables.get(var_name) {
            let vars_resolved = self.resolve_nested_variables(var);
            Some(vars_resolved.first().unwrap().clone())
        } else {
            None
        }
    }
}

fn vars_to_string(vars: &[String]) -> String {
    if vars.is_empty() {
        String::new()
    } else if vars.len() == 1 {
        vars[0].clone()
    } else {
        vars.join("\n")
    }
}

fn parse_string(content: String) -> Result<Makefile> {
    let mut m = Makefile::default();

    let makefile_pair = MakefileParser::parse(Rule::makefile, &content)?;

    for p in makefile_pair {
        match p.as_rule() {
            Rule::makefile => {
                parse_makefile(p.into_inner(), &mut m)?;
            }
            _ => panic!(
                "unexpected rule {:?} inside pair expected manifest",
                p.as_rule()
            ),
        }
    }

    Ok(m)
}

fn parse_makefile(pairs: Pairs<crate::Rule>, m: &mut Makefile) -> Result<()> {
    for p in pairs {
        match p.as_rule() {
            Rule::variable => {
                parse_variable(p.into_inner(), m)?;
            }
            Rule::comment_string => (),
            Rule::include => {
                parse_include(p.into_inner(), m)?;
            }
            Rule::target => (),
            Rule::define => {
                parse_define(p.into_inner(), m)?;
            }
            Rule::EOI => (),
            _ => panic!(
                "unexpected rule {:?} inside makefile rule expected variable, define, comment, NEWLINE, include, target",
                p.as_rule()
            ),
        }
    }

    Ok(())
}

fn parse_include(include_pair: Pairs<crate::Rule>, m: &mut Makefile) -> Result<()> {
    for p in include_pair {
        match p.as_rule() {
            Rule::variable_value => {
                m.includes.push(p.as_str().to_string());
            }
            _ => panic!(
                "unexpected rule {:?} inside include rule expected variable_value",
                p.as_rule()
            ),
        }
    }
    Ok(())
}

fn parse_define(define_pair: Pairs<crate::Rule>, m: &mut Makefile) -> Result<()> {
    let mut var = (String::new(), MakefileVariable::default());
    for p in define_pair {
        match p.as_rule() {
            Rule::variable_name => {
                var.0 = p.as_str().to_string();
            }
            Rule::define_value => {
                var.1.values.push(p.as_str().to_string());
            }
            _ => panic!(
                "unexpected rule {:?} inside define rule expected variable_name, define_value",
                p.as_rule()
            ),
        }
    }
    m.variables.insert(var.0, var.1);

    Ok(())
}

fn parse_variable(variable_pair: Pairs<crate::Rule>, m: &mut Makefile) -> Result<()> {
    let mut var = (String::new(), MakefileVariable::default());
    for p in variable_pair {
        match p.as_rule() {
            Rule::variable_name => {
                var.0 = p.as_str().to_string();
            }
            Rule::variable_set => var.1.mode = VariableMode::Set,
            Rule::variable_add => var.1.mode = VariableMode::Add,
            Rule::variable_value => match var.1.mode {
                VariableMode::Add => {
                    if m.variables.contains_key(&var.0) {
                        var.1 = m.variables.get(&var.0).unwrap().clone()
                    }
                    var.1.values.push(p.as_str().to_string());
                }
                VariableMode::Set => {
                    var.1.values.push(p.as_str().to_string());
                }
            },
            _ => panic!(
                "unexpected rule {:?} inside makefile rule expected variable_name, variable_set, variable_add, variable_value",
                p.as_rule()
            ),
        }
    }
    m.variables.insert(var.0, var.1);

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
