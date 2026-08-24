mod child_surface;
mod damage;
mod internals;

#[cfg(test)]
mod tests;

use child_surface::*;
use damage::*;

use super::runtime_tree::RetainedTreeDirtySummary;
use super::*;
use crate::shell::component::runtime::{
    merge_reloaded_props, resolved_props_json, script_has_service_read,
};
use crate::shell::{ServiceInterfaceEventSubscription, ServiceObservationSummary};

impl FrontendSurfaceComponent {
    /// Refresh the immutable motion snapshot at a settings/frame boundary.
    /// Existing non-essential work is resolved immediately when reduced motion
    /// becomes active so no stale animation or momentum invalidation survives
    /// the preference change.
    pub(super) fn refresh_motion_policy(&mut self, now: Instant) {
        let next = mesh_core_animation::MotionPolicy::new(self.settings.shell().motion.reduced);
        if next == self.motion_policy {
            return;
        }

        if next.reduced_motion {
            for (key, animation) in self.scroll_animations.drain() {
                self.scroll_offsets.insert(key, animation.target);
            }
            self.scroll_inertia.clear();
        } else if self.tooltip_visible {
            // A tooltip that was already visible should begin its authored
            // enter animation from the preference-change boundary.
            self.tooltip_appeared_at = Some(now);
        }

        self.motion_policy = next;
        self.transitions.clear();
        self.keyframe_animations.clear();
        self.keyframe_rules.clear();
        self.keyframe_animation_slots.clear();
        self.keyframe_animation_lifecycles.clear();
        self.has_active_keyframe_animation = false;
        self.invalidate_style_path(ComponentDirtyFlags::STYLE_RELAYOUT);
    }

    /// Drop every retained render/layout cache so the next paint rebuilds the
    /// tree from scratch. Shared by `theme_changed` and `locale_changed`, which
    /// both invalidate the entire retained pipeline.
    fn reset_render_caches(&mut self) {
        self.clear_component_memo();
        self.last_tree = None;
        self.last_frame_snapshot = None;
        self.interaction_frame.reset();
        self.cached_restyle_rules = None;
        self.cached_restyle_state_dependencies = StyleStateDependencies::default();
        self.cached_style_rule_index = None;
        self.runtime_style_diagnostic_fingerprint = None;
        self.intrinsic_layout_cache = IntrinsicLayoutCache::default();
        self.layout_state = PerSurfaceLayoutState::default();
        self.retained_tree = RetainedWidgetTree::default();
        self.retained_display_list = RetainedDisplayList::default();
        self.pending_service_template_nodes = None;
        self.child_display_lists.get_mut().clear();
        #[cfg(test)]
        {
            self.focused_proof_snapshot = None;
        }
        self.last_visual_damage.clear();
    }

    fn runtime_observes_service_event(
        runtime: &EmbeddedFrontendRuntime,
        event: &ServiceEvent,
    ) -> bool {
        match event {
            ServiceEvent::Updated { service, .. } => {
                let service_name = crate::shell::service::service_name_from_interface_cow(service);
                runtime
                    .script_ctx
                    .has_tracked_fields_for_service(&service_name)
                    || runtime
                        .script_ctx
                        .has_interface_event_subscription_for_service(&service_name)
            }
            ServiceEvent::InterfaceEvent { service, name, .. } => {
                let service_name = crate::shell::service::service_name_from_interface_cow(service);
                runtime
                    .script_ctx
                    .is_subscribed_to_interface_event(&service_name, name)
            }
        }
    }
}

impl ShellComponent for FrontendSurfaceComponent {
    fn id(&self) -> &str {
        &self.compiled.manifest.package.id
    }

    fn surface_id(&self) -> &str {
        &self.surface_id
    }

    fn initial_visibility(&self) -> Option<bool> {
        Some(self.surface_layout.visible_on_start)
    }

    fn mount(&mut self, ctx: ComponentContext) -> Result<Vec<CoreRequest>, ComponentError> {
        if !self.runtimes.lock().unwrap().is_empty() {
            self.unmount_runtimes()?;
        }
        self.diagnostics = Some(ctx.diagnostics);
        self.load_graph_i18n_catalogs();
        self.record_declared_missing_icon_diagnostics();
        if let Err(error) = self.init_root_runtime() {
            let message = self.record_frontend_runtime_issue(
                "initialization",
                self.root_instance_key(),
                &error,
            );
            *self.runtime_failure.borrow_mut() = Some(message);
        }
        self.render_hooks_pending = true;
        self.invalidate_script_state();
        Ok(vec![CoreRequest::PublishDiagnostics {
            message: format!(
                "mounted frontend component '{}' from {}",
                self.id(),
                self.compiled.source_path.display()
            ),
        }])
    }

    fn unmount(&mut self) -> Result<Vec<CoreRequest>, ComponentError> {
        self.unmount_runtimes()?;
        Ok(Vec::new())
    }

