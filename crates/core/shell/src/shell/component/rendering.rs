use super::*;

impl FrontendSurfaceComponent {
    pub(super) fn record_profiling_stage(
        &mut self,
        stage: mesh_core_debug::ProfilingStage,
        started_at: std::time::Instant,
        trigger_kind: Option<&str>,
    ) {
        if !self.profiling_enabled {
            return;
        }
        self.profiling_records
            .borrow_mut()
            .push(ComponentProfilingRecord {
                stage,
                duration: started_at.elapsed(),
                module_id: Some(self.compiled.manifest.package.id.clone()),
                trigger_kind: trigger_kind.map(str::to_string),
            });
    }

    pub(super) fn record_profiling_stage_with_elapsed(
        &mut self,
        stage: mesh_core_debug::ProfilingStage,
        elapsed: std::time::Duration,
        trigger_kind: Option<&str>,
    ) {
        if !self.profiling_enabled {
            return;
        }
        self.profiling_records
            .borrow_mut()
            .push(ComponentProfilingRecord {
                stage,
                duration: elapsed,
                module_id: Some(self.compiled.manifest.package.id.clone()),
                trigger_kind: trigger_kind.map(str::to_string),
            });
    }

    fn module_restyle_rules(&mut self) -> &[mesh_core_component::style::StyleRule] {
        if self.cached_restyle_rules.is_none() {
            let mut rules = Vec::new();

            if let Some(style) = self.compiled.component.style.as_ref() {
                rules.extend(style.rules.iter().cloned());
            }

            let mut aliases: Vec<_> = self.compiled.local_components.keys().cloned().collect();
            aliases.sort();
            for alias in aliases {
                if let Some(component) = self.compiled.local_components.get(&alias)
                    && let Some(style) = component.style.as_ref()
                {
                    rules.extend(style.rules.iter().cloned());
                }
            }
            let mut imported_module_ids: Vec<_> = self
                .compiled
                .module_component_imports
                .values()
                .filter(|module_id| module_id.as_str() != self.id())
                .cloned()
                .collect();
            imported_module_ids.sort();
            imported_module_ids.dedup();
            for module_id in imported_module_ids {
                let Some(entry) = self.frontend_catalog.modules.get(&module_id) else {
                    continue;
                };
                if let Some(style) = entry.compiled.component.style.as_ref() {
                    rules.extend(style.rules.iter().cloned());
                }
                let mut aliases: Vec<_> = entry.compiled.local_components.keys().cloned().collect();
                aliases.sort();
                for alias in aliases {
                    if let Some(component) = entry.compiled.local_components.get(&alias)
                        && let Some(style) = component.style.as_ref()
                    {
                        rules.extend(style.rules.iter().cloned());
                    }
                }
            }
            let mut state_dependencies = StyleStateDependencies::default();
            for rule in &rules {
                collect_selector_state_dependencies(&rule.selector, &mut state_dependencies);
            }
            self.cached_restyle_state_dependencies = state_dependencies;
            rules.sort_by_key(|rule| selector_contains_state(&rule.selector));

            self.cached_restyle_rules = Some(rules);
            self.style_rules_generation = self.style_rules_generation.saturating_add(1);
        }
        self.cached_restyle_rules.as_deref().unwrap()
    }

    /// Whether any rule this surface restyles with — including rules pulled in
    /// from imported component modules — carries a state selector
    /// (`:hover`/`:focus`/...). Derived from the same cache as
    /// `module_restyle_rules` (which sorts state rules last), so it stays in
    /// sync with imported modules and hot source reloads.
    pub(super) fn module_styles_have_state_rules(&mut self) -> bool {
        self.module_restyle_rules();
        self.cached_restyle_state_dependencies.any
    }

    /// Whether a pointer hover transition can change any rule in this surface,
    /// including rules contributed by imported component modules.
    pub(super) fn module_styles_have_hover_rules(&mut self) -> bool {
        self.module_restyle_rules();
        self.cached_restyle_state_dependencies.hover
    }

    pub(super) fn requested_layout_size(&self) -> (u32, u32) {
        // Every surface is CSS content-measured now. Until the first paint
        // populates `measured_size`, report `(0, 0)`: zero flows through the
        // render loop's dynamic-size / span resolution and `paint`'s
        // available-size fallback, so a spanning bar still spans and a popover
        // waits for its measured content size.
        self.measured_size.unwrap_or((0, 0))
    }

    pub(super) fn render_layout(&self, surface: &mut dyn ShellSurface) {
        // Promoted popovers are positioned entirely by their `xdg_positioner`;
        // the layer-surface anchor/margin/size pokes below do not apply and the
        // surface's `configure()` is skipped for popups anyway.
        if self.popup_promoted {
            return;
        }
        surface.anchor(self.surface_layout.edge);
        surface.set_layer(self.surface_layout.layer);
        let (width, height) = self.requested_layout_size();
        // Content size only — the tooltip overlay reserve is added at the
        // presentation boundary in `render_components`, never to the shell
        // surface record the component reads back.
        surface.set_size(width, height);
        surface.set_exclusive_zone(self.surface_layout.exclusive_zone);
        surface.set_keyboard_interactivity(
            self.keyboard_mode_override
                .unwrap_or(self.surface_layout.keyboard_mode),
        );
        surface.set_margin(
            self.surface_layout.margin_top,
            self.surface_layout.margin_right,
            self.surface_layout.margin_bottom,
            self.surface_layout.margin_left,
        );
    }

    /// Capture the paint-time theme into `active_theme` if it changed since
    /// the last capture. `theme_changed()` is the only path through which the
    /// shell swaps the active theme, so the stale flag is a complete signal.
    pub(super) fn refresh_active_theme(&self, theme: &Theme) {
        if self.active_theme_stale.get() {
            self.active_theme.replace(Arc::new(theme.clone()));
            self.active_theme_stale.set(false);
        }
    }

    /// The surface root's resolved CSS prop map (`prop(name)` → resolved value).
    ///
    /// Built from this surface component's own `<props>` and script state, so the
    /// restyle and animation passes resolve `prop()` references identically to the
    /// initial `build_tree_with_state` pass. (Embedded child components with their
    /// own `<props>` are resolved during build; the flat restyle pass uses the
    /// surface root's props — full per-instance restyle is future work.)
    pub(super) fn surface_css_props(&self) -> SurfaceCssProps {
        let state = self.runtime_state(self.id()).unwrap_or_default();
        let bound = LocaleBoundState::new(&state, &self.locale);
        mesh_core_frontend::resolve_css_props(self.compiled.component.props.as_ref(), Some(&bound))
    }

    pub(super) fn build_tree(&mut self, theme: &Theme, width: u32, height: u32) -> WidgetNode {
        let surface_css_props = self.surface_css_props();
        self.build_tree_with_surface_css_props(theme, width, height, &surface_css_props)
    }

    pub(super) fn build_tree_with_surface_css_props(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        surface_css_props: &SurfaceCssProps,
    ) -> WidgetNode {
        let _span = tracing::debug_span!("build_tree", surface = %self.id()).entered();
        if self.render_hooks_pending {
            self.call_render_hooks();
            self.render_hooks_pending = false;
        }
        self.refresh_active_theme(theme);
        let root_state = self.runtime_state(self.id()).unwrap_or_default();
        let bound = LocaleBoundState::new(&root_state, &self.locale);
        {
            let mut stack = self.render_stack.borrow_mut();
            stack.clear();
            stack.push(self.id().to_string());
        }
        self.composition_occurrences.borrow_mut().clear();
        self.has_promoted_popover_wrappers.set(false);
        self.has_error_placeholders.set(false);
        let measurer = SharedTextMeasurer;
        let tree_build_started = std::time::Instant::now();
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
        self.record_profiling_stage(
            mesh_core_debug::ProfilingStage::TreeBuild,
            tree_build_started,
            Some("rebuild"),
        );
        // Every runtime reachable during this build has now had its active
        // template expressions evaluated. This distinguishes a genuinely
        // empty dependency set from the pre-first-paint unknown state.
        for runtime in self.runtimes.lock().unwrap().values_mut() {
            runtime.script_ctx.mark_template_dependencies_ready();
        }
        self.render_stack.borrow_mut().clear();
        self.finalize_tree(
            &mut tree,
            theme,
            width,
            height,
            "rebuild",
            ComponentDirtyFlags::TREE_REBUILD,
            false,
            surface_css_props,
        );
        tree
    }

