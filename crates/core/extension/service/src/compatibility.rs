//! Contract compatibility diffing.
//!
//! Interface contracts are **data**, so an update can decide whether a
//! candidate revision breaks a consumer without executing a line of module
//! code. That is what turns "fetch and hope" into a transaction with a
//! pre-commit refusal point.
//!
//! The direction matters: this asks *"can a consumer written against `locked`
//! still work against `candidate`?"*. Additions are safe; removals, renames,
//! and type changes are not.

use crate::contract::{InterfaceArgument, InterfaceContract};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompatibilityClass {
    /// Nothing a consumer relied on changed.
    Compatible,
    /// Purely additive: new state, methods, events, or optional arguments.
    Additive,
    /// A consumer written against the old contract can break.
    Breaking,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractChange {
    pub class: CompatibilityClass,
    /// Dotted path to what changed, e.g. `methods.set_volume.args.percent`.
    pub path: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractDiff {
    pub interface: String,
    pub changes: Vec<ContractChange>,
}

/// The two compatibility checks needed when an interface contract changes.
///
/// `consumer` asks whether consumers compiled against the locked contract can
/// use the candidate provider. `provider` reverses that question: whether a
/// provider compiled against the locked contract can serve consumers compiled
/// against the candidate contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BidirectionalContractDiff {
    pub consumer: ContractDiff,
    pub provider: ContractDiff,
}

/// Independent compatibility outcomes for the two sides of an interface
/// update. A change can be safe for existing consumers while unsafe for an
/// existing provider, or vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompatibilityClassification {
    pub consumer: CompatibilityClass,
    pub provider: CompatibilityClass,
}

impl BidirectionalContractDiff {
    pub fn classification(&self) -> CompatibilityClassification {
        CompatibilityClassification {
            consumer: self.consumer.class(),
            provider: self.provider.class(),
        }
    }
}

impl ContractDiff {
    pub fn class(&self) -> CompatibilityClass {
        self.changes
            .iter()
            .map(|change| change.class)
            .max()
            .unwrap_or(CompatibilityClass::Compatible)
    }

    pub fn is_breaking(&self) -> bool {
        self.class() == CompatibilityClass::Breaking
    }

    pub fn breaking_changes(&self) -> impl Iterator<Item = &ContractChange> {
        self.changes
            .iter()
            .filter(|change| change.class == CompatibilityClass::Breaking)
    }
}

/// Diff a locked contract against its candidate replacement.
pub fn diff_contracts(locked: &InterfaceContract, candidate: &InterfaceContract) -> ContractDiff {
    let mut changes = Vec::new();

    diff_identity(locked, candidate, &mut changes);
    diff_state_fields(locked, candidate, &mut changes);
    diff_methods(locked, candidate, &mut changes);
    diff_events(locked, candidate, &mut changes);
    diff_types(locked, candidate, &mut changes);
    diff_capabilities(locked, candidate, &mut changes);
    diff_feature_groups(locked, candidate, &mut changes);

    changes.sort_by(|left, right| {
        right
            .class
            .cmp(&left.class)
            .then_with(|| left.path.cmp(&right.path))
    });
    ContractDiff {
        interface: candidate.interface.clone(),
        changes,
    }
}

/// Diff a contract in both upgrade directions.
pub fn diff_contracts_bidirectional(
    locked: &InterfaceContract,
    candidate: &InterfaceContract,
) -> BidirectionalContractDiff {
    BidirectionalContractDiff {
        consumer: diff_contracts(locked, candidate),
        provider: diff_contracts(candidate, locked),
    }
}

fn diff_identity(
    locked: &InterfaceContract,
    candidate: &InterfaceContract,
    changes: &mut Vec<ContractChange>,
) {
    if locked.interface != candidate.interface {
        changes.push(ContractChange {
            class: CompatibilityClass::Breaking,
            path: "interface".to_string(),
            detail: format!(
                "contract identity changed from '{}' to '{}'",
                locked.interface, candidate.interface
            ),
        });
    }
    if locked.version != candidate.version {
        changes.push(ContractChange {
            class: CompatibilityClass::Breaking,
            path: "version".to_string(),
            detail: format!(
                "contract version changed from {} to {}",
                locked.version, candidate.version
            ),
        });
    }
}

