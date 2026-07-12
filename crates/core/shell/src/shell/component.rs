use super::service::{
    apply_service_update_with_name, script_events_to_requests, seed_service_state,
};
use super::surface_layout::{SurfaceLayoutSettings, load_frontend_module_settings};
use super::types::{
    ChildSurfaceKind, ChildSurfaceRequest, ComponentContext, ComponentError, ComponentInput,
    ComponentProfilingRecord, CoreEvent, CoreRequest, KeyModifiers, ServiceEvent, ShellComponent,
    TabFocusTarget,
};
use mesh_core_interaction::{
    annotate_overflow_tree, collect_focus_traversal, find_click_handler, find_event_handler,
    find_focusable_at, find_node_bounds_by_key, find_node_by_key, find_node_path_at,
    find_nodes_by_keys, find_scrollable_at_with_limits, find_tooltip_by_key,
    find_tooltip_container_bounds, is_input_key, is_slider_key, measure_content_size,
    next_focus_target, node_is_source, parse_namespaced_handler, scroll_into_view_offsets,
    scroll_limits, source_element_tag,
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
mod tooltip;

pub(in crate::shell) use catalog::FrontendCatalog;
#[cfg(test)]
pub(crate) use input::KeybindResolutionSource;
use input::ResolvedSurfaceShortcut;
use mesh_core_animation::transition::TransitionAnimator;
pub(in crate::shell) use mesh_core_interaction::ScrollOffsetState;
use runtime_tree::{
    NodeServiceFieldDependencies, RetainedWidgetTree, RuntimeAnnotationContext,
    annotate_runtime_tree, collect_element_metrics, input_accepts_char,
};

use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_component::template::{AttributeValue, TemplateNode};
use mesh_core_config::TooltipSettings;
use mesh_core_diagnostics::Diagnostics;
use mesh_core_elements::{
    IntrinsicLayoutCache, LayoutEngine, NodeId, PerSurfaceLayoutState, PopoverPlacement,
    StyleContext, StyleResolver, VariableStore, WidgetNode, element_snapshot_json,
};
use mesh_core_frontend::{
    CompiledFrontendModule, FrontendRenderMode, compile_frontend_module, root_accessibility_role,
};
use mesh_core_locale::LocaleEngine;
use mesh_core_scripting::{
    LocaleBoundState, PublishedEvent, ScriptContext, ScriptInterfaceImport, ScriptState, SurfaceVm,
};
use mesh_core_theme::{Theme, default_theme};
use mesh_core_wayland::{Edge, KeyboardMode, ShellSurface};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) type SurfaceCssProps = HashMap<String, mesh_core_component::style::StyleValue>;

use mesh_core_render::{
    DamageRect, DisplayListMetrics, DisplayListRepaintPolicy, DisplayPaintCommand, PixelBuffer,
    RenderObjectTree, RetainedDisplayList, SharedTextMeasurer, TextCacheMetrics, TextRenderer,
};

const TOOLTIP_OVERLAY_WIDTH: u32 = 240;
const TOOLTIP_OVERLAY_HEIGHT: u32 = 80;

/// Extra logical pixels a parent layer surface reserves beyond its content so
/// tooltips can paint outside the content box (e.g. below a bar).
///
/// The reserve is a presentation-boundary concern only: the compositor
/// configure and the paint buffer are inflated by it (`render_components`),
/// while every component-facing size — `surface_size_changed`,
/// `observe_surface_size`, `content_input_size`, popup sizing — stays the
/// plain content size. Feeding an inflated size back into the component
/// invalidates its measurement cache and ping-pongs with paint's own content
/// observation, forcing a full rebuild every frame. Pointer input is confined
/// back to the content rect at present time, so the reserve never takes
/// clicks or focus from windows beneath it.
pub(in crate::shell) fn tooltip_overlay_extra_for_content(width: u32, height: u32) -> (u32, u32) {
    let extra_w = if width > 0 && width < TOOLTIP_OVERLAY_WIDTH {
        TOOLTIP_OVERLAY_WIDTH.saturating_sub(width)
    } else {
        0
    };
    let extra_h = if height > 0 {
        TOOLTIP_OVERLAY_HEIGHT
    } else {
        0
    };
    (extra_w, extra_h)
}

