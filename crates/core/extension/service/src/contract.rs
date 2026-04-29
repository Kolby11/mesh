use semver::{Version, VersionReq};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceContract {
    pub interface: String,
    pub version: Version,
    pub file_path: PathBuf,
    pub methods: Vec<InterfaceMethod>,
    pub events: Vec<InterfaceEvent>,
    pub types: HashMap<String, InterfaceTypeDef>,
    pub capabilities: ContractCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceMethod {
    pub name: String,
    pub args: Vec<InterfaceArgument>,
    pub returns: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceArgument {
    pub name: String,
    pub arg_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceEvent {
    pub name: String,
    pub payload: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceTypeDef {
    pub name: String,
    pub fields: Vec<InterfaceArgument>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractCapabilities {
    pub required: Vec<String>,
    pub optional: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("failed to read interface contract {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse interface contract {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("invalid interface version '{value}' in {path}")]
    InvalidVersion { path: PathBuf, value: String },
}

pub fn load_interface_contract(
    plugin_dir: &Path,
    interface_name: &str,
    interface_version: &str,
    relative_file: &str,
) -> Result<InterfaceContract, ContractError> {
    let path = plugin_dir.join(relative_file);
    let content = std::fs::read_to_string(&path).map_err(|source| ContractError::Io {
        path: path.clone(),
        source,
    })?;
    let parsed: ContractToml = toml::from_str(&content).map_err(|source| ContractError::Parse {
        path: path.clone(),
        source,
    })?;

    Ok(InterfaceContract {
        interface: interface_name.to_string(),
        version: parse_contract_version(interface_version).ok_or_else(|| {
            ContractError::InvalidVersion {
                path: path.clone(),
                value: interface_version.to_string(),
            }
        })?,
        file_path: path,
        methods: parsed
            .methods
            .into_iter()
            .map(|method| InterfaceMethod {
                name: method.name,
                args: method
                    .args
                    .into_iter()
                    .map(|arg| InterfaceArgument {
                        name: arg.name,
                        arg_type: arg.arg_type,
                    })
                    .collect(),
                returns: method.returns,
            })
            .collect(),
        events: parsed
            .events
            .into_iter()
            .map(|event| InterfaceEvent {
                name: event.name,
                payload: event.payload,
            })
            .collect(),
        types: parsed
            .types
            .into_iter()
            .map(|(name, def)| {
                let fields = def
                    .fields
                    .into_iter()
                    .map(|field| InterfaceArgument {
                        name: field.name,
                        arg_type: field.arg_type,
                    })
                    .collect();
                (name.clone(), InterfaceTypeDef { name, fields })
            })
            .collect(),
        capabilities: ContractCapabilities {
            required: parsed.capabilities.required,
            optional: parsed.capabilities.optional,
        },
    })
}

pub fn parse_contract_version(value: &str) -> Option<Version> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    Version::parse(trimmed)
        .ok()
        .or_else(|| Version::parse(&format!("{trimmed}.0")).ok())
}

pub fn parse_version_req(value: &str) -> Option<VersionReq> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed
        .chars()
        .any(|ch| matches!(ch, '<' | '>' | '=' | '^' | '~' | ',' | '*'))
    {
        return VersionReq::parse(trimmed).ok();
    }

    parse_contract_version(trimmed)
        .and_then(|version| VersionReq::parse(&format!("={version}")).ok())
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ContractToml {
    #[serde(default)]
    methods: Vec<ContractMethodToml>,
    #[serde(default)]
    events: Vec<ContractEventToml>,
    #[serde(default)]
    types: HashMap<String, ContractTypeToml>,
    #[serde(default)]
    capabilities: ContractCapabilitiesToml,
}

#[derive(Debug, Clone, Deserialize)]
struct ContractMethodToml {
    name: String,
    #[serde(default)]
    args: Vec<ContractFieldToml>,
    #[serde(default)]
    returns: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ContractEventToml {
    name: String,
    #[serde(default)]
    payload: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ContractTypeToml {
    #[serde(default)]
    fields: Vec<ContractFieldToml>,
}

#[derive(Debug, Clone, Deserialize)]
struct ContractFieldToml {
    name: String,
    #[serde(rename = "type")]
    arg_type: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ContractCapabilitiesToml {
    #[serde(default)]
    required: Vec<String>,
    #[serde(default)]
    optional: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_semver_contract_version() {
        let version = parse_contract_version("1.0").unwrap();
        assert_eq!(version.to_string(), "1.0.0");
    }

    #[test]
    fn parses_exact_request_from_short_version() {
        let req = parse_version_req("1.0").unwrap();
        assert!(req.matches(&Version::parse("1.0.0").unwrap()));
        assert!(!req.matches(&Version::parse("1.1.0").unwrap()));
    }

    #[test]
    fn loads_contract_toml_shape() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("interface.toml");
        std::fs::write(
            &file,
            r#"
[[methods]]
name = "sensors"
returns = "[Sensor]"

[[methods]]
name = "read"
args = [{ name = "sensor_id", type = "string" }]
returns = "float"

[[events]]
name = "TemperatureChanged"
payload = "{ sensor_id: string, celsius: float }"

[types.Sensor]
fields = [
  { name = "id", type = "string" },
  { name = "name", type = "string" }
]

[capabilities]
required = ["service.thermal.read"]
"#,
        )
        .unwrap();

        let contract =
            load_interface_contract(dir.path(), "alice.thermal", "1.0", "interface.toml").unwrap();

        assert_eq!(contract.interface, "alice.thermal");
        assert_eq!(contract.version.to_string(), "1.0.0");
        assert_eq!(contract.methods.len(), 2);
        assert_eq!(contract.events[0].name, "TemperatureChanged");
        assert_eq!(
            contract.capabilities.required,
            vec!["service.thermal.read".to_string()]
        );
        assert!(contract.types.contains_key("Sensor"));
    }
}
