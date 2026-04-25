/// A live component instance: parsed file + running script + current widget tree.
use crate::context::{ScriptContext, ScriptError};
use mesh_capability::CapabilitySet;
use mesh_component::ComponentFile;
use mesh_ui::events::UiEvent;
use mesh_ui::style::StyleResolver;
use mesh_ui::tree::WidgetNode;

/// A running component instance that ties together parsing, scripting, and UI.
#[derive(Debug)]
pub struct ComponentInstance {
    pub plugin_id: String,
    pub file: ComponentFile,
    pub script_ctx: ScriptContext,
    pub current_tree: Option<WidgetNode>,
}

impl ComponentInstance {
    /// Create a new component instance from a parsed file.
    pub fn new(
        plugin_id: impl Into<String>,
        file: ComponentFile,
        capabilities: CapabilitySet,
    ) -> Result<Self, ScriptError> {
        let plugin_id = plugin_id.into();
        let script_ctx = ScriptContext::new(&plugin_id, capabilities)?;

        Ok(Self {
            plugin_id,
            file,
            script_ctx,
            current_tree: None,
        })
    }

    /// Initialize the component: load script, call init, build initial tree.
    pub fn init(&mut self, style_resolver: &StyleResolver) -> Result<(), ScriptError> {
        // Load the script if present.
        if let Some(ref script) = self.file.script {
            self.script_ctx.load_script(&script.source)?;
            self.script_ctx.call_init()?;
        }

        // Build the initial widget tree.
        self.rebuild_tree(style_resolver);
        Ok(())
    }

    /// Rebuild the widget tree from the template and current script state.
    fn rebuild_tree(&mut self, style_resolver: &StyleResolver) {
        if let Some(ref template) = self.file.template {
            let tree = build_tree_from_template(
                template,
                self.script_ctx.state(),
                style_resolver,
                self.file.style.as_ref(),
            );
            self.current_tree = Some(tree);
            self.script_ctx.state_mut().clear_dirty();
        }
    }

    /// Rebuild the widget tree if script state has changed. Returns true if rebuilt.
    pub fn rebuild_if_dirty(&mut self, style_resolver: &StyleResolver) -> bool {
        if self.script_ctx.state().is_dirty() {
            self.rebuild_tree(style_resolver);
            true
        } else {
            false
        }
    }

    /// Route a UI event to the appropriate script handler.
    pub fn handle_event(&mut self, event: &UiEvent) -> Result<(), ScriptError> {
        let handler_name = event.handler_name();
        let node_id = event.node_id();

        // Find the node in the tree and check for event handlers.
        if let Some(ref tree) = self.current_tree {
            if let Some(node) = tree.find(node_id) {
                if let Some(script_fn) = node.event_handlers.get(handler_name) {
                    return self.script_ctx.call_handler(script_fn, &[]);
                }
            }
        }

        Ok(())
    }
}

/// Build a widget tree from a template AST.
///
/// This evaluates bindings against script state and resolves styles.
fn build_tree_from_template(
    template: &mesh_component::TemplateBlock,
    state: &dyn mesh_ui::VariableStore,
    style_resolver: &StyleResolver,
    style_block: Option<&mesh_component::StyleBlock>,
) -> WidgetNode {
    let rules = style_block.map(|s| s.rules.as_slice()).unwrap_or(&[]);

    let mut root = WidgetNode::new("root");
    root.children = template
        .root
        .iter()
        .map(|n| build_node(n, state, style_resolver, rules))
        .collect();
    root
}

