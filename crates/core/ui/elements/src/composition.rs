//! Typed protocol helpers for compiler-to-shell component composition.
//!
//! The widget tree still stores handler names and component props as strings
//! at its serialization boundary, but these helpers own the reserved wire
//! format so compiler, interaction, and shell code do not independently parse
//! or construct it.

/// Reserved handler prefix for an embedded component instance.
pub const EMBEDDED_HANDLER_PREFIX: &str = "__mesh_embed__::";
/// Reserved attribute prefix for a component binding.
pub const COMPONENT_BINDING_PREFIX: &str = "__mesh_binding_";
/// Reserved component attribute that carries an instance binding target.
pub const COMPONENT_BIND_THIS_ATTRIBUTE: &str = "__mesh_bind_this";

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

/// Returns the reserved wire attribute for a component binding name.
pub fn component_binding_attribute(name: &str) -> String {
    let mut key = String::with_capacity(COMPONENT_BINDING_PREFIX.len() + name.len());
    key.push_str(COMPONENT_BINDING_PREFIX);
    key.push_str(name);
    key
}

/// Returns whether an attribute belongs to the composition protocol rather
/// than the embedded component's public props.
pub fn is_composition_protocol_attribute(name: &str) -> bool {
    name.starts_with(COMPONENT_BINDING_PREFIX) || name == COMPONENT_BIND_THIS_ATTRIBUTE
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

    #[test]
    fn composition_protocol_attributes_are_not_public_props() {
        assert_eq!(
            component_binding_attribute("hidden"),
            "__mesh_binding_hidden"
        );
        assert!(is_composition_protocol_attribute("__mesh_binding_hidden"));
        assert!(is_composition_protocol_attribute("__mesh_bind_this"));
        assert!(!is_composition_protocol_attribute("hidden"));
    }
}
