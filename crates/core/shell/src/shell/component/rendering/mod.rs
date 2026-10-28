mod annotate;

#[cfg(test)]
mod interaction_changed_key_tests;
#[cfg(test)]
mod tests;

pub(in crate::shell::component) use annotate::*;

use super::*;

fn changed_map_keys<V: PartialEq>(
    previous: &HashMap<NodeId, V>,
    current: &HashMap<NodeId, V>,
    changed: &mut HashSet<NodeId>,
) {
    for (id, value) in previous {
        if current.get(id) != Some(value) {
            changed.insert(*id);
        }
    }
    for (id, value) in current {
        if previous.get(id) != Some(value) {
            changed.insert(*id);
        }
    }
}

fn state_rule_requires_layout<'a>(
    selector: &mesh_core_component::style::Selector,
    mut properties: impl Iterator<Item = &'a str>,
) -> bool {
    selector_contains_state(selector)
        && properties.any(|property| {
            // Unknown properties and custom variables stay conservative: a
            // variable can feed an inherited font or a descendant's dimensions.
            !matches!(
                property,
                "color"
                    | "background"
                    | "background-color"
                    | "background-image"
                    | "border-color"
                    | "border-radius"
                    | "border-top-left-radius"
                    | "border-top-right-radius"
                    | "border-bottom-left-radius"
                    | "border-bottom-right-radius"
                    | "corner-shape"
                    | "corner-top-left-shape"
                    | "corner-top-right-shape"
                    | "corner-bottom-right-shape"
                    | "corner-bottom-left-shape"
                    | "opacity"
                    | "transform"
                    | "transform-origin"
                    | "box-shadow"
                    | "outline"
                    | "outline-width"
                    | "outline-color"
                    | "outline-offset"
                    | "outline-style"
                    | "filter"
                    | "backdrop-filter"
                    | "mix-blend-mode"
                    | "z-index"
                    | "visibility"
                    | "text-align"
                    | "text-overflow"
                    | "icon-fill"
                    | "icon-weight"
                    | "icon-grade"
                    | "icon-optical-size"
            )
        })
}

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
        mesh_core_debug::allocation::with_tracking_suspended(|| {
            self.profiling_records
                .borrow_mut()
                .push(ComponentProfilingRecord {
                    stage,
                    duration: started_at.elapsed(),
                    module_id: Some(self.compiled.manifest.package.id.clone()),
                    trigger_kind: trigger_kind.map(str::to_string),
                });
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
        mesh_core_debug::allocation::with_tracking_suspended(|| {
            self.profiling_records
                .borrow_mut()
                .push(ComponentProfilingRecord {
                    stage,
                    duration: elapsed,
                    module_id: Some(self.compiled.manifest.package.id.clone()),
                    trigger_kind: trigger_kind.map(str::to_string),
                });
        });
    }

    fn module_restyle_rules(&mut self) -> &[mesh_core_component::style::StyleRule] {
        let current_contribution_count = self
            .frontend_catalog
            .contribution_entries_for_host(self.id())
            .len();
        if self.cached_restyle_rules.is_some()
            && current_contribution_count != self.cached_restyle_rules_contribution_count
        {
            self.cached_restyle_rules = None;
        }
        if self.cached_restyle_rules.is_none() {
            self.cached_restyle_rules_contribution_count = current_contribution_count;
            let mut rules = Vec::new();

            if let Some(style) = self.compiled.component.style.as_ref() {
                rules.extend(style.rules.iter().cloned());
            }

            for component in self.compiled.all_local_components() {
                if let Some(style) = component.style.as_ref() {
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
            // Extension-point contributions are alternate compiled roots of
            // this surface's module. They are not local components and do not
            // appear in the primary entrypoint's import map, but their CSS is
            // still part of the rendered tree. Keep it in the retained
            // restyle rule set or the first click/hover repaint will resolve
            // those nodes from theme defaults and drop their authored
            // dimensions and interaction styles.
            let contribution_entries: Vec<_> = self
                .frontend_catalog
                .contribution_entries_for_host(self.id())
                .into_iter()
                .cloned()
                .collect();
            for compiled in &contribution_entries {
                if let Some(style) = compiled.component.style.as_ref() {
                    rules.extend(style.rules.iter().cloned());
                }
                for component in compiled.all_local_components() {
                    if let Some(style) = component.style.as_ref() {
                        rules.extend(style.rules.iter().cloned());
                    }
                }
                imported_module_ids.extend(
                    compiled
                        .module_component_imports
                        .values()
                        .filter(|module_id| module_id.as_str() != self.id())
                        .cloned(),
                );
            }
            // Imported component modules render inside this surface too, and so
            // do the modules *they* import (e.g. navigation-bar → theme-selector
            // → bubble-options). Follow imports transitively; stopping at the
            // first level restyles nested module nodes from theme defaults.
            imported_module_ids.sort();
            imported_module_ids.dedup();
            imported_module_ids.reverse();
            let mut visited = std::collections::HashSet::new();
            while let Some(module_id) = imported_module_ids.pop() {
                let module_id = self
                    .frontend_catalog
                    .dependency_forks
                    .get(&module_id)
                    .cloned()
                    .unwrap_or(module_id);
                if module_id == self.id() || !visited.insert(module_id.clone()) {
                    continue;
                }
                let Some(entry) = self.frontend_catalog.modules.get(&module_id) else {
                    continue;
                };
                if let Some(style) = entry.compiled.component.style.as_ref() {
                    rules.extend(style.rules.iter().cloned());
                }
                for component in entry.compiled.all_local_components() {
                    if let Some(style) = component.style.as_ref() {
                        rules.extend(style.rules.iter().cloned());
                    }
                }
                imported_module_ids.extend(
                    entry
                        .compiled
                        .module_component_imports
                        .values()
                        .filter(|nested| !visited.contains(*nested))
                        .cloned(),
                );
            }
            let mut state_dependencies = StyleStateDependencies::default();
            let mut layout_state_dependencies = StyleStateDependencies::default();
            for rule in &rules {
                collect_selector_state_dependencies(&rule.selector, &mut state_dependencies);
                if state_rule_requires_layout(
                    &rule.selector,
                    rule.declarations
                        .iter()
                        .map(|declaration| declaration.property.as_str()),
                ) {
                    collect_selector_state_dependencies(
                        &rule.selector,
                        &mut layout_state_dependencies,
                    );
                }
            }
            let theme = self.active_theme.borrow();
            for rule in theme
                .style_rules()
                .iter()
                .chain(theme.modules().values().flat_map(|module| &module.rules))
            {
                collect_selector_state_dependencies(&rule.selector, &mut state_dependencies);
                if state_rule_requires_layout(
                    &rule.selector,
                    rule.declarations
                        .iter()
                        .map(|(property, _)| property.as_str()),
                ) {
                    collect_selector_state_dependencies(
                        &rule.selector,
                        &mut layout_state_dependencies,
                    );
                }
            }
            for rule in theme.user_stylesheet() {
                collect_selector_state_dependencies(&rule.state, &mut state_dependencies);
                if state_rule_requires_layout(
                    &rule.state,
                    rule.declarations.iter().map(|decl| decl.property.as_str()),
                ) {
                    collect_selector_state_dependencies(
                        &rule.state,
                        &mut layout_state_dependencies,
                    );
                }
            }
            self.cached_restyle_state_dependencies = state_dependencies;
            self.cached_layout_state_dependencies = layout_state_dependencies;
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
    #[cfg(test)]
    pub(super) fn module_styles_have_state_rules(&mut self) -> bool {
        self.module_restyle_rules();
        self.cached_restyle_state_dependencies.mask != 0
    }

    /// Whether a pointer hover transition can change any rule in this surface,
    /// including active theme rules and imported component modules.
    pub(super) fn module_styles_have_hover_rules(&mut self) -> bool {
        self.module_restyle_rules();
        self.cached_restyle_state_dependencies
            .contains(mesh_core_elements::PseudoState::Hovered)
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
        surface.set_role(self.surface_layout.role);
        if self.surface_layout.role == mesh_core_wayland::SurfaceRole::Window {
            surface.set_window_options(self.resolved_window_options());
        }
        surface.anchor(self.surface_layout.edge);
        surface.set_layer(self.surface_layout.layer);
        let (measured_width, measured_height) = self.requested_layout_size();
        // In layer-shell, zero is the protocol spelling for "span the output"
        // when the surface is anchored to both opposing edges. A spanning
        // root's measured width is the output's, but feeding it back would
        // turn a spanning bar into a fixed-width surface that some compositors
        // centre with gaps on both sides, so that axis stays dynamic. A fit or
        // fixed-width root sends its measured width, which the compositor
        // centres along the edge.
        let (width, height) = layer_surface_request_size(
            self.surface_layout.edge,
            self.surface_layout.exclusive_zone,
            self.surface_fills_output,
            self.surface_spans_width,
            (measured_width, measured_height),
        );
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
        surface.set_blur(self.surface_layout.blur);
    }

    /// Resolve the toplevel-only settings against this component's locale and
    /// module identity.
    ///
    /// The title is localized here rather than at settings-load time because
    /// that is where the locale lives; an unset title falls back to the module
    /// id so a window is never nameless in a task switcher. The app id defaults
    /// to the module id too — it is what compositor window rules match on, so
    /// it must be stable and predictable rather than derived from a title the
    /// user can change.
    fn resolved_window_options(&self) -> mesh_core_wayland::WindowOptions {
        let module_id = self.compiled.manifest.package.id.clone();
        let title = match &self.surface_layout.window.title {
            Some(text) => {
                self.resolve_manifest_text(&module_id, "mesh.surface.title", text)
                    .text
            }
            None => module_id.clone(),
        };
        mesh_core_wayland::WindowOptions {
            title,
            app_id: self
                .surface_layout
                .window
                .app_id
                .clone()
                .unwrap_or(module_id),
            resizable: self.surface_layout.window.resizable,
            decorations: self.surface_layout.window.decorations,
        }
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
        let state = self
            .runtime_state(self.root_instance_key())
            .unwrap_or_default();
        let bound = LocaleBoundState::with_localized_misses(
            &state,
            self.locale
                .module_translator(&self.compiled.manifest.package.id),
            self.localized_misses_handle(self.root_instance_key()),
        );
        mesh_core_frontend::resolve_css_props(self.compiled.component.props.as_ref(), Some(&bound))
    }

    /// Container-query size for the component root. An axis this paint has
    /// no size for is laid out against a 1px stand-in, and querying that would
    /// match every `max-*` condition: after each geometry-changing configure,
    /// while the compositor's ack is pending, a rule changing the root's
    /// margin or size would flip and reconfigure the surface again, forever.
    /// Such an axis keeps the last size the surface really had instead, or
    /// the initial output hint before the first definite paint.
    pub(super) fn root_style_container(&self, width: u32, height: u32) -> StyleContext {
        let (width_unmeasured, height_unmeasured) = self.unmeasured_root_axes;
        let (last_width, last_height) = self.last_definite_root_size;
        let width = last_width
            .or(self.initial_style_container_size.map(|size| size.0))
            .filter(|_| width_unmeasured)
            .unwrap_or(width);
        let height = last_height
            .or(self.initial_style_container_size.map(|size| size.1))
            .filter(|_| height_unmeasured)
            .unwrap_or(height);
        StyleContext {
            container_width: width as f32,
            container_height: height as f32,
        }
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
        self.build_tree_with_surface_css_props_inner(theme, width, height, surface_css_props, None)
    }

    fn build_tree_with_surface_css_props_inner(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        surface_css_props: &SurfaceCssProps,
        selective: Option<(&WidgetNode, &HashSet<NodeId>)>,
    ) -> WidgetNode {
        let _span = tracing::debug_span!("build_tree", surface = %self.id()).entered();
        if self.reactive_effects_pending {
            self.flush_reactive_effects();
            self.reactive_effects_pending = false;
        }
        self.refresh_active_theme(theme);
        self.pending_component_prop_refreshes.clear();
        self.component_subtrees_patched = false;
        let root_state = self
            .runtime_state(self.root_instance_key())
            .unwrap_or_default();
        let bound = LocaleBoundState::with_localized_misses(
            &root_state,
            self.locale
                .module_translator(&self.compiled.manifest.package.id),
            self.localized_misses_handle(self.root_instance_key()),
        );
        {
            let mut stack = self.render_stack.borrow_mut();
            stack.clear();
            stack.push(self.root_instance_key().to_string());
        }
        self.composition_occurrences.borrow_mut().clear();
        self.has_promoted_popover_wrappers.set(false);
        self.has_error_placeholders.set(false);
        self.template_expression_failure.borrow_mut().take();
        let tree_build_started = std::time::Instant::now();
        let fallback_tree = selective.map(|(previous, _)| previous);
        let mut tree = if let Some(runtime_failure) = self.runtime_failure.borrow().clone() {
            self.build_error_widget(runtime_failure)
        } else {
            self.compiled.build_tree_for_finalization(
                theme,
                width,
                height,
                Some(&bound),
                self.root_instance_key(),
                self,
                selective,
            )
        };
        if let Some((_failed_instance_key, failure)) =
            self.template_expression_failure.borrow_mut().take()
        {
            self.composition_occurrences.borrow_mut().clear();
            self.pending_surface_states.borrow_mut().clear();
            self.portal_hidden_bindings.borrow_mut().clear();
            tree = fallback_tree
                .cloned()
                .or_else(|| self.last_tree.clone())
                .unwrap_or_else(|| self.build_error_widget(failure));
        } else {
            self.template_patch_recovery_pending.set(false);
        }
        if let Err(error) = mesh_core_elements::validate_widget_tree(&tree) {
            tracing::error!(
                component = %self.id(),
                error = %error,
                "rejecting invalid widget tree before shell finalization"
            );
            tree = self.build_error_widget(format!("invalid widget tree: {error}"));
        }
        #[cfg(test)]
        {
            self.last_template_build_reused_nodes = selective
                .map(|(previous, _)| count_shared_authored_nodes(&tree, previous))
                .unwrap_or(0);
        }
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
        self.pending_service_template_nodes = None;
        self.pending_service_binding_readers.clear();
        let localized_misses = self
            .runtimes
            .lock()
            .unwrap()
            .values_mut()
            .flat_map(|runtime| runtime.script_ctx.drain_localized_misses())
            .collect::<Vec<_>>();
        for resolution in localized_misses {
            self.record_localized_miss(&resolution, None);
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
        let mut previous = self.last_tree.take();
        let pending_service_nodes = self.pending_service_template_nodes.take();
        self.pending_service_binding_readers.clear();
        let root_lifecycle_settled = !self.surface_exiting && !self.surface_entering;
        let scope_is_safe = root_lifecycle_settled
            && self.closing_child_keys.is_empty()
            && self.entering_child_keys.is_empty();
        let mut selective_rebuild_nodes = pending_service_nodes.clone().unwrap_or_default();
        if let (Some(previous), Some(_)) = (previous.as_ref(), pending_service_nodes.as_ref()) {
            let affected = selective_rebuild_nodes.clone();
            narrow_expand_ancestors(previous, &affected, &mut selective_rebuild_nodes);
        }
        // The render hooks for this frame have already run (`paint` drives
        // them before taking the dirty flags). Only a hook that actually wrote
        // a template input can invalidate nodes outside the service-derived
        // set, so gate on what the hooks did rather than on their existence.
        let can_selectively_build = root_lifecycle_settled
            && !self.template_patch_recovery_pending.get()
            && !selective_rebuild_nodes.is_empty()
            && self.selective_service_build_supported
            && !self.effects_changed_templates;
        #[cfg(test)]
        let can_selectively_build = can_selectively_build && !self.force_full_template_build;
        let tree = if can_selectively_build {
            // Reused branches carry the last frame's lifecycle markers. Rebuild
            // finalization must capture their authored classes before reapplying them.
            if !scope_is_safe && let Some(previous) = previous.as_mut() {
                restore_lifecycle_classes(
                    previous,
                    self.last_frame_snapshot.as_ref(),
                    &mut self.lifecycle_base_classes,
                );
            }
            self.build_tree_with_surface_css_props_inner(
                theme,
                width,
                height,
                surface_css_props,
                previous
                    .as_ref()
                    .map(|previous| (previous, &selective_rebuild_nodes)),
            )
        } else {
            if let Some(previous) = previous.as_ref() {
                // The narrow path takes the retained tree out before a full
                // rebuild. Put a copy back as the fallback input so a failed
                // expression cannot replace the last-known-good frame with
                // an error placeholder merely because the update was broad.
                self.last_tree = Some(previous.clone());
            }
            self.build_tree_with_surface_css_props(theme, width, height, surface_css_props)
        };
        if scope_is_safe
            && let Some(affected) = self
                .last_tree
                .as_ref()
                .or(previous.as_ref())
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
    /// tree out of `last_tree`, mutates it in place, and returns it for paint,
    /// so appearance-only frames never clone the tree.
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
        let replaced = std::mem::take(&mut self.component_subtrees_patched);
        // Input keeps the last painted geometry until paint commits the queued
        // child refreshes; another pointer event may arrive before this frame.
        let mut pending = std::mem::take(&mut self.pending_component_prop_refreshes);
        pending.sort_by(|a, b| a.0.cmp(&b.0));
        for (path, recipe) in pending {
            let Some(node) = mesh_core_frontend::FrontendCompositionResolver::render_import(
                self,
                &recipe.host,
                &recipe.parent,
                recipe.owner.as_deref(),
                &recipe.alias,
                recipe.ordinal,
                recipe.duplicate,
                false,
                None,
                &recipe.props,
                &recipe.handlers,
                recipe.container.0,
                recipe.container.1,
            ) else {
                self.last_tree = Some(tree);
                return None;
            };
            let paths = [path.as_slice()];
            let mut replacement = Some(node);
            mesh_core_elements::for_each_node_at_paths_mut(&mut tree, &paths, |_, target| {
                *target = replacement.take().expect("one child refresh target");
            });
        }
        if replaced {
            self.interaction_snapshot_valid = false;
            self.layout_state.valid = false;
        }
        self.finalize_tree(
            &mut tree,
            theme,
            width,
            height,
            if replaced { "rebuild" } else { "restyle" },
            if replaced {
                dirty_types | ComponentDirtyFlags::SCRIPT
            } else {
                dirty_types
            },
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
        let mut dirty_types = dirty_types;
        self.retained_update_dirty_roots = None;
        self.retained_update_dirty_nodes.clear();
        self.frame_restyled_nodes = None;
        if trigger_kind == "restyle" {
            restore_lifecycle_classes(
                tree,
                self.last_frame_snapshot.as_ref(),
                &mut self.lifecycle_base_classes,
            );
        } else {
            self.lifecycle_base_classes.clear();
        }
        // Advance smooth-scroll animations before annotation reads scroll_offsets,
        // so the eased offset lands in this frame's `_mesh_scroll_*` attributes.
        let now = std::time::Instant::now();
        self.refresh_motion_policy(now);
        self.advance_scroll_animations(now);
        self.advance_scroll_inertia(now);
        if self.has_surface_shortcut_declarations() {
            self.resolved_surface_shortcuts(&self.current_keyboard_settings());
        }
        let mut scoped_finalize = trigger_kind == "restyle"
            && (dirty_types.contains(ComponentDirtyFlags::STATE)
                || animation_only_frame
                || !self.patched_binding_nodes.is_empty()
                || dirty_types
                    .difference(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS)
                    .is_empty())
            && !dirty_types.intersects(ComponentDirtyFlags::SCRIPT | ComponentDirtyFlags::TEXT)
            && self.interaction_snapshot_valid;
        let scoped_interaction_frame = scoped_finalize;
        #[cfg(test)]
        let scoped_interaction_frame =
            scoped_interaction_frame && !self.force_full_interaction_frames;
        // Inputs stored by the previous annotation describe a projection whose
        // overflow extents are still current; only the nodes whose inputs
        // changed, and scroll containers, need projecting again.
        let scoped_annotation_ids = self
            .scoped_annotation_inputs
            .take()
            .filter(|_| scoped_interaction_frame && self.last_frame_snapshot.is_some())
            .and_then(|inputs| self.annotation_input_changes(&inputs));
        let overflow_was_scoped;
        let mut annotation_dirty = {
            let shortcut_cache = self.resolved_surface_shortcuts_cache.borrow();
            let mut annotation_context = RuntimeAnnotationContext::new(
                self.focused_id
                    .or_else(|| self.focused_key.as_deref().map(runtime_node_id_for_key)),
                self.focus_visible_id.or_else(|| {
                    self.focus_visible_key
                        .as_deref()
                        .map(runtime_node_id_for_key)
                }),
                &self.hovered_path,
                self.pointer_down_id,
                self.active_slider_id,
                &self.input_values,
                &self.input_preedits,
                &mut self.slider_values,
                &mut self.slider_script_values,
                &self.checked_values,
                &mut self.scroll_offsets,
            )
            .with_retained_tree(scoped_finalize)
            .with_window_state(self.window_states)
            .with_promoted_windows(&self.promoted_window_keys)
            .with_expanded_triggers(&self.expanded_triggers)
            .with_shortcuts(
                shortcut_cache
                    .as_ref()
                    .map(|cache| &cache.shortcuts_by_keybind),
            );
            let scoped = scoped_annotation_ids.as_ref().is_some_and(|ids| {
                annotate_runtime_nodes(
                    tree,
                    self.last_frame_snapshot.as_ref().expect("checked above"),
                    ids,
                    &mut annotation_context,
                )
            });
            overflow_was_scoped = scoped;
            #[cfg(test)]
            {
                self.scoped_annotation_frames += u64::from(scoped);
            }
            if !scoped && trigger_kind == "rebuild" {
                super::runtime_tree::annotate_runtime_tree(
                    tree,
                    "root".to_string(),
                    &mut annotation_context,
                );
            } else if !scoped {
                annotate_runtime_and_overflow_tree(
                    tree,
                    "root".to_string(),
                    &mut annotation_context,
                );
            }
            let changed = std::mem::take(&mut annotation_context.changed);
            drop(annotation_context);
            drop(shortcut_cache);
            changed
        };
        let annotation_inputs = (!self.surface_exiting
            && !self.surface_entering
            && self.closing_child_keys.is_empty()
            && self.entering_child_keys.is_empty())
        .then(|| self.capture_annotation_inputs());
        if self.surface_exiting {
            append_lifecycle_class(
                tree,
                "mesh-surface-exiting",
                &mut self.lifecycle_base_classes,
            );
            tree.state.insert(NodeState::SURFACE_EXITING);
        }
        if self.surface_entering {
            append_lifecycle_class(
                tree,
                "mesh-surface-entering",
                &mut self.lifecycle_base_classes,
            );
            tree.state.insert(NodeState::SURFACE_ENTERING);
        }
        for node_key in &self.closing_child_keys {
            if let Some(node) =
                lifecycle_node_mut(tree, self.last_frame_snapshot.as_ref(), node_key)
            {
                append_lifecycle_class(
                    node,
                    "mesh-surface-exiting",
                    &mut self.lifecycle_base_classes,
                );
                node.state.insert(NodeState::SURFACE_EXITING);
            }
        }
        for node_key in &self.entering_child_keys {
            if let Some(node) =
                lifecycle_node_mut(tree, self.last_frame_snapshot.as_ref(), node_key)
            {
                append_lifecycle_class(
                    node,
                    "mesh-surface-entering",
                    &mut self.lifecycle_base_classes,
                );
                node.state.insert(NodeState::SURFACE_ENTERING);
            }
        }
        if trigger_kind == "rebuild" {
            self.node_service_field_deps = NodeServiceFieldDependencies::build(tree);
        }

        let lifecycle_nodes = self.lifecycle_style_nodes(tree);
        let scoped_interaction_frame = scoped_interaction_frame
            || (!lifecycle_nodes.is_empty()
                && trigger_kind == "restyle"
                && !dirty_types
                    .intersects(ComponentDirtyFlags::SCRIPT | ComponentDirtyFlags::TEXT)
                && self.last_frame_snapshot.is_some());
        #[cfg(test)]
        let scoped_interaction_frame =
            scoped_interaction_frame && !self.force_full_interaction_frames;
        // Populate the rule cache once, then run restyle while borrowing it.
        // The cache survives across paints — we only pay the clone cost on
        // source reload (when `cached_restyle_rules` is reset).
        self.advance_interaction_frame(
            mesh_core_interaction::InteractionFramePhase::StyleInvalidated,
        );
        self.module_restyle_rules();
        let cache_finalization = trigger_kind == "rebuild"
            && !self.surface_entering
            && !self.surface_exiting
            && self.entering_child_keys.is_empty()
            && self.closing_child_keys.is_empty();
        let resolver = StyleResolver::new(theme)
            .with_borrowed_props(surface_css_props)
            .with_finalized_memo_styles(cache_finalization)
            .with_incremental_matching();
        let context = self.root_style_container(width, height);
        let restyle_started = std::time::Instant::now();
        let collect_style_attribution = self.profiling_enabled && self.style_rule_attribution;
        let restyle_rules = self
            .cached_restyle_rules
            .as_deref()
            .expect("cache populated above");
        let mut style_attribution = collect_style_attribution
            .then(|| mesh_core_elements::style::StyleRuleAttribution::new(restyle_rules));
        // Animation-only frames have already resolved their authored styles and
        // only need the animated values applied below. Reuse the targeted path
        // so an otherwise empty interaction diff avoids a full style walk.
        let targeted_interaction_restyle = trigger_kind == "restyle"
            && (dirty_types.contains(ComponentDirtyFlags::STATE)
                || animation_only_frame
                || !lifecycle_nodes.is_empty()
                || !self.patched_binding_nodes.is_empty())
            && !dirty_types.intersects(ComponentDirtyFlags::SCRIPT | ComponentDirtyFlags::TEXT);
        #[cfg(test)]
        let targeted_interaction_restyle =
            targeted_interaction_restyle && !self.force_full_interaction_frames;
        let paint_only_restyle = trigger_kind == "restyle"
            && !dirty_types.is_empty()
            && dirty_types
                .difference(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS)
                .is_empty();
        // Collect affected ids before borrowing index_cache, to satisfy the
        // borrow checker.
        let mut affected_keys =
            if targeted_interaction_restyle && dirty_types.contains(ComponentDirtyFlags::STATE) {
                self.collect_interaction_changed_node_ids()
            } else {
                InteractionChangedNodeIds::default()
            };
        affected_keys.affected.extend(lifecycle_nodes);
        let patched_nodes = std::mem::take(&mut self.patched_binding_nodes);
        let patched_frame = !patched_nodes.is_empty();
        affected_keys.affected.extend(&patched_nodes);
        let patched_layout_class = std::mem::take(&mut self.patched_binding_layout);
        let mut scoped_layout_changed = false;
        let interaction_snapshot_valid = self.interaction_snapshot_valid;
        let index_cache = &mut self.cached_style_rule_index;
        let mut reused_retained_layout = false;
        let mut empty_restyle_avoided = false;
        let mut scoped_restyled = None;
        let preserve_surface_root = tree.tag == "surface";
        if paint_only_restyle {
            // Scroll, tooltip, and other paint-only invalidations do not alter
            // authored attributes or computed styles. Re-resolving the whole
            // tree here used to make every scroll tick pay the Appearance
            // page's full style walk before the retained painter could move
            // its already-laid-out content.
            reused_retained_layout = true;
        } else if targeted_interaction_restyle {
            if affected_keys.affected.is_empty() || !interaction_snapshot_valid {
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
            } else if let Some(restyled) = scoped_interaction_frame
                .then_some(())
                .and(self.last_frame_snapshot.as_ref())
                .filter(|_| style_attribution.is_none())
                .and_then(|snapshot| {
                    let mut restyled = HashSet::new();
                    resolver
                        .restyle_targets_by_path(
                            tree,
                            restyle_rules,
                            context,
                            index_cache,
                            snapshot,
                            &affected_keys.affected,
                            preserve_surface_root,
                            &mut restyled,
                            &mut scoped_layout_changed,
                        )
                        .then(|| {
                            apply_runtime_node_state_by_path(tree, snapshot, &restyled);
                            restyled
                        })
                })
            {
                scoped_restyled = Some(restyled);
            } else {
                // Narrow restyle: only restyle state-changed nodes and their
                // descendants. Siblings, cousins, and unrelated subtrees are
                // left untouched.
                if preserve_surface_root {
                    resolver.restyle_subtree_children_for_ids_cached(
                        tree,
                        restyle_rules,
                        context,
                        index_cache,
                        &affected_keys.affected,
                        style_attribution.as_mut(),
                    );
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
        // A patch invalidates layout conservatively; when the bound property
        // is not layout-class and no re-resolved style changed a layout input,
        // geometry is current and the sparse retained update applies.
        if patched_frame
            && scoped_restyled.is_some()
            && !scoped_layout_changed
            && !patched_layout_class
            && dirty_types.contains(ComponentDirtyFlags::LAYOUT)
        {
            dirty_types.remove(ComponentDirtyFlags::LAYOUT);
            reused_retained_layout = true;
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
        if tree.tag == "surface" {
            // `auto` on an axis the shell has no size for yet: a definite box
            // built from the stand-in size shrinks the content to fit it (a
            // 56px bar in a 1px surface lays out 1px tall), and that collapsed
            // layout is exactly what `measure_content_size` then reports as
            // the surface's content size.
            tree.computed_style.width = if self.unmeasured_root_axes.0 {
                mesh_core_elements::Dimension::Auto
            } else {
                mesh_core_elements::Dimension::Px(width as f32)
            };
            tree.computed_style.height = if self.unmeasured_root_axes.1 {
                mesh_core_elements::Dimension::Auto
            } else {
                mesh_core_elements::Dimension::Px(height as f32)
            };
        }

        // Hidden attributes and shell-owned promotion/error constraints are
        // applied together by the post-restyle state pass above. Retained
        // subtrees that were not restyled already carry those constraints.

        if trigger_kind == "restyle"
            && scoped_restyled.is_some()
            && !dirty_types.intersects(ComponentDirtyFlags::SCRIPT | ComponentDirtyFlags::TEXT)
        {
            scoped_finalize = true;
        }
        // Sample animations before layout: laying out first shows every
        // layout-affecting sample one frame late, and a transition that
        // starts this frame would flash its end state.
        if let Some(pass) = self.pending_animation_pass.take() {
            let restyled = if paint_only_restyle || empty_restyle_avoided {
                Some(HashSet::new())
            } else {
                scoped_restyled.clone()
            };
            let sampled = self.run_style_animation_pass(
                tree,
                trigger_kind == "rebuild",
                restyled.as_ref(),
                pass,
                surface_css_props,
            );
            let geometry_sampled = self.frame_dirty.iter().any(|(id, change)| {
                change.contains(mesh_core_elements::ChangeClass::LAYOUT) && sampled.contains(&id)
            });
            if geometry_sampled {
                reused_retained_layout = false;
                dirty_types |= ComponentDirtyFlags::LAYOUT;
            }
            self.animation_pass_dirty_roots = Some(sampled);
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
        // Path-scoped restyle and active animation registrations name the
        // changed layout inputs. The root joins them because surface sizing
        // above rewrites its style.
        let stable_layout_nodes = scoped_restyled
            .clone()
            .or_else(|| (paint_only_restyle || empty_restyle_avoided).then(HashSet::new))
            .filter(|_| dirty_layout && !dirty_structural && scoped_finalize)
            .map(|mut nodes| {
                nodes.extend(&patched_nodes);
                nodes.extend(&annotation_dirty);
                nodes.extend(self.animation_sample_nodes());
                nodes.insert(tree.id);
                nodes
            });
        if let (Some(nodes), Some(snapshot)) = (&stable_layout_nodes, &self.last_frame_snapshot) {
            LayoutEngine::compute_incremental_for_stable_tree(
                tree,
                &mut self.layout_state,
                snapshot,
                nodes,
                width as f32,
                height as f32,
                &mut self.intrinsic_layout_cache,
                Some(&measurer),
            );
        } else {
            LayoutEngine::compute_incremental_with_dirty_node_snapshots(
                tree,
                &mut self.layout_state,
                width as f32,
                height as f32,
                dirty_layout,
                dirty_structural,
                None,
                &mut self.intrinsic_layout_cache,
                Some(&measurer),
            );
        }
        self.advance_interaction_frame(mesh_core_interaction::InteractionFramePhase::LayoutReady);
        if layout_work_required {
            self.record_profiling_stage(
                mesh_core_debug::ProfilingStage::Layout,
                layout_started,
                Some(trigger_kind),
            );
        }
        if !overflow_was_scoped {
            self.overflow_bounds.invalidate();
        }
        if layout_work_required && self.layout_state.changed_nodes.is_none() {
            scoped_finalize = false;
        }
        if let Some(changed) = &self.layout_state.changed_nodes {
            annotation_dirty.extend(changed.iter().copied());
        }
        let mut overflow_current = self.overflow_bounds.is_current(tree.id);
        if overflow_current && layout_work_required {
            overflow_current = false;
            if let (Some(changed), Some(snapshot)) =
                (&self.layout_state.changed_nodes, &self.last_frame_snapshot)
            {
                let mut dirty = changed.clone();
                if let Some(inputs) = &stable_layout_nodes {
                    dirty.extend(inputs);
                }
                if let Some(restyled) = &scoped_restyled {
                    dirty.extend(restyled);
                }
                let visit = || {
                    mesh_core_elements::frame_work::visit(
                        mesh_core_elements::frame_work::Pass::Annotation,
                    )
                };
                if dirty.len().saturating_mul(2) <= self.layout_state.node_map.len()
                    && let Some(paths) = snapshot.live_paths(tree, &dirty, visit)
                    && paths.len() == dirty.len()
                    && let Some(remeasured) =
                        self.overflow_bounds
                            .refresh(tree, &paths, &mut self.scroll_offsets)
                {
                    annotation_dirty.extend(remeasured);
                    overflow_current = true;
                }
            }
        }
        if !overflow_current {
            annotation_dirty.extend(
                self.overflow_bounds
                    .annotate_tree(tree, &mut self.scroll_offsets),
            );
            overflow_current = true;
        }
        self.scoped_annotation_inputs = annotation_inputs.filter(|_| overflow_current);
        // Normalize the live projection after style/layout. Paint refreshes
        // any subsequently animated nodes before the retained diff and final
        // immutable capture, so all consumers see the sampled frame.
        if scoped_finalize {
            if let Some(previous) = &self.last_frame_snapshot {
                // A scoped restyle reports inherited descendants itself.
                let scoped_nodes = scoped_restyled
                    .as_ref()
                    .map(|restyled| annotation_dirty.union(restyled).copied().collect());
                let no_subtrees = HashSet::new();
                let (dirty_nodes, dirty_subtrees) = match &scoped_nodes {
                    Some(nodes) => (nodes, &no_subtrees),
                    None => (&annotation_dirty, &affected_keys.affected),
                };
                match previous.normalize_accessibility_dirty(tree, dirty_nodes, dirty_subtrees) {
                    Some(normalized) => annotation_dirty.extend(normalized),
                    None => scoped_finalize = false,
                }
            } else {
                if trigger_kind == "restyle" {
                    mesh_core_elements::normalize_accessibility_with_validated_structure(tree);
                } else {
                    mesh_core_elements::normalize_accessibility(tree);
                }
                scoped_finalize = false;
            }
        } else if trigger_kind == "rebuild" {
            mesh_core_elements::normalize_accessibility_finalized_memo(tree);
        } else {
            if trigger_kind == "restyle" {
                mesh_core_elements::normalize_accessibility_with_validated_structure(tree);
            } else {
                mesh_core_elements::normalize_accessibility(tree);
            }
        }
        if cache_finalization {
            self.finalize_component_memo(tree);
        }
        annotation_dirty.extend(self.annotate_selection_tree(tree, theme, !scoped_finalize));

        // Store current interaction state for next frame's targeted restyle diff.
        // Preserve the prior frame's allocation while replacing its contents.
        // Interaction paths typically have the same depth across pointer moves.
        self.previous_hovered_path.clone_from(&self.hovered_path);
        self.previous_focused_key = self
            .focused_id
            .or_else(|| self.focused_key.as_deref().map(runtime_node_id_for_key));
        self.previous_focus_visible_key = self.focus_visible_id.or_else(|| {
            self.focus_visible_key
                .as_deref()
                .map(runtime_node_id_for_key)
        });
        self.previous_active_key = self.pointer_down_id;
        if self.cached_restyle_state_dependencies.contains_any([
            mesh_core_elements::PseudoState::Checked,
            mesh_core_elements::PseudoState::Selected,
        ]) {
            self.previous_checked_values
                .clone_from(&self.checked_values);
        } else {
            self.previous_checked_values.clear();
        }
        self.previous_expanded_triggers
            .clone_from(&self.expanded_triggers);
        // Not gated on a selector dependency the way `checked` is: a slider
        // value is painted content, so it matters whether or not any rule
        // selects on it.
        self.previous_slider_values.clone_from(&self.slider_values);
        self.interaction_snapshot_valid = true;
        for &id in &annotation_dirty {
            self.frame_dirty.mark(
                id,
                mesh_core_elements::ChangeClass::A11Y
                    | mesh_core_elements::ChangeClass::HIT
                    | mesh_core_elements::ChangeClass::PAINT,
            );
        }
        for &id in &affected_keys.affected {
            self.frame_dirty
                .mark(id, mesh_core_elements::ChangeClass::STYLE);
        }
        self.frame_restyled_nodes = if paint_only_restyle || empty_restyle_avoided {
            Some(HashSet::new())
        } else {
            scoped_restyled.clone()
        };
        self.retained_update_dirty_nodes = annotation_dirty;
        if let Some(restyled) = scoped_restyled.filter(|_| {
            scoped_finalize && (!layout_work_required || self.layout_state.changed_nodes.is_some())
        }) {
            // Restyle and layout report their complete changed sets, including
            // inherited values and siblings moved by geometry propagation.
            for &id in &restyled {
                self.frame_dirty.mark(
                    id,
                    mesh_core_elements::ChangeClass::STYLE | mesh_core_elements::ChangeClass::PAINT,
                );
            }
            self.retained_update_dirty_nodes.extend(restyled);
            self.retained_update_dirty_roots = Some(HashSet::new());
        } else if scoped_finalize
            && ((targeted_interaction_restyle && interaction_snapshot_valid)
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
        if trigger_kind == "restyle"
            && (self.retained_update_dirty_roots.is_none()
                || (layout_work_required && self.layout_state.changed_nodes.is_none()))
        {
            // A broad restyle owns the whole subtree; there is no need to
            // discover which descendants changed before synchronizing it.
            self.retained_update_dirty_roots = Some(HashSet::from([tree.id]));
        }
    }

    /// Publish only after animation and the authoritative retained diff finish.
    pub(super) fn capture_finalized_frame(&mut self, tree: &WidgetNode) {
        self.frame_revision = self.frame_revision.saturating_add(1);
        let dirty = self.retained_tree.last_dirty();
        let phases = FramePhaseStamps::complete(self.frame_revision);
        let frame = match self.last_frame_snapshot.as_ref() {
            Some(previous) if dirty.inserted == 0 && dirty.removed == 0 && dirty.children == 0 => {
                FrameSnapshot::capture_dirty(
                    tree,
                    self.frame_revision,
                    phases,
                    previous,
                    self.retained_tree.dirty_node_ids(),
                )
            }
            previous => FrameSnapshot::capture(tree, self.frame_revision, phases, previous),
        };
        match frame {
            Ok(frame) => {
                self.publish_interaction_tree_snapshot(frame.clone());
                self.last_frame_snapshot = Some(frame);
            }
            Err(error) => {
                self.last_frame_snapshot = None;
                tracing::error!(
                    component = %self.id(),
                    "frame snapshot construction failed: {error}"
                );
            }
        }
    }

    /// Collects stable IDs for nodes whose interaction state changed this frame.
    ///
    /// Compares every interaction-driven pseudo-state against the previous
    /// frame and returns only nodes whose changed state is referenced by this
    /// surface's selectors.
    fn resolved_annotation_focus(&self) -> (Option<NodeId>, Option<NodeId>) {
        (
            self.focused_id
                .or_else(|| self.focused_key.as_deref().map(runtime_node_id_for_key)),
            self.focus_visible_id.or_else(|| {
                self.focus_visible_key
                    .as_deref()
                    .map(runtime_node_id_for_key)
            }),
        )
    }

    fn shortcuts_generation(&self) -> Option<u64> {
        self.resolved_surface_shortcuts_cache
            .borrow()
            .as_ref()
            .map(|cache| cache.generation)
    }

    fn capture_annotation_inputs(&self) -> RuntimeAnnotationInputs {
        let (focused_id, focus_visible_id) = self.resolved_annotation_focus();
        RuntimeAnnotationInputs {
            focused_id,
            focus_visible_id,
            hovered_path: self.hovered_path.clone(),
            active_id: self.pointer_down_id,
            active_slider_id: self.active_slider_id,
            window: self.window_states,
            promoted_windows: self.promoted_window_keys.clone(),
            input_values: self.input_values.clone(),
            input_preedits: self.input_preedits.clone(),
            checked_values: self.checked_values.clone(),
            expanded_triggers: self.expanded_triggers.clone(),
            slider_values: self.slider_values.clone(),
            slider_script_values: self.slider_script_values.clone(),
            shortcuts_generation: self.shortcuts_generation(),
        }
    }

    /// Nodes whose annotation inputs differ from `previous`, or `None` when an
    /// ambient input changed and every node needs projecting again.
    fn annotation_input_changes(
        &self,
        previous: &RuntimeAnnotationInputs,
    ) -> Option<HashSet<NodeId>> {
        if previous.window != self.window_states
            || previous.promoted_windows != self.promoted_window_keys
            || previous.shortcuts_generation != self.shortcuts_generation()
        {
            return None;
        }
        let (focused_id, focus_visible_id) = self.resolved_annotation_focus();
        let mut changed = HashSet::new();
        collect_hover_changed_ids(&previous.hovered_path, &self.hovered_path, &mut changed);
        // Focus-visible falls back to logical focus on inputs.
        if previous.focused_id != focused_id || previous.focus_visible_id != focus_visible_id {
            changed.extend(
                [
                    previous.focused_id,
                    focused_id,
                    previous.focus_visible_id,
                    focus_visible_id,
                ]
                .into_iter()
                .flatten(),
            );
        }
        for (before, now) in [
            (previous.active_id, self.pointer_down_id),
            (previous.active_slider_id, self.active_slider_id),
        ] {
            if before != now {
                changed.extend(before.into_iter().chain(now));
            }
        }
        changed_map_keys(&previous.input_values, &self.input_values, &mut changed);
        changed_map_keys(&previous.input_preedits, &self.input_preedits, &mut changed);
        changed_map_keys(&previous.checked_values, &self.checked_values, &mut changed);
        changed_map_keys(&previous.slider_values, &self.slider_values, &mut changed);
        changed_map_keys(
            &previous.slider_script_values,
            &self.slider_script_values,
            &mut changed,
        );
        changed.extend(
            previous
                .expanded_triggers
                .symmetric_difference(&self.expanded_triggers)
                .copied(),
        );
        changed.extend(self.patched_binding_nodes.iter().copied());
        Some(changed)
    }

    pub(super) fn collect_interaction_changed_node_ids(&self) -> InteractionChangedNodeIds {
        let dependencies = self.cached_restyle_state_dependencies;
        let mut changed_ids =
            HashSet::with_capacity(self.previous_hovered_path.len() + self.hovered_path.len() + 6);

        // Hover paths are ordered root-to-leaf chains. Only the tails after
        // their common prefix changed hover state; walking those tails avoids
        // the quadratic pair of `Vec::contains` scans on deep trees.
        if dependencies.contains(mesh_core_elements::PseudoState::Hovered) {
            collect_hover_changed_ids(
                &self.previous_hovered_path,
                &self.hovered_path,
                &mut changed_ids,
            );
        }

        let focused_id = self
            .focused_id
            .or_else(|| self.focused_key.as_deref().map(runtime_node_id_for_key));
        let focus_visible_id = self.focus_visible_id.or_else(|| {
            self.focus_visible_key
                .as_deref()
                .map(runtime_node_id_for_key)
        });

        // Focus keys remain at input and handler boundaries; restyle diffs use
        // the stable identities computed once here.
        if dependencies.contains(mesh_core_elements::PseudoState::Focused)
            && self.previous_focused_key != focused_id
        {
            changed_ids.extend(self.previous_focused_key.into_iter().chain(focused_id));
        }

        if dependencies.contains(mesh_core_elements::PseudoState::FocusVisible)
            && (self.previous_focus_visible_key != focus_visible_id
                || self.previous_focused_key != focused_id)
        {
            changed_ids.extend(
                self.previous_focus_visible_key
                    .into_iter()
                    .chain(focus_visible_id),
            );
            // Inputs implicitly expose focus-visible while focused even when
            // no explicit focus-visible key is stored, so logical focus is
            // part of the dependency for this pseudo-state too.
            changed_ids.extend(self.previous_focused_key.into_iter().chain(focused_id));
        }

        if dependencies.contains(mesh_core_elements::PseudoState::Active)
            && self.previous_active_key != self.pointer_down_id
        {
            changed_ids.extend(
                self.previous_active_key
                    .into_iter()
                    .chain(self.pointer_down_id),
            );
        }

        if dependencies.contains_any([
            mesh_core_elements::PseudoState::Checked,
            mesh_core_elements::PseudoState::Selected,
        ]) {
            changed_map_keys(
                &self.previous_checked_values,
                &self.checked_values,
                &mut changed_ids,
            );
        }

        if dependencies.contains(mesh_core_elements::PseudoState::Expanded) {
            changed_ids.extend(
                self.previous_expanded_triggers
                    .symmetric_difference(&self.expanded_triggers)
                    .copied(),
            );
        }

        // A moved slider knob repaints even when no pseudo-state changed, so
        // this diff is unconditional — see `previous_slider_values`.
        changed_map_keys(
            &self.previous_slider_values,
            &self.slider_values,
            &mut changed_ids,
        );

        // Nodes whose bound properties a script write patched in place.
        changed_ids.extend(self.patched_binding_nodes.iter().copied());

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
        // Container queries can change outside the current hover/focus targets.
        self.interaction_snapshot_valid = false;
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

    /// Adopt the compositor's toplevel states for this surface.
    ///
    /// Unlike interaction pseudo-states, this changes ambient state on every
    /// node at once, so there is no targeted-restyle path worth taking: a
    /// fullscreen or tiling transition happens at human frequency, and it
    /// changes what the whole tree measures against. Invalidate the same way a
    /// resize does and let the next paint rebuild.
    pub(super) fn observe_window_states(&mut self, states: WindowSurfaceState) -> bool {
        if self.window_states == states {
            return false;
        }
        self.window_states = states;
        // Surface pseudo-states apply to every node, including when a click
        // has already queued a targeted interaction restyle for this frame.
        self.interaction_snapshot_valid = false;
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

    pub(super) fn annotate_selection_tree(
        &mut self,
        tree: &mut WidgetNode,
        theme: &Theme,
        force: bool,
    ) -> HashSet<NodeId> {
        let mut dirty = HashSet::new();
        if let Some((previous, _, _, _)) = &self.selection_annotation {
            if self
                .selection
                .as_ref()
                .is_none_or(|selection| selection.anchor.node_key != previous.anchor.node_key)
            {
                if let Some(node) = find_node_by_key_mut(tree, &previous.anchor.node_key) {
                    node.text_selection = None;
                    dirty.insert(node.id);
                }
            }
        }
        let Some(selection) = &self.selection else {
            self.selection_annotation = None;
            return dirty;
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
            let projection = (
                selection.clone(),
                [
                    node.layout.x.to_bits(),
                    node.layout.y.to_bits(),
                    node.computed_style.padding.left.to_bits(),
                    node.computed_style.padding.top.to_bits(),
                ],
                selection_background.clone(),
                selection_foreground.clone(),
            );
            if !force && self.selection_annotation.as_ref() == Some(&projection) {
                return dirty;
            }
            self.selection_annotation = Some(projection);
            dirty.insert(node.id);
            annotate_selected_text_node(
                node,
                selection,
                &selection_background,
                &selection_foreground,
            );
        }
        dirty
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

    pub(super) fn record_runtime_style_diagnostics_after_retained_update(
        &mut self,
        tree: &mut WidgetNode,
        theme: &Theme,
        retained_tree_generation: u64,
        width: u32,
        height: u32,
        surface_css_props: &SurfaceCssProps,
    ) {
        if self.diagnostics.is_none() {
            self.runtime_style_diagnostic_fingerprint = None;
            return;
        }
        let context = self.root_style_container(width, height);
        let fingerprint = RuntimeStyleDiagnosticFingerprint {
            rules_generation: self.style_rules_generation,
            retained_tree_generation,
            props: runtime_style_diagnostic_props_fingerprint(surface_css_props),
            container_width: context.container_width.to_bits(),
            container_height: context.container_height.to_bits(),
        };
        let previous = self.runtime_style_diagnostic_fingerprint;
        if !runtime_style_diagnostic_inputs_changed(
            &mut self.runtime_style_diagnostic_fingerprint,
            fingerprint,
        ) {
            return;
        }
        // Diagnostics are a per-node function of the node's own style inputs
        // under fixed rules, props, and container size. When this retained
        // update kept the structure, the exact restyled set covers new inputs
        // even when animation-only generations separated binding patches. Without
        // that set, retained dirty flags are valid only for consecutive updates.
        let dirty = self.retained_tree.last_dirty();
        let restyled_paths = self.frame_restyled_nodes.as_ref().and_then(|ids| {
            self.last_frame_snapshot
                .as_ref()?
                .live_paths(tree, ids, || {})
        });
        let scoped = previous.is_some_and(|previous| {
            RuntimeStyleDiagnosticFingerprint {
                retained_tree_generation: fingerprint.retained_tree_generation,
                ..previous
            } == fingerprint
                && (restyled_paths.is_some()
                    || previous.retained_tree_generation.checked_add(1)
                        == Some(fingerprint.retained_tree_generation))
        }) && dirty.inserted == 0
            && dirty.removed == 0
            && dirty.children == 0;
        if scoped && dirty.style == 0 && dirty.attributes == 0 && dirty.state == 0 {
            return;
        }

        let resolver = StyleResolver::new(theme).with_borrowed_props(surface_css_props);
        let restyle_rules = self
            .cached_restyle_rules
            .as_deref()
            .expect("rule cache populated during tree finalization");
        let style_index = self
            .cached_style_rule_index
            .as_ref()
            .expect("style index cache populated during tree finalization");
        if scoped {
            if let Some(paths) = restyled_paths {
                mesh_core_elements::for_each_node_at_paths_mut(tree, &paths, |_, node| {
                    self.record_runtime_style_diagnostics_for_single_node(
                        node,
                        restyle_rules,
                        style_index,
                        &resolver,
                        context,
                    );
                });
                return;
            }
            self.record_runtime_style_diagnostics_for_dirty_nodes(
                tree,
                restyle_rules,
                style_index,
                &resolver,
                context,
            );
            return;
        }
        self.record_runtime_style_diagnostics(tree, restyle_rules, style_index, &resolver, context);
    }

    fn record_runtime_style_diagnostics_for_dirty_nodes(
        &self,
        node: &mut WidgetNode,
        rules: &[mesh_core_component::style::StyleRule],
        index: &mesh_core_elements::StyleRuleIndex,
        resolver: &StyleResolver,
        context: StyleContext,
    ) {
        if self.retained_tree.dirty_flags_for(node.id).intersects(
            super::runtime_tree::RetainedNodeDirtyFlags::STYLE
                | super::runtime_tree::RetainedNodeDirtyFlags::ATTRIBUTES
                | super::runtime_tree::RetainedNodeDirtyFlags::STATE,
        ) {
            self.record_runtime_style_diagnostics_for_single_node(
                node, rules, index, resolver, context,
            );
        }
        for child in &mut node.children {
            self.record_runtime_style_diagnostics_for_dirty_nodes(
                child, rules, index, resolver, context,
            );
        }
    }

    fn record_runtime_style_diagnostics_for_node(
        &self,
        node: &mut WidgetNode,
        rules: &[mesh_core_component::style::StyleRule],
        index: &mesh_core_elements::StyleRuleIndex,
        resolver: &StyleResolver,
        context: StyleContext,
    ) {
        self.record_runtime_style_diagnostics_for_single_node(
            node, rules, index, resolver, context,
        );
        for child in &mut node.children {
            self.record_runtime_style_diagnostics_for_node(child, rules, index, resolver, context);
        }
    }

    fn record_runtime_style_diagnostics_for_single_node(
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
    }
}

fn layer_surface_request_size(
    edge: mesh_core_wayland::Edge,
    exclusive_zone: i32,
    fills_output: bool,
    spans_width: bool,
    measured_size: (u32, u32),
) -> (u32, u32) {
    match edge {
        mesh_core_wayland::Edge::Top | mesh_core_wayland::Edge::Bottom if fills_output => (0, 0),
        mesh_core_wayland::Edge::Top | mesh_core_wayland::Edge::Bottom if spans_width => {
            (0, measured_size.1)
        }
        mesh_core_wayland::Edge::Top | mesh_core_wayland::Edge::Bottom => measured_size,
        // Only a docked side rail is anchored to both vertical edges. A
        // floating left/right surface needs its measured height; sending zero
        // would turn it into a full-height input surface.
        mesh_core_wayland::Edge::Left | mesh_core_wayland::Edge::Right if exclusive_zone > 0 => {
            (measured_size.0, 0)
        }
        mesh_core_wayland::Edge::Left | mesh_core_wayland::Edge::Right => measured_size,
    }
}
