use super::*;

impl FrontendSurfaceComponent {
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
        surface.set_keyboard_interactivity(self.surface_layout.keyboard_mode);
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
        self.render_stack.borrow_mut().clear();
        annotate_runtime_tree(
            &mut tree,
            "root".to_string(),
            &self.focused_key,
            &self.hovered_path,
            &self.pointer_down_key,
            &self.input_values,
            &self.slider_values,
            &self.checked_values,
            &self.scroll_offsets,
        );
        annotate_overflow_tree(&mut tree, "root", &mut self.scroll_offsets);

        let rules = self
            .compiled
            .component
            .style
            .as_ref()
            .map(|s| s.rules.as_slice())
            .unwrap_or(&[]);
        let resolver = StyleResolver::new(theme);
        let context = StyleContext {
            container_width: width as f32,
            container_height: height as f32,
        };
        resolver.restyle_subtree(&mut tree, rules, context);

        // Recompute layout after restyle so that pseudo-state and container-query style
        // changes (display:none, width, height, etc.) are reflected in final layout
        // bounds before hit-testing, accessibility publishing, and paint.
        LayoutEngine::compute_with_measurer(
            &mut tree,
            width as f32,
            height as f32,
            Some(&measurer),
        );

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
}