fn diff_state_fields(
    locked: &InterfaceContract,
    candidate: &InterfaceContract,
    changes: &mut Vec<ContractChange>,
) {
    let candidate_fields: BTreeMap<&str, &str> = candidate
        .state_fields
        .iter()
        .map(|field| (field.name.as_str(), field.field_type.as_str()))
        .collect();

    for field in &locked.state_fields {
        match candidate_fields.get(field.name.as_str()) {
            None => {
                let optional = is_optional(&field.field_type);
                changes.push(ContractChange {
                    class: if optional {
                        CompatibilityClass::Additive
                    } else {
                        CompatibilityClass::Breaking
                    },
                    path: format!("state.{}", field.name),
                    detail: if optional {
                        format!("optional state field '{}' was removed", field.name)
                    } else {
                        format!(
                            "state field '{}' was removed; consumers reading it break",
                            field.name
                        )
                    },
                });
            }
            Some(candidate_type) if *candidate_type != field.field_type => {
                changes.push(ContractChange {
                    class: CompatibilityClass::Breaking,
                    path: format!("state.{}", field.name),
                    detail: format!(
                        "state field '{}' changed type from {} to {candidate_type}",
                        field.name, field.field_type
                    ),
                });
            }
            Some(_) => {}
        }
    }

    let locked_names: Vec<&str> = locked
        .state_fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    for field in &candidate.state_fields {
        if !locked_names.contains(&field.name.as_str()) {
            changes.push(ContractChange {
                class: CompatibilityClass::Additive,
                path: format!("state.{}", field.name),
                detail: format!("state field '{}' was added", field.name),
            });
        }
    }
}

fn diff_methods(
    locked: &InterfaceContract,
    candidate: &InterfaceContract,
    changes: &mut Vec<ContractChange>,
) {
    for method in &locked.methods {
        let Some(candidate_method) = candidate
            .methods
            .iter()
            .find(|other| other.name == method.name)
        else {
            changes.push(ContractChange {
                class: CompatibilityClass::Breaking,
                path: format!("methods.{}", method.name),
                detail: format!(
                    "method '{}' was removed; consumers calling it break",
                    method.name
                ),
            });
            continue;
        };

        if method.returns != candidate_method.returns {
            changes.push(ContractChange {
                class: CompatibilityClass::Breaking,
                path: format!("methods.{}.returns", method.name),
                detail: format!(
                    "method '{}' return type changed from {} to {}",
                    method.name,
                    method.returns.as_deref().unwrap_or("none"),
                    candidate_method.returns.as_deref().unwrap_or("none")
                ),
            });
        }
        if method.coalesce != candidate_method.coalesce {
            changes.push(ContractChange {
                class: CompatibilityClass::Breaking,
                path: format!("methods.{}.coalesce", method.name),
                detail: format!(
                    "method '{}' coalesce behavior changed from {} to {}",
                    method.name, method.coalesce, candidate_method.coalesce
                ),
            });
        }
        if method.state_binding != candidate_method.state_binding {
            changes.push(ContractChange {
                class: CompatibilityClass::Breaking,
                path: format!("methods.{}.stateBinding", method.name),
                detail: format!(
                    "method '{}' state binding changed from {:?} to {:?}",
                    method.name, method.state_binding, candidate_method.state_binding
                ),
            });
        }
        diff_arguments(
            &format!("methods.{}", method.name),
            &method.args,
            &candidate_method.args,
            changes,
        );
    }

    for method in &candidate.methods {
        if !locked.methods.iter().any(|other| other.name == method.name) {
            changes.push(ContractChange {
                class: CompatibilityClass::Additive,
                path: format!("methods.{}", method.name),
                detail: format!("method '{}' was added", method.name),
            });
        }
    }
}

