use mesh_core_component::{
    PropDef, PropsBlock, json_to_prop_value_ref, prop_value_to_json, validate_prop_value,
};
use mesh_core_config::validate::{
    FieldKind, FieldSpec, SettingsDiagnostic, unknown_key_diagnostic_from, validate_object,
};
use mesh_core_module::{LocalizedText, Manifest};
use mesh_core_wayland::{Edge, KeyboardMode, Layer, SurfaceRole, WindowDecorations};
use std::collections::BTreeMap;

/// Surface **placement**, resolved from the manifest and user settings.
///
/// Sizing is not part of this struct: surfaces are sized by CSS content
/// measurement of the component root. See `docs/spec/03-components.md` §2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceLayoutSettings {
    /// The layer fields below are inert for [`SurfaceRole::Window`], and
    /// [`Self::window`] is inert for [`SurfaceRole::Layer`].
    pub role: SurfaceRole,
    /// Whether [`Self::role`] may change while the surface is running.
    pub promotable: bool,
    pub window: WindowLayoutSettings,
    pub edge: Edge,
    pub layer: Layer,
    pub exclusive_zone: i32,
    pub keyboard_mode: KeyboardMode,
    pub visible_on_start: bool,
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    /// Appends a `:blur` suffix to the compositor namespace so a single
    /// compositor blur rule can target every opted-in surface.
    pub blur: bool,
}

/// Toplevel-only placement settings (`role: "window"`).
///
/// `title` stays a [`LocalizedText`] because this struct is built at load time,
/// before a locale is bound; the shell resolves it per render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowLayoutSettings {
    pub title: Option<LocalizedText>,
    /// `None` means "derive from the module id" — resolved by the shell.
    pub app_id: Option<String>,
    pub resizable: bool,
    pub decorations: WindowDecorations,
}

