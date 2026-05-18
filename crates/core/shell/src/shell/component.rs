use super::service::{apply_service_update, script_events_to_requests, seed_service_state};
use super::surface_layout::{
    SurfaceLayoutSettings, SurfaceSizePolicy, load_frontend_module_settings,
};
use super::types::{
    ComponentContext, ComponentError, ComponentInput, ComponentProfilingRecord, CoreEvent,
    CoreRequest, KeyModifiers, ServiceEvent, ShellComponent, TabFocusTarget,
};
use mesh_core_interaction::{
    annotate_overflow_tree, collect_focus_traversal, find_click_handler, find_event_handler,
    find_focusable_at, find_node_bounds_by_key, find_node_by_key, find_node_path_at,
    find_scrollable_at, find_tooltip_by_key, find_tooltip_text_by_key, is_input_key, is_slider_key,
    measure_content_size, namespace_event_handlers, next_focus_target, parse_namespaced_handler,
    scroll_limits,
};
mod animation;
mod catalog;
mod composition;
mod diagnostics;
mod input;
mod interaction_state;
mod rendering;
mod runtime;
mod runtime_tree;
mod shell_component;

use animation::StyleAnimation;
pub(in crate::shell) use catalog::FrontendCatalog;
#[cfg(test)]
pub(crate) use input::KeybindResolutionSource;
pub(in crate::shell) use mesh_core_interaction::ScrollOffsetState;
use runtime_tree::{
    RetainedWidgetTree, annotate_runtime_tree, collect_all_keys, collect_element_metrics,
    input_accepts_char,
};

use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_diagnostics::Diagnostics;
use mesh_core_elements::{
    IntrinsicLayoutCache, LayoutEngine, StyleContext, StyleResolver, VariableStore, WidgetNode,
    element_snapshot_json,
};
use mesh_core_frontend::{
    CompiledFrontendModule, FrontendRenderMode, compile_frontend_module, root_accessibility_role,
};
use mesh_core_locale::LocaleEngine;
use mesh_core_scripting::{LocaleBoundState, ScriptContext, ScriptInterfaceImport};
use mesh_core_theme::{Theme, default_theme};
use mesh_core_wayland::{Edge, KeyboardMode, ShellSurface};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mesh_core_render::{
    DamageRect, DisplayListMetrics, DisplayListRepaintPolicy, PixelBuffer, RenderObjectTree,
    RetainedDisplayList, SharedTextMeasurer, TextCacheMetrics, TextRenderer,
    paint_display_list_for_module_with_profiling_metrics,
};

const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
const TOOLTIP_OVERLAY_WIDTH: u32 = 260;
const TOOLTIP_OVERLAY_HEIGHT: u32 = 96;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub(super) struct ComponentDirtyFlags: u16 {
        const SCRIPT = 1 << 0;
        const STATE = 1 << 1;
        const STYLE = 1 << 2;
        const LAYOUT = 1 << 3;
        const PAINT = 1 << 4;
        const TEXT = 1 << 5;
        const ACCESSIBILITY = 1 << 6;
        const METRICS = 1 << 7;
        const SURFACE_CONFIG = 1 << 8;
    }
}

impl ComponentDirtyFlags {
    pub(super) const TREE_REBUILD: Self = Self::SCRIPT
        .union(Self::STATE)
        .union(Self::STYLE)
        .union(Self::LAYOUT)
        .union(Self::PAINT)
        .union(Self::TEXT)
        .union(Self::ACCESSIBILITY)
        .union(Self::METRICS);

    pub(super) const STYLE_RELAYOUT: Self = Self::STYLE
        .union(Self::LAYOUT)
        .union(Self::PAINT)
        .union(Self::ACCESSIBILITY)
        .union(Self::METRICS);

    pub(super) const TEXT_RELAYOUT: Self = Self::STATE
        .union(Self::TEXT)
        .union(Self::STYLE)
        .union(Self::LAYOUT)
        .union(Self::PAINT)
        .union(Self::ACCESSIBILITY)
        .union(Self::METRICS);

    pub(super) const VISUAL_REPAINT: Self = Self::STYLE
        .union(Self::PAINT)
        .union(Self::ACCESSIBILITY)
        .union(Self::METRICS);

