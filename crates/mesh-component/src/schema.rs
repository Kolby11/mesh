/// Schema block — typed settings definitions for components.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The schema block parsed from a component's `<schema>` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaBlock {
    #[serde(flatten)]
    pub fields: HashMap<String, SchemaFieldDef>,
}

/// A single field in the settings schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaFieldDef {
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub default: Option<toml::Value>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub values: Option<Vec<String>>,
}