impl Default for WindowLayoutSettings {
    fn default() -> Self {
        Self {
            title: None,
            app_id: None,
            resizable: true,
            decorations: WindowDecorations::Client,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FrontendModuleSettingsState {
    /// The namespace exactly as stored, retained for diagnostics and tooling.
    pub raw: serde_json::Value,
    /// Runtime-facing namespace with rejected prop values removed so the
    /// declaration defaults win during precedence resolution.
    pub effective: serde_json::Value,
    pub layout: SurfaceLayoutSettings,
    pub props: FrontendModulePropSettings,
    /// Returned rather than logged: only the caller knows whether this is a
    /// startup read or a reload, and a reload must not repeat itself.
    pub diagnostics: Vec<SettingsDiagnostic>,
}

/// User prop overrides, shaped as
/// `{ "global": { ... }, "instances": { "<instance_key>": { ... } } }`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrontendModulePropSettings {
    pub global: BTreeMap<String, serde_json::Value>,
    pub instances: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
}

pub fn default_surface_visibility() -> bool {
    false
}

pub fn generic_surface_layout_fallback() -> SurfaceLayoutSettings {
    SurfaceLayoutSettings {
        role: SurfaceRole::Layer,
        promotable: false,
        window: WindowLayoutSettings::default(),
        edge: Edge::Top,
        layer: Layer::Top,
        exclusive_zone: 0,
        keyboard_mode: KeyboardMode::None,
        visible_on_start: false,
        margin_top: 0,
        margin_right: 0,
        margin_bottom: 0,
        margin_left: 0,
        blur: false,
    }
}

/// Whether a live role request is authorized by the module declaration.
///
/// A surface may always repeat its current role, but crossing between layer
/// shell and toplevel roles is an author opt-in. Keeping this policy here lets
/// settings resolution and the shell's request path enforce the same rule.
pub fn surface_role_change_allowed(
    current: SurfaceRole,
    requested: SurfaceRole,
    promotable: bool,
) -> bool {
    current == requested || promotable
}

/// Resolve a surface's baseline layout: core defaults overridden by whatever
/// the module's `mesh.surface` block declares. User overrides land on top of
/// this in [`resolve_frontend_module_settings`].
pub fn surface_layout_from_manifest(manifest: &Manifest) -> SurfaceLayoutSettings {
    let mut layout = generic_surface_layout_fallback();

    let Some(surface) = &manifest.surface_layout else {
        return layout;
    };

    if let Some(role) = surface.role.as_deref().and_then(parse_surface_role) {
        layout.role = role;
    }
    if let Some(promotable) = surface.promotable {
        layout.promotable = promotable;
    }
    layout.window.title = surface.title.clone();
    if let Some(app_id) = &surface.app_id {
        layout.window.app_id = Some(app_id.clone());
    }
    if let Some(resizable) = surface.resizable {
        layout.window.resizable = resizable;
    }
    if let Some(decorations) = surface
        .decorations
        .as_deref()
        .and_then(parse_window_decorations)
    {
        layout.window.decorations = decorations;
    }

    if let Some(edge) = surface.anchor.as_deref().and_then(parse_surface_edge) {
        layout.edge = edge;
    }
    if let Some(layer) = surface.layer.as_deref().and_then(parse_surface_layer) {
        layout.layer = layer;
    }
    if let Some(zone) = surface.exclusive_zone {
        layout.exclusive_zone = zone;
    }
    if let Some(mode) = surface
        .keyboard_mode
        .as_deref()
        .and_then(parse_keyboard_mode)
    {
        layout.keyboard_mode = mode;
    }
    if let Some(visible) = surface.visible_on_start {
        layout.visible_on_start = visible;
    }
    if let Some(margins) = &surface.margins {
        layout.margin_top = margins.top;
        layout.margin_right = margins.right;
        layout.margin_bottom = margins.bottom;
        layout.margin_left = margins.left;
    }
    if let Some(blur) = surface.blur {
        layout.blur = blur;
    }

    layout
}

/// Layer a module's stored user overrides over its manifest declarations.
///
/// `raw` is the module's sparse namespace from the settings store
/// (`docs/spec/08-settings.md` §1); untouched fields fall through to the
/// manifest and then to the core default. `namespace` is the store key it came
/// from, used to locate diagnostics. Rejected values are dropped before the
/// reads below, so an unusable stored value falls through *and* is reported.
/// `raw` is returned untouched — it is what the module's script sees.
pub fn resolve_frontend_module_settings(
    namespace: &str,
    raw: serde_json::Value,
    manifest: &Manifest,
) -> FrontendModuleSettingsState {
    resolve_frontend_module_settings_with_props(namespace, raw, manifest, None)
}

/// Resolve module settings and validate prop overrides against the primary
/// component's declarations.
///
/// `props_block` is optional so manifest-only callers can still resolve
/// placement; frontend and tooling callers should pass the compiled block.
pub fn resolve_frontend_module_settings_with_props(
    namespace: &str,
    raw: serde_json::Value,
    manifest: &Manifest,
    props_block: Option<&PropsBlock>,
) -> FrontendModuleSettingsState {
    let mut layout = surface_layout_from_manifest(manifest);
    let (checked_surface, checked_props, diagnostics) =
        validate_module_namespace(namespace, &raw, manifest, props_block);
    let surface = checked_surface.as_object();

    if let Some(role) = surface
        .and_then(|value| value.get("role"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_surface_role)
    {
        layout.role = role;
    }

    if let Some(title) = surface
        .and_then(|value| value.get("title"))
        .and_then(serde_json::Value::as_str)
    {
        layout.window.title = Some(LocalizedText::Literal(title.to_string()));
    }

    if let Some(app_id) = surface
        .and_then(|value| value.get("app_id"))
        .and_then(serde_json::Value::as_str)
    {
        layout.window.app_id = Some(app_id.to_string());
    }

    if let Some(resizable) = surface
        .and_then(|value| value.get("resizable"))
        .and_then(serde_json::Value::as_bool)
    {
        layout.window.resizable = resizable;
    }

    if let Some(decorations) = surface
        .and_then(|value| value.get("decorations"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_window_decorations)
    {
        layout.window.decorations = decorations;
    }

    if let Some(anchor) = surface
        .and_then(|value| value.get("anchor"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_surface_edge)
    {
        layout.edge = anchor;
    }

    if let Some(layer) = surface
        .and_then(|value| value.get("layer"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_surface_layer)
    {
        layout.layer = layer;
    }

    if let Some(zone) = surface
        .and_then(|value| value.get("exclusive_zone"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
    {
        layout.exclusive_zone = zone;
    }

    if let Some(mode) = surface
        .and_then(|value| value.get("keyboard_mode"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_keyboard_mode)
    {
        layout.keyboard_mode = mode;
    }

    if let Some(visible_on_start) = surface
        .and_then(|value| value.get("visible_on_start"))
        .and_then(serde_json::Value::as_bool)
    {
        layout.visible_on_start = visible_on_start;
    }

    if let Some(v) = surface
        .and_then(|value| value.get("margin_top"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
    {
        layout.margin_top = v;
    }
    if let Some(v) = surface
        .and_then(|value| value.get("margin_right"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
    {
        layout.margin_right = v;
    }
    if let Some(v) = surface
        .and_then(|value| value.get("margin_bottom"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
    {
        layout.margin_bottom = v;
    }
    if let Some(v) = surface
        .and_then(|value| value.get("margin_left"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
    {
        layout.margin_left = v;
    }
    if let Some(blur) = surface
        .and_then(|value| value.get("blur"))
        .and_then(serde_json::Value::as_bool)
    {
        layout.blur = blur;
    }

    let props = load_prop_settings(&checked_props);
    let mut effective = raw.clone();
    if raw.get("props").is_some() {
        if let Some(namespace) = effective.as_object_mut() {
            namespace.insert("props".into(), checked_props);
        }
    }

    FrontendModuleSettingsState {
        raw,
        effective,
        layout,
        props,
        diagnostics,
    }
}

/// Materialize the effective global values of exposed component props.
///
/// Declared defaults form the baseline; validated stored globals override them.
/// Props with neither are omitted, having no effective value to eject.
pub fn effective_global_props_to_json(
    block: Option<&PropsBlock>,
    stored: &FrontendModulePropSettings,
) -> serde_json::Value {
    let mut values = serde_json::Map::new();
    let Some(block) = block else {
        return serde_json::Value::Object(values);
    };
    for def in block.props.iter().filter(|def| def.expose) {
        if let Some(value) = stored.global.get(&def.name) {
            values.insert(def.name.clone(), value.clone());
        } else if let Some(default) = &def.default {
            values.insert(def.name.clone(), prop_value_to_json(default));
        }
    }
    serde_json::Value::Object(values)
}

fn load_prop_settings(raw: &serde_json::Value) -> FrontendModulePropSettings {
    let mut settings = FrontendModulePropSettings::default();
    let Some(props) = raw.as_object() else {
        return settings;
    };
    if let Some(global) = props.get("global").and_then(serde_json::Value::as_object) {
        settings.global = global
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
    }
    if let Some(instances) = props
        .get("instances")
        .and_then(serde_json::Value::as_object)
    {
        for (instance_key, values) in instances {
            let Some(values) = values.as_object() else {
                continue;
            };
            settings.instances.insert(
                instance_key.clone(),
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            );
        }
    }
    settings
}

/// Serialize a resolved layout back into a settings namespace's `surface`
/// block, for `mesh-shell config eject`.
///
/// Round-trips through [`resolve_frontend_module_settings`], and emits
/// role-specific fields only for the matching role so nothing inert is written.
/// The author-only `promotable` capability is intentionally never serialized
/// into the user settings namespace.
pub fn surface_layout_to_json(layout: &SurfaceLayoutSettings) -> serde_json::Value {
    let mut block = serde_json::Map::new();
    block.insert("role".into(), surface_role_name(layout.role).into());

    if layout.role == SurfaceRole::Window || layout.promotable {
        if let Some(title) = &layout.window.title {
            block.insert("title".into(), title.fallback_text().into());
        }
        if let Some(app_id) = &layout.window.app_id {
            block.insert("app_id".into(), app_id.clone().into());
        }
        block.insert("resizable".into(), layout.window.resizable.into());
        block.insert(
            "decorations".into(),
            window_decorations_name(layout.window.decorations).into(),
        );
    }

    if layout.role == SurfaceRole::Layer || layout.promotable {
        block.insert("anchor".into(), surface_edge_name(layout.edge).into());
        block.insert("layer".into(), surface_layer_name(layout.layer).into());
        block.insert("exclusive_zone".into(), layout.exclusive_zone.into());
        block.insert("blur".into(), layout.blur.into());
    }

    block.insert(
        "keyboard_mode".into(),
        keyboard_mode_name(layout.keyboard_mode).into(),
    );
    block.insert("visible_on_start".into(), layout.visible_on_start.into());
    block.insert("margin_top".into(), layout.margin_top.into());
    block.insert("margin_right".into(), layout.margin_right.into());
    block.insert("margin_bottom".into(), layout.margin_bottom.into());
    block.insert("margin_left".into(), layout.margin_left.into());

    serde_json::Value::Object(block)
}

pub const fn surface_role_name(role: SurfaceRole) -> &'static str {
    match role {
        SurfaceRole::Layer => "layer",
        SurfaceRole::Window => "window",
    }
}

pub const fn window_decorations_name(decorations: WindowDecorations) -> &'static str {
    match decorations {
        WindowDecorations::Client => "client",
        WindowDecorations::Server => "server",
    }
}

pub const fn surface_edge_name(edge: Edge) -> &'static str {
    match edge {
        Edge::Top => "top",
        Edge::Bottom => "bottom",
        Edge::Left => "left",
        Edge::Right => "right",
    }
}

pub const fn surface_layer_name(layer: Layer) -> &'static str {
    match layer {
        Layer::Background => "background",
        Layer::Bottom => "bottom",
        Layer::Top => "top",
        Layer::Overlay => "overlay",
    }
}

pub const fn keyboard_mode_name(mode: KeyboardMode) -> &'static str {
    match mode {
        KeyboardMode::None => "none",
        KeyboardMode::Exclusive => "exclusive",
        KeyboardMode::OnDemand => "on_demand",
    }
}

pub fn parse_surface_role(value: &str) -> Option<SurfaceRole> {
    match value.trim().to_ascii_lowercase().as_str() {
        "layer" => Some(SurfaceRole::Layer),
        "window" | "toplevel" => Some(SurfaceRole::Window),
        _ => None,
    }
}

pub fn parse_window_decorations(value: &str) -> Option<WindowDecorations> {
    match value.trim().to_ascii_lowercase().as_str() {
        "client" | "client_side" | "csd" => Some(WindowDecorations::Client),
        "server" | "server_side" | "ssd" => Some(WindowDecorations::Server),
        _ => None,
    }
}

pub fn parse_surface_edge(value: &str) -> Option<Edge> {
    match value.trim().to_ascii_lowercase().as_str() {
        "top" => Some(Edge::Top),
        "bottom" => Some(Edge::Bottom),
        "left" => Some(Edge::Left),
        "right" => Some(Edge::Right),
        _ => None,
    }
}

pub fn parse_surface_layer(value: &str) -> Option<Layer> {
    match value.trim().to_ascii_lowercase().as_str() {
        "background" => Some(Layer::Background),
        "bottom" => Some(Layer::Bottom),
        "top" => Some(Layer::Top),
        "overlay" => Some(Layer::Overlay),
        _ => None,
    }
}

pub fn parse_keyboard_mode(value: &str) -> Option<KeyboardMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "none" => Some(KeyboardMode::None),
        "exclusive" => Some(KeyboardMode::Exclusive),
        "on_demand" | "ondemand" | "on-demand" => Some(KeyboardMode::OnDemand),
        _ => None,
    }
}

// The schema below is built from the parsers above rather than a second copy of
// their strings: each enum field uses `parse_*` as its acceptance test and the
// canonical `*_name` output as the list quoted back to the user. The
// `*_is_listed` guards turn adding a variant into a compile error until its
// value list is updated, so a new variant cannot go missing from a diagnostic.

pub const SURFACE_ROLE_VALUES: &[&str] = &[
    surface_role_name(SurfaceRole::Layer),
    surface_role_name(SurfaceRole::Window),
];

pub const WINDOW_DECORATIONS_VALUES: &[&str] = &[
    window_decorations_name(WindowDecorations::Client),
    window_decorations_name(WindowDecorations::Server),
];

pub const SURFACE_EDGE_VALUES: &[&str] = &[
    surface_edge_name(Edge::Top),
    surface_edge_name(Edge::Bottom),
    surface_edge_name(Edge::Left),
    surface_edge_name(Edge::Right),
];

pub const SURFACE_LAYER_VALUES: &[&str] = &[
    surface_layer_name(Layer::Background),
    surface_layer_name(Layer::Bottom),
    surface_layer_name(Layer::Top),
    surface_layer_name(Layer::Overlay),
];

pub const KEYBOARD_MODE_VALUES: &[&str] = &[
    keyboard_mode_name(KeyboardMode::None),
    keyboard_mode_name(KeyboardMode::Exclusive),
    keyboard_mode_name(KeyboardMode::OnDemand),
];

const fn surface_role_is_listed(role: SurfaceRole) -> bool {
    match role {
        SurfaceRole::Layer | SurfaceRole::Window => true,
    }
}

const fn window_decorations_is_listed(decorations: WindowDecorations) -> bool {
    match decorations {
        WindowDecorations::Client | WindowDecorations::Server => true,
    }
}

const fn surface_edge_is_listed(edge: Edge) -> bool {
    match edge {
        Edge::Top | Edge::Bottom | Edge::Left | Edge::Right => true,
    }
}

const fn surface_layer_is_listed(layer: Layer) -> bool {
    match layer {
        Layer::Background | Layer::Bottom | Layer::Top | Layer::Overlay => true,
    }
}

const fn keyboard_mode_is_listed(mode: KeyboardMode) -> bool {
    match mode {
        KeyboardMode::None | KeyboardMode::Exclusive | KeyboardMode::OnDemand => true,
    }
}

const _: () = assert!(
    surface_role_is_listed(SurfaceRole::Layer)
        && window_decorations_is_listed(WindowDecorations::Client)
        && surface_edge_is_listed(Edge::Top)
        && surface_layer_is_listed(Layer::Background)
        && keyboard_mode_is_listed(KeyboardMode::None),
);

fn canonical_surface_role(value: &str) -> Option<&'static str> {
    parse_surface_role(value).map(surface_role_name)
}
fn canonical_window_decorations(value: &str) -> Option<&'static str> {
    parse_window_decorations(value).map(window_decorations_name)
}
fn canonical_surface_edge(value: &str) -> Option<&'static str> {
    parse_surface_edge(value).map(surface_edge_name)
}
fn canonical_surface_layer(value: &str) -> Option<&'static str> {
    parse_surface_layer(value).map(surface_layer_name)
}
fn canonical_keyboard_mode(value: &str) -> Option<&'static str> {
    parse_keyboard_mode(value).map(keyboard_mode_name)
}

/// One entry per user-overridable field [`resolve_frontend_module_settings`]
/// reads. The manifest-only `promotable` capability deliberately does not
/// belong to this schema.
pub const SURFACE_FIELDS: &[FieldSpec] = &[
    FieldSpec::new(
        "role",
        FieldKind::Enum {
            canonicalize: canonical_surface_role,
            values: SURFACE_ROLE_VALUES,
        },
    ),
    FieldSpec::new("title", FieldKind::Str),
    FieldSpec::new("app_id", FieldKind::Str),
    FieldSpec::new("resizable", FieldKind::Bool),
    FieldSpec::new(
        "decorations",
        FieldKind::Enum {
            canonicalize: canonical_window_decorations,
            values: WINDOW_DECORATIONS_VALUES,
        },
    ),
    FieldSpec::new(
        "anchor",
        FieldKind::Enum {
            canonicalize: canonical_surface_edge,
            values: SURFACE_EDGE_VALUES,
        },
    ),
    FieldSpec::new(
        "layer",
        FieldKind::Enum {
            canonicalize: canonical_surface_layer,
            values: SURFACE_LAYER_VALUES,
        },
    ),
    FieldSpec::new("exclusive_zone", FieldKind::Int32),
    FieldSpec::new(
        "keyboard_mode",
        FieldKind::Enum {
            canonicalize: canonical_keyboard_mode,
            values: KEYBOARD_MODE_VALUES,
        },
    ),
    FieldSpec::new("visible_on_start", FieldKind::Bool),
    FieldSpec::new("margin_top", FieldKind::Int32),
    FieldSpec::new("margin_right", FieldKind::Int32),
    FieldSpec::new("margin_bottom", FieldKind::Int32),
    FieldSpec::new("margin_left", FieldKind::Int32),
    FieldSpec::new("blur", FieldKind::Bool),
];

/// A module namespace's top-level keys. `props` has a component-specific
/// vocabulary, so this static walk only checks its scope shape;
/// [`validate_module_namespace`] validates names and values afterward.
pub const MODULE_NAMESPACE_FIELDS: &[FieldSpec] = &[
    FieldSpec::new("surface", FieldKind::Section(SURFACE_FIELDS)),
    FieldSpec::new(
        "props",
        FieldKind::Section(&[
            FieldSpec::new("global", FieldKind::Opaque),
            FieldSpec::new("instances", FieldKind::Map(&FieldKind::Opaque)),
        ]),
    ),
    FieldSpec::new(
        "icons",
        FieldKind::Section(&[
            FieldSpec::new("use_packs", FieldKind::StrArray),
            FieldSpec::new("overrides", FieldKind::Map(&FieldKind::Str)),
            FieldSpec::new("ignore_shell_default", FieldKind::Bool),
        ]),
    ),
    FieldSpec::new(
        "i18n",
        FieldKind::Section(&[FieldSpec::new("default_locale", FieldKind::Str)]),
    ),
];

const WINDOW_ONLY_KEYS: &[&str] = &["title", "app_id", "resizable", "decorations"];
const LAYER_ONLY_KEYS: &[&str] = &["anchor", "layer", "exclusive_zone", "blur"];

/// Validate one module's stored namespace, returning the `surface` block
/// stripped of anything unusable plus everything worth telling the user.
///
/// `manifest` supplies the declared role, because whether a stored field is
/// inert depends on the role the surface ends up with.
fn validate_module_namespace(
    namespace: &str,
    raw: &serde_json::Value,
    manifest: &Manifest,
    props_block: Option<&PropsBlock>,
) -> (
    serde_json::Value,
    serde_json::Value,
    Vec<SettingsDiagnostic>,
) {
    let mut diagnostics = Vec::new();
    if raw.as_object().is_some_and(serde_json::Map::is_empty) {
        return (
            serde_json::Value::Null,
            serde_json::Value::Null,
            diagnostics,
        );
    }

    let checked = validate_object(
        namespace,
        "",
        MODULE_NAMESPACE_FIELDS,
        raw,
        &mut diagnostics,
    );
    let declared = surface_layout_from_manifest(manifest);
    let mut surface = checked.get("surface").cloned().unwrap_or_default();
    let requested_role = surface
        .get("role")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_surface_role);
    if requested_role.is_some_and(|requested_role| {
        !surface_role_change_allowed(declared.role, requested_role, declared.promotable)
    }) {
        diagnostics.push(SettingsDiagnostic::warning(
            namespace,
            "surface.role",
            format!(
                "\"surface.role\" cannot change from \"{}\" because the module manifest does not declare \"mesh.surface.promotable\"",
                surface_role_name(declared.role)
            ),
            "remove the override, or ask the module author to set mesh.surface.promotable to true",
        ));
        if let Some(surface) = surface.as_object_mut() {
            surface.remove("role");
        }
    }
    let props = checked
        .get("props")
        .map(|value| validate_prop_scopes(namespace, value, props_block, &mut diagnostics))
        .unwrap_or_default();

    if let Some(stored) = raw.get("surface").and_then(serde_json::Value::as_object) {
        report_inert_placement_fields(namespace, stored, &surface, manifest, &mut diagnostics);
    }

    (surface, props, diagnostics)
}

fn validate_prop_scopes(
    namespace: &str,
    raw: &serde_json::Value,
    block: Option<&PropsBlock>,
    diagnostics: &mut Vec<SettingsDiagnostic>,
) -> serde_json::Value {
    // Without the owning declaration there is no sound judgment to make about
    // names or values; preserve them for a prop-aware caller.
    let Some(block) = block else {
        return raw.clone();
    };
    let Some(scopes) = raw.as_object() else {
        return serde_json::Value::Null;
    };
    let definitions: Vec<&PropDef> = block.props.iter().filter(|def| def.expose).collect();
    let mut checked = serde_json::Map::new();

    if let Some(global) = scopes.get("global").and_then(serde_json::Value::as_object) {
        checked.insert(
            "global".into(),
            validate_prop_map(namespace, "props.global", global, &definitions, diagnostics),
        );
    }
    if let Some(instances) = scopes
        .get("instances")
        .and_then(serde_json::Value::as_object)
    {
        let mut checked_instances = serde_json::Map::new();
        for (instance_key, values) in instances {
            let Some(values) = values.as_object() else {
                continue;
            };
            checked_instances.insert(
                instance_key.clone(),
                validate_prop_map(
                    namespace,
                    &format!("props.instances.{instance_key}"),
                    values,
                    &definitions,
                    diagnostics,
                ),
            );
        }
        checked.insert(
            "instances".into(),
            serde_json::Value::Object(checked_instances),
        );
    }
    serde_json::Value::Object(checked)
}

fn validate_prop_map(
    namespace: &str,
    prefix: &str,
    values: &serde_json::Map<String, serde_json::Value>,
    definitions: &[&PropDef],
    diagnostics: &mut Vec<SettingsDiagnostic>,
) -> serde_json::Value {
    let known: Vec<&str> = definitions.iter().map(|def| def.name.as_str()).collect();
    let mut accepted = serde_json::Map::new();
    for (name, value) in values {
        let Some(def) = definitions.iter().find(|def| def.name == *name) else {
            diagnostics.push(unknown_key_diagnostic_from(namespace, prefix, name, &known));
            continue;
        };
        let path = format!("{prefix}.{name}");
        let valid = json_to_prop_value_ref(value).ok().and_then(|prop_value| {
            validate_prop_value(def, &prop_value)
                .ok()
                .map(|_| prop_value)
        });
        match valid {
            Some(_) => {
                accepted.insert(name.clone(), value.clone());
            }
            None => diagnostics.push(SettingsDiagnostic::error(
                namespace,
                path,
                prop_value_error(def, value),
                "use a value accepted by the component's <props> declaration, or remove the key",
            )),
        }
    }
    serde_json::Value::Object(accepted)
}

fn prop_value_error(def: &PropDef, value: &serde_json::Value) -> String {
    json_to_prop_value_ref(value)
        .ok()
        .and_then(|value| validate_prop_value(def, &value).err())
        .map(|error| error.message)
        .unwrap_or_else(|| {
            format!(
                "prop `{}` expects a {}, found {}",
                def.name,
                def.ty.lua_type(),
                mesh_core_config::validate::describe(value)
            )
        })
}

/// Warn about stored placement fields the surface's role ignores.
///
/// Never an error: `resizable` on a layer surface is meaningless but harmless.
/// Promotable surfaces are exempt — both roles' fields apply over their life.
fn report_inert_placement_fields(
    namespace: &str,
    stored: &serde_json::Map<String, serde_json::Value>,
    surface: &serde_json::Value,
    manifest: &Manifest,
    diagnostics: &mut Vec<SettingsDiagnostic>,
) {
    let declared = surface_layout_from_manifest(manifest);
    if declared.promotable {
        return;
    }

    let role = surface
        .get("role")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_surface_role)
        .unwrap_or(declared.role);
    let (inert, other_role) = match role {
        SurfaceRole::Layer => (WINDOW_ONLY_KEYS, SurfaceRole::Window),
        SurfaceRole::Window => (LAYER_ONLY_KEYS, SurfaceRole::Layer),
    };

    for key in inert.iter().filter(|key| stored.contains_key(**key)) {
        diagnostics.push(SettingsDiagnostic::warning(
            namespace,
            format!("surface.{key}"),
            format!(
                "\"{key}\" only applies to role \"{}\"; this surface has role \"{}\", so it has no effect",
                surface_role_name(other_role),
                surface_role_name(role)
            ),
            format!(
                "remove it, or ask the module author to declare mesh.surface.promotable if this surface should be able to become a {}",
                surface_role_name(other_role)
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_module::manifest::{Manifest, ModuleSection, ModuleType, SurfaceLayoutSection};
    use std::collections::HashMap;

    fn manifest_with_surface_layout(surface_layout: SurfaceLayoutSection) -> Manifest {
        Manifest {
            package: ModuleSection {
                id: "@mesh/test".into(),
                name: None,
                version: "0.1.0".into(),
                module_type: ModuleType::Surface,
                api_version: "0.1".into(),
                license: None,
                description: None,
                authors: Vec::new(),
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
            provides: Vec::new(),
            interface: None,
            interfaces: Vec::new(),
            extensions: Vec::new(),
            exports: Default::default(),
            hosted_extension_points: HashMap::new(),
            extension_point_contributions: HashMap::new(),
            assets: None,
            icons: None,
            icon_pack: None,
            icon_requirements: Default::default(),
            translations: HashMap::new(),
            surface_layout: Some(surface_layout),
        }
    }

    #[test]
    fn manifest_surface_layout_sets_keyboard_mode_default() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            keyboard_mode: Some("on_demand".into()),
            ..Default::default()
        });

        let layout = surface_layout_from_manifest(&manifest);

        assert_eq!(layout.keyboard_mode, KeyboardMode::OnDemand);
    }

    #[test]
    fn manifest_surface_role_defaults_to_layer() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());

        let layout = surface_layout_from_manifest(&manifest);

        assert_eq!(layout.role, SurfaceRole::Layer);
        assert!(layout.window.resizable);
        assert_eq!(layout.window.decorations, WindowDecorations::Client);
    }

    #[test]
    fn manifest_surface_role_window_carries_window_options() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            role: Some("window".into()),
            title: Some(mesh_core_module::LocalizedText::Translation {
                key: "settings.title".into(),
                fallback: "Settings".into(),
            }),
            app_id: Some("mesh.settings".into()),
            resizable: Some(false),
            decorations: Some("server".into()),
            ..Default::default()
        });

        let layout = surface_layout_from_manifest(&manifest);

        assert_eq!(layout.role, SurfaceRole::Window);
        assert_eq!(
            layout.window.title.as_ref().map(|t| t.fallback_text()),
            Some("Settings")
        );
        assert_eq!(layout.window.app_id.as_deref(), Some("mesh.settings"));
        assert!(!layout.window.resizable);
        assert_eq!(layout.window.decorations, WindowDecorations::Server);
    }

