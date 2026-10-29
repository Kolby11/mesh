use super::*;

impl FrontendSurfaceComponent {
    pub(super) fn apply_pending_embedded_popover_focus(&mut self, tree: &WidgetNode) {
        if self.pending_embedded_popover_focus_restore {
            self.pending_embedded_popover_focus_restore = false;
            if let Some(key) = self.embedded_popover_return_focus.take()
                && find_node_by_key(tree, &key).is_some()
            {
                let mut transaction = self.interaction_state.begin();
                transaction.focus(Some(runtime_node_id_for_key(&key)), true);
                self.commit_interaction_delta(transaction);
                self.focused_key = Some(key.clone());
                self.focus_visible_key = Some(key);
                self.focused_id = self.focused_key.as_deref().map(runtime_node_id_for_key);
                self.focus_visible_id = self.focused_id;
                self.invalidate_interaction_restyle();
            }
            return;
        }
        let Some(visible) = self.pending_embedded_popover_focus.take() else {
            if !contains_open_popover(tree) {
                self.embedded_popover_return_focus = None;
            }
            return;
        };

        let Some(popover) = open_popover(tree) else {
            return;
        };
        // A promoted popover's content becomes focusable only once its popup
        // surface is configured, which can be a paint after it opens; keep
        // the request until then rather than leaving focus on the trigger.
        let Some(key) = mesh_core_interaction::initial_focus_target(tree, popover) else {
            self.pending_embedded_popover_focus = Some(visible);
            return;
        };
        {
            self.embedded_popover_return_focus = self.focused_key.clone();
            let mut transaction = self.interaction_state.begin();
            transaction.focus(Some(runtime_node_id_for_key(&key)), visible);
            self.commit_interaction_delta(transaction);
            self.focused_key = Some(key.clone());
            self.focus_visible_key = visible.then(|| key.clone());
            self.focused_id = Some(runtime_node_id_for_key(&key));
            self.focus_visible_id = visible.then_some(self.focused_id).flatten();
            self.invalidate_interaction_restyle();
        }
    }

    #[cfg(test)]
    pub fn display_list_paint_commands(&self) -> &[DisplayPaintCommand] {
        self.retained_display_list.paint_commands()
    }

    pub(super) fn refresh_tooltip_settings(&mut self) {
        let revision = self.settings.revision();
        if self.tooltip_settings_revision == revision {
            return;
        }
        self.tooltip_settings = self.settings.shell().tooltip.clone();
        self.tooltip_settings_revision = revision;
    }

    pub(super) fn refresh_tooltip_settings_for_store_change(&mut self) {
        self.tooltip_settings = self.settings.shell().tooltip.clone();
        self.tooltip_settings_revision = self.settings.revision();
    }

    /// Like `refresh_tooltip_settings` but also merges theme component
    /// defaults for `"tooltip"`. Called from `paint()` which has access to the
    /// active theme. Variable references such as
    /// `var(--animation-duration-short)` are resolved against the theme's token
    /// map.
    pub(super) fn refresh_tooltip_settings_from_theme(&mut self, theme: &Theme) {
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
        if let Some(v) = parse_f64("gap") {
            self.tooltip_settings.gap = v as f32;
        }
        if let Some(v) = parse_f64("cursor-offset-x") {
            self.tooltip_settings.cursor_offset_x = v as f32;
        }
        if let Some(v) = parse_f64("cursor-offset-y") {
            self.tooltip_settings.cursor_offset_y = v as f32;
        }

        // The enter animation is pure theme CSS: `animation:` shorthand on
        // the tooltip block plus a theme-level `@keyframes` rule.
        self.tooltip_animation = tooltip::tooltip_animation_from_theme(theme);
    }

    /// How long the tooltip keeps animating after it appears. Zero when the
    /// theme declares no enter animation.
    pub(super) fn tooltip_fade_duration(&self) -> Duration {
        self.tooltip_animation
            .as_ref()
            .map(|animation| {
                self.motion_policy
                    .duration(animation.total_duration(), false)
            })
            .unwrap_or(Duration::ZERO)
    }

