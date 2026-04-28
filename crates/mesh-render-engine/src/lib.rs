mod accessibility;
mod compile;
mod expr;
mod render;
mod style;
pub mod surface;
mod tags;

use mesh_component::ComponentFile;
use mesh_plugin::Manifest;
use mesh_theme::Theme;
use mesh_ui::{LayoutEngine, StyleContext, StyleResolver, VariableStore, WidgetNode};

use std::collections::HashMap;
use std::path::PathBuf;

pub use accessibility::root_accessibility_role;
pub use compile::{CompileFrontendError, compile_frontend_plugin, is_frontend_plugin};
pub use render::build_widget_tree_from_component;
pub use surface::{
    DebugOverlay, FrontendRenderEngine, LayerSurfaceConfig, PixelBuffer, RenderEngine, RenderError,
    SharedTextMeasurer, WindowEvent, WindowKeyEvent, coalesce_pointer_moves, event_surface_id,
    paint_frontend_tree, paint_frontend_tree_at,
};
pub use tags::UiTag;

/// A `VariableStore` overlay used during `{#for}` iteration.
/// Shadows one variable name with the current loop item value while
/// delegating everything else to the underlying store.
struct LayeredStore<'a> {
    base: &'a dyn VariableStore,
    item_name: &'a str,
    item_value: serde_json::Value,
}