    #[test]
    fn promotable_defaults_off_and_is_read_from_the_manifest() {
        let plain = manifest_with_surface_layout(SurfaceLayoutSection::default());
        assert!(!surface_layout_from_manifest(&plain).promotable);

        let promotable = manifest_with_surface_layout(SurfaceLayoutSection {
            role: Some("layer".into()),
            promotable: Some(true),
            anchor: Some("right".into()),
            app_id: Some("mesh.settings".into()),
            ..Default::default()
        });
        let layout = surface_layout_from_manifest(&promotable);

        assert!(layout.promotable);
        assert_eq!(layout.role, SurfaceRole::Layer);
        assert_eq!(layout.edge, Edge::Right);
        assert_eq!(layout.window.app_id.as_deref(), Some("mesh.settings"));
    }

    #[test]
    fn a_surface_role_change_requires_the_author_promotable_opt_in() {
        assert!(surface_role_change_allowed(
            SurfaceRole::Layer,
            SurfaceRole::Layer,
            false
        ));
        assert!(!surface_role_change_allowed(
            SurfaceRole::Layer,
            SurfaceRole::Window,
            false
        ));
        assert!(surface_role_change_allowed(
            SurfaceRole::Layer,
            SurfaceRole::Window,
            true
        ));
    }

    #[test]
    fn non_promotable_user_settings_cannot_override_manifest_surface_role() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let settings = resolve_frontend_module_settings(
            "@mesh/test",
            serde_json::json!({ "surface": { "role": "window" } }),
            &manifest,
        );

