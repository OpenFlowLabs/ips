use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Default, Deserialize, Serialize)]
pub enum ImageProperty {
    String(String),
    Boolean(bool),
    #[default]
    None,
    Array(Vec<ImageProperty>),
    Integer(i32),
}
