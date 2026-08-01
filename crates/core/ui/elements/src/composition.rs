//! Typed protocol helpers for compiler-to-shell component composition.
//!
//! Handler names still use a serialized string at the widget-tree boundary.
//! Component values and binding metadata use [`ComponentCompositionProps`], so
//! bindings cannot leak into an embedded component's public prop namespace.

use crate::AttributeMap;

/// Reserved handler prefix for an embedded component instance.
pub const EMBEDDED_HANDLER_PREFIX: &str = "__mesh_embed__::";
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

/// Returns whether `handler` already targets an embedded component instance.
pub fn is_embedded_handler(handler: &str) -> bool {
    handler.starts_with(EMBEDDED_HANDLER_PREFIX)
}

/// Builds the serialized handler target for an embedded component instance.
/// Existing embedded targets pass through unchanged, which makes repeated
/// composition idempotent.
pub fn namespace_embedded_handler(instance_key: &str, handler: &str) -> String {
    if is_embedded_handler(handler) {
        return handler.to_owned();
    }
    namespace_embedded_handler_with_prefix(&embedded_handler_prefix(instance_key), handler)
}

/// Builds the reusable prefix for all handlers in an embedded subtree.
pub fn embedded_handler_prefix(instance_key: &str) -> String {
    let mut prefix = String::with_capacity(EMBEDDED_HANDLER_PREFIX.len() + instance_key.len() + 2);
    prefix.push_str(EMBEDDED_HANDLER_PREFIX);
    prefix.push_str(instance_key);
    prefix.push_str("::");
    prefix
}

/// Namespaces a local handler using a prefix returned by
/// [`embedded_handler_prefix`].
pub fn namespace_embedded_handler_with_prefix(prefix: &str, handler: &str) -> String {
    let mut namespaced = String::with_capacity(prefix.len() + handler.len());
    namespaced.push_str(prefix);
    namespaced.push_str(handler);
    namespaced
}

/// Splits an embedded handler target into its instance key and local handler.
pub fn parse_embedded_handler(handler: &str) -> Option<(&str, &str)> {
    let rest = handler.strip_prefix(EMBEDDED_HANDLER_PREFIX)?;
    rest.rsplit_once("::")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_handler_protocol_round_trips_and_is_idempotent() {
        let namespaced = namespace_embedded_handler("@mesh/panel/local:Clock", "open");
        assert_eq!(namespaced, "__mesh_embed__::@mesh/panel/local:Clock::open");
        assert_eq!(
            parse_embedded_handler(&namespaced),
            Some(("@mesh/panel/local:Clock", "open"))
        );
        assert_eq!(namespace_embedded_handler("other", &namespaced), namespaced);
    }
}