    pub(super) fn narrow_script_update(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        surface_css_props: &SurfaceCssProps,
    ) -> WidgetNode {
        let tree = self.build_tree_with_surface_css_props(theme, width, height, surface_css_props);
        let scope_is_safe = !self.surface_exiting
            && !self.surface_entering
            && self.closing_child_keys.is_empty()
            && self.entering_child_keys.is_empty();
        if scope_is_safe
            && let Some(affected) = self
                .last_tree
                .as_ref()
                .and_then(|previous| runtime_tree::narrow_script_dirty_roots(previous, &tree))
        {
            if self.profiling_enabled {
                let mut full_affected = affected.clone();
                narrow_expand_ancestors(&tree, &affected, &mut full_affected);
                self.affected_node_count = full_affected.len() as u64;
                self.narrow_path_active = true;
            }
            self.retained_update_dirty_roots = Some(affected);
        }
        // Structural/broad changes are already present in `tree`; do not
        // rebuild it a second time merely to label the profiling path as full.
        // The retained-tree update below is the authoritative fallback.
        tree
    }

    /// Retained fast path used when only appearance changed (e.g. hover)
    /// without any script-state mutation. Moves the previously built widget
    /// tree out of `last_tree`, mutates it in place, and returns it for paint.
    /// This avoids the old clone-the-whole-tree path and establishes the
    /// retained-tree cache boundary for later dirty-subtree work.
    pub(super) fn restyle_retained_tree(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        dirty_types: ComponentDirtyFlags,
        animation_only_frame: bool,
        surface_css_props: &SurfaceCssProps,
    ) -> Option<WidgetNode> {
        let _span = tracing::debug_span!("restyle", surface = %self.id()).entered();
        let mut tree = self.last_tree.take()?;
        self.refresh_active_theme(theme);
        self.finalize_tree(
            &mut tree,
            theme,
            width,
            height,
            "restyle",
            dirty_types,
            animation_only_frame,
            surface_css_props,
        );
        Some(tree)
    }

    fn finalize_tree(
        &mut self,
        tree: &mut WidgetNode,
        theme: &Theme,
        width: u32,
        height: u32,
        trigger_kind: &'static str,
        dirty_types: ComponentDirtyFlags,
        animation_only_frame: bool,
        surface_css_props: &SurfaceCssProps,
    ) {
        let _span =
            tracing::debug_span!("finalize_tree", surface = %self.id(), trigger_kind).entered();
        self.retained_update_dirty_roots = None;
        // Advance smooth-scroll animations before annotation reads scroll_offsets,
        // so the eased offset lands in this frame's `_mesh_scroll_*` attributes.
        self.advance_scroll_animations(std::time::Instant::now());
        let mut annotation_context = RuntimeAnnotationContext::new(
            &self.focused_key,
            &self.focus_visible_key,
            &self.hovered_path,
            &self.pointer_down_key,
            &self.active_slider_key,
            &self.input_values,
            &mut self.slider_values,
            &mut self.slider_script_values,
            &self.checked_values,
            &mut self.scroll_offsets,
        );
        annotate_runtime_and_overflow_tree(tree, "root".to_string(), &mut annotation_context);
        if self.surface_exiting {
            append_class_recursive(tree, "mesh-surface-exiting");
            tree.attributes
                .insert("_mesh_surface_exiting".into(), "true".into());
        }
        if self.surface_entering {
            append_class_recursive(tree, "mesh-surface-entering");
            tree.attributes
                .insert("_mesh_surface_entering".into(), "true".into());
        }
        for node_key in &self.closing_child_keys {
            if let Some(node) = find_node_by_key_mut(tree, node_key) {
                append_class_recursive(node, "mesh-surface-exiting");
                node.attributes
                    .insert("_mesh_surface_exiting".into(), "true".into());
            }
        }
        for node_key in &self.entering_child_keys {
            if let Some(node) = find_node_by_key_mut(tree, node_key) {
                append_class_recursive(node, "mesh-surface-entering");
                node.attributes
                    .insert("_mesh_surface_entering".into(), "true".into());
            }
        }
        self.annotate_surface_shortcuts(tree);
        if trigger_kind == "rebuild" {
            self.node_service_field_deps = NodeServiceFieldDependencies::build(tree);
        }

        // Populate the rule cache once, then run restyle while borrowing it.
        // The cache survives across paints — we only pay the clone cost on
        // source reload (when `cached_restyle_rules` is reset).
        self.module_restyle_rules();
        let resolver = StyleResolver::new(theme).with_props(surface_css_props.clone());
        let context = StyleContext {
            container_width: width as f32,
            container_height: height as f32,
        };
        let restyle_started = std::time::Instant::now();
        let collect_style_attribution = self.profiling_enabled;
        let restyle_rules = self
            .cached_restyle_rules
            .as_deref()
            .expect("cache populated above");
        let mut style_attribution = collect_style_attribution
            .then(|| mesh_core_elements::style::StyleRuleAttribution::new(restyle_rules));
        let targeted_interaction_restyle = trigger_kind == "restyle"
            && dirty_types.contains(ComponentDirtyFlags::STATE)
            && !dirty_types.intersects(ComponentDirtyFlags::SCRIPT | ComponentDirtyFlags::TEXT);
        let paint_only_restyle = trigger_kind == "restyle"
            && !dirty_types.is_empty()
            && dirty_types
                .difference(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS)
                .is_empty();
        // Compute affected keys before borrowing index_cache to satisfy the borrow checker.
        // Collect affected IDs before borrowing index_cache to satisfy the borrow checker.
        let affected_keys = if targeted_interaction_restyle {
            self.collect_interaction_changed_node_ids()
        } else {
            InteractionChangedNodeIds::default()
        };
        let interaction_snapshot_valid = self.interaction_snapshot_valid;
        let index_cache = &mut self.cached_style_rule_index;
        let mut reused_retained_layout = false;
        let mut empty_restyle_avoided = false;
        let preserve_surface_root = tree.tag == "surface";
        if targeted_interaction_restyle {
            if affected_keys.affected.is_empty() {
                if !interaction_snapshot_valid {
                    // First frame or no previous interaction state — fall back
                    // to full-tree restyle.
                    if preserve_surface_root {
                        if let Some(attribution) = style_attribution.as_mut() {
                            resolver.restyle_subtree_children_cached_profiled(
                                tree,
                                restyle_rules,
                                context,
                                index_cache,
                                attribution,
                            );
                        } else {
                            resolver.restyle_subtree_children_cached(
                                tree,
                                restyle_rules,
                                context,
                                index_cache,
                            );
                        }
                    } else {
                        if let Some(attribution) = style_attribution.as_mut() {
                            resolver.restyle_subtree_cached_profiled(
                                tree,
                                restyle_rules,
                                context,
                                index_cache,
                                attribution,
                            );
                        } else {
                            resolver.restyle_subtree_cached(
                                tree,
                                restyle_rules,
                                context,
                                index_cache,
                            );
                        }
                    }
                    apply_runtime_attribute_state(tree);
                } else {
                    empty_restyle_avoided = true;
                }
            } else {
                // Narrow restyle: only restyle state-changed nodes and their
                // descendants. Siblings, cousins, and unrelated subtrees are
                // left untouched.
                if preserve_surface_root {
                    // For surface root, restyle only children that are in
                    // the affected set. Use targeted restyle on each child.
                    for child in &mut tree.children {
                        if let Some(attribution) = style_attribution.as_mut() {
                            resolver.restyle_subtree_for_ids_cached_profiled(
                                child,
                                restyle_rules,
                                context,
                                index_cache,
                                &affected_keys.affected,
                                attribution,
                            );
                        } else {
                            resolver.restyle_subtree_for_ids_cached(
                                child,
                                restyle_rules,
                                context,
                                index_cache,
                                &affected_keys.affected,
                            );
                        }
                    }
                } else {
                    // For non-surface trees, restyle the entire tree but only
                    // nodes in affected_keys will be touched.
                    if let Some(attribution) = style_attribution.as_mut() {
                        resolver.restyle_subtree_for_ids_cached_profiled(
                            tree,
                            restyle_rules,
                            context,
                            index_cache,
                            &affected_keys.affected,
                            attribution,
                        );
                    } else {
                        resolver.restyle_subtree_for_ids_cached(
                            tree,
                            restyle_rules,
                            context,
                            index_cache,
                            &affected_keys.affected,
                        );
                    }
                }
                apply_runtime_attribute_state_for_ids(tree, &affected_keys.affected);
            }
            reused_retained_layout = !dirty_types.contains(ComponentDirtyFlags::LAYOUT);
        } else {
            if preserve_surface_root {
                if let Some(attribution) = style_attribution.as_mut() {
                    resolver.restyle_subtree_children_cached_profiled(
                        tree,
                        restyle_rules,
                        context,
                        index_cache,
                        attribution,
                    );
                } else {
                    resolver.restyle_subtree_children_cached(
                        tree,
                        restyle_rules,
                        context,
                        index_cache,
                    );
                }
            } else {
                if let Some(attribution) = style_attribution.as_mut() {
                    resolver.restyle_subtree_cached_profiled(
                        tree,
                        restyle_rules,
                        context,
                        index_cache,
                        attribution,
                    );
                } else {
                    resolver.restyle_subtree_cached(tree, restyle_rules, context, index_cache);
                }
            }
            apply_runtime_attribute_state(tree);
        }
        let restyle_elapsed = restyle_started.elapsed();
        self.record_profiling_stage_with_elapsed(
            mesh_core_debug::ProfilingStage::StyleRestyle,
            restyle_elapsed,
            Some(trigger_kind),
        );
        if let Some(attribution) = style_attribution {
            for entry in attribution.entries() {
                self.record_profiling_stage_with_elapsed(
                    mesh_core_debug::ProfilingStage::StyleRestyle,
                    std::time::Duration::from_micros(entry.elapsed_micros),
                    Some(&format!("attribution:style_rule:{}", entry.selector)),
                );
            }
        }
        if empty_restyle_avoided {
            self.record_profiling_stage_with_elapsed(
                mesh_core_debug::ProfilingStage::StyleRestyle,
                std::time::Duration::ZERO,
                Some("waste:empty_restyle_avoided"),
            );
        }
        // The diagnostic pass re-resolves every node's style a second time to
        // surface rule errors (bad animation references etc.). Those are
        // properties of the rules and selector-facing tree inputs. Rebuilds
        // commonly reproduce those inputs exactly, so fingerprint them before
        // paying for another full diagnostic resolution pass.
        let record_style_diagnostics = trigger_kind == "rebuild"
            && self.runtime_style_diagnostics_changed(tree, context, surface_css_props);
        // Re-borrow after the &mut self call above; the rule cache itself has
        // not changed, only the diagnostic fingerprint has.
        let restyle_rules = self
            .cached_restyle_rules
            .as_deref()
            .expect("cache still populated");
        if record_style_diagnostics {
            let style_index = self
                .cached_style_rule_index
                .as_ref()
                .expect("style index cache populated by restyle");
            self.record_runtime_style_diagnostics(
                tree,
                restyle_rules,
                style_index,
                &resolver,
                context,
            );
        }

        if tree.tag == "surface" {
            tree.computed_style.width = mesh_core_elements::Dimension::Px(width as f32);
            tree.computed_style.height = mesh_core_elements::Dimension::Px(height as f32);
        }

        // Re-apply the out-of-flow collapse for promoted `<popover>` wrappers. The
        // restyle pass above re-resolves `computed_style` from CSS only, dropping
        // the `position: absolute` set when the wrapper was composed. Without this,
        // a promoted (but hidden) popover's full-size subtree would lay out inline
        // and push its trigger row's siblings into overlap. Must run before layout.
        if self.has_promoted_popover_wrappers.get() {
            collapse_promoted_popover_wrappers(tree);
        }
        if self.has_error_placeholders.get() {
            constrain_error_placeholders(tree);
        }

        let layout_work_required = !reused_retained_layout || !self.layout_state.valid;
        // Enter the retained layout path on every finalized tree. On
        // VISUAL_REPAINT-only frames `compute_incremental` updates retained
        // styles and intentionally skips Taffy `compute_layout`; on invalid,
        // structural, size, or LAYOUT-dirty frames it recomputes geometry.
        let layout_started = std::time::Instant::now();
        let measurer = SharedTextMeasurer;
        let dirty_structural =
            dirty_types.intersects(ComponentDirtyFlags::SCRIPT | ComponentDirtyFlags::TEXT);
        let dirty_layout = dirty_types.contains(ComponentDirtyFlags::LAYOUT);
        let layout_dirty_node_ids = if dirty_layout && !dirty_structural {
            self.retained_tree.layout_dirty_node_ids(tree)
        } else {
            None
        };
        LayoutEngine::compute_incremental_with_dirty_nodes(
            tree,
            &mut self.layout_state,
            width as f32,
            height as f32,
            dirty_layout,
            dirty_structural,
            layout_dirty_node_ids.as_ref(),
            &mut self.intrinsic_layout_cache,
            Some(&measurer),
        );
        if layout_work_required {
            self.record_profiling_stage(
                mesh_core_debug::ProfilingStage::Layout,
                layout_started,
                Some(trigger_kind),
            );
        }
        self.annotate_selection_tree(tree, theme);

        // Store current interaction state for next frame's targeted restyle diff.
        // Preserve the prior frame's allocation while replacing its contents.
        // Interaction paths typically have the same depth across pointer moves.
        self.previous_hovered_path.clone_from(&self.hovered_path);
        self.previous_focused_key.clone_from(&self.focused_key);
        self.previous_focus_visible_key
            .clone_from(&self.focus_visible_key);
        self.previous_active_key.clone_from(&self.pointer_down_key);
        if self.cached_restyle_state_dependencies.checked {
            self.previous_checked_values
                .clone_from(&self.checked_values);
        } else {
            self.previous_checked_values.clear();
        }
        self.interaction_snapshot_valid = true;
        if ((targeted_interaction_restyle && interaction_snapshot_valid)
            || animation_only_frame
            || paint_only_restyle)
            && !self.surface_exiting
            && !self.surface_entering
            && self.closing_child_keys.is_empty()
            && self.entering_child_keys.is_empty()
        {
            self.retained_update_dirty_roots = Some(if targeted_interaction_restyle {
                affected_keys.affected
            } else {
                HashSet::new()
            });
        }
    }

