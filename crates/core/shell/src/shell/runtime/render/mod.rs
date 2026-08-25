mod child;
mod popup;

#[cfg(test)]
mod performance_tests;
#[cfg(test)]
mod tests;

use popup::*;

use super::super::*;
use crate::shell::types::{
    ChildSurface, ChildSurfaceKind, PopoverSurfaceRelationship, SurfaceTarget,
};
use mesh_core_elements::style::BackgroundPaint;
use mesh_core_elements::{PopoverAnchor, PopoverConstraintAdjustment, PopoverGrab, PopoverGravity};
use mesh_core_presentation::{
    ContentExtent, LayerSurfaceSizePolicy, LayerWireSize, PopupAnchor, PopupConfig,
    PopupConstraint, PopupGravity, PopupPlacement, PresentStatus, SurfaceConfig,
    SurfaceExtent as ConfiguredSurfaceExtent, SurfaceLifecycleEvent, SurfacePadding,
    SurfaceStateStatus, UnmeasuredSize,
};
use mesh_core_render::{BackdropBlurPolicy, DamageRect, DisplayPaintCommand};
use mesh_core_wayland::SurfaceRole;
use smallvec::SmallVec;

const DEBUG_INSPECTOR_SURFACE_ID: &str = "@mesh/debug-inspector";

pub(super) fn revisioned_surface_config(
    previous: Option<&SurfaceConfig>,
    mut next: SurfaceConfig,
) -> SurfaceConfig {
    next.policy_revision = previous.map_or(1, |previous| {
        let diff = previous.semantic_diff(previous.keyboard_mode, &next, next.keyboard_mode);
        if diff.is_noop() {
            previous.policy_revision
        } else {
            previous.policy_revision.saturating_add(1).max(1)
        }
    });
    next
}