    /// Resolves the currently hovered tooltip's text and paint position, and
    /// pushes the per-frame tooltip rendering hints (opacity/center/scale)
    /// consumed by the painter. Returns `None` when no tooltip should show.
    pub(super) fn compute_tooltip_state(
        &mut self,
        theme: &Theme,
        tree: &WidgetNode,
        retained_tree_generation: u64,
        paint_width: u32,
        paint_height: u32,
    ) -> Option<(Arc<str>, f32, f32)> {
        self.tooltip_popup = None;
        if !self.tooltip_visible {
            return None;
        }
        self.refresh_tooltip_settings_from_theme(theme);

        let hovered_key = self.hovered_key.as_ref()?;
        let tooltip_target = self.tooltip_target_cache.resolve(
            tree,
            hovered_key,
            retained_tree_generation,
            self.hovered_element_bounds,
        )?;
        let text = tooltip_target.text.as_ref();

        // Sample the theme-CSS enter animation at the current elapsed time.
        // No animation in the theme (or no appear timestamp) → resting state.
        let sample = match (&self.tooltip_animation, self.tooltip_appeared_at) {
            (Some(animation), Some(appeared)) if !self.motion_policy.reduced_motion => {
                animation.sample(appeared.elapsed())
            }
            _ => tooltip::TooltipAnimationSample::FINISHED,
        };

        // Inherited tooltips use the owner for placement and style so a
        // titled button still anchors below the button when a child icon
        // receives pointer hover.
        let anchor = tooltip::effective_anchor(tooltip_target.anchor, &self.tooltip_settings);
        let element_offset = tooltip_target.offset;
        let element_bounds = tooltip_target.bounds;
        // Tooltips are overlay chrome. They should be constrained by the
        // tooltip-padded paint surface, not by a scroll/clip container inside
        // the component tree.
        let container_bounds = None;

        // Measure the real logical tooltip box with the same style the painter
        // uses, so fit checks match what actually paints.
        let font_family = theme
            .resolve_token_value("typography.family")
            .ok()
            .flatten()
            .and_then(|value| match value {
                mesh_core_theme::TokenValue::String(value) => Some(value),
                _ => None,
            })
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Inter".into());
        self.paint_context.tooltip_font_family = font_family.clone();
        let style = tooltip::tooltip_paint_style_from_theme(theme);
        self.paint_context.tooltip_style = style;
        let (text_w, text_h) = SharedTextMeasurer.measure_styled(
            &text,
            &font_family,
            style.font_size,
            style.font_weight,
            mesh_core_render::TooltipPaintStyle::LINE_HEIGHT,
            Some(style.max_width),
        );
        let tooltip_size = style.box_size(text_w, text_h);

        let placement = tooltip::compute_tooltip_placement(
            anchor,
            element_bounds,
            container_bounds,
            self.hovered_pos,
            tooltip_size,
            (paint_width as f32, paint_height as f32),
            sample.opacity,
            &self.tooltip_settings,
        );

        let surface = (paint_width as f32, paint_height as f32);
        if placement.side != tooltip::PlacedSide::Cursor
            && let Some(element) = element_bounds
            && tooltip::TooltipPopup::overflows(&placement, tooltip_size, surface)
        {
            self.tooltip_popup = Some(tooltip::TooltipPopup {
                text: Arc::clone(&tooltip_target.text),
                element,
                side: placement.side,
                size: tooltip_size,
                gap: self.tooltip_settings.gap,
                style,
                font_family,
                opacity: placement.opacity,
                scale: sample.scale,
            });
            return None;
        }

        // The keyframes' `translate()` moves the whole box relative to its
        // resting spot; the authored element offset stacks on top.
        let paint_x =
            placement.paint_x + element_offset.map(|(x, _)| x).unwrap_or(0.0) + sample.translate_x;
        let paint_y =
            placement.paint_y + element_offset.map(|(_, y)| y).unwrap_or(0.0) + sample.translate_y;

        // Set per-frame tooltip rendering hints from the animation sample.
        self.paint_context.tooltip_opacity = placement.opacity;
        let center_x = matches!(
            placement.side,
            tooltip::PlacedSide::Bottom | tooltip::PlacedSide::Top
        );
        self.paint_context.tooltip_center_x = center_x;
        self.paint_context.tooltip_scale = sample.scale;

        Some((Arc::clone(&tooltip_target.text), paint_x, paint_y))
    }

