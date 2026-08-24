use super::service::{
    ServiceCapabilities, apply_service_update_with_name,
    apply_service_update_with_name_and_fingerprint, script_events_to_requests,
    seed_service_context, service_capabilities,
};
use super::surface_layout::{SurfaceLayoutSettings, resolve_frontend_module_settings_with_props};
use super::types::{
    ChildSurfaceDiagnostic, ChildSurfaceKind, ChildSurfaceRequest, ComponentContext,
    ComponentError, ComponentInput, ComponentProfilingRecord, CoreEvent, CoreRequest, KeyModifiers,
    ServiceEvent, ShellComponent, SurfaceExtent, TabFocusTarget,
};
use mesh_core_config::SettingsStore;
use mesh_core_interaction::{
    GestureKind, InteractionDelta, InteractionFrame, InteractionState, ScrollbarAxis,
    collect_focus_traversal, find_click_handler, find_event_handler, find_node_bounds_by_key,
    find_node_by_key, find_node_path_at, find_node_with_bounds_by_key, find_nodes_by_keys,
    find_scrollable_at_with_limits, find_scrollbar_at, is_input_key, is_slider_key,
    measure_content_size, next_focus_target, node_can_receive_target as tree_target_is_eligible,
    node_is_source, pointer_event_handler_hit, pointer_press_hit, scroll_into_view_offsets,
    scroll_limits, source_element_tag,
};
mod animation;
mod catalog;
mod composition;
mod diagnostics;
mod input;
mod interaction_state;
mod memo;
mod rendering;
mod runtime;
mod runtime_tree;
mod shell_component;
mod tooltip;

pub(in crate::shell) use catalog::{
    FrontendCatalog, FrontendCatalogHandle, SharedCompiledFrontendModule,
};
#[cfg(test)]
pub(crate) use input::KeybindResolutionSource;
use input::ResolvedSurfaceShortcut;
use mesh_core_animation::{AnimationInstanceId, MotionPolicy, transition::TransitionAnimator};
pub(in crate::shell) use mesh_core_interaction::ScrollOffsetState;
#[cfg(test)]
use runtime_tree::stable_runtime_node_id;
use runtime_tree::{
    NodeServiceFieldDependencies, RetainedWidgetTree, RuntimeAnnotationContext,
    annotate_runtime_and_overflow_tree, collect_element_metrics, input_accepts_char,
    runtime_node_id_for_key,
};

use mesh_core_capability::{Capability, CapabilitySet, EffectiveCapabilities};
use mesh_core_component::template::{AttributeValue, TemplateNode};
use mesh_core_config::TooltipSettings;
use mesh_core_diagnostics::Diagnostics;
use mesh_core_elements::{
    FramePhaseStamps, FrameSnapshot, HandlerTarget, InteractionTarget, IntrinsicLayoutCache,
    LayoutEngine, NodeId, PerSurfaceLayoutState, PopoverPlacement, StyleContext, StyleResolver,
    VariableStore, WidgetNode, WindowSurfaceState, element_snapshot_json,
};
use mesh_core_frontend::{CompiledFrontendModule, FrontendRenderMode, root_accessibility_role};
use mesh_core_locale::LocaleEngine;
use mesh_core_scripting::{
    LocaleBoundState, OperationRegistry, PublishedEvent, ScriptContext, ScriptInterfaceImport,
    ScriptState, SurfaceVm,
};
use mesh_core_theme::{Theme, default_theme};
use mesh_core_wayland::{Edge, KeyboardMode, ShellSurface, WindowStates};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(super) type SurfaceCssProps = HashMap<String, mesh_core_component::style::StyleValue>;

pub(super) fn find_node_by_id(node: &WidgetNode, node_id: NodeId) -> Option<&WidgetNode> {
    if node.id == node_id {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_node_by_id(child, node_id))
}

pub(super) fn node_can_receive_interaction(
    tree: &WidgetNode,
    node_key: &str,
    interaction: InteractionTarget,
) -> bool {
    find_node_by_key(tree, node_key)
        .is_some_and(|node| tree_target_is_eligible(tree, node.id, interaction))
}