impl Shell {
    pub(in crate::shell) fn render_components(&mut self) -> Result<(), ShellRunError> {
        let backdrop_policy = if self.presentation_engine.supports_compositor_backdrop_blur() {
            BackdropBlurPolicy::CompositorRegion
        } else if mesh_core_render::paint_backend_supports_backdrop_blur() {
            BackdropBlurPolicy::InSurfaceFilter
        } else {
            BackdropBlurPolicy::Rejected
        };
        mesh_core_render::set_backdrop_blur_policy(backdrop_policy);
        let icon_resolutions_ready = mesh_core_render::poll_icon_resolution_jobs();
        let icon_rasters_ready = mesh_core_render::poll_icon_raster_jobs();
        let glyph_rasters_ready = mesh_core_render::poll_glyph_raster_jobs();
        let image_decodes_ready = mesh_core_render::poll_image_decode_jobs();
        if icon_resolutions_ready
            || icon_rasters_ready
            || glyph_rasters_ready
            || image_decodes_ready
        {
            for runtime in &mut self.components {
                runtime.component.request_paint();
            }
        }
        self.drain_surface_lifecycle_events()?;
        self.drain_window_close_requests()?;
        self.sync_text_input_state()?;

        let font_revision = self.font_registry.revision();
        if self.font_renderer_revision != font_revision {
            mesh_core_render::set_font_database(self.font_registry.font_database());
            self.font_renderer_revision = font_revision;
        }

        if self.debug.enabled {
            let mut debug_requests = self.publish_debug_snapshot()?;
            self.drain_requests(&mut debug_requests)?;
        }

        let mut components_want_render_after_frame = false;
        let mut any_component_presented = false;
        for index in 0..self.components.len() {
            let surface_id = self.components[index].surface_id.clone();
            // Ahead of the `wants_render` gate: a compositor state change is
            // precisely what makes an otherwise-quiet window want to render.
            // Asking behind the gate would strand a fullscreened surface at its
            // floating style until something else happened to dirty it.
            // Keyed off the shell-surface record — what has actually been
            // applied — not the component's requested role. A promotion is
            // applied by `render_layout` further down this same frame, so the
            // record catches up before the compositor has any states to report.
            // Demotion needs no delivery here at all: `surface_role_changed`
            // clears the states itself, because a layer surface has none.
            if self
                .surfaces
                .get(&surface_id)
                .is_some_and(|surface| surface.role == SurfaceRole::Window)
            {
                let states = self.presentation_engine.window_states(&surface_id);
                self.components[index]
                    .component
                    .surface_window_states_changed(states);
                // The compositor's configured size has to cross the gate for the
                // same reason the states do — and it is the more urgent of the
                // two. During an interactive resize the pointer is grabbed by
                // the compositor, so a window under the drag receives *no* input
                // events: a resize configure is the only thing happening, and if
                // it does not dirty the component nothing re-lays-out until the
                // drag ends. The compositor meanwhile scales the last committed
                // buffer to the window box it is dragging, which is what makes
                // the whole surface — text, borders, background fills — appear
                // stretched until release. Observing the size here re-measures
                // and repaints per configure instead, so what the compositor
                // scales is never more than a frame stale.
                if let Some((width, height)) =
                    self.presentation_engine.window_configured_size(&surface_id)
                {
                    let resolved_size = (width.max(1), height.max(1));
                    if self.components[index].parent.known_surface_size != Some(resolved_size) {
                        self.components[index].parent.known_surface_size = Some(resolved_size);
                        self.components[index]
                            .component
                            .surface_size_changed(resolved_size.0, resolved_size.1);
                    }
                }
            }
            let resource_revision = mesh_core_resources::resource_revision();
            let resource_revision_changed = self.components[index].targets().any(|target| {
                target
                    .last_paint_resource_revision
                    .is_some_and(|seen| seen != resource_revision)
            });
            if resource_revision_changed {
                self.components[index].component.request_paint();
            }
            if !self.components[index].component.wants_render() {
                continue;
            }
            let visible = self.surface_is_effectively_visible(&surface_id);
            if !visible
                && self.components[index].parent.last_surface_config.is_none()
                && self.components[index].parent.known_surface_size.is_none()
            {
                continue;
            }
            if visible
                && (self
                    .presentation_engine
                    .surface_waiting_for_frame_callback(&surface_id)
                    || self
                        .presentation_engine
                        .surface_waiting_for_buffer_release(&surface_id))
            {
                components_want_render_after_frame = true;
                continue;
            }
            // A surface that still owes the compositor a configure must not be
            // gated on the compositor having answered one. Hiding a layer
            // surface unmaps it by attaching a null buffer, which returns it to
            // the unconfigured state on both sides: the backend clears
            // `configured` and the hidden frame below drops
            // `last_surface_config`. Waiting for readiness in that state waits
            // for a configure event that only the configure request further
            // down can provoke, so the surface would never map again — the
            // second and every later `ShowSurface` would silently do nothing.
            // The present itself stays safe: it reports `NotReady` while the
            // compositor has not answered, which retains the damage and asks
            // for another frame.
            let owes_configure = self.components[index].parent.last_surface_config.is_none();
            if visible
                && !owes_configure
                && !self
                    .presentation_engine
                    .surface_ready_to_present(&surface_id)
            {
                components_want_render_after_frame = true;
                continue;
            }
            let surface_size = {
                let surface = self
                    .surfaces
                    .get(&surface_id)
                    .ok_or_else(|| ShellRunError::MissingSurface(surface_id.clone()))?;
                if surface.width == 0 || surface.height == 0 {
                    self.presentation_engine.surface_size(&surface_id)?
                } else {
                    Some((surface.width.max(1), surface.height.max(1)))
                }
            };
            if let Some(surface_size) = surface_size {
                let resolved_size =
                    self.content_size_for_target(index, TargetRef::Parent, surface_size);
                if self.components[index].parent.known_surface_size != Some(resolved_size) {
                    self.components[index].parent.known_surface_size = Some(resolved_size);
                    self.components[index]
                        .component
                        .surface_size_changed(resolved_size.0, resolved_size.1);
                }
            }
            let profiling_enabled = self.profiling_enabled();
            let total_render_started = profiling_enabled.then(std::time::Instant::now);
            let allocation_started = (profiling_enabled
                && mesh_core_debug::allocation::profiling_available())
            .then(mesh_core_debug::allocation::snapshot);
            let mut rerender_attempts = 0;
            let mut component_stage_records = Vec::new();
            let component_id = surface_id.as_str();
            // Hoist logical dimensions and scale before the loop so that
            // the post-loop force-full-redraw and debug-overlay paths can
            // reference them without depending on loop-scoped mutable borrows.
            let (width, height, scale) = {
                let surface = self
                    .surfaces
                    .get(&surface_id)
                    .ok_or_else(|| ShellRunError::MissingSurface(surface_id.clone()))?;
                let requested_width = surface.width;
                let requested_height = surface.height;
                let (width, height) = if requested_width == 0 || requested_height == 0 {
                    let dynamic_size = self.resolve_dynamic_surface_size(index, &surface_id)?;
                    let w = if requested_width == 0 {
                        dynamic_size.map(|(w, _)| w).unwrap_or(1)
                    } else {
                        requested_width.max(1)
                    };
                    let h = if requested_height == 0 {
                        dynamic_size.map(|(_, h)| h).unwrap_or(1)
                    } else {
                        requested_height.max(1)
                    };
                    (w, h)
                } else {
                    (requested_width.max(1), requested_height.max(1))
                };
                // See the in-loop override below: for a window the compositor's
                // configured size wins over CSS measurement.
                let (width, height) = self
                    .presentation_engine
                    .window_configured_size(&surface_id)
                    .map(|(w, h)| (w.max(1), h.max(1)))
                    .unwrap_or((width, height));
                let scale = self.presentation_engine.surface_scale(&surface_id);
                (width, height, scale)
            };
            let mut width = width;
            let mut height = height;
            let mut scale = scale;
            // Buffer/present dimensions: content plus the tooltip overlay
            // reserve for parent layer surfaces (popups stay content-sized).
            let mut paint_width = width;
            let mut paint_height = height;
            let mut parent_reconfigured = false;
            loop {
                let surface = self
                    .surfaces
                    .get_mut(&surface_id)
                    .ok_or_else(|| ShellRunError::MissingSurface(surface_id.clone()))?;
                self.components[index]
                    .component
                    .set_profiling_enabled(profiling_enabled);
                self.components[index]
                    .component
                    .render(surface)
                    .map_err(ShellRunError::Component)?;

                let visible = self
                    .core
                    .surfaces
                    .get(&surface_id)
                    .map(|state| state.visible)
                    .unwrap_or(surface.visible);
                if !visible {
                    // Do not reconfigure hidden surfaces to synthetic 1x1/zero-margin
                    // geometry before detaching them. Some compositors can show that
                    // transient geometry during close, which makes anchored popovers
                    // appear to fly toward the default screen position.
                    let runtime = &mut self.components[index];
                    if runtime
                        .parent
                        .paint_buffer
                        .as_ref()
                        .map(|buffer| buffer.width() != 1 || buffer.height() != 1)
                        .unwrap_or(true)
                    {
                        runtime.parent.paint_buffer = Some(PixelBuffer::new(1, 1));
                    }
                    runtime.parent.known_surface_size = None;
                    runtime.parent.last_surface_config = None;
                    runtime.parent.last_popup_size = None;
                    runtime.parent.last_region_state = None;
                    break;
                }

                // Popup surfaces (xdg_popup) skip the layer-surface configure
                // path entirely — they are created/repositioned via
                // configure_popup() after the content size is resolved below.
                let is_popup = self.components[index].parent.popup_config.is_some();

                // A never-before-configured layer surface has no retained
                // widget tree yet, so `render()` (which measures content from
                // the *existing* tree) reports a stale/zero size on this very
                // first pass — the real tree is only built moments later by
                // `paint()` below. Popups already dodge this with
                // `defer_popup_create` + an immediate-rerender pass; mirror
                // that here so the first-ever configure for a layer surface
                // isn't sent with an unmeasured size (which the layer-shell
                // backend then has to clamp to a broken 1x1, see
                // `layer_protocol_size`).
                let first_layer_configure =
                    !is_popup && self.components[index].parent.last_surface_config.is_none();

                // Compare all copy fields before cloning namespace (the only heap field).
                let size_policy = self.components[index].parent.surface_size_policy;
                let layer = surface.layer.unwrap_or(Layer::Top);
                // The compositor-facing layer surface is inflated by the
                // tooltip overlay reserve so tooltips can paint outside the
                // content box. `surface.width/height` (and everything the
                // component sees) stay content-sized, and `configured_padding`
                // travels with the inflated size so the presentation layer can
                // confine pointer input back to the content rect.
                let (requested_width, requested_height, _) = surface_geometry_with_overlay_reserve(
                    &surface_id,
                    surface.role,
                    surface.width,
                    surface.height,
                );
                // `render()` may have just completed a measurement by updating
                // the shell-side surface record, while `width`/`height` still
                // describe the previous pass's buffer. Prefer that record and
                // use the resolved pass size only for an intentional spanning
                // axis so the typed extent follows the post-measurement
                // geometry decision.
                let measured_content = UnmeasuredSize::from_optional(
                    (surface.width > 0)
                        .then_some(surface.width)
                        .or((width > 0).then_some(width)),
                    (surface.height > 0)
                        .then_some(surface.height)
                        .or((height > 0).then_some(height)),
                );
                let content = measured_content.content().map_err(|error| {
                    ShellRunError::Presentation(
                        mesh_core_presentation::PresentationError::SurfaceCreate(error.to_string()),
                    )
                })?;
                let (_, _, configured_padding) = surface_geometry_with_overlay_reserve(
                    &surface_id,
                    surface.role,
                    content.width(),
                    content.height(),
                );
                let extent =
                    ConfiguredSurfaceExtent::from_content_and_padding(content, configured_padding)
                        .map_err(|error| {
                            ShellRunError::Presentation(
                                mesh_core_presentation::PresentationError::SurfaceCreate(
                                    error.to_string(),
                                ),
                            )
                        })?;
                let wire_size = if surface.role == SurfaceRole::Window {
                    LayerWireSize::fixed(extent.surface_size().0, extent.surface_size().1)
                } else {
                    LayerWireSize::from_requested((surface.width, surface.height), extent.surface())
                }
                .map_err(|error| {
                    ShellRunError::Presentation(
                        mesh_core_presentation::PresentationError::SurfaceCreate(error.to_string()),
                    )
                })?;
                let mut cfg = SurfaceConfig {
                    role: surface.role,
                    window: surface.window.clone(),
                    edge: surface.edge,
                    layer,
                    size_policy,
                    extent,
                    wire_size,
                    exclusive_zone: surface.exclusive_zone,
                    keyboard_mode: surface.keyboard_mode,
                    namespace: surface_id.clone(),
                    margin_top: surface.margin_top,
                    margin_right: surface.margin_right,
                    margin_bottom: surface.margin_bottom,
                    margin_left: surface.margin_left,
                    blur: surface.blur,
                    policy_revision: 0,
                };
                let previous_config = self.components[index].parent.last_surface_config.as_ref();
                let config_changed = previous_config.map_or(true, |last| {
                    !last
                        .semantic_diff(last.keyboard_mode, &cfg, cfg.keyboard_mode)
                        .is_noop()
                });
                cfg = revisioned_surface_config(previous_config, cfg);
                cfg.policy_revision = cfg
                    .policy_revision
                    .max(self.components[index].component.surface_policy_revision());
                // `render_layout` writes `(0, 0)` for a component with no content
                // measurement, and `observe_surface_size` drops that measurement
                // whenever the available size changes — so a zero can reach this
                // point on any frame between a configure and the paint that
                // re-measures, and it survives into later passes of the same
                // frame because `render()` only re-runs `render_layout` while the
                // surface config is dirty. Zero is not "unknown" on the wire: it
                // is layer-shell for "span the output", so `layer_protocol_size`
                // has to invent a size for a zero the anchor does not back — the
                // exclusive zone for a docked rail, a 1x1 clamp for a floating
                // surface rather than map a screen-sized input sink. Either way
                // it is a real, visible resize that collapses the surface into a
                // sliver at its anchor corner until the next measured configure
                // pops it back, which is the flicker seen when a surface is shown
                // or hidden. Hold the last good geometry instead: `config_changed`
                // stays true, so the configure goes out as soon as a size the
                // compositor can honour exists.
                //
                // A window is exempt: its size is a hint the compositor answers
                // rather than a placement it applies, and `apply_window_config`
                // reads a zero as "no hint" instead of as a span.
                let configure_size_resolved = surface.role == SurfaceRole::Window
                    || layer_configure_size_is_resolved(
                        surface.edge,
                        surface.exclusive_zone,
                        requested_width,
                        requested_height,
                    );
                // A pass that cannot produce a usable configure earns one more,
                // taken after the paint below has measured the content.
                let unmeasured_configure = !is_popup
                    && (!configure_size_resolved
                        || self.components[index].component.needs_content_measure());
                let defer_first_layer_configure = first_layer_configure && rerender_attempts == 0;
                if config_changed
                    && !is_popup
                    && !defer_first_layer_configure
                    && configure_size_resolved
                {
                    tracing::debug!(
                        surface_id = %surface_id,
                        width = extent.surface_size().0,
                        height = extent.surface_size().1,
                        edge = ?surface.edge,
                        exclusive_zone = surface.exclusive_zone,
                        margin_top = surface.margin_top,
                        margin_right = surface.margin_right,
                        margin_bottom = surface.margin_bottom,
                        margin_left = surface.margin_left,
                        first_layer_configure,
                        rerender_attempts,
                        "sending layer-surface configure"
                    );
                    self.presentation_engine
                        .configure(&surface_id, cfg.clone())
                        .map_err(ShellRunError::Presentation)?;
                    let target = &mut self.components[index].parent;
                    target.last_surface_config = Some(cfg);
                    // A successful configure may have recreated the Wayland
                    // role. Region state belongs to the compositor object, so
                    // force it to be restaged for the replacement even when
                    // display-list generation and geometry are unchanged.
                    target.last_region_state = None;
                    // A geometry-changing configure invalidates the
                    // compositor's previously-acked size: `apply_config`
                    // flips the backend's `entry.configured` to false until a
                    // fresh configure event arrives, but `known_surface_size`
                    // here is a separate shell-side cache that isn't tied to
                    // that flag. Left stale, `resolve_dynamic_surface_size`
                    // short-circuits on the old (possibly clamped-to-1x1)
                    // size below instead of waiting for the new ack, which
                    // pins the surface at its first-ever (often wrong) size
                    // forever.
                    self.components[index].parent.known_surface_size = None;
                    // Reconfiguring a parent may replace its compositor role,
                    // which also destroys every popup/window child owned by
                    // that role. Drop child-side compositor caches together
                    // with the parent cache so the next reconcile recreates
                    // the child object and repaints its retained content.
                    parent_reconfigured = true;
                }

                let inner_requested_width = surface.width;
                let inner_requested_height = surface.height;
                // A content-measured popup has no real size until its first
                // paint measures the content. Defer creating the `xdg_popup`
                // until the loop's immediate-rerender pass below, so it is
                // created at the measured size instead of a placeholder that
                // visibly grows on the next open.
                let defer_popup_create =
                    is_popup && self.components[index].component.needs_content_measure();
                // A promoted popup has no compositor size before its first
                // configure. Its positioner starts with a 1x1 protocol-safe
                // placeholder, but using that as the layout bound collapses
                // intrinsic cross-axis content and makes the 1px result a
                // fixed point. Use the already-configured parent surface as a
                // generous measurement bound; `SurfaceExtent::intrinsic`
                // keeps it semantically unknown so the root can still shrink
                // to its content.
                let popup_measurement_bound = defer_popup_create
                    .then(|| self.popup_intrinsic_measurement_bound(index))
                    .flatten();
                // Whether each axis below is a size the shell actually has
                // (compositor configure, declared placement, or a completed
                // measurement) rather than a placeholder standing in for one.
                let mut width_known;
                let mut height_known;
                if is_popup {
                    // A popup's size must come from the component's own
                    // CSS-measured content size, NOT the presentation surface
                    // size. The presentation/shell-surface size can be unknown
                    // before first creation, compositor-reported after creation,
                    // or a stale layer-surface render-buffer size that includes
                    // transparent tooltip padding. `render` runs the loop's
                    // first paint, which populates `measured_size`; the loop's
                    // immediate-rerender pass then reaches this point with the
                    // real measured size and creates/repositions the popup to
                    // that geometry within the same frame. (Layer surfaces keep
                    // their own `set_size`/`resolve_dynamic_surface_size` path
                    // below; it feeds `measured_size` to the compositor via
                    // `render_layout`, which is skipped for promoted popups.)
                    let (measured_w, measured_h) =
                        self.components[index].component.declared_or_measured_size();
                    width_known = measured_w > 0;
                    height_known = measured_h > 0;
                    width = measured_w.max(1);
                    height = measured_h.max(1);
                    if let Some((bound_width, bound_height)) = popup_measurement_bound {
                        if !width_known {
                            width = bound_width.max(1);
                        }
                        if !height_known {
                            height = bound_height.max(1);
                        }
                    }
                } else {
                    let dynamic_size = if inner_requested_width == 0 || inner_requested_height == 0
                    {
                        self.resolve_dynamic_surface_size(index, &surface_id)?
                    } else {
                        None
                    };
                    let (fallback_width, fallback_height) =
                        self.components[index].component.declared_or_measured_size();
                    // `None` is "this axis has no size yet", which is not the
                    // same statement as the 1px stand-in the paint buffer
                    // needs: it travels on to `paint` through the extent so
                    // the surface root lays that axis out as `auto` instead of
                    // collapsing its content into one pixel.
                    let resolved_width = if inner_requested_width == 0 {
                        dynamic_size
                            .map(|(w, _)| w)
                            .or((fallback_width > 0).then_some(fallback_width))
                    } else {
                        Some(inner_requested_width)
                    };
                    let resolved_height = if inner_requested_height == 0 {
                        dynamic_size
                            .map(|(_, h)| h)
                            .or((fallback_height > 0).then_some(fallback_height))
                    } else {
                        Some(inner_requested_height)
                    };
                    width_known = resolved_width.is_some();
                    height_known = resolved_height.is_some();
                    width = resolved_width.unwrap_or(1).max(1);
                    height = resolved_height.unwrap_or(1).max(1);
                }
                // A window surface sizes in the opposite direction to a layer
                // surface: once the compositor has decided a size (tiling
                // layout, maximize, interactive resize) that decision is
                // binding and the content lays out into it, rather than the
                // CSS-measured size being sent as a request.
                if let Some((window_width, window_height)) =
                    self.presentation_engine.window_configured_size(&surface_id)
                {
                    width = window_width.max(1);
                    height = window_height.max(1);
                    width_known = true;
                    height_known = true;
                }
                let resolved_size = (width, height);
                if self.components[index].parent.known_surface_size != Some(resolved_size) {
                    self.components[index].parent.known_surface_size = Some(resolved_size);
                    self.components[index]
                        .component
                        .surface_size_changed(width, height);
                }

                // For xdg_popup surfaces, call configure_popup with the
                // resolved content size. This creates the surface on first
                // show and repositions it when the size changes (e.g. the
                // content grows or shrinks between opens).
                if is_popup
                    && !defer_popup_create
                    && self.components[index].parent.last_popup_size != Some(resolved_size)
                {
                    self.components[index].parent.last_popup_size = Some(resolved_size);
                    let config = self.components[index]
                        .parent
                        .popup_config
                        .as_mut()
                        .map(|c| {
                            c.placement.size = resolved_size;
                            c.clone()
                        });
                    if let Some(config) = config {
                        if let Err(e) = self
                            .presentation_engine
                            .configure_popup(&surface_id, config)
                        {
                            tracing::warn!("configure_popup for {surface_id} failed: {e}");
                        }
                    }
                }

                scale = self.presentation_engine.surface_scale(&surface_id);
                // The paint buffer matches the compositor-configured surface:
                // content plus the tooltip overlay reserve for parent layer
                // surfaces. `width`/`height` stay content-sized for the
                // component-facing notifications and popup config above, and
                // the component is handed both halves so its own unmeasured
                // fallback can never pick up the reserve.
                // An axis the shell could not resolve is handed over as `0`
                // ("no size yet"), never as the 1px stand-in the buffer below
                // is allocated with: `paint` lays an unknown axis out as
                // `auto` and measures the content, rather than pinning the
                // surface root to one pixel and measuring the collapse.
                let measured = UnmeasuredSize::from_optional(
                    (width_known || popup_measurement_bound.is_some()).then_some(width),
                    (height_known || popup_measurement_bound.is_some()).then_some(height),
                );
                let extent_content = (
                    measured.width().unwrap_or(0),
                    measured.height().unwrap_or(0),
                );
                let paint_extent = if is_popup {
                    if popup_measurement_bound.is_some() && (!width_known || !height_known) {
                        SurfaceExtent::intrinsic((width, height))
                    } else {
                        SurfaceExtent::padded(extent_content, (width, height))
                    }
                } else {
                    let (padded_width, padded_height, _) = surface_geometry_with_overlay_reserve(
                        &surface_id,
                        self.surfaces
                            .get(&surface_id)
                            .map(|surface| surface.role)
                            .unwrap_or_default(),
                        width,
                        height,
                    );
                    SurfaceExtent::padded(extent_content, (padded_width, padded_height))
                };
                (paint_width, paint_height) = paint_extent.padded;
                let scale_policy = mesh_core_render::FractionalScale::new(scale);
                let physical_w = scale_policy.physical_extent(paint_width);
                let physical_h = scale_policy.physical_extent(paint_height);

                // Cap the buffer allocation so a bad measured size cannot ask
                // for gigabytes.
                const MAX_BUFFER_BYTES: u64 = PixelBuffer::MAX_BYTES as u64;
                let requested_bytes = (physical_w as u64) * (physical_h as u64) * 4;
                if requested_bytes > MAX_BUFFER_BYTES {
                    return Err(ShellRunError::BufferAlloc {
                        surface_id: surface_id.clone(),
                        logical_w: paint_width,
                        logical_h: paint_height,
                        physical_w,
                        physical_h,
                        scale,
                        requested_bytes,
                        max_bytes: MAX_BUFFER_BYTES,
                    });
                }

                let module_id = self.components[index].component.id().to_string();
                mesh_core_render::set_font_aliases(
                    self.font_registry.reference_aliases_for_module(&module_id),
                );
                let runtime = &mut self.components[index];
                if runtime
                    .parent
                    .paint_buffer
                    .as_ref()
                    .map(|buffer| buffer.width() != physical_w || buffer.height() != physical_h)
                    .unwrap_or(true)
                {
                    let buffer = PixelBuffer::try_new(physical_w, physical_h).ok_or_else(|| {
                        ShellRunError::BufferAlloc {
                            surface_id: surface_id.clone(),
                            logical_w: paint_width,
                            logical_h: paint_height,
                            physical_w,
                            physical_h,
                            scale,
                            requested_bytes,
                            max_bytes: MAX_BUFFER_BYTES,
                        }
                    })?;
                    runtime.parent.paint_buffer = Some(buffer);
                    // A resized buffer starts fully transparent; `paint()` only
                    // repaints dirty regions against the retained tree, so
                    // without forcing a full present the untouched pixels of a
                    // freshly-allocated buffer never get drawn until something
                    // else marks the whole surface dirty.
                    runtime.parent.force_full_present = true;
                }
                runtime
                    .component
                    .paint(
                        self.theme.active(),
                        paint_extent,
                        runtime
                            .parent
                            .paint_buffer
                            .as_mut()
                            .expect("paint buffer initialised"),
                        scale,
                    )
                    .map_err(ShellRunError::Component)?;
                runtime.parent.last_paint_resource_revision =
                    Some(mesh_core_resources::resource_revision());
                component_stage_records.extend(runtime.component.take_profiling_records());

                // When popup creation was deferred to measure the content, the
                // paint above has now populated `measured_size`; force one more
                // iteration so the `xdg_popup` is created at the measured size
                // (the immediate-rerender gate alone returns false for a
                // surface-config-only change). Layer surfaces get the same
                // treatment on their first-ever configure: the paint above
                // just built the retained tree for the first time, so
                // re-running render() now lets it re-measure and send a
                // corrected `configure()` instead of leaving the surface
                // stuck at the unmeasured first-pass size. A pass that skipped
                // its configure for want of a measurement takes the same second
                // pass, so the configure it owes goes out in this frame instead
                // of a later one.
                if (!self.components[index].component.wants_immediate_rerender()
                    && !defer_popup_create
                    && !first_layer_configure
                    && !unmeasured_configure)
                    || rerender_attempts >= 1
                {
                    break;
                }

                // The corrective pass exists to re-derive the surface config
                // from the measurement the paint above just produced, but
                // `render` only re-runs `render_layout` while that config is
                // dirty — and a component whose content measures exactly what it
                // was laid out against leaves it clean. Without this the second
                // pass would recompute the configure from the first pass's
                // unmeasured dimensions and send those instead.
                self.components[index].component.invalidate_surface_config();

                rerender_attempts += 1;
            }

            if parent_reconfigured {
                self.invalidate_child_surface_targets_after_parent_configure(index);
            }

            // Component(VM)-level profiling + invalidation are recorded once,
            // regardless of how many surface targets the component drives.
            let observation_summary = self.components[index]
                .component
                .service_observation_summary();
            self.service_delivery_index
                .mark_dirty_if_summary_changed(index, observation_summary);
            for record in component_stage_records {
                let module_id = record
                    .module_id
                    .as_deref()
                    .filter(|id| !id.is_empty())
                    .or(Some(component_id));
                self.record_surface_profiling_stage(
                    &surface_id,
                    module_id,
                    record.stage,
                    record.duration,
                    record.trigger_kind.as_deref(),
                );
            }
            if let Some(invalidation) = self.components[index]
                .component
                .take_invalidation_snapshot()
            {
                self.record_surface_invalidation(&surface_id, Some(component_id), invalidation);
            }

            // Present the component's parent surface. Child popup targets paint
            // their own subtree and are presented separately during reconcile.
            let presented = self.present_surface_target(
                index,
                TargetRef::Parent,
                component_id,
                paint_width,
                paint_height,
                scale,
                total_render_started,
                allocation_started,
            )?;
            any_component_presented |= presented;
            if presented {
                components_want_render_after_frame |=
                    self.components[index].component.wants_render();
            }

            let child_presented = self.reconcile_child_surface_requests(
                index,
                component_id,
                &surface_id,
                scale,
                total_render_started,
            )?;
            any_component_presented |= child_presented;
            // Reconciliation can invalidate the component without presenting
            // a child yet (notably the staged first entrance paint).
            components_want_render_after_frame |= self.components[index].component.wants_render();
        }
        self.components_want_render = components_want_render_after_frame;
        self.presented_last_frame = any_component_presented;
        Ok(())
    }

