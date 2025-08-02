use diff::Diff;
use libips::actions::File;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone, Diff)]
#[diff(attr(
    #[derive(Debug, PartialEq)]
))]
struct Manifest {
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    files: HashMap<String, File>,
}

fn main() {
    let base = Manifest {
        files: HashMap::from([
            (
                "0dh5".to_string(),
                File {
                    payload: None,
                    path: "var/file".to_string(),
                    group: "bin".to_string(),
                    owner: "root".to_string(),
                    mode: "0755".to_string(),
                    preserve: false,
                    overlay: false,
                    original_name: "".to_string(),
                    revert_tag: "".to_string(),
                    sys_attr: "".to_string(),
                    properties: vec![],
                    facets: Default::default(),
                },
            ),
            (
                "12ds3".to_string(),
                File {
                    payload: None,
                    path: "var/file1".to_string(),
                    group: "bin".to_string(),
                    owner: "root".to_string(),
                    mode: "0755".to_string(),
                    preserve: false,
                    overlay: false,
                    original_name: "".to_string(),
                    revert_tag: "".to_string(),
                    sys_attr: "".to_string(),
                    properties: vec![],
                    facets: Default::default(),
                },
            ),
            (
                "654".to_string(),
                File {
                    payload: None,
                    path: "var/file1".to_string(),
                    group: "bin".to_string(),
                    owner: "root".to_string(),
                    mode: "0755".to_string(),
                    preserve: false,
                    overlay: false,
                    original_name: "".to_string(),
                    revert_tag: "".to_string(),
                    sys_attr: "".to_string(),
                    properties: vec![],
                    facets: Default::default(),
                },
            ),
        ]),
    };

    let new_set = Manifest {
        files: HashMap::from([
            (
                "0dh5".to_string(),
                File {
                    payload: None,
                    path: "var/file".to_string(),
                    group: "bin".to_string(),
                    owner: "root".to_string(),
                    mode: "0755".to_string(),
                    preserve: false,
                    overlay: false,
                    original_name: "".to_string(),
                    revert_tag: "".to_string(),
                    sys_attr: "".to_string(),
                    properties: vec![],
                    facets: Default::default(),
                },
            ),
            (
                "654".to_string(),
                File {
                    payload: None,
                    path: "var/file1".to_string(),
                    group: "bin".to_string(),
                    owner: "root".to_string(),
                    mode: "0755".to_string(),
                    preserve: false,
                    overlay: false,
                    original_name: "".to_string(),
                    revert_tag: "".to_string(),
                    sys_attr: "".to_string(),
                    properties: vec![],
                    facets: Default::default(),
                },
            ),
        ]),
    };
    let d = base.diff(&new_set);
    println!("{:#?}", d);
}