pub(super) fn node_can_receive_activation(tree: &WidgetNode, node_key: &str) -> bool {
    node_can_receive_interaction(tree, node_key, InteractionTarget::Pointer)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct StyleStateDependencies {
    mask: u32,
}

impl StyleStateDependencies {
    fn contains(self, state: mesh_core_elements::PseudoState) -> bool {
        self.mask & state.spec().bit != 0
    }

    fn contains_any<const N: usize>(self, states: [mesh_core_elements::PseudoState; N]) -> bool {
        states.into_iter().any(|state| self.contains(state))
    }
}

#[derive(Clone)]
struct CachedServicePayload {
    value: Arc<serde_json::Value>,
    fingerprint: u64,
}

fn update_cached_service_payload(
    cache: &mut HashMap<String, CachedServicePayload>,
    service_name: &str,
    payload: &serde_json::Value,
    fingerprint: u64,
) -> Option<Arc<serde_json::Value>> {
    if let Some(cached) = cache.get_mut(service_name) {
        let previous = Arc::clone(&cached.value);
        if cached.fingerprint != fingerprint {
            cached.value = Arc::new(payload.clone());
            cached.fingerprint = fingerprint;
        }
        return Some(previous);
    }

    cache.insert(
        service_name.to_owned(),
        CachedServicePayload {
            value: Arc::new(payload.clone()),
            fingerprint,
        },
    );
    None
}

fn cached_service_capabilities(
    cache: &mut HashMap<String, Arc<ServiceCapabilities>>,
    interface: &str,
) -> Arc<ServiceCapabilities> {
    if let Some(capabilities) = cache.get(interface) {
        return Arc::clone(capabilities);
    }

    let capabilities = service_capabilities(interface);
    cache.insert(interface.to_owned(), Arc::clone(&capabilities));
    capabilities
}

fn update_last_service_trace(
    summary: &mut Option<String>,
    service: &str,
    source_module: &str,
    debug_enabled: bool,
) {
    if !debug_enabled {
        summary.take();
        return;
    }

    let mut value = String::with_capacity(service.len() + source_module.len() + 1);
    value.push_str(service);
    value.push(':');
    value.push_str(source_module);
    *summary = Some(value);
}

#[derive(Default)]
struct InstanceKeyInterner {
    keys: HashSet<Arc<str>>,
    scratch: String,
}

const MAX_RETAINED_CHILD_DISPLAY_LISTS: usize = 64;

#[derive(Debug)]
struct ChildDisplayListCacheEntry {
    display_list: RetainedDisplayList,
    last_used: u64,
}

/// Bounded popup-local display-list cache.
///
/// Cache misses are uncommon, so hits only update a timestamp while misses
/// scan the bounded set for the least-recently-used entry. This avoids the
/// previous flush-all cliff, where adding one popup at the cap discarded every
/// still-hot popup display list.
#[derive(Debug)]
struct ChildDisplayListCache {
    entries: HashMap<NodeId, ChildDisplayListCacheEntry>,
    clock: u64,
}

impl Default for ChildDisplayListCache {
    fn default() -> Self {
        Self {
            entries: HashMap::with_capacity(MAX_RETAINED_CHILD_DISPLAY_LISTS),
            clock: 0,
        }
    }
}

impl ChildDisplayListCache {
    fn clear(&mut self) {
        self.entries.clear();
        self.clock = 0;
    }

    fn get(&self, node_id: NodeId) -> Option<&RetainedDisplayList> {
        self.entries.get(&node_id).map(|entry| &entry.display_list)
    }

    fn get_or_insert(&mut self, node_id: NodeId) -> &mut RetainedDisplayList {
        self.clock = self.clock.saturating_add(1);
        if self.clock == u64::MAX {
            for entry in self.entries.values_mut() {
                entry.last_used = 0;
            }
            self.clock = 1;
        }
        let last_used = self.clock;

        if self.entries.contains_key(&node_id) {
            let entry = self
                .entries
                .get_mut(&node_id)
                .expect("child display-list entry checked above");
            entry.last_used = last_used;
            return &mut entry.display_list;
        }

        if self.entries.len() >= MAX_RETAINED_CHILD_DISPLAY_LISTS
            && let Some(evicted) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(node_id, _)| *node_id)
        {
            self.entries.remove(&evicted);
        }
        &mut self
            .entries
            .entry(node_id)
            .or_insert_with(|| ChildDisplayListCacheEntry {
                display_list: RetainedDisplayList::default(),
                last_used,
            })
            .display_list
    }
}

impl InstanceKeyInterner {
    fn intern(&mut self, key: &str) -> Arc<str> {
        if let Some(key) = self.keys.get(key) {
            return Arc::clone(key);
        }
        let key: Arc<str> = Arc::from(key);
        self.keys.insert(Arc::clone(&key));
        key
    }

    fn intern_embedded(&mut self, host: &str, kind: &str, identifier: &str) -> Arc<str> {
        self.intern_embedded_occurrence(host, kind, identifier, None, None, None)
    }

    fn intern_embedded_occurrence(
        &mut self,
        host: &str,
        kind: &str,
        identifier: &str,
        duplicate_ordinal: Option<usize>,
        loop_ordinal: Option<usize>,
        loop_identity: Option<&str>,
    ) -> Arc<str> {
        self.scratch.clear();
        self.scratch
            .reserve(host.len() + 1 + kind.len() + 1 + identifier.len() + 4);
        self.scratch.push_str(host);
        self.scratch.push('/');
        self.scratch.push_str(kind);
        self.scratch.push(':');
        self.scratch.push_str(identifier);
        if let Some(ordinal) = duplicate_ordinal {
            use std::fmt::Write;
            write!(&mut self.scratch, "#{ordinal}").expect("writing to a String cannot fail");
        }
        if let Some(ordinal) = loop_ordinal {
            use std::fmt::Write;
            write!(&mut self.scratch, "@{ordinal}").expect("writing to a String cannot fail");
        }
        if let Some(identity) = loop_identity {
            self.scratch.push_str("@key:");
            self.scratch.push_str(identity);
        }
        if let Some(key) = self.keys.get(self.scratch.as_str()) {
            return Arc::clone(key);
        }
        let key: Arc<str> = Arc::from(self.scratch.as_str());
        self.keys.insert(Arc::clone(&key));
        key
    }

    fn intern_slot(&mut self, host: &str, slot: &str, contribution: &str) -> Arc<str> {
        self.scratch.clear();
        self.scratch
            .reserve(host.len() + "/slot:".len() + slot.len() + 1 + contribution.len());
        self.scratch.push_str(host);
        self.scratch.push_str("/slot:");
        self.scratch.push_str(slot);
        self.scratch.push('/');
        self.scratch.push_str(contribution);
        if let Some(key) = self.keys.get(self.scratch.as_str()) {
            return Arc::clone(key);
        }
        let key: Arc<str> = Arc::from(self.scratch.as_str());
        self.keys.insert(Arc::clone(&key));
        key
    }
}

use mesh_core_render::{
    DamageRect, DisplayListMetrics, DisplayListRepaintPolicy, DisplayPaintCommand, PixelBuffer,
    RetainedDisplayList, SharedTextMeasurer, TextCacheMetrics, TextRenderer,
};

const TOOLTIP_OVERLAY_WIDTH: u32 = 352;
const TOOLTIP_OVERLAY_HEIGHT: u32 = 200;

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
        /// Script change eligible for retained diffing and damage tracking.
        /// The template is still evaluated so structural changes remain safe,
        /// but the authoritative retained diff avoids forcing a full-surface
        /// repaint for ordinary bound-value changes.
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