/// Marker attribute set on an embedded `<popover>` wrapper that is promoted to a
/// child surface. `finalize_tree` re-applies the out-of-flow collapse to nodes
/// carrying this after each restyle pass (restyle re-resolves `computed_style`
/// from CSS and would otherwise drop imperatively-set geometry).
pub(super) const PROMOTED_POPOVER_MARKER: &str = "_mesh_promoted_popover";
pub(super) const ERROR_PLACEHOLDER_MARKER: &str = "_mesh_error_placeholder";
pub(super) const ERROR_PLACEHOLDER_MAX_WIDTH: f32 = 320.0;

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
        /// Leaf-only script change; bypasses TREE_REBUILD.
        /// Does NOT trigger a full Luau re-evaluation of the tree structure;
        /// instead, `narrow_script_update()` diffs the freshly rebuilt tree
        /// against the retained tree and marks only changed leaf nodes dirty.
        const SCRIPT_NARROW = 1 << 9;
    }
}

/// A running smooth-scroll animation from `start` to `target` over `duration`,
/// eased with `EaseOut`. Created when a script requests a scroll with
/// `{ smooth = true }`; advanced each frame by `advance_scroll_animations`.
#[derive(Debug, Clone, Copy)]
pub(super) struct ScrollAnimation {
    pub(super) start: ScrollOffsetState,
    pub(super) target: ScrollOffsetState,
    pub(super) start_time: std::time::Instant,
    pub(super) duration: std::time::Duration,
}

#[derive(Debug, Clone)]
struct ScheduledHandler {
    instance_key: String,
    handler: String,
    deadline: Instant,
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
        .union(Self::LAYOUT)
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
            script_narrow: self.contains(Self::SCRIPT_NARROW) as u64,
        }
    }
}

const MAX_DAMAGE_RECTS: usize = 4;

#[derive(Debug, Clone)]
struct EffectiveDamage {
    rect: Option<DamageRect>,
    rects: Vec<DamageRect>,
    full_surface: bool,
    policy: DisplayListRepaintPolicy,
}

impl EffectiveDamage {
    fn none() -> Self {
        Self {
            rect: None,
            rects: Vec::new(),
            full_surface: false,
            policy: DisplayListRepaintPolicy::MinimalDamage,
        }
    }

    fn damage_area(&self, surface_area: u64) -> u64 {
        if self.full_surface {
            surface_area
        } else {
            self.rects.iter().map(|rect| rect.area()).sum()
        }
    }

    fn damage_rect_count(&self) -> u64 {
        if self.full_surface {
            u64::from(self.rect.is_some())
        } else {
            self.rects.len() as u64
        }
    }
}

