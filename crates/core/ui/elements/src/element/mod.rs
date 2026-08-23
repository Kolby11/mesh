//! Core element model exposed to runtime code and tooling.
//!
//! Elements are MESH-owned primitives (`button`, `icon`, `input`, etc.).
//! Components compose these primitives; modules package complete features.

use crate::{AccessibilityRole, ElementState, WidgetNode};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The parser's source-tag enum is the canonical element-kind vocabulary.
/// Keeping this public alias avoids a second enum that can drift from the
/// component AST's source tags.
pub use mesh_core_component::template::SourceTag as ElementKind;

mod contracts;
mod snapshot;
mod validate;

pub use contracts::{BASE_ELEMENT_FIELDS, ELEMENT_CONTRACT_DEFS, ELEMENT_TYPE_DEFS};
pub use snapshot::{
    ElementRect, ElementSnapshot, ElementStateSnapshot, element_snapshot, element_snapshot_json,
};
pub use validate::{validate_element_attribute, validate_element_event};

use contracts::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ElementFamily {
    Layout,
    Display,
    Action,
    TextInput,
    ChoiceMenu,
    Container,
    Collection,
    Shell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementAttributeType {
    String,
    Number,
    Boolean,
    Token,
}

#[derive(Debug, Clone, Copy)]
pub struct ElementAttributeDef {
    pub name: &'static str,
    pub attribute_type: ElementAttributeType,
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ElementStateFlag {
    Disabled,
    ReadOnly,
    Required,
    Focused,
    Selected,
    Checked,
    Expanded,
    Pressed,
    Invalid,
    Active,
    Value,
}

#[derive(Debug, Clone, Copy)]
pub struct ElementEventDef {
    pub name: &'static str,
    pub payload: &'static str,
    pub description: &'static str,
}

#[derive(Debug, Clone)]
pub struct ElementAccessibilityDef {
    pub role: AccessibilityRole,
    pub focusable: bool,
    pub label_required: bool,
}

#[derive(Debug, Clone)]
pub struct ElementContractDef {
    pub kind: ElementKind,
    /// The authored/source tag. This is never replaced by the lowered runtime
    /// primitive and is the key used by diagnostics and accessibility.
    pub source_tag: &'static str,
    /// Compatibility alias for the authored tag.
    pub tag: &'static str,
    /// The primitive tag consumed by layout, paint, and runtime dispatch.
    pub runtime_tag: &'static str,
    pub family: ElementFamily,
    pub type_name: &'static str,
    pub attributes: &'static [ElementAttributeDef],
    pub states: &'static [ElementStateFlag],
    pub events: &'static [ElementEventDef],
    pub accessibility: ElementAccessibilityDef,
    pub style_hooks: &'static [&'static str],
    pub input_type: Option<&'static str>,
    pub default_attributes: &'static [(&'static str, &'static str)],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementDiagnosticKind {
    UnknownElementTag,
    UnsupportedAttribute,
    UnsupportedEvent,
    InvalidAttributeValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementDiagnostic {
    pub tag: String,
    pub name: String,
    pub kind: ElementDiagnosticKind,
    pub message: String,
    pub action: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementFieldType {
    String,
    Number,
    Boolean,
    Rect,
    Object,
}

#[derive(Debug, Clone, Copy)]
pub struct ElementFieldDef {
    pub name: &'static str,
    pub field_type: ElementFieldType,
    pub optional: bool,
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ElementTypeDef {
    pub kind: ElementKind,
    /// The authored/source tag represented by this type.
    pub source_tag: &'static str,
    /// Compatibility alias for the authored tag.
    pub tag: &'static str,
    /// The runtime primitive tag used when the source tag is lowered.
    pub runtime_tag: &'static str,
    pub type_name: &'static str,
    pub fields: &'static [ElementFieldDef],
}

pub fn element_type_for_tag(tag: &str) -> &'static ElementTypeDef {
    ELEMENT_TYPE_DEFS
        .iter()
        .find(|def| def.source_tag == tag || def.runtime_tag == tag)
        .unwrap_or(&UNKNOWN_ELEMENT_TYPE)
}

pub fn element_contract_for_tag(tag: &str) -> Option<&'static ElementContractDef> {
    Some(&ELEMENT_CONTRACT_DEFS[contract_slot_for_tag(tag)?])
}

pub fn element_contract_tags() -> impl Iterator<Item = &'static str> {
    ELEMENT_CONTRACT_DEFS.iter().map(|def| def.source_tag)
}

/// Resolve the lowered runtime primitive for an authored source tag.
pub fn element_runtime_tag_for_tag(tag: &str) -> Option<&'static str> {
    element_contract_for_tag(tag).map(|definition| definition.runtime_tag)
}

pub fn element_input_type_for_tag(tag: &str) -> Option<&'static str> {
    element_contract_for_tag(tag).and_then(|definition| definition.input_type)
}

pub fn element_default_attributes_for_tag(tag: &str) -> &'static [(&'static str, &'static str)] {
    element_contract_for_tag(tag)
        .map(|definition| definition.default_attributes)
        .unwrap_or(&[])
}

pub fn common_state_flags() -> &'static [ElementStateFlag] {
    COMMON_STATES
}

#[cfg(test)]
mod tests;
