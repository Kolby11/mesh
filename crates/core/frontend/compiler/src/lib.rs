mod accessibility;
mod compile;
mod expr;
mod render;
mod style;
mod tags;

use mesh_core_component::ComponentFile;
use mesh_core_elements::{LayoutEngine, StyleContext, StyleResolver, VariableStore, WidgetNode};
use mesh_core_module::Manifest;
use mesh_core_theme::Theme;

use std::collections::BTreeMap;
use std::path::PathBuf;

pub use accessibility::root_accessibility_role;
pub use compile::{CompileFrontendError, compile_frontend_module, is_frontend_module};
pub use render::{build_widget_tree_from_component, props_settings_schema, resolve_css_props};
pub use style::merge_missing_defaults;
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
        props: &BTreeMap<String, String>,
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
pub struct CompiledFrontendModule {
    pub manifest: Manifest,
    pub source_path: PathBuf,
    pub component: ComponentFile,
    /// Local single-file components shipped in `src/components/*.mesh` inside
    /// the module directory. Keyed by filename stem (e.g. `settings-button`).
    pub local_components: std::collections::HashMap<String, mesh_core_component::ComponentFile>,
    /// Explicit component module imports keyed by template alias.
    pub module_component_imports: std::collections::HashMap<String, String>,
    /// On-disk paths of every `.mesh` source file that contributed to this
    /// compilation — entrypoint plus every locally imported component. The
    /// shell's hot-reload watcher mtimes each of these so editing any
    /// component triggers a recompile, not just the entrypoint.
    pub watched_paths: Vec<PathBuf>,
}

impl CompiledFrontendModule {
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
        measurer: Option<&dyn mesh_core_elements::TextMeasurer>,
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

        if let Some(accessibility) = &self.manifest.accessibility {
            if let Some(role) = accessibility.role.as_deref() {
                root.accessibility.role = accessibility::parse_accessibility_role(role);
            }
            root.accessibility.label = accessibility.label.clone();
            root.accessibility.description = accessibility.description.clone();
        }