fn retained_paint_snapshot(
    metrics: DisplayListMetrics,
    effective_damage: &EffectiveDamage,
) -> mesh_core_debug::RetainedPaintSnapshot {
    let damage_area = effective_damage.damage_area(metrics.surface_area);
    mesh_core_debug::RetainedPaintSnapshot {
        retained_generation: metrics.retained_generation,
        entries_total: metrics.entries_total,
        entries_reused: metrics.entries_reused,
        entries_rebuilt: metrics.entries_rebuilt,
        entries_removed: metrics.entries_removed,
        subtree_segments_reused: metrics.subtree_segments_reused,
        subtree_segments_rebuilt: metrics.subtree_segments_rebuilt,
        subtree_commands_rebuilt: metrics.subtree_commands_rebuilt,
        changed_layout_count: metrics.changed_layout_count,
        changed_paint_count: metrics.changed_paint_count,
        effect_overflow_count: metrics.effect_overflow_count,
        fallback_promotion_count: metrics.fallback_promotion_count,
        full_fallback_count: metrics.full_fallback_count,
        broad_dirty_fallback_count: metrics.broad_dirty_fallback_count,
        damage_rect_count: effective_damage.damage_rect_count(),
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
    /// True while this surface is promoted to an `xdg_popup`. Popups are placed
    /// by their `xdg_positioner` (see `configure_popup`), so `render_layout`
    /// must not poke anchor/margin/size onto the underlying surface — doing so
    /// is harmless (the layer-surface `configure()` is skipped for popups) but
    /// noisy. Set/cleared alongside the wrapper's `popup_config`.
    pub(super) popup_promoted: bool,
    pub(super) frontend_catalog: Arc<FrontendCatalog>,
    graph_i18n_catalogs: Vec<(String, String, PathBuf)>,
    pub(super) visible: bool,
    surface_exiting: bool,
    surface_entering: bool,
    /// `_mesh_key`s of in-tree child popovers currently playing their exit
    /// transition. `finalize_tree` appends `mesh-surface-exiting` scoped to
    /// just these subtrees (not the whole tree, unlike `surface_exiting`) so
    /// each popover's own CSS exit animation resolves and advances through
    /// the normal transition engine while the shell keeps its child surface
    /// mapped. Set by the shell via `set_closing_child_keys`.
    closing_child_keys: HashSet<String>,
    /// `_mesh_key`s of newly opened child popovers receiving a controlled
    /// first paint in their collapsed entrance state.
    entering_child_keys: HashSet<String>,
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
    cached_service_payloads: HashMap<String, std::sync::Arc<serde_json::Value>>,
    focused_key: Option<String>,
    focus_visible_key: Option<String>,
    pointer_down_key: Option<String>,
    pointer_down_bounds: Option<(f32, f32, f32, f32)>,
    active_slider_key: Option<String>,
    keyboard_button_press_activations: HashSet<(String, String)>,
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
    scheduled_handlers: HashMap<String, ScheduledHandler>,
    /// In-flight smooth-scroll animations keyed by scroll-container node key.
    /// Ticked at the top of `finalize_tree`; each writes an eased offset into
    /// `scroll_offsets` until it settles, then is dropped. Started by
    /// `refs.x:scroll_to(.., { smooth = true })` / `:scroll_into_view({ smooth })`.
    pub(super) scroll_animations: HashMap<String, ScrollAnimation>,
    // Hover tracking for CSS :hover and tooltip system.
    hovered_key: Option<String>,
    hovered_path: Vec<String>,
    hovered_tooltip: Option<(String, String)>,
    /// Previous frame's hovered path — used to detect which nodes' hover state
    /// changed between frames for targeted interaction restyle.
    previous_hovered_path: Vec<String>,
    /// Previous frame's focused key — used to detect which node's focus state
    /// changed between frames for targeted interaction restyle.
    previous_focused_key: Option<String>,
    interaction_snapshot_valid: bool,
    hovered_pos: (f32, f32),
    hover_start: Option<std::time::Instant>,
    tooltip_visible: bool,
    /// Bounding box of the currently hovered element: (left, top, right, bottom).
    hovered_element_bounds: Option<(f32, f32, f32, f32)>,
    /// Timestamp when the current tooltip became visible (for fade-in animation timing).
    tooltip_appeared_at: Option<std::time::Instant>,
    last_tooltip_damage: Option<DamageRect>,
    runtimes: Arc<Mutex<HashMap<String, EmbeddedFrontendRuntime>>>,
    /// The single Lua realm shared by every component instance in this surface.
    /// Each runtime's `ScriptContext` attaches a clone, so sibling/child
    /// components can hold live `bind:this` references to one another.
    surface_vm: SurfaceVm,
    render_stack: RefCell<Vec<String>>,
    /// The theme used by the current/last paint, shared cheaply with child
    /// component builds and animation restyle. Refreshed from the paint-time
    /// `&Theme` only when `active_theme_stale` is set — cloning the full
    /// token/defaults maps every frame is wasted work while the theme is
    /// unchanged.
    active_theme: RefCell<Arc<Theme>>,
    /// Set on construction and by `theme_changed()`; cleared once the next
    /// paint captures the new theme into `active_theme`.
    active_theme_stale: Cell<bool>,
    measured_size: Option<(u32, u32)>,
    last_surface_size: Option<(u32, u32)>,
    last_painted_buffer_size: Option<(u32, u32)>,
    surface_pixels_invalid: bool,
    locale: LocaleEngine,
    interface_catalog: Arc<mesh_core_service::InterfaceCatalog>,
    last_tree: Option<WidgetNode>,
    intrinsic_layout_cache: IntrinsicLayoutCache,
    layout_state: PerSurfaceLayoutState,
    pub(super) retained_tree: RetainedWidgetTree,
    node_service_field_deps: NodeServiceFieldDependencies,
    retained_render_objects: RenderObjectTree,
    retained_display_list: RetainedDisplayList,
    diagnostics: Option<Diagnostics>,
    /// Desired visibility for surface portals (`<ImportedSurface hidden={...} />`).
    /// Updated during build_tree; compared to last_surface_states in tick().
    pending_surface_states: RefCell<HashMap<String, bool>>,
    /// Last visibility state emitted for each surface portal, to avoid redundant requests.
    last_surface_states: HashMap<String, bool>,
    /// `surface_id -> (owner_instance_key, state variable)` for portals
    /// declared as `<ImportedSurface hidden={some_state} />`. Used when the
    /// shell hides a popover through keyboard navigation so the owner script
    /// does not immediately re-show it from stale state. The owner instance
    /// key identifies which component runtime owns the bound variable — the
    /// portal may be declared inside a nested child component (e.g. a
    /// navigation-bar button), not the surface's root component, so the
    /// write-back must target that child's `_ENV`, not the root's.
    portal_hidden_bindings: RefCell<HashMap<String, (String, String)>>,
    /// `parent_instance_key -> [(binding, child_instance_key)]` for live
    /// `bind:this` references. After a parent event handler runs, each linked
    /// child is re-synced so values its parent mutated through the live proxy
    /// re-render. Refreshed every render by `bind_child_instance`.
    bound_children: RefCell<HashMap<String, Vec<(String, String)>>>,
    /// `refs.<name>` -> live widget node key, rebuilt every paint by
    /// `publish_element_metrics`. Lets imperative element actions
    /// (`refs.<name>:focus()`) resolve a script-facing ref name back to the
    /// retained node it targets.
    ref_node_keys: RefCell<HashMap<String, String>>,
    transitions: TransitionAnimator,
    keyframe_animations: HashMap<String, mesh_core_animation::keyframes::ActiveKeyframeAnimation>,
    keyframe_rules: HashMap<String, mesh_core_animation::keyframes::KeyframeRule>,
    has_animatable_style_rules: bool,
    has_active_keyframe_animation: bool,
    has_promoted_popover_wrappers: Cell<bool>,
    has_error_placeholders: Cell<bool>,
    narrow_path_active: bool,
    affected_node_count: u64,
    profiling_enabled: bool,
    profiling_records: Vec<ComponentProfilingRecord>,
    invalidation_snapshot: Option<mesh_core_debug::ProfilingInvalidationSnapshot>,
    focused_proof_snapshot: Option<mesh_core_render::FocusedProofSnapshot>,
    last_present_damage_rects: Vec<DamageRect>,
    last_visual_damage: HashMap<NodeId, DamageRect>,
    tooltip_damage_scratch: Vec<DamageRect>,
    dirty_node_visual_damage_scratch: Vec<DamageRect>,
    /// Current tooltip configuration from shell settings. Refreshed while a
    /// tooltip hover is active so settings changes apply without remounting.
    tooltip_settings: TooltipSettings,
    /// Enter animation lowered from the active theme's CSS (`tooltip {
    /// animation: ... }` + `@keyframes`). `None` = show instantly.
    tooltip_animation: Option<tooltip::TooltipAnimation>,
    visual_damage_scratch: Vec<DamageRect>,
    effective_damage_scratch: Vec<DamageRect>,
    /// Cached aggregate of restyle rules collected from `compiled.component`
    /// and every entry in `compiled.local_components`. Populated lazily on the
    /// first restyle and invalidated whenever the compiled module is replaced
    /// (source reload). Avoids allocating + cloning every StyleRule per paint.
    cached_restyle_rules: Option<Vec<mesh_core_component::style::StyleRule>>,
    /// Cached `StyleRuleIndex` built from `cached_restyle_rules`. Reused
    /// across restyle passes; `is_for()` verifies identity against the rules
    /// slice before each restyle so a rules rebuild forces a rebuild here too.
    cached_style_rule_index: Option<mesh_core_elements::style::StyleRuleIndex>,
    /// Which per-element host metric tables this module can observe. When both
    /// flags are false, `publish_element_metrics` is skipped: building the JSON
    /// snapshots costs meaningful interaction-frame time and is wasted on
    /// scripts that never read them. Recomputed on source reload.
    element_metric_usage: ElementMetricUsage,
    /// Cache of the global `KeyboardSettings` (button/toggle/slider activation
    /// keys, surface shortcut overrides) keyed by the mtimes of the two files
    /// `load_shell_settings` reads. Every key press/release re-derives this,
    /// so without a cache typing in a launcher input pays a file read + JSON
    /// parse + merge per keystroke. Invalidated by re-stat, not a full
    /// re-parse, so it stays correct across live settings edits.
    keyboard_settings_cache: RefCell<Option<KeyboardSettingsCache>>,
    /// Cache of resolved surface shortcuts keyed by the already-cached
    /// `KeyboardSettings` plus active locale. Resolution clones manifest
    /// declarations, checks overrides, and localizes triggers, so avoid doing
    /// that again for every key event when neither input changed.
    resolved_surface_shortcuts_cache: RefCell<Option<ResolvedSurfaceShortcutsCache>>,
}

#[derive(Debug, Clone)]
struct KeyboardSettingsCache {
    defaults_mtime: Option<std::time::SystemTime>,
    user_mtime: Option<std::time::SystemTime>,
    settings: mesh_core_config::KeyboardSettings,
}

#[derive(Debug, Clone)]
struct ResolvedSurfaceShortcutsCache {
    keyboard_settings: mesh_core_config::KeyboardSettings,
    locale: String,
    shortcuts: Vec<ResolvedSurfaceShortcut>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ElementMetricUsage {
    elements: bool,
    refs: bool,
}

impl ElementMetricUsage {
    fn any(self) -> bool {
        self.elements || self.refs
    }
}

#[derive(Debug)]
struct EmbeddedFrontendRuntime {
    module_id: String,
    script_ctx: ScriptContext,
    /// Cached clone of the script state, keyed by its mutation generation.
    /// Tree builds need a state snapshot that outlives the runtimes lock;
    /// this avoids re-cloning the full variable map on every frame the
    /// state did not change.
    cached_state_clone: Option<(u64, Arc<ScriptState>)>,
}

impl FrontendSurfaceComponent {
    pub(super) fn new(
        compiled: CompiledFrontendModule,
        module_dir: PathBuf,
        frontend_catalog: impl Into<Arc<FrontendCatalog>>,
        interface_catalog: impl Into<Arc<mesh_core_service::InterfaceCatalog>>,
    ) -> Self {
        let module_settings_file = module_dir.join("config/settings.json");
        let settings_state =
            load_frontend_module_settings(&module_settings_file, &compiled.manifest);
        let service_payload_capacity = service_payload_cache_capacity(&compiled.manifest);
        let element_metric_usage = element_metric_usage(&compiled);
        let has_animatable_style_rules = compiled_module_has_animatable_style_rules(&compiled);
        Self {
            compiled,
            module_dir,
            module_settings_file,
            settings_json: settings_state.raw,
            surface_layout: settings_state.layout.clone(),
            keyboard_mode_override: None,
            popup_promoted: false,
            frontend_catalog: frontend_catalog.into(),
            graph_i18n_catalogs: Vec::new(),
            visible: settings_state.layout.visible_on_start,
            surface_exiting: false,
            surface_entering: false,
            closing_child_keys: HashSet::new(),
            entering_child_keys: HashSet::new(),
            dirty: true,
            style_only_dirty: false,
            dirty_types: ComponentDirtyFlags::TREE_REBUILD | ComponentDirtyFlags::SURFACE_CONFIG,
            last_dirty_types: ComponentDirtyFlags::empty(),
            last_service_update: None,
            cached_service_payloads: HashMap::with_capacity(service_payload_capacity),
            focused_key: None,
            focus_visible_key: None,
            pointer_down_key: None,
            pointer_down_bounds: None,
            active_slider_key: None,
            keyboard_button_press_activations: HashSet::new(),
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
            scheduled_handlers: HashMap::new(),
            scroll_animations: HashMap::new(),
            hovered_key: None,
            hovered_path: Vec::new(),
            hovered_tooltip: None,
            previous_hovered_path: Vec::new(),
            previous_focused_key: None,
            interaction_snapshot_valid: false,
            hovered_pos: (0.0, 0.0),
            hover_start: None,
            tooltip_visible: false,
            hovered_element_bounds: None,
            tooltip_appeared_at: None,
            last_tooltip_damage: None,
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            surface_vm: SurfaceVm::new(),
            render_stack: RefCell::new(Vec::new()),
            active_theme: RefCell::new(Arc::new(default_theme())),
            active_theme_stale: Cell::new(true),
            measured_size: None,
            last_surface_size: None,
            last_painted_buffer_size: None,
            surface_pixels_invalid: true,
            locale: LocaleEngine::new("en"),
            interface_catalog: interface_catalog.into(),
            last_tree: None,
            intrinsic_layout_cache: IntrinsicLayoutCache::default(),
            layout_state: PerSurfaceLayoutState::default(),
            retained_tree: RetainedWidgetTree::default(),
            node_service_field_deps: NodeServiceFieldDependencies::default(),
            retained_render_objects: RenderObjectTree::default(),
            retained_display_list: RetainedDisplayList::default(),
            diagnostics: None,
            pending_surface_states: RefCell::new(HashMap::new()),
            last_surface_states: HashMap::new(),
            portal_hidden_bindings: RefCell::new(HashMap::new()),
            bound_children: RefCell::new(HashMap::new()),
            ref_node_keys: RefCell::new(HashMap::new()),
            transitions: TransitionAnimator::new(),
            keyframe_animations: HashMap::new(),
            keyframe_rules: HashMap::new(),
            has_animatable_style_rules,
            has_active_keyframe_animation: false,
            has_promoted_popover_wrappers: Cell::new(false),
            has_error_placeholders: Cell::new(false),
            narrow_path_active: false,
            affected_node_count: 0,
            profiling_enabled: false,
            profiling_records: Vec::new(),
            invalidation_snapshot: None,
            focused_proof_snapshot: None,
            last_present_damage_rects: Vec::new(),
            last_visual_damage: HashMap::new(),
            tooltip_damage_scratch: Vec::new(),
            dirty_node_visual_damage_scratch: Vec::new(),
            tooltip_settings: TooltipSettings::default(),
            tooltip_animation: None,
            visual_damage_scratch: Vec::new(),
            effective_damage_scratch: Vec::new(),
            cached_restyle_rules: None,
            cached_style_rule_index: None,
            element_metric_usage,
            keyboard_settings_cache: RefCell::new(None),
            resolved_surface_shortcuts_cache: RefCell::new(None),
        }
    }

    pub(super) fn with_graph_i18n_catalogs(
        mut self,
        graph_i18n_catalogs: Vec<(String, String, PathBuf)>,
    ) -> Self {
        self.graph_i18n_catalogs = graph_i18n_catalogs;
        self
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

    /// Narrow script invalidation — signals that only leaf-level values may
    /// have changed (no structural changes: no added/removed nodes, no
    /// conditional-branch flips). The paint path will call `narrow_script_update()`,
    /// which rebuilds the tree, diffs against the prior retained snapshot, and
    /// falls back to a full TREE_REBUILD if any structural change is detected or
    /// if more than 50% of nodes are affected.
    pub(super) fn invalidate_script_state_narrow(&mut self) {
        self.surface_pixels_invalid = true;
        self.invalidate(ComponentDirtyFlags::SCRIPT_NARROW);
    }

    pub(super) fn invalidate_interaction_restyle(&mut self) {
        self.invalidate_style_path(ComponentDirtyFlags::INTERACTION_RESTYLE);
    }

    pub(super) fn invalidate_hover_change(&mut self, tooltip_may_change: bool) {
        if self.module_styles_have_state_rules() {
            self.invalidate_interaction_restyle();
        } else if tooltip_may_change {
            self.invalidate_paint();
        }
    }

    pub(super) fn invalidate_text_state(&mut self) {
        self.invalidate(ComponentDirtyFlags::TEXT_RELAYOUT);
    }

    pub(super) fn invalidate_paint(&mut self) {
        self.invalidate_style_path(ComponentDirtyFlags::PAINT);
    }

    pub(super) fn invalidate_surface_config(&mut self) {
        self.invalidate_style_path(ComponentDirtyFlags::SURFACE_CONFIG);
    }

    pub(super) fn should_update_surface_config_on_render(&self) -> bool {
        self.dirty_types
            .contains(ComponentDirtyFlags::SURFACE_CONFIG)
            || (self.dirty_types.is_empty() && (self.dirty || self.style_only_dirty))
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

fn compiled_module_has_animatable_style_rules(compiled: &CompiledFrontendModule) -> bool {
    component_has_animatable_style_rules(&compiled.component)
        || compiled
            .local_components
            .values()
            .any(component_has_animatable_style_rules)
}

fn component_has_animatable_style_rules(component: &mesh_core_component::ComponentFile) -> bool {
    let Some(style) = &component.style else {
        return false;
    };
    !style.keyframes.is_empty()
        || style.rules.iter().any(|rule| {
            rule.declarations.iter().any(|declaration| {
                declaration.property == "transition"
                    || declaration.property.starts_with("transition-")
                    || declaration.property == "animation"
                    || declaration.property.starts_with("animation-")
            })
        })
}

#[cfg(test)]
mod animation_rule_detection_tests {
    use super::*;

    fn component(source: &str) -> mesh_core_component::ComponentFile {
        mesh_core_component::parse_component(source).expect("component parses")
    }

    #[test]
    fn detects_animatable_style_rules_from_declarations_and_keyframes() {
        let plain = component(
            r#"
<template><box class="panel" /></template>
<style>.panel { color: #fff; }</style>
"#,
        );
        assert!(!component_has_animatable_style_rules(&plain));

        let transition = component(
            r#"
<template><box class="panel" /></template>
<style>.panel { transition: opacity 120ms ease; }</style>
"#,
        );
        assert!(component_has_animatable_style_rules(&transition));

        let animation = component(
            r#"
<template><box class="panel" /></template>
<style>.panel { animation-name: pulse; }</style>
"#,
        );
        assert!(component_has_animatable_style_rules(&animation));

        let keyframes = component(
            r#"
<template><box class="panel" /></template>
<style>
@keyframes pulse {
  0% { opacity: 0; }
  100% { opacity: 1; }
}
</style>
"#,
        );
        assert!(component_has_animatable_style_rules(&keyframes));
    }
}

fn invalidation_requires_pixel_repaint(flags: ComponentDirtyFlags) -> bool {
    // Accessibility and metrics changes update metadata/measurements but do not
    // change the rendered pixels, so they are excluded from the repaint gate.
    flags.intersects(
        ComponentDirtyFlags::STATE
            | ComponentDirtyFlags::STYLE
            | ComponentDirtyFlags::LAYOUT
            | ComponentDirtyFlags::PAINT
            | ComponentDirtyFlags::TEXT,
    )
}

#[cfg(test)]
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

/// Which element metric host tables a module can observe. Substring matches over
/// raw script/expression sources are intentionally conservative: a false
/// positive only re-enables publication, never breaks a consumer.
fn element_metric_usage(compiled: &CompiledFrontendModule) -> ElementMetricUsage {
    let mut usage =
        component_element_metric_usage(&compiled.component.script, &compiled.component.template);
    for component in compiled.local_components.values() {
        let component_usage =
            component_element_metric_usage(&component.script, &component.template);
        usage.elements |= component_usage.elements;
        usage.refs |= component_usage.refs;
    }
    usage
}

fn component_element_metric_usage(
    script: &Option<mesh_core_component::ScriptBlock>,
    template: &Option<mesh_core_component::template::TemplateBlock>,
) -> ElementMetricUsage {
    let mut usage = ElementMetricUsage::default();
    if let Some(script) = script {
        usage.elements |= source_uses_element_metrics_table(&script.source, "elements");
        usage.refs |= source_uses_element_metrics_table(&script.source, "refs");
    }
    let template_usage = template_element_metric_usage(template);
    usage.elements |= template_usage.elements;
    usage.refs |= template_usage.refs;
    usage.refs |= template_declares_element_refs(template) || template_uses_bind_this(template);
    usage
}

fn source_uses_element_metrics_table(source: &str, table: &str) -> bool {
    match table {
        "elements" => source.contains("elements.") || source.contains("elements["),
        "refs" => source.contains("refs.") || source.contains("refs["),
        _ => false,
    }
}

fn template_element_metric_usage(
    template: &Option<mesh_core_component::template::TemplateBlock>,
) -> ElementMetricUsage {
    template
        .as_ref()
        .map_or_else(ElementMetricUsage::default, |template| {
            nodes_element_metric_usage(&template.root)
        })
}

fn nodes_element_metric_usage(nodes: &[TemplateNode]) -> ElementMetricUsage {
    let mut usage = ElementMetricUsage::default();
    for node in nodes {
        let node_usage = match node {
            TemplateNode::Element(element) => {
                let mut usage = attributes_element_metric_usage(&element.attributes);
                let child_usage = nodes_element_metric_usage(&element.children);
                usage.elements |= child_usage.elements;
                usage.refs |= child_usage.refs;
                usage
            }
            TemplateNode::Component(component) => {
                let mut usage = attributes_element_metric_usage(&component.props);
                let child_usage = nodes_element_metric_usage(&component.children);
                usage.elements |= child_usage.elements;
                usage.refs |= child_usage.refs;
                usage
            }
            TemplateNode::If(if_node) => {
                let mut usage = nodes_element_metric_usage(&if_node.then_children);
                let else_usage = nodes_element_metric_usage(&if_node.else_children);
                usage.elements |= else_usage.elements;
                usage.refs |= else_usage.refs;
                usage
            }
            TemplateNode::For(for_node) => nodes_element_metric_usage(&for_node.children),
            TemplateNode::Text(text) => string_element_metric_usage(&text.content),
            TemplateNode::Expr(expr) => string_element_metric_usage(&expr.expression),
            TemplateNode::Slot(_) => ElementMetricUsage::default(),
        };
        usage.elements |= node_usage.elements;
        usage.refs |= node_usage.refs;
    }
    usage
}

fn attributes_element_metric_usage(
    attributes: &[mesh_core_component::template::Attribute],
) -> ElementMetricUsage {
    let mut usage = ElementMetricUsage::default();
    for attribute in attributes {
        let attribute_usage = attribute_value_element_metric_usage(&attribute.value);
        usage.elements |= attribute_usage.elements;
        usage.refs |= attribute_usage.refs;
    }
    usage
}

fn attribute_value_element_metric_usage(value: &AttributeValue) -> ElementMetricUsage {
    match value {
        AttributeValue::Static(value)
        | AttributeValue::Binding(value)
        | AttributeValue::TwoWayBinding(value)
        | AttributeValue::InstanceBinding(value)
        | AttributeValue::EventHandler(value) => string_element_metric_usage(value),
        AttributeValue::EventHandlerCall { handler, args } => {
            let mut usage = string_element_metric_usage(handler);
            for arg in args {
                let arg_usage = string_element_metric_usage(arg);
                usage.elements |= arg_usage.elements;
                usage.refs |= arg_usage.refs;
            }
            usage
        }
    }
}

fn string_element_metric_usage(value: &str) -> ElementMetricUsage {
    ElementMetricUsage {
        elements: source_uses_element_metrics_table(value, "elements"),
        refs: source_uses_element_metrics_table(value, "refs"),
    }
}

fn template_declares_element_refs(
    template: &Option<mesh_core_component::template::TemplateBlock>,
) -> bool {
    template
        .as_ref()
        .is_some_and(|template| nodes_declare_element_refs(&template.root))
}

fn nodes_declare_element_refs(nodes: &[TemplateNode]) -> bool {
    nodes.iter().any(|node| match node {
        TemplateNode::Element(element) => {
            element
                .attributes
                .iter()
                .any(|attribute| matches!(attribute.name.as_str(), "ref" | "id"))
                || nodes_declare_element_refs(&element.children)
        }
        TemplateNode::Component(component) => {
            component
                .props
                .iter()
                .any(|attribute| matches!(attribute.name.as_str(), "ref" | "id"))
                || nodes_declare_element_refs(&component.children)
        }
        TemplateNode::If(if_node) => {
            nodes_declare_element_refs(&if_node.then_children)
                || nodes_declare_element_refs(&if_node.else_children)
        }
        TemplateNode::For(for_node) => nodes_declare_element_refs(&for_node.children),
        TemplateNode::Slot(_) | TemplateNode::Text(_) | TemplateNode::Expr(_) => false,
    })
}

fn template_uses_bind_this(
    template: &Option<mesh_core_component::template::TemplateBlock>,
) -> bool {
    template
        .as_ref()
        .is_some_and(|template| nodes_use_bind_this(&template.root))
}

fn nodes_use_bind_this(nodes: &[TemplateNode]) -> bool {
    nodes.iter().any(|node| match node {
        TemplateNode::Element(element) => {
            element
                .attributes
                .iter()
                .any(|attribute| matches!(attribute.value, AttributeValue::InstanceBinding(_)))
                || nodes_use_bind_this(&element.children)
        }
        TemplateNode::Component(component) => {
            component
                .props
                .iter()
                .any(|attribute| matches!(attribute.value, AttributeValue::InstanceBinding(_)))
                || nodes_use_bind_this(&component.children)
        }
        TemplateNode::If(if_node) => {
            nodes_use_bind_this(&if_node.then_children)
                || nodes_use_bind_this(&if_node.else_children)
        }
        TemplateNode::For(for_node) => nodes_use_bind_this(&for_node.children),
        TemplateNode::Slot(_) | TemplateNode::Text(_) | TemplateNode::Expr(_) => false,
    })
}

fn service_payload_cache_capacity(manifest: &mesh_core_module::Manifest) -> usize {
    manifest
        .capabilities
        .required
        .iter()
        .chain(manifest.capabilities.optional.iter())
        .filter(|capability| capability_caches_service_payload(capability))
        .count()
}

fn capability_caches_service_payload(capability: &str) -> bool {
    capability == "theme.read"
        || capability == "locale.read"
        || capability
            .strip_prefix("service.")
            .and_then(|capability| capability.strip_suffix(".read"))
            .is_some_and(|service| !service.is_empty())
}

pub(super) fn json_field_diff(
    service: &str,
    previous: &serde_json::Value,
    next: &serde_json::Value,
) -> Vec<(String, String)> {
    let mut changed = Vec::new();
    let prev_obj = match previous.as_object() {
        Some(o) => o,
        None => return changed,
    };
    let next_obj = match next.as_object() {
        Some(o) => o,
        None => return changed,
    };
    for (key, val) in next_obj {
        match prev_obj.get(key) {
            Some(prev_val) if prev_val == val => {}
            _ => {
                changed.push((service.to_string(), key.clone()));
            }
        }
    }
    for key in prev_obj.keys() {
        if !next_obj.contains_key(key) {
            changed.push((service.to_string(), key.clone()));
        }
    }
    changed
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