fn build_node(
    node: &mesh_component::TemplateNode,
    state: &dyn mesh_ui::VariableStore,
    style_resolver: &StyleResolver,
    rules: &[mesh_component::style::StyleRule],
) -> WidgetNode {
    match node {
        mesh_component::TemplateNode::Element(el) => {
            let mut widget = WidgetNode::new(&el.tag);

            // Extract class and id from attributes for style matching.
            let mut classes = Vec::new();
            let mut id = None;

            for attr in &el.attributes {
                match &attr.value {
                    mesh_component::AttributeValue::Static(val) => {
                        if attr.name == "class" {
                            classes.extend(val.split_whitespace().map(|s| s.to_string()));
                        } else if attr.name == "id" {
                            id = Some(val.clone());
                        } else {
                            widget.attributes.insert(attr.name.clone(), val.clone());
                        }
                    }
                    mesh_component::AttributeValue::Binding(expr)
                    | mesh_component::AttributeValue::TwoWayBinding(expr) => {
                        let value = state
                            .get(expr)
                            .map(|v| match v {
                                serde_json::Value::String(s) => s,
                                other => other.to_string(),
                            })
                            .unwrap_or_default();
                        widget.attributes.insert(attr.name.clone(), value);
                    }
                    mesh_component::AttributeValue::EventHandler(handler) => {
                        widget
                            .event_handlers
                            .insert(attr.name.clone(), handler.clone());
                    }
                }
            }

            // Resolve computed style. State is always default at build time;
            // InputState::process updates it at runtime and restyle_subtree re-resolves.
            widget.computed_style = style_resolver.resolve_node_style(
                rules,
                &el.tag,
                &classes,
                id.as_deref(),
                mesh_ui::StyleContext::default(),
                mesh_ui::ElementState::default(),
            );

            // Recurse into children.
            widget.children = el
                .children
                .iter()
                .map(|c| build_node(c, state, style_resolver, rules))
                .collect();

            widget
        }

        mesh_component::TemplateNode::Text(text) => {
            let mut widget = WidgetNode::new("text");
            widget
                .attributes
                .insert("content".to_string(), text.content.clone());
            widget
        }

        mesh_component::TemplateNode::Expr(expr) => {
            let mut widget = WidgetNode::new("text");
            let value = state
                .get(&expr.expression)
                .map(|v| match v {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                })
                .unwrap_or_else(|| format!("{{ {} }}", expr.expression));
            widget.attributes.insert("content".to_string(), value);
            widget
        }

        // Simplified stubs for control flow nodes.
        mesh_component::TemplateNode::If(_)
        | mesh_component::TemplateNode::For(_)
        | mesh_component::TemplateNode::Slot(_)
        | mesh_component::TemplateNode::Component(_) => WidgetNode::new("placeholder"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_capability::CapabilitySet;
    use mesh_component::parse_component;
    use mesh_theme::default_theme;

    #[test]
    fn component_instance_lifecycle() {
        let source = r#"
<template>
  <div>
    <span class="title">Hello</span>
  </div>
</template>

<script lang="luau">
function init()
end
</script>

<style>
.title {
    color: token(color.primary);
    font-size: 20px;
}
</style>
"#;

        let file = parse_component(source).unwrap();
        let caps = CapabilitySet::new();
        let mut instance = ComponentInstance::new("@test/hello", file, caps).unwrap();

        let theme = default_theme();
        let resolver = StyleResolver::new(&theme);

        instance.init(&resolver).unwrap();
        assert!(instance.current_tree.is_some());

        let tree = instance.current_tree.as_ref().unwrap();
        assert_eq!(tree.tag, "root");
        assert!(!tree.children.is_empty());
    }

    #[test]
    fn dirty_state_triggers_rebuild() {
        let source = r#"
<template>
  <span>{message}</span>
</template>
"#;
        let file = parse_component(source).unwrap();
        let caps = CapabilitySet::new();
        let mut instance = ComponentInstance::new("@test/dirty", file, caps).unwrap();

        let theme = default_theme();
        let resolver = StyleResolver::new(&theme);
        instance.init(&resolver).unwrap();

        // Not dirty yet.
        assert!(!instance.rebuild_if_dirty(&resolver));

        // Set a variable — now dirty.
        instance
            .script_ctx
            .state_mut()
            .set("message", serde_json::Value::String("Updated".to_string()));
        assert!(instance.rebuild_if_dirty(&resolver));

        // Rebuilt — no longer dirty.
        assert!(!instance.rebuild_if_dirty(&resolver));
    }
}
