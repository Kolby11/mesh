//! Typed protocol helpers for compiler-to-shell component composition.
//!
//! Handler targets, component values, and binding metadata cross the widget-tree
//! boundary as typed values. The legacy serialized handler form is accepted
//! only when converting externally supplied runtime values.

use crate::AttributeMap;
use std::ops::Deref;

/// A script handler and the component runtime that owns it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HandlerTarget {
    Root(String),
    Embedded {
        instance_key: String,
        handler: String,
    },
}

impl HandlerTarget {
    pub fn root(handler: impl Into<String>) -> Self {
        Self::Root(handler.into())
    }

    pub fn embedded(instance_key: impl Into<String>, handler: impl Into<String>) -> Self {
        Self::Embedded {
            instance_key: instance_key.into(),
            handler: handler.into(),
        }
    }

    /// Assign an owner to a root handler. Repeated composition is idempotent.
    pub fn namespace(&mut self, instance_key: &str) {
        let Self::Root(handler) = self else {
            return;
        };
        *self = Self::embedded(instance_key, std::mem::take(handler));
    }

    pub fn handler(&self) -> &str {
        match self {
            Self::Root(handler) | Self::Embedded { handler, .. } => handler,
        }
    }

    pub fn as_str(&self) -> &str {
        self.handler()
    }

    pub fn instance_key(&self) -> Option<&str> {
        match self {
            Self::Root(_) => None,
            Self::Embedded { instance_key, .. } => Some(instance_key),
        }
    }

    pub fn dynamic_heap_bytes(&self) -> usize {
        match self {
            Self::Root(handler) => handler.capacity(),
            Self::Embedded {
                instance_key,
                handler,
            } => instance_key.capacity() + handler.capacity(),
        }
    }

    /// Compatibility parser for callers that still submit the former external
    /// string form. Widget trees and composition never store this encoding.
    pub fn from_legacy_serialized(value: impl Into<String>) -> Self {
        const PREFIX: &str = "__mesh_embed__::";
        let value = value.into();
        let Some(rest) = value.strip_prefix(PREFIX) else {
            return Self::Root(value);
        };
        let Some((instance_key, handler)) = rest.rsplit_once("::") else {
            return Self::Root(value);
        };
        Self::embedded(instance_key, handler)
    }
}

impl Deref for HandlerTarget {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.handler()
    }
}

impl AsRef<str> for HandlerTarget {
    fn as_ref(&self) -> &str {
        self.handler()
    }
}

impl From<String> for HandlerTarget {
    fn from(handler: String) -> Self {
        Self::Root(handler)
    }
}

impl From<&str> for HandlerTarget {
    fn from(handler: &str) -> Self {
        Self::Root(handler.to_owned())
    }
}

impl PartialEq<str> for HandlerTarget {
    fn eq(&self, other: &str) -> bool {
        self.handler() == other
    }
}

impl PartialEq<&str> for HandlerTarget {
    fn eq(&self, other: &&str) -> bool {
        self.handler() == *other
    }
}

/// Resolved values and binding metadata passed from the frontend compiler to
/// the shell composition host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComponentCompositionProps {
    /// Public values supplied to the embedded component.
    pub values: AttributeMap,
    /// Parent-state bindings keyed by public prop name.
    pub bindings: AttributeMap,
    /// Optional parent field receiving the child component instance.
    pub bind_this: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_target_namespacing_is_typed_and_idempotent() {
        let mut target = HandlerTarget::root("open");
        target.namespace("@mesh/panel/local:Clock");
        assert_eq!(target.handler(), "open");
        assert_eq!(target.instance_key(), Some("@mesh/panel/local:Clock"));
        target.namespace("other");
        assert_eq!(target.instance_key(), Some("@mesh/panel/local:Clock"));
    }

    #[test]
    fn legacy_handler_strings_are_parsed_only_at_the_compatibility_edge() {
        let target =
            HandlerTarget::from_legacy_serialized("__mesh_embed__::@mesh/panel/local:Clock::open");
        assert_eq!(target.handler(), "open");
        assert_eq!(target.instance_key(), Some("@mesh/panel/local:Clock"));
    }
}
