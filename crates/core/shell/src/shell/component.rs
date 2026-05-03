use super::layout::{
    annotate_overflow_tree, find_click_handler, find_event_handler, find_focusable_at,
    find_node_bounds_by_key, find_node_by_key, find_node_path_at, find_scrollable_at,
    find_tooltip_text_by_key, is_input_key, is_slider_key, measure_content_size,
    namespace_event_handlers, node_tooltip_text, parse_namespaced_handler, scroll_limits,
};
use super::service::{apply_service_update, script_events_to_requests, seed_service_state};
use super::surface_layout::{
    SurfaceLayoutSettings, SurfaceSizePolicy, load_frontend_plugin_settings,
};
use super::types::{
    ComponentContext, ComponentError, ComponentInput, CoreEvent, CoreRequest, ServiceEvent,
    ShellComponent,
};
use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_diagnostics::Diagnostics;
use mesh_core_elements::{
    Corners, ElementState, StyleContext, StyleResolver, TransitionEasing, TransitionStyle,
    VariableStore, WidgetNode, element_snapshot_json, style::Color,
};
use mesh_core_locale::LocaleEngine;
use mesh_core_plugin::PluginType;
use mesh_core_plugin::lifecycle::PluginInstance;
use mesh_core_render::{
    CompiledFrontendPlugin, FrontendCompositionResolver, FrontendRenderMode,
    compile_frontend_plugin, root_accessibility_role,
};
use mesh_core_scripting::{LocaleBoundState, ScriptContext, ScriptInterfaceImport};
use mesh_core_theme::{Theme, default_theme};
use mesh_core_wayland::{Edge, ShellSurface};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::shell::ShellRunError;
use mesh_core_render::{PixelBuffer, SharedTextMeasurer, paint_frontend_tree_at};

const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
const TOOLTIP_OVERLAY_WIDTH: u32 = 260;
const TOOLTIP_OVERLAY_HEIGHT: u32 = 96;

#[derive(Debug, Clone)]
pub(super) struct BackendServiceCandidate {
    pub(super) plugin_id: String,
    pub(super) priority: u32,
}

pub(super) struct FrontendSurfaceComponent {
    pub(super) compiled: CompiledFrontendPlugin,
    pub(super) plugin_dir: PathBuf,
    plugin_settings_file: PathBuf,
    settings_json: serde_json::Value,
    pub(super) surface_layout: SurfaceLayoutSettings,
    pub(super) frontend_catalog: FrontendCatalog,
    pub(super) visible: bool,
    dirty: bool,
    last_service_update: Option<String>,
    focused_key: Option<String>,
    pointer_down_key: Option<String>,
    active_slider_key: Option<String>,
    last_audio_slider_percent: Option<u32>,
    input_values: HashMap<String, String>,
    slider_values: HashMap<String, f32>,
    checked_values: HashMap<String, bool>,
    render_hooks_pending: bool,
    pub(super) scroll_offsets: HashMap<String, ScrollOffsetState>,
    // Hover tracking for CSS :hover and tooltip system.
    hovered_key: Option<String>,
    hovered_path: Vec<String>,
    hovered_pos: (f32, f32),
    hover_start: Option<std::time::Instant>,
    runtimes: Arc<Mutex<HashMap<String, EmbeddedFrontendRuntime>>>,
    render_stack: RefCell<Vec<String>>,
    active_theme: RefCell<Theme>,
    measured_size: Option<(u32, u32)>,
    locale: LocaleEngine,
    interface_catalog: mesh_core_service::InterfaceCatalog,
    last_tree: Option<WidgetNode>,
    diagnostics: Option<Diagnostics>,
    /// Desired visibility for surface portals (`<ImportedSurface hidden={...} />`).
    /// Updated during build_tree; compared to last_surface_states in tick().
    pending_surface_states: RefCell<HashMap<String, bool>>,
    /// Last visibility state emitted for each surface portal, to avoid redundant requests.
    last_surface_states: HashMap<String, bool>,
    style_animations: HashMap<String, StyleAnimation>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ScrollOffsetState {
    pub(super) x: f32,
    pub(super) y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AnimatedVisualStyle {
    border_radius: Corners,
    opacity: f32,
    background_color: Color,
    color: Color,
}

impl AnimatedVisualStyle {
    fn from_node(node: &WidgetNode) -> Self {
        Self {
            border_radius: node.computed_style.border_radius,
            opacity: node.computed_style.opacity,
            background_color: node.computed_style.background_color,
            color: node.computed_style.color,
        }
    }

    fn apply_to_node(self, node: &mut WidgetNode) {
        node.computed_style.border_radius = self.border_radius;
        node.computed_style.opacity = self.opacity;
        node.computed_style.background_color = self.background_color;
        node.computed_style.color = self.color;
    }

    fn interpolate(self, target: Self, progress: f32) -> Self {
        Self {
            border_radius: lerp_corners(self.border_radius, target.border_radius, progress),
            opacity: lerp_f32(self.opacity, target.opacity, progress),
            background_color: lerp_color(self.background_color, target.background_color, progress),
            color: lerp_color(self.color, target.color, progress),
        }
    }

    fn selective_from(
        previous: Self,
        desired: Self,
        props: mesh_core_elements::TransitionProperties,
    ) -> Self {
        Self {
            border_radius: if props.animates_border_radius() {
                previous.border_radius
            } else {
                desired.border_radius
            },
            opacity: if props.animates_opacity() {
                previous.opacity
            } else {
                desired.opacity
            },
            background_color: if props.animates_background_color() {
                previous.background_color
            } else {
                desired.background_color
            },
            color: if props.animates_color() {
                previous.color
            } else {
                desired.color
            },
        }
    }
}

#[derive(Debug, Clone)]
struct StyleAnimation {
    from: AnimatedVisualStyle,
    to: AnimatedVisualStyle,
    started_at: Instant,
    duration: Duration,
    delay: Duration,
    transition: TransitionStyle,
}

impl StyleAnimation {
    fn current_style(&self, now: Instant) -> AnimatedVisualStyle {
        if self.duration.is_zero() {
            return self.to;
        }
        let elapsed = now.saturating_duration_since(self.started_at);
        if elapsed < self.delay {
            return self.from;
        }
        let active_elapsed = elapsed - self.delay;
        let raw = (active_elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0);
        self.from
            .interpolate(self.to, apply_easing(self.transition.easing, raw))
    }

    fn finished(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started_at) >= self.delay + self.duration
    }
}

#[derive(Debug, Clone)]
pub(super) struct FrontendCatalog {
    pub(super) plugins: HashMap<String, FrontendCatalogEntry>,
    slot_contributions: HashMap<String, Vec<ResolvedSlotContribution>>,
}

#[derive(Debug, Clone)]
pub(super) struct FrontendCatalogEntry {
    pub(super) plugin_dir: PathBuf,
    pub(super) compiled: CompiledFrontendPlugin,
}

#[derive(Debug, Clone)]
struct ResolvedSlotContribution {
    source_plugin_id: String,
    widget_id: String,
    contribution_id: String,
    order: i64,
    props: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug)]
struct EmbeddedFrontendRuntime {
    plugin_id: String,
    script_ctx: ScriptContext,
}

impl FrontendCatalog {
    pub(super) fn from_plugins(
        plugins: &HashMap<String, PluginInstance>,
    ) -> Result<Self, ShellRunError> {
        let mut plugin_ids: Vec<String> = plugins.keys().cloned().collect();
        plugin_ids.sort();

        let mut catalog = Self {
            plugins: HashMap::new(),
            slot_contributions: HashMap::new(),
        };

        for plugin_id in plugin_ids {
            let Some(plugin) = plugins.get(&plugin_id) else {
                continue;
            };

            if !mesh_core_render::is_frontend_plugin(&plugin.manifest) {
                continue;
            }

            let compiled =
                compile_frontend_plugin(&plugin.manifest, &plugin.path).map_err(|source| {
                    ShellRunError::FrontendCompile {
                        plugin_id: plugin_id.clone(),
                        source,
                    }
                })?;

            catalog.plugins.insert(
                plugin_id.clone(),
                FrontendCatalogEntry {
                    plugin_dir: plugin.path.clone(),
                    compiled,
                },
            );
        }

        for (plugin_id, entry) in &catalog.plugins {
            for (slot_id, contributions) in &entry.compiled.manifest.slot_contributions {
                let bucket = catalog
                    .slot_contributions
                    .entry(slot_id.clone())
                    .or_default();
                for (index, contribution) in contributions.iter().enumerate() {
                    bucket.push(ResolvedSlotContribution {
                        source_plugin_id: plugin_id.clone(),
                        widget_id: contribution
                            .widget
                            .clone()
                            .unwrap_or_else(|| plugin_id.clone()),
                        contribution_id: contribution
                            .id
                            .clone()
                            .unwrap_or_else(|| format!("{plugin_id}:{slot_id}:{index}")),
                        order: contribution.order.unwrap_or(0),
                        props: contribution.props.clone(),
                    });
                }
            }
        }

        for contributions in catalog.slot_contributions.values_mut() {
            contributions.sort_by(|left, right| {
                left.order
                    .cmp(&right.order)
                    .then_with(|| left.widget_id.cmp(&right.widget_id))
                    .then_with(|| left.contribution_id.cmp(&right.contribution_id))
            });
        }

        for (plugin_id, entry) in &catalog.plugins {
            for (alias, target_plugin_id) in &entry.compiled.plugin_component_imports {
                catalog
                    .validate_component_plugin_import(&entry.compiled.manifest, target_plugin_id)
                    .map_err(|message| ShellRunError::FrontendComposition {
                        message: format!(
                            "plugin '{plugin_id}' cannot import {alias} from '{target_plugin_id}': {message}"
                        ),
                    })?;
            }
            for component_tag in entry.compiled.referenced_component_tags() {
                if entry.compiled.local_components.contains_key(&component_tag) {
                    continue;
                }
                if entry
                    .compiled
                    .plugin_component_imports
                    .contains_key(&component_tag)
                {
                    continue;
                }
                return Err(ShellRunError::FrontendComposition {
                    message: format!(
                        "plugin '{plugin_id}' references <{component_tag}> but no explicit component import was compiled for that tag"
                    ),
                });
            }
        }

        Ok(catalog)
    }