fn diff_events(
    locked: &InterfaceContract,
    candidate: &InterfaceContract,
    changes: &mut Vec<ContractChange>,
) {
    for event in &locked.events {
        let Some(candidate_event) = candidate
            .events
            .iter()
            .find(|other| other.name == event.name)
        else {
            changes.push(ContractChange {
                class: CompatibilityClass::Breaking,
                path: format!("events.{}", event.name),
                detail: format!(
                    "event '{}' was removed; consumers subscribed to it break",
                    event.name
                ),
            });
            continue;
        };
        // A payload field a subscriber reads must keep its name and type.
        for field in &event.payload {
            match candidate_event
                .payload
                .iter()
                .find(|other| other.name == field.name)
            {
                None => {
                    let optional = is_optional(&field.arg_type);
                    changes.push(ContractChange {
                        class: if optional {
                            CompatibilityClass::Additive
                        } else {
                            CompatibilityClass::Breaking
                        },
                        path: format!("events.{}.{}", event.name, field.name),
                        detail: if optional {
                            format!(
                                "event '{}' no longer carries optional payload field '{}'",
                                event.name, field.name
                            )
                        } else {
                            format!(
                                "event '{}' no longer carries payload field '{}'",
                                event.name, field.name
                            )
                        },
                    });
                }
                Some(other) if other.arg_type != field.arg_type => {
                    changes.push(ContractChange {
                        class: CompatibilityClass::Breaking,
                        path: format!("events.{}.{}", event.name, field.name),
                        detail: format!(
                            "event '{}' payload field '{}' changed type from {} to {}",
                            event.name, field.name, field.arg_type, other.arg_type
                        ),
                    });
                }
                Some(_) => {}
            }
        }
        for field in &candidate_event.payload {
            if !event.payload.iter().any(|other| other.name == field.name) {
                changes.push(ContractChange {
                    class: CompatibilityClass::Additive,
                    path: format!("events.{}.{}", event.name, field.name),
                    detail: format!(
                        "event '{}' payload field '{}' was added",
                        event.name, field.name
                    ),
                });
            }
        }
    }

    for event in &candidate.events {
        if !locked.events.iter().any(|other| other.name == event.name) {
            changes.push(ContractChange {
                class: CompatibilityClass::Additive,
                path: format!("events.{}", event.name),
                detail: format!("event '{}' was added", event.name),
            });
        }
    }
}

fn diff_types(
    locked: &InterfaceContract,
    candidate: &InterfaceContract,
    changes: &mut Vec<ContractChange>,
) {
    for (name, definition) in &locked.types {
        let Some(candidate_definition) = candidate.types.get(name) else {
            changes.push(ContractChange {
                class: CompatibilityClass::Breaking,
                path: format!("types.{name}"),
                detail: format!("named type '{name}' was removed"),
            });
            continue;
        };
        diff_named_fields(
            &format!("types.{name}"),
            &definition.fields,
            &candidate_definition.fields,
            changes,
        );
    }

    for name in candidate.types.keys() {
        if !locked.types.contains_key(name) {
            changes.push(ContractChange {
                class: CompatibilityClass::Additive,
                path: format!("types.{name}"),
                detail: format!("named type '{name}' was added"),
            });
        }
    }
}

fn diff_named_fields(
    path: &str,
    locked: &[InterfaceArgument],
    candidate: &[InterfaceArgument],
    changes: &mut Vec<ContractChange>,
) {
    for field in locked {
        match candidate.iter().find(|other| other.name == field.name) {
            None => changes.push(ContractChange {
                class: if is_optional(&field.arg_type) {
                    CompatibilityClass::Additive
                } else {
                    CompatibilityClass::Breaking
                },
                path: format!("{path}.{}", field.name),
                detail: if is_optional(&field.arg_type) {
                    format!("optional named type field '{}' was removed", field.name)
                } else {
                    format!("named type field '{}' was removed", field.name)
                },
            }),
            Some(other) if other.arg_type != field.arg_type => changes.push(ContractChange {
                class: CompatibilityClass::Breaking,
                path: format!("{path}.{}", field.name),
                detail: format!(
                    "named type field '{}' changed type from {} to {}",
                    field.name, field.arg_type, other.arg_type
                ),
            }),
            Some(_) => {}
        }
    }
    for field in candidate {
        if !locked.iter().any(|other| other.name == field.name) {
            changes.push(ContractChange {
                class: CompatibilityClass::Additive,
                path: format!("{path}.{}", field.name),
                detail: format!("named type field '{}' was added", field.name),
            });
        }
    }
}