    pub(super) fn paint_pixel_regions(
        &self,
        _theme: &Theme,
        session: &mut PixelCanvasSession<'_>,
        scale: f32,
        selected_paint: &mesh_core_render::display_list::SelectedDisplayListPaint<'_>,
        effective_damage: &EffectiveDamage,
        paint_bounding_rect: bool,
        tooltip: Option<&(Arc<str>, f32, f32)>,
        current_tooltip_damage: Option<DamageRect>,
    ) -> mesh_core_render::PaintProfilingMetrics {
        let _span = tracing::debug_span!("paint_pixel_regions").entered();
        if effective_damage.rects.is_empty() {
            return mesh_core_render::PaintProfilingMetrics::default();
        }

        if effective_damage.full_surface {
            session.clear_rect(
                DamageRect {
                    x: 0,
                    y: 0,
                    width: session.width(),
                    height: session.height(),
                },
                mesh_core_elements::style::Color::TRANSPARENT,
            );
            return self.paint_selected_pixels(
                session,
                scale,
                selected_paint,
                None,
                tooltip.map(|(text, cx, cy)| (text.as_ref(), *cx, *cy)),
            );
        }

        if paint_bounding_rect {
            return effective_damage
                .rect
                .map(|damage| {
                    self.paint_damage_rect(
                        session,
                        scale,
                        selected_paint,
                        damage,
                        tooltip,
                        current_tooltip_damage,
                    )
                })
                .unwrap_or_default();
        }

        if effective_damage.rects.len() == 1 {
            return self.paint_damage_rect(
                session,
                scale,
                selected_paint,
                effective_damage.rects[0],
                tooltip,
                current_tooltip_damage,
            );
        }

        // RND-09: the final damage stage. Rects that overlap after device
        // rounding would be cleared once and painted once per region, which
        // composites translucent content over itself.
        let device_damage = mesh_core_render::DeviceDamage::from_logical(
            &effective_damage.rects,
            scale,
            session.width(),
            session.height(),
        );
        let mut physical_regions: smallvec::SmallVec<[(u32, u32, u32, u32); MAX_DAMAGE_RECTS]> =
            smallvec::SmallVec::with_capacity(device_damage.rects().len());
        for physical_damage in device_damage.rects() {
            session.clear_rect(
                *physical_damage,
                mesh_core_elements::style::Color::TRANSPARENT,
            );
            physical_regions.push((
                physical_damage.x,
                physical_damage.y,
                physical_damage.width,
                physical_damage.height,
            ));
        }
        let tooltip_for_regions = tooltip.and_then(|(text, cx, cy)| {
            current_tooltip_damage
                .filter(|tooltip_rect| {
                    effective_damage
                        .rects
                        .iter()
                        .any(|damage| tooltip_rect.intersects(*damage))
                })
                .map(|_| (text.as_ref(), *cx, *cy))
        });
        mesh_core_render::paint_selected_display_list_regions_for_module_with_profiling_metrics_and_attribution_in_session_with_context(
            selected_paint,
            session,
            scale,
            physical_regions.as_slice(),
            None,
            tooltip_for_regions,
            Some(self.compiled.manifest.package.id.as_str()),
            self.profiling_enabled,
            Some(&self.paint_context),
        )
    }

