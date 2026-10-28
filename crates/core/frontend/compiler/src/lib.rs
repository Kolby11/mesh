mod accessibility;
mod bindings;
mod compile;
mod render;
mod style;
mod tags;

use mesh_core_component::ComponentFile;
use mesh_core_elements::HandlerTarget;
use mesh_core_elements::Prop;
use mesh_core_elements::{
    AccessibilityInfo, ComponentCompositionProps, EventHandlerCall, LayoutEngine, NodeId,
    StyleContext, StyleResolver, VariableStore, WidgetNode, validate_widget_tree,
};
use mesh_core_module::Manifest;
use mesh_core_theme::Theme;

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::Arc;

pub use accessibility::root_accessibility_role;
pub use compile::{
    CompileFrontendError, FrontendDiagnosticCategory, ImportCyclePath, compile_frontend_entrypoint,
    compile_frontend_entrypoint_revision, compile_frontend_module,
    compile_frontend_module_revision, is_frontend_module, validate_component_import_props,
};
pub use render::{
    PreparedComponentStyleRules, TemplateBindingPatch, build_embedded_widget_tree_from_component,
    build_embedded_widget_tree_from_component_with_prepared_styles,
    build_embedded_widget_tree_from_component_with_prepared_styles_and_owner,
    build_widget_tree_from_component, keyed_template_list_member, patch_template_bindings,
    props_settings_schema, refresh_keyed_template_items, refresh_keyed_template_list,
    resolve_css_props,
};
pub use tags::UiTag;

/// Shadows one variable with the current `{#for}` item, delegating the rest.
struct LayeredStore<'a> {
    base: &'a dyn VariableStore,
    item_name: &'a str,
    item_value: &'a serde_json::Value,
    loop_identity: Option<String>,
    keyed_item: Option<mesh_core_elements::KeyedTemplateItem>,
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
    fn keyed_template_item(&self) -> Option<mesh_core_elements::KeyedTemplateItem> {
        self.keyed_item.clone()
    }
    fn in_loop(&self) -> bool {
        true
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
    fn bind_handler_expression(
        &self,
        _instance_key: &str,
        _expression: &mesh_core_expression::CompiledExpression,
        _locals: &serde_json::Map<String, serde_json::Value>,
        _source: &str,
        _call_binding: bool,
    ) -> Option<HandlerTarget> {
        None
    }

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
    use mesh_core_component::template::{AttributeValue, TemplateNode};
    fn visit(
        nodes: &[TemplateNode],
        expressions: &mut Vec<mesh_core_expression::SharedCompiledExpression>,
    ) {
        for node in nodes {
            match node {
                TemplateNode::Element(element) => {
                    if let Ok(Some(lowered)) = bindings::lower_value_bindings(element) {
                        for attr in &lowered.attributes {
                            if let AttributeValue::EventHandlerExpression { expression, .. } =
                                &attr.value
                            {
                                if !expressions
                                    .iter()
                                    .any(|existing| existing.source() == expression.source())
                                {
                                    expressions.push(expression.clone());
                                }
                            }
                        }
                    }
                    visit(&element.children, expressions);
                }
                TemplateNode::Component(component) => visit(&component.children, expressions),
                TemplateNode::If(branch) => {
                    visit(&branch.then_children, expressions);
                    visit(&branch.else_children, expressions);
                }
                TemplateNode::For(repeat) => visit(&repeat.children, expressions),
                _ => {}
            }
        }
    }
    let mut expressions = component.template_expressions.clone();
    if let Some(template) = &component.template {
        visit(&template.root, &mut expressions);
    }
    expressions
}