#[derive(Debug, Clone, Copy)]
pub(super) struct ScrollbarDragState {
    pub(super) node_id: NodeId,
    pub(super) axis: ScrollbarAxis,
    pub(super) grab_offset: f32,
    pub(super) track_start: f32,
    pub(super) track_extent: f32,
    pub(super) thumb_extent: f32,
    pub(super) max_scroll: f32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ScrollInertia {
    pub(super) velocity: ScrollOffsetState,
    pub(super) samples: u8,
    pub(super) travel: f32,
    pub(super) last_input: Instant,
    pub(super) last_tick: Instant,
    pub(super) max_x: f32,
    pub(super) max_y: f32,
}

#[derive(Debug, Clone)]
struct ScheduledHandler {
    target: HandlerTarget,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeStyleDiagnosticFingerprint {
    rules_generation: u64,
    retained_tree_generation: u64,
    props: u64,
    container_width: u32,
    container_height: u32,
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

/// Transient IME composition for an input. The committed value remains in
/// `input_values`; this state is only projected into the rendered input value
/// until the compositor sends the next text-input transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TextPreeditState {
    pub(super) text: String,
    pub(super) cursor_begin: i32,
    pub(super) cursor_end: i32,
    pub(super) insert_at: usize,
}

#[derive(Debug)]
pub(super) struct GestureTargetCapture {
    pub(super) node_key: String,
    pub(super) fingers: u32,
    pub(super) started_at: Instant,
    pub(super) pointer: (f32, f32),
}

#[derive(Debug)]
pub(super) enum GestureCapture {
    Swipe {
        target: GestureTargetCapture,
        dx: f32,
        dy: f32,
    },
    Pinch {
        target: GestureTargetCapture,
        dx: f32,
        dy: f32,
        scale: f32,
        rotation: f32,
    },
    Hold {
        target: GestureTargetCapture,
    },
}

#[derive(Debug)]
pub(super) struct TouchGestureCapture {
    pub(super) node_key: String,
    pub(super) started_at: Instant,
    pub(super) origin: (f32, f32),
    pub(super) point: (f32, f32),
    pub(super) eligible: bool,
    pub(super) long_press_enabled: bool,
    pub(super) long_press_fired: bool,
}

#[derive(Debug)]
pub(super) struct TapRecord {
    pub(super) node_key: String,
    pub(super) at: Instant,
    pub(super) point: (f32, f32),
}

pub(super) struct FrontendSurfaceComponent {
    /// Runtime instance identity. Defaults to the module's declared surface id
    /// and is replaced by a profile root key for composed instances.
    surface_id: String,
    /// Shared immutable frontend source. Multiple profile roots of the same
    /// module must not duplicate the compiled template, scripts, or styles.
    pub(super) compiled: SharedCompiledFrontendModule,
    pub(super) module_dir: PathBuf,
    /// The one settings store, shared with the shell and every sibling
    /// component. Swapped wholesale when the file changes.
    settings: Arc<SettingsStore>,
    /// This component's namespace in the store — the module id today, an
    /// instance key (`@scope/name#instance`) once profiles land.
    settings_namespace: String,
    /// This component's resolved overrides: `settings.namespace(&namespace)`.
    /// Cached because it is handed to Luau on every runtime creation.
    settings_json: serde_json::Value,
    /// Reduced-motion preference captured as an immutable decision for the
    /// current animation/scroll scheduling pass.
    motion_policy: MotionPolicy,
    /// What the last resolution of this namespace rejected. Kept so a live
    /// reload can report only what is new: the file is re-validated on every
    /// save, and a user fixing one of five mistakes should hear about four.
    settings_diagnostics: Vec<mesh_core_config::SettingsDiagnostic>,
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
    frontend_catalog_handle: FrontendCatalogHandle,
    frontend_catalog_version: u64,
    /// Activation-resolved grants keyed by owning module id. Embedded
    /// components use their own entry rather than inheriting the host's
    /// capabilities.
    effective_capabilities: Arc<HashMap<String, EffectiveCapabilities>>,
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
    /// Retained node keys whose embedded widget is currently realized as an
    /// independent `xdg_toplevel` child surface. The node stays in this
    /// component's tree so its shared surface VM and state remain live.
    promoted_window_keys: HashSet<String>,
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
    cached_service_capabilities: HashMap<String, Arc<ServiceCapabilities>>,
    cached_service_payloads: HashMap<String, CachedServicePayload>,
    /// Service names declared by this surface's manifest (and the manifests of
    /// the component modules it embeds). Payloads for these are cached even
    /// while no live runtime reads them, so lazily created child runtimes are
    /// seeded with real state instead of an empty service proxy.
    declared_service_names: HashSet<String>,
    /// Readable key cache used only at event/ref navigation boundaries.
    focused_key: Option<String>,
    focus_visible_key: Option<String>,
    /// Authoritative focus identities for runtime annotation and restyling.
    focused_id: Option<NodeId>,
    focus_visible_id: Option<NodeId>,
    /// Canonical staged ownership for focus, pointer capture, press origin,
    /// gestures, scrolling, and their typed invalidation output. The legacy
    /// key/id fields beside it are lookup caches for script and render paths.
    pub(super) interaction_state: InteractionState,
    /// Renderer-neutral phase contract shared by input, state, style,
    /// layout, animation, paint, and semantic publication.
    pub(super) interaction_frame: InteractionFrame,
    pointer_down_id: Option<NodeId>,
    pointer_down_bounds: Option<(f32, f32, f32, f32)>,
    pointer_down_target: Option<input::PressedTargetSnapshot>,
    active_slider_id: Option<NodeId>,
    gesture_capture: Option<GestureCapture>,
    touch_targets: HashMap<i32, String>,
    active_touches: HashMap<i32, (f32, f32)>,
    touch_gestures: HashMap<i32, TouchGestureCapture>,
    last_tap: Option<TapRecord>,
    keyboard_button_press_activations: HashSet<(NodeId, String)>,
    /// When a surface with keyboard interactivity transitions visible→true,
    /// this flag tells the next paint to seed focus on the first tabbable
    /// element. Lets a popover work with keyboard immediately after opening
    /// without the user needing to click inside it first.
    pending_auto_focus: bool,
    /// Keyboard activation of an `aria-haspopup` button may reveal an embedded
    /// `<popover>` inside this same component. On the following paint, focus
    /// the first control in that promoted subtree.
    pending_embedded_popover_focus: bool,
    /// Trigger key to restore after Escape closes an embedded popover.
    embedded_popover_return_focus: Option<String>,
    /// Set by Escape and consumed after the close state reaches the next tree.
    pending_embedded_popover_focus_restore: bool,
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
    input_values: HashMap<NodeId, String>,
    input_preedits: HashMap<NodeId, TextPreeditState>,
    /// UTF-8 byte cursor positions for editable inputs. A missing entry is
    /// initialized to the value's end, preserving the historic append-only
    /// behavior until an editor operation moves the cursor.
    input_cursors: HashMap<NodeId, usize>,
    slider_values: HashMap<NodeId, f32>,
    slider_script_values: HashMap<NodeId, f32>,
    checked_values: HashMap<NodeId, bool>,
    render_hooks_pending: bool,
    /// Whether the render hooks run for the frame currently being painted
    /// wrote script state that a template expression actually reads. A render
    /// hook is only observable through that state, so a hook that ran without
    /// touching any template input leaves the retained and selective build
    /// paths valid for this frame.
    render_hooks_changed_templates: bool,
    pub(super) scroll_offsets: HashMap<NodeId, ScrollOffsetState>,
    scheduled_handlers: HashMap<String, ScheduledHandler>,
    /// In-flight smooth-scroll animations keyed by scroll-container node ID.
    /// Ticked at the top of `finalize_tree`; each writes an eased offset into
    /// `scroll_offsets` until it settles, then is dropped. Started by
    /// `refs.x:scroll_to(.., { smooth = true })` / `:scroll_into_view({ smooth })`.
    pub(super) scroll_animations: HashMap<NodeId, ScrollAnimation>,
    /// Pixel-precise touchpad motion that continues briefly after a finger
    /// lift, decaying until it settles or reaches a scroll boundary.
    pub(super) scroll_inertia: HashMap<NodeId, ScrollInertia>,
    /// Pointer capture for dragging a painted scrollbar thumb.
    pub(super) active_scrollbar_drag: Option<ScrollbarDragState>,
    // Hover tracking for CSS :hover and tooltip system.
    hovered_key: Option<String>,
    hovered_path: Vec<NodeId>,
    /// Structural paths retained only to dispatch pointer enter/leave handlers.
    /// The runtime style/restyle path above is keyed by `NodeId`.
    hovered_event_path: Vec<String>,
    hovered_tooltip: Option<(String, String)>,
    /// Previous frame's hovered path — used to detect which nodes' hover state
    /// changed between frames for targeted interaction restyle.
    previous_hovered_path: Vec<NodeId>,
    /// Previous frame's focused key — used to detect which node's focus state
    /// changed between frames for targeted interaction restyle.
    previous_focused_key: Option<NodeId>,
    previous_focus_visible_key: Option<NodeId>,
    /// Previous interaction states whose pseudo-classes can change without a
    /// template rebuild. Together with hover/focus these make targeted
    /// interaction restyle complete for every supported dynamic pseudo-state.
    previous_active_key: Option<NodeId>,
    previous_checked_values: HashMap<NodeId, bool>,
    /// Previous slider positions. Unlike the states above these drive no
    /// pseudo-class — they change the node's *painted content*. A drag that
    /// moves only the knob leaves every pseudo-state untouched, so without
    /// this the targeted restyle finds nothing changed and the frame is
    /// skipped: the knob stops following the pointer.
    previous_slider_values: HashMap<NodeId, f32>,
    interaction_snapshot_valid: bool,
    hovered_pos: (f32, f32),
    hover_start: Option<std::time::Instant>,
    tooltip_visible: bool,
    /// Bounding box of the currently hovered element: (left, top, right, bottom).
    hovered_element_bounds: Option<(f32, f32, f32, f32)>,
    /// Fully resolved tooltip render inputs for stable paint-only/fade frames.
    /// Retained generation and hovered key jointly guard every borrowed tree
    /// fact copied into this cache.
    tooltip_target_cache: mesh_core_interaction::TooltipTargetCache,
    /// Timestamp when the current tooltip became visible (for fade-in animation timing).
    tooltip_appeared_at: Option<std::time::Instant>,
    last_tooltip_damage: Option<DamageRect>,
    runtimes: Arc<Mutex<HashMap<Arc<str>, EmbeddedFrontendRuntime>>>,
    instance_keys: RefCell<InstanceKeyInterner>,
    composition_occurrences: RefCell<HashMap<(Arc<str>, usize, Option<Arc<str>>), usize>>,
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
    /// Per axis: this paint has no size for the surface, only a stand-in, so
    /// the synthetic surface root is laid out `auto` on that axis and the
    /// content measures itself. Written by `paint`, read by `finalize_tree`.
    unmeasured_root_axes: (bool, bool),
    last_surface_size: Option<(u32, u32)>,
    /// The compositor's toplevel states for this surface, projected onto every
    /// node at annotation time so `:fullscreen`, `:maximized`, `:activated`,
    /// and `:tiled` can be styled. Stays default for layer surfaces.
    window_states: WindowSurfaceState,
    last_painted_buffer_size: Option<(u32, u32)>,
    surface_pixels_invalid: bool,
    locale: LocaleEngine,
    /// Shell-created components share the coordinator's immutable catalog
    /// snapshot. Standalone component fixtures keep their local source-backed
    /// catalog for authoring and integration tests.
    locale_catalog_is_shared: bool,
    interface_catalog: Arc<mesh_core_service::InterfaceCatalog>,
    last_tree: Option<WidgetNode>,
    /// Immutable cross-phase hand-off produced after layout and semantic
    /// normalization. The mutable `last_tree` remains the input-side working
    /// copy; consumers that need a coherent frame use this snapshot instead.
    last_frame_snapshot: Option<FrameSnapshot>,
    frame_revision: u64,
    intrinsic_layout_cache: IntrinsicLayoutCache,
    layout_state: PerSurfaceLayoutState,
    pub(in crate::shell::component) retained_tree: RetainedWidgetTree,
    /// Authoritative roots touched by the latest targeted retained restyle.
    /// Consumed by the retained-tree fingerprint pass in the same paint.
    retained_update_dirty_roots: Option<HashSet<NodeId>>,
    #[cfg(test)]
    force_full_retained_update: bool,
    /// True only when the pending style frame was requested exclusively by
    /// the animation pass. Any unrelated invalidation clears this marker.
    animation_only_dirty: bool,
    node_service_field_deps: NodeServiceFieldDependencies,
    /// Template nodes whose tracked service fields changed since the last
    /// paint. `None` means a narrow invalidation without an authoritative
    /// service-node scope and therefore requires normal template evaluation.
    pending_service_template_nodes: Option<HashSet<NodeId>>,
    selective_service_build_supported: bool,
    #[cfg(test)]
    last_template_build_reused_nodes: usize,
    #[cfg(test)]
    force_full_template_build: bool,
    retained_display_list: RetainedDisplayList,
    /// Popup-local display lists keyed by the promoted subtree's stable node id.
    /// These retain the expensive tree-to-paint-command lowering between child
    /// rasters; the shell's per-child generation cache decides when raster is
    /// needed, while this cache makes that raster a command replay.
    child_display_lists: RefCell<ChildDisplayListCache>,
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
    portal_hidden_bindings: RefCell<HashMap<String, (Arc<str>, String)>>,
    /// `parent_instance_key -> [(binding, child_instance_key)]` for live
    /// `bind:this` references. After a parent event handler runs, each linked
    /// child is re-synced so values its parent mutated through the live proxy
    /// re-render. Refreshed every render by `bind_child_instance`.
    bound_children: RefCell<HashMap<Arc<str>, Vec<(String, Arc<str>)>>>,
    /// `refs.<name>` -> live widget node key, rebuilt every paint by
    /// `publish_element_metrics`. Lets imperative element actions
    /// (`refs.<name>:focus()`) resolve a script-facing ref name back to the
    /// retained node it targets.
    ref_node_keys: RefCell<HashMap<String, String>>,
    transitions: TransitionAnimator,
    keyframe_animations:
        HashMap<AnimationInstanceId, mesh_core_animation::keyframes::ActiveKeyframeAnimation>,
    keyframe_rules: HashMap<AnimationInstanceId, mesh_core_animation::keyframes::KeyframeRule>,
    /// The currently active instance in each node/list slot. Keeping the slot
    /// separate from the declaration fingerprint makes replacement and
    /// cancellation explicit when a rule changes or disappears.
    keyframe_animation_slots: HashMap<(NodeId, u32), AnimationInstanceId>,
    /// Last reconciliation decision for each live declaration slot. This keeps
    /// replacement and cancellation observable at the shell boundary without
    /// making renderers infer lifecycle from map mutations.
    keyframe_animation_lifecycles: HashMap<(NodeId, u32), mesh_core_animation::AnimationLifecycle>,
    previous_visual_styles_scratch:
        HashMap<NodeId, mesh_core_animation::transition::AnimatableStyle>,
    /// Per-animation-pass key sets. Retain their hash-table allocations across
    /// ticks because the same surface is usually traversed every frame.
    animation_live_keys_scratch: HashSet<NodeId>,
    animation_live_keyframe_keys_scratch: HashSet<AnimationInstanceId>,
    animation_dirty_node_ids_scratch: HashSet<NodeId>,
    has_animatable_style_rules: bool,
    has_active_keyframe_animation: bool,
    has_promoted_popover_wrappers: Cell<bool>,
    has_error_placeholders: Cell<bool>,
    /// Memoized built subtrees for embedded/local component instances, keyed
    /// by instance key. An entry is reused wholesale on rebuild when the
    /// instance's props, script-state generations (own + descendants), theme,
    /// locale, and container size are unchanged — skipping template
    /// re-evaluation, style resolution, and prop sync for that subtree.
    component_memo: RefCell<HashMap<Arc<str>, memo::ComponentMemoEntry>>,
    /// Parent/child aggregate generations used to validate component memo
    /// entries without scanning every embedded runtime.
    runtime_generations: RefCell<memo::RuntimeGenerationIndex>,
    /// Host-module → local-component alias → immutable merged rules and
    /// selector index. Local source and host styles are stable for this
    /// compiled surface, so cache misses can reuse the prepared style input.
    prepared_component_styles: RefCell<
        HashMap<String, HashMap<String, Arc<mesh_core_frontend::PreparedComponentStyleRules>>>,
    >,
    /// Monotonic counters for build side effects that a memoized subtree must
    /// replay (popover wrapper promotion, error placeholders) or that veto
    /// caching entirely (surface-portal state writes). `render_import`
    /// snapshots them around a child build; a delta records the effect on the
    /// stored entry.
    popover_wrapper_marks: Cell<u64>,
    error_placeholder_marks: Cell<u64>,
    portal_state_writes: Cell<u64>,
    /// Number of `render_import` calls served from `component_memo`.
    component_memo_hits: Cell<u64>,
    narrow_path_active: bool,
    affected_node_count: u64,
    profiling_enabled: bool,
    profiling_records: RefCell<Vec<ComponentProfilingRecord>>,
    invalidation_snapshot: Option<mesh_core_debug::ProfilingInvalidationSnapshot>,
    #[cfg(test)]
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
    /// Pseudo-state dependencies in the cached aggregate. Interaction diffs
    /// use this to target only states that can change CSS for this surface.
    cached_restyle_state_dependencies: StyleStateDependencies,
    /// Cached `StyleRuleIndex` built from `cached_restyle_rules`. Reused
    /// across restyle passes; `is_for()` verifies identity against the rules
    /// slice before each restyle so a rules rebuild forces a rebuild here too.
    cached_style_rule_index: Option<mesh_core_elements::style::StyleRuleIndex>,
    /// Incremented whenever the flattened rule cache is rebuilt. Runtime
    /// diagnostic fingerprints include this generation so source/rule reloads
    /// can never reuse a result produced from the previous rule set.
    style_rules_generation: u64,
    /// Inputs from the last full runtime style-diagnostic pass. Script/text
    /// rebuilds often reproduce an identical selector-facing tree; retaining
    /// this lets them skip a second full style resolution per node.
    runtime_style_diagnostic_fingerprint: Option<RuntimeStyleDiagnosticFingerprint>,
    /// Which per-element host metric tables this module can observe. When both
    /// flags are false, `publish_element_metrics` is skipped: building the JSON
    /// snapshots costs meaningful interaction-frame time and is wasted on
    /// scripts that never read them. Recomputed on source reload.
    element_metric_usage: ElementMetricUsage,
    /// Cache of resolved surface shortcuts keyed by the already-cached
    /// `KeyboardSettings` plus active locale. Resolution clones manifest
    /// declarations, checks overrides, and localizes triggers, so avoid doing
    /// that again for every key event when neither input changed.
    resolved_surface_shortcuts_cache: RefCell<Option<ResolvedSurfaceShortcutsCache>>,
}

#[derive(Debug, Clone)]
struct ResolvedSurfaceShortcutsCache {
    keyboard_settings: mesh_core_config::KeyboardSettings,
    locale: String,
    shortcuts: Vec<ResolvedSurfaceShortcut>,
    shortcuts_by_keybind: HashMap<String, Vec<String>>,
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
    /// The declarations used to validate script writes to the reactive props
    /// table. Host layers have already been resolved into `host_props`.
    prop_definitions: Vec<mesh_core_component::PropDef>,
    /// Last prop snapshot supplied by declaration/settings/instance layers.
    /// Used on settings reload to distinguish host-owned values from a newer
    /// script assignment, which has higher precedence and must survive.
    host_props: serde_json::Value,
    /// Cached clone of the script state, keyed by its mutation generation.
    /// Tree builds need a state snapshot that outlives the runtimes lock;
    /// this avoids re-cloning the full variable map on every frame the
    /// state did not change.
    cached_state_clone: Option<(u64, Arc<ScriptState>)>,
}

impl FrontendSurfaceComponent {
    pub(super) fn root_instance_key(&self) -> &str {
        &self.surface_id
    }

