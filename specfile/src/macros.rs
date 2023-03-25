use pest::Parser;
use pest_derive::Parser;
use std::collections::HashMap;
use thiserror::Error;
use anyhow::Result;

#[derive(Debug, Error)]
pub enum MacroParserError {
    #[error("macro does not exist: {macro_name}")]
    DoesNotExist {
        macro_name: String,
    }
}

#[derive(Parser)]
#[grammar = "macro.pest"]
struct InternalMacroParser;

#[derive(Default, Debug)]
pub struct MacroParser {
    pub macros: HashMap<String, String>
}

#[derive(Default, Debug)]
pub struct Macro {
    pub name: String,
    pub parameters: Vec<String>
}

impl MacroParser {
    pub fn parse(&self ,raw_string: String) -> Result<String> {
        let mut return_string = String::new();

        for (i, line) in raw_string.lines().enumerate() {
            let mut replaced_line = String::new();
            let pairs = InternalMacroParser::parse(Rule::file, line)?;

            for pair in pairs {
                for test_pair in pair.into_inner() {
                    match test_pair.as_rule() {
                        Rule::text_with_macros => {
                            for inner in test_pair.into_inner() {
                                match inner.as_rule() {
                                    Rule::spec_macro => {
                                        for macro_pair in inner.clone().into_inner() {
                                            match macro_pair.as_rule() {
                                                Rule::macro_name => {
                                                    replaced_line += self.get_variable(macro_pair.as_str())?;
                                                },
                                                Rule::macro_parameter => {
                                                    println!("macro parameter: {}", macro_pair.as_str())
                                                },
                                                _ => panic!(
                                                    "Unexpected macro match please update the code together with the peg grammar: {:?}",
                                                    macro_pair.as_rule()
                                                )
                                            }
                                        }
                                    }
                                    _ => panic!(
                                        "Unexpected inner match please update the code together with the peg grammar: {:?}",
                                        inner.as_rule()
                                    )
                                }
                            }
                        },
                        Rule::EOI => (),
                        Rule::text => {
                            replaced_line += test_pair.as_str();
                            replaced_line += " ";
                        },
                        _ => panic!(
                            "Unexpected match please update the code together with the peg grammar: {:?}",
                            test_pair.as_rule()
                        )
                    }
                }
            }
            replaced_line = replaced_line.trim_end().to_owned();

            if i == 0 {
                return_string += &replaced_line;
            } else {
                return_string += "\n";
                return_string += &replaced_line;
            }
        }

        Ok(return_string)
    }

    fn get_variable(&self, macro_name: &str) -> Result<&str> {
        if self.macros.contains_key(macro_name) {
            return Ok(self.macros[macro_name].as_str())
        }
        Err(MacroParserError::DoesNotExist {macro_name: macro_name.into()})?
    }
}