/// Only native, fixed-shape bodies can reuse their binding targets after an
/// item changes. Structural consumers and captured handler arguments rebuild.
fn patchable_keyed_loop(repetition: &mesh_core_component::template::ForNode) -> bool {
    use mesh_core_component::template::{AttributeValue, TemplateNode};
    fn native(nodes: &[TemplateNode]) -> bool {
        nodes.iter().all(|node| match node {
            TemplateNode::Element(element) => {
                element.attributes.iter().all(|attribute| {
                    // The patch re-binds handler expressions to refreshed locals.
                    if matches!(
                        attribute.value,
                        AttributeValue::EventHandlerExpression { .. }
                    ) {
                        return true;
                    }
                    !attribute.name.starts_with("on")
                        && !matches!(
                            attribute.value,
                            AttributeValue::InstanceBinding(_)
                                | AttributeValue::TwoWayBinding(_)
                                | AttributeValue::EventHandlerExpression { .. }
                                | AttributeValue::EventHandlerCall { .. }
                        )
                }) && native(&element.children)
            }
            TemplateNode::Text(_) | TemplateNode::Expr(_) => true,
            _ => false,
        })
    }
    repetition.key.is_some()
        && repetition
            .children
            .iter()
            .any(|node| matches!(node, TemplateNode::Element(_) | TemplateNode::Expr(_)))
        && native(&repetition.children)
}

/// Count child-prop source occurrences so an element binding cannot mask an
/// unsupported or currently absent component boundary reading the same source.
pub fn collect_component_prop_binding_counts(
    component: &ComponentFile,
) -> std::collections::HashMap<String, usize> {
    use mesh_core_component::template::{AttributeValue, TemplateNode};
    fn visit(nodes: &[TemplateNode], counts: &mut std::collections::HashMap<String, usize>) {
        for node in nodes {
            match node {
                TemplateNode::Component(component) => {
                    for attribute in &component.props {
                        if !attribute.name.starts_with("on")
                            && let AttributeValue::Binding(source) = &attribute.value
                        {
                            *counts.entry(source.trim().to_owned()).or_default() += 1;
                        }
                    }
                    visit(&component.children, counts);
                }
                TemplateNode::Element(element) => visit(&element.children, counts),
                TemplateNode::If(branch) => {
                    visit(&branch.then_children, counts);
                    visit(&branch.else_children, counts);
                }
                TemplateNode::For(repeat) => visit(&repeat.children, counts),
                _ => {}
            }
        }
    }
    let mut counts = std::collections::HashMap::new();
    if let Some(template) = &component.template {
        visit(&template.root, &mut counts);
    }
    counts
}

