pub mod repology;

extern crate pest;
extern crate maplit;

#[macro_use]
extern crate pest_derive;


use anyhow::Result;
use std::collections::HashMap;
use std::fs::read_to_string;
use pest::iterators::{Pairs};
use std::path::Path;
use pest::Parser;

#[derive(Parser)]
#[grammar = "makefile.pest"]
struct MakefileParser;

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Makefile {
    pub variables: HashMap<String, Vec<String>>,
    // pub includes: Vec<String>,
    // pub targets: HashMap<String, String>,
}

impl Makefile {
    pub fn parse_file(path: &Path) -> Result<Self> {
        let content = read_to_string(path)?;
        parse_string(content)
    }

    pub fn parse_string(content: String) -> Result<Self> {
        parse_string(content)
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
            Rule::EOI => (),
            _ => panic!("unexpected rule {:?} inside makefile rule expected variable, comment, NEWLINE, include, target", p.as_rule()),
        }
    }

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