    /// Collects stable IDs for nodes whose interaction state changed this frame.
    ///
    /// Compares every interaction-driven pseudo-state against the previous
    /// frame and returns only nodes whose changed state is referenced by this
    /// surface's selectors.
    pub(super) fn collect_interaction_changed_node_ids(&self) -> InteractionChangedNodeIds {
        let dependencies = self.cached_restyle_state_dependencies;
        let mut changed_ids =
            HashSet::with_capacity(self.previous_hovered_path.len() + self.hovered_path.len() + 6);

        // Hover paths are ordered root-to-leaf chains. Only the tails after
        // their common prefix changed hover state; walking those tails avoids
        // the quadratic pair of `Vec::contains` scans on deep trees.
        if dependencies.hover {
            collect_hover_changed_ids(
                &self.previous_hovered_path,
                &self.hovered_path,
                &mut changed_ids,
            );
        }

        // Collect keys that had focus change.
        if dependencies.focus && self.previous_focused_key != self.focused_key {
            if let Some(ref prev) = self.previous_focused_key {
                changed_ids.insert(runtime_node_id_for_key(prev));
            }
            if let Some(ref curr) = self.focused_key {
                changed_ids.insert(runtime_node_id_for_key(curr));
            }
        }

        if dependencies.focus_visible
            && (self.previous_focus_visible_key != self.focus_visible_key
                || self.previous_focused_key != self.focused_key)
        {
            if let Some(ref prev) = self.previous_focus_visible_key {
                changed_ids.insert(runtime_node_id_for_key(prev));
            }
            if let Some(ref curr) = self.focus_visible_key {
                changed_ids.insert(runtime_node_id_for_key(curr));
            }
            // Inputs implicitly expose focus-visible while focused even when
            // no explicit focus-visible key is stored, so logical focus is
            // part of the dependency for this pseudo-state too.
            if let Some(ref prev) = self.previous_focused_key {
                changed_ids.insert(runtime_node_id_for_key(prev));
            }
            if let Some(ref curr) = self.focused_key {
                changed_ids.insert(runtime_node_id_for_key(curr));
            }
        }

        if dependencies.active && self.previous_active_key != self.pointer_down_key {
            if let Some(ref prev) = self.previous_active_key {
                changed_ids.insert(runtime_node_id_for_key(prev));
            }
            if let Some(ref curr) = self.pointer_down_key {
                changed_ids.insert(runtime_node_id_for_key(curr));
            }
        }

        if dependencies.checked {
            for (key, value) in &self.previous_checked_values {
                if self.checked_values.get(key) != Some(value) {
                    changed_ids.insert(runtime_node_id_for_key(key));
                }
            }
            for (key, value) in &self.checked_values {
                if self.previous_checked_values.get(key) != Some(value) {
                    changed_ids.insert(runtime_node_id_for_key(key));
                }
            }
        }

        if changed_ids.is_empty() {
            return InteractionChangedNodeIds::default(); // first frame: no previous state
        }

        direct_interaction_changed_node_ids(changed_ids)
    }

