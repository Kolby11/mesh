mod accessibility;
mod compile;
mod expr;
mod render;
mod style;
mod tags;

use mesh_core_component::ComponentFile;
use mesh_core_elements::{
    EventHandlerCall, LayoutEngine, StyleContext, StyleResolver, VariableStore, WidgetNode,
};
use mesh_core_module::Manifest;
use mesh_core_theme::Theme;

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::PathBuf;

pub use accessibility::root_accessibility_role;
pub use compile::{CompileFrontendError, compile_frontend_module, is_frontend_module};
pub use render::{
    PreparedComponentStyleRules, build_embedded_widget_tree_from_component,
    build_embedded_widget_tree_from_component_with_prepared_styles,
    build_widget_tree_from_component, props_settings_schema, resolve_css_props,
};
pub use style::merge_missing_defaults;
pub use tags::UiTag;

/// A `VariableStore` overlay used during `{#for}` iteration.
/// Shadows one variable name with the current loop item value while
/// delegating everything else to the underlying store.
struct LayeredStore<'a> {
    base: &'a dyn VariableStore,
    item_name: &'a str,
    item_value: &'a serde_json::Value,
}

impl VariableStore for LayeredStore<'_> {
    fn get(&self, name: &str) -> Option<serde_json::Value> {
        if name == self.item_name {
            Some(self.item_value.clone())
        } else {
            self.base.get(name)
        }
    }

    fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
        if name == self.item_name {
            Some(&self.item_value)
        } else {
            self.base.get_ref(name)
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

    fn template_locals(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut locals = self.base.template_locals();
        locals.insert(self.item_name.to_owned(), self.item_value.clone());
        locals
    }
    fn record_template_service_reads(&self, reads: &[(String, String)]) {
        self.base.record_template_service_reads(reads);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontendRenderMode {
    Surface,
    Embedded,
}

pub trait FrontendCompositionResolver {
    fn evaluate_template_expression(
        &self,
        instance_key: &str,
        expression: &str,
        locals: &serde_json::Map<String, serde_json::Value>,
    ) -> Option<TemplateExpressionResult>;

    fn render_import(
        &self,
        host: &Manifest,
        host_instance_key: &str,
        alias: &str,
        props: &BTreeMap<String, String>,
        prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
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

pub struct TemplateExpressionResult {
    pub value: serde_json::Value,
    pub service_reads: Vec<(String, String)>,
}

pub fn collect_template_expressions(component: &ComponentFile) -> Vec<String> {
    fn insert(expression: &str, seen: &mut HashSet<String>, out: &mut Vec<String>) {
        if seen.insert(expression.to_owned()) {
            out.push(expression.to_owned());
        }
    }
    fn attributes(
        attributes: &[mesh_core_component::template::Attribute],
        seen: &mut HashSet<String>,
        out: &mut Vec<String>,
    ) {
        use mesh_core_component::template::AttributeValue;
        for attribute in attributes {
            match &attribute.value {
                AttributeValue::Binding(expression) | AttributeValue::TwoWayBinding(expression) => {
                    insert(expression, seen, out)
                }
                AttributeValue::EventHandlerCall { args, .. } => {
                    for expression in args {
                        insert(expression, seen, out);
                    }
                }
                AttributeValue::Static(_)
                | AttributeValue::InstanceBinding(_)
                | AttributeValue::EventHandler(_) => {}
            }
        }
    }
    fn nodes(
        template_nodes: &[mesh_core_component::template::TemplateNode],
        seen: &mut HashSet<String>,
        out: &mut Vec<String>,
    ) {
        use mesh_core_component::template::TemplateNode;
        for node in template_nodes {
            match node {
                TemplateNode::Element(node) => {
                    attributes(&node.attributes, seen, out);
                    nodes(&node.children, seen, out);
                }
                TemplateNode::Component(node) => {
                    attributes(&node.props, seen, out);
                    nodes(&node.children, seen, out);
                }
                TemplateNode::Expr(node) => insert(&node.expression, seen, out),
                TemplateNode::If(node) => {
                    insert(&node.condition, seen, out);
                    nodes(&node.then_children, seen, out);
                    nodes(&node.else_children, seen, out);
                }
                TemplateNode::For(node) => {
                    insert(&node.iterable, seen, out);
                    nodes(&node.children, seen, out);
                }
                TemplateNode::Text(_) | TemplateNode::Slot(_) => {}
            }
        }
    }

    let mut seen = HashSet::new();
    let mut expressions = Vec::new();
    if let Some(template) = &component.template {
        nodes(&template.root, &mut seen, &mut expressions);
    }
    expressions
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
            let build_style = render::BuildStyleContext::new(rules, &resolver)
                .with_handler_namespacing(mode == FrontendRenderMode::Embedded);
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

        // Embedded trees are inserted into a surface tree and laid out there after
        // composition/finalization. Computing their geometry here is both wasted
        // work and potentially misleading: it uses the host's full bounds rather
        // than the embedded node's eventual constraints.
        if mode == FrontendRenderMode::Surface {
            LayoutEngine::compute_with_measurer(&mut root, width as f32, height as f32, measurer);
        }
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

        fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
            self.0.get(name)
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
    fn touch_gesture_proof_fixture_compiles_with_authoring_handlers() {
        fn gesture_pad(node: &WidgetNode) -> Option<&WidgetNode> {
            if node.attributes.get("class").is_some_and(|classes| {
                classes
                    .split_whitespace()
                    .any(|class| class == "gesture-pad")
            }) {
                return Some(node);
            }
            node.children.iter().find_map(gesture_pad)
        }

        let module_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../modules/frontend/touch-gesture-proof");
        let loaded = mesh_core_module::manifest::load_manifest(&module_dir)
            .expect("touch gesture proof manifest should load");
        let compiled = compile_frontend_module(&loaded.manifest, &module_dir)
            .expect("touch gesture proof module should compile");
        let theme = mesh_core_theme::default_theme();
        let tree = compiled.build_preview_tree(&theme, 380, 220);
        let pad = gesture_pad(&tree).expect("gesture proof pad");

        for handler in [
            "click",
            "swipe",
            "pinch",
            "hold",
            "touchstart",
            "touchmove",
            "touchend",
            "touchcancel",
            "tap",
            "doubletap",
            "longpress",
        ] {
            assert!(
                pad.event_handlers.contains_key(handler),
                "missing {handler}"
            );
        }
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

        let (_, _, _, handlers, _) = render::parse_attributes(&attrs, None);

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

        let (_, _, _, handlers, _) = render::parse_attributes(&attrs, Some(&store));

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

        let (_, _, resolved, handlers, _) = render::parse_attributes(&attrs, Some(&store));

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
    fn eval_expr_dotted_path_uses_borrowed_variable_lookup() {
        use std::cell::Cell;

        struct BorrowCountingStore {
            payload: serde_json::Value,
            owned_gets: Cell<usize>,
        }

        impl mesh_core_elements::VariableStore for BorrowCountingStore {
            fn get(&self, name: &str) -> Option<serde_json::Value> {
                self.owned_gets.set(self.owned_gets.get() + 1);
                (name == "payload").then(|| self.payload.clone())
            }

            fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
                (name == "payload").then_some(&self.payload)
            }

            fn keys(&self) -> Vec<String> {
                vec!["payload".to_string()]
            }
        }

        let store = BorrowCountingStore {
            payload: serde_json::json!({
                "metrics": {
                    "node": {
                        "bounds": {
                            "x": 42
                        }
                    }
                }
            }),
            owned_gets: Cell::new(0),
        };

        assert_eq!(
            expr::eval_expr("payload.metrics.node.bounds.x", &store),
            "42"
        );
        assert_eq!(
            store.owned_gets.get(),
            0,
            "borrowed dotted-path reads should not clone the root JSON value"
        );
    }

    // Run with:
    // cargo test -p mesh-core-frontend --release -- eval_expr_borrowed_path_beats_owned_clone --ignored --nocapture
    #[test]
    #[ignore]
    fn eval_expr_borrowed_path_beats_owned_clone() {
        use std::time::Instant;

        struct OwnedStore(HashMap<String, serde_json::Value>);
        impl mesh_core_elements::VariableStore for OwnedStore {
            fn get(&self, name: &str) -> Option<serde_json::Value> {
                self.0.get(name).cloned()
            }

            fn keys(&self) -> Vec<String> {
                self.0.keys().cloned().collect()
            }
        }

        let mut metrics = serde_json::Map::new();
        for index in 0..1_000usize {
            metrics.insert(
                format!("node_{index}"),
                serde_json::json!({
                    "x": index,
                    "y": index + 1,
                    "width": 20,
                    "height": 12,
                }),
            );
        }

        let payload = serde_json::Value::Object(metrics);
        let mut map = HashMap::new();
        map.insert("payload".to_string(), payload);
        let owned = OwnedStore(map.clone());
        let borrowed = MapStore(map);
        let iterations = 20_000usize;
        let expression = "payload.node_999.height";

        let owned_start = Instant::now();
        for _ in 0..iterations {
            assert_eq!(expr::eval_expr(expression, &owned), "12");
        }
        let owned_ns = owned_start.elapsed().as_nanos().max(1);

        let borrowed_start = Instant::now();
        for _ in 0..iterations {
            assert_eq!(expr::eval_expr(expression, &borrowed), "12");
        }
        let borrowed_ns = borrowed_start.elapsed().as_nanos();

        eprintln!("owned_clone={owned_ns}ns borrowed_ref={borrowed_ns}ns");
        assert!(
            borrowed_ns.saturating_mul(2) <= owned_ns,
            "borrowed path should be at least 2x faster for large JSON roots"
        );
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
    fn eval_expr_preserves_numeric_and_boolean_semantics() {
        let store = MapStore(
            [
                ("count".to_string(), serde_json::json!(12)),
                ("limit".to_string(), serde_json::json!(8)),
                ("ratio".to_string(), serde_json::json!(12.0)),
                ("enabled".to_string(), serde_json::json!(true)),
                ("empty".to_string(), serde_json::json!(0)),
            ]
            .into_iter()
            .collect(),
        );

        assert_eq!(expr::eval_expr("count", &store), "12");
        assert_eq!(
            expr::eval_expr("ratio", &store),
            serde_json::json!(12.0).to_string()
        );
        assert_eq!(expr::eval_expr("count > limit", &store), "true");
        assert_eq!(expr::eval_expr("count == '12'", &store), "true");
        assert_eq!(expr::eval_expr("enabled and count > limit", &store), "true");
        assert_eq!(expr::eval_expr("not empty", &store), "true");
    }

    // Run with:
    // cargo test -p mesh-core-frontend --release -- eval_expr_typed_compare_beats_string_parse_compare --ignored --nocapture
    #[test]
    #[ignore]
    fn eval_expr_typed_compare_beats_string_parse_compare() {
        use std::time::Instant;

        fn old_string_compare(left: &serde_json::Value, right: &serde_json::Value) -> bool {
            let left = match left {
                serde_json::Value::String(value) => value.clone(),
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            };
            let right = match right {
                serde_json::Value::String(value) => value.clone(),
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            };
            if let (Ok(left), Ok(right)) = (left.parse::<f64>(), right.parse::<f64>()) {
                left > right
            } else {
                false
            }
        }

        let store = MapStore(
            [
                ("count".to_string(), serde_json::json!(12.5)),
                ("limit".to_string(), serde_json::json!(8.25)),
            ]
            .into_iter()
            .collect(),
        );
        let left = serde_json::json!(12.5);
        let right = serde_json::json!(8.25);
        let iterations = 500_000usize;

        let old_start = Instant::now();
        let mut old_count = 0usize;
        for _ in 0..iterations {
            old_count += usize::from(old_string_compare(
                std::hint::black_box(&left),
                std::hint::black_box(&right),
            ));
        }
        let old_time = old_start.elapsed();

        let typed_start = Instant::now();
        let mut typed_count = 0usize;
        for _ in 0..iterations {
            typed_count += usize::from(
                expr::eval_expr("count > limit", std::hint::black_box(&store)) == "true",
            );
        }
        let typed_time = typed_start.elapsed();

        eprintln!(
            "typed expression compare: string-parse {old_time:?}; typed {typed_time:?}; ratio {:.1}x; counts={old_count}/{typed_count}",
            old_time.as_secs_f64() / typed_time.as_secs_f64()
        );
        assert_eq!(old_count, typed_count);
        assert!(typed_time < old_time);
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
            interfaces: Vec::new(),
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

    #[test]
    fn for_node_wrapper_carries_no_padding_or_gap() {
        // The synthetic <column> wrapper a {#for} block compiles into is
        // invisible authoring structure, not an author-styled container.
        // Regression test for a bug where it silently inherited
        // container_style's 12px padding / 8px gap, shifting every child
        // (worst for absolutely-positioned ones, since the padding offsets
        // their containing block).
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
            interfaces: Vec::new(),
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
            [("items".to_string(), serde_json::json!([{"name": "Alice"}]))]
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
        fn find_column(node: &WidgetNode) -> Option<&WidgetNode> {
            if node.tag == "column" {
                return Some(node);
            }
            node.children.iter().find_map(find_column)
        }
        let wrapper = find_column(&tree).expect("for-loop wrapper node");
        assert_eq!(
            wrapper.computed_style.padding,
            mesh_core_elements::Edges::zero()
        );
        assert_eq!(wrapper.computed_style.gap, 0.0);
    }

    #[test]
    fn for_node_borrows_iterable_without_owned_root_clone() {
        use std::cell::Cell;

        struct CountingStore {
            values: HashMap<String, serde_json::Value>,
            owned_gets: Cell<usize>,
        }

        impl mesh_core_elements::VariableStore for CountingStore {
            fn get(&self, name: &str) -> Option<serde_json::Value> {
                if name == "items" {
                    self.owned_gets.set(self.owned_gets.get() + 1);
                }
                self.values.get(name).cloned()
            }

            fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
                self.values.get(name)
            }

            fn keys(&self) -> Vec<String> {
                self.values.keys().cloned().collect()
            }
        }

        let source = r#"
<template>
  <box>
    {#for item in items}
      <text>{item.name}</text>
    {/for}
  </box>
</template>
"#;
        let compiled = make_test_module(source);
        let theme = mesh_core_theme::default_theme();
        let items = (0..256)
            .map(|index| serde_json::json!({ "name": format!("Item {index}") }))
            .collect::<Vec<_>>();
        let store = CountingStore {
            values: [("items".to_string(), serde_json::Value::Array(items))]
                .into_iter()
                .collect(),
            owned_gets: Cell::new(0),
        };

        let tree = compiled.build_preview_tree_with_state(&theme, 400, 300, Some(&store));
        let texts = collect_text_content(&tree);
        assert!(texts.contains(&"Item 255".to_string()));
        assert_eq!(
            store.owned_gets.get(),
            0,
            "loop iteration should borrow the iterable instead of cloning the root array"
        );
    }

    #[test]
    fn for_node_falls_back_to_owned_iterable_store() {
        struct OwnedOnlyStore(HashMap<String, serde_json::Value>);

        impl mesh_core_elements::VariableStore for OwnedOnlyStore {
            fn get(&self, name: &str) -> Option<serde_json::Value> {
                self.0.get(name).cloned()
            }

            fn keys(&self) -> Vec<String> {
                self.0.keys().cloned().collect()
            }
        }

        let source = r#"
<template>
  <box>
    {#for item in items}
      <text>{item.name}</text>
    {/for}
  </box>
</template>
"#;
        let compiled = make_test_module(source);
        let theme = mesh_core_theme::default_theme();
        let store = OwnedOnlyStore(
            [(
                "items".to_string(),
                serde_json::json!([{"name": "Fallback"}]),
            )]
            .into_iter()
            .collect(),
        );

        let tree = compiled.build_preview_tree_with_state(&theme, 400, 300, Some(&store));
        let texts = collect_text_content(&tree);
        assert!(
            texts.contains(&"Fallback".to_string()),
            "owned-only stores should continue to render loop items"
        );
    }

    // Run with:
    // cargo test -p mesh-core-frontend --release -- for_node_borrowed_iterable_beats_owned_array_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only for-loop iterable lookup microbenchmark"]
    fn for_node_borrowed_iterable_beats_owned_array_clone() {
        use std::time::Instant;

        struct OwnedOnlyStore(HashMap<String, serde_json::Value>);
        impl mesh_core_elements::VariableStore for OwnedOnlyStore {
            fn get(&self, name: &str) -> Option<serde_json::Value> {
                self.0.get(name).cloned()
            }

            fn keys(&self) -> Vec<String> {
                self.0.keys().cloned().collect()
            }
        }

        let source = r#"
<template>
  <box>
    {#for item in items}
      <row>
        <text>{item.name}</text>
        <text>{item.value}</text>
      </row>
    {/for}
  </box>
</template>
"#;
        let compiled = make_test_module(source);
        let theme = mesh_core_theme::default_theme();
        let unused_payload = "x".repeat(1_024);
        let items = (0..1_000)
            .map(|index| {
                serde_json::json!({
                    "name": format!("Item {index}"),
                    "value": index,
                    "unused": {
                        "description": unused_payload,
                        "metrics": (0..32).collect::<Vec<_>>()
                    }
                })
            })
            .collect::<Vec<_>>();
        let map = [("items".to_string(), serde_json::Value::Array(items))]
            .into_iter()
            .collect::<HashMap<_, _>>();
        let owned = OwnedOnlyStore(map.clone());
        let borrowed = MapStore(map);
        let iterations = 80usize;

        let owned_started = Instant::now();
        let mut owned_count = 0usize;
        for _ in 0..iterations {
            let tree = compiled.build_preview_tree_with_state(&theme, 400, 300, Some(&owned));
            owned_count = owned_count.wrapping_add(collect_text_content(&tree).len());
        }
        let owned_time = owned_started.elapsed();

        let borrowed_started = Instant::now();
        let mut borrowed_count = 0usize;
        for _ in 0..iterations {
            let tree = compiled.build_preview_tree_with_state(&theme, 400, 300, Some(&borrowed));
            borrowed_count = borrowed_count.wrapping_add(collect_text_content(&tree).len());
        }
        let borrowed_time = borrowed_started.elapsed();

        eprintln!(
            "for iterable lookup: owned clone {owned_time:?}; borrowed ref {borrowed_time:?}; ratio {:.1}x; counts={owned_count}/{borrowed_count}",
            owned_time.as_secs_f64() / borrowed_time.as_secs_f64()
        );
        assert_eq!(owned_count, borrowed_count);
        assert!(borrowed_time < owned_time);
    }

    #[test]
    fn embedded_build_defers_layout_until_surface_composition() {
        let compiled = make_test_module(
            r#"
<template>
  <column>
    <text onclick="onFirst">first</text>
    <text>second</text>
  </column>
</template>
"#,
        );
        let theme = mesh_core_theme::default_theme();

        let embedded = compiled.build_tree_with_state(
            &theme,
            400,
            300,
            None,
            FrontendRenderMode::Embedded,
            "test/embedded",
            None,
            None,
        );
        let surface = compiled.build_tree_with_state(
            &theme,
            400,
            300,
            None,
            FrontendRenderMode::Surface,
            "test/surface",
            None,
            None,
        );

        assert_eq!(embedded.layout.width, 0.0);
        assert_eq!(embedded.layout.height, 0.0);
        assert_eq!(
            embedded.children[0].children[0]
                .event_handlers
                .get("click")
                .map(String::as_str),
            Some("__mesh_embed__::test/embedded::onFirst")
        );
        assert!(surface.layout.width > 0.0);
        assert!(surface.layout.height > 0.0);
    }

    #[test]
    fn shipped_navigation_surface_root_spans_available_width() {
        fn first_node_with_class<'a>(
            node: &'a WidgetNode,
            class_name: &str,
        ) -> Option<&'a WidgetNode> {
            if node
                .attributes
                .get("class")
                .is_some_and(|classes| classes.split_whitespace().any(|class| class == class_name))
            {
                return Some(node);
            }
            node.children
                .iter()
                .find_map(|child| first_node_with_class(child, class_name))
        }

        let module_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../modules/frontend/navigation-bar");
        let loaded = mesh_core_module::manifest::load_manifest(&module_dir)
            .expect("navigation manifest should load");
        let compiled = compile_frontend_module(&loaded.manifest, &module_dir)
            .expect("navigation module should compile");
        let theme = mesh_core_theme::default_theme();
        let tree = compiled.build_preview_tree(&theme, 960, 56);
        let nav_shell = first_node_with_class(&tree, "nav-shell").expect("nav-shell node");

        assert_eq!(tree.layout.width.round() as u32, 960);
        assert_eq!(tree.layout.height.round() as u32, 56);
        assert_eq!(
            nav_shell.layout.width.round() as u32,
            960,
            "nav-shell should span the surface width, got {:?}",
            nav_shell.layout
        );
        assert_eq!(
            nav_shell.layout.height.round() as u32,
            56,
            "nav-shell should span the bar height, got {:?}",
            nav_shell.layout
        );
    }

    // Run with:
    // cargo test -p mesh-core-frontend --release -- embedded_build_layout_deferral_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only embedded layout deferral microbenchmark"]
    fn embedded_build_layout_deferral_benchmark() {
        use std::time::Instant;

        let rows = (0..256)
            .map(|index| format!("<row><text>label {index}</text><text>value {index}</text></row>"))
            .collect::<String>();
        let compiled = make_test_module(&format!("<template><column>{rows}</column></template>"));
        let theme = mesh_core_theme::default_theme();
        let iterations = 200usize;

        let deferred_started = Instant::now();
        let mut deferred_width = 0.0f32;
        for _ in 0..iterations {
            let tree = compiled.build_tree_with_state(
                &theme,
                1200,
                800,
                None,
                FrontendRenderMode::Embedded,
                "benchmark/embedded",
                None,
                None,
            );
            deferred_width += tree.layout.width;
        }
        let deferred_time = deferred_started.elapsed();

        let eager_started = Instant::now();
        let mut eager_width = 0.0f32;
        for _ in 0..iterations {
            let mut tree = compiled.build_tree_with_state(
                &theme,
                1200,
                800,
                None,
                FrontendRenderMode::Embedded,
                "benchmark/embedded",
                None,
                None,
            );
            LayoutEngine::compute(&mut tree, 1200.0, 800.0);
            eager_width += tree.layout.width;
        }
        let eager_time = eager_started.elapsed();

        eprintln!(
            "embedded build: eager layout {eager_time:?}; deferred {deferred_time:?}; ratio {:.1}x; widths={eager_width}/{deferred_width}",
            eager_time.as_secs_f64() / deferred_time.as_secs_f64()
        );
        assert!(eager_width > deferred_width);
        assert!(deferred_time < eager_time);
    }

    // Run with:
    // cargo test -p mesh-core-frontend --release -- inline_handler_namespacing_beats_post_build_walk --ignored --nocapture
    #[test]
    #[ignore = "release-only embedded handler namespacing microbenchmark"]
    fn inline_handler_namespacing_beats_post_build_walk() {
        use std::time::Instant;

        fn legacy_namespace_walk(node: &mut WidgetNode, instance_key: &str) {
            for handler in node.event_handlers.values_mut() {
                if !handler.starts_with("__mesh_embed__::") {
                    *handler = format!("__mesh_embed__::{instance_key}::{handler}");
                }
            }
            for call in node.event_handler_calls.values_mut() {
                if !call.handler.starts_with("__mesh_embed__::") {
                    call.handler = format!("__mesh_embed__::{instance_key}::{}", call.handler);
                }
            }
            for child in &mut node.children {
                legacy_namespace_walk(child, instance_key);
            }
        }

        let buttons = (0..512)
            .map(|index| format!(r#"<button onclick="onRow{index}">row {index}</button>"#))
            .collect::<String>();
        let compiled =
            make_test_module(&format!("<template><column>{buttons}</column></template>"));
        let theme = mesh_core_theme::default_theme();
        let base = compiled.build_tree_with_state(
            &theme,
            1200,
            800,
            None,
            FrontendRenderMode::Surface,
            "benchmark/root",
            None,
            None,
        );
        let iterations = 2_000usize;

        let inline_started = Instant::now();
        let mut inline_total = 0usize;
        for _ in 0..iterations {
            let tree = std::hint::black_box(base.clone());
            inline_total = inline_total.wrapping_add(tree.children.len());
        }
        let inline_time = inline_started.elapsed();

        let post_walk_started = Instant::now();
        let mut post_walk_total = 0usize;
        for _ in 0..iterations {
            let mut tree = std::hint::black_box(base.clone());
            legacy_namespace_walk(&mut tree, "benchmark/embedded");
            post_walk_total = post_walk_total.wrapping_add(tree.children.len());
        }
        let post_walk_time = post_walk_started.elapsed();

        eprintln!(
            "embedded handler namespacing: post-build walk {post_walk_time:?}; inline construction {inline_time:?}; ratio {:.1}x; totals={post_walk_total}/{inline_total}",
            post_walk_time.as_secs_f64() / inline_time.as_secs_f64()
        );
        assert_eq!(post_walk_total, inline_total);
        assert!(inline_time < post_walk_time);
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
            interfaces: Vec::new(),
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
