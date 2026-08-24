mod accessibility;
mod compile;
#[cfg(test)]
mod expr;
mod render;
mod style;
mod tags;

use mesh_core_component::ComponentFile;
#[cfg(test)]
use mesh_core_elements::HandlerTarget;
use mesh_core_elements::{
    ComponentCompositionProps, EventHandlerCall, LayoutEngine, NodeId, StyleContext, StyleResolver,
    VariableStore, WidgetNode,
};
use mesh_core_module::Manifest;
use mesh_core_theme::Theme;

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::PathBuf;

pub use accessibility::root_accessibility_role;
pub use compile::{
    CompileFrontendError, FrontendDiagnosticCategory, ImportCyclePath, compile_frontend_entrypoint,
    compile_frontend_module, is_frontend_module, validate_component_import_props,
};
pub use render::{
    PreparedComponentStyleRules, build_embedded_widget_tree_from_component,
    build_embedded_widget_tree_from_component_with_prepared_styles,
    build_embedded_widget_tree_from_component_with_prepared_styles_and_owner,
    build_widget_tree_from_component, props_settings_schema, resolve_css_props,
};
pub use tags::UiTag;

/// Shadows one variable with the current `{#for}` item, delegating the rest.
struct LayeredStore<'a> {
    base: &'a dyn VariableStore,
    item_name: &'a str,
    item_value: &'a serde_json::Value,
    loop_identity: Option<String>,
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
    fn loop_identity(&self) -> Option<&str> {
        self.loop_identity.as_deref()
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
        expression: &mesh_core_expression::CompiledExpression,
        locals: &serde_json::Map<String, serde_json::Value>,
    ) -> Option<TemplateExpressionResult>;

    fn render_import(
        &self,
        host: &Manifest,
        host_instance_key: &str,
        owner_source_path: Option<&std::path::Path>,
        alias: &str,
        source_ordinal: usize,
        duplicate_ordinal: Option<usize>,
        repeated_by_loop: bool,
        loop_identity: Option<&str>,
        props: &ComponentCompositionProps,
        prop_handler_calls: &BTreeMap<String, EventHandlerCall>,
        container_width: f32,
        container_height: f32,
    ) -> Option<WidgetNode>;

    fn render_slot(
        &self,
        host: &Manifest,
        host_instance_key: &str,
        extension_point: Option<&str>,
        slot_name: Option<&str>,
        customizable: bool,
        container_width: f32,
        container_height: f32,
    ) -> Vec<WidgetNode>;
}

pub struct TemplateExpressionResult {
    pub value: serde_json::Value,
    pub service_reads: Vec<(String, String)>,
}

pub fn collect_template_expressions(
    component: &ComponentFile,
) -> Vec<mesh_core_expression::SharedCompiledExpression> {
    component.template_expressions.clone()
}

#[derive(Debug, Clone)]
pub struct CompiledFrontendModule {
    pub manifest: Manifest,
    pub source_path: PathBuf,
    pub component: ComponentFile,
    /// The normalized public `<props>` declarations published at the compiled
    /// component boundary. Private declarations never cross an import.
    pub public_props: Vec<mesh_core_component::PropDef>,
    /// Owner-scoped records keyed by canonical owner/alias/target identities.
    ///
    /// Compiler-produced entries use only these scoped keys. The public map
    /// remains a compatibility surface for hand-built test fixtures, whose
    /// legacy alias-only entries are resolved only when no scoped record is
    /// available.
    pub local_components: std::collections::HashMap<String, mesh_core_component::ComponentFile>,
    /// Explicit component module imports. Direct imports use their alias;
    /// recursive imports use a canonical owner-scoped key.
    pub module_component_imports: std::collections::HashMap<String, String>,
    /// Every `.mesh` file that contributed, entrypoint and imports alike. The
    /// hot-reload watcher mtimes each, so editing any one triggers a recompile.
    pub watched_paths: Vec<PathBuf>,
}