    pub(super) fn observe_surface_size(&mut self, width: u32, height: u32) -> bool {
        let size = (width.max(1), height.max(1));
        if self.last_surface_size == Some(size) {
            return false;
        }
        self.last_surface_size = Some(size);
        // The retained tree is deliberately kept: the STYLE|LAYOUT flags below
        // already force a full rebuild at the new size, and the stale tree is
        // the diff source for `previous_visual_styles` — dropping it here
        // would cancel in-flight/starting transitions on every resize
        // (including the self-inflicted one-frame settle when a
        // content-measured surface adopts its measured size).
        // A new available size means the CSS content measurement must be redone:
        // clear the cached measurement so the next paint re-measures the root
        // against the new available space (e.g. a `width: 100%` root re-spans, a
        // container query re-evaluates). Without this the stale measured size
        // would feed back into `content_width` and pin the old dimensions.
        self.measured_size = None;
        self.surface_pixels_invalid = true;
        self.invalidate(
            ComponentDirtyFlags::STYLE
                | ComponentDirtyFlags::LAYOUT
                | ComponentDirtyFlags::PAINT
                | ComponentDirtyFlags::METRICS,
        );
        true
    }

    fn annotate_selection_tree(&self, tree: &mut WidgetNode, theme: &Theme) {
        let Some(selection) = &self.selection else {
            return;
        };
        let selection_background = theme
            .token("color.selection-background")
            .or_else(|| theme.token("color.primary"))
            .map(ToString::to_string)
            .unwrap_or_else(|| "#6750A4".to_string());
        let selection_foreground = theme
            .token("color.selection-foreground")
            .or_else(|| theme.token("color.on-primary"))
            .map(ToString::to_string)
            .unwrap_or_else(|| "#FFFFFF".to_string());
        if let Some(node) = find_node_by_key_mut(tree, &selection.anchor.node_key) {
            annotate_selected_text_node(
                node,
                selection,
                &selection_background,
                &selection_foreground,
            );
        }
    }

    fn record_runtime_style_diagnostics(
        &self,
        tree: &mut WidgetNode,
        rules: &[mesh_core_component::style::StyleRule],
        index: &mesh_core_elements::StyleRuleIndex,
        resolver: &StyleResolver,
        context: StyleContext,
    ) {
        // Skip the full diagnostic restyle pass when no diagnostics sink is
        // attached. Otherwise this walks the entire tree and re-runs style
        // resolution per node every paint, doubling restyle cost in the hot
        // path.
        if self.diagnostics.is_none() {
            return;
        }
        self.record_runtime_style_diagnostics_for_node(tree, rules, index, resolver, context);
    }

    fn runtime_style_diagnostics_changed(
        &mut self,
        tree: &WidgetNode,
        context: StyleContext,
        surface_css_props: &SurfaceCssProps,
    ) -> bool {
        if self.diagnostics.is_none() {
            self.runtime_style_diagnostic_fingerprint = None;
            return false;
        }
        let fingerprint = RuntimeStyleDiagnosticFingerprint {
            rules_generation: self.style_rules_generation,
            tree: runtime_style_diagnostic_tree_fingerprint(tree),
            props: runtime_style_diagnostic_props_fingerprint(surface_css_props),
            container_width: context.container_width.to_bits(),
            container_height: context.container_height.to_bits(),
        };
        runtime_style_diagnostic_inputs_changed(
            &mut self.runtime_style_diagnostic_fingerprint,
            fingerprint,
        )
    }

    fn record_runtime_style_diagnostics_for_node(
        &self,
        node: &mut WidgetNode,
        rules: &[mesh_core_component::style::StyleRule],
        index: &mesh_core_elements::StyleRuleIndex,
        resolver: &StyleResolver,
        context: StyleContext,
    ) {
        let (_style, diagnostics) = resolver
            .resolve_node_style_with_diagnostics_for_node_indexed(rules, index, node, context);

        for diagnostic in diagnostics {
            if diagnostic.message.contains("animation.")
                || diagnostic.property.starts_with("animation")
            {
                self.record_runtime_animation_diagnostic(diagnostic.message);
            }
        }

        for child in &mut node.children {
            self.record_runtime_style_diagnostics_for_node(child, rules, index, resolver, context);
        }
    }
}

fn collect_hover_changed_ids(
    previous: &[String],
    current: &[String],
    changed_ids: &mut HashSet<NodeId>,
) {
    let shared_prefix_len = previous
        .iter()
        .zip(current)
        .take_while(|(previous, current)| previous == current)
        .count();
    changed_ids.extend(
        previous[shared_prefix_len..]
            .iter()
            .chain(&current[shared_prefix_len..])
            .map(|key| runtime_node_id_for_key(key)),
    );
}

fn runtime_style_diagnostic_inputs_changed(
    previous: &mut Option<RuntimeStyleDiagnosticFingerprint>,
    current: RuntimeStyleDiagnosticFingerprint,
) -> bool {
    if *previous == Some(current) {
        return false;
    }
    *previous = Some(current);
    true
}

const DIAGNOSTIC_FNV_OFFSET: u64 = 0xcbf29ce484222325;
const DIAGNOSTIC_FNV_PRIME: u64 = 0x100000001b3;

fn diagnostic_hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(DIAGNOSTIC_FNV_PRIME);
    }
    // Separate adjacent fields, including absent and empty strings.
    *hash ^= 0xff;
    *hash = hash.wrapping_mul(DIAGNOSTIC_FNV_PRIME);
}

fn diagnostic_hash_optional_bytes(hash: &mut u64, bytes: Option<&[u8]>) {
    match bytes {
        Some(bytes) => {
            diagnostic_hash_bytes(hash, &[1]);
            diagnostic_hash_bytes(hash, bytes);
        }
        None => diagnostic_hash_bytes(hash, &[0]),
    }
}

fn runtime_style_diagnostic_tree_fingerprint(tree: &WidgetNode) -> u64 {
    fn visit(node: &WidgetNode, hash: &mut u64) {
        diagnostic_hash_bytes(hash, node.tag.as_bytes());
        diagnostic_hash_optional_bytes(
            hash,
            node.attributes.get("class").map(|value| value.as_bytes()),
        );
        diagnostic_hash_optional_bytes(
            hash,
            node.attributes.get("id").map(|value| value.as_bytes()),
        );
        diagnostic_hash_optional_bytes(hash, node.mesh_key().map(str::as_bytes));
        diagnostic_hash_optional_bytes(hash, node.module_id().map(str::as_bytes));
        let state = node.state;
        let state_bits = u16::from(state.hovered)
            | (u16::from(state.active) << 1)
            | (u16::from(state.focused) << 2)
            | (u16::from(state.focus_visible) << 3)
            | (u16::from(state.disabled) << 4)
            | (u16::from(state.read_only) << 5)
            | (u16::from(state.required) << 6)
            | (u16::from(state.selected) << 7)
            | (u16::from(state.checked) << 8)
            | (u16::from(state.expanded) << 9)
            | (u16::from(state.pressed) << 10)
            | (u16::from(state.invalid) << 11)
            | (u16::from(state.value) << 12);
        diagnostic_hash_bytes(hash, &state_bits.to_le_bytes());
        diagnostic_hash_bytes(hash, &(node.children.len() as u64).to_le_bytes());
        for child in &node.children {
            visit(child, hash);
        }
    }

    let mut hash = DIAGNOSTIC_FNV_OFFSET;
    visit(tree, &mut hash);
    hash
}

fn runtime_style_diagnostic_props_fingerprint(props: &SurfaceCssProps) -> u64 {
    use mesh_core_component::style::StyleValue;

    // Hash entries independently and combine commutatively: SurfaceCssProps is
    // rebuilt as a randomized HashMap each paint, so iteration order is not a
    // stable input and sorting would allocate on every diagnostic check.
    let mut combined = (props.len() as u64).wrapping_mul(DIAGNOSTIC_FNV_PRIME);
    for (name, value) in props {
        let mut entry = DIAGNOSTIC_FNV_OFFSET;
        diagnostic_hash_bytes(&mut entry, name.as_bytes());
        match value {
            StyleValue::Literal(value) => {
                diagnostic_hash_bytes(&mut entry, &[0]);
                diagnostic_hash_bytes(&mut entry, value.as_bytes());
            }
            StyleValue::Var(value) => {
                diagnostic_hash_bytes(&mut entry, &[1]);
                diagnostic_hash_bytes(&mut entry, value.as_bytes());
            }
            StyleValue::Prop(value) => {
                diagnostic_hash_bytes(&mut entry, &[2]);
                diagnostic_hash_bytes(&mut entry, value.as_bytes());
            }
        }
        combined ^= entry.rotate_left((entry & 63) as u32);
    }
    combined
}