    fn handle_core_event(&mut self, event: &CoreEvent) -> Result<Vec<CoreRequest>, ComponentError> {
        if let CoreEvent::SurfaceVisibilityChanged {
            surface_id,
            visible,
        } = event
        {
            // Any surface hiding may have been a popover triggered from
            // this surface — drop its registration so a stale Tab doesn't
            // try to re-enter it.
            if !visible && surface_id != self.surface_id() {
                self.triggered_popovers
                    .retain(|_, target| target != surface_id);
            }
            // Sync portal bookkeeping when an OTHER surface's visibility
            // changes. This handles two cases:
            //   1. Shell hides a popover via Tab transfer — the trigger
            //      surface's Lua may still think the popover is open, so
            //      a click would emit a redundant Hide.
            //   2. Surface shown via a non-portal path (mesh.popover.activate)
            //      bypassing tick()'s bookkeeping — the next tick would
            //      otherwise re-emit a stale HideSurface from the previous
            //      paint's pending_surface_states.
            // Update last_surface_states whenever this component owns a
            // portal binding for the surface (not just when the key was
            // already present), and clear any stale pending state so the
            // next tick's diff is honest.
            if surface_id != self.surface_id() {
                let portal_tracks = self
                    .portal_hidden_bindings
                    .borrow()
                    .contains_key(surface_id);
                if portal_tracks || self.last_surface_states.contains_key(surface_id) {
                    self.last_surface_states
                        .insert(surface_id.clone(), *visible);
                    self.pending_surface_states.borrow_mut().remove(surface_id);
                    let binding = self
                        .portal_hidden_bindings
                        .borrow()
                        .get(surface_id)
                        .cloned();
                    if let Some((owner_instance_key, binding)) = binding {
                        // Target the runtime that actually owns the bound
                        // variable. For a portal declared inside a nested child
                        // component this is the child's instance key, not the
                        // surface root's `self.id()`.
                        let component_id = owner_instance_key;
                        let mut state_dirty = false;
                        let state_error = if let Some(runtime) =
                            self.runtimes.lock().unwrap().get_mut(&component_id)
                        {
                            match runtime
                                .script_ctx
                                .set_member_state(&binding, serde_json::json!(!*visible))
                            {
                                Ok(()) => {
                                    state_dirty = true;
                                    None
                                }
                                Err(source) => Some(source),
                            }
                        } else {
                            None
                        };
                        if let Some(error) = state_error {
                            let _ = self.record_frontend_runtime_issue(
                                "surface visibility update",
                                &component_id,
                                error,
                            );
                        }
                        if state_dirty {
                            self.invalidate_script_state();
                        }
                    }
                }
            }
            if surface_id == self.surface_id() {
                let was_visible = self.visible;
                self.visible = *visible;
                if !visible {
                    self.surface_exiting = false;
                    self.clear_selection();
                    self.reset_interaction_owners();
                    self.input_preedits.clear();
                    self.focused_key = None;
                    self.focus_visible_key = None;
                    self.focused_id = None;
                    self.focus_visible_id = None;
                    self.pending_auto_focus = false;
                    self.pending_embedded_popover_focus = false;
                    self.embedded_popover_return_focus = None;
                    self.pending_embedded_popover_focus_restore = false;
                    self.return_focus = None;
                    self.close_on_focus_leave = false;
                    self.keyboard_mode_override = None;
                    self.gesture_capture = None;
                    self.touch_targets.clear();
                    self.active_touches.clear();
                    self.touch_gestures.clear();
                    self.last_tap = None;
                } else if !was_visible {
                    self.surface_exiting = false;
                    self.surface_pixels_invalid = true;
                    // Hiding shrinks the paint buffer to 1x1 and clears
                    // `known_surface_size` (see runtime/render.rs). A style-only
                    // repaint would reuse that stale 1x1 tree/buffer, so a
                    // surface re-shown without any intervening script change
                    // (e.g. a static language/theme popover with no service
                    // polling to dirty it) would present nothing on its first
                    // frame. Force a full tree rebuild + pixel repaint so the
                    // first shown frame is painted at the real surface size.
                    self.invalidate(ComponentDirtyFlags::TREE_REBUILD);
                    if self.surface_layout.keyboard_mode != KeyboardMode::None {
                        self.pending_auto_focus = true;
                    }
                }
                self.invalidate_surface_config();
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
            source_module,
            payload,
        } = event
        else {
            return self.handle_interface_event(event);
        };
        update_last_service_trace(
            &mut self.last_service_update,
            service,
            source_module,
            tracing::enabled!(tracing::Level::DEBUG),
        );
        let caps = cached_service_capabilities(&mut self.cached_service_capabilities, service);
        let service_name = &caps.service_name;
        let payload_fingerprint = ScriptContext::service_payload_fingerprint(payload);
        let previous_payload = update_cached_service_payload(
            &mut self.cached_service_payloads,
            service_name,
            payload,
            payload_fingerprint,
        );
        let mut needs_rebuild = false;
        let mut runtimes = {
            let mut runtimes = self.runtimes.lock().unwrap();
            std::mem::take(&mut *runtimes)
        };
        for (instance_key, runtime) in runtimes.iter_mut() {
            let observes_event = Self::runtime_observes_service_event(runtime, event);
            let has_read = runtime.script_ctx.can_read_service_interface(service);
            if !has_read && !observes_event {
                self.sync_runtime_generation(
                    instance_key,
                    runtime.script_ctx.state().mutation_generation(),
                );
                continue;
            }
            if !has_read {
                self.sync_runtime_generation(
                    instance_key,
                    runtime.script_ctx.state().mutation_generation(),
                );
                continue;
            }
            runtime.script_ctx.apply_service_payload_with_fingerprint(
                service_name,
                payload,
                payload_fingerprint,
            );
            // Capability is module-wide, so every component instance of a
            // module that declares `service.x.read` is handed the payload —
            // including the five settings pages that never mention it. Once a
            // runtime has evaluated its template, its recorded read sets say
            // whether it can see this update at all: through the service proxy
            // (`observes_event`), through the `<service>` state member, or
            // through the generic `last_service_update` trace. If none of them
            // reach it, install the value without dirtying the instance or
            // advancing the generation its render memoization keys on —
            // otherwise an unrelated 1 Hz poll re-instantiates every page's
            // subtree on every tick.
            let observed = observes_event
                || runtime.script_ctx.observes_state_member(service_name)
                || runtime
                    .script_ctx
                    .observes_state_member("last_service_update");
            if apply_service_update_with_name_and_fingerprint(
                runtime.script_ctx.state_mut(),
                true,
                observed,
                service_name,
                source_module,
                payload,
                payload_fingerprint,
            ) {
                // Template expressions are Luau closures over `_ENV`, so the
                // trace has to land there too — reaching `ScriptState` alone
                // renders `{last_service_update.name}` as an empty string.
                let _ = runtime.script_ctx.seed_context_global(
                    "last_service_update",
                    crate::shell::service::service_update_metadata(service_name, source_module),
                );
            }
            let state_changed = runtime.script_ctx.state().is_dirty();
            if state_changed || {
                let previous = runtime.script_ctx.state().get(service_name);
                runtime.script_ctx.tracked_service_fields_changed(
                    service_name,
                    previous.as_ref(),
                    payload,
                )
            } {
                needs_rebuild = true;
            }
            self.sync_runtime_generation(
                instance_key,
                runtime.script_ctx.state().mutation_generation(),
            );
        }
        *self.runtimes.lock().unwrap() = runtimes;
        if needs_rebuild {
            self.render_hooks_pending = true;
            let narrow_nodes = if let Some(ref prev) = previous_payload {
                let fields = json_field_diff(service_name, prev, payload);
                let mut nodes = HashSet::new();
                for (service, field) in fields {
                    nodes.extend(
                        self.node_service_field_deps
                            .nodes_reading_field(&service, &field),
                    );
                }
                nodes
            } else {
                HashSet::new()
            };
            if narrow_nodes.is_empty() {
                self.invalidate_script_state();
            } else {
                self.invalidate_service_template_nodes(narrow_nodes);
            }
        }
        Ok(Vec::new())
    }

    fn deliver_service_call_result(
        &mut self,
        instance_id: &str,
        call_id: u64,
        status: &str,
        result: &serde_json::Value,
    ) -> bool {
        self.deliver_service_call_result_to_instance(instance_id, call_id, status, result)
    }

    fn cache_service_payload(&mut self, event: &ServiceEvent) {
        let ServiceEvent::Updated {
            service, payload, ..
        } = event
        else {
            return;
        };
        let service_name = crate::shell::service::service_name_from_interface_cow(service);
        if !self.declared_service_names.contains(service_name.as_ref()) {
            return;
        }
        let fingerprint = ScriptContext::service_payload_fingerprint(payload);
        update_cached_service_payload(
            &mut self.cached_service_payloads,
            service_name.as_ref(),
            payload,
            fingerprint,
        );
    }

    fn observes_service_event(&self, event: &ServiceEvent) -> bool {
        let Ok(runtimes) = self.runtimes.lock() else {
            return true;
        };
        runtimes
            .values()
            .any(|runtime| Self::runtime_observes_service_event(runtime, event))
    }

    fn service_observation_summary(&self) -> Option<ServiceObservationSummary> {
        let Ok(runtimes) = self.runtimes.lock() else {
            return None;
        };
        let mut update_services = std::collections::HashSet::new();
        let mut interface_events = std::collections::HashSet::new();
        for runtime in runtimes.values() {
            for (service, fields) in runtime.script_ctx.tracked_service_fields() {
                if !fields.is_empty() {
                    update_services.insert(service);
                }
            }
            for (service, events) in runtime.script_ctx.subscribed_interface_events() {
                if !events.is_empty() {
                    update_services.insert(service.clone());
                    for event in events {
                        interface_events.insert(ServiceInterfaceEventSubscription {
                            service: service.clone(),
                            event,
                        });
                    }
                }
            }
        }
        let mut update_services = update_services.into_iter().collect::<Vec<_>>();
        update_services.sort();
        let mut cached_update_services = self
            .declared_service_names
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        cached_update_services.sort();
        let mut interface_events = interface_events.into_iter().collect::<Vec<_>>();
        interface_events.sort_by(|a, b| {
            a.service
                .cmp(&b.service)
                .then_with(|| a.event.cmp(&b.event))
        });
        Some(ServiceObservationSummary {
            update_services,
            cached_update_services,
            interface_events,
        })
    }

    fn wants_tick(&self) -> bool {
        let tooltip_delay_pending = self.hover_start.is_some() && !self.tooltip_visible;
        let tooltip_fade_pending = self.tooltip_visible
            && self
                .tooltip_appeared_at
                .is_some_and(|appeared| appeared.elapsed() < self.tooltip_fade_duration());
        tooltip_delay_pending
            || tooltip_fade_pending
            || !self.scheduled_handlers.is_empty()
            || self
                .touch_gestures
                .values()
                .any(|touch| touch.eligible && touch.long_press_enabled && !touch.long_press_fired)
            || !self.pending_surface_states.borrow().is_empty()
    }

    fn next_tick_deadline(&self) -> Option<std::time::Instant> {
        if !self.pending_surface_states.borrow().is_empty() {
            return Some(std::time::Instant::now());
        }

        if let Some(deadline) = self
            .scheduled_handlers
            .values()
            .map(|scheduled| scheduled.deadline)
            .min()
        {
            return Some(deadline);
        }

        if let Some(deadline) = self
            .touch_gestures
            .values()
            .filter(|touch| touch.eligible && touch.long_press_enabled && !touch.long_press_fired)
            .map(|touch| touch.started_at + input::LONG_PRESS_DELAY)
            .min()
        {
            return Some(deadline);
        }

        if let Some(start) = self.hover_start
            && !self.tooltip_visible
        {
            return Some(start + Duration::from_millis(self.tooltip_settings.delay_ms));
        }

        if self.tooltip_visible
            && let Some(appeared) = self.tooltip_appeared_at
        {
            const TOOLTIP_FADE_FRAME_INTERVAL: Duration = Duration::from_millis(16);
            let now = std::time::Instant::now();
            let fade_until = appeared + self.tooltip_fade_duration();
            if fade_until > now {
                return Some((now + TOOLTIP_FADE_FRAME_INTERVAL).min(fade_until));
            }
        }

        None
    }

    fn tick(&mut self) -> Result<Vec<CoreRequest>, ComponentError> {
        let now = std::time::Instant::now();
        let due_handlers: Vec<_> = self
            .scheduled_handlers
            .iter()
            .filter(|(_, scheduled)| scheduled.deadline <= now)
            .map(|(key, scheduled)| (key.clone(), scheduled.target.clone()))
            .collect();

        let mut requests = Vec::new();
        for (key, target) in due_handlers {
            self.scheduled_handlers.remove(&key);
            requests.extend(self.call_handler_target(&target, &[])?);
        }

        let due_long_presses = self.due_long_presses(now);
        if !due_long_presses.is_empty()
            && let Some(tree) = self.last_tree.take()
        {
            let result = self.dispatch_due_long_presses(&tree, due_long_presses);
            self.last_tree = Some(tree);
            requests.extend(result?);
        }

        if self.hover_start.is_some() {
            self.refresh_tooltip_settings();
        }

        let tooltip_delay = Duration::from_millis(self.tooltip_settings.delay_ms);
        let tooltip_fade_duration = self.tooltip_fade_duration();

        // Trigger a repaint once the tooltip delay has elapsed so the tooltip appears.
        if let Some(start) = self.hover_start {
            if start.elapsed() >= tooltip_delay && !self.tooltip_visible {
                self.tooltip_visible = true;
                self.tooltip_appeared_at = Some(std::time::Instant::now());
                if !self.dirty && !self.style_only_dirty {
                    self.invalidate_paint();
                }
            }
        }
        // Keep repainting while the tooltip is fading in.
        if let Some(appeared) = self.tooltip_appeared_at {
            if self.tooltip_visible && appeared.elapsed() < tooltip_fade_duration {
                self.invalidate_paint();
            }
        }

        // Emit Show/HideSurface requests for surface portals whose desired visibility changed.
        let pending = std::mem::take(&mut *self.pending_surface_states.borrow_mut());
        self.last_surface_states.reserve(pending.len());
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
        self.dirty
            || self.style_only_dirty
            || !self.transitions.is_empty()
            || self.has_active_keyframe_animation
            || !self.scroll_animations.is_empty()
            || !self.scroll_inertia.is_empty()
            || !self.closing_child_keys.is_empty()
            || !self.entering_child_keys.is_empty()
    }

    fn request_paint(&mut self) {
        self.invalidate_paint();
    }

    fn surface_size_changed(&mut self, width: u32, height: u32) -> bool {
        self.observe_surface_size(width, height)
    }

    fn surface_window_states_changed(&mut self, states: WindowStates) -> bool {
        // `windowed` is MESH's own decision rather than one of the states the
        // compositor sends, so it is carried over instead of being derived from
        // `states` — a configure must not silently demote the surface.
        self.observe_window_states(WindowSurfaceState {
            windowed: self.window_states.windowed,
            fullscreen: states.fullscreen,
            maximized: states.maximized,
            activated: states.activated,
            tiled: states.tiled,
        })
    }

    fn surface_role_changed(&mut self, role: mesh_core_wayland::SurfaceRole) -> bool {
        let windowed = role == mesh_core_wayland::SurfaceRole::Window;
        self.surface_layout.role = role;
        // Leaving window role drops the compositor states with it: a layer
        // surface is never fullscreen, maximized, activated, or tiled, and
        // keeping the last window's flags would strand the component in its
        // filling style. The fresh toplevel's real states arrive with its first
        // configure.
        let restyled = self.observe_window_states(WindowSurfaceState {
            windowed,
            ..WindowSurfaceState::default()
        });
        // `observe_window_states` invalidates style/layout/paint but not the
        // surface config, and the config is the whole point here: without this
        // flag `render` skips `render_layout`, the shell-surface record keeps the
        // old role, and the presentation layer is never asked to swap the
        // compositor object.
        self.invalidate(ComponentDirtyFlags::SURFACE_CONFIG);
        restyled
    }

    fn surface_role(&self) -> mesh_core_wayland::SurfaceRole {
        self.surface_layout.role
    }

    fn surface_promotable(&self) -> bool {
        self.surface_layout.promotable
    }

    fn render(&mut self, surface: &mut dyn ShellSurface) -> Result<(), ComponentError> {
        if self.should_update_surface_config_on_render() {
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
            let role = root_accessibility_role(&self.compiled.manifest)
                .unwrap_or_else(|| "unknown".into());

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
        }

        Ok(())
    }

    fn paint(
        &mut self,
        theme: &Theme,
        extent: SurfaceExtent,
        buffer: &mut PixelBuffer,
        scale: f32,
    ) -> Result<(), ComponentError> {
        let _span = tracing::debug_span!("paint", surface = %self.id()).entered();
        self.prepare_interaction_frame();
        // Capture and clear dirty flags up front. paint is the work-doer; if
        // anything during paint (measured_size change, active animation) needs
        // another frame, it sets a flag again and wants_render picks it up.
        let (requested_width, requested_height) = self.requested_layout_size();
        // The fallback for an unmeasured surface is the *content* extent, never
        // the padded one. When these were one argument, a surface that had not
        // measured itself yet fell back to the padded surface and laid its first
        // frame out — and recorded `last_surface_size` — against the tooltip
        // reserve, e.g. 1920x201 for a 1920x56 bar.
        let content_width = if requested_width == 0 {
            extent.content_width().max(1)
        } else {
            requested_width.max(1)
        };
        let content_height = if requested_height == 0 {
            extent.content_height().max(1)
        } else {
            requested_height.max(1)
        };
        // An axis with neither a measurement of its own nor a size from the
        // shell has no layout box yet. The numbers above still stand in for it
        // (the buffer and the damage rects need real pixels), but the surface
        // root lays it out as `auto` so this paint measures what the content
        // wants instead of what the placeholder allows — a 56px bar in a 1px
        // stand-in surface measures 1px, and `render_layout` would then send
        // that 1px on to the compositor as the bar's height, for good.
        self.unmeasured_root_axes = (
            requested_width == 0 && !extent.content_width_known(),
            requested_height == 0 && !extent.content_height_known(),
        );
        // Observe the content size BEFORE snapshotting dirty flags: a size
        // change raises STYLE|LAYOUT|PAINT|METRICS, and this very paint is the
        // one that rebuilds at the new size — snapshotting first would leave
        // those flags pending and burn one extra full frame per resize.
        self.observe_surface_size(content_width, content_height);
        // Run render hooks BEFORE the dirty flags are snapshotted, so a hook
        // that does invalidate lands in this frame rather than costing an
        // extra one, and so the path decision below can see whether the hooks
        // changed anything a template reads.
        self.run_pending_render_hooks();
        let animation_only_frame = std::mem::take(&mut self.animation_only_dirty);
        let (requires_tree_rebuild, can_use_retained_path, dirty_types, _) =
            self.take_dirty_for_paint();
        self.record_component_frame_dirty(dirty_types);
        let paint_width = extent.padded_width().max(content_width).max(1);
        let paint_height = extent.padded_height().max(content_height).max(1);
        // The paint buffer's physical size tracks `paint_width`/`paint_height`
        // (content plus the tooltip overlay reserve, times scale), not the
        // content-only dimensions `observe_surface_size` watches above. A
        // tooltip appearing/disappearing changes the buffer size without
        // changing content size, so the caller (`Shell::render_components`)
        // reallocates a fresh, zeroed `PixelBuffer` that `observe_surface_size`
        // never notices. Without forcing a full repaint here, only the dirty
        // diff gets drawn into the new buffer and everything else stays
        // transparent.
        if self.last_painted_buffer_size != Some((buffer.width(), buffer.height())) {
            self.surface_pixels_invalid = true;
        }
        self.last_painted_buffer_size = Some((buffer.width(), buffer.height()));
        let use_retained_style_path = !requires_tree_rebuild
            && can_use_retained_path
            && self.last_tree.is_some()
            && !self.render_hooks_pending;
        let run_style_animation_pass = self.should_run_style_animation_pass();
        let previous_visual_styles = if run_style_animation_pass && self.last_tree.is_some() {
            self.take_previous_visual_styles()
        } else {
            Default::default()
        };
        let surface_css_props = self.surface_css_props();
        let mut tree_was_rebuilt = false;
        let mut tree = if dirty_types.contains(ComponentDirtyFlags::SCRIPT_NARROW) {
            tree_was_rebuilt = true;
            self.narrow_script_update(theme, content_width, content_height, &surface_css_props)
        } else if use_retained_style_path {
            match self.restyle_retained_tree(
                theme,
                content_width,
                content_height,
                dirty_types,
                animation_only_frame,
                &surface_css_props,
            ) {
                Some(t) => t,
                None => {
                    tree_was_rebuilt = true;
                    self.build_tree_with_surface_css_props(
                        theme,
                        content_width,
                        content_height,
                        &surface_css_props,
                    )
                }
            }
        } else {
            tree_was_rebuilt = true;
            self.build_tree_with_surface_css_props(
                theme,
                content_width,
                content_height,
                &surface_css_props,
            )
        };
        self.prune_stale_interaction_targets(&tree);
        self.apply_pending_auto_focus(&tree);
        self.apply_pending_embedded_popover_focus(&tree);
        let mut animation_dirty_roots = if run_style_animation_pass {
            self.apply_style_animations_with_previous(
                &mut tree,
                &previous_visual_styles,
                &surface_css_props,
            )
        } else {
            HashSet::new()
        };
        if run_style_animation_pass {
            self.restore_previous_visual_styles(previous_visual_styles);
        }
        self.record_interaction_frame_dirty(
            animation_dirty_roots.iter().copied(),
            mesh_core_interaction::InteractionDirtyFlags::ANIMATION,
        );
        let mut retained_update_dirty_roots = self.retained_update_dirty_roots.take();
        #[cfg(test)]
        if self.force_full_retained_update {
            retained_update_dirty_roots = None;
        }
        if let Some(dirty_roots) = retained_update_dirty_roots.as_mut() {
            dirty_roots.extend(animation_dirty_roots.drain());
        }
        self.animation_dirty_node_ids_scratch = animation_dirty_roots;
        self.advance_interaction_frame(
            mesh_core_interaction::InteractionFramePhase::AnimationSampled,
        );
        // Render fingerprints are synchronized inside the authoritative
        // retained-tree pass, so this stage now covers that consolidated work.
        let render_object_started = std::time::Instant::now();
        let retained_dirty = if let Some(dirty_roots) = retained_update_dirty_roots.as_ref() {
            self.retained_tree
                .update_for_dirty_roots(&tree, dirty_roots)
        } else {
            self.retained_tree.update(&tree)
        };
        let retained_tree_generation = self.retained_tree.generation();
        let render_object_dirty = self.retained_tree.render_dirty();
        self.record_profiling_stage_with_elapsed(
            mesh_core_debug::ProfilingStage::RenderObjectSync,
            render_object_started.elapsed(),
            Some("rebuild"),
        );
        if tree_was_rebuilt {
            self.record_runtime_style_diagnostics_after_retained_update(
                &mut tree,
                theme,
                retained_tree_generation,
                content_width,
                content_height,
                &surface_css_props,
            );
        }

        let tooltip = self.compute_tooltip_state(
            theme,
            &tree,
            retained_tree_generation,
            paint_width,
            paint_height,
        );

        let surface_damage = DamageRect {
            x: 0,
            y: 0,
            width: paint_width,
            height: paint_height,
        };
        let content_damage = DamageRect {
            x: 0,
            y: 0,
            width: content_width.max(1),
            height: content_height.max(1),
        };
        let display_list_started = std::time::Instant::now();
        let display_list_span = tracing::debug_span!("display_list_update").entered();
        let display_list_metrics = self.retained_display_list.update_for_retained_generation(
            &tree,
            retained_tree_generation,
            render_object_dirty,
            self.retained_tree.render_dirty_node_ids(),
            content_width,
            content_height,
            self.surface_pixels_invalid,
            true,
        );
        drop(display_list_span);
        self.record_profiling_stage_with_elapsed(
            mesh_core_debug::ProfilingStage::RetainedDisplayListUpdate,
            display_list_started.elapsed(),
            Some("rebuild"),
        );
        let current_tooltip_damage =
            tooltip_damage_rect(tooltip.as_ref(), paint_width, paint_height);
        let mut tooltip_damage_rects = std::mem::take(&mut self.tooltip_damage_scratch);
        damage_rects_from_options_into(
            [current_tooltip_damage, self.last_tooltip_damage],
            surface_damage,
            &mut tooltip_damage_rects,
        );
        let mut dirty_node_visual_damage_rects =
            std::mem::take(&mut self.dirty_node_visual_damage_scratch);
        damage_rects_for_node_ids_into(
            &tree,
            self.retained_tree.render_dirty_node_ids(),
            &self.last_visual_damage,
            content_damage,
            &mut dirty_node_visual_damage_rects,
        );
        let mut visual_damage_rects = std::mem::take(&mut self.visual_damage_scratch);
        visual_damage_rects.clear();
        if render_object_dirty.reordered > 0
            || render_object_dirty.transform > 0
            || render_object_dirty.opacity > 0
            || render_object_dirty.material > 0
        {
            merge_damage_rects(
                &mut visual_damage_rects,
                dirty_node_visual_damage_rects.iter().copied(),
                surface_damage,
            );
        }
        let effective_damage_rects = std::mem::take(&mut self.effective_damage_scratch);
        let mut effective_damage = select_effective_damage_rects(
            display_list_metrics,
            self.retained_display_list.damage_rects(),
            surface_damage,
            requires_tree_rebuild,
            &visual_damage_rects,
            &tooltip_damage_rects,
            effective_damage_rects,
        );
        // Blur regions are all-or-nothing: a backdrop-filter node re-reads the
        // pixels beneath it and a `filter: blur()` layer resolves as one image,
        // so damage touching either must repaint the whole region or the blur
        // would mix freshly painted and stale-frame pixels.
        if !effective_damage.full_surface
            && self
                .retained_display_list
                .expand_damage_for_blur_regions(&mut effective_damage.rects)
        {
            effective_damage.rect = bounding_damage_rect(&effective_damage.rects, surface_damage);
        }
        let _paint_damage = if effective_damage.full_surface {
            Some(surface_damage)
        } else {
            effective_damage.rect
        };
        {
            let measured_size = measure_content_size(&tree, content_width, content_height);
            if self.measured_size != Some(measured_size) {
                self.measured_size = Some(measured_size);
                // Only schedule a surface reconfigure when the measurement
                // actually disagrees with the size this paint laid out
                // against. `observe_surface_size` clears `measured_size`
                // whenever the available size changes, so without this guard
                // the None → Some re-measure after a self-inflicted resize
                // (content measured smaller, surface reconfigured to match)
                // would invalidate one extra frame per settle and oscillating
                // tests/surfaces would never converge.
                if measured_size != (content_width, content_height) {
                    self.invalidate_surface_config();
                }
            }
        }
        // Element metrics depend on geometry plus ref/id/scroll attributes,
        // not paint-only style or interaction state. Avoid rebuilding and
        // fingerprinting the full JSON snapshot when the retained diff proves
        // those inputs are unchanged.
        let element_metrics_changed = retained_dirty_affects_element_metrics(retained_dirty);
        if self.element_metric_usage.any() && element_metrics_changed {
            self.publish_element_metrics(&tree, self.element_metric_usage);
        }

        let effective_damage_area = effective_damage.damage_area(display_list_metrics.surface_area);
        let paint_bounding_rect = matches!(
            effective_damage.policy,
            DisplayListRepaintPolicy::BoundingRect
        ) && effective_damage.rects.len() > 1
            && effective_damage.rect.is_some_and(|damage| {
                effective_damage_area > 0
                    && damage.area() <= effective_damage_area.saturating_mul(3)
            });
        let selected_paint = if paint_bounding_rect {
            self.retained_display_list
                .select_paint_commands(effective_damage.rect, effective_damage.policy)
        } else {
            self.retained_display_list
                .select_paint_commands_for_rects(&effective_damage.rects, effective_damage.policy)
        };
        #[cfg(test)]
        {
            let focused_proof_snapshot = mesh_core_render::build_focused_proof_snapshot(
                &tree,
                render_object_dirty,
                display_list_metrics,
                &selected_paint,
            );
            for diagnostic in &focused_proof_snapshot.diagnostics {
                self.record_focused_proof_diagnostic(diagnostic);
            }
            self.focused_proof_snapshot = Some(focused_proof_snapshot);
        }
        let narrow_path = self.narrow_path_active;
        let affected_count = self.affected_node_count;
        self.narrow_path_active = false;
        self.affected_node_count = 0;
        self.invalidation_snapshot = Some(mesh_core_debug::ProfilingInvalidationSnapshot {
            full_rebuild: requires_tree_rebuild,
            retained_path: use_retained_style_path,
            retained_generation: self.retained_tree.generation(),
            component: dirty_types.to_debug_counts(),
            retained: retained_dirty.to_debug_counts(),
            paint: retained_paint_snapshot(selected_paint.metrics(), &effective_damage),
            text: mesh_core_debug::TextCacheSnapshot::default(),
            narrow_path,
            affected_node_count: affected_count,
        });
        tracing::trace!(
            "retained widget tree '{}' generation={} dirty={:?}",
            self.id(),
            self.retained_tree.generation(),
            retained_dirty
        );
        tracing::trace!(
            "component '{}' invalidation={:?} retained_path={}",
            self.id(),
            dirty_types,
            use_retained_style_path
        );
        tracing::trace!(
            "retained render fingerprints '{}' generation={} dirty={:?}",
            self.id(),
            retained_tree_generation,
            render_object_dirty
        );

        let paint_started = std::time::Instant::now();
        let paint_metrics = self.paint_pixel_regions(
            theme,
            buffer,
            scale,
            &selected_paint,
            &effective_damage,
            paint_bounding_rect,
            tooltip.as_ref(),
            current_tooltip_damage,
        );
        self.advance_interaction_frame(mesh_core_interaction::InteractionFramePhase::PaintReady);
        if effective_damage.full_surface {
            self.last_present_damage_rects.clear();
            self.last_present_damage_rects.push(surface_damage);
        } else if !effective_damage.rects.is_empty() {
            for &rect in &effective_damage.rects {
                push_damage_rect(&mut self.last_present_damage_rects, rect, surface_damage);
            }
        }
        // When effective_damage.rects is empty, leave last_present_damage_rects unchanged
        // (accumulates across immediate-rerender passes, matching old merge_optional_damage behaviour)
        self.last_visual_damage = collect_visual_damage_rects(&tree, content_damage);
        let traversal_micros = paint_metrics
            .traversal_micros
            .saturating_sub(paint_metrics.text.shaping_micros)
            .saturating_sub(paint_metrics.icon_image_raster_micros);
        self.record_profiling_stage_with_elapsed(
            mesh_core_debug::ProfilingStage::PaintTraversal,
            std::time::Duration::from_micros(traversal_micros),
            Some("rebuild"),
        );
        self.record_profiling_stage_with_elapsed(
            mesh_core_debug::ProfilingStage::TextShaping,
            std::time::Duration::from_micros(paint_metrics.text.shaping_micros),
            Some("rebuild"),
        );
        self.record_profiling_stage_with_elapsed(
            mesh_core_debug::ProfilingStage::IconImageRaster,
            std::time::Duration::from_micros(paint_metrics.icon_image_raster_micros),
            Some("rebuild"),
        );
        for class in mesh_core_render::PaintCommandClass::ALL {
            let metrics = paint_metrics.command_attribution.get(class);
            if metrics.command_count == 0 {
                continue;
            }
            self.record_profiling_stage_with_elapsed(
                mesh_core_debug::ProfilingStage::PaintTraversal,
                std::time::Duration::from_micros(metrics.elapsed_micros),
                Some(&format!("attribution:paint_command:{}", class.label())),
            );
        }
        if let Some(snapshot) = self.invalidation_snapshot.as_mut() {
            snapshot.text = text_cache_snapshot(paint_metrics.text);
            snapshot.paint.raster_cache_hits = paint_metrics.raster_cache_hits;
            snapshot.paint.raster_cache_misses = paint_metrics.raster_cache_misses;
            snapshot.paint.raster_cache_bypasses = paint_metrics.raster_cache_bypasses;
            snapshot.paint.raster_cache_opaque_hits = paint_metrics.raster_cache_opaque_hits;
            snapshot.paint.raster_cache_translucent_hits =
                paint_metrics.raster_cache_translucent_hits;
            snapshot.paint.glyph_cache_hits = paint_metrics.glyph_cache_hits;
            snapshot.paint.glyph_cache_misses = paint_metrics.glyph_cache_misses;
            snapshot.paint.glyph_cache_entries = paint_metrics.glyph_cache_entries;
            snapshot.paint.glyph_cache_capacity = paint_metrics.glyph_cache_capacity;
            snapshot.paint.font_bytes_cache_hits = paint_metrics.font_bytes_cache_hits;
            snapshot.paint.font_bytes_cache_misses = paint_metrics.font_bytes_cache_misses;
            snapshot.paint.font_bytes_cache_entries = paint_metrics.font_bytes_cache_entries;
            snapshot.paint.font_bytes_cache_capacity = paint_metrics.font_bytes_cache_capacity;
            snapshot.paint.skia_glyph_cache_hits = paint_metrics.skia_glyph_cache_hits;
            snapshot.paint.skia_glyph_cache_misses = paint_metrics.skia_glyph_cache_misses;
            snapshot.paint.skia_glyph_cache_entries = paint_metrics.skia_glyph_cache_entries;
            snapshot.paint.skia_glyph_cache_capacity = paint_metrics.skia_glyph_cache_capacity;
        }
        if self.profiling_enabled {
            mesh_core_debug::allocation::with_tracking_suspended(|| {
                self.profiling_records
                    .borrow_mut()
                    .push(ComponentProfilingRecord {
                        stage: mesh_core_debug::ProfilingStage::Paint,
                        duration: paint_started.elapsed(),
                        module_id: Some(self.compiled.manifest.package.id.clone()),
                        trigger_kind: Some("rebuild".to_string()),
                    });
            });
        }
        self.tooltip_damage_scratch = tooltip_damage_rects;
        self.dirty_node_visual_damage_scratch = dirty_node_visual_damage_rects;
        self.visual_damage_scratch = visual_damage_rects;
        self.effective_damage_scratch = std::mem::take(&mut effective_damage.rects);
        self.last_tree = Some(tree);
        self.advance_interaction_frame(
            mesh_core_interaction::InteractionFramePhase::SemanticsReady,
        );
        self.last_tooltip_damage = current_tooltip_damage;
        self.surface_pixels_invalid = false;
        self.clear_runtime_dirty_states();
        if self.surface_entering {
            self.surface_entering = false;
            // A top-level layer surface reopens with one controlled entering
            // frame, then immediately repaints without the marker. Reusing
            // that transient tree as the "previous visual style" bootstrap
            // source lets global width/height transitions animate from the
            // entering frame's temporary geometry, which visibly squashes the
            // first settled frame of drawers like the debug inspector. Drop
            // the snapshot for the follow-up repaint so it snaps straight to
            // the resting layout. Promoted popups keep the snapshot because
            // their second frame intentionally transitions from the entering
            // pose into place.
            if !self.popup_promoted {
                self.last_tree = None;
                self.last_frame_snapshot = None;
            }
            self.invalidate_script_state();
        }

        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), ComponentError> {
        // Theme tokens drive every styled property. Drop every retained cache
        // so the next paint rebuilds the tree from scratch with the new theme,
        // and force a full pixel-buffer repaint so the selective-damage path
        // cannot skip the present.
        tracing::debug!("theme_changed for component '{}'", self.id());
        self.active_theme_stale.set(true);
        self.reset_render_caches();
        // A theme swap is a global palette replacement, not a local CSS
        // transition. Drop transition state so stale light/dark colors cannot
        // paint over the newly active theme.
        self.transitions.clear();
        // Preserve keyframe timelines, but rebuild token-resolved rules.
        self.keyframe_rules.clear();
        self.render_hooks_pending = true;
        self.surface_pixels_invalid = true;
        self.invalidate_script_state();
        Ok(())
    }

    fn locale_changed(&mut self, locale: &LocaleEngine) -> Result<(), ComponentError> {
        tracing::debug!("locale_changed for component '{}'", self.id());
        if self.locale_catalog_is_shared {
            self.locale
                .replace_catalog_snapshot(locale.catalog_snapshot());
        }
        self.locale.replace_selection(locale.selection());
        let selection = locale.selection();
        let payload = serde_json::json!({
            "locale": locale.current(),
            "current": locale.current(),
            "chain": selection.chain(),
            "direction": selection.direction().as_str(),
            "revision": selection.revision().to_string(),
        });
        let mut generations = Vec::new();
        for (instance_key, runtime) in self.runtimes.lock().unwrap().iter_mut() {
            let translator = self.locale.module_translator(&runtime.script_ctx.module_id);
            runtime.script_ctx.set_i18n_translator(&translator);
            runtime.script_ctx.apply_service_payload("locale", &payload);
            if script_has_service_read(&runtime.script_ctx, "mesh.locale", "locale") {
                apply_service_update_with_name(
                    runtime.script_ctx.state_mut(),
                    true,
                    "locale",
                    "@mesh/shell",
                    &payload,
                );
            }
            generations.push((
                instance_key.clone(),
                runtime.script_ctx.state().mutation_generation(),
            ));
        }
        for (instance_key, generation) in generations {
            self.sync_runtime_generation(&instance_key, generation);
        }
        self.reset_render_caches();
        self.render_hooks_pending = true;
        self.surface_pixels_invalid = true;
        self.invalidate_script_state();
        Ok(())
    }

    fn source_path(&self) -> Option<&Path> {
        Some(self.compiled.source_path.as_path())
    }

    fn watched_source_paths(&self) -> Vec<PathBuf> {
        self.compiled.watched_paths.clone()
    }

    fn apply_settings(
        &mut self,
        settings: &Arc<mesh_core_config::SettingsStore>,
    ) -> Result<bool, ComponentError> {
        self.settings = settings.clone();
        self.refresh_motion_policy(Instant::now());
        let settings_state = resolve_frontend_module_settings_with_props(
            &self.settings_namespace,
            self.settings.namespace(&self.settings_namespace),
            &self.compiled.manifest,
            self.compiled.component.props.as_ref(),
        );
        // Only what this save introduced: re-reporting the rest on every write
        // would bury the one line the user is trying to fix.
        mesh_core_config::log_settings_diagnostics(
            "settings reload",
            &mesh_core_config::new_settings_diagnostics(
                &self.settings_diagnostics,
                &settings_state.diagnostics,
            ),
        );
        self.settings_diagnostics = settings_state.diagnostics;
        let layout_changed = self.surface_layout != settings_state.layout;
        let settings_changed = self.settings_json != settings_state.effective;

        self.surface_layout = settings_state.layout;
        self.settings_json = settings_state.effective;

        if settings_changed {
            if let Some(runtime) = self
                .runtimes
                .lock()
                .unwrap()
                .get_mut(self.root_instance_key())
            {
                let next_host_props = resolved_props_json(
                    &self.compiled.component,
                    &HashMap::new(),
                    &self.settings_json,
                    self.root_instance_key(),
                );
                let merged_props = merge_reloaded_props(
                    runtime.script_ctx.state().get_ref("props"),
                    &runtime.host_props,
                    &next_host_props,
                );
                runtime.host_props = next_host_props;
                if let Err(error) = runtime.script_ctx.set_member_state("props", merged_props) {
                    tracing::warn!(
                        "failed to refresh component props after settings reload: {error}"
                    );
                }
                Self::normalize_script_props(&self.diagnostics, runtime);
            }
            if let Some(generation) = self
                .runtimes
                .lock()
                .unwrap()
                .get(self.root_instance_key())
                .map(|runtime| runtime.script_ctx.state().mutation_generation())
            {
                self.sync_runtime_generation(self.root_instance_key(), generation);
            }
        }

        if layout_changed || settings_changed {
            self.invalidate_surface_config();
        }
        Ok(layout_changed || settings_changed)
    }

    fn reload_source(&mut self) -> Result<bool, ComponentError> {
        let manifest = self.compiled.manifest.clone();
        let component_id = self.id().to_string();
        self.frontend_catalog_handle
            .reload_module(&component_id, &manifest, &self.module_dir)
            .map_err(|err| ComponentError::Failed {
                component_id: component_id.clone(),
                message: format!("frontend recompile failed: {err}"),
            })?;
        let recompiled = self
            .frontend_catalog_handle
            .snapshot()
            .catalog
            .module(&component_id)
            .map(|entry| entry.compiled.clone())
            .ok_or_else(|| ComponentError::Failed {
                component_id: component_id.clone(),
                message: "frontend recompile omitted the primary catalog entry".into(),
            })?;

        self.compiled = recompiled;
        self.selective_service_build_supported = self.compiled.supports_selective_service_build();
        self.element_metric_usage = element_metric_usage(&self.compiled);
        self.unmount_runtimes()?;
        self.frontend_catalog_changed();
        self.clear_runtime_generation_index();
        if let Err(error) = self.init_root_runtime() {
            let message = self.record_frontend_runtime_issue(
                "replacement initialization",
                self.root_instance_key(),
                &error,
            );
            *self.runtime_failure.borrow_mut() = Some(message);
        }
        self.render_hooks_pending = true;
        self.invalidate_script_state();
        // Prepared local-component rules own cloned selectors and declarations
        // from the previous compilation. Keeping them would rebuild the fresh
        // template against stale CSS after a hot reload.
        self.prepared_component_styles.get_mut().clear();
        // Source reload may change structure, styles, scripts, local imports,
        // or render-object identities. Drop every retained render/layout cache
        // so the next paint starts from the newly compiled module rather than
        // diffing against the stale tree from the previous source version.
        self.reset_render_caches();
        Ok(true)
    }

    fn frontend_catalog_changed(&mut self) -> bool {
        let state = self.frontend_catalog_handle.snapshot();
        if state.version == self.frontend_catalog_version {
            return false;
        }

        self.frontend_catalog = state.catalog;
        self.frontend_catalog_version = state.version;
        if !state.affected_modules.contains(self.id()) {
            return false;
        }

        let runtimes = std::mem::take(&mut *self.runtimes.lock().unwrap());
        let mut retained = HashMap::with_capacity(runtimes.len());
        for (instance_key, mut runtime) in runtimes {
            if state
                .changed_modules
                .contains(&runtime.script_ctx.module_id)
            {
                if let Err(error) =
                    Self::dispatch_runtime_hook(&self.diagnostics, &mut runtime, "unmount")
                {
                    tracing::warn!(
                        component_id = %runtime.module_id,
                        error = %error,
                        "frontend catalog replacement unmount failed"
                    );
                }
                runtime.script_ctx.drain_published_events();
                runtime.script_ctx.drain_element_actions();
            } else {
                retained.insert(instance_key, runtime);
            }
        }
        *self.runtimes.lock().unwrap() = retained;
        self.rebuild_runtime_generation_index();
        self.prepared_component_styles
            .get_mut()
            .retain(|module_id, _| !state.changed_modules.contains(module_id));
        self.declared_service_names =
            declared_service_names(&self.compiled, &self.frontend_catalog);

        // Instance-derived composition bookkeeping is rebuilt with the tree.
        // Keeping any of it would leave dead slot/import identities reachable
        // after a contribution or dependency changes.
        self.composition_occurrences.get_mut().clear();
        self.bound_children.get_mut().clear();
        self.portal_hidden_bindings.get_mut().clear();
        self.pending_surface_states.get_mut().clear();
        self.ref_node_keys.get_mut().clear();
        self.scheduled_handlers.clear();
        self.render_hooks_pending = true;
        self.reset_render_caches();
        self.invalidate_script_state();
        true
    }

    fn handle_input(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        input: ComponentInput,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        self.handle_component_input(theme, width, height, input)
    }

    fn handle_child_surface_input(
        &mut self,
        node_key: &str,
        theme: &Theme,
        width: u32,
        height: u32,
        content_offset: (f32, f32),
        input: ComponentInput,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(tree) = self.last_tree.as_ref() else {
            return Ok(Vec::new());
        };
        let Some(node) = find_node_by_key(tree, node_key) else {
            return Ok(Vec::new());
        };
        let Some(bounds) = find_node_bounds_by_key(tree, node_key, 0.0, 0.0) else {
            return Ok(Vec::new());
        };
        // The promoted popover is laid out in-flow under its trigger (often extending
        // off the parent surface), but presented as a separate popup. Hit-testing the
        // parent tree would clip the off-surface content and is blocked by the popover's
        // `hidden` wrapper, so instead run input against the popover subtree in isolation
        // — offset to local origin exactly like `paint_child_surface` — using the
        // popup-local coordinates directly. Node `_mesh_key`s are identical to the real
        // tree, so the hover state and dispatched handlers (`onpointerenter` /
        // `onpointerleave`, option clicks) stay consistent after the real tree is
        // restored.
        //
        // The child buffer is padded so descendant shadow/transform overshoot
        // has pixels to paint into, and `paint_child_surface` places the
        // subtree at `content_offset` inside it. Pointer coordinates arrive in
        // that same padded buffer space, so the identical offset has to be
        // baked in here — otherwise every hit test is skewed by the padding,
        // and because the bubble float animation makes the padding oscillate,
        // a stationary cursor over an option drifts in and out of the subtree
        // and spuriously fires `pointerleave` on the popover.
        let mut subtree = node.clone();
        // `hidden` is the parent-surface paint/input exclusion marker. The
        // same retained node is painted again as the child window, where that
        // marker must not prune the subtree.
        subtree.attributes.remove("hidden");
        offset_widget_tree_layout(
            &mut subtree,
            -bounds.0 + content_offset.0,
            -bounds.1 + content_offset.1,
        );
        let saved_tree = self.last_tree.replace(subtree);
        let result = self.handle_component_input(theme, width, height, input);
        self.last_tree = saved_tree;
        result
    }

    fn hovered_target_is_interactive(&self) -> bool {
        let Some(tree) = self.last_tree.as_ref() else {
            return false;
        };
        self.pointer_event_target_key(tree, self.hovered_pos.0, self.hovered_pos.1)
            .is_some()
    }

    fn text_input_state(&self) -> Option<mesh_core_frontend_host::TextInputState> {
        FrontendSurfaceComponent::text_input_state(self)
    }

    fn last_widget_tree(&self) -> Option<&WidgetNode> {
        self.last_tree.as_ref()
    }

    fn child_surface_debug_tree(
        &self,
        node_key: &str,
        content_offset: (f32, f32),
    ) -> Option<WidgetNode> {
        let tree = self.last_tree.as_ref()?;
        let node = find_node_by_key(tree, node_key)?;
        let bounds = find_node_bounds_by_key(tree, node_key, 0.0, 0.0)?;
        let mut child_tree = node.clone();
        child_tree.attributes.remove("hidden");
        // Must match `paint_child_surface`'s offset exactly: that call bakes
        // `-bounds + content_offset` in as the painter's starting offset
        // rather than into `.layout`, but for a pure translation the two are
        // equivalent, so baking it into `.layout` here keeps this a plain
        // `WidgetNode` the debug overlay can walk with no special-casing.
        offset_widget_tree_layout(
            &mut child_tree,
            -bounds.0 + content_offset.0,
            -bounds.1 + content_offset.1,
        );
        Some(child_tree)
    }

    fn child_surface_requests(&self) -> Vec<ChildSurfaceRequest> {
        let Some(tree) = self.last_tree.as_ref() else {
            return Vec::new();
        };

        let mut requests = Vec::new();
        let mut diagnostics = Vec::new();
        collect_child_surface_requests_with_diagnostics(
            tree,
            tree,
            &mut requests,
            &mut diagnostics,
        );
        for diagnostic in diagnostics {
            self.record_child_surface_diagnostic(&diagnostic);
        }
        requests
    }

    fn paint_child_surface(
        &self,
        node_key: &str,
        buffer: &mut PixelBuffer,
        scale: f32,
        content_offset: (u32, u32),
        exiting: bool,
    ) -> Result<bool, ComponentError> {
        let Some(tree) = self.last_tree.as_ref() else {
            return Ok(false);
        };
        let Some(node) = find_node_by_key(tree, node_key) else {
            return Ok(false);
        };
        let Some(bounds) = find_node_bounds_by_key(tree, node_key, 0.0, 0.0) else {
            return Ok(false);
        };

        // The exiting class (when applicable) is baked into `node`'s
        // `computed_style` already: `finalize_tree` scopes it to this node's
        // subtree via `closing_child_keys` before style resolution runs, so
        // the popover's own CSS transition resolves and advances through the
        // normal per-node transition engine like any other animated style.
        let _ = exiting;
        // The retained node is hidden in the parent display list, but the
        // promoted target owns the node's pixels and must paint its subtree.
        let mut child_root = node.clone();
        child_root.attributes.remove("hidden");
        let logical_width = ((buffer.width() as f32) / scale.max(f32::EPSILON)).ceil() as u32;
        let logical_height = ((buffer.height() as f32) / scale.max(f32::EPSILON)).ceil() as u32;
        let retained_generation = self
            .retained_display_list
            .subtree_generation(node.id)
            .unwrap_or_default();
        let mut child_display_lists = self.child_display_lists.borrow_mut();
        let display_list = child_display_lists.get_or_insert(node.id);
        display_list.update_at_for_retained_generation_with_dirty_nodes(
            &child_root,
            retained_generation,
            self.retained_tree.render_dirty(),
            self.retained_tree.render_dirty_node_ids(),
            -bounds.0 + content_offset.0 as f32,
            -bounds.1 + content_offset.1 as f32,
            logical_width,
            logical_height,
            false,
            true,
        );

        let mut damage_rects = display_list.damage_rects().to_vec();
        // A raster can be requested solely because the physical buffer was
        // replaced (for example after a scale change) while the logical
        // display list remains unchanged. Rebuild the whole fresh buffer in
        // that case; the damage accessor below returns `None` so presentation
        // uses the matching full-surface fallback.
        if damage_rects.is_empty() {
            damage_rects.push(DamageRect {
                x: 0,
                y: 0,
                width: logical_width.max(1),
                height: logical_height.max(1),
            });
        }
        display_list.expand_damage_for_blur_regions(&mut damage_rects);
        let selected = display_list.select_paint_commands_for_rects(
            &damage_rects,
            DisplayListRepaintPolicy::MinimalDamage,
        );
        for damage in damage_rects {
            let physical_damage =
                scale_damage_rect_to_buffer(damage, scale, buffer.width(), buffer.height());
            buffer.clear_rect(
                physical_damage.x,
                physical_damage.y,
                physical_damage.width,
                physical_damage.height,
                mesh_core_elements::style::Color::TRANSPARENT,
            );
            mesh_core_render::paint_selected_display_list_for_module_with_profiling_metrics(
                &selected,
                buffer,
                scale,
                Some((
                    physical_damage.x,
                    physical_damage.y,
                    physical_damage.width,
                    physical_damage.height,
                )),
                None,
                None,
                Some(self.compiled.manifest.package.id.as_str()),
            );
        }
        Ok(true)
    }

    fn child_surface_present_damage(&self, node_key: &str) -> Option<Vec<DamageRect>> {
        let node = find_node_by_key(self.last_tree.as_ref()?, node_key)?;
        let child_display_lists = self.child_display_lists.borrow();
        let display_list = child_display_lists.get(node.id)?;
        if display_list.damage_rects().is_empty() {
            return None;
        }
        let mut damage = display_list.damage_rects().to_vec();
        display_list.expand_damage_for_blur_regions(&mut damage);
        Some(damage)
    }

    fn child_surface_blur_regions(&self, node_key: &str) -> Vec<DamageRect> {
        let Some(tree) = self.last_tree.as_ref() else {
            return Vec::new();
        };
        let Some(node) = find_node_by_key(tree, node_key) else {
            return Vec::new();
        };
        let child_display_lists = self.child_display_lists.borrow();
        let Some(display_list) = child_display_lists.get(node.id) else {
            return Vec::new();
        };
        // Stable full-tree regions: deriving these from `paint_commands` would
        // drop the blurred nodes on scoped retained repaints, leaving an empty
        // set that the compositor reads as "blur the whole surface".
        display_list.blur_regions().to_vec()
    }

    fn child_hide_transition_ms(&self, node_key: &str) -> u64 {
        if self.motion_policy.reduced_motion {
            return 0;
        }
        let Some(tree) = self.last_tree.as_ref() else {
            return 0;
        };
        let Some(node) = find_node_by_key(tree, node_key) else {
            return 0;
        };
        node.computed_style
            .transitions
            .iter()
            .filter(|transition| transition.properties.all || transition.properties.opacity)
            .map(|transition| u64::from(transition.duration_ms))
            .max()
            .unwrap_or(0)
    }

    fn content_input_size(&self) -> Option<(u32, u32)> {
        // `last_surface_size` is the logical content size (from the component's own
        // `requested_layout_size`), NOT the tooltip-inflated surface size the shell's
        // StubSurface reports. Confining pointer input to this rect keeps clicks over
        // the tooltip padding falling through to the windows beneath.
        self.last_surface_size
    }

    fn declared_or_measured_size(&self) -> (u32, u32) {
        self.requested_layout_size()
    }

    fn needs_content_measure(&self) -> bool {
        self.measured_size.is_none()
    }

    fn invalidate_surface_config(&mut self) {
        FrontendSurfaceComponent::invalidate_surface_config(self);
    }

    fn node_bounds_by_key(&self, key: &str) -> Option<(f32, f32, f32, f32)> {
        let tree = self.last_tree.as_ref()?;
        find_node_bounds_by_key(tree, key, 0.0, 0.0)
    }

    fn popover_margin_left(&self) -> i32 {
        self.surface_layout.margin_left
    }

    fn apply_position(&mut self, margin_top: i32, margin_left: i32) {
        self.surface_layout.edge = Edge::Left;
        self.surface_layout.margin_top = margin_top;
        self.surface_layout.margin_left = margin_left;
        self.invalidate_surface_config();
    }

    fn hide_transition_ms(&self) -> u64 {
        if self.motion_policy.reduced_motion {
            return 0;
        }
        // The show/hide transition is a CSS `transition` on the surface root
        // (replacing the old manifest `display_transition`). Read the resolved
        // opacity transition duration from the last painted root style; the
        // shell delays unmapping the surface by this long so the exit animation
        // (typically `opacity -> 0` under `.mesh-surface-exiting`) can play.
        // `last_tree`'s root is the synthetic `surface` wrapper; the component's
        // own template root (which carries the `transition`) is its first child.
        let Some(root) = self
            .last_tree
            .as_ref()
            .and_then(|tree| tree.children.first())
        else {
            return 0;
        };
        root.computed_style
            .transitions
            .iter()
            .filter(|transition| transition.properties.all || transition.properties.opacity)
            .map(|transition| u64::from(transition.duration_ms))
            .max()
            .unwrap_or(0)
    }

    fn set_surface_exiting(&mut self, exiting: bool) {
        if !exiting {
            // A hidden surface keeps its component instance alive. Restart CSS
            // keyframes when it is shown again so one-shot entrance animations
            // do not remain stuck at their completed timestamp.
            self.transitions.clear();
            self.keyframe_animations.clear();
            self.keyframe_rules.clear();
            self.keyframe_animation_slots.clear();
            self.keyframe_animation_lifecycles.clear();
            self.surface_entering = true;
        }
        if self.surface_exiting == exiting {
            if !exiting {
                self.invalidate_interaction_restyle();
            }
            return;
        }
        self.surface_exiting = exiting;
        self.invalidate_interaction_restyle();
    }

    fn set_closing_child_keys(&mut self, keys: std::collections::HashSet<String>) {
        if self.closing_child_keys == keys {
            return;
        }
        self.closing_child_keys = keys;
        // A full rebuild (not just a style-only restyle) so the affected
        // popover subtree's `class` attribute is re-derived fresh from the
        // template rather than carrying forward a stale appended class from
        // a previous closing/reopening cycle.
        self.invalidate(ComponentDirtyFlags::TREE_REBUILD);
    }

    fn set_closing_child_keys_from_slice(&mut self, keys: &[&str]) {
        if self.closing_child_keys.len() == keys.len()
            && keys
                .iter()
                .all(|key| self.closing_child_keys.contains(*key))
        {
            return;
        }
        self.closing_child_keys = keys.iter().map(|key| (*key).to_owned()).collect();
        // A full rebuild (not just a style-only restyle) so the affected
        // popover subtree's `class` attribute is re-derived fresh from the
        // template rather than carrying forward a stale appended class from
        // a previous closing/reopening cycle.
        self.invalidate(ComponentDirtyFlags::TREE_REBUILD);
    }

    fn set_entering_child_keys(&mut self, keys: std::collections::HashSet<String>) {
        if self.entering_child_keys == keys {
            return;
        }
        self.entering_child_keys = keys;
        self.invalidate(ComponentDirtyFlags::TREE_REBUILD);
    }

    fn set_entering_child_keys_from_slice(&mut self, keys: &[&str]) {
        if self.entering_child_keys.len() == keys.len()
            && keys
                .iter()
                .all(|key| self.entering_child_keys.contains(*key))
        {
            return;
        }
        self.entering_child_keys = keys.iter().map(|key| (*key).to_owned()).collect();
        self.invalidate(ComponentDirtyFlags::TREE_REBUILD);
    }

    fn allows_shrink_to_fit(&self) -> bool {
        // All surfaces are CSS content-measured, so shrink-to-fit always applies.
        true
    }

    fn set_profiling_enabled(&mut self, enabled: bool) {
        self.profiling_enabled = enabled;
        if !enabled {
            self.profiling_records.get_mut().clear();
        }
    }

    fn take_profiling_records(&mut self) -> Vec<ComponentProfilingRecord> {
        std::mem::take(self.profiling_records.get_mut())
    }

    fn take_invalidation_snapshot(
        &mut self,
    ) -> Option<mesh_core_debug::ProfilingInvalidationSnapshot> {
        self.invalidation_snapshot.take()
    }

    fn take_present_damage(&mut self) -> Vec<DamageRect> {
        std::mem::take(&mut self.last_present_damage_rects)
    }

    fn wants_immediate_rerender(&self) -> bool {
        if !self.wants_render() {
            return false;
        }
        let configure_only = !self.dirty
            && self.style_only_dirty
            && !self.dirty_types.is_empty()
            && self
                .dirty_types
                .difference(ComponentDirtyFlags::SURFACE_CONFIG)
                .is_empty();
        !configure_only
    }

    fn receive_focus_transfer(
        &mut self,
        target: &TabFocusTarget,
        return_focus: Option<(String, String)>,
        close_on_focus_leave: bool,
    ) {
        if let Some(traversal) = self.last_tree.as_ref().map(collect_focus_traversal) {
            self.apply_focus_transfer_from_traversal(
                &traversal,
                target,
                return_focus,
                close_on_focus_leave,
            );
        } else {
            // No tree yet — defer via pending_auto_focus and keep return target.
            self.pending_auto_focus = true;
            self.return_focus = return_focus;
            self.close_on_focus_leave = close_on_focus_leave;
        }
    }

    fn release_focus_for_transfer(&mut self) {
        self.clear_focus_for_transfer();
    }

    fn register_popover_trigger(&mut self, trigger_key: String, popover_surface: String) {
        self.triggered_popovers.insert(trigger_key, popover_surface);
    }

    fn unregister_popover_trigger(&mut self, popover_surface: &str) {
        self.triggered_popovers
            .retain(|_, surface| surface != popover_surface);
    }

    fn set_keyboard_mode_override(&mut self, mode: Option<KeyboardMode>) {
        self.keyboard_mode_override = mode;
        self.invalidate_surface_config();
    }

    fn set_popup_promoted(&mut self, promoted: bool) {
        self.popup_promoted = promoted;
    }

    fn set_child_surface_promoted(&mut self, node_key: &str, promoted: bool) -> bool {
        if node_key == "root" || node_key.is_empty() {
            return false;
        }
        let changed = if promoted {
            self.promoted_window_keys.insert(node_key.to_string())
        } else {
            self.promoted_window_keys.remove(node_key)
        };
        if changed {
            // The node remains owned by this tree, but its layout and paint
            // ownership move across the parent/window seam.
            self.invalidate(
                ComponentDirtyFlags::TREE_REBUILD
                    | ComponentDirtyFlags::STYLE
                    | ComponentDirtyFlags::LAYOUT
                    | ComponentDirtyFlags::PAINT
                    | ComponentDirtyFlags::METRICS,
            );
        }
        changed
    }

    fn display_list_paint_commands(&self) -> &[DisplayPaintCommand] {
        self.retained_display_list.paint_commands()
    }

    fn display_list_blur_regions(&self) -> &[DamageRect] {
        self.retained_display_list.blur_regions()
    }

    fn display_list_generation(&self) -> u64 {
        self.retained_display_list.generation()
    }

    fn child_surface_paint_generation(&self, node_key: &str) -> Option<u64> {
        let node_id = find_node_by_key(self.last_tree.as_ref()?, node_key)?.id;
        self.retained_display_list.subtree_generation(node_id)
    }

    fn debug_keybinds(&self) -> Vec<mesh_core_debug::DebugKeybindEntry> {
        self.debug_surface_keybinds()
    }
}

fn retained_dirty_affects_element_metrics(dirty: RetainedTreeDirtySummary) -> bool {
    dirty.inserted > 0
        || dirty.removed > 0
        || dirty.layout > 0
        || dirty.attributes > 0
        || dirty.children > 0
}