    /// Act on close requests from window surfaces (title-bar button, compositor
    /// close binding).
    ///
    /// Closing hides the surface rather than destroying the component: the
    /// module, its services, and its Lua state survive, so reopening the window
    /// is the same cheap show that reopening a hidden panel is. Nothing here
    /// unmaps the toplevel directly — `set_surface_visibility_now` runs the
    /// same hide path every other surface uses, including the CSS exit
    /// transition.
    fn drain_window_close_requests(&mut self) -> Result<(), ShellRunError> {
        for surface_id in self.presentation_engine.take_close_requests() {
            let Some((index, target)) = self.component_target_for_surface(&surface_id) else {
                continue;
            };
            if let TargetRef::Child(child_index) = target
                && self.components[index].children[child_index].kind == ChildSurfaceKind::Window
            {
                let node_key = self.components[index].children[child_index]
                    .node_key
                    .clone();
                tracing::info!(surface_id, %node_key, "embedded widget window close requested; demoting widget");
                self.set_child_surface_role(
                    self.components[index].surface_id.clone(),
                    node_key,
                    SurfaceRole::Layer,
                )?;
                continue;
            }
            tracing::info!(surface_id, "window close requested; hiding surface");
            let mut pending = self.set_surface_visibility_now(surface_id, false)?;
            self.drain_requests(&mut pending)?;
        }
        Ok(())
    }