fn apply_runtime_attribute_state(node: &mut WidgetNode) {
    apply_hidden_attribute_layout(node);
    for child in &mut node.children {
        apply_runtime_attribute_state(child);
    }
}

fn apply_hidden_attribute_layout(node: &mut WidgetNode) {
    let hidden = node.attributes.get("hidden").is_some_and(|value| {
        matches!(
            value.as_str(),
            "" | "true" | "1" | "hidden" | "disabled" | "checked"
        )
    });
    if hidden && !node.attributes.contains_key(PROMOTED_POPOVER_MARKER) {
        node.computed_style.display = mesh_core_elements::style::Display::None;
    }
}

fn apply_runtime_attribute_state_for_ids(
    node: &mut WidgetNode,
    affected_ids: &HashSet<NodeId>,
) -> bool {
    let node_affected = affected_ids.contains(&node.id);
    if node_affected {
        apply_runtime_attribute_state(node);
        return true;
    }
    let mut descendant_affected = false;
    for child in &mut node.children {
        descendant_affected |= apply_runtime_attribute_state_for_ids(child, affected_ids);
    }
    descendant_affected
}

/// Collapses promoted `<popover>` wrappers to a zero-size, overflow-visible box so
/// their (still full-size) popover subtree does not push trigger-row siblings around.
/// A zero flex-basis contributes nothing to the parent's layout, while the overflowing
/// popover content keeps its real size and stays anchored at the wrapper's in-flow
/// position — which child-surface paint and input translation rely on to locate the
/// promoted subtree. (Out-of-flow `position: absolute` would instead relocate the
/// subtree's layout coordinates, breaking that translation.) See
/// [`PROMOTED_POPOVER_MARKER`].
fn collapse_promoted_popover_wrappers(node: &mut WidgetNode) {
    if node.attributes.contains_key(PROMOTED_POPOVER_MARKER) {
        node.computed_style.width = mesh_core_elements::Dimension::Px(0.0);
        node.computed_style.height = mesh_core_elements::Dimension::Px(0.0);
        node.computed_style.min_width = Some(0.0);
        node.computed_style.min_height = Some(0.0);
        node.computed_style.overflow_x = mesh_core_elements::style::Overflow::Visible;
        node.computed_style.overflow_y = mesh_core_elements::style::Overflow::Visible;
    }
    for child in &mut node.children {
        collapse_promoted_popover_wrappers(child);
    }
}

/// Keeps generated component-error content from taking over its host layout.
/// These constraints are shell-owned and must be restored after CSS restyling,
/// just like promoted-popover geometry above.
pub(super) fn constrain_error_placeholders(node: &mut WidgetNode) {
    if node.attributes.contains_key(ERROR_PLACEHOLDER_MARKER) {
        node.computed_style.min_width = Some(0.0);
        node.computed_style.max_width = Some(ERROR_PLACEHOLDER_MAX_WIDTH);
        node.computed_style.flex_shrink = 1.0;
        node.computed_style.overflow_x = mesh_core_elements::style::Overflow::Hidden;
        node.computed_style.overflow_y = mesh_core_elements::style::Overflow::Hidden;
        node.computed_style.white_space = mesh_core_elements::style::WhiteSpace::Nowrap;
        node.computed_style.text_overflow = mesh_core_elements::style::TextOverflow::Ellipsis;
    }
    for child in &mut node.children {
        constrain_error_placeholders(child);
    }
}

#[derive(Default)]
pub(super) struct InteractionChangedNodeIds {
    pub(super) affected: HashSet<NodeId>,
}

fn direct_interaction_changed_node_ids(changed_ids: HashSet<NodeId>) -> InteractionChangedNodeIds {
    // `changed_ids` already contains exactly the nodes whose state toggled and
    // can serve directly as both targeted-restyle IDs and runtime-state roots.
    // When targets are nested, the ancestor runtime-state application consumes
    // its whole subtree and returns before the redundant descendant probe.
    InteractionChangedNodeIds {
        affected: changed_ids,
    }
}

