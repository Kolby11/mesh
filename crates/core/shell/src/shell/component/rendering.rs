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
        self.profiling_records.push(ComponentProfilingRecord {
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
        self.profiling_records.push(ComponentProfilingRecord {
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
            rules.sort_by_key(|rule| selector_contains_state(&rule.selector));

            self.cached_restyle_rules = Some(rules);
        }
        self.cached_restyle_rules.as_deref().unwrap()
    }

    /// Whether any rule this surface restyles with — including rules pulled in
    /// from imported component modules — carries a state selector
    /// (`:hover`/`:focus`/...). Derived from the same cache as
    /// `module_restyle_rules` (which sorts state rules last), so it stays in
    /// sync with imported modules and hot source reloads.
    pub(super) fn module_styles_have_state_rules(&mut self) -> bool {
        self.module_restyle_rules()
            .last()
            .is_some_and(|rule| selector_contains_state(&rule.selector))
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
        self.render_stack.borrow_mut().clear();
        self.finalize_tree(
            &mut tree,
            theme,
            width,
            height,
            "rebuild",
            ComponentDirtyFlags::TREE_REBUILD,
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
    ) -> Option<WidgetNode> {
        let tree = self.build_tree_with_surface_css_props(theme, width, height, surface_css_props);
        // Narrow-script analysis currently feeds profiling telemetry only; the
        // finalized tree still takes the same full restyle/layout path. Avoid
        // snapshotting and hashing every node when nobody will consume those
        // measurements. Once affected subtrees drive rendering behavior this
        // gate can be removed alongside that implementation.
        if !self.profiling_enabled {
            return Some(tree);
        }
        let (affected, total) = self.retained_tree.narrow_script_diff(&tree)?;
        if affected.len() * 2 > total {
            return None;
        }
        let mut full_affected = affected.clone();
        narrow_expand_ancestors(&tree, &affected, &mut full_affected);
        self.affected_node_count = full_affected.len() as u64;
        self.narrow_path_active = true;
        Some(tree)
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
        surface_css_props: &SurfaceCssProps,
    ) {
        let _span =
            tracing::debug_span!("finalize_tree", surface = %self.id(), trigger_kind).entered();
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
            &self.scroll_offsets,
        );
        annotate_runtime_tree(tree, "root".to_string(), &mut annotation_context);
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
        annotate_overflow_tree(tree, "root", &mut self.scroll_offsets);

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
        let restyle_rules = self
            .cached_restyle_rules
            .as_deref()
            .expect("cache populated above");
        let targeted_interaction_restyle = trigger_kind == "restyle"
            && dirty_types.contains(ComponentDirtyFlags::STATE)
            && !dirty_types.intersects(ComponentDirtyFlags::SCRIPT | ComponentDirtyFlags::TEXT);
        // Compute affected keys before borrowing index_cache to satisfy the borrow checker.
        // Collect affected IDs before borrowing index_cache to satisfy the borrow checker.
        let affected_keys = if targeted_interaction_restyle {
            self.collect_interaction_changed_node_ids(tree)
        } else {
            InteractionChangedNodeIds::default()
        };
        let interaction_snapshot_valid = self.interaction_snapshot_valid;
        let index_cache = &mut self.cached_style_rule_index;
        let mut reused_retained_layout = false;
        let preserve_surface_root = tree.tag == "surface";
        if targeted_interaction_restyle {
            if affected_keys.affected.is_empty() {
                if !interaction_snapshot_valid {
                    // First frame or no previous interaction state — fall back
                    // to full-tree restyle.
                    if preserve_surface_root {
                        resolver.restyle_subtree_children_cached(
                            tree,
                            restyle_rules,
                            context,
                            index_cache,
                        );
                    } else {
                        resolver.restyle_subtree_cached(tree, restyle_rules, context, index_cache);
                    }
                    merge_runtime_primitive_defaults(tree);
                }
            } else {
                // Narrow restyle: only restyle state-changed nodes and their
                // descendants. Siblings, cousins, and unrelated subtrees are
                // left untouched.
                if preserve_surface_root {
                    // For surface root, restyle only children that are in
                    // the affected set. Use targeted restyle on each child.
                    for child in &mut tree.children {
                        resolver.restyle_subtree_for_ids_cached(
                            child,
                            restyle_rules,
                            context,
                            index_cache,
                            &affected_keys.affected,
                        );
                    }
                } else {
                    // For non-surface trees, restyle the entire tree but only
                    // nodes in affected_keys will be touched.
                    resolver.restyle_subtree_for_ids_cached(
                        tree,
                        restyle_rules,
                        context,
                        index_cache,
                        &affected_keys.affected,
                    );
                }
                merge_runtime_primitive_defaults_for_ids(tree, &affected_keys.roots);
            }
            reused_retained_layout = !dirty_types.contains(ComponentDirtyFlags::LAYOUT);
        } else {
            if preserve_surface_root {
                resolver.restyle_subtree_children_cached(tree, restyle_rules, context, index_cache);
            } else {
                resolver.restyle_subtree_cached(tree, restyle_rules, context, index_cache);
            }
            merge_runtime_primitive_defaults(tree);
        }
        let restyle_elapsed = restyle_started.elapsed();
        self.record_profiling_stage_with_elapsed(
            mesh_core_debug::ProfilingStage::StyleRestyle,
            restyle_elapsed,
            Some(trigger_kind),
        );
        // Re-borrow after the &mut self call above; the cache hasn't been
        // touched so the slice is identical to what restyle just consumed.
        let restyle_rules = self
            .cached_restyle_rules
            .as_deref()
            .expect("cache still populated");
        // The diagnostic pass re-resolves every node's style a second time to
        // surface rule errors (bad animation references etc.). Those are
        // properties of the rules and tree structure, so re-deriving them on
        // every restyle frame (hover transitions, animations) only burns CPU;
        // run the pass when the tree was rebuilt.
        if trigger_kind == "rebuild" {
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
        LayoutEngine::compute_incremental(
            tree,
            &mut self.layout_state,
            width as f32,
            height as f32,
            dirty_layout,
            dirty_structural,
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
        self.previous_hovered_path = self.hovered_path.clone();
        self.previous_focused_key = self.focused_key.clone();
        self.interaction_snapshot_valid = true;
    }

    /// Collects stable IDs for all nodes whose interaction
    /// state changed this frame plus all their descendants.
    ///
    /// Compares current hover/focus state against previous frame's state to
    /// identify which nodes changed.
    fn collect_interaction_changed_node_ids(&self, tree: &WidgetNode) -> InteractionChangedNodeIds {
        let mut changed_keys: HashSet<&str> = HashSet::new();

        // Collect keys that had hover change: union of old and new hovered paths.
        for key in &self.previous_hovered_path {
            changed_keys.insert(key.as_str());
        }
        for key in &self.hovered_path {
            changed_keys.insert(key.as_str());
        }

        // Collect keys that had focus change.
        if let Some(ref prev) = self.previous_focused_key {
            changed_keys.insert(prev.as_str());
        }
        if let Some(ref curr) = self.focused_key {
            changed_keys.insert(curr.as_str());
        }

        if changed_keys.is_empty() {
            return InteractionChangedNodeIds::default(); // first frame: no previous state
        }

        let mut ids = InteractionChangedNodeIds::default();
        collect_changed_subtree_node_ids(tree, &changed_keys, false, &mut ids);
        ids
    }

    pub(super) fn observe_surface_size(&mut self, width: u32, height: u32) -> bool {
        let size = (width.max(1), height.max(1));
        if self.last_surface_size == Some(size) {
            return false;
        }
        self.last_surface_size = Some(size);
        self.last_tree = None;
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
        tree: &WidgetNode,
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

    fn record_runtime_style_diagnostics_for_node(
        &self,
        node: &WidgetNode,
        rules: &[mesh_core_component::style::StyleRule],
        index: &mesh_core_elements::StyleRuleIndex,
        resolver: &StyleResolver,
        context: StyleContext,
    ) {
        let classes: Vec<String> = node
            .attributes
            .get("class")
            .map(|value| value.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default();
        let id = node.attributes.get("id").map(|value| value.as_str());
        let module_id = node.attributes.get("_mesh_module_id").map(String::as_str);
        let (_style, diagnostics) = resolver
            .resolve_node_style_with_diagnostics_for_module_indexed(
                rules, index, &node.tag, &classes, id, context, node.state, module_id,
            );

        for diagnostic in diagnostics {
            if diagnostic.message.contains("animation.")
                || diagnostic.property.starts_with("animation")
            {
                self.record_runtime_animation_diagnostic(diagnostic.message);
            }
        }

        for child in &node.children {
            self.record_runtime_style_diagnostics_for_node(child, rules, index, resolver, context);
        }
    }
}

fn merge_runtime_primitive_defaults(node: &mut WidgetNode) {
    if node.tag != "surface" {
        mesh_core_frontend::merge_missing_defaults(&node.tag, &mut node.computed_style);
    }
    for child in &mut node.children {
        merge_runtime_primitive_defaults(child);
    }
}

fn merge_runtime_primitive_defaults_for_ids(
    node: &mut WidgetNode,
    affected_ids: &HashSet<NodeId>,
) -> bool {
    let node_affected = affected_ids.contains(&node.id);
    if node_affected {
        merge_runtime_primitive_defaults(node);
        return true;
    }
    let mut descendant_affected = false;
    for child in &mut node.children {
        descendant_affected |= merge_runtime_primitive_defaults_for_ids(child, affected_ids);
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
struct InteractionChangedNodeIds {
    affected: HashSet<NodeId>,
    roots: HashSet<NodeId>,
}

fn collect_changed_subtree_node_ids(
    node: &WidgetNode,
    changed_keys: &HashSet<&str>,
    parent_affected: bool,
    out: &mut InteractionChangedNodeIds,
) {
    let node_key = node.attributes.get("_mesh_key");
    let directly_affected = node_key.is_some_and(|key| changed_keys.contains(key.as_str()));
    let node_affected = parent_affected || directly_affected;
    if node_affected {
        out.affected.insert(node.id);
    }
    if directly_affected && !parent_affected {
        out.roots.insert(node.id);
    }

    for child in &node.children {
        collect_changed_subtree_node_ids(child, changed_keys, node_affected, out);
    }
}

#[cfg(test)]
mod interaction_changed_key_tests {
    use super::*;
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
            .attributes
            .get("_mesh_key")
            .is_some_and(|key| key == &selection.anchor.node_key);
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
    fn collect_changed_subtree_node_ids_collects_descendants_in_one_walk() {
        let tree = keyed_node(
            "root",
            vec![
                keyed_node(
                    "root/0",
                    vec![
                        keyed_node("root/0/0", vec![]),
                        keyed_node("root/0/1", vec![]),
                    ],
                ),
                keyed_node("root/1", vec![keyed_node("root/1/0", vec![])]),
            ],
        );
        let changed = HashSet::from(["root/0"]);
        let mut affected = InteractionChangedNodeIds::default();

        collect_changed_subtree_node_ids(&tree, &changed, false, &mut affected);

        assert_eq!(
            affected.affected,
            HashSet::from([
                crate::shell::component::runtime_tree::stable_runtime_node_id("root/0"),
                crate::shell::component::runtime_tree::stable_runtime_node_id("root/0/0"),
                crate::shell::component::runtime_tree::stable_runtime_node_id("root/0/1"),
            ])
        );
        assert_eq!(
            affected.roots,
            HashSet::from([
                crate::shell::component::runtime_tree::stable_runtime_node_id("root/0")
            ])
        );
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

        merge_runtime_primitive_defaults_for_ids(&mut tree, &affected);

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

    // cargo test -p mesh-core-shell --release -- interaction_changed_node_ids_beat_runtime_key_clones --ignored --nocapture
    #[test]
    #[ignore = "release-only interaction changed-set microbenchmark"]
    fn interaction_changed_node_ids_beat_runtime_key_clones() {
        fn build(key: String, width: usize, depth: usize, next_id: &mut NodeId) -> WidgetNode {
            let mut node = WidgetNode::new("box");
            node.id = *next_id;
            *next_id += 1;
            node.attributes.insert("_mesh_key".into(), key.clone());
            if depth > 0 {
                node.children = (0..width)
                    .map(|index| build(format!("{key}/{index}"), width, depth - 1, next_id))
                    .collect();
            }
            node
        }
        fn old_collect(
            node: &WidgetNode,
            changed: &HashSet<String>,
            parent_affected: bool,
            out: &mut HashSet<String>,
        ) {
            let key = node.attributes.get("_mesh_key");
            let affected = parent_affected || key.is_some_and(|key| changed.contains(key));
            if affected && let Some(key) = key {
                out.insert(key.clone());
            }
            for child in &node.children {
                old_collect(child, changed, affected, out);
            }
        }

        let mut next_id = 1;
        let tree = build("root".into(), 4, 5, &mut next_id);
        let iterations = 2_000;
        let old_changed = HashSet::from(["root/0".to_string()]);
        let new_changed = HashSet::from(["root/0"]);

        let old_started = Instant::now();
        let mut old_count = 0;
        for _ in 0..iterations {
            let mut affected = HashSet::new();
            old_collect(&tree, &old_changed, false, &mut affected);
            old_count += std::hint::black_box(affected.len());
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_count = 0;
        for _ in 0..iterations {
            let mut affected = HashSet::new();
            let mut ids = InteractionChangedNodeIds::default();
            collect_changed_subtree_node_ids(&tree, &new_changed, false, &mut ids);
            affected.extend(ids.affected);
            new_count += std::hint::black_box(affected.len());
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "interaction changed set: String keys {old_time:?}; NodeId keys {new_time:?}; ratio {:.1}x; counts={old_count}/{new_count}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_count, new_count);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-shell --release -- targeted_default_merge_skips_unaffected_subtrees --ignored --nocapture
    #[test]
    #[ignore = "release-only targeted default merge microbenchmark"]
    fn targeted_default_merge_skips_unaffected_subtrees() {
        fn build(key: String, width: usize, depth: usize, next_id: &mut NodeId) -> WidgetNode {
            let id = *next_id;
            *next_id += 1;
            let mut node = WidgetNode::new(if depth % 2 == 0 { "column" } else { "text" });
            node.id = id;
            node.attributes.insert("_mesh_key".into(), key.clone());
            if depth > 0 {
                node.children = (0..width)
                    .map(|index| build(format!("{key}/{index}"), width, depth - 1, next_id))
                    .collect();
            }
            node
        }

        let mut next_id = 1;
        let tree = build("root".into(), 4, 5, &mut next_id);
        let mut affected = InteractionChangedNodeIds::default();
        collect_changed_subtree_node_ids(&tree, &HashSet::from(["root/0/0"]), false, &mut affected);
        let iterations = 5_000;

        let old_started = Instant::now();
        let mut old_total = 0.0f32;
        for _ in 0..iterations {
            let mut tree = tree.clone();
            merge_runtime_primitive_defaults(std::hint::black_box(&mut tree));
            old_total += std::hint::black_box(tree.children[0].computed_style.gap);
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0.0f32;
        for _ in 0..iterations {
            let mut tree = tree.clone();
            merge_runtime_primitive_defaults_for_ids(
                std::hint::black_box(&mut tree),
                std::hint::black_box(&affected.roots),
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
    if node
        .attributes
        .get("_mesh_key")
        .is_some_and(|value| value == key)
    {
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
    let mut parent_map = HashMap::new();
    narrow_build_parent_map(tree, None, &mut parent_map);
    for &leaf_id in affected.iter() {
        let mut current = leaf_id;
        while let Some(&parent) = parent_map.get(&current) {
            full_affected.insert(parent);
            current = parent;
        }
    }
}

fn narrow_build_parent_map(
    node: &WidgetNode,
    parent_id: Option<NodeId>,
    parent_map: &mut HashMap<NodeId, NodeId>,
) {
    if let Some(pid) = parent_id {
        parent_map.insert(node.id, pid);
    }
    for child in &node.children {
        narrow_build_parent_map(child, Some(node.id), parent_map);
    }
}
