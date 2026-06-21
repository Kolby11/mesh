use super::*;

impl FrontendSurfaceComponent {
    pub(super) fn clear_selection(&mut self) {
        self.selection = None;
    }

    pub(super) fn begin_text_selection(&mut self, node_key: String, x: f32, y: f32) {
        let point = TextSelectionPoint { node_key, x, y };
        self.selection = Some(TextSelectionState {
            anchor: point.clone(),
            focus: point,
            dragging: true,
        });
    }

    pub(super) fn end_text_selection_drag(&mut self) {
        if let Some(selection) = self.selection.as_mut() {
            selection.dragging = false;
        }
    }

    pub(super) fn update_text_selection_focus(&mut self, x: f32, y: f32) {
        let Some(selection) = self.selection.as_mut() else {
            return;
        };
        if !selection.dragging {
            return;
        }
        selection.focus.x = x;
        selection.focus.y = y;
        selection.focus.node_key = selection.anchor.node_key.clone();
    }

    pub(super) fn clear_runtime_dirty_states(&self) {
        for runtime in self.runtimes.lock().unwrap().values_mut() {
            runtime.script_ctx.state_mut().clear_dirty();
        }
    }

    pub(super) fn publish_element_metrics(&self, tree: &WidgetNode) {
        let mut elements = serde_json::Map::new();
        let mut refs = serde_json::Map::new();
        let mut ref_keys = HashMap::new();
        collect_element_metrics(tree, 0.0, 0.0, &mut elements, &mut refs, &mut ref_keys);
        let refs = serde_json::Value::Object(refs);

        if let Some(root_runtime) = self.runtimes.lock().unwrap().get_mut(self.id()) {
            // `elements` and the `refs` snapshot stay in script state (templates can
            // bind `{refs.x.width}`); `apply_element_metrics` additionally feeds the
            // live `refs.<name>` proxy that scripts read, which also exposes
            // imperative methods (`refs.x:focus()`).
            root_runtime
                .script_ctx
                .state_mut()
                .set_host_value("elements", serde_json::Value::Object(elements));
            root_runtime
                .script_ctx
                .state_mut()
                .set_host_value("refs", refs.clone());
            root_runtime.script_ctx.apply_element_metrics(&refs);
        }
        // Remember name -> node key so drained element actions resolve their target.
        *self.ref_node_keys.borrow_mut() = ref_keys;
    }

    /// Execute imperative element actions queued by scripts through live
    /// `refs.<name>` references (`focus`, `blur`, …), resolving each target to its
    /// live widget node and routing through the real focus/interaction paths.
    pub(super) fn apply_element_actions(&mut self) -> Result<Vec<CoreRequest>, ComponentError> {
        let actions = match self.runtimes.lock().unwrap().get_mut(self.id()) {
            Some(runtime) => runtime.script_ctx.drain_element_actions(),
            None => Vec::new(),
        };
        if actions.is_empty() {
            return Ok(Vec::new());
        }
        let Some(tree) = self.last_tree.clone() else {
            return Ok(Vec::new());
        };
        let ref_keys = self.ref_node_keys.borrow().clone();

        let mut requests = Vec::new();
        for action in actions {
            match action.action.as_str() {
                "focus" => {
                    if let Some(key) = ref_keys.get(&action.target) {
                        requests.extend(self.set_focus_target(&tree, Some(key.clone()), true)?);
                    }
                }
                "blur" => {
                    // Only clear focus if the target currently holds it, so a stale
                    // blur does not steal focus from an unrelated element.
                    let holds_focus = ref_keys
                        .get(&action.target)
                        .is_some_and(|key| self.focused_key.as_deref() == Some(key));
                    if holds_focus {
                        requests.extend(self.set_focus_target(&tree, None, false)?);
                    }
                }
                "scroll_to" => {
                    // `refs.x:scroll_to(top[, left])` sets the element's own scroll
                    // offset (DOM `element.scrollTop = y`), clamped to the container's
                    // range; omitted axes stay put. `{ smooth = true }` eases there.
                    if let Some(key) = ref_keys.get(&action.target).cloned()
                        && let Some(node) = find_node_by_key(&tree, &key)
                    {
                        let (max_x, max_y) = scroll_limits(node);
                        let nums = action.args.as_array();
                        let arg_f32 = |index: usize| {
                            nums.and_then(|values| values.get(index))
                                .and_then(serde_json::Value::as_f64)
                                .map(|value| value as f32)
                        };
                        let current = self.scroll_offsets.get(&key).copied().unwrap_or_default();
                        let mut target = current;
                        if let Some(top) = arg_f32(0) {
                            target.y = top.clamp(0.0, max_y);
                        }
                        if let Some(left) = arg_f32(1) {
                            target.x = left.clamp(0.0, max_x);
                        }
                        if self.apply_scroll_target(key, current, target, &action.options) {
                            self.invalidate(
                                ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS,
                            );
                        }
                    }
                }
                "scroll_into_view" => {
                    // Nudge each scrollable ancestor's offset just enough to reveal
                    // the target, routing through the same scroll_offsets map the
                    // wheel handler mutates. Geometry lives in mesh-core-interaction.
                    if let Some(key) = ref_keys.get(&action.target) {
                        let updates =
                            scroll_into_view_offsets(&tree, key, &self.scroll_offsets);
                        let mut moved = false;
                        for (container_key, target) in updates {
                            let current = self
                                .scroll_offsets
                                .get(&container_key)
                                .copied()
                                .unwrap_or_default();
                            moved |= self.apply_scroll_target(
                                container_key,
                                current,
                                target,
                                &action.options,
                            );
                        }
                        if moved {
                            self.invalidate(
                                ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS,
                            );
                        }
                    }
                }
                other => {
                    tracing::debug!(action = %other, target = %action.target, "unhandled element action");
                }
            }
        }
        Ok(requests)
    }