    fn slot_contributions_for(&self, slot_id: &str) -> &[ResolvedSlotContribution] {
        self.slot_contributions
            .get(slot_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(super) fn top_level_surfaces(&self) -> Vec<FrontendCatalogEntry> {
        let mut entries: Vec<FrontendCatalogEntry> = self
            .plugins
            .values()
            .filter(|entry| entry.compiled.manifest.package.plugin_type == PluginType::Surface)
            .cloned()
            .collect();
        entries.sort_by(|left, right| {
            left.compiled
                .manifest
                .package
                .id
                .cmp(&right.compiled.manifest.package.id)
        });
        entries
    }

    fn validate_component_plugin_import(
        &self,
        host: &mesh_core_plugin::Manifest,
        plugin_id: &str,
    ) -> Result<(), String> {
        if !host
            .required_plugin_dependencies()
            .iter()
            .any(|dependency_id| dependency_id == plugin_id)
        {
            return Err("target plugin is not a required dependency".into());
        }
        let Some(entry) = self.plugins.get(plugin_id) else {
            return Err("target plugin is not loaded".into());
        };
        match entry.compiled.manifest.package.plugin_type {
            PluginType::Widget | PluginType::Surface => Ok(()),
            other => Err(format!(
                "target plugin must be a frontend widget or surface, got {other}"
            )),
        }
    }

    fn imported_component_plugin_id(
        &self,
        host: &mesh_core_plugin::Manifest,
        alias: &str,
    ) -> Result<String, String> {
        let Some(entry) = self.plugins.get(&host.package.id) else {
            return Err("host plugin is not loaded".into());
        };
        let Some(plugin_id) = entry.compiled.plugin_component_imports.get(alias) else {
            return Err(format!(
                "no explicit plugin import for component alias '{alias}'"
            ));
        };
        self.validate_component_plugin_import(host, plugin_id)?;
        Ok(plugin_id.clone())
    }
}

impl FrontendSurfaceComponent {
    pub(super) fn new(
        compiled: CompiledFrontendPlugin,
        plugin_dir: PathBuf,
        frontend_catalog: FrontendCatalog,
        interface_catalog: mesh_core_service::InterfaceCatalog,
    ) -> Self {
        let plugin_settings_file = plugin_dir.join("config/settings.json");
        let settings_state =
            load_frontend_plugin_settings(&plugin_settings_file, &compiled.manifest);
        Self {
            compiled,
            plugin_dir,
            plugin_settings_file,
            settings_json: settings_state.raw,
            surface_layout: settings_state.layout.clone(),
            frontend_catalog,
            visible: settings_state.layout.visible_on_start,
            dirty: true,
            last_service_update: None,
            focused_key: None,
            pointer_down_key: None,
            active_slider_key: None,
            last_audio_slider_percent: None,
            input_values: HashMap::new(),
            slider_values: HashMap::new(),
            checked_values: HashMap::new(),
            render_hooks_pending: true,
            scroll_offsets: HashMap::new(),
            hovered_key: None,
            hovered_path: Vec::new(),
            hovered_pos: (0.0, 0.0),
            hover_start: None,
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            render_stack: RefCell::new(Vec::new()),
            active_theme: RefCell::new(default_theme()),
            measured_size: None,
            locale: LocaleEngine::new("en"),
            interface_catalog,
            last_tree: None,
            diagnostics: None,
            pending_surface_states: RefCell::new(HashMap::new()),
            last_surface_states: HashMap::new(),
            style_animations: HashMap::new(),
        }
    }

    fn apply_style_animations(&mut self, tree: &mut WidgetNode) {
        let previous_styles = self
            .last_tree
            .as_ref()
            .map(collect_visual_styles)
            .unwrap_or_default();
        let now = Instant::now();
        let mut live_keys = HashSet::new();
        let mut has_active_animation = false;

        self.apply_style_animations_to_node(
            tree,
            &previous_styles,
            now,
            &mut live_keys,
            &mut has_active_animation,
        );

        self.style_animations
            .retain(|key, animation| live_keys.contains(key) && !animation.finished(now));

        if has_active_animation {
            self.dirty = true;
        }
    }

    fn apply_style_animations_to_node(
        &mut self,
        node: &mut WidgetNode,
        previous_styles: &HashMap<String, AnimatedVisualStyle>,
        now: Instant,
        live_keys: &mut HashSet<String>,
        has_active_animation: &mut bool,
    ) {
        if let Some(key) = node.attributes.get("_mesh_key").cloned() {
            live_keys.insert(key.clone());
            self.apply_node_style_animation(&key, node, previous_styles, now, has_active_animation);
        }

        for child in &mut node.children {
            self.apply_style_animations_to_node(
                child,
                previous_styles,
                now,
                live_keys,
                has_active_animation,
            );
        }
    }

    fn apply_node_style_animation(
        &mut self,
        key: &str,
        node: &mut WidgetNode,
        previous_styles: &HashMap<String, AnimatedVisualStyle>,
        now: Instant,
        has_active_animation: &mut bool,
    ) {
        let desired = AnimatedVisualStyle::from_node(node);
        let previous_displayed = self
            .style_animations
            .get(key)
            .map(|animation| animation.current_style(now))
            .or_else(|| previous_styles.get(key).copied())
            .unwrap_or(desired);

        let transition = node.computed_style.transition;
        let props = transition.properties;
        let should_animate = transition.duration_ms > 0
            && ((props.animates_border_radius()
                && previous_displayed.border_radius != desired.border_radius)
                || (props.animates_opacity() && previous_displayed.opacity != desired.opacity)
                || (props.animates_background_color()
                    && previous_displayed.background_color != desired.background_color)
                || (props.animates_color() && previous_displayed.color != desired.color));

        if should_animate {
            let restart = self.style_animations.get(key).is_none_or(|animation| {
                animation.to != desired
                    || animation.transition != transition
                    || animation.finished(now)
            });

            if restart {
                let from = AnimatedVisualStyle::selective_from(previous_displayed, desired, props);
                self.style_animations.insert(
                    key.to_string(),
                    StyleAnimation {
                        from,
                        to: desired,
                        started_at: now,
                        duration: Duration::from_millis(u64::from(transition.duration_ms)),
                        delay: Duration::from_millis(u64::from(transition.delay_ms)),
                        transition,
                    },
                );
            }
        } else {
            self.style_animations.remove(key);
        }

        if let Some(animation) = self.style_animations.get(key) {
            let current = animation.current_style(now);
            current.apply_to_node(node);
            if !animation.finished(now) {
                *has_active_animation = true;
            }
        }
    }

    fn layout_content_size(&self) -> (u32, u32) {
        let (width, height) = match self.surface_layout.size_policy {
            SurfaceSizePolicy::Fixed => (self.surface_layout.width, self.surface_layout.height),
            SurfaceSizePolicy::ContentMeasured => self
                .measured_size
                .unwrap_or((self.surface_layout.width, self.surface_layout.height)),
        };
        (width.max(1), height.max(1))
    }

    fn tooltip_overlay_extra_for_content(width: u32) -> (u32, u32) {
        if width < TOOLTIP_OVERLAY_WIDTH {
            (
                TOOLTIP_OVERLAY_WIDTH.saturating_sub(width),
                TOOLTIP_OVERLAY_HEIGHT,
            )
        } else {
            (0, 0)
        }
    }

    fn render_layout(&self, surface: &mut dyn ShellSurface) {
        surface.anchor(self.surface_layout.edge);
        surface.set_layer(self.surface_layout.layer);
        let (width, height) = self.layout_content_size();
        let (tooltip_extra_width, tooltip_extra_height) =
            Self::tooltip_overlay_extra_for_content(width);
        surface.set_size(
            width.saturating_add(tooltip_extra_width),
            height.saturating_add(tooltip_extra_height),
        );
        surface.set_exclusive_zone(self.surface_layout.exclusive_zone);
        surface.set_keyboard_interactivity(self.surface_layout.keyboard_mode);
        surface.set_margin(
            self.surface_layout.margin_top,
            self.surface_layout.margin_right,
            self.surface_layout.margin_bottom,
            self.surface_layout.margin_left,
        );
    }

    fn build_tree(&mut self, theme: &Theme, width: u32, height: u32) -> WidgetNode {
        if self.render_hooks_pending {
            self.call_render_hooks();
            self.render_hooks_pending = false;
        }
        self.active_theme.replace(theme.clone());
        let root_state = self.runtime_state(self.id()).unwrap_or_default();
        let bound = LocaleBoundState::new(&root_state, &self.locale);
        {
            let mut stack = self.render_stack.borrow_mut();
            stack.clear();
            stack.push(self.id().to_string());
        }
        let measurer = SharedTextMeasurer;
        let mut tree = self.compiled.build_tree_with_state(
            theme,
            width,
            height,
            Some(&bound),
            FrontendRenderMode::Surface,
            self.id(),
            Some(self),
            Some(&measurer),
        );
        self.render_stack.borrow_mut().clear();
        annotate_runtime_tree(
            &mut tree,
            "root".to_string(),
            &self.focused_key,
            &self.hovered_path,
            &self.pointer_down_key,
            &self.input_values,
            &self.slider_values,
            &self.checked_values,
            &self.scroll_offsets,
        );
        annotate_overflow_tree(&mut tree, "root", &mut self.scroll_offsets);

        let rules = self
            .compiled
            .component
            .style
            .as_ref()
            .map(|s| s.rules.as_slice())
            .unwrap_or(&[]);
        let resolver = StyleResolver::new(theme);
        let context = StyleContext {
            container_width: width as f32,
            container_height: height as f32,
        };
        resolver.restyle_subtree(&mut tree, rules, context);

        tree
    }

    fn update_slider_from_position(
        &mut self,
        tree: &WidgetNode,
        slider_key: &str,
        x: f32,
        y: f32,
    ) -> Option<CoreRequest> {
        let Some(node) = find_node_by_key(tree, slider_key) else {
            return None;
        };
        let action = node.attributes.get("mesh-action").cloned();
        let is_vertical = node
            .attributes
            .get("orient")
            .map(|v| v == "vertical")
            .unwrap_or(false);
        let Some((left, top, right, bottom)) = find_node_bounds_by_key(tree, slider_key, 0.0, 0.0)
        else {
            return None;
        };

        let min = node
            .attributes
            .get("min")
            .and_then(|value: &String| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let max = node
            .attributes
            .get("max")
            .and_then(|value: &String| value.parse::<f32>().ok())
            .unwrap_or(100.0);

        if max <= min {
            return None;
        }

        let pct = if is_vertical {
            // Vertical: top = 100%, bottom = 0% (inverted Y axis).
            let height = (bottom - top).max(1.0);
            let local_y = (y - top).clamp(0.0, height);
            1.0 - (local_y / height).clamp(0.0, 1.0)
        } else {
            let width = (right - left).max(1.0);
            let local_x = (x - left).clamp(0.0, width);
            (local_x / width).clamp(0.0, 1.0)
        };
        let value = min + (max - min) * pct;
        self.slider_values.insert(slider_key.to_string(), value);
        if action.as_deref() == Some("audio-volume") {
            let percent = value.round().clamp(0.0, 100.0) as u32;
            self.update_local_audio_percent(percent);
            if self.last_audio_slider_percent != Some(percent) {
                self.last_audio_slider_percent = Some(percent);
                return Some(CoreRequest::ServiceCommand {
                    interface: "mesh.audio".to_string(),
                    command: "set-volume".to_string(),
                    payload: serde_json::json!({ "percent": percent }),
                    source_plugin_id: self.id().to_string(),
                    source_capabilities: self.source_capabilities(),
                });
            }
        }
        None
    }

    fn slider_value(&self, tree: &WidgetNode, slider_key: &str) -> Option<f32> {
        self.slider_values.get(slider_key).copied().or_else(|| {
            find_node_by_key(tree, slider_key).and_then(|node| {
                node.attributes
                    .get("value")
                    .and_then(|value| value.parse::<f32>().ok())
            })
        })
    }

    fn current_checked_value(&self, tree: &WidgetNode, key: &str) -> bool {
        self.checked_values.get(key).copied().unwrap_or_else(|| {
            find_node_by_key(tree, key)
                .and_then(|node| node.attributes.get("checked"))
                .is_some_and(|value| matches!(value.as_str(), "true" | "1" | "checked"))
        })
    }

    fn toggle_checked_value(&mut self, tree: &WidgetNode, key: &str) -> bool {
        let next = !self.current_checked_value(tree, key);
        self.checked_values.insert(key.to_string(), next);
        next
    }

    fn call_node_handler(
        &mut self,
        tree: &WidgetNode,
        node_key: &str,
        event_name: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(handler) = find_event_handler(tree, node_key, event_name) else {
            return Ok(Vec::new());
        };
        self.call_namespaced_handler(&handler, args)
    }

    fn call_render_hooks(&mut self) {
        let mut runtimes = self.runtimes.lock().unwrap();
        for runtime in runtimes.values_mut() {
            if !runtime.script_ctx.has_handler("onRender") {
                continue;
            }

            if let Err(source) = runtime.script_ctx.call_handler("onRender", &[]) {
                let component_id = runtime.plugin_id.clone();
                let error_message = source.to_string();
                tracing::warn!(
                    component_id = %component_id,
                    handler = "onRender",
                    error = %error_message,
                    "frontend render hook failed"
                );
                if let Some(diagnostics) = &self.diagnostics {
                    diagnostics.record_handler_error(
                        component_id,
                        "onRender".to_string(),
                        error_message,
                    );
                }
                Self::drain_script_diagnostics(&self.diagnostics, runtime);
                continue;
            }
            Self::drain_script_diagnostics(&self.diagnostics, runtime);

            if runtime.script_ctx.state().is_dirty() {
                self.dirty = true;
            }
        }
    }

    fn drain_script_diagnostics(
        diagnostics: &Option<Diagnostics>,
        runtime: &mut EmbeddedFrontendRuntime,
    ) {
        let Some(diagnostics) = diagnostics else {
            return;
        };
        for diagnostic in runtime.script_ctx.drain_diagnostics() {
            diagnostics.error(format!(
                "interface '{}' unavailable for '{}': {}",
                diagnostic.interface, diagnostic.plugin_id, diagnostic.reason
            ));
        }
    }

    fn pointer_event_target_key(&self, tree: &WidgetNode, x: f32, y: f32) -> Option<String> {
        find_focusable_at(tree, x, y).or_else(|| {
            find_node_path_at(tree, x, y).and_then(|path| {
                path.into_iter()
                    .rev()
                    .find(|key| find_event_handler(tree, key, "click").is_some())
            })
        })
    }

    fn update_local_audio_percent(&self, percent: u32) {
        let percent = percent.min(100);
        for runtime in self.runtimes.lock().unwrap().values_mut() {
            if !runtime
                .script_ctx
                .capabilities
                .is_granted(&Capability::new("service.audio.read"))
            {
                continue;
            }
            let mut audio = runtime
                .script_ctx
                .state()
                .get("audio")
                .unwrap_or_else(|| serde_json::json!({}));
            if let Some(obj) = audio.as_object_mut() {
                obj.insert("percent".into(), serde_json::Value::from(percent));
            }
            runtime.script_ctx.state_mut().set("audio", audio);
        }
    }

    fn slider_release_request(&self, tree: &WidgetNode, slider_key: &str) -> Option<CoreRequest> {
        let node = find_node_by_key(tree, slider_key)?;
        match node.attributes.get("mesh-action").map(String::as_str) {
            Some("audio-volume") => {
                let value = self
                    .slider_values
                    .get(slider_key)
                    .copied()
                    .or_else(|| {
                        node.attributes
                            .get("value")
                            .and_then(|value| value.parse::<f32>().ok())
                    })
                    .unwrap_or(0.0);
                let percent = value.round().clamp(0.0, 100.0) as u32;
                Some(CoreRequest::ServiceCommand {
                    interface: "mesh.audio".to_string(),
                    command: "set-volume".to_string(),
                    payload: serde_json::json!({ "percent": percent }),
                    source_plugin_id: self.id().to_string(),
                    source_capabilities: self.source_capabilities(),
                })
            }
            _ => None,
        }
    }

    fn source_capabilities(&self) -> CapabilitySet {
        grant_capabilities_from_manifest(&self.compiled.manifest)
    }

    fn runtime_state(&self, instance_key: &str) -> Option<mesh_core_scripting::ScriptState> {
        self.runtimes
            .lock()
            .unwrap()
            .get(instance_key)
            .map(|runtime| runtime.script_ctx.state().clone())
    }

    /// Load translation files from `config/i18n/{locale}.json` inside the plugin directory.
    fn load_plugin_i18n_from_dir(&mut self, plugin_dir: &Path) {
        let i18n_dir = plugin_dir.join("config/i18n");
        let entries = match std::fs::read_dir(&i18n_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let messages: HashMap<String, String> = match serde_json::from_str(&content) {
                Ok(m) => m,
                Err(_) => {
                    tracing::warn!(
                        "plugin '{}': failed to parse i18n file {}",
                        self.id(),
                        path.display()
                    );
                    continue;
                }
            };
            tracing::debug!(
                "plugin '{}': loaded {} translations for locale '{}'",
                self.id(),
                messages.len(),
                stem
            );
            self.locale
                .load_translations(mesh_core_locale::TranslationSet {
                    locale: stem.to_string(),
                    messages,
                });
        }
    }

    fn load_plugin_i18n(&mut self) {
        let plugin_dir = self.plugin_dir.clone();
        self.load_plugin_i18n_from_dir(&plugin_dir);
    }

    fn load_catalog_i18n(&mut self) {
        let plugin_dirs: Vec<PathBuf> = self
            .frontend_catalog
            .plugins
            .values()
            .map(|entry| entry.plugin_dir.clone())
            .collect();
        for plugin_dir in plugin_dirs {
            self.load_plugin_i18n_from_dir(&plugin_dir);
        }
    }

    fn create_runtime_for_component(
        &self,
        component_id: String,
        manifest: &mesh_core_plugin::Manifest,
        component: &mesh_core_component::ComponentFile,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<EmbeddedFrontendRuntime, ComponentError> {
        let mut script_ctx = ScriptContext::new(
            component_id.clone(),
            grant_capabilities_from_manifest(manifest),
        )
        .map_err(|source| ComponentError::Script {
            component_id: component_id.clone(),
            source,
        })?;
        script_ctx.set_interface_catalog(self.interface_catalog.clone());
        seed_service_state(script_ctx.state_mut());

        for (key, value) in props {
            script_ctx.state_mut().set(key.clone(), value.clone());
        }

        if let Some(script) = &component.script {
            let interface_imports = component
                .imports
                .iter()
                .filter_map(|import| match &import.target {
                    mesh_core_component::ComponentImportTarget::InterfaceApi {
                        interface,
                        version,
                    } => Some(ScriptInterfaceImport {
                        alias: import.alias.clone(),
                        interface: interface.clone(),
                        version: version.clone(),
                    }),
                    _ => None,
                })
                .collect::<Vec<_>>();
            script_ctx
                .load_script_with_interface_imports(&script.source, &interface_imports)
                .map_err(|source| ComponentError::Script {
                    component_id: component_id.clone(),
                    source,
                })?;
            script_ctx
                .call_init()
                .map_err(|source| ComponentError::Script {
                    component_id: component_id.clone(),
                    source,
                })?;
        }

        Ok(EmbeddedFrontendRuntime {
            plugin_id: component_id,
            script_ctx,
        })
    }

    fn create_runtime(
        &self,
        compiled: &CompiledFrontendPlugin,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<EmbeddedFrontendRuntime, ComponentError> {
        self.create_runtime_for_component(
            compiled.manifest.package.id.clone(),
            &compiled.manifest,
            &compiled.component,
            props,
        )
    }

    fn init_root_runtime(&self) -> Result<(), ComponentError> {
        let mut props = HashMap::new();
        props.insert("settings".into(), self.settings_json.clone());
        let runtime = self.create_runtime(&self.compiled, &props)?;
        self.runtimes
            .lock()
            .unwrap()
            .insert(self.id().to_string(), runtime);
        Ok(())
    }

    fn ensure_runtime(
        &self,
        instance_key: &str,
        plugin_id: &str,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<(), ComponentError> {
        if !self.runtimes.lock().unwrap().contains_key(instance_key) {
            let Some(entry) = self.frontend_catalog.plugins.get(plugin_id) else {
                return Err(ComponentError::Failed {
                    component_id: self.id().to_string(),
                    message: format!("missing embedded frontend plugin '{plugin_id}'"),
                });
            };
            let runtime = self.create_runtime(&entry.compiled, props)?;
            self.runtimes
                .lock()
                .unwrap()
                .insert(instance_key.to_string(), runtime);
        }

        if let Some(runtime) = self.runtimes.lock().unwrap().get_mut(instance_key) {
            for (key, value) in props {
                runtime
                    .script_ctx
                    .state_mut()
                    .set(key.clone(), value.clone());
            }
        }

        Ok(())
    }

    fn build_error_widget(&self, message: impl Into<String>) -> WidgetNode {
        let message = message.into();
        let mut node = WidgetNode::new("box");
        let mut text = WidgetNode::new("text");
        text.attributes.insert("content".into(), message.clone());
        node.attributes.insert("content".into(), message);
        node.children.push(text);
        node
    }

    fn ensure_local_component_runtime(
        &self,
        instance_key: &str,
        host_plugin_id: &str,
        alias: &str,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<(), ComponentError> {
        if !self.runtimes.lock().unwrap().contains_key(instance_key) {
            let Some(entry) = self.frontend_catalog.plugins.get(host_plugin_id) else {
                return Err(ComponentError::Failed {
                    component_id: self.id().to_string(),
                    message: format!("missing host plugin '{host_plugin_id}'"),
                });
            };
            let Some(component) = entry.compiled.local_components.get(alias) else {
                return Err(ComponentError::Failed {
                    component_id: self.id().to_string(),
                    message: format!("missing local component import '{alias}'"),
                });
            };
            let runtime = self.create_runtime_for_component(
                format!("{host_plugin_id}::{alias}"),
                &entry.compiled.manifest,
                component,
                props,
            )?;
            self.runtimes
                .lock()
                .unwrap()
                .insert(instance_key.to_string(), runtime);
        }

        if let Some(runtime) = self.runtimes.lock().unwrap().get_mut(instance_key) {
            for (key, value) in props {
                runtime
                    .script_ctx
                    .state_mut()
                    .set(key.clone(), value.clone());
            }
        }

        Ok(())
    }

    fn render_local_component(
        &self,
        host: &mesh_core_plugin::Manifest,
        alias: &str,
        instance_key: &str,
        props: &HashMap<String, serde_json::Value>,
        container_width: f32,
        container_height: f32,
    ) -> WidgetNode {
        if let Err(err) =
            self.ensure_local_component_runtime(instance_key, &host.package.id, alias, props)
        {
            return self.build_error_widget(err.to_string());
        }

        let Some(entry) = self.frontend_catalog.plugins.get(&host.package.id) else {
            return self.build_error_widget(format!("missing host plugin '{}'", host.package.id));
        };
        let Some(component) = entry.compiled.local_components.get(alias) else {
            return self.build_error_widget(format!("missing local component import '{alias}'"));
        };

        let theme = self.active_theme.borrow().clone();
        let state = self.runtime_state(instance_key).unwrap_or_default();
        let bound = LocaleBoundState::new(&state, &self.locale);
        let host_rules = entry
            .compiled
            .component
            .style
            .as_ref()
            .map(|style| style.rules.as_slice())
            .unwrap_or(&[]);
        let mut node = mesh_core_render::build_widget_tree_from_component(
            component,
            host,
            &theme,
            container_width,
            container_height,
            Some(self),
            instance_key,
            Some(&bound),
            host_rules,
        );
        namespace_event_handlers(&mut node, instance_key);
        node
    }

    fn render_embedded_instance(
        &self,
        instance_key: &str,
        plugin_id: &str,
        props: &HashMap<String, serde_json::Value>,
        container_width: f32,
        container_height: f32,
    ) -> WidgetNode {
        if self
            .render_stack
            .borrow()
            .iter()
            .filter(|ancestor| ancestor.as_str() == plugin_id)
            .count()
            >= 2
        {
            return self.build_error_widget(format!("composition cycle blocked for '{plugin_id}'"));
        }

        if let Err(err) = self.ensure_runtime(instance_key, plugin_id, props) {
            return self.build_error_widget(err.to_string());
        }

        let Some(entry) = self.frontend_catalog.plugins.get(plugin_id) else {
            return self.build_error_widget(format!("missing embedded plugin '{plugin_id}'"));
        };

        let state = self.runtime_state(instance_key).unwrap_or_default();
        let bound = LocaleBoundState::new(&state, &self.locale);
        let active_theme = self.active_theme.borrow().clone();
        self.render_stack.borrow_mut().push(plugin_id.to_string());
        let measurer = SharedTextMeasurer;
        let mut tree = entry.compiled.build_tree_with_state(
            &active_theme,
            container_width.max(0.0).ceil() as u32,
            container_height.max(0.0).ceil() as u32,
            Some(&bound),
            FrontendRenderMode::Embedded,
            instance_key,
            Some(self),
            Some(&measurer),
        );
        self.render_stack.borrow_mut().pop();
        namespace_event_handlers(&mut tree, instance_key);
        tree
    }

    fn call_namespaced_handler(
        &mut self,
        handler: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let (instance_key, handler_name, component_id) =
            if let Some((instance_key, handler_name)) = parse_namespaced_handler(handler) {
                let component_id = self
                    .runtimes
                    .lock()
                    .unwrap()
                    .get(instance_key)
                    .map(|runtime| runtime.plugin_id.clone())
                    .unwrap_or_else(|| self.id().to_string());
                (
                    instance_key.to_string(),
                    handler_name.to_string(),
                    component_id,
                )
            } else {
                (
                    self.id().to_string(),
                    handler.to_string(),
                    self.id().to_string(),
                )
            };

        let mut runtimes = self.runtimes.lock().unwrap();
        let Some(runtime) = runtimes.get_mut(&instance_key) else {
            return Ok(Vec::new());
        };
        if let Err(source) = runtime.script_ctx.call_handler(&handler_name, args) {
            let error_message = source.to_string();
            tracing::warn!(
                component_id = %component_id,
                handler = %handler_name,
                error = %error_message,
                "frontend event handler failed"
            );
            if let Some(diagnostics) = &self.diagnostics {
                diagnostics.record_handler_error(
                    component_id.clone(),
                    handler_name.clone(),
                    error_message,
                );
            }
            Self::drain_script_diagnostics(&self.diagnostics, runtime);
            return Ok(Vec::new());
        }
        Self::drain_script_diagnostics(&self.diagnostics, runtime);
        if runtime.script_ctx.state().is_dirty() {
            self.dirty = true;
        }

        Ok(script_events_to_requests(
            runtime.script_ctx.drain_published_events(),
        ))
    }

    fn build_click_event(
        &self,
        tree: &WidgetNode,
        node_key: &str,
        x: f32,
        y: f32,
    ) -> serde_json::Value {
        let target = find_node_by_key(tree, node_key);
        let (left, top, right, bottom) =
            find_node_bounds_by_key(tree, node_key, 0.0, 0.0).unwrap_or((0.0, 0.0, 0.0, 0.0));
        let width = (right - left).max(0.0);
        let height = (bottom - top).max(0.0);
        let bounds = serde_json::json!({
            "left": left,
            "top": top,
            "right": right,
            "bottom": bottom,
            "width": width,
            "height": height,
        });
        let position = serde_json::json!({
            "margin_left": left.round() as i32,
            "margin_top": bottom.round() as i32,
        });
        let tag = target.map(|node| node.tag.clone()).unwrap_or_default();
        let mut current_target = target
            .map(|node| element_snapshot_json(node, left - node.layout.x, top - node.layout.y))
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(object) = current_target.as_object_mut() {
            object.insert(
                "key".into(),
                serde_json::Value::String(node_key.to_string()),
            );
            object.insert("tag".into(), serde_json::Value::String(tag.clone()));
            object.insert("bounds".into(), bounds.clone());
            object.insert("position".into(), position.clone());
        }

        serde_json::json!({
            "type": "click",
            "pointer": {
                "x": x,
                "y": y,
            },
            "surface": {
                "id": self.surface_id(),
                "width": tree.layout.width,
                "height": tree.layout.height,
            },
            "current": {
                "key": node_key,
                "tag": tag,
                "bounds": bounds,
                "position": position,
            },
            "current_target": current_target
        })
    }
}

impl FrontendCompositionResolver for FrontendSurfaceComponent {
    fn render_import(
        &self,
        host: &mesh_core_plugin::Manifest,
        host_instance_key: &str,
        alias: &str,
        props: &HashMap<String, String>,
        container_width: f32,
        container_height: f32,
    ) -> Option<WidgetNode> {
        if let Some(entry) = self.frontend_catalog.plugins.get(&host.package.id) {
            if entry.compiled.local_components.contains_key(alias) {
                let props_json: HashMap<String, serde_json::Value> = props
                    .iter()
                    .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
                    .collect();
                let instance_key = format!("{host_instance_key}/local:{alias}");
                return Some(self.render_local_component(
                    host,
                    alias,
                    &instance_key,
                    &props_json,
                    container_width,
                    container_height,
                ));
            }
        }

        let plugin_id = match self
            .frontend_catalog
            .imported_component_plugin_id(host, alias)
        {
            Ok(id) => id,
            Err(message) => return Some(self.build_error_widget(message)),
        };

        // Surface plugins are portals: their visibility is tracked via pending_surface_states
        // and translated to ShowSurface/HideSurface requests in tick(). They render nothing inline.
        let is_surface = self
            .frontend_catalog
            .plugins
            .get(&plugin_id)
            .map(|e| e.compiled.manifest.package.plugin_type == PluginType::Surface)
            .unwrap_or(false);
        if is_surface {
            let hidden = props
                .get("hidden")
                .map(|v| v == "true" || v == "True")
                .unwrap_or(false);
            self.pending_surface_states
                .borrow_mut()
                .insert(plugin_id, !hidden);
            return Some(WidgetNode::new("box")); // placeholder, takes no space
        }

        let props_json: HashMap<String, serde_json::Value> = props
            .iter()
            .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
            .collect();
        let instance_key = format!("{host_instance_key}/import:{alias}");
        Some(self.render_embedded_instance(
            &instance_key,
            &plugin_id,
            &props_json,
            container_width,
            container_height,
        ))
    }

    fn render_slot(
        &self,
        host: &mesh_core_plugin::Manifest,
        host_instance_key: &str,
        slot_name: Option<&str>,
        container_width: f32,
        container_height: f32,
    ) -> Vec<WidgetNode> {
        let Some(slot_name) = slot_name else {
            return Vec::new();
        };

        let slot_id = format!("{}:{slot_name}", host.package.id);
        let accepts_widget = host
            .provides_slots
            .get(slot_name)
            .and_then(|definition| definition.accepts.as_deref())
            .map(|accepts| accepts == "widget")
            .unwrap_or(false);

        let mut nodes = Vec::new();
        for contribution in self.frontend_catalog.slot_contributions_for(&slot_id) {
            let Some(entry) = self.frontend_catalog.plugins.get(&contribution.widget_id) else {
                nodes.push(self.build_error_widget(format!(
                    "slot '{slot_id}' references missing plugin '{}'",
                    contribution.widget_id
                )));
                continue;
            };

            if accepts_widget && entry.compiled.manifest.package.plugin_type != PluginType::Widget {
                nodes.push(self.build_error_widget(format!(
                    "slot '{slot_id}' accepts widgets, but '{}' is {}",
                    contribution.widget_id, entry.compiled.manifest.package.plugin_type
                )));
                continue;
            }

            let props_json: HashMap<String, serde_json::Value> = contribution
                .props
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            let instance_key = format!(
                "{host_instance_key}/slot:{slot_name}/{}",
                contribution.contribution_id
            );
            let mut node = self.render_embedded_instance(
                &instance_key,
                &contribution.widget_id,
                &props_json,
                container_width,
                container_height,
            );
            node.attributes.insert(
                "_mesh_slot_source".into(),
                contribution.source_plugin_id.clone(),
            );
            nodes.push(node);
        }

        nodes
    }
}

impl ShellComponent for FrontendSurfaceComponent {
    fn id(&self) -> &str {
        &self.compiled.manifest.package.id
    }

    fn surface_id(&self) -> &str {
        self.compiled.surface_id()
    }

    fn initial_visibility(&self) -> Option<bool> {
        Some(self.surface_layout.visible_on_start)
    }

    fn mount(&mut self, ctx: ComponentContext) -> Result<Vec<CoreRequest>, ComponentError> {
        self.diagnostics = Some(ctx.diagnostics);
        self.load_plugin_i18n();
        self.load_catalog_i18n();
        self.init_root_runtime()?;
        self.render_hooks_pending = true;
        self.dirty = true;
        Ok(vec![CoreRequest::PublishDiagnostics {
            message: format!(
                "mounted frontend component '{}' from {}",
                self.id(),
                self.compiled.source_path.display()
            ),
        }])
    }

    fn handle_core_event(&mut self, event: &CoreEvent) -> Result<Vec<CoreRequest>, ComponentError> {
        if let CoreEvent::SurfaceVisibilityChanged {
            surface_id,
            visible,
        } = event
        {
            if surface_id == self.surface_id() {
                self.visible = *visible;
                self.dirty = true;
            }
        }
        Ok(Vec::new())
    }

    fn handle_service_event(
        &mut self,
        event: &ServiceEvent,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let ServiceEvent::Updated {
            service,
            source_plugin,
            payload,
        } = event;
        self.last_service_update = Some(format!("{service}:{source_plugin}"));
        for runtime in self.runtimes.lock().unwrap().values_mut() {
            let service_name = super::service::service_name_from_interface(service);
            let required = format!("service.{service_name}.read");
            let has_read = runtime
                .script_ctx
                .capabilities
                .is_granted(&Capability::new(&required));
            let previous = runtime.script_ctx.state().get(&service_name);
            let tracked_fields = runtime.script_ctx.tracked_fields_for_service(&service_name);
            apply_service_update(
                runtime.script_ctx.state_mut(),
                has_read,
                service,
                source_plugin,
                payload.clone(),
            );
            let state_changed = runtime.script_ctx.state().is_dirty();
            if has_read {
                runtime
                    .script_ctx
                    .apply_service_payload(&service_name, payload);
            }
            let tracked_fields_changed = has_read
                && tracked_service_fields_changed(previous.as_ref(), payload, &tracked_fields);
            if state_changed || tracked_fields_changed {
                self.render_hooks_pending = true;
                self.dirty = true;
            }
        }
        Ok(Vec::new())
    }

    fn tick(&mut self) -> Result<Vec<CoreRequest>, ComponentError> {
        // Trigger a repaint once the tooltip delay has elapsed so the tooltip appears.
        if let Some(start) = self.hover_start {
            if start.elapsed() >= TOOLTIP_DELAY && !self.dirty {
                self.dirty = true;
            }
        }

        // Emit Show/HideSurface requests for surface portals whose desired visibility changed.
        let pending = std::mem::take(&mut *self.pending_surface_states.borrow_mut());
        let mut requests = Vec::new();
        for (surface_id, visible) in pending {
            let was_visible = self.last_surface_states.get(&surface_id).copied();
            if was_visible != Some(visible) {
                self.last_surface_states.insert(surface_id.clone(), visible);
                if visible {
                    requests.push(CoreRequest::ShowSurface { surface_id });
                } else {
                    requests.push(CoreRequest::HideSurface { surface_id });
                }
            }
        }
        Ok(requests)
    }

    fn wants_render(&self) -> bool {
        self.dirty || !self.style_animations.is_empty()
    }

    fn render(&mut self, surface: &mut dyn ShellSurface) -> Result<(), ComponentError> {
        self.render_layout(surface);

        if self.visible {
            surface.show();
        } else {
            surface.hide();
        }

        let template_nodes = self
            .compiled
            .component
            .template
            .as_ref()
            .map(|template| template.root.len())
            .unwrap_or(0);
        let role =
            root_accessibility_role(&self.compiled.manifest).unwrap_or_else(|| "unknown".into());

        tracing::debug!(
            "rendered frontend '{}' visible={} nodes={} role={}{}",
            self.id(),
            self.visible,
            template_nodes,
            role,
            self.last_service_update
                .as_deref()
                .map(|summary| format!(" service={summary}"))
                .unwrap_or_default()
        );

        self.dirty = false;
        Ok(())
    }

    fn paint(
        &mut self,
        theme: &Theme,
        _width: u32,
        _height: u32,
        buffer: &mut PixelBuffer,
    ) -> Result<(), ComponentError> {
        let (content_width, content_height) = self.layout_content_size();
        let mut tree = self.build_tree(theme, content_width, content_height);
        self.apply_style_animations(&mut tree);
        if self.surface_layout.size_policy == SurfaceSizePolicy::ContentMeasured {
            let surface_layout_manifest = self.compiled.manifest.surface_layout.as_ref();
            let measured_size = measure_content_size(
                &tree,
                content_width,
                content_height,
                surface_layout_manifest,
            );
            if self.measured_size != Some(measured_size) {
                self.measured_size = Some(measured_size);
                self.dirty = true;
            }
        }
        self.publish_element_metrics(&tree);
        buffer.clear(mesh_core_elements::style::Color::TRANSPARENT);

        let tooltip = if let (Some(start), Some(hovered_key)) =
            (self.hover_start, self.hovered_key.as_ref())
        {
            if start.elapsed() >= TOOLTIP_DELAY {
                find_tooltip_text_by_key(&tree, hovered_key).map(|text| {
                    let (cx, cy) = self.hovered_pos;
                    (text, cx, cy)
                })
            } else {
                None
            }
        } else {
            None
        };

        paint_frontend_tree_at(
            &tree,
            buffer,
            1.0,
            0.0,
            0.0,
            tooltip
                .as_ref()
                .map(|(text, cx, cy)| (text.as_str(), *cx, *cy)),
        );
        self.last_tree = Some(tree);
        self.clear_runtime_dirty_states();

        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), ComponentError> {
        self.dirty = true;
        Ok(())
    }

    fn locale_changed(&mut self, locale: &LocaleEngine) -> Result<(), ComponentError> {
        self.locale.set_locale(locale.current());
        self.runtimes.lock().unwrap().clear();
        self.init_root_runtime()?;
        self.render_hooks_pending = true;
        self.dirty = true;
        Ok(())
    }

    fn source_path(&self) -> Option<&Path> {
        Some(self.compiled.source_path.as_path())
    }

    fn plugin_settings_path(&self) -> Option<&Path> {
        if self.plugin_settings_file.exists() {
            Some(self.plugin_settings_file.as_path())
        } else {
            None
        }
    }

    fn reload_plugin_settings(&mut self) -> Result<bool, ComponentError> {
        let settings_state =
            load_frontend_plugin_settings(&self.plugin_settings_file, &self.compiled.manifest);
        let layout_changed = self.surface_layout != settings_state.layout;
        let settings_changed = self.settings_json != settings_state.raw;

        self.surface_layout = settings_state.layout;
        self.settings_json = settings_state.raw;

        if settings_changed {
            if let Some(runtime) = self.runtimes.lock().unwrap().get_mut(self.id()) {
                runtime
                    .script_ctx
                    .state_mut()
                    .set("settings", self.settings_json.clone());
            }
        }

        let Some(locale) = self
            .settings_json
            .get("i18n")
            .and_then(|i18n| i18n.get("default_locale"))
            .and_then(|l| l.as_str())
        else {
            if layout_changed || settings_changed {
                self.dirty = true;
            }
            return Ok(layout_changed || settings_changed);
        };

        if self.locale.current() != locale {
            tracing::info!(
                "plugin '{}': applying locale '{}' from plugin settings",
                self.id(),
                locale
            );
            self.locale.set_locale(locale);
        }

        if layout_changed || settings_changed {
            self.dirty = true;
        }
        Ok(layout_changed || settings_changed)
    }

    fn reload_source(&mut self) -> Result<bool, ComponentError> {
        let manifest = self.compiled.manifest.clone();
        let recompiled = compile_frontend_plugin(&manifest, &self.plugin_dir).map_err(|err| {
            ComponentError::Failed {
                component_id: self.id().to_string(),
                message: format!("frontend recompile failed: {err}"),
            }
        })?;

        let component_id = self.id().to_string();
        self.compiled = recompiled;
        if let Some(entry) = self.frontend_catalog.plugins.get_mut(&component_id) {
            entry.compiled = self.compiled.clone();
        }
        self.runtimes.lock().unwrap().clear();
        self.init_root_runtime()?;
        self.render_hooks_pending = true;
        self.dirty = true;
        Ok(true)
    }

    fn handle_input(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        input: ComponentInput,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        tracing::trace!(
            "[hover] handle_input called: id={} visible={} input={:?}",
            self.id(),
            self.visible,
            std::mem::discriminant(&input)
        );
        if !self.visible {
            return Ok(Vec::new());
        }

        let tree = self
            .last_tree
            .clone()
            .unwrap_or_else(|| self.build_tree(theme, width, height));

        match input {
            ComponentInput::PointerButton { x, y, pressed } => {
                if pressed {
                    if let Some(node_key) = self.pointer_event_target_key(&tree, x, y) {
                        self.pointer_down_key = Some(node_key.clone());
                        let mut requests = Vec::new();

                        if let Some(focused_key) = find_focusable_at(&tree, x, y) {
                            let focus_changed =
                                self.focused_key.as_deref() != Some(focused_key.as_str());
                            self.focused_key = Some(focused_key.clone());
                            if focus_changed {
                                requests.extend(self.call_node_handler(
                                    &tree,
                                    &focused_key,
                                    "focus",
                                    &[],
                                )?);
                            }
                        } else {
                            self.focused_key = None;
                        }

                        if is_slider_key(&tree, &node_key) {
                            self.active_slider_key = Some(node_key.clone());
                            self.last_audio_slider_percent = None;
                            if let Some(request) =
                                self.update_slider_from_position(&tree, &node_key, x, y)
                            {
                                requests.push(request);
                            }
                            if let Some(value) = self.slider_value(&tree, &node_key) {
                                requests.extend(self.call_node_handler(
                                    &tree,
                                    &node_key,
                                    "change",
                                    &[serde_json::json!(value)],
                                )?);
                            }
                        } else {
                            self.active_slider_key = None;
                            self.last_audio_slider_percent = None;
                            if find_node_by_key(&tree, &node_key).is_some_and(|node| {
                                matches!(node.tag.as_str(), "switch" | "checkbox")
                            }) {
                                let value = self.toggle_checked_value(&tree, &node_key);
                                requests.extend(self.call_node_handler(
                                    &tree,
                                    &node_key,
                                    "change",
                                    &[serde_json::json!(value)],
                                )?);
                            }
                        }

                        self.dirty = true;
                        if !requests.is_empty() {
                            return Ok(requests);
                        }
                    } else {
                        self.focused_key = None;
                        self.pointer_down_key = None;
                        self.active_slider_key = None;
                        self.last_audio_slider_percent = None;
                        self.dirty = true;
                    }
                } else {
                    let mut requests = Vec::new();
                    let slider_request = self
                        .active_slider_key
                        .as_ref()
                        .and_then(|slider_key| self.slider_release_request(&tree, slider_key));

                    if let Some(slider_key) = self.active_slider_key.clone()
                        && let Some(value) = self.slider_value(&tree, &slider_key)
                    {
                        requests.extend(self.call_node_handler(
                            &tree,
                            &slider_key,
                            "release",
                            &[serde_json::json!(value)],
                        )?);
                    }

                    if let Some(node_key) = self.pointer_event_target_key(&tree, x, y) {
                        if self.pointer_down_key.as_deref() == Some(node_key.as_str()) {
                            if let Some(handler) = find_click_handler(&tree, &node_key) {
                                let click_event = self.build_click_event(&tree, &node_key, x, y);
                                requests.extend(
                                    self.call_namespaced_handler(&handler, &[click_event])?,
                                );
                            }
                        }
                    }
                    self.pointer_down_key = None;
                    self.active_slider_key = None;
                    self.last_audio_slider_percent = None;
                    if let Some(request) = slider_request {
                        requests.push(request);
                    }
                    if !requests.is_empty() {
                        self.dirty = true;
                        return Ok(requests);
                    }
                }
            }
            ComponentInput::PointerMove { x, y } => {
                if let Some(slider_key) = self.active_slider_key.clone() {
                    let request = self.update_slider_from_position(&tree, &slider_key, x, y);
                    let mut requests = Vec::new();
                    if let Some(request) = request {
                        requests.push(request);
                    }
                    if let Some(value) = self.slider_value(&tree, &slider_key) {
                        requests.extend(self.call_node_handler(
                            &tree,
                            &slider_key,
                            "change",
                            &[serde_json::json!(value)],
                        )?);
                    }
                    self.dirty = true;
                    if !requests.is_empty() {
                        return Ok(requests);
                    }
                }

                // Update hover state for CSS :hover and the tooltip system.
                self.hovered_pos = (x, y);
                let new_path = find_node_path_at(&tree, x, y).unwrap_or_default();
                let new_key = new_path.last().cloned();
                tracing::trace!(
                    "[hover] pointer=({x:.1},{y:.1}) path={:?} hit={:?} prev={:?}",
                    new_path,
                    new_key,
                    self.hovered_key
                );
                if new_key != self.hovered_key || new_path != self.hovered_path {
                    self.hovered_key = new_key.clone();
                    self.hovered_path = new_path;
                    // Only start the tooltip timer when hovering a node with tooltip content.
                    self.hover_start = new_key
                        .as_ref()
                        .and_then(|k| find_node_by_key(&tree, k))
                        .and_then(|n| node_tooltip_text(n))
                        .map(|_| std::time::Instant::now());
                    self.dirty = true;
                }
            }
            ComponentInput::Scroll { x, y, dx, dy } => {
                if let Some(scroll_key) = find_scrollable_at(&tree, x, y) {
                    if let Some(node) = find_node_by_key(&tree, &scroll_key) {
                        let (max_x, max_y) = scroll_limits(node);
                        let current = self.scroll_offsets.entry(scroll_key).or_default();
                        let next_x = (current.x - dx * 28.0).clamp(0.0, max_x);
                        let next_y = (current.y - dy * 28.0).clamp(0.0, max_y);
                        if (next_x - current.x).abs() > f32::EPSILON
                            || (next_y - current.y).abs() > f32::EPSILON
                        {
                            current.x = next_x;
                            current.y = next_y;
                            self.dirty = true;
                        }
                    }
                }
            }
            ComponentInput::Char { ch } => {
                if let Some(focused_key) = self.focused_key.clone() {
                    let accepts_char = find_node_by_key(&tree, &focused_key)
                        .is_some_and(|node| input_accepts_char(node, ch));
                    if is_input_key(&tree, &focused_key) && accepts_char {
                        let value = self.input_values.entry(focused_key.clone()).or_default();
                        value.push(ch);
                        let current = value.clone();
                        self.dirty = true;
                        return self.call_node_handler(
                            &tree,
                            &focused_key,
                            "change",
                            &[serde_json::json!(current)],
                        );
                    }
                }
            }
            ComponentInput::KeyPressed { key } => {
                if let Some(focused_key) = self.focused_key.clone() {
                    if is_input_key(&tree, &focused_key) {
                        let value = self.input_values.entry(focused_key.clone()).or_default();
                        match key.as_str() {
                            "Backspace" => {
                                value.pop();
                                let current = value.clone();
                                self.dirty = true;
                                return self.call_node_handler(
                                    &tree,
                                    &focused_key,
                                    "change",
                                    &[serde_json::json!(current)],
                                );
                            }
                            _ => {}
                        }
                    } else if matches!(key.as_str(), "Enter" | " " | "Space")
                        && find_node_by_key(&tree, &focused_key)
                            .is_some_and(|node| matches!(node.tag.as_str(), "switch" | "checkbox"))
                    {
                        let value = self.toggle_checked_value(&tree, &focused_key);
                        self.dirty = true;
                        return self.call_node_handler(
                            &tree,
                            &focused_key,
                            "change",
                            &[serde_json::json!(value)],
                        );
                    }
                }
            }
            ComponentInput::KeyReleased { .. } => {}
        }

        Ok(Vec::new())
    }

    fn last_widget_tree(&self) -> Option<&WidgetNode> {
        self.last_tree.as_ref()
    }

    fn apply_position(&mut self, margin_top: i32, margin_left: i32) {
        self.surface_layout.edge = Edge::Left;
        self.surface_layout.margin_top = margin_top;
        self.surface_layout.margin_left = margin_left;
        self.dirty = true;
    }
}

impl FrontendSurfaceComponent {
    fn clear_runtime_dirty_states(&self) {
        for runtime in self.runtimes.lock().unwrap().values_mut() {
            runtime.script_ctx.state_mut().clear_dirty();
        }
    }

    fn publish_element_metrics(&self, tree: &WidgetNode) {
        let mut elements = serde_json::Map::new();
        let mut refs = serde_json::Map::new();
        collect_element_metrics(tree, 0.0, 0.0, &mut elements, &mut refs);

        if let Some(root_runtime) = self.runtimes.lock().unwrap().get_mut(self.id()) {
            let state = root_runtime.script_ctx.state_mut();
            state.set_host_value("elements", serde_json::Value::Object(elements));
            state.set_host_value("refs", serde_json::Value::Object(refs));
        }
    }
}

fn collect_visual_styles(root: &WidgetNode) -> HashMap<String, AnimatedVisualStyle> {
    let mut styles = HashMap::new();
    collect_visual_styles_into(root, &mut styles);
    styles
}

fn tracked_service_fields_changed(
    previous: Option<&serde_json::Value>,
    next: &serde_json::Value,
    tracked_fields: &HashSet<String>,
) -> bool {
    tracked_fields.iter().any(|field| {
        let previous_value = previous.and_then(|value| value.get(field));
        let next_value = next.get(field);
        previous_value != next_value
    })
}

fn collect_visual_styles_into(
    node: &WidgetNode,
    styles: &mut HashMap<String, AnimatedVisualStyle>,
) {
    if let Some(key) = node.attributes.get("_mesh_key") {
        styles.insert(key.clone(), AnimatedVisualStyle::from_node(node));
    }

    for child in &node.children {
        collect_visual_styles_into(child, styles);
    }
}

fn apply_easing(easing: TransitionEasing, t: f32) -> f32 {
    match easing {
        TransitionEasing::Linear => t,
        TransitionEasing::Ease => ease_in_out_cubic(t),
        TransitionEasing::EaseIn => ease_in_cubic(t),
        TransitionEasing::EaseOut => ease_out_cubic(t),
        TransitionEasing::EaseInOut => ease_in_out_cubic(t),
    }
}

fn ease_in_cubic(t: f32) -> f32 {
    t * t * t
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn lerp_corners(from: Corners, to: Corners, progress: f32) -> Corners {
    Corners {
        top_left: lerp_f32(from.top_left, to.top_left, progress),
        top_right: lerp_f32(from.top_right, to.top_right, progress),
        bottom_right: lerp_f32(from.bottom_right, to.bottom_right, progress),
        bottom_left: lerp_f32(from.bottom_left, to.bottom_left, progress),
    }
}

fn lerp_color(from: Color, to: Color, progress: f32) -> Color {
    Color {
        r: lerp_f32(from.r as f32, to.r as f32, progress).round() as u8,
        g: lerp_f32(from.g as f32, to.g as f32, progress).round() as u8,
        b: lerp_f32(from.b as f32, to.b as f32, progress).round() as u8,
        a: lerp_f32(from.a as f32, to.a as f32, progress).round() as u8,
    }
}

fn lerp_f32(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

fn input_accepts_char(node: &WidgetNode, ch: char) -> bool {
    if ch.is_control() {
        return false;
    }

    match node.attributes.get("type").map(|value| value.as_str()) {
        Some("number") => ch.is_ascii_digit() || matches!(ch, '.' | '-'),
        _ => true,
    }
}

fn collect_element_metrics(
    node: &WidgetNode,
    offset_x: f32,
    offset_y: f32,
    elements: &mut serde_json::Map<String, serde_json::Value>,
    refs: &mut serde_json::Map<String, serde_json::Value>,
) {
    let metrics = element_snapshot_json(node, offset_x, offset_y);
    let scroll_x = metrics
        .get("scroll_x")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0) as f32;
    let scroll_y = metrics
        .get("scroll_y")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0) as f32;

    if let Some(key) = node.attributes.get("_mesh_key") {
        elements.insert(key.clone(), metrics.clone());
    }
    if let Some(id) = node.attributes.get("id") {
        refs.insert(id.clone(), metrics.clone());
    }
    if let Some(reference) = node.attributes.get("ref") {
        refs.insert(reference.clone(), metrics);
    }

    let child_offset_x = offset_x - scroll_x;
    let child_offset_y = offset_y - scroll_y;
    for child in &node.children {
        collect_element_metrics(child, child_offset_x, child_offset_y, elements, refs);
    }
}

pub(super) fn annotate_runtime_tree(
    node: &mut WidgetNode,
    key: String,
    focused_key: &Option<String>,
    hovered_path: &[String],
    active_key: &Option<String>,
    input_values: &HashMap<String, String>,
    slider_values: &HashMap<String, f32>,
    checked_values: &HashMap<String, bool>,
    scroll_offsets: &HashMap<String, ScrollOffsetState>,
) {
    node.attributes.insert("_mesh_key".into(), key.clone());

    let key_str = key.as_str();
    let checked = checked_values
        .get(&key)
        .copied()
        .or_else(|| {
            node.attributes
                .get("checked")
                .map(|value| matches!(value.as_str(), "true" | "1" | "checked"))
        })
        .unwrap_or(false);

    node.state = ElementState {
        focused: focused_key.as_deref() == Some(key_str),
        hovered: hovered_path
            .iter()
            .any(|hovered_key| hovered_key == key_str),
        active: active_key.as_deref() == Some(key_str),
        disabled: false,
        checked,
    };
    if node.state.hovered {
        tracing::trace!(
            "[hover] annotate: key={key} tag={} set hovered=true",
            node.tag
        );
    }

    if node.state.focused {
        node.attributes
            .insert("_mesh_focused".into(), "true".into());
    }

    match node.tag.as_str() {
        "input" => {
            let value = input_values
                .get(&key)
                .cloned()
                .or_else(|| node.attributes.get("value").cloned())
                .unwrap_or_default();
            node.attributes.insert("value".into(), value);
        }
        "slider" => {
            let value = slider_values
                .get(&key)
                .copied()
                .or_else(|| {
                    node.attributes
                        .get("value")
                        .and_then(|value: &String| value.parse::<f32>().ok())
                })
                .unwrap_or(50.0);
            node.attributes
                .insert("value".into(), format!("{value:.2}"));
        }
        "switch" | "checkbox" => {
            node.attributes.insert(
                "checked".into(),
                if checked { "true" } else { "false" }.into(),
            );
        }
        _ => {}
    }

    let offset = scroll_offsets.get(&key).copied().unwrap_or_default();
    node.attributes
        .insert("_mesh_scroll_x".into(), format!("{:.2}", offset.x));
    node.attributes
        .insert("_mesh_scroll_y".into(), format!("{:.2}", offset.y));

    for (index, child) in node.children.iter_mut().enumerate() {
        annotate_runtime_tree(
            child,
            format!("{key}/{index}"),
            focused_key,
            hovered_path,
            active_key,
            input_values,
            slider_values,
            checked_values,
            scroll_offsets,
        );
    }
}

pub(super) fn grant_capabilities_from_manifest(
    manifest: &mesh_core_plugin::Manifest,
) -> CapabilitySet {
    let mut granted = CapabilitySet::new();

    for capability in &manifest.capabilities.required {
        granted.grant(Capability::new(capability.clone()));
    }

    for capability in &manifest.capabilities.optional {
        granted.grant(Capability::new(capability.clone()));
    }

    granted
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_capability::Capability;
    use mesh_core_component::parse_component;
    use mesh_core_plugin::manifest::{
        CapabilitiesSection, CompatibilitySection, DependenciesSection, EntrypointsSection,
        ExportsSection, Manifest, PackageSection, PluginType,
    };
    use mesh_core_scripting::ScriptContext;
    use mesh_core_service::{
        ContractCapabilities, InterfaceArgument, InterfaceCatalog, InterfaceContract,
        InterfaceMethod, InterfaceProvider, parse_contract_version,
    };
    use std::path::PathBuf;

    #[test]
    fn service_update_marks_component_dirty_only_when_tracked_fields_change() {
        let previous = serde_json::json!({
            "percent": 65,
            "muted": false,
            "source_plugin": "@mesh/pipewire-audio"
        });
        let unchanged_tracked = serde_json::json!({
            "percent": 65,
            "muted": false,
            "source_plugin": "@mesh/alternate-audio"
        });
        let changed_tracked = serde_json::json!({
            "percent": 66,
            "muted": false,
            "source_plugin": "@mesh/alternate-audio"
        });
        let tracked_fields = HashSet::from(["percent".to_string(), "muted".to_string()]);

        assert!(!tracked_service_fields_changed(
            Some(&previous),
            &unchanged_tracked,
            &tracked_fields
        ));
        assert!(tracked_service_fields_changed(
            Some(&previous),
            &changed_tracked,
            &tracked_fields
        ));
    }

    // ---------- helpers shared by the three integration tests below ----------

    fn audio_network_catalog() -> InterfaceCatalog {
        let mut catalog = InterfaceCatalog::default();
        catalog.register_contract(InterfaceContract {
            interface: "mesh.audio".into(),
            version: parse_contract_version("1.0").unwrap(),
            file_path: PathBuf::from("<test>"),
            state_fields: Vec::new(),
            methods: vec![
                InterfaceMethod {
                    name: "set_volume".into(),
                    args: vec![
                        InterfaceArgument {
                            name: "device_id".into(),
                            arg_type: "string".into(),
                        },
                        InterfaceArgument {
                            name: "volume".into(),
                            arg_type: "float".into(),
                        },
                    ],
                    returns: None,
                },
                InterfaceMethod {
                    name: "volume_up".into(),
                    args: Vec::new(),
                    returns: None,
                },
                InterfaceMethod {
                    name: "volume_down".into(),
                    args: Vec::new(),
                    returns: None,
                },
                InterfaceMethod {
                    name: "toggle_mute".into(),
                    args: Vec::new(),
                    returns: None,
                },
            ],
            events: Vec::new(),
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        });
        catalog.register_provider(InterfaceProvider {
            interface: "mesh.audio".into(),
            version: Some("1.0".into()),
            base_plugin: Some("@mesh/audio-interface".into()),
            provider_plugin: "@mesh/pipewire-audio".into(),
            backend_name: "PipeWire".into(),
            priority: 100,
        });
        catalog.register_contract(InterfaceContract {
            interface: "mesh.network".into(),
            version: parse_contract_version("1.0").unwrap(),
            file_path: PathBuf::from("<test>"),
            state_fields: Vec::new(),
            methods: vec![InterfaceMethod {
                name: "set_wifi_enabled".into(),
                args: vec![InterfaceArgument {
                    name: "enabled".into(),
                    arg_type: "bool".into(),
                }],
                returns: None,
            }],
            events: Vec::new(),
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        });
        catalog.register_provider(InterfaceProvider {
            interface: "mesh.network".into(),
            version: Some("1.0".into()),
            base_plugin: Some("@mesh/network-interface".into()),
            provider_plugin: "@mesh/networkmanager-network".into(),
            backend_name: "NetworkManager".into(),
            priority: 100,
        });
        catalog
    }

    fn audio_network_power_catalog() -> InterfaceCatalog {
        let mut catalog = audio_network_catalog();
        catalog.register_contract(InterfaceContract {
            interface: "mesh.power".into(),
            version: parse_contract_version("1.0").unwrap(),
            file_path: PathBuf::from("<test>"),
            state_fields: Vec::new(),
            methods: Vec::new(),
            events: Vec::new(),
            types: HashMap::new(),
            capabilities: ContractCapabilities::default(),
        });
        catalog.register_provider(InterfaceProvider {
            interface: "mesh.power".into(),
            version: Some("1.0".into()),
            base_plugin: Some("@mesh/power-interface".into()),
            provider_plugin: "@mesh/upower-power".into(),
            backend_name: "UPower".into(),
            priority: 100,
        });
        catalog
    }

    fn make_audio_ctx() -> ScriptContext {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        caps.grant(Capability::new("service.audio.control"));
        let mut ctx = ScriptContext::new("@mesh/panel", caps).unwrap();
        ctx.set_interface_catalog(audio_network_catalog());
        ctx
    }

    fn make_network_ctx() -> ScriptContext {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.network.read"));
        caps.grant(Capability::new("service.network.control"));
        let mut ctx = ScriptContext::new("@mesh/quick-settings", caps).unwrap();
        ctx.set_interface_catalog(audio_network_catalog());
        ctx
    }

    fn make_panel_ctx() -> ScriptContext {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        caps.grant(Capability::new("service.network.read"));
        caps.grant(Capability::new("service.power.read"));
        let mut ctx = ScriptContext::new("@mesh/panel", caps).unwrap();
        ctx.set_interface_catalog(audio_network_power_catalog());
        ctx
    }

    fn shipped_component_script(source: &str) -> String {
        parse_component(source)
            .unwrap()
            .script
            .expect("shipped component should contain a script block")
            .source
    }

    fn assert_no_legacy_service_callbacks(source_name: &str, source: &str) {
        for forbidden in ["mesh.service.bind", "mesh.service.on", ".on_change("] {
            assert!(
                !source.contains(forbidden),
                "{source_name} must not teach or use legacy service callback API {forbidden}"
            );
        }
    }

    fn minimal_test_manifest(id: &str) -> Manifest {
        Manifest {
            package: PackageSection {
                id: id.to_string(),
                name: None,
                version: "0.1.0".into(),
                plugin_type: PluginType::Surface,
                api_version: "0.1".into(),
                license: None,
                description: None,
                authors: Vec::new(),
                repository: None,
            },
            compatibility: CompatibilitySection::default(),
            dependencies: DependenciesSection::default(),
            capabilities: CapabilitiesSection::default(),
            entrypoints: EntrypointsSection {
                main: Some("src/main.mesh".into()),
                settings_ui: None,
            },
            accessibility: None,
            settings: None,
            i18n: None,
            theme: None,
            service: None,
            provides: Vec::new(),
            interface: None,
            extensions: Vec::new(),
            exports: ExportsSection::default(),
            provides_slots: HashMap::new(),
            slot_contributions: HashMap::new(),
            assets: None,
            translations: HashMap::new(),
            surface_layout: None,
        }
    }

    fn test_frontend_component(source: &str) -> FrontendSurfaceComponent {
        test_frontend_component_with_catalog(source, InterfaceCatalog::default(), &[])
    }

    fn test_frontend_component_with_catalog(
        source: &str,
        interface_catalog: InterfaceCatalog,
        required_capabilities: &[&str],
    ) -> FrontendSurfaceComponent {
        let manifest = minimal_test_manifest("@test/reactive-surface");
        let mut manifest = manifest;
        manifest.capabilities.required = required_capabilities
            .iter()
            .map(|capability| (*capability).to_string())
            .collect();
        let compiled = CompiledFrontendPlugin {
            manifest,
            source_path: PathBuf::from("src/main.mesh"),
            component: parse_component(source).unwrap(),
            local_components: HashMap::new(),
            plugin_component_imports: HashMap::new(),
        };
        let catalog = FrontendCatalog {
            plugins: HashMap::new(),
            slot_contributions: HashMap::new(),
        };
        let mut component =
            FrontendSurfaceComponent::new(compiled, PathBuf::from("."), catalog, interface_catalog);
        component
            .mount(ComponentContext {
                component_id: "@test/reactive-surface".into(),
                surface_id: "@test/reactive-surface".into(),
                diagnostics: Diagnostics::new("@test/reactive-surface"),
            })
            .unwrap();
        component.visible = true;
        component
    }

    fn runtime_value(
        component: &FrontendSurfaceComponent,
        name: &str,
    ) -> Option<serde_json::Value> {
        component
            .runtimes
            .lock()
            .unwrap()
            .get(component.id())
            .and_then(|runtime| runtime.script_ctx.state().get(name))
    }

    fn runtime_number(component: &FrontendSurfaceComponent, name: &str) -> f64 {
        runtime_value(component, name)
            .and_then(|value| value.as_f64())
            .unwrap_or_else(|| panic!("expected numeric runtime value for {name}"))
    }

    fn event_node(
        tag: &str,
        key: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        handlers: &[(&str, &str)],
    ) -> WidgetNode {
        let mut node = WidgetNode::new(tag);
        node.attributes.insert("_mesh_key".into(), key.into());
        node.layout.x = x;
        node.layout.y = y;
        node.layout.width = width;
        node.layout.height = height;
        node.event_handlers = handlers
            .iter()
            .map(|(event, handler)| ((*event).into(), (*handler).into()))
            .collect();
        node
    }

    fn root_with(children: Vec<WidgetNode>) -> WidgetNode {
        let mut root = WidgetNode::new("box");
        root.attributes.insert("_mesh_key".into(), "root".into());
        root.layout.width = 240.0;
        root.layout.height = 160.0;
        root.children = children;
        root
    }

    fn first_node_by_tag<'a>(node: &'a WidgetNode, tag: &str) -> Option<&'a WidgetNode> {
        if node.tag == tag {
            return Some(node);
        }
        node.children
            .iter()
            .find_map(|child| first_node_by_tag(child, tag))
    }

    #[test]
    fn slider_change_handler_receives_number_on_pointer_move() {
        let mut component = test_frontend_component(
            r#"
<template><box /></template>
<script lang="luau">
slider_seen = -1
function onSliderChange(value)
    slider_seen = value
end
</script>
"#,
        );
        let mut slider = event_node(
            "slider",
            "root/0",
            0.0,
            0.0,
            100.0,
            20.0,
            &[("change", "onSliderChange")],
        );
        slider.attributes.insert("min".into(), "0".into());
        slider.attributes.insert("max".into(), "1".into());
        slider.attributes.insert("value".into(), "0".into());
        component.last_tree = Some(root_with(vec![slider]));

        let theme = default_theme();
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerButton {
                    x: 0.0,
                    y: 10.0,
                    pressed: true,
                },
            )
            .unwrap();
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerMove { x: 75.0, y: 10.0 },
            )
            .unwrap();

        assert!((runtime_number(&component, "slider_seen") - 0.75).abs() < 0.001);
    }

    #[test]
    fn navigation_volume_slider_proves_event_state_render_flow() {
        let mut component = test_frontend_component_with_catalog(
            r#"
<template>
  <slider min="0" max="1" value="{slider_value}" onchange={onVolumeChange} />
</template>
<script lang="luau">
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

audio_percent = 0
slider_value = 0.0
icon_name = "audio-volume-muted"
audio_tooltip = "Volume unavailable"
handler_value_type = "unset"

local function clamp_volume(value)
    local numeric = tonumber(value) or 0
    if numeric < 0 then return 0.0 end
    if numeric > 1 then return 1.0 end
    return numeric
end

local function update_audio_copy(percent, muted)
    audio_percent = percent
    slider_value = clamp_volume(percent / 100)
    if muted or percent == 0 then
        icon_name = "audio-volume-muted"
    elseif percent < 34 then
        icon_name = "audio-volume-low"
    elseif percent < 67 then
        icon_name = "audio-volume-medium"
    else
        icon_name = "audio-volume-high"
    end
    if muted then
        audio_tooltip = string.format("Volume muted at %d%%", percent)
    else
        audio_tooltip = string.format("Volume %d%%", percent)
    end
end

function onRender()
    if not audio_ok or not audio then
        icon_name = "audio-volume-muted"
        audio_tooltip = "Audio service unavailable"
        audio_percent = 0
        slider_value = 0.0
        return
    end
    local percent = math.floor(tonumber(audio.percent) or 0)
    local muted = audio.muted or false
    update_audio_copy(percent, muted)
end

function onVolumeChange(value)
    handler_value_type = type(value)
    local normalized = clamp_volume(value)
    local percent = math.floor((normalized * 100) + 0.5)
    slider_value = normalized
    update_audio_copy(percent, false)
    if audio_ok and audio then
        mesh.events.publish("mesh.audio.set-volume", { percent = percent })
    end
end
</script>
"#,
            audio_network_catalog(),
            &["service.audio.read"],
        );
        {
            let mut runtimes = component.runtimes.lock().unwrap();
            let runtime = runtimes.get_mut(component.id()).unwrap();
            runtime.script_ctx.apply_service_payload(
                "audio",
                &serde_json::json!({ "percent": 20, "muted": false }),
            );
            runtime.script_ctx.call_handler("onRender", &[]).unwrap();
        }
        component.render_hooks_pending = false;

        let mut slider = event_node(
            "slider",
            "root/0",
            0.0,
            0.0,
            100.0,
            20.0,
            &[("change", "onVolumeChange")],
        );
        slider.attributes.insert("min".into(), "0".into());
        slider.attributes.insert("max".into(), "1".into());
        slider.attributes.insert("value".into(), "0.2".into());
        component.last_tree = Some(root_with(vec![slider]));
        component.clear_runtime_dirty_states();
        component.dirty = false;

        let theme = default_theme();
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerButton {
                    x: 80.0,
                    y: 10.0,
                    pressed: true,
                },
            )
            .unwrap();
        let requests = component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerMove { x: 50.0, y: 10.0 },
            )
            .unwrap();

        assert_eq!(
            runtime_value(&component, "handler_value_type"),
            Some(serde_json::json!("number"))
        );
        assert_eq!(
            runtime_value(&component, "audio_percent"),
            Some(serde_json::json!(50))
        );
        assert!((runtime_number(&component, "slider_value") - 0.5).abs() < 0.001);
        assert_eq!(
            runtime_value(&component, "icon_name"),
            Some(serde_json::json!("audio-volume-medium"))
        );
        assert_eq!(
            runtime_value(&component, "audio_tooltip"),
            Some(serde_json::json!("Volume 50%"))
        );
        assert!(
            component.wants_render(),
            "changed reactive globals should mark dirty"
        );

        match requests.as_slice() {
            [
                CoreRequest::ServiceCommand {
                    interface,
                    command,
                    payload,
                    ..
                },
            ] => {
                assert_eq!(interface, "mesh.audio");
                assert_eq!(command, "set-volume");
                assert_eq!(payload, &serde_json::json!({ "percent": 50 }));
            }
            other => panic!("expected one mesh.audio.set-volume request, got {other:?}"),
        }

        let mut buffer = PixelBuffer::new(240, 40);
        component.paint(&theme, 240, 40, &mut buffer).unwrap();
        let tree = component
            .last_tree
            .as_ref()
            .expect("paint should cache tree");
        let slider = first_node_by_tag(tree, "slider").expect("painted tree should contain slider");
        let rendered_value = slider
            .attributes
            .get("value")
            .and_then(|value| value.parse::<f64>().ok())
            .expect("painted slider value should be numeric");
        assert!(
            (rendered_value - 0.5).abs() < 0.001,
            "next paint should rebuild from the updated reactive slider state"
        );
        assert!(
            !component
                .runtimes
                .lock()
                .unwrap()
                .get(component.id())
                .unwrap()
                .script_ctx
                .state()
                .is_dirty(),
            "paint should consume runtime dirty state after rebuilding"
        );
    }

    #[test]
    fn text_input_change_handler_receives_current_string() {
        let mut component = test_frontend_component(
            r#"
<template><box /></template>
<script lang="luau">
text_seen = ""
function onTextChange(value)
    text_seen = value
end
</script>
"#,
        );
        component.last_tree = Some(root_with(vec![event_node(
            "input",
            "root/0",
            0.0,
            0.0,
            100.0,
            24.0,
            &[("change", "onTextChange")],
        )]));

        let theme = default_theme();
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerButton {
                    x: 4.0,
                    y: 4.0,
                    pressed: true,
                },
            )
            .unwrap();
        component
            .handle_input(&theme, 240, 160, ComponentInput::Char { ch: 'A' })
            .unwrap();

        assert_eq!(
            runtime_value(&component, "text_seen"),
            Some(serde_json::json!("A"))
        );
    }

