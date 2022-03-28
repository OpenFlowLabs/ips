pub mod repology;

extern crate pest;
extern crate maplit;

#[macro_use]
extern crate pest_derive;


use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fs::read_to_string;
use pest::iterators::{Pairs};
use std::path::{Path, PathBuf};
use pest::Parser;
use thiserror::Error;
use lazy_static::lazy_static;
use regex::Regex;

#[derive(Parser)]
#[grammar = "makefile.pest"]
struct MakefileParser;

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Makefile {
    pub variables: HashMap<String, Vec<String>>,
    // pub includes: Vec<String>,
    // pub targets: HashMap<String, String>,
}

#[derive(Error, Debug)]
pub enum ParserError {
    #[error("cannot parse {file}: {reason}")]
    MakefileReadError {
        file: PathBuf,
        reason: anyhow::Error,
    }
}

impl Makefile {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = read_to_string(&path)?;
        parse_string(content).map_err(|err|
            anyhow!(ParserError::MakefileReadError{file: path.as_ref().to_path_buf(), reason: anyhow!(err)})
        )
    }

    pub fn parse_string(content: String) -> Result<Self> {
        parse_string(content)
    }

    pub fn get(&self, var_name: &str) -> Option<String> {
        if let Some(var) = self.variables.get(var_name) {
            let vars_resolved = self.resolve_nested_variables(var);
            Some(vars_to_string(&vars_resolved))
        } else {
            None
        }
    }

    fn resolve_nested_variables(&self, var: &Vec<String>) -> Vec<String> {
        // Make a mutable copy of the variables so we can replace nested variables with their final strings
        let mut vars_copy = var.clone();

        // Logic to resolve all the nested Variables when we access them.
        for (i, maybe_nested_var) in var.iter().enumerate() {
            lazy_static! {
                    static ref VARRE: Regex = Regex::new(r"(?P<var_name>\$\(.+?\))").unwrap();
                }
            for captures in VARRE.captures_iter(maybe_nested_var) {
                if let Some(nested_var) = captures.name("var_name") {
                    let nested_var_name = nested_var.as_str().replace("$(", "").replace(")", "");
                    if let Some(resolved_nested_var) = self.get(&nested_var_name) {
                        let mut new_string = vars_copy[i].clone();
                        new_string = new_string.replacen(nested_var.as_str(), &resolved_nested_var, 1);
                        vars_copy[i] = new_string;
                    }
                }
            }
        }
        vars_copy
    }

    pub fn get_first(&self, var_name: &str) -> Option<String> {
        if let Some(var) = self.variables.get(var_name) {
            let vars_resolved = self.resolve_nested_variables(var);
            Some(vars_resolved.first().unwrap().clone())
        } else {
            None
        }
    }
}

fn vars_to_string(vars: &Vec<String>) -> String {
    if vars.len() == 0 {
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
            _ => panic!("unexpected rule {:?} inside pair expected manifest", p.as_rule()),
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
            Rule::include => (),
            Rule::target => (),
            Rule::define => {
                parse_define(p.into_inner(), m)?;
            }
            Rule::EOI => (),
            _ => panic!("unexpected rule {:?} inside makefile rule expected variable, define, comment, NEWLINE, include, target", p.as_rule()),
        }
    }

    Ok(())
}

fn parse_define(define_pair: Pairs<crate::Rule>, m: &mut Makefile) -> Result<()> {
    let mut var = (String::new(), Vec::<String>::new());
    for p in define_pair {
        match p.as_rule() {
            Rule::variable_name => {
                var.0 = p.as_str().to_string();
            }
            Rule::define_value => {
                var.1.push(p.as_str().to_string());
            }
            _ => panic!("unexpected rule {:?} inside makefile rule expected variable_name, define_value", p.as_rule()),
        }
    }
    m.variables.insert(var.0, var.1);

    Ok(())
}

fn parse_variable(variable_pair: Pairs<crate::Rule>, m: &mut Makefile) -> Result<()> {
    let mut var = (String::new(), Vec::<String>::new());
    for p in variable_pair {
        match p.as_rule() {
            Rule::variable_name => {
                var.0 = p.as_str().to_string();
            }
            Rule::variable_set => (),
            Rule::variable_add => {
                if m.variables.contains_key(&var.0) {
                    var.1 = m.variables.get(&var.0).unwrap().clone()
                }
            }
            Rule::variable_value => {
                var.1.push(p.as_str().to_string());
            }
            _ => panic!("unexpected rule {:?} inside makefile rule expected variable_name, variable_set, variable_add, variable_value", p.as_rule()),
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
