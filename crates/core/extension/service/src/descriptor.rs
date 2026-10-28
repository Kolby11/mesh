//! The language-neutral, serializable form of a compiled contract.
//!
//! Every binding generator reads a [`CompiledContract`]; tools outside this
//! workspace (a TypeScript or Go generator, devtools, a provider SDK in
//! another language) read this descriptor instead of re-parsing `module.json`
//! with their own rules. It carries only normalized data: type expressions
//! are structured, named types and policies are ordered deterministically,
//! and fingerprints are hex strings so JSON consumers without 64-bit integers
//! read them exactly.

use crate::contract::{
    BUILTIN_RESULT_TYPE, BaseType, CommandCoalescing, CompiledContract, TypeExpr,
};
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

/// Descriptor format revision. Incremented on any change a consumer must
/// handle; consumers refuse revisions they do not know.
pub const CONTRACT_DESCRIPTOR_REVISION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractDescriptor {
    pub descriptor: u32,
    pub interface: String,
    pub version: String,
    pub fingerprints: DescriptorFingerprints,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub types: BTreeMap<String, DescriptorType>,
    pub state: Vec<DescriptorStateField>,
    pub methods: Vec<DescriptorMethod>,
    pub events: Vec<DescriptorEvent>,
    pub features: BTreeMap<String, DescriptorFeature>,
    /// Capabilities required to read state; `None` is the compatibility
    /// fallback for contracts that declare no policy.
    pub read: Option<Vec<String>>,
    pub permissions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DescriptorFingerprints {
    pub schema: String,
    pub policy: String,
    pub behavior: String,
}

/// A type expression with its grammar resolved: `kind` is one of `string`,
/// `int`, `float`, `boolean`, `blob`, `object`, `any`, `result` (the builtin
/// `{ ok, error? }` record) or `named`, which sets `name`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DescriptorTypeRef {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub array: bool,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DescriptorField {
    pub name: String,
    #[serde(rename = "type")]
    pub type_ref: DescriptorTypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DescriptorType {
    pub fields: Vec<DescriptorField>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DescriptorStateField {
    pub name: String,
    #[serde(rename = "type")]
    pub type_ref: DescriptorTypeRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<serde_json::Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<serde_json::Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescriptorMethod {
    pub name: String,
    pub args: Vec<DescriptorField>,
    pub returns: Option<DescriptorTypeRef>,
    /// Coalescing key arguments; `None` when calls are never replaced.
    pub coalesce: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_binding: Option<DescriptorStateBinding>,
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescriptorStateBinding {
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_arg: Option<String>,
    pub toggle: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DescriptorEvent {
    pub name: String,
    pub payload: Vec<DescriptorField>,
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DescriptorFeature {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub members: Vec<String>,
}

impl DescriptorTypeRef {
    pub fn from_expr(expr: &TypeExpr) -> Self {
        let (kind, name) = match &expr.base {
            BaseType::String => ("string", None),
            BaseType::Int => ("int", None),
            BaseType::Float => ("float", None),
            BaseType::Boolean => ("boolean", None),
            BaseType::Blob => ("blob", None),
            BaseType::Object => ("object", None),
            BaseType::Any => ("any", None),
            BaseType::Named(name) if name == BUILTIN_RESULT_TYPE => ("result", None),
            BaseType::Named(name) => ("named", Some(name.clone())),
        };
        Self {
            kind,
            name,
            array: expr.array,
            optional: expr.optional,
        }
    }
}

impl CompiledContract {
    /// The language-neutral descriptor every generator and external tool reads.
    pub fn descriptor(&self) -> ContractDescriptor {
        let fields = |fields: &[crate::contract::CompiledField]| {
            fields
                .iter()
                .map(|field| DescriptorField {
                    name: field.name.clone(),
                    type_ref: DescriptorTypeRef::from_expr(&field.type_expr),
                })
                .collect::<Vec<_>>()
        };
        let feature_of = |member: &str| self.member_features.get(member).cloned();
        let policy =
            |policy: Option<&std::sync::Arc<[String]>>| policy.map(|values| values.to_vec());

        ContractDescriptor {
            descriptor: CONTRACT_DESCRIPTOR_REVISION,
            interface: self.interface.clone(),
            version: self.version.to_string(),
            fingerprints: DescriptorFingerprints {
                schema: format!("{:016x}", self.schema_fingerprint),
                policy: format!("{:016x}", self.policy_fingerprint),
                behavior: format!("{:016x}", self.behavior_fingerprint),
            },
            module: self.provenance.module.clone(),
            source: self.provenance.source.clone(),
            types: self
                .schemas
                .types
                .values()
                .map(|definition| {
                    (
                        definition.name.clone(),
                        DescriptorType {
                            fields: fields(&definition.fields),
                        },
                    )
                })
                .collect(),
            state: self
                .schemas
                .state_fields
                .iter()
                .map(|field| DescriptorStateField {
                    name: field.name.clone(),
                    type_ref: DescriptorTypeRef::from_expr(&field.type_expr),
                    description: field.description.clone(),
                    minimum: field.minimum.clone(),
                    maximum: field.maximum.clone(),
                    default: field.default.clone(),
                    feature: feature_of(&field.name),
                })
                .collect(),
            methods: self
                .schemas
                .methods
                .iter()
                .map(|method| DescriptorMethod {
                    name: method.name.clone(),
                    args: fields(&method.args),
                    returns: method.returns.as_ref().map(DescriptorTypeRef::from_expr),
                    coalesce: match &method.coalesce {
                        CommandCoalescing::Disabled => None,
                        CommandCoalescing::Latest { key } => Some(key.clone()),
                    },
                    state_binding: method.state_binding.as_ref().map(|binding| {
                        DescriptorStateBinding {
                            field: binding.field.clone(),
                            from_arg: binding.from_arg.clone(),
                            toggle: binding.toggle,
                        }
                    }),
                    capabilities: policy(self.operation_policy.methods.get(&method.name)),
                    feature: feature_of(&method.name),
                })
                .collect(),
            events: self
                .schemas
                .events
                .iter()
                .map(|event| DescriptorEvent {
                    name: event.name.clone(),
                    payload: fields(&event.payload),
                    capabilities: policy(self.operation_policy.events.get(&event.name)),
                    feature: feature_of(&event.name),
                })
                .collect(),
            features: self
                .features
                .values()
                .map(|feature| {
                    (
                        feature.name.clone(),
                        DescriptorFeature {
                            description: self
                                .capabilities
                                .features
                                .get(&feature.name)
                                .and_then(|declared| declared.description.clone()),
                            members: feature.members.to_vec(),
                        },
                    )
                })
                .collect(),
            read: policy(self.operation_policy.read.as_ref()),
            permissions: self.capabilities.permissions.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::contract::parse_compiled_contract;
    use serde_json::json;

    #[test]
    fn descriptor_resolves_types_policy_and_features() {
        let contract = parse_compiled_contract(
            "mesh.audio",
            "1.2",
            &json!({
                "state": {
                    "percent": { "type": "float", "minimum": 0, "maximum": 100 },
                    "apps": "App[]?"
                },
                "methods": {
                    "set_volume": {
                        "signature": "(percent: float) -> Result",
                        "coalesce": true
                    }
                },
                "events": { "VolumeChanged": { "level": "float" } },
                "types": { "App": { "id": "string" } },
                "capabilities": {
                    "read": ["audio.read"],
                    "methods": { "set_volume": ["audio.control"] }
                },
                "features": { "per-app-volume": { "members": ["apps"] } }
            }),
        )
        .unwrap();
        let descriptor = serde_json::to_value(contract.descriptor()).unwrap();

        assert_eq!(descriptor["descriptor"], 1);
        assert_eq!(descriptor["version"], "1.2.0");
        assert_eq!(
            descriptor["fingerprints"]["schema"],
            format!("{:016x}", contract.schema_fingerprint)
        );
        assert_eq!(
            descriptor["state"][0],
            json!({ "name": "apps",
                "type": { "kind": "named", "name": "App", "array": true, "optional": true },
                "feature": "per-app-volume" })
        );
        assert_eq!(
            descriptor["methods"][0]["returns"],
            json!({ "kind": "result", "array": false, "optional": false })
        );
        assert_eq!(descriptor["methods"][0]["coalesce"], json!([]));
        assert_eq!(
            descriptor["methods"][0]["capabilities"],
            json!(["audio.control"])
        );
        assert_eq!(descriptor["read"], json!(["audio.read"]));
        assert_eq!(
            descriptor["types"]["App"]["fields"][0]["type"]["kind"],
            "string"
        );
    }
}
