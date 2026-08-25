use super::*;
use mesh_core_elements::PopoverPlacement;

impl Shell {
    pub(super) fn reconcile_child_surface_requests(
        &mut self,
        index: usize,
        component_id: &str,
        parent_surface_id: &str,
        parent_scale: f32,
        total_render_started: Option<std::time::Instant>,
    ) -> Result<bool, ShellRunError> {
        let requests = self.components[index].component.child_surface_requests();
        if requests.is_empty() && self.components[index].children.is_empty() {
            // Common no-popup frame: avoid allocating key sets and running
            // reconciliation bookkeeping. A dismissed popover only suppresses
            // recreation until its authored request disappears for one frame.
            self.components[index].dismissed_child_surfaces.clear();
            self.components[index].entering_child_node_keys.clear();
            self.components[index]
                .component
                .set_entering_child_keys_from_slice(&[]);
            return Ok(false);
        }
        let requested_keys: SmallVec<[(ChildSurfaceKind, &str); 4]> = requests
            .iter()
            .map(|request| (request.kind, request.node_key.as_str()))
            .collect();
        let requested_node_keys: SmallVec<[&str; 4]> = requests
            .iter()
            .map(|request| request.node_key.as_str())
            .collect();
        self.components[index]
            .entering_child_node_keys
            .retain(|node_key| requested_node_keys.contains(&node_key.as_str()));
        self.components[index]
            .dismissed_child_surfaces
            .retain(|(kind, node_key)| requested_keys.contains(&(*kind, node_key.as_str())));
        let popup_supported = self.presentation_engine.popup_supported();
        let window_supported = self.presentation_engine.window_role_supported();

        let now = std::time::Instant::now();
        let mut child_index = 0;
        while child_index < self.components[index].children.len() {
            let child = &self.components[index].children[child_index];
            let still_requested = requests.iter().any(|request| {
                request.node_key == child.node_key
                    && request.kind == child.kind
                    && child_kind_supported(request.kind, popup_supported, window_supported)
            });
            if still_requested {
                self.components[index].children[child_index].closing_until = None;
                child_index += 1;
                continue;
            }
            let closing_until = self.components[index].children[child_index].closing_until;
            match closing_until {
                Some(until) if until > now => {
                    // Still playing its exit transition; keep the surface
                    // alive and let the closing-repaint pass below animate it.
                    child_index += 1;
                }
                Some(_) => {
                    // Grace period elapsed.
                    self.destroy_child_surface_at(index, child_index);
                }
                None => {
                    let duration = match self.components[index].children[child_index].kind {
                        // A popover has an authored open/close lifecycle and
                        // may keep its last pixels alive for the CSS exit.
                        ChildSurfaceKind::Popover => {
                            let node_key = self.components[index].children[child_index]
                                .node_key
                                .as_str();
                            self.components[index]
                                .component
                                .child_hide_transition_ms(node_key)
                        }
                        // Overflow is a geometry projection. Once the node no
                        // longer escapes the parent, keeping a detached copy
                        // alive would make stale content receive input.
                        ChildSurfaceKind::Overflow | ChildSurfaceKind::Window => 0,
                    };
                    if duration == 0 {
                        self.destroy_child_surface_at(index, child_index);
                    } else {
                        self.components[index].children[child_index].closing_until =
                            Some(now + std::time::Duration::from_millis(duration));
                        child_index += 1;
                    }
                }
            }
        }

        {
            let runtime = &mut self.components[index];
            let closing_keys: SmallVec<[&str; 4]> = runtime
                .children
                .iter()
                .filter(|child| child.closing_until.is_some())
                .map(|child| child.node_key.as_str())
                .collect();
            runtime
                .component
                .set_closing_child_keys_from_slice(&closing_keys);
        }

        let mut any_presented = false;
        for request in &requests {
            if !child_kind_supported(request.kind, popup_supported, window_supported) {
                continue;
            }
            if self.components[index]
                .dismissed_child_surfaces
                .contains(&(request.kind, request.node_key.clone()))
            {
                continue;
            }
            let existing_child = self.components[index]
                .children
                .iter()
                .position(|child| child.node_key == request.node_key);
            if let Some(existing) = existing_child
                && self.components[index].children[existing].kind != request.kind
            {
                self.destroy_child_surface_at(index, existing);
            }
            let existing_child = self.components[index]
                .children
                .iter()
                .position(|child| child.node_key == request.node_key);
            if existing_child.is_none()
                && !self.components[index]
                    .entering_child_node_keys
                    .contains(&request.node_key)
            {
                self.components[index]
                    .entering_child_node_keys
                    .insert(request.node_key.clone());
                let entering = self.components[index].entering_child_node_keys.clone();
                self.components[index]
                    .component
                    .set_entering_child_keys(entering);
                // Defer mapping until the component has rebuilt this subtree
                // with mesh-surface-entering applied. Otherwise the compositor
                // exposes one resting frame and there is nothing to animate.
                continue;
            }
            let child_surface_id = existing_child
                .map(|existing| {
                    self.components[index].children[existing]
                        .target
                        .surface_id
                        .clone()
                })
                .unwrap_or_else(|| child_surface_id(parent_surface_id, &request.node_key));
            let child_ref = if let Some(existing) = existing_child {
                TargetRef::Child(existing)
            } else {
                let mut target =
                    SurfaceTarget::new(child_surface_id.clone(), LayerSurfaceSizePolicy::Flexible);
                if request.kind != ChildSurfaceKind::Window {
                    target.popup_parent_surface = Some(parent_surface_id.to_string());
                }
                let popover_relationship = (request.kind == ChildSurfaceKind::Popover)
                    .then(|| {
                        request
                            .popover_trigger
                            .as_ref()
                            .map(|trigger| PopoverSurfaceRelationship {
                                trigger_surface_id: parent_surface_id.to_string(),
                                trigger_reference: trigger.clone(),
                                popup_surface_id: child_surface_id.clone(),
                                popup_node_key: request.node_key.clone(),
                            })
                    })
                    .flatten();
                target.popover_relationship = popover_relationship.clone();
                target.force_full_present = true;
                self.components[index].children.push(ChildSurface {
                    target,
                    kind: request.kind,
                    node_key: request.node_key.clone(),
                    popover_relationship,
                    anchor_rect: request.anchor_rect,
                    content_padding: request.content_padding,
                    closing_until: None,
                    last_paint_generation: None,
                    last_paint_exiting: None,
                    last_paint_scale_bits: None,
                    last_paint_content_offset: None,
                    pending_present_damage: Vec::new(),
                });
                self.rebuild_component_surface_index();
                TargetRef::Child(self.components[index].children.len() - 1)
            };

            let TargetRef::Child(child_index) = child_ref else {
                unreachable!("child reconcile only creates child targets");
            };
            self.components[index].children[child_index].anchor_rect = request.anchor_rect;
            self.components[index].children[child_index].content_padding = request.content_padding;
            let popover_relationship = (request.kind == ChildSurfaceKind::Popover)
                .then(|| {
                    request
                        .popover_trigger
                        .as_ref()
                        .map(|trigger| PopoverSurfaceRelationship {
                            trigger_surface_id: parent_surface_id.to_string(),
                            trigger_reference: trigger.clone(),
                            popup_surface_id: child_surface_id.clone(),
                            popup_node_key: request.node_key.clone(),
                        })
                })
                .flatten();
            self.components[index].children[child_index].popover_relationship =
                popover_relationship.clone();
            self.components[index].children[child_index]
                .target
                .popover_relationship = popover_relationship;
            if self
                .presentation_engine
                .surface_waiting_for_frame_callback(&child_surface_id)
                || self
                    .presentation_engine
                    .surface_waiting_for_buffer_release(&child_surface_id)
            {
                continue;
            }

            let is_window = request.kind == ChildSurfaceKind::Window;
            // Popovers and overflow surfaces reserve a shadow/filter ring. A
            // promoted widget is a real toplevel, so its buffer is exactly its
            // content and the compositor owns its placement.
            let (pad_left, pad_top, pad_right, pad_bottom) = if is_window {
                (0, 0, 0, 0)
            } else {
                request.content_padding
            };
            let padded_size = (
                request.content_size.0 + pad_left + pad_right,
                request.content_size.1 + pad_top + pad_bottom,
            );
            self.core
                .surfaces
                .entry(child_surface_id.clone())
                .and_modify(|state| {
                    state.visible = true;
                    state.closing_until = None;
                })
                .or_insert(SurfaceState {
                    visible: true,
                    closing_until: None,
                });
            let parent_window = self
                .surfaces
                .get(parent_surface_id)
                .map(|parent| parent.window.clone())
                .unwrap_or_default();
            let surface = self.surfaces.entry(child_surface_id.clone()).or_default();
            surface.visible = true;
            surface.width = padded_size.0.max(1);
            surface.height = padded_size.1.max(1);

            let (width, height) = if is_window {
                surface.role = mesh_core_wayland::SurfaceRole::Window;
                surface.edge = None;
                surface.layer = Some(Layer::Top);
                surface.exclusive_zone = 0;
                surface.keyboard_mode = mesh_core_wayland::KeyboardMode::None;
                surface.window = parent_window;
                let config = SurfaceConfig {
                    role: mesh_core_wayland::SurfaceRole::Window,
                    window: surface.window.clone(),
                    edge: None,
                    layer: Layer::Top,
                    size_policy: LayerSurfaceSizePolicy::Flexible,
                    width: padded_size.0.max(1),
                    height: padded_size.1.max(1),
                    padding: SurfacePadding::default(),
                    exclusive_zone: 0,
                    keyboard_mode: mesh_core_wayland::KeyboardMode::None,
                    namespace: child_surface_id.clone(),
                    margin_top: 0,
                    margin_right: 0,
                    margin_bottom: 0,
                    margin_left: 0,
                    blur: false,
                };
                let changed = self.components[index].children[child_index]
                    .target
                    .last_surface_config
                    .as_ref()
                    != Some(&config);
                if changed {
                    self.presentation_engine
                        .configure(&child_surface_id, config.clone())
                        .map_err(ShellRunError::Presentation)?;
                    self.components[index].children[child_index]
                        .target
                        .last_surface_config = Some(config);
                    self.components[index].children[child_index]
                        .target
                        .known_surface_size = None;
                    self.components[index].children[child_index]
                        .target
                        .last_region_state = None;
                }
                self.presentation_engine
                    .window_configured_size(&child_surface_id)
                    .unwrap_or((padded_size.0.max(1), padded_size.1.max(1)))
            } else {
                // Explicit popovers use their authored placement. Overflow is
                // different: its anchor is the escaped node's own bounds, so
                // it must remain top-left anchored and cannot inherit a
                // popover offset or click grab from a generic request.
                let placement = child_popup_placement(request.kind, request.placement);
                let offset_x = placement.offset_x
                    + axis_padding_compensation(
                        popover_gravity_horizontal_alignment(placement.gravity),
                        pad_left,
                        pad_right,
                    );
                let offset_y = placement.offset_y
                    + axis_padding_compensation(
                        popover_gravity_vertical_alignment(placement.gravity),
                        pad_top,
                        pad_bottom,
                    );
                let grab = placement.grab == PopoverGrab::Click;
                let grab_identity = if grab
                    && self.components[index].children[child_index]
                        .target
                        .popup_config
                        .is_none()
                {
                    self.take_pending_popup_grab(parent_surface_id)
                } else {
                    None
                };
                let popup_config = PopupConfig {
                    parent_surface_id: parent_surface_id.to_string(),
                    // The reserve travels with the padded size it produced, so
                    // the popup's input region is confined to the visible
                    // content and clicks over the shadow ring reach whatever
                    // is behind it.
                    padding: SurfacePadding {
                        left: pad_left,
                        top: pad_top,
                        right: pad_right,
                        bottom: pad_bottom,
                    },
                    placement: PopupPlacement {
                        anchor_rect: request.anchor_rect,
                        size: padded_size,
                        anchor: map_popover_anchor(placement.anchor),
                        gravity: map_popover_gravity(placement.gravity),
                        constraint: map_popover_constraint(placement.constraint_adjustment),
                        offset: (offset_x, offset_y),
                    },
                    grab,
                    grab_identity,
                };
                // The triggering credential is valid only for this creation
                // transaction. Keep the accepted placement in the child
                // cache, but never compare or reuse the one-shot identity on
                // later size/reposition passes.
                let mut accepted_popup_config = popup_config.clone();
                accepted_popup_config.grab_identity = None;
                let (popup_config_changed, popup_needs_configure) = {
                    let child = &self.components[index].children[child_index];
                    let config_changed =
                        child.target.popup_config.as_ref() != Some(&accepted_popup_config);
                    let size_changed = child.target.last_popup_size != Some(padded_size);
                    (config_changed, config_changed || size_changed)
                };
                {
                    let child = &mut self.components[index].children[child_index];
                    child.target.known_surface_size = Some(padded_size);
                    if child.target.last_popup_size != Some(padded_size) {
                        child.target.last_popup_size = Some(padded_size);
                    }
                }
                if popup_needs_configure
                    && let Err(error) = self
                        .presentation_engine
                        .configure_popup(&child_surface_id, popup_config)
                {
                    tracing::warn!("configure_popup for child {child_surface_id} failed: {error}");
                    self.destroy_child_surface_at(index, child_index);
                    // A rejected reposition destroys the old popup so the
                    // next attempt can create it with the requested placement.
                    // Keep the component scheduled even when it has no other
                    // dirty work; otherwise a real component can lose the
                    // popup until an unrelated event causes another render.
                    self.components[index].component.request_paint();
                    continue;
                }
                if popup_config_changed {
                    self.components[index].children[child_index]
                        .target
                        .popup_config = Some(accepted_popup_config);
                }
                padded_size
            };
            let presented = self.paint_and_present_child_surface(
                index,
                child_index,
                component_id,
                width,
                height,
                parent_scale,
                total_render_started,
                false,
            )?;
            any_presented |= presented;

            if self.components[index]
                .entering_child_node_keys
                .remove(&request.node_key)
            {
                let entering = self.components[index].entering_child_node_keys.clone();
                self.components[index]
                    .component
                    .set_entering_child_keys(entering);
            }
        }

        // Popovers whose node dropped out of the open requests this frame but
        // still have exit-transition time left: keep painting/presenting them
        // with the exiting class applied so their CSS exit animation runs
        // before `destroy_child_surface_at` tears the popup down above.
        let closing_indices: Vec<usize> = self.components[index]
            .children
            .iter()
            .enumerate()
            .filter(|(_, child)| {
                child.kind == ChildSurfaceKind::Popover
                    && child.closing_until.is_some()
                    && !requested_keys.contains(&(child.kind, child.node_key.as_str()))
            })
            .map(|(child_index, _)| child_index)
            .collect();
        for child_index in closing_indices {
            let (width, height) = self.components[index].children[child_index]
                .target
                .known_surface_size
                .unwrap_or((1, 1));
            let presented = self.paint_and_present_child_surface(
                index,
                child_index,
                component_id,
                width,
                height,
                parent_scale,
                total_render_started,
                true,
            )?;
            any_presented |= presented;
        }

        Ok(any_presented)
    }