    pub(in crate::shell) fn drain_surface_lifecycle_events(&mut self) -> Result<(), ShellRunError> {
        for event in self.presentation_engine.take_surface_lifecycle_events() {
            match event {
                SurfaceLifecycleEvent::Dismissed { surface_id } => {
                    match self.component_target_for_surface(&surface_id) {
                        Some((index, TargetRef::Child(child_index))) => {
                            let child = &self.components[index].children[child_index];
                            let kind = child.kind;
                            let node_key = child.node_key.clone();
                            let relationship = child.popover_relationship.clone();
                            let restore_focus = kind == ChildSurfaceKind::Popover
                                && self.keyboard_focus_surface.as_deref()
                                    == Some(surface_id.as_str());
                            self.destroy_child_surface_at(index, child_index);
                            if let Some(runtime) = self.components.get_mut(index) {
                                if kind == ChildSurfaceKind::Popover {
                                    runtime.dismissed_child_surfaces.insert((kind, node_key));
                                } else {
                                    // Overflow is derived from current escape
                                    // geometry. A compositor dismissal means
                                    // only that this object disappeared; it
                                    // must be retried while the node still
                                    // escapes the parent.
                                    runtime.component.request_paint();
                                }
                            }
                            if restore_focus
                                && let Some(relationship) = relationship
                                && self.surface_is_effectively_visible(
                                    &relationship.trigger_surface_id,
                                )
                                && matches!(
                                    self.component_target_for_surface(
                                        &relationship.trigger_surface_id,
                                    ),
                                    Some((_, TargetRef::Parent))
                                )
                            {
                                let mut requests =
                                    self.apply_request(CoreRequest::TransferTabFocus {
                                        from_surface: surface_id.clone(),
                                        to_surface: relationship.trigger_surface_id,
                                        target: TabFocusTarget::AtKey(
                                            relationship.trigger_reference.reference,
                                        ),
                                        return_target: None,
                                        target_closes_on_leave: false,
                                        close_source: None,
                                    })?;
                                self.drain_requests(&mut requests)?;
                            }
                        }
                        Some((index, TargetRef::Parent))
                            if self.components[index].parent.popup_parent_surface.is_some() =>
                        {
                            self.pending_popover_hides.remove(&surface_id);
                            let mut pending = self.set_surface_visibility_now(surface_id, false)?;
                            self.drain_requests(&mut pending)?;
                        }
                        _ => {}
                    }
                }
                SurfaceLifecycleEvent::Closed { surface_id }
                | SurfaceLifecycleEvent::Lost { surface_id, .. } => {
                    let Some((index, target)) = self.component_target_for_surface(&surface_id)
                    else {
                        continue;
                    };
                    match target {
                        TargetRef::Child(child_index) => {
                            self.destroy_child_surface_at(index, child_index);
                        }
                        TargetRef::Parent => {
                            self.destroy_all_child_surfaces(index);
                            let target = self.components[index].target_mut(TargetRef::Parent);
                            target.last_surface_config = None;
                            target.known_surface_size = None;
                            target.last_region_state = None;
                            target.force_full_present = true;
                        }
                    }
                    self.components[index].component.request_paint();
                }
            }
        }
        Ok(())
    }