    pub(super) fn new(
        compiled: impl Into<SharedCompiledFrontendModule>,
        module_dir: PathBuf,
        frontend_catalog: impl Into<FrontendCatalogHandle>,
        interface_catalog: impl Into<Arc<mesh_core_service::InterfaceCatalog>>,
        settings: impl Into<Arc<SettingsStore>>,
    ) -> Self {
        let compiled = compiled.into();
        let settings = settings.into();
        let motion_policy = MotionPolicy::new(settings.shell().motion.reduced);
        let settings_namespace = compiled.manifest.package.id.clone();
        let settings_state = resolve_frontend_module_settings_with_props(
            &settings_namespace,
            settings.namespace(&settings_namespace),
            &compiled.manifest,
            compiled.component.props.as_ref(),
        );
        mesh_core_config::log_settings_diagnostics("settings", &settings_state.diagnostics);
        let service_payload_capacity = service_payload_cache_capacity(&compiled.manifest);
        let element_metric_usage = element_metric_usage(&compiled);
        let has_animatable_style_rules = compiled_module_has_animatable_style_rules(&compiled);
        let selective_service_build_supported = compiled.supports_selective_service_build();
        let frontend_catalog_handle = frontend_catalog.into();
        let frontend_catalog_state = frontend_catalog_handle.snapshot();
        let frontend_catalog = frontend_catalog_state.catalog;
        let declared_service_names = declared_service_names(&compiled, &frontend_catalog);
        let surface_id = compiled.surface_id().to_string();
        Self {
            surface_id,
            compiled,
            module_dir,
            settings,
            settings_namespace,
            settings_json: settings_state.effective,
            motion_policy,
            settings_diagnostics: settings_state.diagnostics,
            surface_layout: settings_state.layout.clone(),
            keyboard_mode_override: None,
            popup_promoted: false,
            frontend_catalog,
            frontend_catalog_handle,
            frontend_catalog_version: frontend_catalog_state.version,
            effective_capabilities: Arc::new(HashMap::new()),
            graph_i18n_catalogs: Vec::new(),
            visible: settings_state.layout.visible_on_start,
            surface_exiting: false,
            surface_entering: false,
            closing_child_keys: HashSet::new(),
            promoted_window_keys: HashSet::new(),
            entering_child_keys: HashSet::new(),
            dirty: true,
            style_only_dirty: false,
            dirty_types: ComponentDirtyFlags::TREE_REBUILD | ComponentDirtyFlags::SURFACE_CONFIG,
            last_dirty_types: ComponentDirtyFlags::empty(),
            last_service_update: None,
            cached_service_capabilities: HashMap::with_capacity(service_payload_capacity),
            cached_service_payloads: HashMap::with_capacity(service_payload_capacity),
            declared_service_names,
            focused_key: None,
            focus_visible_key: None,
            focused_id: None,
            focus_visible_id: None,
            interaction_state: InteractionState::default(),
            interaction_frame: InteractionFrame::default(),
            pointer_down_id: None,
            pointer_down_bounds: None,
            pointer_down_target: None,
            active_slider_id: None,
            gesture_capture: None,
            touch_targets: HashMap::new(),
            active_touches: HashMap::new(),
            touch_gestures: HashMap::new(),
            last_tap: None,
            keyboard_button_press_activations: HashSet::new(),
            pending_auto_focus: settings_state.layout.visible_on_start
                && settings_state.layout.keyboard_mode != KeyboardMode::None,
            pending_embedded_popover_focus: false,
            embedded_popover_return_focus: None,
            pending_embedded_popover_focus_restore: false,
            return_focus: None,
            close_on_focus_leave: false,
            triggered_popovers: HashMap::new(),
            selection: None,
            input_values: HashMap::new(),
            input_preedits: HashMap::new(),
            input_cursors: HashMap::new(),
            slider_values: HashMap::new(),
            slider_script_values: HashMap::new(),
            checked_values: HashMap::new(),
            render_hooks_pending: true,
            render_hooks_changed_templates: false,
            scroll_offsets: HashMap::new(),
            scheduled_handlers: HashMap::new(),
            scroll_animations: HashMap::new(),
            scroll_inertia: HashMap::new(),
            active_scrollbar_drag: None,
            hovered_key: None,
            hovered_path: Vec::new(),
            hovered_event_path: Vec::new(),
            hovered_tooltip: None,
            previous_hovered_path: Vec::new(),
            previous_focused_key: None,
            previous_focus_visible_key: None,
            previous_active_key: None,
            previous_checked_values: HashMap::new(),
            previous_slider_values: HashMap::new(),
            interaction_snapshot_valid: false,
            hovered_pos: (0.0, 0.0),
            hover_start: None,
            tooltip_visible: false,
            hovered_element_bounds: None,
            tooltip_target_cache: mesh_core_interaction::TooltipTargetCache::default(),
            tooltip_appeared_at: None,
            last_tooltip_damage: None,
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            instance_keys: RefCell::new(InstanceKeyInterner::default()),
            composition_occurrences: RefCell::new(HashMap::new()),
            surface_vm: SurfaceVm::new(),
            render_stack: RefCell::new(Vec::new()),
            active_theme: RefCell::new(Arc::new(default_theme())),
            active_theme_stale: Cell::new(true),
            measured_size: None,
            unmeasured_root_axes: (false, false),
            last_surface_size: None,
            window_states: WindowSurfaceState::default(),
            last_painted_buffer_size: None,
            surface_pixels_invalid: true,
            locale: LocaleEngine::new("en"),
            locale_catalog_is_shared: false,
            interface_catalog: interface_catalog.into(),
            last_tree: None,
            last_frame_snapshot: None,
            frame_revision: 0,
            intrinsic_layout_cache: IntrinsicLayoutCache::default(),
            layout_state: PerSurfaceLayoutState::default(),
            retained_tree: RetainedWidgetTree::default(),
            retained_update_dirty_roots: None,
            #[cfg(test)]
            force_full_retained_update: false,
            animation_only_dirty: false,
            node_service_field_deps: NodeServiceFieldDependencies::default(),
            pending_service_template_nodes: None,
            selective_service_build_supported,
            #[cfg(test)]
            last_template_build_reused_nodes: 0,
            #[cfg(test)]
            force_full_template_build: false,
            retained_display_list: RetainedDisplayList::default(),
            child_display_lists: RefCell::new(ChildDisplayListCache::default()),
            diagnostics: None,
            pending_surface_states: RefCell::new(HashMap::new()),
            last_surface_states: HashMap::new(),
            portal_hidden_bindings: RefCell::new(HashMap::new()),
            bound_children: RefCell::new(HashMap::new()),
            ref_node_keys: RefCell::new(HashMap::new()),
            transitions: TransitionAnimator::new(),
            keyframe_animations: HashMap::new(),
            keyframe_rules: HashMap::new(),
            keyframe_animation_slots: HashMap::new(),
            keyframe_animation_lifecycles: HashMap::new(),
            previous_visual_styles_scratch: HashMap::new(),
            animation_live_keys_scratch: HashSet::new(),
            animation_live_keyframe_keys_scratch: HashSet::new(),
            animation_dirty_node_ids_scratch: HashSet::new(),
            has_animatable_style_rules,
            has_active_keyframe_animation: false,
            has_promoted_popover_wrappers: Cell::new(false),
            has_error_placeholders: Cell::new(false),
            component_memo: RefCell::new(HashMap::new()),
            runtime_generations: RefCell::new(memo::RuntimeGenerationIndex::default()),
            prepared_component_styles: RefCell::new(HashMap::new()),
            popover_wrapper_marks: Cell::new(0),
            error_placeholder_marks: Cell::new(0),
            portal_state_writes: Cell::new(0),
            component_memo_hits: Cell::new(0),
            narrow_path_active: false,
            affected_node_count: 0,
            profiling_enabled: false,
            profiling_records: RefCell::new(Vec::new()),
            invalidation_snapshot: None,
            #[cfg(test)]
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
            cached_restyle_state_dependencies: StyleStateDependencies::default(),
            cached_style_rule_index: None,
            style_rules_generation: 0,
            runtime_style_diagnostic_fingerprint: None,
            element_metric_usage,
            resolved_surface_shortcuts_cache: RefCell::new(None),
        }
    }