    pub(super) fn paint_damage_rect(
        &self,
        session: &mut PixelCanvasSession<'_>,
        scale: f32,
        selected_paint: &mesh_core_render::display_list::SelectedDisplayListPaint<'_>,
        damage: DamageRect,
        tooltip: Option<&(Arc<str>, f32, f32)>,
        current_tooltip_damage: Option<DamageRect>,
    ) -> mesh_core_render::PaintProfilingMetrics {
        let physical_damage =
            scale_damage_rect_to_buffer(damage, scale, session.width(), session.height());
        session.clear_rect(
            physical_damage,
            mesh_core_elements::style::Color::TRANSPARENT,
        );
        let tooltip_for_damage = tooltip.and_then(|(text, cx, cy)| {
            current_tooltip_damage
                .filter(|tooltip_rect| tooltip_rect.intersects(damage))
                .map(|_| (text.as_ref(), *cx, *cy))
        });
        self.paint_selected_pixels(
            session,
            scale,
            selected_paint,
            Some(physical_damage),
            tooltip_for_damage,
        )
    }

    /// Key caps for every bound, eligible control on the surface.
    pub(super) fn paint_key_hints(
        &self,
        session: &mut PixelCanvasSession<'_>,
        scale: f32,
        tree: &WidgetNode,
    ) {
        let hints = self.collect_key_hints(tree);
        if hints.is_empty() {
            return;
        }
        let corner = mesh_core_render::KeyHintCorner::parse(
            &self.current_keyboard_settings().hint_corner,
        );
        mesh_core_render::paint_key_hints_in_session(
            &hints,
            corner,
            session,
            scale,
            None,
            Some(&self.paint_context),
        );
    }

    pub(in crate::shell::component) fn collect_key_hints(
        &self,
        tree: &WidgetNode,
    ) -> Vec<mesh_core_render::KeyHint> {
        let mut hints = Vec::new();
        let shortcuts = self.current_surface_shortcut_labels();
        self.collect_key_hints_from(tree, tree, &shortcuts, &mut hints);
        hints
    }

    fn collect_key_hints_from(
        &self,
        root: &WidgetNode,
        node: &WidgetNode,
        shortcuts: &HashMap<String, Vec<String>>,
        hints: &mut Vec<mesh_core_render::KeyHint>,
    ) {
        if let (Some(keybind_id), Some(key)) = (node.attributes.get(Prop::Keybind), node.mesh_key())
            && let Some(shortcut) = shortcuts.get(keybind_id).and_then(|labels| labels.first())
            && self.key_hint_is_active(keybind_id, key)
            && let Some((left, top, right, bottom)) =
                mesh_core_interaction::find_node_bounds_by_key(root, key, 0.0, 0.0)
        {
            let label = mesh_core_config::keybinds::KeyChord::parse(shortcut)
                .map_or_else(|| shortcut.to_string(), |chord| chord.display());
            hints.push(mesh_core_render::KeyHint {
                x: left,
                y: top,
                width: right - left,
                height: bottom - top,
                label,
            });
        }
        for child in &node.children {
            self.collect_key_hints_from(root, child, shortcuts, hints);
        }
    }
    pub(super) fn paint_selected_pixels(
        &self,
        session: &mut PixelCanvasSession<'_>,
        scale: f32,
        selected_paint: &mesh_core_render::display_list::SelectedDisplayListPaint<'_>,
        damage: Option<DamageRect>,
        tooltip: Option<(&str, f32, f32)>,
    ) -> mesh_core_render::PaintProfilingMetrics {
        mesh_core_render::paint_selected_display_list_for_module_with_profiling_metrics_and_attribution_in_session_with_context(
            selected_paint,
            session,
            scale,
            damage.map(|rect| (rect.x, rect.y, rect.width, rect.height)),
            None,
            tooltip,
            Some(self.compiled.manifest.package.id.as_str()),
            self.profiling_enabled,
            Some(&self.paint_context),
        )
    }