    pub(in crate::shell) fn destroy_all_child_surfaces(&mut self, index: usize) {
        while !self.components[index].children.is_empty() {
            self.destroy_child_surface_at(index, 0);
        }
    }

    fn invalidate_child_surface_targets_after_parent_configure(&mut self, index: usize) {
        for child in &mut self.components[index].children {
            child.target.last_surface_config = None;
            child.target.known_surface_size = None;
            child.target.last_region_state = None;
            if child.kind != ChildSurfaceKind::Window {
                child.target.last_popup_size = None;
            }
            child.target.force_full_present = true;
        }
    }

    pub(in crate::shell) fn destroy_child_surface_at(&mut self, index: usize, child_index: usize) {
        if child_index >= self.components[index].children.len() {
            return;
        }
        let surface_id = self.components[index].children[child_index]
            .target
            .surface_id
            .clone();
        let kind = self.components[index].children[child_index].kind;
        self.pending_popover_hides.remove(&surface_id);
        self.components[index].children.remove(child_index);
        if kind == ChildSurfaceKind::Window {
            self.presentation_engine.destroy_surface(&surface_id);
        } else {
            self.presentation_engine.destroy_popup(&surface_id);
        }
        self.core.surfaces.remove(&surface_id);
        self.surfaces.remove(&surface_id);
        self.component_by_surface.remove(&surface_id);
        if self.keyboard_focus_surface.as_deref() == Some(surface_id.as_str()) {
            self.keyboard_focus_surface = None;
        }
        self.transfer_owned_keyboard_modes.remove(&surface_id);
        self.rebuild_component_surface_index();
    }