    pub(super) const INTERACTION_RESTYLE: Self = Self::STATE
        .union(Self::STYLE)
        .union(Self::PAINT)
        .union(Self::ACCESSIBILITY)
        .union(Self::METRICS);

    pub(super) fn requires_tree_rebuild(self) -> bool {
        self.intersects(Self::SCRIPT | Self::TEXT)
    }

    pub(super) fn to_debug_counts(self) -> mesh_core_debug::ComponentInvalidationCounts {
        mesh_core_debug::ComponentInvalidationCounts {
            script: self.contains(Self::SCRIPT) as u64,
            state: self.contains(Self::STATE) as u64,
            style: self.contains(Self::STYLE) as u64,
            layout: self.contains(Self::LAYOUT) as u64,
            paint: self.contains(Self::PAINT) as u64,
            text: self.contains(Self::TEXT) as u64,
            accessibility: self.contains(Self::ACCESSIBILITY) as u64,
            metrics: self.contains(Self::METRICS) as u64,
            surface_config: self.contains(Self::SURFACE_CONFIG) as u64,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct EffectiveDamage {
    rect: Option<DamageRect>,
    full_surface: bool,
    policy: DisplayListRepaintPolicy,
}

impl EffectiveDamage {
    fn none() -> Self {
        Self {
            rect: None,
            full_surface: false,
            policy: DisplayListRepaintPolicy::MinimalDamage,
        }
    }
}

fn retained_paint_snapshot(
    metrics: DisplayListMetrics,
    effective_damage: EffectiveDamage,
) -> mesh_core_debug::RetainedPaintSnapshot {
    let damage_area = if effective_damage.full_surface {
        metrics.surface_area
    } else {
        effective_damage.rect.map(|rect| rect.area()).unwrap_or(0)
    };
    mesh_core_debug::RetainedPaintSnapshot {
        retained_generation: metrics.retained_generation,
        entries_total: metrics.entries_total,
        entries_reused: metrics.entries_reused,
        entries_rebuilt: metrics.entries_rebuilt,
        entries_removed: metrics.entries_removed,
        subtree_segments_reused: metrics.subtree_segments_reused,
        subtree_segments_rebuilt: metrics.subtree_segments_rebuilt,
        subtree_commands_rebuilt: metrics.subtree_commands_rebuilt,
        full_fallback_count: metrics.full_fallback_count,
        broad_dirty_fallback_count: metrics.broad_dirty_fallback_count,
        damage_rect_count: u64::from(effective_damage.rect.is_some()),
        damage_area,
        surface_area: metrics.surface_area,
        full_surface_damage: effective_damage.full_surface,
        partial_present_supported: metrics.partial_present_supported,
        skipped_paint_pixels: if metrics.partial_present_supported {
            metrics.surface_area.saturating_sub(damage_area)
        } else {
            0
        },
        omitted_subtrees: metrics.omitted_subtrees,
        omitted_nodes: metrics.omitted_nodes,
        omitted_commands: metrics.omitted_commands,
        preclipped_descendants: metrics.preclipped_descendants,
        repaint_policy: repaint_policy_snapshot(metrics.repaint_policy),
        filtered_span_count: metrics.filtered_span_count,
        filtered_command_count: metrics.filtered_command_count,
        filtered_commands_skipped: metrics.filtered_commands_skipped,
        filtered_fallback_count: metrics.filtered_fallback_count,
        batch_count: metrics.batch_count,
        batched_primitives: metrics.batched_primitives,
        barrier_count: metrics.barrier_count,
        barriers: mesh_core_debug::DisplayBatchBarrierSnapshot {
            text: metrics.barriers.text,
            icon: metrics.barriers.icon,
            opacity: metrics.barriers.opacity,
            clip: metrics.barriers.clip,
            translucency: metrics.barriers.translucency,
            material_change: metrics.barriers.material_change,
        },
        ..Default::default()
    }
}

fn repaint_policy_snapshot(
    policy: DisplayListRepaintPolicy,
) -> mesh_core_debug::RepaintPolicySnapshot {
    match policy {
        DisplayListRepaintPolicy::MinimalDamage => {
            mesh_core_debug::RepaintPolicySnapshot::MinimalDamage
        }
        DisplayListRepaintPolicy::BoundingRect => {
            mesh_core_debug::RepaintPolicySnapshot::BoundingRect
        }
        DisplayListRepaintPolicy::FullSurface => {
            mesh_core_debug::RepaintPolicySnapshot::FullSurface
        }
    }
}

fn text_cache_snapshot(metrics: TextCacheMetrics) -> mesh_core_debug::TextCacheSnapshot {
    mesh_core_debug::TextCacheSnapshot {
        layout_hits: metrics.layout_hits,
        layout_misses: metrics.layout_misses,
        layout_invalidations: metrics.layout_invalidations,
        shaped_entries: metrics.shaped_entries,
        glyph_cache_active: metrics.glyph_cache_active,
        shaping_micros: metrics.shaping_micros,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct TextSelectionPoint {
    pub(super) node_key: String,
    pub(super) x: f32,
    pub(super) y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct TextSelectionState {
    pub(super) anchor: TextSelectionPoint,
    pub(super) focus: TextSelectionPoint,
    pub(super) dragging: bool,
}

pub(super) struct FrontendSurfaceComponent {
    pub(super) compiled: CompiledFrontendModule,
    pub(super) module_dir: PathBuf,
    module_settings_file: PathBuf,
    settings_json: serde_json::Value,
    pub(super) surface_layout: SurfaceLayoutSettings,
    /// Runtime override for `surface_layout.keyboard_mode`. Used during
    /// cross-surface Tab transfer to force `Exclusive` on the popover
    /// (compositors don't reliably switch `OnDemand` mid-flight). `None`
    /// means use the configured value from the manifest. Cleared when the
    /// surface hides.
    pub(super) keyboard_mode_override: Option<KeyboardMode>,
    pub(super) frontend_catalog: FrontendCatalog,
    pub(super) visible: bool,
    surface_exiting: bool,
    dirty: bool,
    /// Set when only appearance changed (e.g. hover) without script-state
    /// changes. Triggers a paint via `wants_render`, but lets `paint` skip the
    /// expensive Luau-driven `build_tree_with_state` and reuse the previously
    /// built widget tree, only re-annotating hover/focus state and re-running
    /// restyle + layout. Cleared on render alongside `dirty`.
    style_only_dirty: bool,
    dirty_types: ComponentDirtyFlags,
    last_dirty_types: ComponentDirtyFlags,
    last_service_update: Option<String>,
    focused_key: Option<String>,
    focus_visible_key: Option<String>,
    pointer_down_key: Option<String>,
    active_slider_key: Option<String>,
    /// When a surface with keyboard interactivity transitions visible→true,
    /// this flag tells the next paint to seed focus on the first tabbable
    /// element. Lets a popover work with keyboard immediately after opening
    /// without the user needing to click inside it first.
    pending_auto_focus: bool,
    /// Set when focus is transferred INTO this surface from another via Tab.
    /// `(surface_id, key)` of the trigger element to return to when Tab/
    /// Shift+Tab leaves this surface's chain. None for top-level surfaces
    /// (panels, navbar) that own the start of a focus chain.
    pub(super) return_focus: Option<(String, String)>,
    /// Set when this surface should be hidden after Tab/Shift+Tab leaves
    /// its chain. True for popovers transferred-into; false for stable
    /// surfaces. Reset whenever `return_focus` is reset.
    pub(super) close_on_focus_leave: bool,
    /// `trigger_key → popover_surface_id` for popovers activated *from*
    /// this surface. Populated by the shell when `mesh.popover.activate`
    /// runs. Tab forward on a trigger key transfers focus into the
    /// matching popover when activation did not already focus it; the
    /// entry is dropped when the popover hides.
    pub(super) triggered_popovers: HashMap<String, String>,
    selection: Option<TextSelectionState>,
    input_values: HashMap<String, String>,
    slider_values: HashMap<String, f32>,
    slider_script_values: HashMap<String, f32>,
    checked_values: HashMap<String, bool>,
    render_hooks_pending: bool,
    pub(super) scroll_offsets: HashMap<String, ScrollOffsetState>,
    // Hover tracking for CSS :hover and tooltip system.
    hovered_key: Option<String>,
    hovered_path: Vec<String>,
    hovered_pos: (f32, f32),
    hover_start: Option<std::time::Instant>,
    last_tooltip_damage: Option<DamageRect>,
    runtimes: Arc<Mutex<HashMap<String, EmbeddedFrontendRuntime>>>,
    render_stack: RefCell<Vec<String>>,
    active_theme: RefCell<Theme>,
    measured_size: Option<(u32, u32)>,
    last_surface_size: Option<(u32, u32)>,
    surface_pixels_invalid: bool,
    locale: LocaleEngine,
    interface_catalog: mesh_core_service::InterfaceCatalog,
    last_tree: Option<WidgetNode>,
    intrinsic_layout_cache: IntrinsicLayoutCache,
    retained_tree: RetainedWidgetTree,
    retained_render_objects: RenderObjectTree,
    retained_display_list: RetainedDisplayList,
    diagnostics: Option<Diagnostics>,
    /// Desired visibility for surface portals (`<ImportedSurface hidden={...} />`).
    /// Updated during build_tree; compared to last_surface_states in tick().
    pending_surface_states: RefCell<HashMap<String, bool>>,
    /// Last visibility state emitted for each surface portal, to avoid redundant requests.
    last_surface_states: HashMap<String, bool>,
    /// `surface_id -> state variable` for portals declared as
    /// `<ImportedSurface hidden={some_state} />`. Used when the shell hides
    /// a popover through keyboard navigation so the owner script does not
    /// immediately re-show it from stale state.
    portal_hidden_bindings: RefCell<HashMap<String, String>>,
    style_animations: HashMap<String, StyleAnimation>,
    keyframe_animations: HashMap<String, mesh_core_animation::keyframes::ActiveKeyframeAnimation>,
    keyframe_rules: HashMap<String, mesh_core_animation::keyframes::KeyframeRule>,
    has_active_keyframe_animation: bool,
    profiling_enabled: bool,
    profiling_records: Vec<ComponentProfilingRecord>,
    invalidation_snapshot: Option<mesh_core_debug::ProfilingInvalidationSnapshot>,
    focused_proof_snapshot: Option<mesh_core_render::FocusedProofSnapshot>,
    last_present_damage: Option<DamageRect>,
    /// Cached aggregate of restyle rules collected from `compiled.component`
    /// and every entry in `compiled.local_components`. Populated lazily on the
    /// first restyle and invalidated whenever the compiled module is replaced
    /// (source reload). Avoids allocating + cloning every StyleRule per paint.
    cached_restyle_rules: Option<Vec<mesh_core_component::style::StyleRule>>,
}

#[derive(Debug)]
struct EmbeddedFrontendRuntime {
    module_id: String,
    script_ctx: ScriptContext,
}

impl FrontendSurfaceComponent {
    pub(super) fn new(
        compiled: CompiledFrontendModule,
        module_dir: PathBuf,
        frontend_catalog: FrontendCatalog,
        interface_catalog: mesh_core_service::InterfaceCatalog,
    ) -> Self {
        let module_settings_file = module_dir.join("config/settings.json");
        let settings_state =
            load_frontend_module_settings(&module_settings_file, &compiled.manifest);
        Self {
            compiled,
            module_dir,
            module_settings_file,
            settings_json: settings_state.raw,
            surface_layout: settings_state.layout.clone(),
            keyboard_mode_override: None,
            frontend_catalog,
            visible: settings_state.layout.visible_on_start,
            surface_exiting: false,
            dirty: true,
            style_only_dirty: false,
            dirty_types: ComponentDirtyFlags::TREE_REBUILD | ComponentDirtyFlags::SURFACE_CONFIG,
            last_dirty_types: ComponentDirtyFlags::empty(),
            last_service_update: None,
            focused_key: None,
            focus_visible_key: None,
            pointer_down_key: None,
            active_slider_key: None,
            pending_auto_focus: settings_state.layout.visible_on_start
                && settings_state.layout.keyboard_mode != KeyboardMode::None,
            return_focus: None,
            close_on_focus_leave: false,
            triggered_popovers: HashMap::new(),
            selection: None,
            input_values: HashMap::new(),
            slider_values: HashMap::new(),
            slider_script_values: HashMap::new(),
            checked_values: HashMap::new(),
            render_hooks_pending: true,
            scroll_offsets: HashMap::new(),
            hovered_key: None,
            hovered_path: Vec::new(),
            hovered_pos: (0.0, 0.0),
            hover_start: None,
            last_tooltip_damage: None,
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            render_stack: RefCell::new(Vec::new()),
            active_theme: RefCell::new(default_theme()),
            measured_size: None,
            last_surface_size: None,
            surface_pixels_invalid: true,
            locale: LocaleEngine::new("en"),
            interface_catalog,
            last_tree: None,
            intrinsic_layout_cache: IntrinsicLayoutCache::default(),
            retained_tree: RetainedWidgetTree::default(),
            retained_render_objects: RenderObjectTree::default(),
            retained_display_list: RetainedDisplayList::default(),
            diagnostics: None,
            pending_surface_states: RefCell::new(HashMap::new()),
            last_surface_states: HashMap::new(),
            portal_hidden_bindings: RefCell::new(HashMap::new()),
            style_animations: HashMap::new(),
            keyframe_animations: HashMap::new(),
            keyframe_rules: HashMap::new(),
            has_active_keyframe_animation: false,
            profiling_enabled: false,
            profiling_records: Vec::new(),
            invalidation_snapshot: None,
            focused_proof_snapshot: None,
            last_present_damage: None,
            cached_restyle_rules: None,
        }
    }

    pub(super) fn invalidate(&mut self, flags: ComponentDirtyFlags) {
        self.dirty_types |= flags;
        self.dirty = true;
        if invalidation_requires_pixel_repaint(flags) {
            self.surface_pixels_invalid = true;
        }
    }

    pub(super) fn invalidate_style_path(&mut self, flags: ComponentDirtyFlags) {
        self.dirty_types |= flags;
        self.style_only_dirty = true;
        if invalidation_requires_pixel_repaint(flags) {
            self.surface_pixels_invalid = true;
        }
    }

    pub(super) fn invalidate_script_state(&mut self) {
        // Handler-driven state mutations can change any rendered value
        // (slider knob position, text content, icon names). Force a full
        // pixel-buffer repaint to bypass the selective-damage shortcut, which
        // can misjudge damage for content-only changes (e.g. drag-driven
        // continuous text and slider knob updates).
        self.surface_pixels_invalid = true;
        self.invalidate(ComponentDirtyFlags::TREE_REBUILD);
    }

    pub(super) fn invalidate_interaction_restyle(&mut self) {
        self.invalidate_style_path(ComponentDirtyFlags::INTERACTION_RESTYLE);
    }

    pub(super) fn invalidate_text_state(&mut self) {
        self.invalidate(ComponentDirtyFlags::TEXT_RELAYOUT);
    }

    pub(super) fn invalidate_paint(&mut self) {
        self.invalidate_style_path(ComponentDirtyFlags::PAINT);
    }

    pub(super) fn invalidate_surface_config(&mut self) {
        self.invalidate_surface_config_only();
    }

    pub(super) fn invalidate_surface_config_only(&mut self) {
        self.invalidate_style_path(ComponentDirtyFlags::SURFACE_CONFIG);
    }

    pub(super) fn take_dirty_for_paint(
        &mut self,
    ) -> (bool, bool, ComponentDirtyFlags, ComponentDirtyFlags) {
        let legacy_dirty = self.dirty && self.dirty_types.is_empty();
        let legacy_style_only = self.style_only_dirty && self.dirty_types.is_empty();
        let flags = self.dirty_types;
        let requires_tree_rebuild = legacy_dirty || flags.requires_tree_rebuild();
        let can_use_retained_path =
            !requires_tree_rebuild && (legacy_style_only || !flags.is_empty());

        self.last_dirty_types = flags;
        self.dirty_types = ComponentDirtyFlags::empty();
        self.dirty = false;
        self.style_only_dirty = false;

        (
            requires_tree_rebuild,
            can_use_retained_path,
            flags,
            self.last_dirty_types,
        )
    }
}

fn invalidation_requires_pixel_repaint(flags: ComponentDirtyFlags) -> bool {
    flags.intersects(
        ComponentDirtyFlags::STATE
            | ComponentDirtyFlags::STYLE
            | ComponentDirtyFlags::LAYOUT
            | ComponentDirtyFlags::PAINT
            | ComponentDirtyFlags::TEXT
            | ComponentDirtyFlags::ACCESSIBILITY
            | ComponentDirtyFlags::METRICS,
    )
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

pub(super) fn grant_capabilities_from_manifest(
    manifest: &mesh_core_module::Manifest,
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
mod tests;
