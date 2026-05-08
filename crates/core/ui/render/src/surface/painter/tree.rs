use super::*;

impl FrontendRenderEngine {
    pub fn render_tree(&self, root: &WidgetNode, buffer: &mut PixelBuffer, scale: f32) {
        self.render_tree_at(root, buffer, scale, 0.0, 0.0);
    }

    pub fn render_tree_at(
        &self,
        root: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        offset_x: f32,
        offset_y: f32,
    ) {
        self.render_tree_at_for_module(root, buffer, scale, offset_x, offset_y, None);
    }

    /// Render variant that knows which module owns the tree, so icon
    /// resolution can consult that module's bindings (preferred pack,
    /// declared mappings, user overrides) before falling back to shell-wide
    /// defaults.
    pub fn render_tree_at_for_module(
        &self,
        root: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        offset_x: f32,
        offset_y: f32,
        module_id: Option<&str>,
    ) {
        let clip = ClipRect {
            x: 0,
            y: 0,
            width: buffer.width as i32,
            height: buffer.height as i32,
        };
        self.render_node(root, buffer, scale, offset_x, offset_y, clip, module_id);
    }

    fn render_node(
        &self,
        node: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        offset_x: f32,
        offset_y: f32,
        clip: ClipRect,
        module_id: Option<&str>,
    ) {
        let style = &node.computed_style;
        if style.display == Display::None {
            return;
        }

        // Apply the additive part of `transform` (translate). Scale and
        // rotation aren't visually applied yet — see
        // `crate::animation::transform::is_paintable`. They still propagate
        // through the data model so authors can wire them up and see the
        // animation graph activate; the painter will start honoring them
        // once the tiny_skia path lands.
        let transform = style.transform;
        let offset_x = offset_x + transform.translate_x;
        let offset_y = offset_y + transform.translate_y;

        let layout = &node.layout;
        let x = ((layout.x + offset_x) * scale).round() as i32;
        let y = ((layout.y + offset_y) * scale).round() as i32;
        let w = (layout.width * scale).round().max(0.0) as i32;
        let h = (layout.height * scale).round().max(0.0) as i32;
        let bounds = ClipRect {
            x,
            y,
            width: w,
            height: h,
        };
        let node_clip = intersect_clip(clip, bounds);

        if node_clip.width <= 0 || node_clip.height <= 0 {
            return;
        }

        if style.background_color.a > 0 {
            let radius = style.border_radius.top_left * scale;
            if radius > 0.5 {
                fill_rounded_rect_clipped(
                    buffer,
                    bounds,
                    radius,
                    style.background_color,
                    node_clip,
                );
            } else {
                fill_rect_clipped(buffer, bounds, style.background_color, node_clip);
            }
        }

        if style.border_width.top > 0.0 && style.border_color.a > 0 {
            let border_width = (style.border_width.top * scale).max(1.0) as i32;
            fill_rect_clipped(
                buffer,
                ClipRect {
                    x,
                    y,
                    width: w,
                    height: border_width,
                },
                style.border_color,
                node_clip,
            );
            fill_rect_clipped(
                buffer,
                ClipRect {
                    x,
                    y: y + h.saturating_sub(border_width),
                    width: w,
                    height: border_width,
                },
                style.border_color,
                node_clip,
            );
            fill_rect_clipped(
                buffer,
                ClipRect {
                    x,
                    y,
                    width: border_width,
                    height: h,
                },
                style.border_color,
                node_clip,
            );
            fill_rect_clipped(
                buffer,
                ClipRect {
                    x: x + w.saturating_sub(border_width),
                    y,
                    width: border_width,
                    height: h,
                },
                style.border_color,
                node_clip,
            );
        }

        match node.tag.as_str() {
            "text" => self.render_text_node(node, buffer, scale, x, y, node_clip),
            "input" => self.render_input_node(node, buffer, scale, x, y, node_clip),
            "slider" => self.render_slider_node(node, buffer, scale, x, y, w, h, node_clip),
            "icon" => self.render_icon_node(node, buffer, x, y, w, h, style.color, module_id),
            _ => {}
        }

        let scroll_x = node_attr_f32(node, "_mesh_scroll_x");
        let scroll_y = node_attr_f32(node, "_mesh_scroll_y");
        let child_offset_x = offset_x - scroll_x;
        let child_offset_y = offset_y - scroll_y;
        let child_clip = if node_clips_children(node) {
            intersect_clip(clip, bounds)
        } else {
            clip
        };

        // The vast majority of parents don't use z-index — every child has the
        // same value (typically 0). Detect that case and iterate in natural
        // order to avoid the Vec allocation + O(n log n) sort per parent per
        // paint frame. Only fall back to sorting when at least one child has a
        // differing z-index.
        let needs_sort = node
            .children
            .windows(2)
            .any(|pair| pair[0].computed_style.z_index != pair[1].computed_style.z_index);

        if needs_sort {
            let mut child_order: Vec<usize> = (0..node.children.len()).collect();
            child_order.sort_by_key(|&index| node.children[index].computed_style.z_index);
            for index in child_order {
                self.render_node(
                    &node.children[index],
                    buffer,
                    scale,
                    child_offset_x,
                    child_offset_y,
                    child_clip,
                    module_id,
                );
            }
        } else {
            for child in &node.children {
                self.render_node(
                    child,
                    buffer,
                    scale,
                    child_offset_x,
                    child_offset_y,
                    child_clip,
                    module_id,
                );
            }
        }

        self.render_scrollbars(node, buffer, scale, bounds, clip);
    }
}