        assert_eq!(settings.layout.role, SurfaceRole::Layer);
        let diagnostic = only(&settings.diagnostics);
        assert_eq!(diagnostic.key_path, "surface.role");
        assert!(diagnostic.message.contains("promotable"));
    }

    #[test]
    fn promotable_user_settings_can_override_manifest_surface_role() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            promotable: Some(true),
            ..Default::default()
        });
        let settings = resolve_frontend_module_settings(
            "@mesh/test",
            serde_json::json!({ "surface": { "role": "window" } }),
            &manifest,
        );

        assert_eq!(settings.layout.role, SurfaceRole::Window);
        assert!(
            settings.diagnostics.is_empty(),
            "{:#?}",
            settings.diagnostics
        );
    }

    #[test]
    fn promotable_is_manifest_only_for_settings_and_ejection() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            promotable: Some(true),
            ..Default::default()
        });
        let state = resolve_frontend_module_settings(
            "@mesh/test",
            serde_json::json!({ "surface": { "promotable": false } }),
            &manifest,
        );

        assert!(state.layout.promotable);
        let diagnostic = only(&state.diagnostics);
        assert_eq!(diagnostic.key_path, "surface.promotable");
        assert!(diagnostic.message.contains("unknown key"));

        let ejected = surface_layout_to_json(&state.layout);
        assert!(ejected.get("promotable").is_none());
    }

    #[test]
    fn user_settings_override_manifest_keyboard_mode_default() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            keyboard_mode: Some("on_demand".into()),
            ..Default::default()
        });
        let settings = resolve_frontend_module_settings(
            "@mesh/test",
            serde_json::json!({ "surface": { "keyboard_mode": "exclusive" } }),
            &manifest,
        );

        assert_eq!(settings.layout.keyboard_mode, KeyboardMode::Exclusive);
    }

    #[test]
    fn an_empty_namespace_leaves_every_manifest_default_intact() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            anchor: Some("bottom".into()),
            exclusive_zone: Some(56),
            ..Default::default()
        });
        let settings =
            resolve_frontend_module_settings("@mesh/test", serde_json::json!({}), &manifest);

        assert_eq!(settings.layout, surface_layout_from_manifest(&manifest));
        assert_eq!(settings.layout.edge, Edge::Bottom);
        assert_eq!(settings.layout.exclusive_zone, 56);
    }

    #[test]
    fn a_partial_surface_override_leaves_sibling_fields_on_the_manifest_default() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            anchor: Some("top".into()),
            layer: Some("top".into()),
            exclusive_zone: Some(56),
            visible_on_start: Some(true),
            ..Default::default()
        });
        let settings = resolve_frontend_module_settings(
            "@mesh/test",
            serde_json::json!({ "surface": { "anchor": "bottom" } }),
            &manifest,
        );

        assert_eq!(settings.layout.edge, Edge::Bottom);
        assert_eq!(settings.layout.layer, Layer::Top);
        assert_eq!(settings.layout.exclusive_zone, 56);
        assert!(settings.layout.visible_on_start);
    }

    #[test]
    fn resolve_frontend_module_settings_reads_prop_scopes() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let settings = resolve_frontend_module_settings(
            "@mesh/test",
            serde_json::json!({
                "props": {
                    "global": { "track_width": "24px", "anim_ms": 90 },
                    "instances": {
                        "@mesh/navigation-bar/import:audio": { "track_width": "28px" }
                    }
                }
            }),
            &manifest,
        );

        assert_eq!(
            settings.props.global.get("track_width"),
            Some(&serde_json::json!("24px"))
        );
        assert_eq!(
            settings
                .props
                .instances
                .get("@mesh/navigation-bar/import:audio")
                .and_then(|props| props.get("track_width")),
            Some(&serde_json::json!("28px"))
        );
    }

    #[test]
    fn compact_surface_block_resolves_editable_defaults() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            anchor: Some("bottom".into()),
            layer: Some("overlay".into()),
            exclusive_zone: Some(48),
            visible_on_start: Some(true),
            keyboard_mode: Some("none".into()),
            ..Default::default()
        });

        let layout = surface_layout_from_manifest(&manifest);

        assert_eq!(layout.edge, Edge::Bottom);
        assert_eq!(layout.layer, Layer::Overlay);
        assert_eq!(layout.exclusive_zone, 48);
        assert!(layout.visible_on_start);
        assert_eq!(layout.keyboard_mode, KeyboardMode::None);
    }

    #[test]
    fn an_ejected_layer_block_round_trips_through_resolution() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            anchor: Some("bottom".into()),
            layer: Some("overlay".into()),
            exclusive_zone: Some(56),
            keyboard_mode: Some("on_demand".into()),
            visible_on_start: Some(true),
            blur: Some(true),
            ..Default::default()
        });
        let original = surface_layout_from_manifest(&manifest);

        let ejected = serde_json::json!({ "surface": surface_layout_to_json(&original) });
        let bare = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let round_tripped = resolve_frontend_module_settings("@mesh/test", ejected, &bare).layout;

        assert_eq!(round_tripped, original);
    }

    #[test]
    fn an_ejected_window_block_round_trips_through_resolution() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            role: Some("window".into()),
            promotable: Some(true),
            app_id: Some("mesh.settings".into()),
            resizable: Some(false),
            decorations: Some("server".into()),
            keyboard_mode: Some("exclusive".into()),
            ..Default::default()
        });
        let original = surface_layout_from_manifest(&manifest);

        let ejected = serde_json::json!({ "surface": surface_layout_to_json(&original) });
        let bare = manifest_with_surface_layout(SurfaceLayoutSection {
            promotable: Some(true),
            ..Default::default()
        });
        let round_tripped = resolve_frontend_module_settings("@mesh/test", ejected, &bare).layout;

        assert_eq!(round_tripped, original);
    }

    #[test]
    fn an_ejected_layer_block_omits_inert_window_fields() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let block = surface_layout_to_json(&surface_layout_from_manifest(&manifest));

        assert_eq!(block["role"], serde_json::json!("layer"));
        assert!(block.get("resizable").is_none());
        assert!(block.get("decorations").is_none());
        assert!(block.get("anchor").is_some());
        assert!(block.get("promotable").is_none());
    }

    fn diagnose(
        surface: serde_json::Value,
        manifest: &Manifest,
    ) -> (SurfaceLayoutSettings, Vec<SettingsDiagnostic>) {
        let state = resolve_frontend_module_settings(
            "@mesh/navigation-bar",
            serde_json::json!({ "surface": surface }),
            manifest,
        );
        (state.layout, state.diagnostics)
    }

    fn only(diagnostics: &[SettingsDiagnostic]) -> &SettingsDiagnostic {
        assert_eq!(
            diagnostics.len(),
            1,
            "expected one diagnostic: {diagnostics:#?}"
        );
        &diagnostics[0]
    }

    #[test]
    fn every_listed_enum_value_is_one_its_parser_accepts() {
        for value in SURFACE_ROLE_VALUES {
            assert!(parse_surface_role(value).is_some(), "role {value}");
        }
        for value in WINDOW_DECORATIONS_VALUES {
            assert!(parse_window_decorations(value).is_some(), "deco {value}");
        }
        for value in SURFACE_EDGE_VALUES {
            assert!(parse_surface_edge(value).is_some(), "anchor {value}");
        }
        for value in SURFACE_LAYER_VALUES {
            assert!(parse_surface_layer(value).is_some(), "layer {value}");
        }
        for value in KEYBOARD_MODE_VALUES {
            assert!(parse_keyboard_mode(value).is_some(), "keyboard {value}");
        }
    }

    #[test]
    fn a_bad_anchor_value_is_reported_and_the_manifest_default_stands() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            anchor: Some("top".into()),
            ..Default::default()
        });
        let (layout, diagnostics) = diagnose(serde_json::json!({ "anchor": "botom" }), &manifest);

        let diagnostic = only(&diagnostics);
        assert!(diagnostic.is_error());
        assert_eq!(diagnostic.namespace, "@mesh/navigation-bar");
        assert_eq!(diagnostic.key_path, "surface.anchor");
        assert!(
            diagnostic.message.contains("\"botom\""),
            "the message should quote what was found: {}",
            diagnostic.message
        );
        assert_eq!(
            diagnostic.suggested_action,
            "use one of: top, bottom, left, right"
        );
        assert_eq!(layout.edge, Edge::Top);
    }

    #[test]
    fn an_enum_alias_the_parser_accepts_is_not_reported() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            promotable: Some(true),
            ..Default::default()
        });
        let (layout, diagnostics) = diagnose(serde_json::json!({ "role": "toplevel" }), &manifest);

        assert_eq!(layout.role, SurfaceRole::Window);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }

    #[test]
    fn a_wrong_type_is_reported_and_the_default_stands() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            exclusive_zone: Some(56),
            ..Default::default()
        });
        let (layout, diagnostics) =
            diagnose(serde_json::json!({ "exclusive_zone": "56" }), &manifest);

        let diagnostic = only(&diagnostics);
        assert!(diagnostic.is_error());
        assert_eq!(diagnostic.key_path, "surface.exclusive_zone");
        assert!(diagnostic.message.contains("an integer"));
        assert_eq!(layout.exclusive_zone, 56);
    }

    #[test]
    fn a_typoed_key_suggests_the_one_it_meant() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let (layout, diagnostics) = diagnose(serde_json::json!({ "anchr": "bottom" }), &manifest);

        let diagnostic = only(&diagnostics);
        assert!(diagnostic.is_error());
        assert_eq!(diagnostic.key_path, "surface.anchr");
        assert_eq!(diagnostic.suggested_action, "did you mean \"anchor\"?");
        assert_eq!(layout.edge, Edge::Top);
    }

    #[test]
    fn an_unknown_key_with_no_near_match_warns_and_lists_the_schema() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let (_, diagnostics) = diagnose(serde_json::json!({ "elevation": 3 }), &manifest);

        let diagnostic = only(&diagnostics);
        assert!(
            !diagnostic.is_error(),
            "an unrecognized key may be foreign, not a typo"
        );
        assert!(diagnostic.suggested_action.contains("anchor"));
    }

    #[test]
    fn a_role_inert_field_warns_rather_than_failing() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let (layout, diagnostics) = diagnose(serde_json::json!({ "resizable": false }), &manifest);

        let diagnostic = only(&diagnostics);
        assert!(!diagnostic.is_error());
        assert_eq!(diagnostic.key_path, "surface.resizable");
        assert!(
            diagnostic.message.contains("role \"layer\""),
            "the warning should name the role that ignores it: {}",
            diagnostic.message
        );
        assert!(!layout.window.resizable);
    }

    #[test]
    fn a_promotable_surface_carries_both_roles_fields_without_warnings() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection {
            promotable: Some(true),
            ..Default::default()
        });
        let (_, diagnostics) = diagnose(
            serde_json::json!({ "resizable": false, "anchor": "right", "decorations": "server" }),
            &manifest,
        );

        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }

    #[test]
    fn a_rejected_user_role_override_does_not_reclassify_inert_fields() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let (_, diagnostics) = diagnose(
            serde_json::json!({ "role": "window", "anchor": "bottom" }),
            &manifest,
        );

        let diagnostic = only(&diagnostics);
        assert_eq!(diagnostic.key_path, "surface.role");
        assert!(diagnostic.message.contains("promotable"));
    }

    #[test]
    fn callers_without_prop_declarations_preserve_prop_values() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let state = resolve_frontend_module_settings(
            "@mesh/navigation-bar",
            serde_json::json!({
                "props": { "global": { "anything": [1, "two", null] } }
            }),
            &manifest,
        );

        assert!(state.diagnostics.is_empty(), "{:#?}", state.diagnostics);
        assert_eq!(
            state.props.global.get("anything"),
            Some(&serde_json::json!([1, "two", null]))
        );
    }

    fn declared_props() -> PropsBlock {
        mesh_core_component::parse_component(
            r#"
<props>
  density: { type: "enum", options: ["compact", "cozy"], default: "cozy" }
  track_width: { type: "size", default: "20px" }
  anim_ms: { type: "duration", default: 120, min: 0, max: 600 }
  internal: { type: "bool", default: true, expose: false }
</props>
<template><box /></template>
"#,
        )
        .expect("component")
        .props
        .expect("props")
    }

    #[test]
    fn declared_props_validate_global_and_instance_overrides() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let props = declared_props();
        let state = resolve_frontend_module_settings_with_props(
            "@mesh/test",
            serde_json::json!({
                "props": {
                    "global": {
                        "density": "dense",
                        "track_width": "28px",
                        "anim_ms": 900,
                        "internal": false,
                        "track_wdth": "30px"
                    },
                    "instances": {
                        "@mesh/test#top": {
                            "density": "compact",
                            "track_width": [28, "px"]
                        }
                    }
                }
            }),
            &manifest,
            Some(&props),
        );

        assert_eq!(
            state.props.global,
            BTreeMap::from([("track_width".into(), serde_json::json!("28px"))])
        );
        assert_eq!(
            state.props.instances["@mesh/test#top"],
            BTreeMap::from([("density".into(), serde_json::json!("compact"))])
        );
        let paths: Vec<_> = state
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.key_path.as_str())
            .collect();
        assert!(paths.contains(&"props.global.density"));
        assert!(paths.contains(&"props.global.anim_ms"));
        assert!(paths.contains(&"props.global.internal"));
        assert!(paths.contains(&"props.global.track_wdth"));
        assert!(paths.contains(&"props.instances.@mesh/test#top.track_width"));
        assert_eq!(
            state.effective.pointer("/props/global/track_width"),
            Some(&serde_json::json!("28px"))
        );
        assert!(state.effective.pointer("/props/global/density").is_none());
        assert!(state.effective.pointer("/props/global/anim_ms").is_none());
    }

    #[test]
    fn eject_materializes_exposed_effective_prop_values() {
        let props = declared_props();
        let stored = FrontendModulePropSettings {
            global: BTreeMap::from([("density".into(), serde_json::json!("compact"))]),
            instances: BTreeMap::new(),
        };

        assert_eq!(
            effective_global_props_to_json(Some(&props), &stored),
            serde_json::json!({
                "density": "compact",
                "track_width": "20px",
                "anim_ms": 120.0
            })
        );
    }

    #[test]
    fn an_unknown_namespace_key_is_reported_but_the_raw_namespace_survives() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let raw = serde_json::json!({ "surfce": { "anchor": "bottom" } });
        let state =
            resolve_frontend_module_settings("@mesh/navigation-bar", raw.clone(), &manifest);

        assert_eq!(only(&state.diagnostics).key_path, "surfce");
        assert_eq!(state.raw, raw);
    }

    #[test]
    fn a_clean_namespace_produces_no_diagnostics() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let state = resolve_frontend_module_settings(
            "@mesh/navigation-bar",
            serde_json::json!({
                "surface": { "anchor": "bottom", "exclusive_zone": 48, "blur": true },
                "i18n": { "default_locale": "sk" },
                "icons": { "overrides": { "settings": "lucide/settings" } }
            }),
            &manifest,
        );

        assert!(state.diagnostics.is_empty(), "{:#?}", state.diagnostics);
    }

    #[test]
    fn unset_surface_layout_uses_core_defaults() {
        let manifest = manifest_with_surface_layout(SurfaceLayoutSection::default());
        let layout = surface_layout_from_manifest(&manifest);
        assert_eq!(layout, generic_surface_layout_fallback());
    }
}