    #[test]
    fn switch_change_handler_receives_boolean_on_click() {
        let mut component = test_frontend_component(
            r#"
<template><box /></template>
<script lang="luau">
switch_seen = false
function onSwitchChange(value)
    switch_seen = value
end
</script>
"#,
        );
        component.last_tree = Some(root_with(vec![event_node(
            "switch",
            "root/0",
            0.0,
            0.0,
            48.0,
            24.0,
            &[("change", "onSwitchChange")],
        )]));

        let theme = default_theme();
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerButton {
                    x: 8.0,
                    y: 8.0,
                    pressed: true,
                },
            )
            .unwrap();

        assert_eq!(
            runtime_value(&component, "switch_seen"),
            Some(serde_json::json!(true))
        );
    }

    #[test]
    fn slider_release_handler_fires_once_with_current_number() {
        let mut component = test_frontend_component(
            r#"
<template><box /></template>
<script lang="luau">
release_count = 0
released_value = -1
function onSliderRelease(value)
    release_count = release_count + 1
    released_value = value
end
</script>
"#,
        );
        let mut slider = event_node(
            "slider",
            "root/0",
            0.0,
            0.0,
            100.0,
            20.0,
            &[("release", "onSliderRelease")],
        );
        slider.attributes.insert("min".into(), "0".into());
        slider.attributes.insert("max".into(), "1".into());
        slider.attributes.insert("value".into(), "0".into());
        component.last_tree = Some(root_with(vec![slider]));

        let theme = default_theme();
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerButton {
                    x: 10.0,
                    y: 10.0,
                    pressed: true,
                },
            )
            .unwrap();
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerMove { x: 60.0, y: 10.0 },
            )
            .unwrap();
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerButton {
                    x: 60.0,
                    y: 10.0,
                    pressed: false,
                },
            )
            .unwrap();

        assert_eq!(runtime_number(&component, "release_count"), 1.0);
        assert!((runtime_number(&component, "released_value") - 0.6).abs() < 0.001);
    }

    #[test]
    fn click_handler_keeps_current_target_position_payload() {
        let mut component = test_frontend_component(
            r#"
<template><box /></template>
<script lang="luau">
click_left = -1
click_top = -1
function onButtonClick(event)
    click_left = event.current_target.position.margin_left
    click_top = event.current_target.position.margin_top
end
</script>
"#,
        );
        component.last_tree = Some(root_with(vec![event_node(
            "button",
            "root/0",
            32.0,
            4.0,
            80.0,
            24.0,
            &[("click", "onButtonClick")],
        )]));

        let theme = default_theme();
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerButton {
                    x: 40.0,
                    y: 10.0,
                    pressed: true,
                },
            )
            .unwrap();
        component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerButton {
                    x: 40.0,
                    y: 10.0,
                    pressed: false,
                },
            )
            .unwrap();

        assert_eq!(runtime_number(&component, "click_left"), 32.0);
        assert_eq!(runtime_number(&component, "click_top"), 28.0);
    }

    #[test]
    fn focus_handler_fires_when_node_becomes_focused() {
        let mut component = test_frontend_component(
            r#"
<template><box /></template>
<script lang="luau">
focus_count = 0
function onInputFocus()
    focus_count = focus_count + 1
end
</script>
"#,
        );
        component.last_tree = Some(root_with(vec![event_node(
            "input",
            "root/0",
            0.0,
            0.0,
            100.0,
            24.0,
            &[("focus", "onInputFocus")],
        )]));

        let theme = default_theme();
        for _ in 0..2 {
            component
                .handle_input(
                    &theme,
                    240,
                    160,
                    ComponentInput::PointerButton {
                        x: 8.0,
                        y: 8.0,
                        pressed: true,
                    },
                )
                .unwrap();
            component
                .handle_input(
                    &theme,
                    240,
                    160,
                    ComponentInput::PointerButton {
                        x: 8.0,
                        y: 8.0,
                        pressed: false,
                    },
                )
                .unwrap();
        }

        assert_eq!(runtime_number(&component, "focus_count"), 1.0);
    }

    #[test]
    fn failing_handler_is_reported_once_and_does_not_clear_render_state() {
        let mut component = test_frontend_component(
            r#"
<template><box /></template>
<script lang="luau">
function onExplode()
    error("boom")
end
</script>
"#,
        );
        component.last_tree = Some(root_with(vec![event_node(
            "button",
            "root/0",
            0.0,
            0.0,
            80.0,
            24.0,
            &[("click", "onExplode")],
        )]));
        component.dirty = false;

        let first = component.call_namespaced_handler("onExplode", &[]);
        let second = component.call_namespaced_handler("onExplode", &[]);

        assert!(first.unwrap().is_empty());
        assert!(second.unwrap().is_empty());
        assert!(
            component.last_tree.is_some(),
            "last successfully rendered tree should remain available"
        );
        let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
        assert_eq!(diagnostics.error_count(), 1);
        assert!(!component.wants_render());
    }

    #[test]
    fn service_update_runs_on_render_before_rebuilding_tree() {
        let mut component = test_frontend_component_with_catalog(
            r#"
<template>
  <box title="{audio_tooltip}" />
</template>
<script lang="luau">
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

audio_tooltip = "Volume unavailable"

function onRender()
    if not audio_ok or not audio then
        audio_tooltip = "Audio service unavailable"
        return
    end
    audio_tooltip = string.format("Volume %d%%", math.floor(tonumber(audio.percent) or 0))
end
</script>
"#,
            audio_network_catalog(),
            &["service.audio.read"],
        );

        component
            .handle_service_event(&ServiceEvent::Updated {
                service: "mesh.audio".into(),
                source_plugin: "@mesh/pipewire-audio".into(),
                payload: serde_json::json!({ "percent": 64, "muted": false }),
            })
            .unwrap();

        let theme = default_theme();
        let mut buffer = PixelBuffer::new(240, 40);
        component.paint(&theme, 240, 40, &mut buffer).unwrap();

        assert_eq!(
            runtime_value(&component, "audio_tooltip"),
            Some(serde_json::json!("Volume 64%"))
        );
        let tree = component.last_tree.as_ref().unwrap();
        fn first_title(node: &WidgetNode) -> Option<&str> {
            node.attributes
                .get("title")
                .map(String::as_str)
                .or_else(|| node.children.iter().find_map(first_title))
        }
        assert_eq!(first_title(tree), Some("Volume 64%"));
    }

    #[test]
    fn raw_service_state_update_schedules_repaint_without_proxy_tracking() {
        let mut component = test_frontend_component(
            r#"
<template>
  <box title="{last_service_update.name}" />
</template>
<script lang="luau">
</script>
"#,
        );
        let theme = default_theme();
        let mut buffer = PixelBuffer::new(240, 40);
        component.paint(&theme, 240, 40, &mut buffer).unwrap();
        component.dirty = false;

        component
            .handle_service_event(&ServiceEvent::Updated {
                service: "mesh.audio".into(),
                source_plugin: "@mesh/pipewire-audio".into(),
                payload: serde_json::json!({ "percent": 64, "muted": false }),
            })
            .unwrap();

        assert!(
            component.wants_render(),
            "raw ScriptState changes should schedule repaint even without proxy tracking"
        );
        component.paint(&theme, 240, 40, &mut buffer).unwrap();
        let tree = component.last_tree.as_ref().unwrap();
        fn first_title(node: &WidgetNode) -> Option<&str> {
            node.attributes
                .get("title")
                .map(String::as_str)
                .or_else(|| node.children.iter().find_map(first_title))
        }
        assert_eq!(first_title(tree), Some("audio"));
    }

    #[test]
    fn pcall_service_lookup_diagnostic_reaches_component_diagnostics() {
        let mut component = test_frontend_component(
            r#"
<template><box /></template>
<script lang="luau">
function onRender()
    pcall(require, "@mesh/missing@>=1.0")
end
</script>
"#,
        );

        let theme = default_theme();
        let mut buffer = PixelBuffer::new(240, 40);
        component.paint(&theme, 240, 40, &mut buffer).unwrap();

        let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
        assert_eq!(diagnostics.error_count(), 1);
    }

    #[test]
    fn navigation_volume_slider_handler_error_records_diagnostic_and_keeps_last_tree() {
        let mut component = test_frontend_component(
            r#"
<template><box /></template>
<script lang="luau">
function onVolumeChange(value)
    error("slider handler error")
end
</script>
"#,
        );
        component.last_tree = Some(root_with(vec![event_node(
            "slider",
            "root/0",
            0.0,
            0.0,
            100.0,
            20.0,
            &[("change", "onVolumeChange")],
        )]));
        component.dirty = false;

        let theme = default_theme();
        let requests = component
            .handle_input(
                &theme,
                240,
                160,
                ComponentInput::PointerButton {
                    x: 50.0,
                    y: 10.0,
                    pressed: true,
                },
            )
            .unwrap();

        assert!(requests.is_empty());
        assert!(
            component.last_tree.is_some(),
            "last successfully rendered tree should remain available after slider handler error"
        );
        let diagnostics = component.diagnostics.as_ref().expect("diagnostics handle");
        assert_eq!(diagnostics.error_count(), 1);
    }

    #[test]
    fn handler_without_state_change_does_not_force_rebuild() {
        let mut component = test_frontend_component(
            r#"
<template>
  <button onclick={onClick}>{label}</button>
</template>

<script lang="luau">
label = "Ready"

function onClick()
    label = "Ready"
end
</script>
"#,
        );
        component.clear_runtime_dirty_states();
        component.dirty = false;

        component.call_namespaced_handler("onClick", &[]).unwrap();

        assert!(!component.wants_render());
    }

    #[test]
    fn handler_state_change_rebuilds_next_paint() {
        let mut component = test_frontend_component(
            r#"
<template>
  <button onclick={onClick}>{label}</button>
</template>

<script lang="luau">
label = "Ready"

function onClick()
    label = "Clicked"
end
</script>
"#,
        );
        component.clear_runtime_dirty_states();
        component.dirty = false;

        component.call_namespaced_handler("onClick", &[]).unwrap();
        assert!(component.wants_render());

        let theme = default_theme();
        let mut buffer = PixelBuffer::new(96, 32);
        component.paint(&theme, 96, 32, &mut buffer).unwrap();
        component.dirty = false;

        assert!(
            !component
                .runtimes
                .lock()
                .unwrap()
                .get(component.id())
                .unwrap()
                .script_ctx
                .state()
                .is_dirty()
        );
        assert!(!component.wants_render());
    }

    // ---------- integration test 1: proxy field reads reach render state ----

    /// Proves that a bundled-style frontend (panel or quick-settings) reading
    /// service state via direct proxy field access — the same pattern used in
    /// the migrated bundled surfaces — picks up the correct value after a
    /// `ServiceEvent::Updated`-equivalent payload is applied, without any
    /// callback registration.
    #[test]
    fn frontend_proxy_update_reaches_panel_or_quick_settings_render_state() {
        let mut ctx = make_audio_ctx();
        ctx.load_script(
            r#"
-- Panel-style: read audio.percent and audio.muted directly on rerender.
volumeIcon = "audio-volume-muted"
volumeLevel = 0

function onRender()
    local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
    if not audio_ok or not audio then return end
    local pct = audio.percent or 0
    local muted = audio.muted or false
    volumeLevel = pct
    if muted or pct == 0 then
        volumeIcon = "audio-volume-muted"
    elseif pct < 34 then
        volumeIcon = "audio-volume-low"
    elseif pct < 67 then
        volumeIcon = "audio-volume-medium"
    else
        volumeIcon = "audio-volume-high"
    end
end
"#,
        )
        .unwrap();

        // Simulate a ServiceEvent::Updated payload arriving (as apply_service_payload does).
        ctx.apply_service_payload(
            "audio",
            &serde_json::json!({ "percent": 75, "muted": false }),
        );

        // The runtime calls the script's render handler on each rerender.
        ctx.call_handler("onRender", &[]).unwrap();

        // Verify that the template-visible reactive globals reflect the emitted payload,
        // proving rerender-visible service state without any callback registration.
        assert_eq!(
            ctx.state.get("volumeIcon"),
            Some(serde_json::json!("audio-volume-high")),
            "volumeIcon should be high for 75% unmuted"
        );
        assert_eq!(
            ctx.state.get("volumeLevel"),
            Some(serde_json::json!(75)),
            "volumeLevel should equal the emitted percent"
        );
        // Confirm the proxy read was tracked (needed for shell invalidation).
        let tracked = ctx.tracked_fields_for_service("audio");
        assert!(
            tracked.contains("percent"),
            "audio.percent should be in tracked fields"
        );
        assert!(
            tracked.contains("muted"),
            "audio.muted should be in tracked fields"
        );
    }

    // ---------- integration test 2: proxy command becomes ServiceCommand ----

    /// Proves that a bundled control handler (e.g. quick-settings onToggleWiFi)
    /// calling a named proxy command method publishes a `CoreRequest::ServiceCommand`
    /// through the `script_events_to_requests` routing layer.
    #[test]
    fn frontend_proxy_command_from_bundled_handler_becomes_service_command_request() {
        let mut ctx = make_network_ctx();
        ctx.load_script(
            r#"
-- Quick-settings style: read wifi_enabled from proxy, then send the command.
wifi_enabled = false

function onToggleWiFi()
    local network_ok, network = pcall(require, "@mesh/network@>=1.0")
    if network_ok and network then
        local enabled = network.wifi_enabled or false
        network.set_wifi_enabled(not enabled)
    end
end
"#,
        )
        .unwrap();

        // Seed proxy state so wifi_enabled read returns false.
        ctx.apply_service_payload("network", &serde_json::json!({ "wifi_enabled": false }));

        ctx.call_handler("onToggleWiFi", &[]).unwrap();
        let events = ctx.drain_published_events();

        // Route published events through the same path the shell uses.
        let requests = super::super::service::script_events_to_requests(events);

        assert!(
            !requests.is_empty(),
            "onToggleWiFi should publish at least one request"
        );
        match &requests[0] {
            CoreRequest::ServiceCommand {
                interface,
                command,
                payload,
                ..
            } => {
                assert_eq!(
                    interface, "mesh.network",
                    "interface should be mesh.network"
                );
                assert_eq!(
                    command, "set_wifi_enabled",
                    "command should be set_wifi_enabled"
                );
                assert_eq!(
                    payload.get("enabled").and_then(|v| v.as_bool()),
                    Some(true),
                    "enabled should be true (toggled from false)"
                );
            }
            other => panic!("expected ServiceCommand for network.set_wifi_enabled, got {other:?}"),
        }
    }

    // ---------- integration test 3: missing service keeps fallback copy -----

    /// Proves that when `pcall(require, "@mesh/audio@>=1.0")` fails (e.g. the
    /// interface contract is not registered in the catalog), the script still
    /// produces user-visible explanatory text rather than a blank or nil surface.
    #[test]
    fn frontend_missing_service_keeps_visible_fallback_copy() {
        // Intentionally use an empty catalog so the require will fail.
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        let mut ctx = ScriptContext::new("@mesh/panel", caps).unwrap();
        // No interface registered → pcall(require, ...) will catch the error.

        ctx.load_script(
            r#"
-- Panel-style degraded path: pcall catches the missing interface.
volumeLevel = "0"
volumeIcon = "audio-volume-muted"
batteryText = "N/A"

function onRender()
    local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
    if not audio_ok or not audio then
        volumeLevel = "0"
        volumeIcon = "audio-volume-muted"
        -- Explicit user-visible copy — not blank.
        batteryText = "N/A"
        return
    end
    volumeLevel = tostring(audio.percent or 0)
end
"#,
        )
        .unwrap();

        // No service payload applied — provider is absent.
        ctx.call_handler("onRender", &[]).unwrap();

        // Template-visible globals must be non-empty explanatory copy.
        assert_eq!(
            ctx.state.get("batteryText"),
            Some(serde_json::json!("N/A")),
            "batteryText should be 'N/A' when service is unavailable"
        );
        assert_eq!(
            ctx.state.get("volumeLevel"),
            Some(serde_json::json!("0")),
            "volumeLevel should be '0' when service is unavailable"
        );
        assert_eq!(
            ctx.state.get("volumeIcon"),
            Some(serde_json::json!("audio-volume-muted")),
            "volumeIcon should fall back to muted when service is unavailable"
        );
    }

    #[test]
    fn quick_settings_audio_render_state_uses_seeded_payload() {
        let mut ctx = make_audio_ctx();
        ctx.load_script(
            r#"
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

audio_label = "0%"
audio_backend = "Unavailable"
audio_tooltip = "Volume unavailable"
icon_name = "audio-volume-muted"

function onRender()
    if not audio_ok or not audio or audio.available == false then
        audio_label = "Audio unavailable"
        audio_backend = "Unavailable"
        audio_tooltip = "Audio unavailable"
        icon_name = "audio-volume-muted"
        return
    end

    local percent = math.floor((tonumber(audio.percent) or 0) + 0.5)
    local muted = audio.muted or false
    audio_label = string.format("%d%%", percent)
    audio_backend = audio.source_plugin or "Unavailable"
    if muted then
        audio_tooltip = string.format("Volume muted at %d%%", percent)
    else
        audio_tooltip = string.format("Volume %d%%", percent)
    end

    if muted or percent == 0 then
        icon_name = "audio-volume-muted"
    elseif percent < 34 then
        icon_name = "audio-volume-low"
    elseif percent < 67 then
        icon_name = "audio-volume-medium"
    else
        icon_name = "audio-volume-high"
    end
end
"#,
        )
        .unwrap();

        ctx.apply_service_payload(
            "audio",
            &serde_json::json!({
                "available": true,
                "percent": 42,
                "muted": false,
                "source_plugin": "@mesh/pipewire-audio"
            }),
        );

        ctx.call_handler("onRender", &[]).unwrap();

        assert_eq!(ctx.state.get("audio_label"), Some(serde_json::json!("42%")));
        assert_eq!(
            ctx.state.get("audio_backend"),
            Some(serde_json::json!("@mesh/pipewire-audio"))
        );
        assert_eq!(
            ctx.state.get("audio_tooltip"),
            Some(serde_json::json!("Volume 42%"))
        );
        assert_eq!(
            ctx.state.get("icon_name"),
            Some(serde_json::json!("audio-volume-medium"))
        );
    }

    #[test]
    fn quick_settings_audio_slider_publishes_set_volume_service_command() {
        let mut ctx = make_audio_ctx();
        ctx.load_script(
            r#"
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

audio_percent = 0
audio_status = ""

local function clamp_percent(value)
    local numeric = tonumber(value) or 0
    if numeric < 0 then return 0 end
    if numeric > 100 then return 100 end
    return math.floor(numeric + 0.5)
end

function onVolumeChange(value)
    local percent = clamp_percent(value)
    audio_percent = percent
    if audio_ok and audio and audio.available ~= false then
        audio.set_volume("default", percent / 100)
    else
        audio_status = "Audio controls unavailable"
    end
end
"#,
        )
        .unwrap();
        ctx.apply_service_payload("audio", &serde_json::json!({ "available": true }));

        ctx.call_handler("onVolumeChange", &[serde_json::json!(42)])
            .unwrap();
        let requests =
            super::super::service::script_events_to_requests(ctx.drain_published_events());

        match requests.as_slice() {
            [
                CoreRequest::ServiceCommand {
                    interface,
                    command,
                    payload,
                    ..
                },
            ] => {
                assert_eq!(interface, "mesh.audio");
                assert_eq!(command, "set_volume");
                assert_eq!(
                    payload,
                    &serde_json::json!({ "device_id": "default", "volume": 0.42 })
                );
            }
            other => panic!("expected one mesh.audio set_volume command, got {other:?}"),
        }
    }

    #[test]
    fn quick_settings_network_toggle_publishes_set_wifi_enabled_service_command() {
        let mut ctx = make_network_ctx();
        ctx.load_script(
            r#"
local network_ok, network = pcall(require, "@mesh/network@>=1.0")
if not network_ok then network = nil end

network_status = ""

function onToggleWiFi()
    if not network_ok or not network or network.available == false then
        network_status = "Network unavailable"
        return
    end
    if network.controls_available == false or network.permission_denied == true then
        network_status = "Network controls unavailable"
        return
    end
    network.set_wifi_enabled(not (network.wifi_enabled or false))
end
"#,
        )
        .unwrap();
        ctx.apply_service_payload(
            "network",
            &serde_json::json!({ "available": true, "wifi_enabled": false }),
        );

        ctx.call_handler("onToggleWiFi", &[]).unwrap();
        let requests =
            super::super::service::script_events_to_requests(ctx.drain_published_events());

        match requests.as_slice() {
            [
                CoreRequest::ServiceCommand {
                    interface,
                    command,
                    payload,
                    ..
                },
            ] => {
                assert_eq!(interface, "mesh.network");
                assert_eq!(command, "set_wifi_enabled");
                assert_eq!(payload, &serde_json::json!({ "enabled": true }));
            }
            other => panic!("expected one mesh.network set_wifi_enabled command, got {other:?}"),
        }
    }

    #[test]
    fn quick_settings_missing_services_keep_visible_fallback_copy() {
        let mut caps = CapabilitySet::new();
        caps.grant(Capability::new("service.audio.read"));
        caps.grant(Capability::new("service.network.read"));
        let mut ctx = ScriptContext::new("@mesh/quick-settings", caps).unwrap();

        ctx.load_script(
            r#"
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end
local network_ok, network = pcall(require, "@mesh/network@>=1.0")
if not network_ok then network = nil end

audio_status = ""
network_status = ""

function onRender()
    if not audio_ok or not audio or audio.available == false then
        audio_status = "Audio unavailable"
    end
    if not network_ok or not network or network.available == false then
        network_status = "Network unavailable"
    end
end
"#,
        )
        .unwrap();

        ctx.call_handler("onRender", &[]).unwrap();

        assert_eq!(
            ctx.state.get("audio_status"),
            Some(serde_json::json!("Audio unavailable"))
        );
        assert_eq!(
            ctx.state.get("network_status"),
            Some(serde_json::json!("Network unavailable"))
        );
    }

    #[test]
    fn quick_settings_wifi_row_empty_id_is_display_only() {
        let mut ctx = make_network_ctx();
        ctx.load_script(
            r#"
network_id = ""
connection_status = ""

function onConnectWiFi()
    if not network_id or network_id == "" then
        connection_status = "Connection details unavailable"
        return
    end

    local ok, network = pcall(require, "@mesh/network@>=1.0")
    if ok and network and network.available ~= false then
        network.connect(network_id)
    end
end
"#,
        )
        .unwrap();

        ctx.call_handler("onConnectWiFi", &[]).unwrap();
        let requests =
            super::super::service::script_events_to_requests(ctx.drain_published_events());

        assert!(
            requests.is_empty(),
            "empty network_id must not publish connect"
        );
        assert_eq!(
            ctx.state.get("connection_status"),
            Some(serde_json::json!("Connection details unavailable"))
        );
    }

    #[test]
    fn real_core_surfaces_panel_render_state_changes_with_seeded_service_payloads() {
        let mut ctx = make_panel_ctx();
        ctx.load_script(&shipped_component_script(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../packages/plugins/frontend/core/panel/src/main.mesh"
        ))))
        .unwrap();

        ctx.apply_service_payload(
            "audio",
            &serde_json::json!({ "available": true, "percent": 12, "muted": false }),
        );
        ctx.apply_service_payload("network", &serde_json::json!({ "connections": [] }));
        ctx.apply_service_payload(
            "power",
            &serde_json::json!({ "available": true, "level": 88 }),
        );
        ctx.call_handler("onRender", &[]).unwrap();

        assert_eq!(ctx.state.get("volumeLevel"), Some(serde_json::json!("12")));
        assert_eq!(
            ctx.state.get("volumeIcon"),
            Some(serde_json::json!("audio-volume-low"))
        );
        assert_eq!(
            ctx.state.get("networkStatus"),
            Some(serde_json::json!("disconnected"))
        );
        assert_eq!(ctx.state.get("batteryText"), Some(serde_json::json!("88%")));

        ctx.apply_service_payload(
            "audio",
            &serde_json::json!({ "available": true, "percent": 76, "muted": true }),
        );
        ctx.apply_service_payload(
            "network",
            &serde_json::json!({ "connections": [{ "id": "wifi-home" }] }),
        );
        ctx.apply_service_payload(
            "power",
            &serde_json::json!({ "available": true, "level": 51 }),
        );
        ctx.call_handler("onRender", &[]).unwrap();

        assert_eq!(ctx.state.get("volumeLevel"), Some(serde_json::json!("76")));
        assert_eq!(
            ctx.state.get("volumeIcon"),
            Some(serde_json::json!("audio-volume-muted"))
        );
        assert_eq!(
            ctx.state.get("networkStatus"),
            Some(serde_json::json!("connected"))
        );
        assert_eq!(ctx.state.get("batteryText"), Some(serde_json::json!("51%")));

        assert!(ctx.tracked_fields_for_service("audio").contains("percent"));
        assert!(ctx.tracked_fields_for_service("audio").contains("muted"));
        assert!(
            ctx.tracked_fields_for_service("network")
                .contains("connections")
        );
        assert!(ctx.tracked_fields_for_service("power").contains("level"));
    }

    #[test]
    fn real_core_surfaces_panel_volume_click_publishes_quick_settings_toggle() {
        let mut ctx = make_panel_ctx();
        ctx.load_script(&shipped_component_script(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../packages/plugins/frontend/core/panel/src/main.mesh"
        ))))
        .unwrap();

        ctx.call_handler("onVolumeClick", &[]).unwrap();
        let events = ctx.drain_published_events();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].channel, "shell.toggle-surface");
        assert_eq!(
            events[0].payload,
            serde_json::json!({ "surface_id": "@mesh/quick-settings" })
        );

        let requests = super::super::service::script_events_to_requests(events);
        match requests.as_slice() {
            [CoreRequest::ToggleSurface { surface_id }] => {
                assert_eq!(surface_id, "@mesh/quick-settings");
            }
            other => panic!("expected quick settings ToggleSurface request, got {other:?}"),
        }
    }

    #[test]
    fn real_core_surfaces_quick_settings_close_publishes_hide_surface() {
        let mut ctx = make_network_ctx();
        ctx.load_script(
            r#"
function onClose()
    mesh.events.publish("shell.hide-surface", { surface_id = "@mesh/quick-settings" })
end
"#,
        )
        .unwrap();

        ctx.call_handler("onClose", &[]).unwrap();
        let requests =
            super::super::service::script_events_to_requests(ctx.drain_published_events());

        match requests.as_slice() {
            [CoreRequest::HideSurface { surface_id }] => {
                assert_eq!(surface_id, "@mesh/quick-settings");
            }
            other => panic!("expected quick settings HideSurface request, got {other:?}"),
        }
    }

    #[test]
    fn real_core_surfaces_quick_settings_commands_publish_service_requests() {
        let mut audio_ctx = make_audio_ctx();
        audio_ctx
            .load_script(
                r#"
local audio_ok, audio = pcall(require, "@mesh/audio@>=1.0")
if not audio_ok then audio = nil end

function onVolumeChange(value)
    local percent = math.floor((tonumber(value) or 0) + 0.5)
    if audio_ok and audio and audio.available ~= false then
        audio.set_volume("default", percent / 100)
    end
end
"#,
            )
            .unwrap();
        audio_ctx.apply_service_payload("audio", &serde_json::json!({ "available": true }));
        audio_ctx
            .call_handler("onVolumeChange", &[serde_json::json!(55)])
            .unwrap();
        let audio_requests =
            super::super::service::script_events_to_requests(audio_ctx.drain_published_events());

        match audio_requests.as_slice() {
            [
                CoreRequest::ServiceCommand {
                    interface,
                    command,
                    payload,
                    ..
                },
            ] => {
                assert_eq!(interface, "mesh.audio");
                assert_eq!(command, "set_volume");
                assert_eq!(
                    payload,
                    &serde_json::json!({ "device_id": "default", "volume": 0.55 })
                );
            }
            other => panic!("expected one mesh.audio set_volume command, got {other:?}"),
        }

        let mut network_ctx = make_network_ctx();
        network_ctx
            .load_script(
                r#"
local network_ok, network = pcall(require, "@mesh/network@>=1.0")
if not network_ok then network = nil end

function onToggleWiFi()
    if network_ok and network and network.available ~= false then
        network.set_wifi_enabled(not (network.wifi_enabled or false))
    end
end
"#,
            )
            .unwrap();
        network_ctx.apply_service_payload(
            "network",
            &serde_json::json!({ "available": true, "wifi_enabled": false }),
        );
        network_ctx.call_handler("onToggleWiFi", &[]).unwrap();
        let network_requests =
            super::super::service::script_events_to_requests(network_ctx.drain_published_events());

        match network_requests.as_slice() {
            [
                CoreRequest::ServiceCommand {
                    interface,
                    command,
                    payload,
                    ..
                },
            ] => {
                assert_eq!(interface, "mesh.network");
                assert_eq!(command, "set_wifi_enabled");
                assert_eq!(payload, &serde_json::json!({ "enabled": true }));
            }
            other => panic!("expected one mesh.network set_wifi_enabled command, got {other:?}"),
        }
    }

    #[test]
    fn real_core_surfaces_reject_legacy_service_callback_api_in_shipped_surfaces() {
        let sources = [
            (
                "panel",
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../packages/plugins/frontend/core/panel/src/main.mesh"
                )),
            ),
            (
                "quick-settings root",
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../packages/plugins/frontend/core/quick-settings/src/main.mesh"
                )),
            ),
            (
                "quick-settings audio",
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh"
                )),
            ),
            (
                "quick-settings wifi",
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh"
                )),
            ),
            (
                "quick-settings wifi item",
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../packages/plugins/frontend/core/quick-settings/src/components/wifi-item.mesh"
                )),
            ),
        ];

        for (name, source) in sources {
            assert_no_legacy_service_callbacks(name, source);
        }
    }

    #[test]
    fn real_core_surfaces_quick_settings_disabled_fallback_copy_stays_visible() {
        let audio_source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh"
        ));
        let wifi_source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh"
        ));
        let wifi_item_source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../packages/plugins/frontend/core/quick-settings/src/components/wifi-item.mesh"
        ));

        for copy in ["Audio unavailable", "Audio controls unavailable"] {
            assert!(
                audio_source.contains(copy),
                "audio section should keep visible fallback copy {copy}"
            );
        }
        for copy in [
            "Network unavailable",
            "Network controls unavailable",
            "Scanning for networks",
            "Wi-Fi is disabled",
        ] {
            assert!(
                wifi_source.contains(copy),
                "wifi section should keep visible fallback copy {copy}"
            );
        }
        assert!(
            wifi_item_source.contains("Connection details unavailable"),
            "wifi rows should keep visible fallback copy for unsafe connect data"
        );
    }
}
