use super::*;
use crate::shell::component::runtime::script_has_service_read;

impl FrontendSurfaceComponent {
    fn runtime_observes_service_event(
        runtime: &EmbeddedFrontendRuntime,
        event: &ServiceEvent,
    ) -> bool {
        match event {
            ServiceEvent::Updated { service, .. } => {
                let service_name = crate::shell::service::service_name_from_interface(service);
                runtime
                    .script_ctx
                    .has_tracked_fields_for_service(&service_name)
                    || runtime
                        .script_ctx
                        .has_interface_event_subscription_for_service(&service_name)
            }
            ServiceEvent::InterfaceEvent { service, name, .. } => {
                let service_name = crate::shell::service::service_name_from_interface(service);
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
        self.compiled.surface_id()
    }

    fn initial_visibility(&self) -> Option<bool> {
        Some(self.surface_layout.visible_on_start)
    }

    fn mount(&mut self, ctx: ComponentContext) -> Result<Vec<CoreRequest>, ComponentError> {
        self.diagnostics = Some(ctx.diagnostics);
        self.load_graph_i18n_catalogs();
        self.record_declared_missing_icon_diagnostics();
        self.init_root_runtime()?;
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
                    if let Some(binding) = binding {
                        let component_id = self.id().to_string();
                        let mut state_dirty = false;
                        if let Some(runtime) = self.runtimes.lock().unwrap().get_mut(&component_id)
                        {
                            runtime
                                .script_ctx
                                .set_global_state(&binding, serde_json::json!(!*visible))
                                .map_err(|source| ComponentError::Script {
                                    component_id: component_id.clone(),
                                    source,
                                })?;
                            state_dirty = true;
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
                    self.focused_key = None;
                    self.focus_visible_key = None;
                    self.pending_auto_focus = false;
                    self.return_focus = None;
                    self.close_on_focus_leave = false;
                    self.keyboard_mode_override = None;
                } else if !was_visible {
                    self.surface_exiting = false;
                    self.surface_pixels_invalid = true;
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
        self.last_service_update = Some(format!("{service}:{source_module}"));
        let caps = crate::shell::service::service_capabilities(service);
        let service_name = &caps.service_name;
        let previous_payload = self.cached_service_payloads.get(service_name).cloned();
        self.cached_service_payloads
            .insert(service_name.clone(), payload.clone().into());
        let mut needs_rebuild = false;
        let mut runtimes = {
            let mut runtimes = self.runtimes.lock().unwrap();
            std::mem::take(&mut *runtimes)
        };
        for runtime in runtimes.values_mut() {
            if !Self::runtime_observes_service_event(runtime, event) {
                continue;
            }
            let capabilities = &runtime.script_ctx.capabilities;
            let has_read = capabilities.is_granted(&caps.read)
                || caps
                    .theme
                    .as_ref()
                    .is_some_and(|capability| capabilities.is_granted(capability))
                || caps
                    .locale
                    .as_ref()
                    .is_some_and(|capability| capabilities.is_granted(capability));
            // Always apply the Lua-level service payload so interface
            // proxies can read state fields even when the runtime lacks
            // the canonical SERVICE_NAME.read capability.
            runtime
                .script_ctx
                .apply_service_payload(service_name, payload);
            if !has_read {
                continue;
            }
            let previous = runtime.script_ctx.state().get(service_name);
            apply_service_update(
                runtime.script_ctx.state_mut(),
                true,
                service,
                source_module,
                payload,
            );
            let state_changed = runtime.script_ctx.state().is_dirty();
            let tracked_fields_changed = runtime.script_ctx.tracked_service_fields_changed(
                service_name,
                previous.as_ref(),
                payload,
            );
            if state_changed || tracked_fields_changed {
                needs_rebuild = true;
            }
        }
        *self.runtimes.lock().unwrap() = runtimes;
        if needs_rebuild {
            self.render_hooks_pending = true;
            let narrow_eligible = if let Some(ref prev) = previous_payload {
                let fields = json_field_diff(service, prev, payload);
                !fields.is_empty()
                    && fields.iter().any(|(svc, fld)| {
                        !self
                            .node_service_field_deps
                            .nodes_reading_field(svc, fld)
                            .is_empty()
                    })
            } else {
                false
            };
            if narrow_eligible {
                self.invalidate_script_state_narrow();
            } else {
                self.invalidate_script_state();
            }
        }
        Ok(Vec::new())
    }

    fn observes_service_event(&self, event: &ServiceEvent) -> bool {
        let Ok(runtimes) = self.runtimes.lock() else {
            return true;
        };
        runtimes
            .values()
            .any(|runtime| Self::runtime_observes_service_event(runtime, event))
    }

    fn wants_tick(&self) -> bool {
        let tooltip_delay_pending = self.hover_start.is_some() && !self.tooltip_visible;
        let tooltip_fade_pending = self.tooltip_visible
            && self
                .tooltip_appeared_at
                .is_some_and(|appeared| appeared.elapsed() < self.tooltip_fade_duration());
        tooltip_delay_pending
            || tooltip_fade_pending
            || !self.pending_surface_states.borrow().is_empty()
    }

    fn next_tick_deadline(&self) -> Option<std::time::Instant> {
        if !self.pending_surface_states.borrow().is_empty() {
            return Some(std::time::Instant::now());
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
        self.dirty
            || self.style_only_dirty
            || !self.transitions.is_empty()
            || self.has_active_keyframe_animation
            || !self.scroll_animations.is_empty()
    }

    fn surface_size_changed(&mut self, width: u32, height: u32) -> bool {
        self.observe_surface_size(width, height)
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

        Ok(())
    }

    fn paint(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        buffer: &mut PixelBuffer,
        scale: f32,
    ) -> Result<(), ComponentError> {
        // Capture and clear dirty flags up front. paint is the work-doer; if
        // anything during paint (measured_size change, active animation) needs
        // another frame, it sets a flag again and wants_render picks it up.
        let (requires_tree_rebuild, can_use_retained_path, dirty_types, _) =
            self.take_dirty_for_paint();
        let (requested_width, requested_height) = self.requested_layout_size();
        let content_width = if requested_width == 0 {
            width.max(1)
        } else {
            requested_width.max(1)
        };
        let content_height = if requested_height == 0 {
            height.max(1)
        } else {
            requested_height.max(1)
        };
        self.observe_surface_size(content_width, content_height);
        let paint_width = width.max(content_width).max(1);
        let paint_height = height.max(content_height).max(1);
        // Partial-damage clip rects are computed in logical coordinates, but the
        // painter applies the damage clip in physical buffer space (and scales the
        // display list by `scale`). At a fractional scale logical != physical, so a
        // partial clear/clip misaligns with where the scaled content actually paints
        // and leaves a fixed transparent gap. Full-surface paint uses clip=None and
        // clears the whole buffer, sidestepping the mismatch. Force it when the scale
        // is non-integer; the optimized partial path still runs at integer scales.
        if (scale - scale.round()).abs() > f32::EPSILON {
            self.surface_pixels_invalid = true;
        }
        let use_retained_style_path = !requires_tree_rebuild
            && can_use_retained_path
            && self.last_tree.is_some()
            && !self.render_hooks_pending;
        let previous_visual_styles = if self.last_tree.is_some() {
            self.previous_visual_styles()
        } else {
            Default::default()
        };
        let mut tree = if dirty_types.contains(ComponentDirtyFlags::SCRIPT_NARROW) {
            match self.narrow_script_update(theme, content_width, content_height) {
                Some(t) => t,
                None => self.build_tree(theme, content_width, content_height),
            }
        } else if use_retained_style_path {
            match self.restyle_retained_tree(theme, content_width, content_height, dirty_types) {
                Some(t) => t,
                None => self.build_tree(theme, content_width, content_height),
            }
        } else {
            self.build_tree(theme, content_width, content_height)
        };
        self.prune_stale_interaction_targets(&tree);
        self.apply_pending_auto_focus(&tree);
        self.apply_style_animations_with_previous(&mut tree, &previous_visual_styles);
        let retained_dirty = self.retained_tree.update(&tree);
        let retained_tree_generation = self.retained_tree.generation();
        let use_generation_shortcuts =
            dirty_types.is_empty() && !requires_tree_rebuild && !self.surface_pixels_invalid;
        let render_object_started = std::time::Instant::now();
        let render_object_dirty = if use_generation_shortcuts {
            self.retained_render_objects
                .update_for_retained_generation(&tree, retained_tree_generation)
        } else {
            self.retained_render_objects.update(&tree)
        };
        self.record_profiling_stage_with_elapsed(
            mesh_core_debug::ProfilingStage::RenderObjectSync,
            render_object_started.elapsed(),
            Some("rebuild"),
        );

        if self.tooltip_visible {
            self.refresh_tooltip_settings_from_theme(theme);
        }

        let tooltip = if self.tooltip_visible {
            self.hovered_key
                .as_ref()
                .and_then(|hovered_key| find_tooltip_by_key(&tree, hovered_key))
                .map(|(owner_key, text)| {
                    let fade_in_ms = self.tooltip_settings.fade_in_ms as f64;
                    let opacity = self
                        .tooltip_appeared_at
                        .map(|appeared| {
                            if fade_in_ms <= f64::EPSILON {
                                return 1.0;
                            }
                            let elapsed = appeared.elapsed().as_millis() as f64;
                            (elapsed / fade_in_ms).min(1.0) as f32
                        })
                        .unwrap_or(1.0);

                    // Inherited tooltips use the owner for placement and style
                    // so a titled button still anchors below the button when a
                    // child icon receives pointer hover.
                    let owner_node = find_node_by_key(&tree, &owner_key);
                    let element_anchor = owner_node
                        .map(|node| node.computed_style.tooltip_anchor)
                        .unwrap_or_default();
                    let anchor = tooltip::effective_anchor(element_anchor, &self.tooltip_settings);

                    let element_offset =
                        owner_node.and_then(|node| node.computed_style.tooltip_offset);
                    let element_bounds = find_node_bounds_by_key(&tree, &owner_key, 0.0, 0.0)
                        .or(self.hovered_element_bounds);

                    let placement = tooltip::compute_tooltip_placement(
                        anchor,
                        element_bounds,
                        self.hovered_pos,
                        (TOOLTIP_OVERLAY_WIDTH as f32, TOOLTIP_OVERLAY_HEIGHT as f32),
                        (paint_width as f32, paint_height as f32),
                        opacity,
                        &self.tooltip_settings,
                    );

                    let base_x = placement.paint_x + element_offset.map(|(x, _)| x).unwrap_or(0.0);
                    let base_y = placement.paint_y + element_offset.map(|(_, y)| y).unwrap_or(0.0);

                    // Slide animation: the tooltip moves along its placement axis
                    // from an offset position toward its final position as it fades
                    // in. For "bottom" the tooltip starts further below and slides
                    // up; for "top" it starts further above and slides down, etc.
                    let slide = self.tooltip_settings.slide_in_px * (1.0 - placement.opacity);
                    let (paint_x, paint_y) = match anchor {
                        tooltip::ResolvedAnchor::Bottom => (base_x, base_y + slide),
                        tooltip::ResolvedAnchor::Top => (base_x, base_y - slide),
                        tooltip::ResolvedAnchor::Right => (base_x + slide, base_y),
                        tooltip::ResolvedAnchor::Left => (base_x - slide, base_y),
                        tooltip::ResolvedAnchor::Cursor => (base_x, base_y),
                    };

                    // Set per-frame tooltip rendering hints.
                    mesh_core_render::set_tooltip_paint_opacity(placement.opacity);
                    let center_x = matches!(
                        anchor,
                        tooltip::ResolvedAnchor::Bottom | tooltip::ResolvedAnchor::Top
                    );
                    mesh_core_render::set_tooltip_center_x(center_x);
                    let scale_from = match self.tooltip_settings.animation.as_str() {
                        "expand" => self.tooltip_settings.expand_from,
                        _ => 0.0,
                    };
                    mesh_core_render::set_tooltip_scale_from(scale_from);

                    (text, paint_x, paint_y)
                })
        } else {
            None
        };

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
        let display_list_metrics = if use_generation_shortcuts {
            self.retained_display_list.update_for_retained_generation(
                &tree,
                retained_tree_generation,
                render_object_dirty,
                self.retained_render_objects.dirty_node_ids(),
                content_width,
                content_height,
                self.surface_pixels_invalid,
                true,
            )
        } else {
            self.retained_display_list.update_with_dirty_nodes(
                &tree,
                render_object_dirty,
                self.retained_render_objects.dirty_node_ids(),
                content_width,
                content_height,
                self.surface_pixels_invalid,
                true,
            )
        };
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
            self.retained_render_objects.dirty_node_ids(),
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
            surface_damage,
            requires_tree_rebuild,
            &visual_damage_rects,
            &tooltip_damage_rects,
            effective_damage_rects,
        );
        let _paint_damage = if effective_damage.full_surface {
            Some(surface_damage)
        } else {
            effective_damage.rect
        };
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
                self.invalidate_surface_config_only();
            }
        }
        if self.scripts_use_element_metrics {
            self.publish_element_metrics(&tree);
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
            "retained render objects '{}' generation={} dirty={:?}",
            self.id(),
            self.retained_render_objects.generation(),
            render_object_dirty
        );

        let paint_started = std::time::Instant::now();
        let paint_metrics = if effective_damage.rects.is_empty() {
            mesh_core_render::PaintProfilingMetrics::default()
        } else {
            if effective_damage.full_surface {
                buffer.clear(mesh_core_elements::style::Color::TRANSPARENT);
                if tooltip.is_some() {
                    mesh_core_render::set_tooltip_paint_colors(resolve_tooltip_colors(theme));
                }
                mesh_core_render::paint_selected_display_list_for_module_with_profiling_metrics(
                    &selected_paint,
                    buffer,
                    scale,
                    None,
                    None,
                    tooltip
                        .as_ref()
                        .map(|(text, cx, cy)| (text.as_str(), *cx, *cy)),
                    Some(self.compiled.manifest.package.id.as_str()),
                )
            } else {
                if tooltip.is_some() {
                    mesh_core_render::set_tooltip_paint_colors(resolve_tooltip_colors(theme));
                }
                if paint_bounding_rect {
                    if let Some(damage) = effective_damage.rect {
                        buffer.clear_rect(
                            damage.x,
                            damage.y,
                            damage.width,
                            damage.height,
                            mesh_core_elements::style::Color::TRANSPARENT,
                        );
                        let tooltip_for_damage = tooltip.as_ref().and_then(|(text, cx, cy)| {
                            current_tooltip_damage
                                .filter(|tooltip_rect| tooltip_rect.intersects(damage))
                                .map(|_| (text.as_str(), *cx, *cy))
                        });
                        mesh_core_render::paint_selected_display_list_for_module_with_profiling_metrics(
                            &selected_paint,
                            buffer,
                            scale,
                            Some((damage.x, damage.y, damage.width, damage.height)),
                            None,
                            tooltip_for_damage,
                            Some(self.compiled.manifest.package.id.as_str()),
                        )
                    } else {
                        mesh_core_render::PaintProfilingMetrics::default()
                    }
                } else if effective_damage.rects.len() == 1 {
                    let damage = effective_damage.rects[0];
                    buffer.clear_rect(
                        damage.x,
                        damage.y,
                        damage.width,
                        damage.height,
                        mesh_core_elements::style::Color::TRANSPARENT,
                    );
                    let tooltip_for_damage = tooltip.as_ref().and_then(|(text, cx, cy)| {
                        current_tooltip_damage
                            .filter(|tooltip_rect| tooltip_rect.intersects(damage))
                            .map(|_| (text.as_str(), *cx, *cy))
                    });
                    mesh_core_render::paint_selected_display_list_for_module_with_profiling_metrics(
                        &selected_paint,
                        buffer,
                        scale,
                        Some((damage.x, damage.y, damage.width, damage.height)),
                        None,
                        tooltip_for_damage,
                        Some(self.compiled.manifest.package.id.as_str()),
                    )
                } else {
                    let mut paint_metrics = mesh_core_render::PaintProfilingMetrics::default();
                    for damage in &effective_damage.rects {
                        buffer.clear_rect(
                            damage.x,
                            damage.y,
                            damage.width,
                            damage.height,
                            mesh_core_elements::style::Color::TRANSPARENT,
                        );
                        let tooltip_for_damage = tooltip.as_ref().and_then(|(text, cx, cy)| {
                            current_tooltip_damage
                                .filter(|tooltip_rect| tooltip_rect.intersects(*damage))
                                .map(|_| (text.as_str(), *cx, *cy))
                        });
                        let damage_metrics =
                            mesh_core_render::paint_selected_display_list_for_module_with_profiling_metrics(
                                &selected_paint,
                                buffer,
                                scale,
                                Some((damage.x, damage.y, damage.width, damage.height)),
                                None,
                                tooltip_for_damage,
                                Some(self.compiled.manifest.package.id.as_str()),
                            );
                        merge_paint_metrics(&mut paint_metrics, damage_metrics);
                    }
                    paint_metrics
                }
            }
        };
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
        if let Some(snapshot) = self.invalidation_snapshot.as_mut() {
            snapshot.text = text_cache_snapshot(paint_metrics.text);
            snapshot.paint.raster_cache_hits = paint_metrics.raster_cache_hits;
            snapshot.paint.raster_cache_misses = paint_metrics.raster_cache_misses;
            snapshot.paint.raster_cache_bypasses = paint_metrics.raster_cache_bypasses;
            snapshot.paint.raster_cache_opaque_hits = paint_metrics.raster_cache_opaque_hits;
            snapshot.paint.raster_cache_translucent_hits =
                paint_metrics.raster_cache_translucent_hits;
        }
        if self.profiling_enabled {
            self.profiling_records.push(ComponentProfilingRecord {
                stage: mesh_core_debug::ProfilingStage::Paint,
                duration: paint_started.elapsed(),
                module_id: Some(self.compiled.manifest.package.id.clone()),
                trigger_kind: Some("rebuild".to_string()),
            });
        }
        self.tooltip_damage_scratch = tooltip_damage_rects;
        self.dirty_node_visual_damage_scratch = dirty_node_visual_damage_rects;
        self.visual_damage_scratch = visual_damage_rects;
        self.effective_damage_scratch = std::mem::take(&mut effective_damage.rects);
        self.last_tree = Some(tree);
        self.last_tooltip_damage = current_tooltip_damage;
        self.surface_pixels_invalid = false;
        self.clear_runtime_dirty_states();
        if self.surface_entering {
            self.surface_entering = false;
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
        self.last_tree = None;
        self.cached_restyle_rules = None;
        self.cached_style_rule_index = None;
        self.intrinsic_layout_cache = IntrinsicLayoutCache::default();
        self.layout_state = PerSurfaceLayoutState::default();
        self.retained_tree = RetainedWidgetTree::default();
        self.retained_render_objects = RenderObjectTree::default();
        self.retained_display_list = RetainedDisplayList::default();
        self.focused_proof_snapshot = None;
        self.last_visual_damage.clear();
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
        self.locale.set_locale(locale.current());
        self.runtimes.lock().unwrap().clear();
        self.init_root_runtime()?;
        self.last_tree = None;
        self.cached_restyle_rules = None;
        self.cached_style_rule_index = None;
        self.intrinsic_layout_cache = IntrinsicLayoutCache::default();
        self.layout_state = PerSurfaceLayoutState::default();
        self.retained_tree = RetainedWidgetTree::default();
        self.retained_render_objects = RenderObjectTree::default();
        self.retained_display_list = RetainedDisplayList::default();
        self.focused_proof_snapshot = None;
        self.last_visual_damage.clear();
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

    fn module_settings_path(&self) -> Option<&Path> {
        if self.module_settings_file.exists() {
            Some(self.module_settings_file.as_path())
        } else {
            None
        }
    }

    fn reload_module_settings(&mut self) -> Result<bool, ComponentError> {
        let settings_state =
            load_frontend_module_settings(&self.module_settings_file, &self.compiled.manifest);
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
                self.invalidate_surface_config();
            }
            return Ok(layout_changed || settings_changed);
        };

        if self.locale.current() != locale {
            tracing::info!(
                "module '{}': applying locale '{}' from module settings",
                self.id(),
                locale
            );
            self.locale.set_locale(locale);
        }

        if layout_changed || settings_changed {
            self.invalidate_surface_config();
        }
        Ok(layout_changed || settings_changed)
    }

    fn reload_source(&mut self) -> Result<bool, ComponentError> {
        // CACHE-03: evict chunk cache entries for old script sources before recompiling.
        // The next compile_and_execute will get_or_insert the fresh source from disk.
        use mesh_core_scripting::chunk_cache::{ChunkCache, fnv64};

        if let Some(script) = &self.compiled.component.script {
            ChunkCache::remove(fnv64(script.source.as_bytes()));
        }
        for component in self.compiled.local_components.values() {
            if let Some(script) = &component.script {
                ChunkCache::remove(fnv64(script.source.as_bytes()));
            }
        }

        let manifest = self.compiled.manifest.clone();
        let recompiled = compile_frontend_module(&manifest, &self.module_dir).map_err(|err| {
            ComponentError::Failed {
                component_id: self.id().to_string(),
                message: format!("frontend recompile failed: {err}"),
            }
        })?;

        let component_id = self.id().to_string();
        self.compiled = recompiled;
        self.scripts_use_element_metrics = scripts_reference_element_metrics(&self.compiled);
        if let Some(entry) = self.frontend_catalog.modules.get_mut(&component_id) {
            entry.compiled = self.compiled.clone();
        }
        self.runtimes.lock().unwrap().clear();
        self.init_root_runtime()?;
        self.render_hooks_pending = true;
        self.invalidate_script_state();
        // Style rules may have changed in the recompiled module.
        self.cached_restyle_rules = None;
        self.cached_style_rule_index = None;
        self.layout_state = PerSurfaceLayoutState::default();
        self.focused_proof_snapshot = None;
        self.last_visual_damage.clear();
        Ok(true)
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

    fn hovered_target_is_interactive(&self) -> bool {
        let Some(tree) = self.last_tree.as_ref() else {
            return false;
        };
        self.pointer_event_target_key(tree, self.hovered_pos.0, self.hovered_pos.1)
            .is_some()
    }

    fn last_widget_tree(&self) -> Option<&WidgetNode> {
        self.last_tree.as_ref()
    }

    fn content_input_size(&self) -> Option<(u32, u32)> {
        // `last_surface_size` is the logical content size (from the component's own
        // `requested_layout_size`), NOT the tooltip-inflated surface size the shell's
        // StubSurface reports. Confining pointer input to this rect keeps clicks over
        // the tooltip padding falling through to the windows beneath.
        self.last_surface_size
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
        self.surface_layout.display_transition.hide_ms
    }

    fn set_surface_exiting(&mut self, exiting: bool) {
        if !exiting {
            // A hidden surface keeps its component instance alive. Restart CSS
            // keyframes when it is shown again so one-shot entrance animations
            // do not remain stuck at their completed timestamp.
            self.transitions.clear();
            self.keyframe_animations.clear();
            self.keyframe_rules.clear();
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

    fn allows_shrink_to_fit(&self) -> bool {
        self.surface_layout.size_policy == SurfaceSizePolicy::ContentMeasured
    }

    fn set_profiling_enabled(&mut self, enabled: bool) {
        self.profiling_enabled = enabled;
        if !enabled {
            self.profiling_records.clear();
        }
    }

    fn take_profiling_records(&mut self) -> Vec<ComponentProfilingRecord> {
        std::mem::take(&mut self.profiling_records)
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

    fn debug_keybinds(&self) -> Vec<mesh_core_debug::DebugKeybindEntry> {
        self.debug_surface_keybinds()
    }
}

impl FrontendSurfaceComponent {
    pub fn display_list_paint_commands(&self) -> &[DisplayPaintCommand] {
        self.retained_display_list.paint_commands()
    }

    fn refresh_tooltip_settings(&mut self) {
        if let Ok(settings) = mesh_core_config::load_shell_settings() {
            self.tooltip_settings = settings.tooltip;
        }
    }

    /// Like `refresh_tooltip_settings` but also merges theme component
    /// defaults for `"tooltip"`. Called from `paint()` which has access to the
    /// active theme. Variable references such as
    /// `var(--animation-duration-short)` are resolved against the theme's token
    /// map.
    fn refresh_tooltip_settings_from_theme(&mut self, theme: &Theme) {
        self.refresh_tooltip_settings();
        let Some(defaults) = theme.component_defaults("tooltip") else {
            return;
        };

        let resolve = |raw: &str| -> String {
            if let Some(variable_name) = raw.strip_prefix("var(").and_then(|s| s.strip_suffix(")"))
            {
                if let Some(token_name) = variable_name
                    .trim()
                    .strip_prefix("--")
                    .map(|name| name.replace('-', "."))
                    && let Some(val) = theme.token(&token_name)
                {
                    return val.to_string();
                }
            }
            raw.to_string()
        };

        let parse_f64 = |key: &str| -> Option<f64> {
            defaults
                .get(key)
                .map(|v| resolve(v))
                .and_then(|s| s.trim().parse::<f64>().ok())
        };
        let parse_str = |key: &str| -> Option<String> { defaults.get(key).map(|v| resolve(v)) };

        if let Some(v) = parse_str("position") {
            self.tooltip_settings.position = v;
        }
        if let Some(v) = parse_f64("delay") {
            self.tooltip_settings.delay_ms = v as u64;
        }
        if let Some(v) = parse_f64("animation-duration") {
            self.tooltip_settings.fade_in_ms = v as u64;
        }
        if let Some(v) = parse_str("animation") {
            self.tooltip_settings.animation = v;
        }
        if let Some(v) = parse_f64("expand-from") {
            self.tooltip_settings.expand_from = v as f32;
        }
        if let Some(v) = parse_f64("gap") {
            self.tooltip_settings.gap = v as f32;
        }
        if let Some(v) = parse_f64("cursor-offset-x") {
            self.tooltip_settings.cursor_offset_x = v as f32;
        }
        if let Some(v) = parse_f64("cursor-offset-y") {
            self.tooltip_settings.cursor_offset_y = v as f32;
        }
    }

    fn tooltip_fade_duration(&self) -> Duration {
        Duration::from_millis(self.tooltip_settings.fade_in_ms)
    }

    fn handle_interface_event(
        &mut self,
        event: &ServiceEvent,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let ServiceEvent::InterfaceEvent {
            service,
            name,
            payload,
            ..
        } = event
        else {
            return Ok(Vec::new());
        };
        let service_name = crate::shell::service::service_name_from_interface(service);
        let mut needs_rebuild = false;
        let mut runtimes = {
            let mut runtimes = self.runtimes.lock().unwrap();
            std::mem::take(&mut *runtimes)
        };
        let emit_result: Result<(), ComponentError> = (|| {
            for runtime in runtimes.values_mut() {
                if !Self::runtime_observes_service_event(runtime, event) {
                    continue;
                }
                if !script_has_service_read(&runtime.script_ctx, service, &service_name) {
                    continue;
                }
                runtime
                    .script_ctx
                    .emit_interface_event(&service_name, name, payload)
                    .map_err(|source| ComponentError::Script {
                        component_id: runtime.module_id.clone(),
                        source,
                    })?;
                if runtime.script_ctx.state().is_dirty() {
                    needs_rebuild = true;
                }
            }
            Ok(())
        })();
        *self.runtimes.lock().unwrap() = runtimes;
        emit_result?;
        if needs_rebuild {
            self.render_hooks_pending = true;
            self.invalidate_script_state();
        }
        Ok(Vec::new())
    }

    #[cfg(test)]
    pub(super) fn last_focused_proof_snapshot(
        &self,
    ) -> Option<&mesh_core_render::FocusedProofSnapshot> {
        self.focused_proof_snapshot.as_ref()
    }
}

fn resolve_tooltip_colors(theme: &Theme) -> mesh_core_render::TooltipPaintColors {
    let fallback = mesh_core_render::TooltipPaintColors::DEFAULT_DARK;
    mesh_core_render::TooltipPaintColors {
        background: token_color(theme, "color.surface-container", fallback.background),
        border: token_color(theme, "color.surface-container-high", fallback.border),
        foreground: token_color(theme, "color.on-surface", fallback.foreground),
    }
}

fn token_color(
    theme: &Theme,
    key: &str,
    fallback: mesh_core_elements::style::Color,
) -> mesh_core_elements::style::Color {
    theme
        .token(key)
        .and_then(|value| match value {
            mesh_core_theme::TokenValue::String(s) => mesh_core_elements::style::Color::from_hex(s),
            _ => None,
        })
        .unwrap_or(fallback)
}

#[cfg(test)]
fn select_effective_damage(
    metrics: DisplayListMetrics,
    surface: DamageRect,
    requires_tree_rebuild: bool,
    reorder_damage: Option<DamageRect>,
    tooltip_damage: Option<DamageRect>,
) -> EffectiveDamage {
    let extra_damage = reorder_damage.into_iter().collect::<Vec<_>>();
    let tooltip_damage = tooltip_damage.into_iter().collect::<Vec<_>>();
    select_effective_damage_rects(
        metrics,
        surface,
        requires_tree_rebuild,
        &extra_damage,
        &tooltip_damage,
        Vec::new(),
    )
}

fn select_effective_damage_rects(
    metrics: DisplayListMetrics,
    surface: DamageRect,
    requires_tree_rebuild: bool,
    extra_damage: &[DamageRect],
    tooltip_damage: &[DamageRect],
    mut rects: Vec<DamageRect>,
) -> EffectiveDamage {
    rects.clear();
    if metrics.full_surface_damage {
        rects.push(surface);
        return EffectiveDamage {
            rect: Some(surface),
            rects,
            full_surface: true,
            policy: DisplayListRepaintPolicy::FullSurface,
        };
    }

    let base_damage = (metrics.damage_area > 0).then_some(metrics.damage_rect);
    let has_extra_damage_sources = !extra_damage.is_empty() || !tooltip_damage.is_empty();
    if let Some(base) = base_damage {
        push_damage_rect(&mut rects, base, surface);
    }
    merge_damage_rects(&mut rects, extra_damage.iter().copied(), surface);
    merge_damage_rects(&mut rects, tooltip_damage.iter().copied(), surface);

    let Some(damage) = bounding_damage_rect(&rects, surface) else {
        return EffectiveDamage::none();
    };

    let damage_area = damage_rects_area(&rects);
    let policy = select_damage_policy(
        metrics,
        requires_tree_rebuild,
        has_extra_damage_sources,
        damage_area,
    );
    match policy {
        DisplayListRepaintPolicy::MinimalDamage | DisplayListRepaintPolicy::BoundingRect => {
            EffectiveDamage {
                rect: Some(damage),
                rects,
                full_surface: false,
                policy,
            }
        }
        DisplayListRepaintPolicy::FullSurface => {
            rects.clear();
            rects.push(surface);
            EffectiveDamage {
                rect: Some(surface),
                rects,
                full_surface: true,
                policy,
            }
        }
    }
}

fn select_damage_policy(
    metrics: DisplayListMetrics,
    requires_tree_rebuild: bool,
    has_extra_damage_sources: bool,
    candidate_area: u64,
) -> DisplayListRepaintPolicy {
    const FULL_SURFACE_DAMAGE_NUMERATOR: u64 = 2;
    const FULL_SURFACE_DAMAGE_DENOMINATOR: u64 = 3;
    const MOSTLY_CHANGED_ENTRIES_NUMERATOR: u64 = 3;
    const MOSTLY_CHANGED_ENTRIES_DENOMINATOR: u64 = 4;

    if candidate_area == 0 {
        return DisplayListRepaintPolicy::MinimalDamage;
    }

    let changed_entries = metrics
        .entries_rebuilt
        .saturating_add(metrics.entries_removed);
    let mostly_changed_entries = metrics.entries_total > 0
        && changed_entries * MOSTLY_CHANGED_ENTRIES_DENOMINATOR
            >= metrics.entries_total * MOSTLY_CHANGED_ENTRIES_NUMERATOR;
    // Acceptance guard: candidate_area * FULL_SURFACE_DAMAGE_DENOMINATOR >= metrics.surface_area * FULL_SURFACE_DAMAGE_NUMERATOR.
    let large_damage = metrics.surface_area > 0
        && candidate_area * FULL_SURFACE_DAMAGE_DENOMINATOR
            >= metrics.surface_area * FULL_SURFACE_DAMAGE_NUMERATOR;

    if large_damage || (requires_tree_rebuild && mostly_changed_entries) {
        DisplayListRepaintPolicy::FullSurface
    } else if has_extra_damage_sources {
        DisplayListRepaintPolicy::BoundingRect
    } else {
        DisplayListRepaintPolicy::MinimalDamage
    }
}

fn tooltip_damage_rect(
    tooltip: Option<&(String, f32, f32)>,
    surface_width: u32,
    surface_height: u32,
) -> Option<DamageRect> {
    let (_, paint_x, paint_y) = tooltip?;
    let width = TOOLTIP_OVERLAY_WIDTH.min(surface_width.max(1));
    let height = TOOLTIP_OVERLAY_HEIGHT.min(surface_height.max(1));
    let max_x = surface_width.saturating_sub(width).saturating_sub(6);
    let max_y = surface_height.saturating_sub(height).saturating_sub(6);
    let x = ((*paint_x).round() as u32).min(max_x).max(4);
    let y = ((*paint_y).round() as u32).min(max_y).max(4);
    Some(DamageRect {
        x,
        y,
        width,
        height,
    })
}

#[cfg(test)]
fn damage_rect_for_node_ids(
    node: &WidgetNode,
    node_ids: &HashSet<mesh_core_elements::NodeId>,
    last_visual_damage: &HashMap<mesh_core_elements::NodeId, DamageRect>,
    surface: DamageRect,
) -> Option<DamageRect> {
    bounding_damage_rect(
        &damage_rects_for_node_ids(node, node_ids, last_visual_damage, surface),
        surface,
    )
}

#[cfg(test)]
fn damage_rects_for_node_ids(
    node: &WidgetNode,
    node_ids: &HashSet<mesh_core_elements::NodeId>,
    last_visual_damage: &HashMap<mesh_core_elements::NodeId, DamageRect>,
    surface: DamageRect,
) -> Vec<DamageRect> {
    let mut damage = Vec::with_capacity(node_ids.len().min(MAX_DAMAGE_RECTS));
    damage_rects_for_node_ids_into(node, node_ids, last_visual_damage, surface, &mut damage);
    damage
}

fn damage_rects_for_node_ids_into(
    node: &WidgetNode,
    node_ids: &HashSet<mesh_core_elements::NodeId>,
    last_visual_damage: &HashMap<mesh_core_elements::NodeId, DamageRect>,
    surface: DamageRect,
    damage: &mut Vec<DamageRect>,
) {
    damage.clear();
    if node_ids.is_empty() {
        return;
    }

    damage.reserve(node_ids.len().min(MAX_DAMAGE_RECTS));
    for node_id in node_ids {
        if let Some(previous) = last_visual_damage.get(node_id).copied() {
            push_damage_rect(damage, previous, surface);
        }
    }
    collect_damage_rects_for_node_ids(node, node_ids, surface, damage);
}

fn collect_damage_rects_for_node_ids(
    node: &WidgetNode,
    node_ids: &HashSet<mesh_core_elements::NodeId>,
    surface: DamageRect,
    damage: &mut Vec<DamageRect>,
) {
    if node_ids.is_empty() {
        return;
    }
    if node_ids.contains(&node.id)
        && let Some(bounds) = damage_rect_for_widget_node(node, surface)
    {
        push_damage_rect(damage, bounds, surface);
    }

    for child in &node.children {
        collect_damage_rects_for_node_ids(child, node_ids, surface, damage);
    }
}

fn damage_rect_for_widget_node(node: &WidgetNode, surface: DamageRect) -> Option<DamageRect> {
    visual_damage_rect_for_widget_node(node, surface)
}

fn visual_damage_rect_for_widget_node(
    node: &WidgetNode,
    surface: DamageRect,
) -> Option<DamageRect> {
    if node.layout.width <= 0.0 || node.layout.height <= 0.0 {
        return None;
    }
    let transform = node.computed_style.transform;
    let scale_x = transform.scale_x.max(0.0);
    let scale_y = transform.scale_y.max(0.0);
    let width = node.layout.width * scale_x;
    let height = node.layout.height * scale_y;
    let mut left = node.layout.x + transform.translate_x;
    let mut top = node.layout.y + transform.translate_y;
    let mut right = left + width;
    let mut bottom = top + height;

    let shadow = node.computed_style.box_shadow;
    if !shadow.is_none() && !shadow.inset {
        let spread = shadow.spread_radius;
        let blur_pad = shadow.blur_radius * 3.0;
        left =
            left.min(node.layout.x + transform.translate_x + shadow.offset_x - spread - blur_pad);
        top = top.min(node.layout.y + transform.translate_y + shadow.offset_y - spread - blur_pad);
        right = right.max(
            node.layout.x + transform.translate_x + width + shadow.offset_x + spread + blur_pad,
        );
        bottom = bottom.max(
            node.layout.y + transform.translate_y + height + shadow.offset_y + spread + blur_pad,
        );
    }

    let filter_pad = node
        .computed_style
        .filter
        .blur_radius
        .max(node.computed_style.backdrop_filter.blur_radius)
        * 3.0;
    if filter_pad > 0.0 {
        left -= filter_pad;
        top -= filter_pad;
        right += filter_pad;
        bottom += filter_pad;
    }

    let left = left.floor().max(0.0) as u32;
    let top = top.floor().max(0.0) as u32;
    let right = right.ceil().max(0.0) as u32;
    let bottom = bottom.ceil().max(0.0) as u32;
    clip_damage(
        DamageRect {
            x: left,
            y: top,
            width: right.saturating_sub(left),
            height: bottom.saturating_sub(top),
        },
        surface,
    )
}

fn collect_visual_damage_rects(
    node: &WidgetNode,
    surface: DamageRect,
) -> HashMap<mesh_core_elements::NodeId, DamageRect> {
    let mut damage = HashMap::new();
    collect_visual_damage_rects_into(node, surface, &mut damage);
    damage
}

fn collect_visual_damage_rects_into(
    node: &WidgetNode,
    surface: DamageRect,
    damage: &mut HashMap<mesh_core_elements::NodeId, DamageRect>,
) {
    if let Some(bounds) = visual_damage_rect_for_widget_node(node, surface) {
        damage.insert(node.id, bounds);
    }
    for child in &node.children {
        collect_visual_damage_rects_into(child, surface, damage);
    }
}

fn damage_rects_from_options_into(
    rects: impl IntoIterator<Item = Option<DamageRect>>,
    surface: DamageRect,
    damage: &mut Vec<DamageRect>,
) {
    damage.clear();
    damage.reserve(2);
    for rect in rects.into_iter().flatten() {
        push_damage_rect(damage, rect, surface);
    }
}

fn merge_damage_rects(
    current: &mut Vec<DamageRect>,
    next: impl IntoIterator<Item = DamageRect>,
    surface: DamageRect,
) {
    for rect in next {
        push_damage_rect(current, rect, surface);
    }
}

fn push_damage_rect(rects: &mut Vec<DamageRect>, rect: DamageRect, surface: DamageRect) {
    let Some(rect) = clip_damage(rect, surface) else {
        return;
    };
    if let Some(index) = rects.iter().position(|existing| existing.intersects(rect)) {
        let merged = union_damage(rects[index], rect);
        rects.remove(index);
        push_damage_rect(rects, merged, surface);
        return;
    }
    if rects.len() < MAX_DAMAGE_RECTS {
        rects.push(rect);
        return;
    }

    let (merge_index, _) = rects
        .iter()
        .enumerate()
        .map(|(index, existing)| {
            let merged = union_damage(*existing, rect);
            let growth = merged.area().saturating_sub(existing.area());
            (index, growth)
        })
        .min_by_key(|(_, growth)| *growth)
        .unwrap_or((0, 0));
    let merged = union_damage(rects[merge_index], rect);
    rects.remove(merge_index);
    push_damage_rect(rects, merged, surface);
}

fn bounding_damage_rect(rects: &[DamageRect], surface: DamageRect) -> Option<DamageRect> {
    let mut iter = rects.iter().copied();
    let first = iter.next()?;
    let bounds = iter.fold(first, union_damage);
    clip_damage(bounds, surface)
}

fn damage_rects_area(rects: &[DamageRect]) -> u64 {
    rects.iter().map(|rect| rect.area()).sum()
}

fn merge_paint_metrics(
    total: &mut mesh_core_render::PaintProfilingMetrics,
    next: mesh_core_render::PaintProfilingMetrics,
) {
    total.text.layout_hits = total.text.layout_hits.saturating_add(next.text.layout_hits);
    total.text.layout_misses = total
        .text
        .layout_misses
        .saturating_add(next.text.layout_misses);
    total.text.layout_invalidations = total
        .text
        .layout_invalidations
        .saturating_add(next.text.layout_invalidations);
    total.text.shaped_entries = total.text.shaped_entries.max(next.text.shaped_entries);
    total.text.glyph_cache_active |= next.text.glyph_cache_active;
    total.text.shaping_micros = total
        .text
        .shaping_micros
        .saturating_add(next.text.shaping_micros);
    total.traversal_micros = total.traversal_micros.saturating_add(next.traversal_micros);
    total.icon_image_raster_micros = total
        .icon_image_raster_micros
        .saturating_add(next.icon_image_raster_micros);
    total.raster_cache_hits = total
        .raster_cache_hits
        .saturating_add(next.raster_cache_hits);
    total.raster_cache_misses = total
        .raster_cache_misses
        .saturating_add(next.raster_cache_misses);
    total.raster_cache_bypasses = total
        .raster_cache_bypasses
        .saturating_add(next.raster_cache_bypasses);
    total.raster_cache_opaque_hits = total
        .raster_cache_opaque_hits
        .saturating_add(next.raster_cache_opaque_hits);
    total.raster_cache_translucent_hits = total
        .raster_cache_translucent_hits
        .saturating_add(next.raster_cache_translucent_hits);
}

fn union_damage(current: DamageRect, next: DamageRect) -> DamageRect {
    let left = current.x.min(next.x);
    let top = current.y.min(next.y);
    let right = current
        .x
        .saturating_add(current.width)
        .max(next.x.saturating_add(next.width));
    let bottom = current
        .y
        .saturating_add(current.height)
        .max(next.y.saturating_add(next.height));
    DamageRect {
        x: left,
        y: top,
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
    }
}

fn clip_damage(rect: DamageRect, surface: DamageRect) -> Option<DamageRect> {
    let left = rect.x.max(surface.x);
    let top = rect.y.max(surface.y);
    let right = rect
        .x
        .saturating_add(rect.width)
        .min(surface.x.saturating_add(surface.width));
    let bottom = rect
        .y
        .saturating_add(rect.height)
        .min(surface.y.saturating_add(surface.height));
    if right > left && bottom > top {
        Some(DamageRect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface(width: u32, height: u32) -> DamageRect {
        DamageRect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    fn visual_node() -> WidgetNode {
        let mut node = WidgetNode::new("box");
        node.id = 1;
        node.layout.x = 10.0;
        node.layout.y = 10.0;
        node.layout.width = 20.0;
        node.layout.height = 10.0;
        node
    }

    fn metrics(surface_area: u64) -> DisplayListMetrics {
        DisplayListMetrics {
            surface_area,
            ..Default::default()
        }
    }

    #[test]
    fn animation_damage_includes_transform_visual_bounds() {
        let mut node = visual_node();
        node.computed_style.transform.translate_x = 15.0;
        node.computed_style.transform.translate_y = 5.0;
        node.computed_style.transform.scale_x = 2.0;
        node.computed_style.transform.scale_y = 2.0;

        let damage = visual_damage_rect_for_widget_node(&node, surface(200, 100));

        assert_eq!(
            damage,
            Some(DamageRect {
                x: 25,
                y: 15,
                width: 40,
                height: 20,
            })
        );
    }

    #[test]
    fn animation_damage_includes_shadow_filter_visual_bounds() {
        let mut node = visual_node();
        node.computed_style.box_shadow = mesh_core_elements::BoxShadow {
            offset_x: 4.0,
            offset_y: 6.0,
            blur_radius: 2.0,
            spread_radius: 1.0,
            color: mesh_core_elements::style::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 128,
            },
            inset: false,
        };
        node.computed_style.filter = mesh_core_elements::VisualFilter { blur_radius: 3.0 };

        let damage = visual_damage_rect_for_widget_node(&node, surface(200, 100));

        assert_eq!(
            damage,
            Some(DamageRect {
                x: 0,
                y: 0,
                width: 50,
                height: 42,
            })
        );
    }

    #[test]
    fn animation_damage_unions_previous_and_current_transform_bounds() {
        let mut node = visual_node();
        node.computed_style.transform.translate_x = 30.0;
        let previous = HashMap::from([(
            1,
            DamageRect {
                x: 10,
                y: 10,
                width: 20,
                height: 10,
            },
        )]);

        let damage =
            damage_rect_for_node_ids(&node, &HashSet::from([1]), &previous, surface(200, 100));

        assert_eq!(
            damage,
            Some(DamageRect {
                x: 10,
                y: 10,
                width: 50,
                height: 10,
            })
        );
    }

    #[test]
    fn animation_damage_unions_previous_and_current_shadow_bounds() {
        let mut node = visual_node();
        node.layout.x = 20.0;
        node.layout.y = 20.0;
        node.computed_style.box_shadow = mesh_core_elements::BoxShadow {
            offset_x: 4.0,
            offset_y: 6.0,
            blur_radius: 2.0,
            spread_radius: 1.0,
            color: mesh_core_elements::style::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 128,
            },
            inset: false,
        };
        let previous = HashMap::from([(
            1,
            DamageRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
        )]);

        let damage =
            damage_rect_for_node_ids(&node, &HashSet::from([1]), &previous, surface(200, 100));

        assert_eq!(
            damage,
            Some(DamageRect {
                x: 0,
                y: 0,
                width: 51,
                height: 43,
            })
        );
    }

    #[test]
    fn policy_keeps_zero_candidate_area_minimal() {
        let policy = select_damage_policy(metrics(10_000), false, false, 0);

        assert_eq!(policy, DisplayListRepaintPolicy::MinimalDamage);
    }

    #[test]
    fn policy_keeps_small_single_damage_minimal() {
        let policy = select_damage_policy(metrics(10_000), false, false, 900);

        assert_eq!(policy, DisplayListRepaintPolicy::MinimalDamage);
    }

    #[test]
    fn policy_keeps_small_overlay_damage_as_bounding_rect() {
        let metrics = metrics(10_000);
        let tooltip = Some(DamageRect {
            x: 10,
            y: 10,
            width: 40,
            height: 20,
        });

        let effective = select_effective_damage(metrics, surface(100, 100), false, None, tooltip);

        assert_eq!(
            effective.rect, tooltip,
            "small tooltip invalidation should stay as a bounded repaint"
        );
        assert!(!effective.full_surface);
        assert_eq!(effective.policy, DisplayListRepaintPolicy::BoundingRect);
    }

    #[test]
    fn policy_keeps_distant_extra_damage_as_multiple_rects() {
        let metrics = metrics(10_000);
        let left = DamageRect {
            x: 5,
            y: 5,
            width: 10,
            height: 10,
        };
        let right = DamageRect {
            x: 80,
            y: 80,
            width: 10,
            height: 10,
        };

        let effective = select_effective_damage_rects(
            metrics,
            surface(100, 100),
            false,
            &[left, right],
            &[],
            Vec::new(),
        );

        assert_eq!(effective.rects, vec![left, right]);
        assert_eq!(
            effective.rect,
            Some(DamageRect {
                x: 5,
                y: 5,
                width: 85,
                height: 85,
            })
        );
        assert_eq!(effective.damage_area(10_000), 200);
        assert_eq!(effective.damage_rect_count(), 2);
        assert!(!effective.full_surface);
        assert_eq!(effective.policy, DisplayListRepaintPolicy::BoundingRect);
    }

    #[test]
    fn damage_rect_limit_recoalesces_after_forced_merge() {
        let surface = surface(100, 100);
        let top_left = DamageRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        };
        let bottom_left = DamageRect {
            x: 0,
            y: 20,
            width: 10,
            height: 10,
        };
        let far_top = DamageRect {
            x: 70,
            y: 0,
            width: 10,
            height: 10,
        };
        let far_bottom = DamageRect {
            x: 70,
            y: 20,
            width: 10,
            height: 10,
        };
        let bridge = DamageRect {
            x: 20,
            y: 0,
            width: 10,
            height: 30,
        };
        let mut rects = vec![top_left, bottom_left, far_top, far_bottom];

        push_damage_rect(&mut rects, bridge, surface);

        assert_eq!(rects.len(), 3);
        assert!(rects.contains(&DamageRect {
            x: 0,
            y: 0,
            width: 30,
            height: 30,
        }));
    }

    #[test]
    fn policy_keeps_below_threshold_extra_damage_as_bounding_rect() {
        let policy = select_damage_policy(metrics(10_000), false, true, 6_600);

        assert_eq!(policy, DisplayListRepaintPolicy::BoundingRect);
    }

    #[test]
    fn policy_promotes_two_thirds_surface_damage_to_full_repaint() {
        let metrics = metrics(9_000);
        let reorder = Some(DamageRect {
            x: 0,
            y: 0,
            width: 60,
            height: 100,
        });

        let effective = select_effective_damage(metrics, surface(90, 100), false, reorder, None);

        assert!(effective.full_surface);
        assert_eq!(effective.rect, Some(surface(90, 100)));
        assert_eq!(effective.policy, DisplayListRepaintPolicy::FullSurface);
    }

    #[test]
    fn policy_promotes_large_bounding_damage_to_full_repaint() {
        let metrics = DisplayListMetrics {
            surface_area: 10_000,
            ..Default::default()
        };
        let reorder = Some(DamageRect {
            x: 0,
            y: 0,
            width: 82,
            height: 82,
        });

        let effective = select_effective_damage(metrics, surface(100, 100), false, reorder, None);

        assert!(effective.full_surface);
        assert_eq!(effective.rect, Some(surface(100, 100)));
        assert_eq!(effective.policy, DisplayListRepaintPolicy::FullSurface);
    }

    #[test]
    fn policy_promotes_tree_rebuild_when_three_quarters_entries_changed() {
        let metrics = DisplayListMetrics {
            surface_area: 10_000,
            entries_total: 8,
            entries_rebuilt: 5,
            entries_removed: 1,
            ..Default::default()
        };

        let policy = select_damage_policy(metrics, true, false, 1_000);

        assert_eq!(policy, DisplayListRepaintPolicy::FullSurface);
    }

    #[test]
    fn policy_keeps_tree_rebuild_below_entry_threshold_non_full_surface() {
        let metrics = DisplayListMetrics {
            surface_area: 10_000,
            entries_total: 8,
            entries_rebuilt: 5,
            entries_removed: 0,
            ..Default::default()
        };

        let policy = select_damage_policy(metrics, true, false, 1_000);

        assert_eq!(policy, DisplayListRepaintPolicy::MinimalDamage);
    }
}