impl CompiledFrontendModule {
    /// Return the published public prop schema, with a compatibility fallback
    /// for hand-built compiled fixtures from older callers.
    pub fn public_prop_schema(&self) -> Vec<mesh_core_component::PropDef> {
        if !self.public_props.is_empty() || self.component.props.is_none() {
            self.public_props.clone()
        } else {
            mesh_core_component::normalized_public_prop_schema(self.component.props.as_ref())
        }
    }
}

/// The resolved local component selected by an owner-scoped import binding.
///
/// `local_components` remains a compatibility index for callers that construct
/// compiled fixtures by hand. Compiler-produced modules resolve through the
/// canonical, owner-scoped records encoded in that index instead.
#[derive(Debug, Clone)]
pub struct ResolvedLocalComponent {
    pub source_path: PathBuf,
    pub component: ComponentFile,
}

const SCOPED_LOCAL_COMPONENT_PREFIX: &str = "\0mesh-local\0";
const SCOPED_MODULE_IMPORT_PREFIX: &str = "\0mesh-module\0";

pub(crate) fn scoped_local_component_key(
    owner: &std::path::Path,
    alias: &str,
    target: &std::path::Path,
) -> String {
    format!(
        "{SCOPED_LOCAL_COMPONENT_PREFIX}{}\0{}\0{}",
        owner.display(),
        alias,
        target.display()
    )
}

pub(crate) fn scoped_module_import_key(owner: &std::path::Path, alias: &str) -> String {
    format!(
        "{SCOPED_MODULE_IMPORT_PREFIX}{}\0{}",
        owner.display(),
        alias
    )
}

impl CompiledFrontendModule {
    /// Resolve a local import in the namespace of the component that authored
    /// it. The fallback is intentionally retained for older hand-built test
    /// fixtures that only populate `local_components`.
    pub fn local_component_for(
        &self,
        owner_source_path: Option<&std::path::Path>,
        alias: &str,
    ) -> Option<ResolvedLocalComponent> {
        let owner = owner_source_path.unwrap_or(&self.source_path);
        let mut scoped = self.local_components.iter().filter(|(key, _)| {
            scoped_local_component_parts(key).is_some_and(|(key_owner, key_alias, _)| {
                key_owner == owner.to_string_lossy().as_ref() && key_alias == alias
            })
        });
        if let Some((key, component)) = scoped.next() {
            // A compiler-produced catalog cannot contain this state because
            // insertion rejects a second target for one owner/alias. Treat a
            // malformed hand-built catalog as unresolved instead of choosing
            // an arbitrary HashMap entry.
            if scoped.next().is_some() {
                return None;
            }
            let target =
                scoped_local_component_parts(key).map(|(_, _, target)| PathBuf::from(target))?;
            return Some(ResolvedLocalComponent {
                source_path: target,
                component: component.clone(),
            });
        }

        if owner != self.source_path {
            return None;
        }
        self.local_components
            .get(alias)
            .map(|component| ResolvedLocalComponent {
                source_path: self
                    .watched_paths
                    .iter()
                    .find(|path| path.file_stem().and_then(|stem| stem.to_str()) == Some(alias))
                    .cloned()
                    .unwrap_or_else(|| self.source_path.clone()),
                component: component.clone(),
            })
    }

    /// Return a module import from its owner's namespace, without allowing a
    /// same alias in another recursive owner to shadow it.
    pub fn component_module_for(
        &self,
        owner_source_path: Option<&std::path::Path>,
        alias: &str,
    ) -> Option<String> {
        if let Some(owner) = owner_source_path {
            let key = scoped_module_import_key(owner, alias);
            if let Some(module_id) = self.module_component_imports.get(&key) {
                return Some(module_id.clone());
            }
        }
        if owner_source_path.is_some_and(|owner| owner != self.source_path) {
            return None;
        }
        self.module_component_imports.get(alias).cloned()
    }

    /// Whether this compiled root owns the component source path. This lets a
    /// contribution root and a primary root use the same local alias safely.
    pub fn owns_component_path(&self, source_path: &std::path::Path) -> bool {
        self.source_path == source_path
            || self.local_components.keys().any(|key| {
                scoped_local_component_parts(key)
                    .map(|(owner, _, _)| PathBuf::from(owner))
                    .is_some_and(|owner| owner == source_path)
            })
    }