    pub(super) fn handle_interface_event(
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
        let service_name = mesh_core_frontend_runtime::service::service_name_from_interface_cow(service);
        let mut needs_rebuild = false;
        let mut runtimes = {
            let mut runtimes = self.runtimes.lock().unwrap();
            std::mem::take(&mut *runtimes)
        };
        for runtime in runtimes.values_mut() {
            if !Self::runtime_observes_service_event(runtime, event) {
                continue;
            }
            if !runtime
                .script_ctx
                .can_subscribe_service_event(service, name)
            {
                continue;
            }
            if let Err(source) =
                runtime
                    .script_ctx
                    .emit_interface_event(&service_name, name, payload)
            {
                let instance_key = runtime.script_ctx.instance_id.clone();
                let _ =
                    self.record_frontend_runtime_issue("interface event", &instance_key, source);
                continue;
            }
            if runtime.script_ctx.state().is_dirty() {
                needs_rebuild = true;
            }
        }
        *self.runtimes.lock().unwrap() = runtimes;
        if needs_rebuild {
            self.reactive_effects_pending = true;
            self.invalidate_script_state();
        }
        Ok(Vec::new())
    }

    #[cfg(test)]
    pub(in crate::shell::component) fn last_focused_proof_snapshot(
        &self,
    ) -> Option<&mesh_core_render::FocusedProofSnapshot> {
        self.focused_proof_snapshot.as_ref()
    }

    /// Paint the promoted tooltip into its popup buffer, the box placed at the
    /// popup's padding. `false` when no tooltip is promoted.
    pub(super) fn paint_tooltip_popup(
        &self,
        canvas: &mut SurfaceCanvas,
        scale: f32,
        content_offset: (u32, u32),
    ) -> bool {
        let Some(popup) = self.tooltip_popup.as_ref() else {
            return false;
        };
        let scale_policy = mesh_core_render::FractionalScale::new(scale);
        let width = scale_policy.logical_extent(canvas.width());
        let height = scale_policy.logical_extent(canvas.height());
        let mut display_list = self.tooltip_popup_display_list.borrow_mut();
        let mut root = WidgetNode::new("box");
        root.layout.width = width as f32;
        root.layout.height = height as f32;
        display_list.update(&root, width, height, true, false);
        let full = DamageRect {
            x: 0,
            y: 0,
            width: width.max(1),
            height: height.max(1),
        };
        if let SurfaceCanvas::Scene(scene) = canvas {
            scene.begin(display_list.frame_paint_plan());
        }
        let selected = display_list
            .select_paint_commands_for_rects(&[full], DisplayListRepaintPolicy::FullSurface);
        let context = mesh_core_render::PaintContext {
            tooltip_style: popup.style,
            tooltip_font_family: popup.font_family.clone(),
            tooltip_opacity: popup.opacity,
            tooltip_scale: popup.scale,
            tooltip_center_x: false,
            ..self.paint_context.clone()
        };
        let mut session = canvas.session();
        let physical = scale_damage_rect_to_buffer(full, scale, session.width(), session.height());
        session.clear_rect(physical, mesh_core_elements::style::Color::TRANSPARENT);
        mesh_core_render::paint_selected_display_list_for_module_with_profiling_metrics_and_attribution_in_session_with_context(
            &selected,
            &mut session,
            scale,
            None,
            None,
            Some((
                popup.text.as_ref(),
                content_offset.0 as f32,
                content_offset.1 as f32,
            )),
            Some(self.compiled.manifest.package.id.as_str()),
            false,
            Some(&context),
        );
        self.tooltip_popup_handled_generation
            .set(Some(popup.paint_generation()));
        true
    }
}

pub(super) fn open_popover(node: &WidgetNode) -> Option<&WidgetNode> {
    if source_element_tag(node) == "popover" && popover_is_open(node) {
        return Some(node);
    }
    node.children.iter().find_map(open_popover)
}

pub(super) fn contains_open_popover(node: &WidgetNode) -> bool {
    (source_element_tag(node) == "popover" && popover_is_open(node))
        || node.children.iter().any(contains_open_popover)
}