impl VariableStore for LayeredStore<'_> {
    fn get(&self, name: &str) -> Option<serde_json::Value> {
        if name == self.item_name {
            Some(self.item_value.clone())
        } else {
            self.base.get(name)
        }
    }

    fn keys(&self) -> Vec<String> {
        let mut keys = self.base.keys();
        if !keys.iter().any(|k| k == self.item_name) {
            keys.push(self.item_name.to_string());
        }
        keys
    }

    fn translate(&self, key: &str) -> Option<String> {
        self.base.translate(key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendRenderMode {
    Surface,
    Embedded,
}

pub trait FrontendCompositionResolver {
    fn render_import(
        &self,
        host: &Manifest,
        host_instance_key: &str,
        alias: &str,
        props: &HashMap<String, String>,
        container_width: f32,
        container_height: f32,
    ) -> Option<WidgetNode>;

    fn render_slot(
        &self,
        host: &Manifest,
        host_instance_key: &str,
        slot_name: Option<&str>,
        container_width: f32,
        container_height: f32,
    ) -> Vec<WidgetNode>;
}

#[derive(Debug, Clone)]
pub struct CompiledFrontendPlugin {
    pub manifest: Manifest,
    pub source_path: PathBuf,
    pub component: ComponentFile,
    /// Local single-file components shipped in `src/components/*.mesh` inside
    /// the plugin directory. Keyed by filename stem (e.g. `settings-button`).
    pub local_components: std::collections::HashMap<String, mesh_component::ComponentFile>,
}

impl CompiledFrontendPlugin {
    pub fn surface_id(&self) -> &str {
        &self.manifest.package.id
    }

    pub fn build_preview_tree(&self, theme: &Theme, width: u32, height: u32) -> WidgetNode {
        self.build_preview_tree_with_state(theme, width, height, None)
    }

    pub fn build_preview_tree_with_state(
        &self,
        theme: &Theme,
        width: u32,
        height: u32,
        state: Option<&dyn VariableStore>,
    ) -> WidgetNode {
        self.build_tree_with_state(
            theme,
            width,
            height,
            state,
            FrontendRenderMode::Surface,
            &self.manifest.package.id,
            None,
            None,
        )
    }

    pub fn build_tree_with_state(
        &self,
        theme: &Theme,
        width: u32,
        height: u32,
        state: Option<&dyn VariableStore>,
        mode: FrontendRenderMode,
        instance_key: &str,
        composition: Option<&dyn FrontendCompositionResolver>,
        measurer: Option<&dyn mesh_ui::TextMeasurer>,
    ) -> WidgetNode {
        let mut root = WidgetNode::new("surface");
        root.attributes
            .insert("id".into(), self.manifest.package.id.clone());
        root.computed_style = match mode {
            FrontendRenderMode::Surface => {
                style::surface_style(&self.manifest.package.id, width, height)
            }
            FrontendRenderMode::Embedded => style::embedded_root_style(),
        };

        let manifest_has_role = self
            .manifest
            .accessibility
            .as_ref()
            .and_then(|accessibility| accessibility.role.as_ref())
            .is_some();
        if let Some(accessibility) = &self.manifest.accessibility {
            if let Some(role) = accessibility.role.as_deref() {
                root.accessibility.role = accessibility::parse_accessibility_role(role);
            }
            root.accessibility.label = accessibility
                .label
                .clone()
                .or_else(|| self.manifest.package.name.clone());
            root.accessibility.description = accessibility
                .description
                .clone()
                .or_else(|| self.manifest.package.description.clone());
        }
        if let Some(meta) = &self.component.meta {
            if let Some(role) = &meta.role {
                if !manifest_has_role {
                    root.accessibility.role = role.clone();
                }
            }
            if root.accessibility.label.is_none() {
                root.accessibility.label = meta.label.clone();
            }
            if root.accessibility.description.is_none() {
                root.accessibility.description = meta.description.clone();
            }
        }

        let resolver = StyleResolver::new(theme);
        let rules = self
            .component
            .style
            .as_ref()
            .map(|style| style.rules.as_slice())
            .unwrap_or(&[]);

        if let Some(template) = &self.component.template {
            let root_context = style::child_style_context(
                &root.computed_style,
                StyleContext {
                    container_width: width as f32,
                    container_height: height as f32,
                },
            );
            root.children = template
                .root
                .iter()
                .map(|node| {
                    render::build_widget_node(
                        node,
                        &self.manifest,
                        rules,
                        &resolver,
                        Some(&root.computed_style),
                        root_context,
                        state,
                        instance_key,
                        composition,
                    )
                })
                .collect();
        }

        LayoutEngine::compute_with_measurer(&mut root, width as f32, height as f32, measurer);
        root
    }

    pub fn referenced_component_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        if let Some(template) = &self.component.template {
            render::collect_component_tags(&template.root, &mut tags);
        }
        tags.sort();
        tags.dedup();
        tags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_component::template::{Attribute, AttributeValue};

    struct MapStore(HashMap<String, serde_json::Value>);

    impl mesh_ui::VariableStore for MapStore {
        fn get(&self, name: &str) -> Option<serde_json::Value> {
            self.0.get(name).cloned()
        }

        fn keys(&self) -> Vec<String> {
            self.0.keys().cloned().collect()
        }
    }

    #[test]
    fn normalizes_on_prefixed_event_handler_names() {
        let attrs = vec![Attribute {
            name: "onclick".into(),
            value: AttributeValue::EventHandler("openPanel".into()),
        }];

        let (_, _, _, handlers) = render::parse_attributes(&attrs, None);

        assert_eq!(handlers.get("click").map(String::as_str), Some("openPanel"));
        assert!(!handlers.contains_key("onclick"));
    }

    #[test]
    fn eval_expr_length_operator() {
        let store = MapStore(
            [("items".to_string(), serde_json::json!(["a", "b", "c"]))]
                .into_iter()
                .collect(),
        );
        assert_eq!(expr::eval_expr("#items", &store), "3");
        assert_eq!(expr::eval_expr("#missing", &store), "0");
    }

    #[test]
    fn eval_expr_boolean_and() {
        let store = MapStore(
            [
                ("a".to_string(), serde_json::json!(true)),
                ("b".to_string(), serde_json::json!(false)),
            ]
            .into_iter()
            .collect(),
        );
        assert_eq!(expr::eval_expr("a and b", &store), "false");
        assert_eq!(expr::eval_expr("a and a", &store), "true");
        assert_eq!(expr::eval_expr("b and a", &store), "false");
    }

    #[test]
    fn for_node_iterates_over_list() {
        let source = r#"
<template>
  <box>
    {#for item in items}
      <text>{item.name}</text>
    {/for}
  </box>
</template>
"#;
        let plugin = mesh_component::parse_component(source).unwrap();
        let manifest = mesh_plugin::Manifest {
            package: mesh_plugin::PackageSection {
                id: "test".into(),
                version: "0.1.0".into(),
                plugin_type: mesh_plugin::PluginType::Widget,
                api_version: "0.1.0".into(),
                name: None,
                license: None,
                description: None,
                authors: vec![],
                repository: None,
            },
            compatibility: Default::default(),
            dependencies: Default::default(),
            capabilities: Default::default(),
            entrypoints: Default::default(),
            accessibility: None,
            settings: None,
            i18n: None,
            theme: None,
            service: None,
            provides: vec![],
            interface: None,
            extensions: vec![],
            exports: Default::default(),
            provides_slots: Default::default(),
            slot_contributions: Default::default(),
            surface_layout: None,
            assets: None,
            translations: Default::default(),
        };
        let theme = mesh_theme::default_theme();
        let store = MapStore(
            [(
                "items".to_string(),
                serde_json::json!([{"name": "Alice"}, {"name": "Bob"}]),
            )]
            .into_iter()
            .collect(),
        );
        let compiled = CompiledFrontendPlugin {
            manifest,
            source_path: std::path::PathBuf::from("test.mesh"),
            component: plugin,
            local_components: Default::default(),
        };
        let tree = compiled.build_preview_tree_with_state(&theme, 400, 300, Some(&store));
        let texts = collect_text_content(&tree);
        assert!(
            texts.contains(&"Alice".to_string()),
            "expected Alice in {texts:?}"
        );
        assert!(
            texts.contains(&"Bob".to_string()),
            "expected Bob in {texts:?}"
        );
    }

    fn collect_text_content(node: &mesh_ui::WidgetNode) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(c) = node.attributes.get("content") {
            if !c.is_empty() {
                out.push(c.clone());
            }
        }
        for child in &node.children {
            out.extend(collect_text_content(child));
        }
        out
    }
}
