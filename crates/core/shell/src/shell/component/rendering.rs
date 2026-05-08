use super::*;

impl FrontendSurfaceComponent {
    fn record_profiling_stage(
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

    fn module_restyle_rules(&self) -> Vec<mesh_core_component::style::StyleRule> {
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

        rules
    }

    pub(super) fn requested_layout_size(&self) -> (u32, u32) {
        let (width, height) = match self.surface_layout.size_policy {
            SurfaceSizePolicy::Fixed => (self.surface_layout.width, self.surface_layout.height),
            SurfaceSizePolicy::ContentMeasured => self
                .measured_size
                .unwrap_or((self.surface_layout.width, self.surface_layout.height)),
        };
        (width, height)
    }

    pub(super) fn tooltip_overlay_extra_for_content(width: u32) -> (u32, u32) {
        if width < TOOLTIP_OVERLAY_WIDTH {
            (
                TOOLTIP_OVERLAY_WIDTH.saturating_sub(width),
                TOOLTIP_OVERLAY_HEIGHT,
            )
        } else {
            (0, 0)
        }
    }

    pub(super) fn render_layout(&self, surface: &mut dyn ShellSurface) {
        surface.anchor(self.surface_layout.edge);
        surface.set_layer(self.surface_layout.layer);
        let (width, height) = self.requested_layout_size();
        let (tooltip_extra_width, tooltip_extra_height) = if width == 0 {
            (0, 0)
        } else {
            Self::tooltip_overlay_extra_for_content(width)
        };
        surface.set_size(
            width.saturating_add(tooltip_extra_width),
            height.saturating_add(tooltip_extra_height),
        );
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

    pub(super) fn build_tree(&mut self, theme: &Theme, width: u32, height: u32) -> WidgetNode {
        if self.render_hooks_pending {
            self.call_render_hooks();
            self.render_hooks_pending = false;
        }
        self.active_theme.replace(theme.clone());
        let root_state = self.runtime_state(self.id()).unwrap_or_default();
        let bound = LocaleBoundState::new(&root_state, &self.locale);
        {
            let mut stack = self.render_stack.borrow_mut();
            stack.clear();
            stack.push(self.id().to_string());
        }
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
        annotate_runtime_tree(
            &mut tree,
            "root".to_string(),
            &self.focused_key,
            &self.focus_visible_key,
            &self.hovered_path,
            &self.pointer_down_key,
            &self.input_values,
            &self.slider_values,
            &self.checked_values,
            &self.scroll_offsets,
        );
        self.annotate_surface_shortcuts(&mut tree);
        annotate_overflow_tree(&mut tree, "root", &mut self.scroll_offsets);

        let restyle_rules = self.module_restyle_rules();
        let resolver = StyleResolver::new(theme);
        let context = StyleContext {
            container_width: width as f32,
            container_height: height as f32,
        };
        let restyle_started = std::time::Instant::now();
        resolver.restyle_subtree(&mut tree, &restyle_rules, context);
        self.record_profiling_stage(
            mesh_core_debug::ProfilingStage::StyleRestyle,
            restyle_started,
            Some("rebuild"),
        );
        self.record_runtime_style_diagnostics(&tree, &restyle_rules, &resolver, context);

        // Recompute layout after restyle so that pseudo-state and container-query style
        // changes (display:none, width, height, etc.) are reflected in final layout
        // bounds before hit-testing, accessibility publishing, and paint.
        let layout_started = std::time::Instant::now();
        LayoutEngine::compute_with_measurer(
            &mut tree,
            width as f32,
            height as f32,
            Some(&measurer),
        );
        self.record_profiling_stage(
            mesh_core_debug::ProfilingStage::Layout,
            layout_started,
            Some("rebuild"),
        );
        self.annotate_selection_tree(&mut tree, theme);

        tree
    }

    pub(super) fn observe_surface_size(&mut self, width: u32, height: u32) -> bool {
        let size = (width.max(1), height.max(1));
        if self.last_surface_size == Some(size) {
            return false;
        }
        self.last_surface_size = Some(size);
        self.dirty = true;
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
        annotate_selection_node(
            tree,
            selection,
            &selection_background,
            &selection_foreground,
        );
    }

    fn record_runtime_style_diagnostics(
        &self,
        tree: &WidgetNode,
        rules: &[mesh_core_component::style::StyleRule],
        resolver: &StyleResolver,
        context: StyleContext,
    ) {
        self.record_runtime_style_diagnostics_for_node(tree, rules, resolver, context);
    }

    fn record_runtime_style_diagnostics_for_node(
        &self,
        node: &WidgetNode,
        rules: &[mesh_core_component::style::StyleRule],
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
        let (_style, diagnostics) = resolver.resolve_node_style_with_diagnostics_for_module(
            rules, &node.tag, &classes, id, context, node.state, module_id,
        );

        for diagnostic in diagnostics {
            if diagnostic.message.contains("animation.")
                || diagnostic.property.starts_with("animation")
            {
                self.record_runtime_animation_diagnostic(diagnostic.message);
            }
        }

        for child in &node.children {
            self.record_runtime_style_diagnostics_for_node(child, rules, resolver, context);
        }
    }
}

fn annotate_selection_node(
    node: &mut WidgetNode,
    selection: &TextSelectionState,
    selection_background: &str,
    selection_foreground: &str,
) -> bool {
    let matches_selection = node
        .attributes
        .get("_mesh_key")
        .is_some_and(|key| key == &selection.anchor.node_key)
        && node.tag == "text"
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

    for child in &mut node.children {
        if annotate_selection_node(child, selection, selection_background, selection_foreground) {
            return true;
        }
    }

    false
}