    /// Run the post-paint present pipeline for one surface target of a
    /// component — its parent surface, or (later) a child popup. Computes
    /// opaque/input/blur regions, resolves present damage (handling force-full
    /// and scale-change full redraws), paints the debug layout overlay, commits
    /// the buffer, and records profiling. Returns whether a present was issued.
    ///
    /// Region and debug-overlay computation is parent-only for now; child popup
    /// targets supply their own subtree damage when reconciled.
    fn present_surface_target(
        &mut self,
        index: usize,
        target: TargetRef,
        component_id: &str,
        width: u32,
        height: u32,
        scale: f32,
        total_render_started: Option<std::time::Instant>,
        allocation_started: Option<mesh_core_debug::allocation::AllocationCounters>,
    ) -> Result<bool, ShellRunError> {
        let surface_id = self.components[index].target(target).surface_id.clone();
        let visible = self.surface_is_effectively_visible(&surface_id);
        let is_parent = matches!(target, TargetRef::Parent);

        if visible && is_parent {
            let generation = self.components[index].component.display_list_generation();
            let surface_size = self.components[index].target(target).known_surface_size;
            let content_size = self.components[index].component.content_input_size();
            let region_state = (
                generation,
                mesh_core_resources::resource_revision(),
                surface_size,
                content_size,
            );
            if self.components[index].target(target).last_region_state != Some(region_state) {
                let commands = self.components[index]
                    .component
                    .display_list_paint_commands();
                let opaque_rect = surface_size.and_then(|(surface_w, surface_h)| {
                    compute_opaque_rect_for_root(commands, surface_w, surface_h)
                });
                self.presentation_engine
                    .update_opaque_region(&surface_id, opaque_rect);

                // No input-region push here on purpose. Confining pointer input
                // to the content rect used to live at this call site, guarded by
                // the `last_region_state` cache above — and every way that cache
                // could be warm while the compositor object was not (a surface
                // recreated for a role swap, a window destroyed on hide, a
                // present that returned before the region was flushed) brought
                // back the dead zone under the bar. The reserve now travels with
                // the size that created it (`SurfacePadding` in the surface
                // config) and the region is re-derived on every commit.

                let blur_regions = self.components[index]
                    .component
                    .display_list_blur_regions()
                    .to_vec();
                self.presentation_engine
                    .update_blur_regions(&surface_id, blur_regions);
                self.components[index].target_mut(target).last_region_state = Some(region_state);
            }
        }

        let mut present_damage: Vec<DamageRect> = match target {
            TargetRef::Parent => self.components[index].component.take_present_damage(),
            TargetRef::Child(child_index) => std::mem::take(
                &mut self.components[index].children[child_index].pending_present_damage,
            ),
        };
        present_damage.extend(std::mem::take(
            &mut self.components[index]
                .target_mut(target)
                .pending_present_damage,
        ));
        // Scale change or explicit force-full triggers full-buffer present (per HDPI-04)
        let mut force_full = false;
        let mut hud_restore = None;
        if visible
            && self
                .presentation_engine
                .surface_needs_full_redraw(&surface_id)
        {
            force_full = true;
            self.presentation_engine
                .clear_surface_needs_full_redraw(&surface_id);
            tracing::debug!(
                surface_id = surface_id.as_str(),
                "scale change triggered full-buffer present"
            );
        }
        if visible && self.components[index].target(target).force_full_present {
            force_full = true;
            self.components[index].target_mut(target).force_full_present = false;
        }
        if force_full {
            // Emit full damage in logical coordinates; the present boundary
            // applies the shared edge-coverage policy before copying and
            // reporting protocol damage.
            present_damage = vec![DamageRect {
                x: 0,
                y: 0,
                width: width.max(1),
                height: height.max(1),
            }];
        }
        if visible
            && is_parent
            && self.debug.enabled
            && self.debug.profiling_enabled
            && surface_id != DEBUG_INSPECTOR_SURFACE_ID
        {
            let hud_snapshot = self.profiling.perf_hud_snapshot(&surface_id);
            let flashed_damage = present_damage.clone();
            let buffer = self.components[index]
                .target_mut(target)
                .paint_buffer
                .as_mut()
                .expect("paint buffer initialised");
            hud_restore = Some(self.debug_overlay.paint_performance_hud(
                buffer,
                scale,
                &hud_snapshot,
                &flashed_damage,
            ));
            let hud_unit = scale.round().max(1.0);
            present_damage.push(DamageRect {
                x: 0,
                y: 0,
                width: ((184.0 * hud_unit) / scale.max(f32::EPSILON)).ceil() as u32,
                height: ((70.0 * hud_unit) / scale.max(f32::EPSILON)).ceil() as u32,
            });
        }
        if visible && self.debug.show_layout_bounds {
            let debug_tree = match target {
                TargetRef::Parent => self.components[index].component.last_widget_tree().cloned(),
                TargetRef::Child(child_index) => {
                    let node_key = self.components[index].children[child_index]
                        .node_key
                        .clone();
                    let (pad_left, pad_top, _, _) =
                        self.components[index].children[child_index].content_padding;
                    self.components[index]
                        .component
                        .child_surface_debug_tree(&node_key, (pad_left as f32, pad_top as f32))
                }
            };
            if let Some(tree) = debug_tree {
                let buffer = self.components[index]
                    .target_mut(target)
                    .paint_buffer
                    .as_mut()
                    .expect("paint buffer initialised");
                self.debug_overlay.paint_layout_bounds(&tree, buffer, scale);
                present_damage = vec![DamageRect {
                    x: 0,
                    y: 0,
                    width: width.max(1),
                    height: height.max(1),
                }];
            }
        }
        if visible
            && let Some(element) = self.debug.inspected_element.as_ref()
            && element.get("surface_id").and_then(|value| value.as_str())
                == Some(surface_id.as_str())
            && let Some(bounds) = element.get("bounds")
        {
            let number = |name: &str| {
                bounds
                    .get(name)
                    .and_then(|value| value.as_f64())
                    .unwrap_or(0.0) as f32
            };
            let buffer = self.components[index]
                .target_mut(target)
                .paint_buffer
                .as_mut()
                .expect("paint buffer initialised");
            self.debug_overlay.paint_element_highlight(
                buffer,
                scale,
                (number("x"), number("y"), number("width"), number("height")),
            );
            present_damage = vec![DamageRect {
                x: 0,
                y: 0,
                width: width.max(1),
                height: height.max(1),
            }];
        }

        let mut presented = false;
        let present_started = self.profiling_enabled().then(std::time::Instant::now);
        // An empty `present_damage` means paint produced no changed pixels, so
        // skip the present entirely.
        if !visible || !present_damage.is_empty() {
            let present_result = self.presentation_engine.present_with_damage(
                &surface_id,
                self.components[index].component.id(),
                visible,
                self.components[index]
                    .target(target)
                    .paint_buffer
                    .as_ref()
                    .expect("paint buffer initialised"),
                &present_damage,
            );
            if let Some(restore) = hud_restore.take() {
                restore.restore(
                    self.components[index]
                        .target_mut(target)
                        .paint_buffer
                        .as_mut()
                        .expect("paint buffer initialised"),
                );
            }
            match present_result {
                Ok(PresentStatus::Presented) => presented = true,
                Ok(PresentStatus::NotReady) => {
                    self.components[index]
                        .target_mut(target)
                        .pending_present_damage = present_damage;
                    self.components[index].component.request_paint();
                }
                Ok(PresentStatus::SurfaceMissing) => {
                    let target = self.components[index].target_mut(target);
                    target.pending_present_damage = present_damage;
                    // A compositor close or failed role creation invalidates
                    // the shell's cached accepted config as well as the
                    // pixels. Clearing it lets the next render issue a fresh
                    // create/configure attempt instead of acknowledging every
                    // retry against an object that no longer exists.
                    target.last_surface_config = None;
                    target.known_surface_size = None;
                    target.last_region_state = None;
                    // `last_popup_size` is the creation/reposition gate for
                    // xdg_popup targets. A missing popup object cannot be
                    // repaired by replaying pixels alone; force the next
                    // render to issue configure_popup and create a fresh role.
                    if target.popup_config.is_some() {
                        target.last_popup_size = None;
                    }
                    self.components[index].component.request_paint();
                }
                Err(error) => {
                    // The presentation backend may have rejected a copy or
                    // attach after render damage was taken. Keep the frame at
                    // the shell seam too, so every failure path remains
                    // retryable and no unshown frame is acknowledged.
                    self.components[index]
                        .target_mut(target)
                        .pending_present_damage = present_damage;
                    self.components[index].component.request_paint();
                    return Err(ShellRunError::Presentation(error));
                }
            }
        } else if visible {
            match self.presentation_engine.commit_surface_state(&surface_id) {
                Ok(SurfaceStateStatus::Committed) => presented = true,
                Ok(SurfaceStateStatus::Unchanged) => {}
                Ok(SurfaceStateStatus::NotReady) => {
                    self.components[index].component.request_paint();
                }
                Ok(SurfaceStateStatus::SurfaceMissing) => {
                    let target = self.components[index].target_mut(target);
                    target.last_surface_config = None;
                    target.known_surface_size = None;
                    target.last_region_state = None;
                    if target.popup_config.is_some() {
                        target.last_popup_size = None;
                        target.force_full_present = true;
                    }
                    self.components[index].component.request_paint();
                }
                Err(error) => {
                    self.components[index].component.request_paint();
                    return Err(ShellRunError::Presentation(error));
                }
            }
        }
        let allocation_counters = allocation_started
            .map(|started| mesh_core_debug::allocation::snapshot().saturating_delta(started));
        if let Some(started) = present_started
            && presented
        {
            self.record_surface_profiling_stage(
                &surface_id,
                Some(component_id),
                mesh_core_debug::ProfilingStage::PresentCommit,
                started.elapsed(),
                Some("present"),
            );
        }
        if let Some(started) = total_render_started {
            self.record_surface_profiling_stage(
                &surface_id,
                Some(component_id),
                mesh_core_debug::ProfilingStage::TotalSurfaceRender,
                started.elapsed(),
                Some("rebuild"),
            );
        }
        if let Some(counters) = allocation_counters {
            self.record_surface_allocations(&surface_id, Some(component_id), counters);
        }
        if visible && presented {
            self.record_surface_redraw(&surface_id, Some(component_id), Some("present"));
        }
        Ok(presented)
    }