fn is_optional(type_expression: &str) -> bool {
    type_expression.trim().ends_with('?')
}

/// Arguments are positional at the call site, so a new argument is breaking
/// unless its type is optional (`T?`) — an optional trailing argument is the
/// one shape a caller written against the old signature still satisfies.
fn diff_arguments(
    path: &str,
    locked: &[InterfaceArgument],
    candidate: &[InterfaceArgument],
    changes: &mut Vec<ContractChange>,
) {
    for (index, argument) in locked.iter().enumerate() {
        match candidate.get(index) {
            None => changes.push(ContractChange {
                class: if is_optional(&argument.arg_type) {
                    CompatibilityClass::Additive
                } else {
                    CompatibilityClass::Breaking
                },
                path: format!("{path}.args.{}", argument.name),
                detail: if is_optional(&argument.arg_type) {
                    format!("optional argument '{}' was removed", argument.name)
                } else {
                    format!("argument '{}' was removed", argument.name)
                },
            }),
            Some(other) if other.arg_type != argument.arg_type => {
                changes.push(ContractChange {
                    class: CompatibilityClass::Breaking,
                    path: format!("{path}.args.{}", argument.name),
                    detail: format!(
                        "argument '{}' changed type from {} to {}",
                        argument.name, argument.arg_type, other.arg_type
                    ),
                });
            }
            Some(other) if other.name != argument.name => {
                changes.push(ContractChange {
                    class: CompatibilityClass::Breaking,
                    path: format!("{path}.args.{}", argument.name),
                    detail: format!(
                        "argument '{}' was renamed to '{}'",
                        argument.name, other.name
                    ),
                });
            }
            Some(_) => {}
        }
    }

    for argument in candidate.iter().skip(locked.len()) {
        let optional = argument.arg_type.ends_with('?');
        changes.push(ContractChange {
            class: if optional {
                CompatibilityClass::Additive
            } else {
                CompatibilityClass::Breaking
            },
            path: format!("{path}.args.{}", argument.name),
            detail: if optional {
                format!("optional argument '{}' was added", argument.name)
            } else {
                format!(
                    "required argument '{}' was added; existing callers omit it",
                    argument.name
                )
            },
        });
    }
}

/// A newly required consumer capability breaks a consumer that does not declare
/// it, so it needs the same review as a new capability on a module.
fn diff_capabilities(
    locked: &InterfaceContract,
    candidate: &InterfaceContract,
    changes: &mut Vec<ContractChange>,
) {
    for capability in &candidate.capabilities.required {
        if !locked.capabilities.required.contains(capability) {
            changes.push(ContractChange {
                class: CompatibilityClass::Breaking,
                path: format!("capabilities.{capability}"),
                detail: format!("consumers must now declare '{capability}' to use this interface"),
            });
        }
    }
    if locked.capabilities.read != candidate.capabilities.read {
        changes.push(ContractChange {
            class: CompatibilityClass::Breaking,
            path: "capabilities.read".to_string(),
            detail: "the state-read operation policy changed".to_string(),
        });
    }
    if locked.capabilities.events != candidate.capabilities.events {
        changes.push(ContractChange {
            class: CompatibilityClass::Breaking,
            path: "capabilities.events".to_string(),
            detail: "the event-subscription operation policy changed".to_string(),
        });
    }
    if locked.capabilities.methods != candidate.capabilities.methods {
        changes.push(ContractChange {
            class: CompatibilityClass::Breaking,
            path: "capabilities.methods".to_string(),
            detail: "the method operation policy changed".to_string(),
        });
    }
}