/// Expressions whose consumers require template evaluation rather than a
/// property patch, even when the same source also occurs in an element binding.
pub fn collect_unpatchable_template_expressions(component: &ComponentFile) -> HashSet<String> {
    use mesh_core_component::template::{Attribute, AttributeValue, TemplateNode};
    fn attributes(attributes: &[Attribute], all: bool, out: &mut HashSet<String>) {
        for attribute in attributes {
            match &attribute.value {
                AttributeValue::Binding(source) | AttributeValue::TwoWayBinding(source)
                    if all || attribute.name.starts_with("on") =>
                {
                    out.insert(source.trim().to_owned());
                }
                AttributeValue::EventHandlerExpression { expression, .. } => {
                    out.insert(expression.source().to_owned());
                }
                AttributeValue::EventHandlerCall { args, .. } => {
                    out.extend(args.iter().map(|source| source.trim().to_owned()));
                }
                _ => {}
            }
        }
    }
    fn visit(nodes: &[TemplateNode], unkeyed: bool, out: &mut HashSet<String>) {
        for node in nodes {
            match node {
                TemplateNode::Element(element) => {
                    attributes(&element.attributes, unkeyed, out);
                    if let Ok(Some(lowered)) = bindings::lower_value_bindings(element) {
                        attributes(&lowered.attributes, unkeyed, out);
                    }
                    visit(&element.children, unkeyed, out);
                }
                TemplateNode::Component(component) => {
                    attributes(&component.props, unkeyed || component.repeated_by_loop, out);
                    for attribute in &component.props {
                        if let AttributeValue::TwoWayBinding(source) = &attribute.value {
                            out.insert(source.trim().to_owned());
                        }
                    }
                    visit(&component.children, unkeyed, out);
                }
                TemplateNode::If(condition) => {
                    out.insert(condition.condition.trim().to_owned());
                    visit(&condition.then_children, unkeyed, out);
                    visit(&condition.else_children, unkeyed, out);
                }
                TemplateNode::For(repetition) => {
                    if unkeyed || !patchable_keyed_loop(repetition) {
                        out.insert(repetition.iterable.trim().to_owned());
                    }
                    out.extend(repetition.key.iter().map(|source| source.trim().to_owned()));
                    visit(
                        &repetition.children,
                        unkeyed || !patchable_keyed_loop(repetition),
                        out,
                    );
                }
                TemplateNode::Expr(expression) if unkeyed => {
                    out.insert(expression.expression.trim().to_owned());
                }
                _ => {}
            }
        }
    }
    let mut out = HashSet::new();
    if let Some(template) = &component.template {
        visit(&template.root, false, &mut out);
    }
    out
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

/// An immutable, content-addressed compilation result.
///
/// The compiler still exposes [`CompiledFrontendModule`] for tree-building and
/// compatibility with hand-built fixtures. Production catalog publication
/// uses this wrapper so a compiled root can be shared across catalog snapshots
/// without exposing a mutable compilation to readers. The revision changes
/// whenever the normalized manifest or one of the watched source files
/// changes.
#[derive(Debug, Clone)]
pub struct CompiledFrontendRevision {
    revision: u64,
    compiled: Arc<CompiledFrontendModule>,
}

impl CompiledFrontendRevision {
    pub fn new(revision: u64, compiled: CompiledFrontendModule) -> Self {
        Self {
            revision,
            compiled: Arc::new(compiled),
        }
    }

    pub fn from_module(compiled: CompiledFrontendModule) -> Self {
        let revision = compiled_revision(&compiled);
        Self::new(revision, compiled)
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn as_module(&self) -> &CompiledFrontendModule {
        &self.compiled
    }

    /// Copy-on-write access is retained for older fixture builders. Runtime
    /// and catalog consumers only receive the shared immutable reference.
    pub fn make_mut(&mut self) -> &mut CompiledFrontendModule {
        Arc::make_mut(&mut self.compiled)
    }
}

impl Deref for CompiledFrontendRevision {
    type Target = CompiledFrontendModule;

    fn deref(&self) -> &Self::Target {
        self.as_module()
    }
}

impl DerefMut for CompiledFrontendRevision {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.make_mut()
    }
}

fn compiled_revision(compiled: &CompiledFrontendModule) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(&compiled.manifest)
        .expect("normalized module manifests serialize")
        .hash(&mut hasher);
    let mut paths = compiled.watched_paths.clone();
    paths.sort();
    for path in paths {
        path.hash(&mut hasher);
        match std::fs::read(&path) {
            Ok(source) => source.hash(&mut hasher),
            Err(error) => error.kind().hash(&mut hasher),
        }
    }
    hasher.finish()
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

    /// Resolve the component that authored a node by its canonical source path.
    pub fn component_at_source(&self, source: &std::path::Path) -> Option<&ComponentFile> {
        if self.source_path == source {
            return Some(&self.component);
        }
        self.local_components.iter().find_map(|(key, component)| {
            scoped_local_component_parts(key)
                .filter(|(_, _, target)| std::path::Path::new(target) == source)
                .map(|_| component)
        })
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
            false,
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
            false,
        )
    }

    /// Build a surface for a host that will annotate, restyle, lay out and
    /// normalize it before use. Geometry here is provisional; previews should
    /// use `build_tree_with_state` instead. Selective builds require the same
    /// statically shaped template guard as `build_tree_with_state_selective`.
    #[allow(clippy::too_many_arguments)]
    pub fn build_tree_for_finalization(
        &self,
        theme: &Theme,
        width: u32,
        height: u32,
        state: Option<&dyn VariableStore>,
        instance_key: &str,
        composition: &dyn FrontendCompositionResolver,
        selective: Option<(&WidgetNode, &HashSet<NodeId>)>,
    ) -> WidgetNode {
        self.build_tree_with_state_inner(
            theme,
            width,
            height,
            state,
            FrontendRenderMode::Surface,
            instance_key,
            Some(composition),
            None,
            selective,
            true,
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
        defer_layout: bool,
    ) -> WidgetNode {
        let _source_path_guard = render::ComponentSourcePathGuard::enter(Some(&self.source_path));
        let mut root = WidgetNode::new("surface");
        root.attributes
            .insert(Prop::Id, self.manifest.package.id.clone());
        root.computed_style = match mode {
            FrontendRenderMode::Surface => {
                style::surface_style(theme, &self.manifest.package.id, width, height)
            }
            FrontendRenderMode::Embedded => style::embedded_root_style(theme),
        };

        if let Some(accessibility) = &self.manifest.accessibility {
            if let Some(role) = accessibility.role.as_deref() {
                root.accessibility.role = accessibility::parse_accessibility_role(role);
            }
            root.accessibility.label = accessibility.label.clone();
            root.accessibility.description = accessibility.description.clone();
        }
        let mut accessibility_baseline = AccessibilityInfo::default();
        accessibility_baseline.role = root.accessibility.role.clone();
        accessibility_baseline.label = root.accessibility.label.clone();
        accessibility_baseline.description = root.accessibility.description.clone();
        root.set_accessibility_baseline(accessibility_baseline);

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
            if let Some(surface_root) = root.children.first_mut() {
                if mode == FrontendRenderMode::Surface {
                    surface_root.mark_surface_root();
                } else {
                    surface_root.clear_surface_root();
                }
            }
        }

        // Do not hand duplicate or unknown nodes to layout. Compiler template
        // validation rejects unknown source tags; this guard also covers
        // runtime composition paths that can reuse an authored subtree.
        if let Err(error) = validate_widget_tree(&root) {
            tracing::error!(
                target: "mesh::frontend::compiler",
                error = %error,
                "rejecting invalid widget tree before compiler layout"
            );
            root.children.clear();
        }

        // Embedded trees are laid out after composition, against the embedded
        // node's constraints — not the host's full bounds available here.
        if mode == FrontendRenderMode::Surface && !defer_layout {
            LayoutEngine::compute_with_measurer(&mut root, width as f32, height as f32, measurer);
        }
        mesh_core_elements::normalize_accessibility_before_finalization(&mut root);
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
                .get(Prop::Class)
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
            "KeyboardPage",
            "WifiPage",
        ] {
            assert!(
                compiled.has_local_component(None, page),
                "settings should register {page}"
            );
        }
        assert_eq!(compiled.watched_paths.len(), 8);

        let tree = compiled.build_preview_tree(&mesh_core_theme::default_theme(), 1280, 900);
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
    fn compiled_frontend_revision_is_shared_and_content_addressed() {
        let module_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../modules/frontend/settings");
        let loaded = mesh_core_module::manifest::load_canonical_manifest(&module_dir)
            .expect("settings manifest should load");
        let revision = compile_frontend_module_revision(&loaded.manifest, &module_dir)
            .expect("settings module should compile as an immutable revision");

        assert_ne!(revision.revision(), 0);
        assert_eq!(revision.as_module().manifest.package.id, "@mesh/settings");
        assert!(std::ptr::eq(revision.as_module(), &*revision));
    }

    #[test]
    fn touch_gesture_proof_fixture_compiles_with_authoring_handlers() {
        fn gesture_pad(node: &WidgetNode) -> Option<&WidgetNode> {
            if node.attributes.get(Prop::Class).is_some_and(|classes| {
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
                quoted_expression: false,
                span: None,
            },
            Attribute {
                name: "onchange".into(),
                value: AttributeValue::EventHandler("updateValue".into()),
                quoted_expression: false,
                span: None,
            },
            Attribute {
                name: "onrelease".into(),
                value: AttributeValue::EventHandler("finishDrag".into()),
                quoted_expression: false,
                span: None,
            },
            Attribute {
                name: "onfocus".into(),
                value: AttributeValue::EventHandler("focusControl".into()),
                quoted_expression: false,
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
            quoted_expression: false,
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
            quoted_expression: false,
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
            embedded.computed_style.direction,
            mesh_core_elements::style::FlexDirection::Column
        );
        assert_eq!(
            embedded.computed_style.align_self,
            mesh_core_elements::style::AlignSelf::Stretch
        );
        assert_eq!(
            embedded.computed_style.justify_content,
            mesh_core_elements::style::JustifyContent::Center
        );
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
    fn surface_roots_inherit_the_active_ui_font_family() {
        let compiled = make_test_module("<template><text>font test</text></template>");
        let mut theme = mesh_core_theme::default_theme();
        theme.set_token(
            "typography.family",
            mesh_core_theme::TokenValue::String("DejaVu Serif".into()),
            mesh_core_theme::ThemeProvenance::UserOverride,
        );

        let surface = compiled.build_preview_tree(&theme, 400, 300);
        let embedded = compiled.build_tree_with_state(
            &theme,
            400,
            300,
            None,
            FrontendRenderMode::Embedded,
            "test/embedded-font",
            None,
            None,
        );

        for tree in [&surface, &embedded] {
            assert_eq!(tree.computed_style.font_family.as_ref(), "DejaVu Serif");
            assert_eq!(
                tree.children[0].computed_style.font_family.as_ref(),
                "DejaVu Serif"
            );
        }
    }

    #[test]
    fn shipped_navigation_surface_root_spans_available_width() {
        fn first_node_with_class<'a>(
            node: &'a WidgetNode,
            class_name: &str,
        ) -> Option<&'a WidgetNode> {
            if node
                .attributes
                .get(Prop::Class)
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
        let tree = compiled.build_preview_tree_with_state(&theme, 960, 40, Some(&state));
        let nav_shell = first_node_with_class(&tree, "nav-shell").expect("nav-shell node");

        assert_eq!(tree.layout.width.round() as u32, 960);
        assert_eq!(tree.layout.height.round() as u32, 40);
        assert_eq!(
            nav_shell.layout.width.round() as u32,
            960,
            "nav-shell should span the surface width, got {:?}",
            nav_shell.layout
        );
        assert_eq!(
            nav_shell.layout.x.round() as u32,
            0,
            "surface margin must not shift the pixels inside the buffer",
        );
        assert_eq!(
            nav_shell.layout.y.round() as u32,
            0,
            "surface margin must be lowered to compositor placement",
        );
        assert_eq!(
            nav_shell.layout.height.round() as u32,
            40,
            "nav-shell should span the bar height, got {:?}",
            nav_shell.layout
        );

        // Control size is derived, not restated: the cluster row takes the bar
        // height minus its block padding, and each control takes the row.
        let cluster = first_node_with_class(&tree, "right-cluster").expect("right-cluster node");
        assert_eq!(
            cluster.layout.height.round() as u32,
            28,
            "control row should derive from bar height minus block padding, got {:?}",
            cluster.layout
        );

        // The center region must be the middle of the bar whatever the side
        // clusters measure, so the clock cannot drift with their content.
        let center = first_node_with_class(&tree, "center-cluster").expect("center-cluster node");
        assert_eq!(
            (center.layout.x + center.layout.width / 2.0).round() as u32,
            480,
            "center cluster should be centred in the 960px bar, got {:?}",
            center.layout
        );
    }

    fn collect_text_content(node: &mesh_core_elements::WidgetNode) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(c) = node.attributes.get(Prop::Content) {
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

    #[test]
    fn shipped_glass_modules_compile() {
        let frontend = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../modules/frontend");
        let mut compiled = Vec::new();
        for entry in std::fs::read_dir(&frontend).expect("modules/frontend is readable") {
            let module_dir = entry.unwrap().path();
            let name = module_dir
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if !name.starts_with("glass-") {
                continue;
            }
            let loaded = mesh_core_module::manifest::load_canonical_manifest(&module_dir)
                .unwrap_or_else(|error| panic!("{name} manifest should load: {error}"));
            compile_frontend_module(&loaded.manifest, &module_dir)
                .unwrap_or_else(|error| panic!("{name} should compile: {error}"));
            compiled.push(name);
        }
        assert!(compiled.len() >= 4, "found {compiled:?}");
    }
}