    fn resolve_dynamic_surface_size(
        &mut self,
        index: usize,
        surface_id: &str,
    ) -> Result<Option<(u32, u32)>, ShellRunError> {
        if let Some(size) = self.presentation_engine.surface_size_if_known(surface_id) {
            let content_size = self.content_size_for_target(index, TargetRef::Parent, size);
            self.components[index].parent.known_surface_size = Some(content_size);
            return Ok(Some(content_size));
        }
        if let Some(size) = self.components[index].parent.known_surface_size {
            return Ok(Some(size));
        }
        let size = self
            .presentation_engine
            .surface_size(surface_id)
            .map_err(ShellRunError::Presentation)?;
        if let Some(size) = size {
            self.components[index].parent.known_surface_size = Some(size);
        }
        Ok(size)
    }

    fn popup_intrinsic_measurement_bound(&self, index: usize) -> Option<(u32, u32)> {
        let parent_surface_id = self.components[index]
            .parent
            .popup_config
            .as_ref()?
            .parent_surface_id
            .as_str();
        let compositor_size = self
            .presentation_engine
            .surface_size_if_known(parent_surface_id);
        let shell_size = self
            .surfaces
            .get(parent_surface_id)
            .map(|surface| (surface.width, surface.height));

        match (shell_size, compositor_size) {
            (Some((shell_width, shell_height)), Some((compositor_width, compositor_height))) => {
                // The shell-side stub carries content dimensions, while the
                // presentation config may include a transparent overlay
                // reserve. Prefer the former whenever it is known and use the
                // compositor only to fill a dynamic/span axis.
                Some((
                    if shell_width > 0 {
                        shell_width
                    } else {
                        compositor_width
                    }
                    .max(1),
                    if shell_height > 0 {
                        shell_height
                    } else {
                        compositor_height
                    }
                    .max(1),
                ))
            }
            (Some(size), _) if size.0 > 0 || size.1 > 0 => Some((size.0.max(1), size.1.max(1))),
            (None, Some(size)) => Some((size.0.max(1), size.1.max(1))),
            (None, None) | (Some(_), None) => None,
        }
    }
}