#[cfg(test)]
mod interaction_changed_key_tests {
    use super::*;
    use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue};
    use std::time::Instant;

    fn keyed_node(key: &str, children: Vec<WidgetNode>) -> WidgetNode {
        let mut node = WidgetNode::new("box");
        node.id = crate::shell::component::runtime_tree::stable_runtime_node_id(key);
        node.attributes.insert("_mesh_key".into(), key.into());
        node.children = children;
        node
    }

    fn broad_plain_tree(width: usize, depth: usize) -> WidgetNode {
        fn build(level: usize, width: usize, depth: usize) -> WidgetNode {
            let mut node = WidgetNode::new("box");
            node.attributes
                .insert("_mesh_key".into(), format!("root/{level}"));
            if level < depth {
                node.children = (0..width)
                    .map(|index| {
                        let mut child = build(level + 1, width, depth);
                        child
                            .attributes
                            .insert("_mesh_key".into(), format!("root/{level}/{index}"));
                        child
                    })
                    .collect();
            }
            node
        }
        build(0, width, depth)
    }

    fn diagnostic_fingerprint(tree: &WidgetNode) -> RuntimeStyleDiagnosticFingerprint {
        RuntimeStyleDiagnosticFingerprint {
            rules_generation: 7,
            tree: runtime_style_diagnostic_tree_fingerprint(tree),
            props: 11,
            container_width: 800.0f32.to_bits(),
            container_height: 600.0f32.to_bits(),
        }
    }

    #[test]
    fn runtime_style_diagnostic_fingerprint_tracks_every_resolution_input() {
        let mut child = WidgetNode::new("button");
        child
            .attributes
            .insert("class".into(), "primary wide".into());
        child.attributes.insert("id".into(), "save".into());
        child.set_mesh_key("root/save");
        child.set_module_id("@test/controls");
        let mut tree = WidgetNode::new("surface");
        tree.children.push(child);
        let baseline = diagnostic_fingerprint(&tree);

        let assert_tree_change = |changed: WidgetNode| {
            assert_ne!(diagnostic_fingerprint(&changed), baseline);
        };

        let mut changed = tree.clone();
        changed.children[0].tag = "input".into();
        assert_tree_change(changed);
        let mut changed = tree.clone();
        changed.children[0]
            .attributes
            .insert("class".into(), "secondary".into());
        assert_tree_change(changed);
        let mut changed = tree.clone();
        changed.children[0]
            .attributes
            .insert("id".into(), "apply".into());
        assert_tree_change(changed);
        let mut changed = tree.clone();
        changed.children[0].state.focused = true;
        assert_tree_change(changed);
        let mut changed = tree.clone();
        changed.children[0].set_module_id("@test/alternate");
        assert_tree_change(changed);
        let mut changed = tree.clone();
        changed.children[0].set_mesh_key("root/apply");
        assert_tree_change(changed);
        let mut changed = tree.clone();
        changed.children.push(WidgetNode::new("text"));
        assert_tree_change(changed);

        let mut changed = baseline;
        changed.rules_generation += 1;
        assert_ne!(changed, baseline);
        let mut changed = baseline;
        changed.container_width = 801.0f32.to_bits();
        assert_ne!(changed, baseline);
        let mut changed = baseline;
        changed.container_height = 601.0f32.to_bits();
        assert_ne!(changed, baseline);
        let mut changed = baseline;
        changed.props += 1;
        assert_ne!(changed, baseline);
    }

    #[test]
    fn runtime_style_diagnostic_gate_reuses_only_identical_inputs() {
        let tree = broad_plain_tree(3, 3);
        let fingerprint = diagnostic_fingerprint(&tree);
        let mut previous = None;
        assert!(runtime_style_diagnostic_inputs_changed(
            &mut previous,
            fingerprint
        ));
        assert!(!runtime_style_diagnostic_inputs_changed(
            &mut previous,
            fingerprint
        ));
        let changed = RuntimeStyleDiagnosticFingerprint {
            rules_generation: fingerprint.rules_generation + 1,
            ..fingerprint
        };
        assert!(runtime_style_diagnostic_inputs_changed(
            &mut previous,
            changed
        ));
    }

    #[test]
    fn runtime_style_diagnostic_props_hash_is_order_independent_and_value_sensitive() {
        let mut left = SurfaceCssProps::new();
        left.insert("accent".into(), StyleValue::Literal("#abcdef".into()));
        left.insert("spacing".into(), StyleValue::Var("--space-md".into()));
        let mut right = SurfaceCssProps::new();
        right.insert("spacing".into(), StyleValue::Var("--space-md".into()));
        right.insert("accent".into(), StyleValue::Literal("#abcdef".into()));
        assert_eq!(
            runtime_style_diagnostic_props_fingerprint(&left),
            runtime_style_diagnostic_props_fingerprint(&right)
        );
        right.insert("accent".into(), StyleValue::Literal("#fedcba".into()));
        assert_ne!(
            runtime_style_diagnostic_props_fingerprint(&left),
            runtime_style_diagnostic_props_fingerprint(&right)
        );
    }

    // cargo test -p mesh-core-shell --release -- runtime_style_diagnostic_fingerprint_gate_beats_full_reresolve -- --ignored --nocapture
    #[test]
    #[ignore = "release-only runtime style diagnostic gate microbenchmark"]
    fn runtime_style_diagnostic_fingerprint_gate_beats_full_reresolve() {
        fn resolve_tree(
            node: &mut WidgetNode,
            rules: &[StyleRule],
            index: &mesh_core_elements::StyleRuleIndex,
            resolver: &StyleResolver,
            context: StyleContext,
        ) -> usize {
            let (style, diagnostics) = resolver
                .resolve_node_style_with_diagnostics_for_node_indexed(rules, index, node, context);
            let mut total = diagnostics.len() + style.opacity.to_bits() as usize;
            for child in &mut node.children {
                total = total.wrapping_add(resolve_tree(child, rules, index, resolver, context));
            }
            total
        }

        let mut tree = broad_plain_tree(5, 3);
        fn decorate(node: &mut WidgetNode, index: &mut usize) {
            node.tag = if (*index).is_multiple_of(3) {
                "button".into()
            } else {
                "box".into()
            };
            node.attributes
                .insert("class".into(), "card interactive".into());
            node.attributes.insert("id".into(), format!("node-{index}"));
            node.set_module_id("@bench/module");
            *index += 1;
            for child in &mut node.children {
                decorate(child, index);
            }
        }
        let mut node_index = 0;
        decorate(&mut tree, &mut node_index);
        let declarations = [
            ("opacity", "0.8"),
            ("padding", "8px"),
            ("margin", "2px"),
            ("border-width", "1px"),
            ("border-radius", "4px"),
            ("width", "120px"),
            ("height", "24px"),
            ("font-size", "13px"),
        ]
        .into_iter()
        .map(|(property, value)| Declaration {
            property: property.into(),
            value: StyleValue::Literal(value.into()),
        })
        .collect();
        let rules = vec![StyleRule {
            selector: Selector::Class("card".into()),
            declarations,
            container_query: None,
        }];
        let index = mesh_core_elements::StyleRuleIndex::new(&rules);
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let context = StyleContext {
            container_width: 800.0,
            container_height: 600.0,
        };
        let iterations = 2_000;

        // Warm class/default caches equally before timing repeated unchanged rebuilds.
        std::hint::black_box(resolve_tree(&mut tree, &rules, &index, &resolver, context));
        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total = old_total.wrapping_add(resolve_tree(
                std::hint::black_box(&mut tree),
                &rules,
                &index,
                &resolver,
                context,
            ));
        }
        let old_time = old_started.elapsed();

        let mut previous = Some(RuntimeStyleDiagnosticFingerprint {
            rules_generation: 1,
            tree: runtime_style_diagnostic_tree_fingerprint(&tree),
            props: 0,
            container_width: context.container_width.to_bits(),
            container_height: context.container_height.to_bits(),
        });
        let gated_started = Instant::now();
        let mut gated_changes = 0usize;
        for _ in 0..iterations {
            let current = RuntimeStyleDiagnosticFingerprint {
                rules_generation: 1,
                tree: runtime_style_diagnostic_tree_fingerprint(std::hint::black_box(&tree)),
                props: 0,
                container_width: context.container_width.to_bits(),
                container_height: context.container_height.to_bits(),
            };
            gated_changes += usize::from(runtime_style_diagnostic_inputs_changed(
                &mut previous,
                current,
            ));
        }
        let gated_time = gated_started.elapsed();

        eprintln!(
            "runtime style diagnostics over {iterations} unchanged rebuilds: full re-resolve {old_time:?}; fingerprint gate {gated_time:?}; ratio {:.1}x; old_total={old_total} gated_changes={gated_changes}",
            old_time.as_secs_f64() / gated_time.as_secs_f64()
        );
        assert_eq!(gated_changes, 0);
        assert!(gated_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- hover_snapshot_clone_from_reuses_path_storage -- --ignored --nocapture
    #[test]
    #[ignore = "release-only hover snapshot storage microbenchmark"]
    fn hover_snapshot_clone_from_reuses_path_storage() {
        let current: Vec<String> = (0..32)
            .map(|index| format!("root/section/{index}/button"))
            .collect();
        let iterations = 100_000usize;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let snapshot = std::hint::black_box(current.clone());
            old_total += std::hint::black_box(snapshot.len());
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_snapshot = current.clone();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            new_snapshot.clone_from(std::hint::black_box(&current));
            new_total += std::hint::black_box(new_snapshot.len());
        }
        let new_time = new_started.elapsed();

        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
        eprintln!(
            "hover snapshot: assignment clone {old_time:?}; clone_from {new_time:?}; ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
    }

    fn broad_keyed_tree_with_selected_text(width: usize, depth: usize) -> (WidgetNode, String) {
        fn build(key: String, width: usize, depth: usize, target: &str) -> WidgetNode {
            let mut node = if key == target {
                let mut text = WidgetNode::new("text");
                text.attributes.insert("selectable".into(), "true".into());
                text.attributes.insert("content".into(), "selected".into());
                text
            } else {
                WidgetNode::new("box")
            };
            node.attributes.insert("_mesh_key".into(), key.clone());
            node.layout.x = key.len() as f32;
            node.layout.y = key.len() as f32 * 0.5;
            if depth > 0 {
                node.children = (0..width)
                    .map(|index| build(format!("{key}/{index}"), width, depth - 1, target))
                    .collect();
            }
            node
        }

        let target = "root/3/3/3/3/3".to_string();
        (build("root".into(), width, depth, &target), target)
    }

    fn old_annotate_selection_node(
        node: &mut WidgetNode,
        selection: &TextSelectionState,
        selection_background: &str,
        selection_foreground: &str,
    ) -> bool {
        let matches_selection = node
            .mesh_key()
            .is_some_and(|key| key == selection.anchor.node_key);
        if matches_selection
            && annotate_selected_text_node(
                node,
                selection,
                selection_background,
                selection_foreground,
            )
        {
            return true;
        }

        for child in &mut node.children {
            if old_annotate_selection_node(
                child,
                selection,
                selection_background,
                selection_foreground,
            ) {
                return true;
            }
        }

        false
    }

    fn benchmark_selection(target: String) -> TextSelectionState {
        TextSelectionState {
            anchor: TextSelectionPoint {
                node_key: target.clone(),
                x: 2.0,
                y: 3.0,
            },
            focus: TextSelectionPoint {
                node_key: target,
                x: 18.0,
                y: 3.0,
            },
            dragging: true,
        }
    }

    #[test]
    fn direct_interaction_scope_keeps_only_changed_targets() {
        let changed = HashSet::from([stable_runtime_node_id("root/0")]);
        let affected = direct_interaction_changed_node_ids(changed);

        assert_eq!(
            affected.affected,
            HashSet::from([
                crate::shell::component::runtime_tree::stable_runtime_node_id("root/0")
            ])
        );
    }

    #[test]
    fn direct_interaction_scope_keeps_nested_targets() {
        let parent = stable_runtime_node_id("root/0");
        let child = stable_runtime_node_id("root/0/0");
        let changed = HashSet::from([parent, child]);
        let affected = direct_interaction_changed_node_ids(changed);

        assert_eq!(affected.affected, HashSet::from([parent, child]));
    }

    #[test]
    fn hover_changed_ids_only_collects_tails_after_common_ancestor() {
        let previous = ["root", "root/menu", "root/menu/left"]
            .map(str::to_string)
            .to_vec();
        let current = [
            "root",
            "root/menu",
            "root/menu/right",
            "root/menu/right/icon",
        ]
        .map(str::to_string)
        .to_vec();
        let mut changed = HashSet::new();

        collect_hover_changed_ids(&previous, &current, &mut changed);

        assert_eq!(
            changed,
            HashSet::from([
                stable_runtime_node_id("root/menu/left"),
                stable_runtime_node_id("root/menu/right"),
                stable_runtime_node_id("root/menu/right/icon"),
            ])
        );
    }

    // cargo test -p mesh-core-shell --release -- hover_common_prefix_beats_symmetric_contains_scans --ignored --nocapture
    #[test]
    #[ignore = "release-only hover-path diff microbenchmark"]
    fn hover_common_prefix_beats_symmetric_contains_scans() {
        use std::time::Instant;

        let previous = (0..64)
            .map(|depth| {
                format!(
                    "root/{}",
                    (0..=depth).map(|_| "left").collect::<Vec<_>>().join("/")
                )
            })
            .collect::<Vec<_>>();
        let mut current = previous[..63].to_vec();
        current.push(format!("{}/right", current.last().unwrap()));
        let iterations = 100_000usize;

        let mut old_changed = HashSet::with_capacity(previous.len() + current.len());
        let old_started = Instant::now();
        for _ in 0..iterations {
            old_changed.clear();
            for key in &previous {
                if !current.contains(key) {
                    old_changed.insert(stable_runtime_node_id(key));
                }
            }
            for key in &current {
                if !previous.contains(key) {
                    old_changed.insert(stable_runtime_node_id(key));
                }
            }
            std::hint::black_box(&old_changed);
        }
        let old_elapsed = old_started.elapsed();

        let mut prefix_changed = HashSet::with_capacity(previous.len() + current.len());
        let prefix_started = Instant::now();
        for _ in 0..iterations {
            prefix_changed.clear();
            collect_hover_changed_ids(&previous, &current, &mut prefix_changed);
            std::hint::black_box(&prefix_changed);
        }
        let prefix_elapsed = prefix_started.elapsed();

        assert_eq!(prefix_changed, old_changed);
        eprintln!(
            "hover path diff over {iterations} 64-level transitions: contains {old_elapsed:?}; common-prefix {prefix_elapsed:?}; ratio {:.1}x",
            old_elapsed.as_secs_f64() / prefix_elapsed.as_secs_f64()
        );
        assert!(prefix_elapsed * 2 < old_elapsed);
    }

    // cargo test -p mesh-core-shell --release -- narrow_ancestor_stack_beats_parent_map_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only narrow ancestor expansion microbenchmark"]
    fn narrow_ancestor_stack_beats_parent_map_benchmark() {
        fn old_build_parent_map(
            node: &WidgetNode,
            parent_id: Option<NodeId>,
            parents: &mut HashMap<NodeId, NodeId>,
        ) {
            if let Some(parent_id) = parent_id {
                parents.insert(node.id, parent_id);
            }
            for child in &node.children {
                old_build_parent_map(child, Some(node.id), parents);
            }
        }

        fn old_expand(tree: &WidgetNode, affected: &HashSet<NodeId>) -> HashSet<NodeId> {
            let mut full_affected = affected.clone();
            let mut parents = HashMap::new();
            old_build_parent_map(tree, None, &mut parents);
            for &leaf_id in affected {
                let mut current = leaf_id;
                while let Some(&parent) = parents.get(&current) {
                    full_affected.insert(parent);
                    current = parent;
                }
            }
            full_affected
        }

        fn branch(key: &str, depth: usize) -> WidgetNode {
            let children = (depth > 0)
                .then(|| {
                    (0..4)
                        .map(|index| branch(&format!("{key}/{index}"), depth - 1))
                        .collect()
                })
                .unwrap_or_default();
            keyed_node(key, children)
        }

        let tree = branch("root", 5);
        let affected = HashSet::from([
            stable_runtime_node_id("root/0/1/2/3/0"),
            stable_runtime_node_id("root/2/3/0/1/2"),
            stable_runtime_node_id("root/3/2/1/0/3"),
        ]);
        let iterations = 2_000;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total ^= std::hint::black_box(old_expand(&tree, &affected).len());
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let mut full_affected = affected.clone();
            narrow_expand_ancestors(&tree, &affected, &mut full_affected);
            new_total ^= std::hint::black_box(full_affected.len());
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "narrow ancestors: parent map {old_time:?}; stack walk {new_time:?}; ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    #[test]
    fn targeted_default_merge_only_updates_affected_subtrees() {
        let mut tree = keyed_node(
            "root",
            vec![
                keyed_node("root/0", vec![keyed_node("root/0/0", vec![])]),
                keyed_node("root/1", vec![keyed_node("root/1/0", vec![])]),
            ],
        );
        tree.children[0].tag = "column".into();
        tree.children[0].children[0].tag = "text".into();
        tree.children[0].children[0].computed_style.color = mesh_core_elements::Color::TRANSPARENT;
        tree.children[1].tag = "column".into();
        tree.children[1].children[0].tag = "text".into();
        tree.children[1].children[0].computed_style.color = mesh_core_elements::Color::TRANSPARENT;

        let affected = HashSet::from([
            crate::shell::component::runtime_tree::stable_runtime_node_id("root/0"),
        ]);

        apply_runtime_attribute_state_for_ids(&mut tree, &affected);

        assert_eq!(
            tree.children[0].computed_style.direction,
            mesh_core_elements::style::FlexDirection::Column
        );
        assert_eq!(tree.children[0].children[0].computed_style.color.a, 255);
        assert_eq!(
            tree.children[1].computed_style.direction,
            mesh_core_elements::style::FlexDirection::Row
        );
        assert_eq!(tree.children[1].children[0].computed_style.color.a, 0);
    }

    // cargo test -p mesh-core-shell --release -- direct_interaction_scope_beats_full_tree_walk --ignored --nocapture
    #[test]
    #[ignore = "release-only direct interaction-scope microbenchmark"]
    fn direct_interaction_scope_beats_full_tree_walk() {
        fn build(key: String, width: usize, depth: usize) -> WidgetNode {
            let mut node = WidgetNode::new("box");
            node.id = stable_runtime_node_id(&key);
            node.attributes.insert("_mesh_key".into(), key.clone());
            if depth > 0 {
                node.children = (0..width)
                    .map(|index| build(format!("{key}/{index}"), width, depth - 1))
                    .collect();
            }
            node
        }
        fn tree_walk_collect(
            node: &WidgetNode,
            changed: &HashSet<NodeId>,
            out: &mut InteractionChangedNodeIds,
        ) {
            let directly_affected = changed.contains(&node.id);
            if directly_affected {
                out.affected.insert(node.id);
            }
            for child in &node.children {
                tree_walk_collect(child, changed, out);
            }
        }

        let tree = build("root".into(), 4, 5);
        let iterations = 2_000;
        let changed = HashSet::from([stable_runtime_node_id("root/0")]);

        let old_started = Instant::now();
        let mut old_count = 0;
        for _ in 0..iterations {
            let mut affected = InteractionChangedNodeIds::default();
            tree_walk_collect(&tree, &changed, &mut affected);
            old_count += std::hint::black_box(affected.affected.len());
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_count = 0;
        for _ in 0..iterations {
            let affected = direct_interaction_changed_node_ids(changed.clone());
            new_count += std::hint::black_box(affected.affected.len());
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "interaction changed scope: full tree walk {old_time:?}; direct IDs {new_time:?}; ratio {:.1}x; counts={old_count}/{new_count}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_count, new_count);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- targeted_default_merge_skips_unaffected_subtrees --ignored --nocapture
    #[test]
    #[ignore = "release-only targeted default merge microbenchmark"]
    fn targeted_default_merge_skips_unaffected_subtrees() {
        fn build(key: String, width: usize, depth: usize) -> WidgetNode {
            let mut node = WidgetNode::new(if depth % 2 == 0 { "column" } else { "text" });
            node.id = stable_runtime_node_id(&key);
            node.attributes.insert("_mesh_key".into(), key.clone());
            if depth > 0 {
                node.children = (0..width)
                    .map(|index| build(format!("{key}/{index}"), width, depth - 1))
                    .collect();
            }
            node
        }

        let tree = build("root".into(), 4, 5);
        let affected =
            direct_interaction_changed_node_ids(HashSet::from([stable_runtime_node_id(
                "root/0/0",
            )]));
        let iterations = 5_000;

        let old_started = Instant::now();
        let mut old_total = 0.0f32;
        for _ in 0..iterations {
            let mut tree = tree.clone();
            apply_runtime_attribute_state(std::hint::black_box(&mut tree));
            old_total += std::hint::black_box(tree.children[0].computed_style.gap);
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0.0f32;
        for _ in 0..iterations {
            let mut tree = tree.clone();
            apply_runtime_attribute_state_for_ids(
                std::hint::black_box(&mut tree),
                std::hint::black_box(&affected.affected),
            );
            new_total += std::hint::black_box(tree.children[0].computed_style.gap);
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "runtime primitive defaults: full tree {old_time:?}; targeted {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- finalize_marker_walk_gates_skip_plain_trees --ignored --nocapture
    #[test]
    #[ignore = "release-only finalize marker walk microbenchmark"]
    fn finalize_marker_walk_gates_skip_plain_trees() {
        let mut tree = broad_plain_tree(4, 5);
        let iterations = 20_000;

        let old_started = Instant::now();
        for _ in 0..iterations {
            collapse_promoted_popover_wrappers(std::hint::black_box(&mut tree));
            constrain_error_placeholders(std::hint::black_box(&mut tree));
        }
        let old_time = old_started.elapsed();

        let gated_started = Instant::now();
        let has_promoted_popover_wrappers = false;
        let has_error_placeholders = false;
        for _ in 0..iterations {
            if has_promoted_popover_wrappers {
                collapse_promoted_popover_wrappers(std::hint::black_box(&mut tree));
            }
            if has_error_placeholders {
                constrain_error_placeholders(std::hint::black_box(&mut tree));
            }
        }
        let gated_time = gated_started.elapsed();

        eprintln!(
            "finalize marker walks plain tree: {old_time:?}; gated: {gated_time:?}; ratio: {:.1}x",
            old_time.as_secs_f64() / gated_time.as_secs_f64()
        );
        assert!(gated_time * 10 < old_time);
    }

    #[test]
    fn keyed_selection_annotation_only_marks_selectable_text_target() {
        let target = "root/0".to_string();
        let selection = benchmark_selection(target.clone());
        let mut selectable = WidgetNode::new("text");
        selectable.attributes.insert("_mesh_key".into(), target);
        selectable
            .attributes
            .insert("selectable".into(), "true".into());
        assert!(annotate_selected_text_node(
            &mut selectable,
            &selection,
            "#112233",
            "#ffffff"
        ));
        assert!(
            selectable
                .attributes
                .contains_key("_mesh_selection_background")
        );

        let mut non_selectable = WidgetNode::new("text");
        non_selectable
            .attributes
            .insert("_mesh_key".into(), "root/0".into());
        assert!(!annotate_selected_text_node(
            &mut non_selectable,
            &selection,
            "#112233",
            "#ffffff"
        ));
        assert!(
            !non_selectable
                .attributes
                .contains_key("_mesh_selection_background")
        );
    }

    // cargo test -p mesh-core-shell --release -- keyed_selection_annotation_beats_recursive_tree_walk --ignored --nocapture
    #[test]
    #[ignore = "release-only selection annotation microbenchmark"]
    fn keyed_selection_annotation_beats_recursive_tree_walk() {
        let (tree, target) = broad_keyed_tree_with_selected_text(4, 5);
        let selection = benchmark_selection(target);
        let iterations = 10_000;

        let old_started = Instant::now();
        let mut old_count = 0usize;
        for _ in 0..iterations {
            let mut tree = tree.clone();
            old_count += usize::from(old_annotate_selection_node(
                std::hint::black_box(&mut tree),
                &selection,
                "#112233",
                "#ffffff",
            ));
        }
        let old_time = old_started.elapsed();

        let keyed_started = Instant::now();
        let mut keyed_count = 0usize;
        for _ in 0..iterations {
            let mut tree = tree.clone();
            if let Some(node) = find_node_by_key_mut(&mut tree, &selection.anchor.node_key) {
                keyed_count += usize::from(annotate_selected_text_node(
                    std::hint::black_box(node),
                    &selection,
                    "#112233",
                    "#ffffff",
                ));
            }
        }
        let keyed_time = keyed_started.elapsed();

        eprintln!(
            "selection annotation: recursive {old_time:?}; keyed {keyed_time:?}; ratio {:.1}x; counts={old_count}/{keyed_count}",
            old_time.as_secs_f64() / keyed_time.as_secs_f64()
        );
        assert_eq!(old_count, keyed_count);
        assert!(keyed_time < old_time);
    }
}

fn selector_contains_state(selector: &mesh_core_component::style::Selector) -> bool {
    use mesh_core_component::style::Selector;

    match selector {
        Selector::State(_, _) => true,
        Selector::Compound(parts) => parts.iter().any(selector_contains_state),
        Selector::Tag(_) | Selector::Class(_) | Selector::Id(_) | Selector::Universal => false,
    }
}

fn collect_selector_state_dependencies(
    selector: &mesh_core_component::style::Selector,
    dependencies: &mut StyleStateDependencies,
) {
    use mesh_core_component::style::Selector;

    match selector {
        Selector::State(_, state) => {
            dependencies.any = true;
            match state.as_str() {
                "hover" | "hovered" => dependencies.hover = true,
                "focus" | "focused" => dependencies.focus = true,
                "focus-visible" => dependencies.focus_visible = true,
                "active" => dependencies.active = true,
                "disabled" => dependencies.disabled = true,
                "checked" => dependencies.checked = true,
                _ => {}
            }
        }
        Selector::Compound(parts) => {
            for part in parts {
                collect_selector_state_dependencies(part, dependencies);
            }
        }
        Selector::Tag(_) | Selector::Class(_) | Selector::Id(_) | Selector::Universal => {}
    }
}

fn append_class(node: &mut WidgetNode, class_name: &str) {
    let class = node.attributes.entry("class".into()).or_default();
    let has_class = class
        .split_whitespace()
        .any(|candidate| candidate == class_name);
    if has_class {
        return;
    }
    if !class.is_empty() {
        class.push(' ');
    }
    class.push_str(class_name);
}

pub(super) fn append_class_recursive(node: &mut WidgetNode, class_name: &str) {
    append_class(node, class_name);
    for child in &mut node.children {
        append_class_recursive(child, class_name);
    }
}

fn find_node_by_key_mut<'a>(node: &'a mut WidgetNode, key: &str) -> Option<&'a mut WidgetNode> {
    if node.mesh_key().is_some_and(|value| value == key) {
        return Some(node);
    }
    for child in &mut node.children {
        if let Some(found) = find_node_by_key_mut(child, key) {
            return Some(found);
        }
    }
    None
}

fn annotate_selected_text_node(
    node: &mut WidgetNode,
    selection: &TextSelectionState,
    selection_background: &str,
    selection_foreground: &str,
) -> bool {
    let matches_selection = node.tag == "text"
        && node
            .attributes
            .get("selectable")
            .is_some_and(|value| matches!(value.as_str(), "" | "true" | "1"));
    if matches_selection {
        node.attributes.insert(
            "_mesh_selection_background".into(),
            selection_background.to_string(),
        );
        node.attributes.insert(
            "_mesh_selection_foreground".into(),
            selection_foreground.to_string(),
        );
        node.attributes.insert(
            "_mesh_selection_anchor_x".into(),
            format!("{:.2}", selection.anchor.x),
        );
        node.attributes.insert(
            "_mesh_selection_anchor_y".into(),
            format!("{:.2}", selection.anchor.y),
        );
        node.attributes.insert(
            "_mesh_selection_focus_x".into(),
            format!("{:.2}", selection.focus.x),
        );
        node.attributes.insert(
            "_mesh_selection_focus_y".into(),
            format!("{:.2}", selection.focus.y),
        );
        node.attributes.insert(
            "_mesh_selection_text_x".into(),
            format!("{:.2}", node.layout.x + node.computed_style.padding.left),
        );
        node.attributes.insert(
            "_mesh_selection_text_y".into(),
            format!("{:.2}", node.layout.y + node.computed_style.padding.top),
        );
        return true;
    }

    false
}

pub(super) fn narrow_expand_ancestors(
    tree: &WidgetNode,
    affected: &HashSet<NodeId>,
    full_affected: &mut HashSet<NodeId>,
) {
    let mut ancestors = Vec::new();
    narrow_collect_ancestors(tree, affected, &mut ancestors, full_affected);
}

fn narrow_collect_ancestors(
    node: &WidgetNode,
    affected: &HashSet<NodeId>,
    ancestors: &mut Vec<NodeId>,
    full_affected: &mut HashSet<NodeId>,
) {
    if affected.contains(&node.id) {
        full_affected.extend(ancestors.iter().copied());
    }
    ancestors.push(node.id);
    for child in &node.children {
        narrow_collect_ancestors(child, affected, ancestors, full_affected);
    }
    ancestors.pop();
}
