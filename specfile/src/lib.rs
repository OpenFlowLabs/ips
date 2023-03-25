pub mod macros;

use anyhow::Result;
use pest::Parser;
use pest_derive::Parser;
use std::collections::HashMap;

#[derive(Parser)]
#[grammar = "specfile.pest"]
struct SpecFileParser;

#[derive(Default, Debug)]
pub struct SpecFile {
    pub name: String,
    pub version: String,
    pub release: String,
    pub summary: String,
    pub license: String,
    pub sources: Vec<String>,
    pub variables: HashMap<String, String>,
    pub description: String,
    pub prep_script: String,
    pub build_script: String,
    pub install_script: String,
    pub files: Vec<String>,
    pub changelog: String,
}

enum KnownVariableControl {
    Name,
    Version,
    Release,
    Summary,
    License,
    None,
}

fn append_newline_string(s: &str, section_line: i32) -> String {
    if section_line == 0 {
        s.to_owned()
    } else {
        "\n".to_owned() + s
    }
}

pub fn parse(file_contents: String) -> Result<SpecFile> {
    let pairs = SpecFileParser::parse(Rule::file, &file_contents)?;
    let mut spec = SpecFile::default();

    for pair in pairs {
        // A pair can be converted to an iterator of the tokens which make it up:
        match pair.as_rule() {
            Rule::variable => {
                let mut var_control = KnownVariableControl::None;
                let mut var_name_tmp = String::new();
                for variable_rule in pair.clone().into_inner() {
                    match variable_rule.as_rule() {
                        Rule::variable_name => match variable_rule.as_str() {
                            "Name" => var_control = KnownVariableControl::Name,
                            "Version" => var_control = KnownVariableControl::Version,
                            "Release" => var_control = KnownVariableControl::Release,
                            "Summary" => var_control = KnownVariableControl::Summary,
                            "License" => var_control = KnownVariableControl::License,
                            _ => {
                                var_control = {
                                    var_name_tmp = variable_rule.as_str().to_string();
                                    KnownVariableControl::None
                                }
                            }
                        },
                        Rule::variable_text => match var_control {
                            KnownVariableControl::Name => {
                                spec.name = variable_rule.as_str().to_string()
                            }
                            KnownVariableControl::Version => {
                                spec.version = variable_rule.as_str().to_string()
                            }
                            KnownVariableControl::Release => {
                                spec.release = variable_rule.as_str().to_string()
                            }
                            KnownVariableControl::Summary => {
                                spec.summary = variable_rule.as_str().to_string()
                            }
                            KnownVariableControl::License => {
                                spec.license = variable_rule.as_str().to_string()
                            }
                            KnownVariableControl::None => {
                                spec.variables.insert(
                                    var_name_tmp.clone(),
                                    variable_rule.as_str().to_string(),
                                );
                            }
                        },
                        _ => (),
                    }
                }
            }
            Rule::section => {
                let mut section_name_tmp = String::new();
                let mut section_line = 0;
                for section_rule in pair.clone().into_inner() {
                    match section_rule.as_rule() {
                        Rule::section_name => section_name_tmp = section_rule.as_str().to_string(),
                        Rule::section_line => {
                            for line_or_comment in section_rule.into_inner() {
                                if line_or_comment.as_rule() == Rule::section_text {
                                    match section_name_tmp.as_str() {
                                        "description" => {
                                            spec.description.push_str(
                                                append_newline_string(
                                                    line_or_comment.as_str(),
                                                    section_line,
                                                )
                                                .as_str(),
                                            );
                                            section_line += 1
                                        }
                                        "prep" => {
                                            spec.prep_script.push_str(
                                                append_newline_string(
                                                    line_or_comment.as_str(),
                                                    section_line,
                                                )
                                                .as_str(),
                                            );
                                            section_line += 1
                                        }
                                        "build" => {
                                            spec.build_script.push_str(
                                                append_newline_string(
                                                    line_or_comment.as_str(),
                                                    section_line,
                                                )
                                                .as_str(),
                                            );
                                            section_line += 1
                                        }
                                        "files" => spec
                                            .files
                                            .push(line_or_comment.as_str().trim_end().to_string()),
                                        "install" => {
                                            spec.install_script.push_str(
                                                append_newline_string(
                                                    line_or_comment.as_str(),
                                                    section_line,
                                                )
                                                .as_str(),
                                            );
                                            section_line += 1
                                        }
                                        "changelog" => {
                                            spec.changelog.push_str(
                                                append_newline_string(
                                                    line_or_comment.as_str(),
                                                    section_line,
                                                )
                                                .as_str(),
                                            );
                                            section_line += 1
                                        }
                                        _ => panic!(
                                            "Unknown Section: {:?}",
                                            line_or_comment.as_rule()
                                        ),
                                    }
                                }
                            }
                        }
                        _ => panic!(
                            "Rule not known please update the code: {:?}",
                            section_rule.as_rule()
                        ),
                    }
                }
            }
            Rule::EOI => (),
            _ => panic!(
                "Rule not known please update the code: {:?}",
                pair.as_rule()
            ),
        }
    }

    Ok(spec)
}

#[cfg(test)]
mod tests {
    use crate::parse;
    use std::fs;

    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn test_parse() {
        let contents = fs::read_to_string("src/test_data/simple.spec");
        match contents {
            Ok(file) => {
                let spec = parse(file);
                assert!(spec.is_ok(), "parsing error {:?}", spec)
            }
            Err(e) => panic!("io error: {:}", e),
        }
    }
}