fn diff_feature_groups(
    locked: &InterfaceContract,
    candidate: &InterfaceContract,
    changes: &mut Vec<ContractChange>,
) {
    for (name, group) in &candidate.capabilities.feature_groups {
        let Some(previous) = locked.capabilities.feature_groups.get(name) else {
            changes.push(ContractChange {
                class: if group.required {
                    CompatibilityClass::Breaking
                } else {
                    CompatibilityClass::Additive
                },
                path: format!("capabilities.featureGroups.{name}"),
                detail: format!(
                    "{} provider feature group '{name}' was added",
                    if group.required {
                        "required"
                    } else {
                        "optional"
                    }
                ),
            });
            continue;
        };
        if previous != group {
            changes.push(ContractChange {
                class: CompatibilityClass::Breaking,
                path: format!("capabilities.featureGroups.{name}"),
                detail: format!("provider feature group '{name}' changed"),
            });
        }
    }
    for (name, group) in &locked.capabilities.feature_groups {
        if !candidate.capabilities.feature_groups.contains_key(name) {
            changes.push(ContractChange {
                class: if group.required {
                    CompatibilityClass::Breaking
                } else {
                    CompatibilityClass::Additive
                },
                path: format!("capabilities.featureGroups.{name}"),
                detail: format!("provider feature group '{name}' was removed"),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::parse_interface_contract;

    fn contract(body: &str) -> InterfaceContract {
        parse_interface_contract("mesh.audio", "1.0", &serde_json::from_str(body).unwrap()).unwrap()
    }

    fn base() -> InterfaceContract {
        contract(
            r#"{
                "state": [{"name":"percent","type":"float"},{"name":"muted","type":"boolean"}],
                "methods": [{"name":"set_volume",
                    "args":[{"name":"percent","type":"float"}],"returns":"Result"}],
                "events": [{"name":"VolumeChanged",
                    "payload":[{"name":"level","type":"float"}]}],
                "capabilities": {"required":["service.audio.read"]}
            }"#,
        )
    }

    #[test]
    fn an_identical_contract_is_compatible() {
        let diff = diff_contracts(&base(), &base());
        assert_eq!(diff.class(), CompatibilityClass::Compatible);
        assert!(diff.changes.is_empty());
    }

    #[test]
    fn operation_policy_changes_are_breaking() {
        let candidate = contract(
            r#"{
                "state": [{"name":"percent","type":"float"}],
                "methods": [{"name":"set_volume","returns":"Result"}],
                "events": [],
                "capabilities": {
                    "required":["service.audio.read"],
                    "read":["service.audio.observe"]
                }
            }"#,
        );
        let diff = diff_contracts(&base(), &candidate);
        assert_eq!(diff.class(), CompatibilityClass::Breaking);
        assert!(
            diff.changes
                .iter()
                .any(|change| change.path == "capabilities.read")
        );
    }

    #[test]
    fn additions_are_additive_not_breaking() {
        let candidate = contract(
            r#"{
                "state": [{"name":"percent","type":"float"},{"name":"muted","type":"boolean"},
                          {"name":"device","type":"string"}],
                "methods": [{"name":"set_volume",
                    "args":[{"name":"percent","type":"float"}],"returns":"Result"},
                    {"name":"set_muted","args":[{"name":"muted","type":"boolean"}],
                     "returns":"Result"}],
                "events": [{"name":"VolumeChanged","payload":[{"name":"level","type":"float"}]},
                           {"name":"DeviceChanged","payload":[]}],
                "capabilities": {"required":["service.audio.read"]}
            }"#,
        );
        let diff = diff_contracts(&base(), &candidate);
        assert_eq!(diff.class(), CompatibilityClass::Additive);
        assert!(!diff.is_breaking());
        assert_eq!(diff.changes.len(), 3);
    }

    #[test]
    fn a_removed_state_field_is_breaking() {
        let candidate = contract(
            r#"{"state":[{"name":"percent","type":"float"}],
                "methods":[{"name":"set_volume",
                    "args":[{"name":"percent","type":"float"}],"returns":"Result"}],
                "events":[{"name":"VolumeChanged","payload":[{"name":"level","type":"float"}]}],
                "capabilities":{"required":["service.audio.read"]}}"#,
        );
        let diff = diff_contracts(&base(), &candidate);
        assert!(diff.is_breaking());
        assert!(
            diff.breaking_changes()
                .any(|change| change.path == "state.muted")
        );
    }

    #[test]
    fn a_changed_type_is_breaking() {
        let candidate = contract(
            r#"{"state":[{"name":"percent","type":"int"},{"name":"muted","type":"boolean"}],
                "methods":[{"name":"set_volume",
                    "args":[{"name":"percent","type":"float"}],"returns":"Result"}],
                "events":[{"name":"VolumeChanged","payload":[{"name":"level","type":"float"}]}],
                "capabilities":{"required":["service.audio.read"]}}"#,
        );
        let diff = diff_contracts(&base(), &candidate);
        assert!(diff.is_breaking());
        assert!(
            diff.breaking_changes()
                .any(|change| change.detail.contains("from float to int"))
        );
    }

    #[test]
    fn a_removed_method_or_event_is_breaking() {
        let no_method = contract(
            r#"{"state":[{"name":"percent","type":"float"},{"name":"muted","type":"boolean"}],
                "methods":[],
                "events":[{"name":"VolumeChanged","payload":[{"name":"level","type":"float"}]}],
                "capabilities":{"required":["service.audio.read"]}}"#,
        );
        assert!(diff_contracts(&base(), &no_method).is_breaking());

        let no_event = contract(
            r#"{"state":[{"name":"percent","type":"float"},{"name":"muted","type":"boolean"}],
                "methods":[{"name":"set_volume",
                    "args":[{"name":"percent","type":"float"}],"returns":"Result"}],
                "events":[],
                "capabilities":{"required":["service.audio.read"]}}"#,
        );
        assert!(diff_contracts(&base(), &no_event).is_breaking());
    }

    #[test]
    fn a_new_required_argument_breaks_but_an_optional_one_does_not() {
        let required = contract(
            r#"{"state":[{"name":"percent","type":"float"},{"name":"muted","type":"boolean"}],
                "methods":[{"name":"set_volume","args":[{"name":"percent","type":"float"},
                    {"name":"device_id","type":"string"}],"returns":"Result"}],
                "events":[{"name":"VolumeChanged","payload":[{"name":"level","type":"float"}]}],
                "capabilities":{"required":["service.audio.read"]}}"#,
        );
        assert!(diff_contracts(&base(), &required).is_breaking());

        let optional = contract(
            r#"{"state":[{"name":"percent","type":"float"},{"name":"muted","type":"boolean"}],
                "methods":[{"name":"set_volume","args":[{"name":"percent","type":"float"},
                    {"name":"device_id","type":"string?"}],"returns":"Result"}],
                "events":[{"name":"VolumeChanged","payload":[{"name":"level","type":"float"}]}],
                "capabilities":{"required":["service.audio.read"]}}"#,
        );
        let diff = diff_contracts(&base(), &optional);
        assert!(!diff.is_breaking());
        assert_eq!(diff.class(), CompatibilityClass::Additive);
    }

    #[test]
    fn a_newly_required_consumer_capability_is_breaking() {
        let candidate = contract(
            r#"{"state":[{"name":"percent","type":"float"},{"name":"muted","type":"boolean"}],
                "methods":[{"name":"set_volume",
                    "args":[{"name":"percent","type":"float"}],"returns":"Result"}],
                "events":[{"name":"VolumeChanged","payload":[{"name":"level","type":"float"}]}],
                "capabilities":{"required":["service.audio.read","service.audio.control"]}}"#,
        );
        let diff = diff_contracts(&base(), &candidate);
        assert!(diff.is_breaking());
        assert!(
            diff.breaking_changes()
                .any(|change| change.path == "capabilities.service.audio.control")
        );
    }

    #[test]
    fn optional_feature_groups_are_additive_but_required_groups_are_breaking() {
        let mut optional = base();
        optional.capabilities.feature_groups.insert(
            "recording".to_string(),
            crate::contract::ContractFeatureGroup::default(),
        );
        let diff = diff_contracts(&base(), &optional);
        assert!(!diff.is_breaking());
        assert!(diff.changes.iter().any(|change| {
            change.path == "capabilities.featureGroups.recording"
                && change.class == CompatibilityClass::Additive
        }));

        let mut required = base();
        required.capabilities.feature_groups.insert(
            "exclusive_output".to_string(),
            crate::contract::ContractFeatureGroup {
                required: true,
                ..Default::default()
            },
        );
        let diff = diff_contracts(&base(), &required);
        assert!(diff.is_breaking());
    }

    #[test]
    fn a_renamed_event_payload_field_is_breaking() {
        let candidate = contract(
            r#"{"state":[{"name":"percent","type":"float"},{"name":"muted","type":"boolean"}],
                "methods":[{"name":"set_volume",
                    "args":[{"name":"percent","type":"float"}],"returns":"Result"}],
                "events":[{"name":"VolumeChanged","payload":[{"name":"volume","type":"float"}]}],
                "capabilities":{"required":["service.audio.read"]}}"#,
        );
        assert!(diff_contracts(&base(), &candidate).is_breaking());
    }

    #[test]
    fn named_type_changes_are_diffed_transitively() {
        let locked = contract(
            r#"{
                "state":[{"name":"volume","type":"Volume"}],
                "types":{"Volume":{"fields":[{"name":"level","type":"float"}]}},
                "capabilities":{"required":["service.audio.read"]}
            }"#,
        );
        let candidate = contract(
            r#"{
                "state":[{"name":"volume","type":"Volume"}],
                "types":{"Volume":{"fields":[{"name":"level","type":"int"}]}},
                "capabilities":{"required":["service.audio.read"]}
            }"#,
        );
        let diff = diff_contracts(&locked, &candidate);
        assert!(diff.breaking_changes().any(|change| {
            change.path == "types.Volume.level" && change.detail.contains("float to int")
        }));
    }

    #[test]
    fn behavioral_annotations_are_breaking_when_changed() {
        let locked = contract(
            r#"{
                "state":[{"name":"muted","type":"boolean"}],
                "methods":[{"name":"set_muted","args":[{"name":"muted","type":"boolean"}],"returns":"Result"}],
                "capabilities":{"required":["service.audio.read"]}
            }"#,
        );
        let candidate = contract(
            r#"{
                "state":[{"name":"muted","type":"boolean"}],
                "methods":[{"name":"set_muted","args":[{"name":"muted","type":"boolean"}],"returns":"Result",
                    "coalesce":true,"stateBinding":{"field":"muted","fromArg":"muted"}}],
                "capabilities":{"required":["service.audio.read"]}
            }"#,
        );
        let diff = diff_contracts(&locked, &candidate);
        assert!(
            diff.breaking_changes()
                .any(|change| { change.path == "methods.set_muted.coalesce" })
        );
        assert!(
            diff.breaking_changes()
                .any(|change| { change.path == "methods.set_muted.stateBinding" })
        );
    }

    #[test]
    fn bidirectional_diff_catches_provider_breakage_from_additions() {
        let locked = base();
        let candidate = contract(
            r#"{
                "state":[{"name":"percent","type":"float"},{"name":"muted","type":"boolean"},
                    {"name":"device","type":"string"}],
                "methods":[{"name":"set_volume","args":[{"name":"percent","type":"float"}],"returns":"Result"}],
                "events":[{"name":"VolumeChanged","payload":[{"name":"level","type":"float"},
                    {"name":"device","type":"string"}]}],
                "capabilities":{"required":["service.audio.read"]}
            }"#,
        );
        let diffs = diff_contracts_bidirectional(&locked, &candidate);
        assert!(!diffs.consumer.is_breaking());
        assert_eq!(
            diffs.classification().consumer,
            CompatibilityClass::Additive
        );
        assert_eq!(
            diffs.classification().provider,
            CompatibilityClass::Breaking
        );
        assert!(
            diffs
                .provider
                .breaking_changes()
                .any(|change| { change.path == "state.device" })
        );
        assert!(
            diffs
                .provider
                .breaking_changes()
                .any(|change| { change.path == "events.VolumeChanged.device" })
        );
    }
}