    /// Bind this compiled module to a named profile instance. The instance key
    /// scopes surface identity and settings while the compiled module remains
    /// reusable by other profiles or sibling instances.
    pub(super) fn with_instance_id(mut self, instance_id: &str) -> Self {
        self.surface_id = instance_id.to_string();
        self.settings_namespace = instance_id.to_string();
        let settings_state = resolve_frontend_module_settings_with_props(
            &self.settings_namespace,
            self.settings.namespace(&self.settings_namespace),
            &self.compiled.manifest,
            self.compiled.component.props.as_ref(),
        );
        mesh_core_config::log_settings_diagnostics("profile settings", &settings_state.diagnostics);
        self.settings_json = settings_state.effective;
        self.settings_diagnostics = settings_state.diagnostics;
        self.surface_layout = settings_state.layout;
        self.visible = self.surface_layout.visible_on_start;
        self
    }

    pub(super) fn with_effective_capabilities(
        mut self,
        effective_capabilities: Arc<HashMap<String, EffectiveCapabilities>>,
    ) -> Self {
        self.effective_capabilities = effective_capabilities;
        self
    }

    pub(super) fn adopt_frontend_catalog(&mut self, handle: FrontendCatalogHandle) {
        let snapshot = handle.snapshot();
        self.frontend_catalog_handle = handle;
        self.frontend_catalog = snapshot.catalog;
        self.frontend_catalog_version = snapshot.version;
    }