/// Whether a layer-surface config carries a size the compositor can apply as
/// written.
///
/// A zero dimension is not "not measured yet" on the wire — in layer-shell it
/// means "span the output", and it is only valid when the surface is anchored
/// to both opposing edges of that axis. `layer_protocol_size` therefore has to
/// substitute a size for a zero the anchor does not back: the exclusive zone
/// for a docked surface, and a 1x1 clamp for a floating one (the alternative
/// being an invisible output-spanning input sink). Both substitutions are
/// visible geometry, so a config that would trigger one is held back rather
/// than sent — see the call site.
///
/// This mirrors `layer_surface_request_size`, which is where the deliberate
/// zeros come from: a top/bottom bar spans horizontally, and a docked side rail
/// spans vertically.
pub(in crate::shell) fn layer_configure_size_is_resolved(
    edge: Option<mesh_core_wayland::Edge>,
    exclusive_zone: i32,
    width: u32,
    height: u32,
) -> bool {
    use mesh_core_wayland::Edge;
    // A docked surface asked for its exclusive zone, so resolving a zero axis
    // to that zone gives it the size it named; nothing is invented.
    if exclusive_zone > 0 {
        return true;
    }
    match edge {
        // Anchored to both horizontal edges unconditionally: a zero width is
        // the intended spanning spelling, but the cross axis is content.
        Some(Edge::Top | Edge::Bottom) => height != 0,
        // A floating surface anchors neither axis to both edges, so every zero
        // here stands for an absent measurement.
        Some(Edge::Left | Edge::Right) | None => width != 0 && height != 0,
    }
}