    /// Shared paint+present tail for a child popup surface, used both for
    /// actively open popovers and for ones playing their exit transition
    /// (`exiting = true` appends `mesh-surface-exiting` to the painted
    /// subtree so its CSS transition animates before teardown).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_and_present_child_surface(
        &mut self,
        index: usize,
        child_index: usize,
        component_id: &str,
        width: u32,
        height: u32,
        parent_scale: f32,
        total_render_started: Option<std::time::Instant>,
        exiting: bool,
    ) -> Result<bool, ShellRunError> {
        let allocation_started = (self.profiling_enabled()
            && mesh_core_debug::allocation::profiling_available())
        .then(mesh_core_debug::allocation::snapshot);
        let child_surface_id = self.components[index].children[child_index]
            .target
            .surface_id
            .clone();
        let node_key = self.components[index].children[child_index]
            .node_key
            .clone();

        let scale = self.presentation_engine.surface_scale(&child_surface_id);
        let scale = if scale > 0.0 { scale } else { parent_scale };
        let scale_policy = mesh_core_render::FractionalScale::new(scale);
        let physical_w = scale_policy.physical_extent(width);
        let physical_h = scale_policy.physical_extent(height);

        const MAX_BUFFER_BYTES: u64 = PixelBuffer::MAX_BYTES as u64;
        let requested_bytes = (physical_w as u64) * (physical_h as u64) * 4;
        if requested_bytes > MAX_BUFFER_BYTES {
            return Err(ShellRunError::BufferAlloc {
                surface_id: child_surface_id,
                logical_w: width,
                logical_h: height,
                physical_w,
                physical_h,
                scale,
                requested_bytes,
                max_bytes: MAX_BUFFER_BYTES,
            });
        }

        {
            let child = &mut self.components[index].children[child_index];
            if child
                .target
                .paint_buffer
                .as_ref()
                .map(|buffer| buffer.width() != physical_w || buffer.height() != physical_h)
                .unwrap_or(true)
            {
                let buffer = PixelBuffer::try_new(physical_w, physical_h).ok_or_else(|| {
                    ShellRunError::BufferAlloc {
                        surface_id: child_surface_id.clone(),
                        logical_w: width,
                        logical_h: height,
                        physical_w,
                        physical_h,
                        scale,
                        requested_bytes,
                        max_bytes: MAX_BUFFER_BYTES,
                    }
                })?;
                child.target.paint_buffer = Some(buffer);
                child.last_paint_generation = None;
                child.last_paint_exiting = None;
                child.last_paint_scale_bits = None;
                child.last_paint_content_offset = None;
            }
        }

        let (pad_left, pad_top, ..) = self.components[index].children[child_index].content_padding;
        // `paint_child_surface`'s offset is in the same logical layout units
        // as `-bounds.0`/`-bounds.1` (the renderer applies `scale` to layout
        // + offset together), so this is unscaled padding, not physical px.
        let content_offset = (pad_left, pad_top);
        let paint_generation = self.components[index]
            .component
            .child_surface_paint_generation(&node_key);
        let child_target = &self.components[index].children[child_index].target;
        let has_pending_present =
            !child_target.pending_present_damage.is_empty() || child_target.force_full_present;
        if !has_pending_present
            && child_surface_paint_cache_matches(
                paint_generation,
                self.components[index].children[child_index].last_paint_generation,
                exiting,
                self.components[index].children[child_index].last_paint_exiting,
                scale.to_bits(),
                self.components[index].children[child_index].last_paint_scale_bits,
                content_offset,
                self.components[index].children[child_index].last_paint_content_offset,
            )
        {
            return Ok(false);
        }
        let painted = {
            let runtime = &mut self.components[index];
            let buffer = runtime.children[child_index]
                .target
                .paint_buffer
                .as_mut()
                .expect("child paint buffer initialised");
            runtime
                .component
                .paint_child_surface(&node_key, buffer, scale, content_offset, exiting)
                .map_err(ShellRunError::Component)?
        };
        if !painted {
            self.destroy_child_surface_at(index, child_index);
            return Ok(false);
        }
        let child_damage = self.components[index]
            .component
            .child_surface_present_damage(&node_key);
        self.components[index].children[child_index].last_paint_generation = paint_generation;
        self.components[index].children[child_index].last_paint_exiting = Some(exiting);
        self.components[index].children[child_index].last_paint_scale_bits = Some(scale.to_bits());
        self.components[index].children[child_index].last_paint_content_offset =
            Some(content_offset);
        let child = &mut self.components[index].children[child_index];
        match child_damage {
            Some(damage) => child.pending_present_damage = damage,
            None => {
                child.pending_present_damage.clear();
                child.target.force_full_present = true;
            }
        }
        // Pointer input is confined to the true (unpadded) content rect by the
        // `SurfacePadding` this popup was configured with in
        // `reconcile_child_surface_requests` — the padding exists so
        // shadow/filter overshoot can paint, not to receive input.
        // Frosted popover content declares `backdrop-filter`; hand the regions
        // to the compositor blur protocol like the parent surface path does.
        let child_blur_regions = self.components[index]
            .component
            .child_surface_blur_regions(&node_key);
        self.presentation_engine.update_blur_regions(
            &self.components[index].children[child_index]
                .target
                .surface_id,
            child_blur_regions,
        );
        match self.present_surface_target(
            index,
            TargetRef::Child(child_index),
            component_id,
            width,
            height,
            scale,
            total_render_started,
            allocation_started,
        ) {
            Ok(presented) => Ok(presented),
            Err(ShellRunError::Presentation(error)) => {
                tracing::warn!(
                    "presenting child popup {child_surface_id} failed; destroying popup and keeping parent surface alive: {error}"
                );
                self.destroy_child_surface_at(index, child_index);
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
}

pub(super) fn child_surface_id(parent_surface_id: &str, node_key: &str) -> String {
    let mut encoded = String::with_capacity(node_key.len() * 2);
    for byte in node_key.as_bytes() {
        use std::fmt::Write;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    format!("{parent_surface_id}::child::{encoded}")
}

fn child_kind_supported(
    kind: ChildSurfaceKind,
    popup_supported: bool,
    window_supported: bool,
) -> bool {
    match kind {
        ChildSurfaceKind::Popover | ChildSurfaceKind::Overflow => popup_supported,
        ChildSurfaceKind::Window => window_supported,
    }
}

fn child_popup_placement(kind: ChildSurfaceKind, authored: PopoverPlacement) -> PopoverPlacement {
    match kind {
        ChildSurfaceKind::Popover => authored,
        ChildSurfaceKind::Overflow => PopoverPlacement {
            // An overflow child is a second view of the node's escaped
            // geometry, not an authored anchored menu. Its request already
            // carries the complete node bounds as the anchor rectangle.
            anchor: PopoverAnchor::TopLeft,
            gravity: PopoverGravity::TopLeft,
            offset_x: 0,
            offset_y: 0,
            // Keep the compositor's safety adjustments for an escaped node,
            // but do not allow menu-only click grabs or authored offsets.
            constraint_adjustment: authored.constraint_adjustment,
            grab: PopoverGrab::Hover,
        },
        ChildSurfaceKind::Window => authored,
    }
}

#[inline]
pub(super) fn child_surface_paint_cache_matches(
    generation: Option<u64>,
    cached_generation: Option<u64>,
    exiting: bool,
    cached_exiting: Option<bool>,
    scale_bits: u32,
    cached_scale_bits: Option<u32>,
    content_offset: (u32, u32),
    cached_content_offset: Option<(u32, u32)>,
) -> bool {
    generation.is_some()
        && cached_generation == generation
        && cached_exiting == Some(exiting)
        && cached_scale_bits == Some(scale_bits)
        && cached_content_offset == Some(content_offset)
}