    pub(super) fn with_graph_i18n_catalogs(
        mut self,
        graph_i18n_catalogs: Vec<(String, String, PathBuf)>,
    ) -> Self {
        self.graph_i18n_catalogs = graph_i18n_catalogs;
        self
    }

    pub(super) fn with_locale_catalog_snapshot(
        mut self,
        snapshot: std::sync::Arc<mesh_core_locale::LocaleCatalogSnapshot>,
    ) -> Self {
        self.locale.replace_catalog_snapshot(snapshot);
        self.locale_catalog_is_shared = true;
        self.graph_i18n_catalogs.clear();
        self
    }

    pub(super) fn invalidate(&mut self, flags: ComponentDirtyFlags) {
        self.animation_only_dirty = false;
        self.dirty_types |= flags;
        self.dirty = true;
        if invalidation_requires_pixel_repaint(flags) {
            self.surface_pixels_invalid = true;
        }
    }

    pub(super) fn invalidate_style_path(&mut self, flags: ComponentDirtyFlags) {
        self.animation_only_dirty = false;
        self.dirty_types |= flags;
        self.style_only_dirty = true;
        if invalidation_requires_pixel_repaint(flags) {
            self.surface_pixels_invalid = true;
        }
    }

    pub(super) fn invalidate_animation_style_path(&mut self, flags: ComponentDirtyFlags) {
        let exclusively_animation = self.dirty_types.is_empty() || self.animation_only_dirty;
        self.dirty_types |= flags;
        self.style_only_dirty = true;
        self.animation_only_dirty = exclusively_animation;
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
        self.pending_service_template_nodes = None;
        self.invalidate(ComponentDirtyFlags::TREE_REBUILD);
    }