    /// Whether `alias` is a local component in the requested owner scope.
    /// Compiler-produced records are always checked through the canonical
    /// owner key; alias-only fixtures are accepted only for the root scope.
    pub fn has_local_component(
        &self,
        owner_source_path: Option<&std::path::Path>,
        alias: &str,
    ) -> bool {
        self.local_component_for(owner_source_path, alias).is_some()
    }

    /// Iterate the recursive local-component records, falling back to the
    /// compatibility alias index for manually assembled fixtures.
    pub fn all_local_components(&self) -> Vec<&ComponentFile> {
        let mut scoped = self
            .local_components
            .iter()
            .filter(|(key, _)| scoped_local_component_parts(key).is_some())
            .collect::<Vec<_>>();
        scoped.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        let scoped = scoped
            .into_iter()
            .map(|(_, component)| component)
            .collect::<Vec<_>>();
        if scoped.is_empty() {
            self.local_components.values().collect()
        } else {
            scoped
        }
    }

    /// Return each recursively imported component together with the canonical
    /// source path of the component that owns its import namespace.
    pub fn local_component_sources(&self) -> Vec<(PathBuf, &ComponentFile)> {
        let mut records = self
            .local_components
            .iter()
            .filter_map(|(key, component)| {
                scoped_local_component_parts(key)
                    .map(|(owner, _, _)| (PathBuf::from(owner), component))
            })
            .collect::<Vec<_>>();
        records.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
        if records.is_empty() {
            records.extend(
                self.local_components
                    .values()
                    .map(|component| (self.source_path.clone(), component)),
            );
        }
        records
    }
}

