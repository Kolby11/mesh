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

    diff_state_fields(locked, candidate, &mut changes);
    diff_methods(locked, candidate, &mut changes);
    diff_events(locked, candidate, &mut changes);
    diff_capabilities(locked, candidate, &mut changes);

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
            None => changes.push(ContractChange {
                class: CompatibilityClass::Breaking,
                path: format!("state.{}", field.name),
                detail: format!(
                    "state field '{}' was removed; consumers reading it break",
                    field.name
                ),
            }),
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
                None => changes.push(ContractChange {
                    class: CompatibilityClass::Breaking,
                    path: format!("events.{}.{}", event.name, field.name),
                    detail: format!(
                        "event '{}' no longer carries payload field '{}'",
                        event.name, field.name
                    ),
                }),
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
                class: CompatibilityClass::Breaking,
                path: format!("{path}.args.{}", argument.name),
                detail: format!("argument '{}' was removed", argument.name),
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
}