    /// Narrow script invalidation. Direct service-field updates may reuse clean
    /// static template subtrees; other callers evaluate the template normally.
    /// The retained tree and display list remain authoritative for sparse versus
    /// structural fallback and pixel damage.
    pub(super) fn invalidate_script_state_narrow(&mut self) {
        self.invalidate(ComponentDirtyFlags::SCRIPT_NARROW);
    }

    pub(super) fn invalidate_service_template_nodes(&mut self, nodes: HashSet<NodeId>) {
        self.pending_service_template_nodes
            .get_or_insert_with(HashSet::new)
            .extend(nodes);
        self.invalidate_script_state_narrow();
    }

    pub(super) fn invalidate_interaction_restyle(&mut self) {
        self.invalidate_style_path(ComponentDirtyFlags::INTERACTION_RESTYLE);
    }

    pub(super) fn invalidate_hover_change(&mut self, tooltip_may_change: bool) {
        if self.module_styles_have_hover_rules() {
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
            .all_local_components()
            .into_iter()
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
    for component in compiled.all_local_components() {
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

/// Service names this surface may read: everything its manifest (and the
/// manifests of the component modules it embeds) declares through interface
/// dependencies or read capabilities. Runtime field tracking only knows what
/// has already been read, so this is what keeps the payload cache warm for
/// runtimes that do not exist yet.
fn declared_service_names(
    compiled: &CompiledFrontendModule,
    frontend_catalog: &FrontendCatalog,
) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut collect = |manifest: &mesh_core_module::Manifest| {
        for dependency in &manifest.dependencies.interfaces {
            names.insert(
                crate::shell::service::service_name_from_interface(&dependency.name).to_string(),
            );
        }
        for capability in manifest
            .capabilities
            .required
            .iter()
            .chain(manifest.capabilities.optional.iter())
            .filter(|capability| capability_caches_service_payload(capability))
        {
            let service = match capability.as_str() {
                "theme.read" => "theme",
                "locale.read" => "locale",
                other => other
                    .strip_prefix("service.")
                    .and_then(|other| other.strip_suffix(".read"))
                    .unwrap_or_default(),
            };
            if !service.is_empty() {
                names.insert(service.to_string());
            }
        }
    };

    collect(&compiled.manifest);
    for module_id in compiled.module_component_imports.values() {
        if let Some(entry) = frontend_catalog.modules.get(module_id) {
            collect(&entry.compiled.manifest);
        }
    }
    names
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