    /// Apply a resolved scroll target to a container, honoring the `{ smooth }`
    /// option. Instant scrolls snap `scroll_offsets` and cancel any running
    /// animation; smooth scrolls register a `ScrollAnimation` that
    /// `advance_scroll_animations` eases over `duration` (default 250ms). Returns
    /// whether anything will change (so the caller can invalidate).
    pub(super) fn apply_scroll_target(
        &mut self,
        key: String,
        current: ScrollOffsetState,
        target: ScrollOffsetState,
        options: &serde_json::Value,
    ) -> bool {
        if (target.x - current.x).abs() < f32::EPSILON
            && (target.y - current.y).abs() < f32::EPSILON
        {
            return false;
        }

        let smooth = options
            .get("smooth")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if smooth {
            let duration_ms = options
                .get("duration")
                .and_then(serde_json::Value::as_f64)
                .filter(|value| *value > 0.0)
                .unwrap_or(250.0);
            self.scroll_animations.insert(
                key,
                ScrollAnimation {
                    start: current,
                    target,
                    start_time: std::time::Instant::now(),
                    duration: std::time::Duration::from_secs_f64(duration_ms / 1000.0),
                },
            );
        } else {
            // A snap supersedes any in-flight smooth scroll on the same container.
            self.scroll_animations.remove(&key);
            self.scroll_offsets.insert(key, target);
        }
        true
    }

    /// Tick every in-flight smooth-scroll animation: write its eased offset into
    /// `scroll_offsets` and drop it once it reaches the target. Called at the top
    /// of `finalize_tree` (before annotation reads the offsets); keeps requesting
    /// repaints via `wants_render` while any animation is live.
    pub(super) fn advance_scroll_animations(&mut self, now: std::time::Instant) {
        if self.scroll_animations.is_empty() {
            return;
        }

        let mut finished = Vec::new();
        let mut updates = Vec::new();
        for (key, animation) in &self.scroll_animations {
            let elapsed = now.saturating_duration_since(animation.start_time).as_secs_f32();
            let duration = animation.duration.as_secs_f32().max(f32::EPSILON);
            let progress = (elapsed / duration).clamp(0.0, 1.0);
            let eased = mesh_core_animation::apply_easing(
                mesh_core_animation::Easing::EaseOut,
                progress,
            );
            let x = animation.start.x + (animation.target.x - animation.start.x) * eased;
            let y = animation.start.y + (animation.target.y - animation.start.y) * eased;
            updates.push((key.clone(), ScrollOffsetState { x, y }));
            if progress >= 1.0 {
                finished.push(key.clone());
            }
        }

        for (key, offset) in updates {
            self.scroll_offsets.insert(key, offset);
        }
        for key in finished {
            self.scroll_animations.remove(&key);
        }

        // Keep frames coming until every animation settles. This runs inside
        // `finalize_tree` (after the per-paint dirty flags were taken), so the
        // flag schedules the next frame via the cheap restyle path — mirroring
        // how keyframe animations re-arm themselves.
        if !self.scroll_animations.is_empty() {
            self.invalidate_style_path(ComponentDirtyFlags::VISUAL_REPAINT);
        }
    }

    /// Remove hover, focus, and active targets that no longer exist in the final
    /// post-restyle tree. Call this after every `build_tree` to ensure that nodes
    /// removed by a conditional render or hidden by `display:none` no longer
    /// receive interaction styling on the next frame.
    ///
    /// Sibling and ancestor state is preserved: only targets whose exact key is
    /// absent from `valid_keys` are cleared. Input, slider, checked, and scroll
    /// maps are never pruned here — their cleanup rules are deliberate and covered
    /// by separate logic when elements are explicitly removed by the user.
    pub(super) fn prune_stale_interaction_targets(&mut self, tree: &WidgetNode) {
        let mut valid_keys = std::collections::HashSet::new();
        collect_all_keys(tree, &mut valid_keys);

        if let Some(key) = &self.focused_key {
            if !valid_keys.contains(key) {
                self.focused_key = None;
                self.focus_visible_key = None;
            }
        }

        if let Some(key) = &self.focus_visible_key
            && !valid_keys.contains(key)
        {
            self.focus_visible_key = None;
        }

        if let Some(key) = &self.hovered_key {
            if !valid_keys.contains(key) {
                self.hovered_key = None;
                self.hovered_path.clear();
                self.hover_start = None;
                self.tooltip_visible = false;
                self.hovered_element_bounds = None;
                self.tooltip_appeared_at = None;
            }
        }

        if let Some(key) = &self.pointer_down_key {
            if !valid_keys.contains(key) {
                self.pointer_down_key = None;
                self.pointer_down_bounds = None;
            }
        }

        if let Some(key) = &self.active_slider_key {
            if !valid_keys.contains(key) {
                self.active_slider_key = None;
            }
        }

        let should_clear_selection = self
            .selection
            .as_ref()
            .is_some_and(|selection| !valid_keys.contains(&selection.anchor.node_key));
        if should_clear_selection {
            self.selection = None;
        }
    }
}