fn scoped_local_component_parts(key: &str) -> Option<(&str, &str, &str)> {
    let value = key.strip_prefix(SCOPED_LOCAL_COMPONENT_PREFIX)?;
    let mut parts = value.split('\0');
    let owner = parts.next()?;
    let alias = parts.next()?;
    let target = parts.next()?;
    parts.next().is_none().then_some((owner, alias, target))
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
        self.build_tree_with_state_inner(
            theme,
            width,
            height,
            state,
            mode,
            instance_key,
            composition,
            measurer,
            None,
        )
    }

    /// Rebuild only `rebuild_node_ids`, reusing clean native subtrees from
    /// `previous`. Callers must gate this to statically shaped templates;
    /// component references still execute so memo side effects stay correct.
    #[allow(clippy::too_many_arguments)]
    pub fn build_tree_with_state_selective(
        &self,
        theme: &Theme,
        width: u32,
        height: u32,
        state: Option<&dyn VariableStore>,
        mode: FrontendRenderMode,
        instance_key: &str,
        composition: Option<&dyn FrontendCompositionResolver>,
        measurer: Option<&dyn mesh_core_elements::TextMeasurer>,
        previous: &WidgetNode,
        rebuild_node_ids: &HashSet<NodeId>,
    ) -> WidgetNode {
        self.build_tree_with_state_inner(
            theme,
            width,
            height,
            state,
            mode,
            instance_key,
            composition,
            measurer,
            Some((previous, rebuild_node_ids)),
        )
    }

    pub fn supports_selective_service_build(&self) -> bool {
        self.component
            .template
            .as_ref()
            .is_none_or(|template| !render::template_has_dynamic_structure(&template.root))
    }

    #[allow(clippy::too_many_arguments)]
    fn build_tree_with_state_inner(
        &self,
        theme: &Theme,
        width: u32,
        height: u32,
        state: Option<&dyn VariableStore>,
        mode: FrontendRenderMode,
        instance_key: &str,
        composition: Option<&dyn FrontendCompositionResolver>,
        measurer: Option<&dyn mesh_core_elements::TextMeasurer>,
        selective: Option<(&WidgetNode, &HashSet<NodeId>)>,
    ) -> WidgetNode {
        let _source_path_guard = render::ComponentSourcePathGuard::enter(Some(&self.source_path));
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
                .enumerate()
                .flat_map(|(index, node)| {
                    if matches!(
                        node,
                        mesh_core_component::template::TemplateNode::If(_)
                            | mesh_core_component::template::TemplateNode::For(_)
                    ) {
                        return render::build_widget_nodes(
                            node,
                            &self.manifest,
                            &build_style,
                            Some(&root.computed_style),
                            root_context,
                            state,
                            instance_key,
                            composition,
                        );
                    }
                    if let Some((previous, rebuild_node_ids)) = selective {
                        vec![render::build_widget_node_selective(
                            node,
                            &self.manifest,
                            &build_style,
                            Some(&root.computed_style),
                            root_context,
                            state,
                            instance_key,
                            composition,
                            previous.children.get(index),
                            rebuild_node_ids,
                        )]
                    } else {
                        vec![render::build_widget_node(
                            node,
                            &self.manifest,
                            &build_style,
                            Some(&root.computed_style),
                            root_context,
                            state,
                            instance_key,
                            composition,
                        )]
                    }
                })
                .collect();
        }

        // Embedded trees are laid out after composition, against the embedded
        // node's constraints — not the host's full bounds available here.
        if mode == FrontendRenderMode::Surface {
            LayoutEngine::compute_with_measurer(&mut root, width as f32, height as f32, measurer);
        }
        mesh_core_elements::normalize_accessibility(&mut root);
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
    fn shipped_settings_surface_compiles_with_local_pages() {
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
            .join("../../../../modules/frontend/settings");
        let loaded = mesh_core_module::manifest::load_canonical_manifest(&module_dir)
            .expect("settings manifest should load");
        let compiled = compile_frontend_module(&loaded.manifest, &module_dir)
            .expect("settings module should compile");

        for page in [
            "AdvancedPage",
            "AppearancePage",
            "AudioPage",
            "BluetoothPage",
            "DeviceInfoPage",
            "WifiPage",
        ] {
            assert!(
                compiled.has_local_component(None, page),
                "settings should register {page}"
            );
        }
        assert_eq!(compiled.watched_paths.len(), 7);

        // A hidden layer surface starts against a 1x1 safety configure, so
        // concrete constraints must still expose the intended content size.
        let tree = compiled.build_preview_tree(&mesh_core_theme::default_theme(), 1, 1);
        let settings_shell =
            first_node_with_class(&tree, "settings-shell").expect("settings-shell node");
        assert_eq!(
            (
                settings_shell.layout.width.round() as u32,
                settings_shell.layout.height.round() as u32,
            ),
            (920, 700)
        );
    }

    #[test]
    fn compiles_a_declared_alternate_frontend_entrypoint() {
        let module_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../modules/frontend/settings");
        let loaded = mesh_core_module::manifest::load_canonical_manifest(&module_dir)
            .expect("settings manifest should load");

        let compiled = compile_frontend_entrypoint(
            &loaded.manifest,
            &module_dir,
            "src/components/advanced-page.mesh",
        )
        .expect("an alternate frontend entrypoint should compile");

        assert!(
            compiled
                .source_path
                .ends_with("src/components/advanced-page.mesh")
        );
        assert_eq!(compiled.watched_paths, vec![compiled.source_path.clone()]);
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
        let loaded = mesh_core_module::manifest::load_canonical_manifest(&module_dir)
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
                span: None,
            },
            Attribute {
                name: "onchange".into(),
                value: AttributeValue::EventHandler("updateValue".into()),
                span: None,
            },
            Attribute {
                name: "onrelease".into(),
                value: AttributeValue::EventHandler("finishDrag".into()),
                span: None,
            },
            Attribute {
                name: "onfocus".into(),
                value: AttributeValue::EventHandler("focusControl".into()),
                span: None,
            },
        ];

        let (_, _, _, handlers, _) = render::parse_attributes(&attrs, None);

        assert_eq!(
            handlers.get("click").map(HandlerTarget::as_str),
            Some("openPanel")
        );
        assert_eq!(
            handlers.get("change").map(HandlerTarget::as_str),
            Some("updateValue")
        );
        assert_eq!(
            handlers.get("release").map(HandlerTarget::as_str),
            Some("finishDrag")
        );
        assert_eq!(
            handlers.get("focus").map(HandlerTarget::as_str),
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
            span: None,
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
            handlers.get("click").map(HandlerTarget::as_str),
            Some("toggleSurface")
        );
        assert_eq!(
            handlers.get("click").and_then(HandlerTarget::instance_key),
            Some("@test/root")
        );
    }

    #[test]
    fn resolves_bound_event_handler_props_from_state_strings() {
        let attrs = vec![Attribute {
            name: "onfocus".into(),
            value: AttributeValue::Binding("onFocusProxy".into()),
            span: None,
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
            handlers.get("focus").map(HandlerTarget::as_str),
            Some("markFocused")
        );
        assert_eq!(
            handlers.get("focus").and_then(HandlerTarget::instance_key),
            Some("@test/root")
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
        use std::cell::{Cell, RefCell};

        struct BorrowCountingStore {
            payload: serde_json::Value,
            owned_gets: Cell<usize>,
            borrowed_reads: RefCell<Vec<String>>,
        }

        impl mesh_core_elements::VariableStore for BorrowCountingStore {
            fn get(&self, name: &str) -> Option<serde_json::Value> {
                self.owned_gets.set(self.owned_gets.get() + 1);
                (name == "payload").then(|| self.payload.clone())
            }

            fn get_ref<'a>(&'a self, name: &str) -> Option<&'a serde_json::Value> {
                self.borrowed_reads.borrow_mut().push(name.to_string());
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
            borrowed_reads: RefCell::new(Vec::new()),
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
        assert_eq!(store.borrowed_reads.borrow().len(), 1);
        assert_eq!(store.borrowed_reads.borrow()[0], "payload");
    }

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
        assert_eq!(
            expr::eval_expr("count == '12'", &store),
            "false",
            "preview follows Luau's typed equality rather than coercing numbers to strings"
        );
        assert_eq!(expr::eval_expr("enabled and count > limit", &store), "true");
        assert_eq!(expr::eval_expr("not empty", &store), "false");
    }

    #[test]
    fn eval_expr_and_or_use_luau_values_truthiness_and_precedence() {
        let store = MapStore(
            [
                ("name".to_string(), serde_json::json!("Mesh")),
                ("zero".to_string(), serde_json::json!(0)),
                ("empty".to_string(), serde_json::json!("")),
                ("disabled".to_string(), serde_json::json!(false)),
                ("nothing".to_string(), serde_json::Value::Null),
                ("fallback".to_string(), serde_json::json!("fallback")),
                ("last".to_string(), serde_json::json!("last")),
            ]
            .into_iter()
            .collect(),
        );

        assert_eq!(expr::eval_expr("name or fallback", &store), "Mesh");
        assert_eq!(
            expr::eval_expr("missing or 'Anonymous'", &store),
            "Anonymous"
        );
        assert_eq!(expr::eval_expr("disabled or fallback", &store), "fallback");
        assert_eq!(expr::eval_expr("nothing or fallback", &store), "fallback");
        assert_eq!(expr::eval_expr("zero or fallback", &store), "0");
        assert_eq!(expr::eval_expr("empty or fallback", &store), "");
        assert_eq!(expr::eval_expr("zero and fallback", &store), "fallback");
        assert_eq!(expr::eval_expr("empty and fallback", &store), "fallback");
        assert_eq!(expr::eval_expr("disabled and fallback", &store), "false");

        assert_eq!(expr::eval_expr("name or disabled and last", &store), "Mesh");
        assert_eq!(
            expr::eval_expr("(name or disabled) and last", &store),
            "last"
        );
        assert_eq!(expr::eval_expr("false or fallback", &store), "fallback");
        assert_eq!(expr::eval_expr("nil or fallback", &store), "fallback");
        assert_eq!(expr::eval_expr("not zero", &store), "false");
        assert_eq!(expr::eval_expr("not empty", &store), "false");
    }

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
            hosted_extension_points: Default::default(),
            extension_point_contributions: Default::default(),
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
            public_props: Default::default(),
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
    fn for_node_children_expand_into_the_parent_without_a_wrapper() {
        // {#for} is layout-transparent: its active children join the surrounding
        // list directly. A synthetic `column` wrapper used to stand in for the
        // block, inheriting author-facing column theme defaults and spacing
        // every child.
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
            hosted_extension_points: Default::default(),
            extension_point_contributions: Default::default(),
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
            public_props: Default::default(),
            local_components: Default::default(),
            module_component_imports: Default::default(),
            watched_paths: Vec::new(),
        };
        let tree = compiled.build_preview_tree_with_state(&theme, 400, 300, Some(&store));
        fn find_tag<'a>(node: &'a WidgetNode, tag: &str) -> Option<&'a WidgetNode> {
            if node.tag == tag {
                return Some(node);
            }
            node.children.iter().find_map(|child| find_tag(child, tag))
        }
        assert!(
            find_tag(&tree, "column").is_none(),
            "{{#for}} must not introduce a synthetic wrapper node"
        );
        let parent = find_tag(&tree, "box").expect("authored box node");
        assert!(
            parent.children.iter().all(|child| child.tag == "text"),
            "loop bodies must be direct children of the authored parent, found {:?}",
            parent
                .children
                .iter()
                .map(|child| child.tag.as_str())
                .collect::<Vec<_>>()
        );
        assert_eq!(collect_text_content(parent), vec!["Alice".to_string()]);
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
                .map(HandlerTarget::as_str),
            Some("onFirst")
        );
        assert_eq!(
            embedded.children[0].children[0]
                .event_handlers
                .get("click")
                .and_then(HandlerTarget::instance_key),
            Some("test/embedded")
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
        let loaded = mesh_core_module::manifest::load_canonical_manifest(&module_dir)
            .expect("navigation manifest should load");
        let compiled = compile_frontend_module(&loaded.manifest, &module_dir)
            .expect("navigation module should compile");
        let theme = mesh_core_theme::default_theme();
        let state = MapStore(HashMap::from([(
            "props".to_string(),
            serde_json::json!({ "blur_enabled": true }),
        )]));
        let tree = compiled.build_preview_tree_with_state(&theme, 960, 56, Some(&state));
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

    #[test]
    #[ignore = "release-only embedded handler namespacing microbenchmark"]
    fn inline_handler_namespacing_beats_post_build_walk() {
        use std::time::Instant;

        fn legacy_namespace_walk(node: &mut WidgetNode, instance_key: &str) {
            for handler in node.event_handlers.values_mut() {
                handler.namespace(instance_key);
            }
            for call in node.event_handler_calls.values_mut() {
                call.handler.namespace(instance_key);
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

    #[test]
    fn component_css_fully_overrides_theme_primitive_defaults() {
        let compiled = make_test_module(
            r#"
<template>
  <row class="restyled-row">
    <button class="flat-button">Plain</button>
    <icon class="restyled-icon" name="test" />
  </row>
</template>
<style>
  .restyled-row {
    flex-direction: column;
    width: auto;
    height: auto;
    padding: 0;
    gap: 0;
  }
  .flat-button {
    padding: 0;
    gap: 0;
    border-radius: 0;
    background: transparent;
  }
  .restyled-icon {
    width: 31px;
    height: 29px;
    padding: 7px;
    border-radius: 9px;
    background: #123456;
  }
</style>
"#,
        );
        let theme = mesh_core_theme::default_theme();
        let tree = compiled.build_preview_tree(&theme, 400, 300);

        let row = find_first_by_tag(&tree, "row").expect("row");
        assert_eq!(
            row.computed_style.direction,
            mesh_core_elements::FlexDirection::Column
        );
        assert_eq!(
            row.computed_style.width,
            mesh_core_elements::Dimension::Auto
        );
        assert_eq!(
            row.computed_style.height,
            mesh_core_elements::Dimension::Auto
        );
        assert_eq!(
            row.computed_style.padding,
            mesh_core_elements::Edges::zero()
        );
        assert_eq!(row.computed_style.gap, 0.0);

        let button = find_first_by_tag(&tree, "button").expect("button");
        assert_eq!(
            button.computed_style.padding,
            mesh_core_elements::Edges::zero()
        );
        assert_eq!(button.computed_style.gap, 0.0);
        assert_eq!(
            button.computed_style.border_radius,
            mesh_core_elements::Corners::zero()
        );
        assert_eq!(
            button.computed_style.background_color,
            mesh_core_elements::Color::TRANSPARENT
        );

        let icon = find_first_by_tag(&tree, "icon").expect("icon");
        assert_eq!(
            icon.computed_style.width,
            mesh_core_elements::Dimension::Px(31.0)
        );
        assert_eq!(
            icon.computed_style.height,
            mesh_core_elements::Dimension::Px(29.0)
        );
        assert_eq!(
            icon.computed_style.padding,
            mesh_core_elements::Edges::all(7.0)
        );
        assert_eq!(
            icon.computed_style.border_radius,
            mesh_core_elements::Corners::all(9.0)
        );
        assert_eq!(
            icon.computed_style.background_color,
            mesh_core_elements::Color::from_hex("#123456").expect("color")
        );
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
            hosted_extension_points: Default::default(),
            extension_point_contributions: Default::default(),
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
            public_props: Default::default(),
            local_components: Default::default(),
            module_component_imports: Default::default(),
            watched_paths: Vec::new(),
        }
    }

    #[test]
    fn local_component_lookup_rejects_ambiguous_owner_alias_records() {
        let mut compiled = make_test_module("<template><box /></template>");
        let owner = std::path::PathBuf::from("/module/src/owner.mesh");
        let other_owner = std::path::PathBuf::from("/module/src/other.mesh");
        compiled.source_path = owner.clone();

        let first =
            mesh_core_component::parse_component("<template><text>A</text></template>").unwrap();
        let second =
            mesh_core_component::parse_component("<template><text>B</text></template>").unwrap();
        let other =
            mesh_core_component::parse_component("<template><text>C</text></template>").unwrap();
        compiled.local_components.insert(
            scoped_local_component_key(&owner, "Item", std::path::Path::new("/module/src/a.mesh")),
            first,
        );
        compiled.local_components.insert(
            scoped_local_component_key(&owner, "Item", std::path::Path::new("/module/src/b.mesh")),
            second,
        );
        compiled.local_components.insert(
            scoped_local_component_key(
                &other_owner,
                "Item",
                std::path::Path::new("/module/src/c.mesh"),
            ),
            other,
        );

        assert!(
            compiled.local_component_for(Some(&owner), "Item").is_none(),
            "an ambiguous owner/alias record must not select a HashMap entry"
        );
        let resolved = compiled
            .local_component_for(Some(&other_owner), "Item")
            .expect("a distinct owner may reuse the alias");
        assert_eq!(
            resolved.source_path,
            std::path::Path::new("/module/src/c.mesh")
        );
    }

    /// Computed styles must differ across a declared breakpoint.
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

        let narrow = compiled.build_preview_tree(&theme, 400, 300);
        let narrow_box = find_first_by_tag(&narrow, "box").expect("box node");
        assert_eq!(
            narrow_box.computed_style.background_color,
            mesh_core_elements::Color::from_hex("#111111").unwrap(),
            "narrow: container query should not apply"
        );

        let wide = compiled.build_preview_tree(&theme, 600, 300);
        let wide_box = find_first_by_tag(&wide, "box").expect("box node");
        assert_eq!(
            wide_box.computed_style.background_color,
            mesh_core_elements::Color::from_hex("#eeeeee").unwrap(),
            "wide: container query should apply"
        );
    }

    /// max-width queries match at small sizes and stop past the threshold.
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

        let narrow = compiled.build_preview_tree(&theme, 300, 200);
        let narrow_box = find_first_by_tag(&narrow, "box").expect("box node");
        assert_eq!(
            narrow_box.computed_style.background_color,
            mesh_core_elements::Color::from_hex("#aaaaaa").unwrap(),
            "narrow: max-width query should match"
        );

        let wide = compiled.build_preview_tree(&theme, 400, 200);
        let wide_box = find_first_by_tag(&wide, "box").expect("box node");
        assert_eq!(
            wide_box.computed_style.background_color,
            mesh_core_elements::Color::from_hex("#333333").unwrap(),
            "wide: max-width query should not match"
        );
    }

    /// No shared computed-style state may bleed between builds.
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