        let resolver = StyleResolver::new(theme).with_props(render::resolve_css_props(
            self.component.props.as_ref(),
            state,
        ));
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
            let build_style = render::BuildStyleContext::new(rules, &resolver);
            root.children = template
                .root
                .iter()
                .map(|node| {
                    render::build_widget_node(
                        node,
                        &self.manifest,
                        &build_style,
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
    use mesh_core_component::template::{Attribute, AttributeValue};
    use std::collections::HashMap;

    struct MapStore(HashMap<String, serde_json::Value>);

    impl mesh_core_elements::VariableStore for MapStore {
        fn get(&self, name: &str) -> Option<serde_json::Value> {
            self.0.get(name).cloned()
        }

        fn keys(&self) -> Vec<String> {
            self.0.keys().cloned().collect()
        }
    }

    #[test]
    fn shipped_settings_surface_source_parses() {
        let source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../modules/frontend/settings/src/main.mesh"
        ));
        let component = mesh_core_component::parse_component(source).unwrap();

        assert!(component.template.is_some());
        assert!(component.script.is_some());
        assert!(component.style.is_some());
    }

    #[test]
    fn normalizes_on_prefixed_event_handler_names() {
        let attrs = vec![
            Attribute {
                name: "onclick".into(),
                value: AttributeValue::EventHandler("openPanel".into()),
            },
            Attribute {
                name: "onchange".into(),
                value: AttributeValue::EventHandler("updateValue".into()),
            },
            Attribute {
                name: "onrelease".into(),
                value: AttributeValue::EventHandler("finishDrag".into()),
            },
            Attribute {
                name: "onfocus".into(),
                value: AttributeValue::EventHandler("focusControl".into()),
            },
        ];

        let (_, _, _, handlers) = render::parse_attributes(&attrs, None);

        assert_eq!(handlers.get("click").map(String::as_str), Some("openPanel"));
        assert_eq!(
            handlers.get("change").map(String::as_str),
            Some("updateValue")
        );
        assert_eq!(
            handlers.get("release").map(String::as_str),
            Some("finishDrag")
        );
        assert_eq!(
            handlers.get("focus").map(String::as_str),
            Some("focusControl")
        );
        assert!(!handlers.contains_key("onclick"));
        assert!(!handlers.contains_key("onchange"));
        assert!(!handlers.contains_key("onrelease"));
        assert!(!handlers.contains_key("onfocus"));
    }

    #[test]
    fn resolves_event_handler_props_from_state_strings() {
        let attrs = vec![Attribute {
            name: "onclick".into(),
            value: AttributeValue::EventHandler("onActivate".into()),
        }];
        let store = MapStore(
            [(
                "onActivate".to_string(),
                serde_json::json!("__mesh_embed__::@test/root::toggleSurface"),
            )]
            .into_iter()
            .collect(),
        );

        let (_, _, _, handlers) = render::parse_attributes(&attrs, Some(&store));

        assert_eq!(
            handlers.get("click").map(String::as_str),
            Some("__mesh_embed__::@test/root::toggleSurface")
        );
    }

    #[test]
    fn resolves_bound_event_handler_props_from_state_strings() {
        let attrs = vec![Attribute {
            name: "onfocus".into(),
            value: AttributeValue::Binding("onFocusProxy".into()),
        }];
        let store = MapStore(
            [(
                "onFocusProxy".to_string(),
                serde_json::json!("__mesh_embed__::@test/root::markFocused"),
            )]
            .into_iter()
            .collect(),
        );

        let (_, _, resolved, handlers) = render::parse_attributes(&attrs, Some(&store));

        assert!(resolved.is_empty());
        assert_eq!(
            handlers.get("focus").map(String::as_str),
            Some("__mesh_embed__::@test/root::markFocused")
        );
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
        let module = mesh_core_component::parse_component(source).unwrap();
        let manifest = mesh_core_module::Manifest {
            package: mesh_core_module::ModuleSection {
                id: "test".into(),
                version: "0.1.0".into(),
                module_type: mesh_core_module::ModuleType::Widget,
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
            keybinds: Default::default(),
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
            icons: None,
            icon_pack: None,
            icon_requirements: Default::default(),
            translations: Default::default(),
        };
        let theme = mesh_core_theme::default_theme();
        let store = MapStore(
            [(
                "items".to_string(),
                serde_json::json!([{"name": "Alice"}, {"name": "Bob"}]),
            )]
            .into_iter()
            .collect(),
        );
        let compiled = CompiledFrontendModule {
            manifest,
            source_path: std::path::PathBuf::from("test.mesh"),
            component: module,
            local_components: Default::default(),
            module_component_imports: Default::default(),
            watched_paths: Vec::new(),
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

    fn collect_text_content(node: &mesh_core_elements::WidgetNode) -> Vec<String> {
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

    fn find_first_by_tag<'a>(
        node: &'a mesh_core_elements::WidgetNode,
        tag: &str,
    ) -> Option<&'a mesh_core_elements::WidgetNode> {
        if node.tag == tag {
            return Some(node);
        }
        for child in &node.children {
            if let Some(found) = find_first_by_tag(child, tag) {
                return Some(found);
            }
        }
        None
    }

    fn make_test_module(source: &str) -> CompiledFrontendModule {
        let component = mesh_core_component::parse_component(source).unwrap();
        let manifest = mesh_core_module::Manifest {
            package: mesh_core_module::ModuleSection {
                id: "test".into(),
                version: "0.1.0".into(),
                module_type: mesh_core_module::ModuleType::Widget,
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
            keybinds: Default::default(),
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
            icons: None,
            icon_pack: None,
            icon_requirements: Default::default(),
            translations: Default::default(),
        };
        CompiledFrontendModule {
            manifest,
            source_path: std::path::PathBuf::from("test.mesh"),
            component,
            local_components: Default::default(),
            module_component_imports: Default::default(),
            watched_paths: Vec::new(),
        }
    }

    /// Container query rules must produce different computed styles when the
    /// root surface size crosses a declared breakpoint.
    #[test]
    fn container_query_applies_different_styles_at_different_root_sizes() {
        let source = r#"
<style>
box {
  background-color: #111111;
  width: 100px;
}
@container (min-width: 500px) {
  box {
    background-color: #eeeeee;
    width: 200px;
  }
}
</style>
<template>
  <box />
</template>
"#;
        let compiled = make_test_module(source);
        let theme = mesh_core_theme::default_theme();

        // Narrow — container query does not match.
        let narrow = compiled.build_preview_tree(&theme, 400, 300);
        let narrow_box = find_first_by_tag(&narrow, "box").expect("box node");
        assert_eq!(
            narrow_box.computed_style.background_color,
            mesh_core_elements::Color::from_hex("#111111").unwrap(),
            "narrow: container query should not apply"
        );

        // Wide — container query matches.
        let wide = compiled.build_preview_tree(&theme, 600, 300);
        let wide_box = find_first_by_tag(&wide, "box").expect("box node");
        assert_eq!(
            wide_box.computed_style.background_color,
            mesh_core_elements::Color::from_hex("#eeeeee").unwrap(),
            "wide: container query should apply"
        );
    }

    /// max-width container queries invert correctly — they match at small
    /// sizes and stop matching when the surface grows past the threshold.
    #[test]
    fn container_query_max_width_inverts_across_breakpoint() {
        let source = r#"
<style>
box {
  background-color: #333333;
}
@container (max-width: 319px) {
  box {
    background-color: #aaaaaa;
  }
}
</style>
<template>
  <box />
</template>
"#;
        let compiled = make_test_module(source);
        let theme = mesh_core_theme::default_theme();

        // Narrow — max-width matches.
        let narrow = compiled.build_preview_tree(&theme, 300, 200);
        let narrow_box = find_first_by_tag(&narrow, "box").expect("box node");
        assert_eq!(
            narrow_box.computed_style.background_color,
            mesh_core_elements::Color::from_hex("#aaaaaa").unwrap(),
            "narrow: max-width query should match"
        );

        // Wide — max-width does not match.
        let wide = compiled.build_preview_tree(&theme, 400, 200);
        let wide_box = find_first_by_tag(&wide, "box").expect("box node");
        assert_eq!(
            wide_box.computed_style.background_color,
            mesh_core_elements::Color::from_hex("#333333").unwrap(),
            "wide: max-width query should not match"
        );
    }

    /// Building the same module twice with different surface sizes must yield
    /// independent trees — no shared computed-style state bleeds between calls.
    #[test]
    fn container_query_consecutive_builds_are_independent() {
        let source = r#"
<style>
box { background-color: #000000; }
@container (min-width: 400px) {
  box { background-color: #ffffff; }
}
</style>
<template><box /></template>
"#;
        let compiled = make_test_module(source);
        let theme = mesh_core_theme::default_theme();

        let wide = compiled.build_preview_tree(&theme, 500, 200);
        let narrow = compiled.build_preview_tree(&theme, 300, 200);

        let wide_bg = find_first_by_tag(&wide, "box")
            .unwrap()
            .computed_style
            .background_color;
        let narrow_bg = find_first_by_tag(&narrow, "box")
            .unwrap()
            .computed_style
            .background_color;

        assert_ne!(
            wide_bg, narrow_bg,
            "builds at different sizes must produce different styles"
        );
        assert_eq!(
            wide_bg,
            mesh_core_elements::Color::from_hex("#ffffff").unwrap()
        );
        assert_eq!(
            narrow_bg,
            mesh_core_elements::Color::from_hex("#000000").unwrap()
        );
    }
}
