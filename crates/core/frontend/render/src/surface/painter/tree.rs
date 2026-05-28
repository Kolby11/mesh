use std::collections::HashSet;

use crate::display_list::{
    DisplayListClip, DisplayPaintCommand, DisplayPaintCommandKind, DisplayPaintContent,
    DisplayPaintNode, SelectedDisplayListPaint,
};
use mesh_core_elements::style::Edges;

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

    pub fn render_tree_at_for_module_clipped(
        &self,
        root: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        offset_x: f32,
        offset_y: f32,
        clip: (u32, u32, u32, u32),
        module_id: Option<&str>,
    ) {
        let surface_clip = ClipRect {
            x: 0,
            y: 0,
            width: buffer.width as i32,
            height: buffer.height as i32,
        };
        let damage_clip = ClipRect {
            x: clip.0 as i32,
            y: clip.1 as i32,
            width: clip.2 as i32,
            height: clip.3 as i32,
        };
        let clip = intersect_clip(surface_clip, damage_clip);
        self.render_node(root, buffer, scale, offset_x, offset_y, clip, module_id);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_tree_at_for_module_clipped_filtered(
        &self,
        root: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        offset_x: f32,
        offset_y: f32,
        clip: (u32, u32, u32, u32),
        paint_nodes: &HashSet<mesh_core_elements::NodeId>,
        module_id: Option<&str>,
    ) {
        let surface_clip = ClipRect {
            x: 0,
            y: 0,
            width: buffer.width as i32,
            height: buffer.height as i32,
        };
        let damage_clip = ClipRect {
            x: clip.0 as i32,
            y: clip.1 as i32,
            width: clip.2 as i32,
            height: clip.3 as i32,
        };
        let clip = intersect_clip(surface_clip, damage_clip);
        self.render_node_with_filter(
            root,
            buffer,
            scale,
            offset_x,
            offset_y,
            clip,
            Some(paint_nodes),
            module_id,
        );
    }

    pub fn render_display_list_for_module(
        &self,
        commands: &[DisplayPaintCommand],
        buffer: &mut PixelBuffer,
        scale: f32,
        clip: Option<(u32, u32, u32, u32)>,
        paint_nodes: Option<&HashSet<mesh_core_elements::NodeId>>,
        module_id: Option<&str>,
    ) {
        let surface_clip = ClipRect {
            x: 0,
            y: 0,
            width: buffer.width as i32,
            height: buffer.height as i32,
        };
        let paint_clip = clip
            .map(|clip| {
                intersect_clip(
                    surface_clip,
                    ClipRect {
                        x: clip.0 as i32,
                        y: clip.1 as i32,
                        width: clip.2 as i32,
                        height: clip.3 as i32,
                    },
                )
            })
            .unwrap_or(surface_clip);

        let mut scratch = self.render_scratch.borrow_mut();
        scratch.prepare(commands.len());
        for command in commands {
            let kind = command.kind;
            if self.try_append_display_self_paint_batch(
                command,
                kind,
                scale,
                paint_clip,
                paint_nodes,
                &mut scratch.batched_commands,
            ) {
                continue;
            }
            if !scratch.batched_commands.is_empty() {
                self.execute_painter_commands(buffer, &scratch.batched_commands);
                scratch.batched_commands.clear();
            }
            self.render_display_command(
                command,
                kind,
                buffer,
                scale,
                paint_clip,
                paint_nodes,
                module_id,
                &mut scratch.node_commands,
            );
        }
        if !scratch.batched_commands.is_empty() {
            self.execute_painter_commands(buffer, &scratch.batched_commands);
        }
    }

    pub fn render_selected_display_list_for_module(
        &self,
        commands: &SelectedDisplayListPaint<'_>,
        buffer: &mut PixelBuffer,
        scale: f32,
        clip: Option<(u32, u32, u32, u32)>,
        paint_nodes: Option<&HashSet<mesh_core_elements::NodeId>>,
        module_id: Option<&str>,
    ) {
        let surface_clip = ClipRect {
            x: 0,
            y: 0,
            width: buffer.width as i32,
            height: buffer.height as i32,
        };
        let paint_clip = clip
            .map(|clip| {
                intersect_clip(
                    surface_clip,
                    ClipRect {
                        x: clip.0 as i32,
                        y: clip.1 as i32,
                        width: clip.2 as i32,
                        height: clip.3 as i32,
                    },
                )
            })
            .unwrap_or(surface_clip);

        let mut scratch = self.render_scratch.borrow_mut();
        scratch.prepare(commands.len());
        for (command, kind) in commands.iter_with_kinds() {
            if self.try_append_display_self_paint_batch(
                command,
                kind,
                scale,
                paint_clip,
                paint_nodes,
                &mut scratch.batched_commands,
            ) {
                continue;
            }
            if !scratch.batched_commands.is_empty() {
                self.execute_painter_commands(buffer, &scratch.batched_commands);
                scratch.batched_commands.clear();
            }
            self.render_display_command(
                command,
                kind,
                buffer,
                scale,
                paint_clip,
                paint_nodes,
                module_id,
                &mut scratch.node_commands,
            );
        }
        if !scratch.batched_commands.is_empty() {
            self.execute_painter_commands(buffer, &scratch.batched_commands);
        }
    }

    fn try_append_display_self_paint_batch(
        &self,
        command: &DisplayPaintCommand,
        kind: DisplayPaintCommandKind,
        scale: f32,
        paint_clip: ClipRect,
        paint_nodes: Option<&HashSet<mesh_core_elements::NodeId>>,
        batched_commands: &mut Vec<PainterCommand>,
    ) -> bool {
        if kind != DisplayPaintCommandKind::Node {
            return false;
        }
        if paint_nodes.is_some_and(|nodes| !nodes.contains(&command.node.id)) {
            return false;
        }
        if !matches!(command.node.content, DisplayPaintContent::None) {
            return false;
        }
        let command_clip = scaled_display_clip(command.clip, scale);
        let clip = intersect_clip(paint_clip, command_clip);
        if clip.width <= 0 || clip.height <= 0 {
            return false;
        }
        let node_bounds = scaled_display_node_bounds(&command.node, scale);
        append_display_node_self_paint_commands(
            &command.node,
            scale,
            node_bounds,
            clip,
            batched_commands,
        )
    }

    fn render_display_command(
        &self,
        command: &DisplayPaintCommand,
        kind: DisplayPaintCommandKind,
        buffer: &mut PixelBuffer,
        scale: f32,
        paint_clip: ClipRect,
        paint_nodes: Option<&HashSet<mesh_core_elements::NodeId>>,
        module_id: Option<&str>,
        node_commands: &mut Vec<PainterCommand>,
    ) {
        if paint_nodes.is_some_and(|nodes| !nodes.contains(&command.node.id)) {
            return;
        }
        let command_clip = scaled_display_clip(command.clip, scale);
        let clip = intersect_clip(paint_clip, command_clip);
        if clip.width <= 0 || clip.height <= 0 {
            return;
        }
        match kind {
            DisplayPaintCommandKind::Node => {
                let node_bounds = scaled_display_node_bounds(&command.node, scale);
                self.render_display_node_self(
                    &command.node,
                    buffer,
                    scale,
                    node_bounds,
                    clip,
                    node_commands,
                    module_id,
                );
            }
            DisplayPaintCommandKind::Scrollbars => {
                let bounds = scaled_display_node_bounds(&command.node, scale);
                self.render_display_scrollbars(&command.node, buffer, scale, bounds, clip);
            }
        }
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
        self.render_node_with_filter(
            node, buffer, scale, offset_x, offset_y, clip, None, module_id,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn render_node_with_filter(
        &self,
        node: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        offset_x: f32,
        offset_y: f32,
        clip: ClipRect,
        paint_nodes: Option<&HashSet<mesh_core_elements::NodeId>>,
        module_id: Option<&str>,
    ) {
        if paint_nodes.is_some_and(|nodes| !nodes.contains(&node.id)) {
            return;
        }

        let style = &node.computed_style;
        if style.display == Display::None {
            return;
        }

        // Apply the visually-supported parts of `transform`. Rotation still
        // propagates through the data model but is not painted yet.
        let transform = style.transform;
        let offset_x = offset_x + transform.translate_x;
        let offset_y = offset_y + transform.translate_y;

        let layout = &node.layout;
        let scale_x = transform.scale_x.max(0.0);
        let scale_y = transform.scale_y.max(0.0);
        let base_w = layout.width * scale;
        let base_h = layout.height * scale;
        let scaled_w = base_w * scale_x;
        let scaled_h = base_h * scale_y;
        let base_x = (layout.x + offset_x) * scale;
        let base_y = (layout.y + offset_y) * scale;
        let x = (base_x - (scaled_w - base_w) * 0.5).round() as i32;
        let y = (base_y - (scaled_h - base_h) * 0.5).round() as i32;
        let w = scaled_w.round().max(0.0) as i32;
        let h = scaled_h.round().max(0.0) as i32;
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

        self.render_node_self(node, buffer, scale, bounds, clip, module_id);

        let scroll_x = node_attr_f32(node, "_mesh_scroll_x");
        let scroll_y = node_attr_f32(node, "_mesh_scroll_y");
        let child_offset_x = offset_x - scroll_x;
        let child_offset_y = offset_y - scroll_y;
        let child_clip = if node_clips_children(node) {
            intersect_clip(clip, bounds)
        } else {
            clip
        };

        let mut needs_sort = false;
        let mut previous_z_index = node
            .children
            .first()
            .map(|child| child.computed_style.z_index);
        for child in node.children.iter().skip(1) {
            if let Some(previous) = previous_z_index
                && previous > child.computed_style.z_index
            {
                needs_sort = true;
                break;
            }
            previous_z_index = Some(child.computed_style.z_index);
        }

        if needs_sort {
            let mut child_order: Vec<usize> = (0..node.children.len()).collect();
            child_order.sort_by_key(|&index| node.children[index].computed_style.z_index);
            for index in child_order {
                self.render_node_with_filter(
                    &node.children[index],
                    buffer,
                    scale,
                    child_offset_x,
                    child_offset_y,
                    child_clip,
                    paint_nodes,
                    module_id,
                );
            }
        } else {
            for child in &node.children {
                self.render_node_with_filter(
                    child,
                    buffer,
                    scale,
                    child_offset_x,
                    child_offset_y,
                    child_clip,
                    paint_nodes,
                    module_id,
                );
            }
        }

        self.render_scrollbars(node, buffer, scale, bounds, clip);
    }

    fn render_node_self(
        &self,
        node: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        bounds: ClipRect,
        clip: ClipRect,
        module_id: Option<&str>,
    ) {
        let style = &node.computed_style;
        if style.display == Display::None {
            return;
        }

        let node_clip = intersect_clip(clip, bounds);
        if node_clip.width <= 0 || node_clip.height <= 0 {
            return;
        }
        let x = bounds.x;
        let y = bounds.y;
        let w = bounds.width;
        let h = bounds.height;

        let background_color = opacity_color(style.background_color, style.opacity);
        let border_color = opacity_color(style.border_color, style.opacity);
        let content_color = opacity_color(style.color, style.opacity);

        self.draw_box_shadow(
            buffer,
            bounds,
            style.border_radius.top_left * scale,
            style.box_shadow,
            clip,
        );
        self.apply_backdrop_filter(
            buffer,
            bounds,
            style.border_radius.top_left * scale,
            style.backdrop_filter,
            node_clip,
        );

        if background_color.a > 0 {
            let radius = style.border_radius.top_left * scale;
            let paint_clip = if style.filter.is_none() {
                node_clip
            } else {
                clip
            };
            if radius > 0.5 {
                self.fill_rounded_rect_clipped_with_filter(
                    buffer,
                    bounds,
                    radius,
                    background_color,
                    paint_clip,
                    style.filter,
                );
            } else {
                self.fill_rect_clipped_with_filter(
                    buffer,
                    bounds,
                    background_color,
                    paint_clip,
                    style.filter,
                );
            }
        }
        self.draw_background_paint(
            buffer,
            &style.background_paint,
            bounds,
            style.border_radius.top_left * scale,
            node_clip,
        );

        self.draw_border_clipped(
            buffer,
            bounds,
            &style.border_width,
            style.border_radius.top_left * scale,
            border_color,
            scale,
            node_clip,
        );

        match node.tag.as_str() {
            "text" => self.render_text_node(node, buffer, scale, x, y, node_clip),
            "input" => self.render_input_node(node, buffer, scale, x, y, node_clip),
            "slider" => self.render_slider_node(node, buffer, scale, x, y, w, h, node_clip),
            "icon" => self.render_icon_node(node, buffer, x, y, w, h, content_color, module_id),
            _ => {}
        }
    }

    fn render_display_node_self(
        &self,
        node: &DisplayPaintNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        bounds: ClipRect,
        clip: ClipRect,
        node_commands: &mut Vec<PainterCommand>,
        module_id: Option<&str>,
    ) {
        let style = &node.style;
        let node_clip = intersect_clip(clip, bounds);
        if node_clip.width <= 0 || node_clip.height <= 0 {
            return;
        }

        let x = bounds.x;
        let y = bounds.y;
        let w = bounds.width;
        let h = bounds.height;

        node_commands.clear();

        push_box_shadow_command(
            node_commands,
            bounds,
            style.border_radius * scale,
            style.box_shadow,
            clip,
        );
        push_backdrop_filter_command(
            node_commands,
            bounds,
            style.border_radius * scale,
            style.backdrop_filter,
            node_clip,
        );

        if style.background_color.a > 0 {
            let radius = style.border_radius * scale;
            let paint_clip = if style.filter.is_none() {
                node_clip
            } else {
                clip
            };
            push_fill_shape_command(
                node_commands,
                bounds,
                radius,
                style.background_color,
                paint_clip,
                style.filter,
            );
        }
        push_background_paint_command(
            node_commands,
            &style.background_paint,
            bounds,
            style.border_radius * scale,
            node_clip,
        );

        push_border_commands(
            node_commands,
            bounds,
            &style.border_width,
            style.border_radius * scale,
            style.border_color,
            scale,
            node_clip,
        );
        self.execute_painter_commands(buffer, node_commands);
        node_commands.clear();

        match &node.content {
            DisplayPaintContent::Text(text) => {
                self.render_display_text_node(node, text, buffer, scale, x, y, node_clip);
            }
            DisplayPaintContent::Input(input) => {
                self.render_display_input_node(node, input, buffer, scale, x, y, node_clip);
            }
            DisplayPaintContent::Slider(slider) => {
                self.render_display_slider_node(node, slider, buffer, scale, x, y, w, h, node_clip);
            }
            DisplayPaintContent::Icon(icon) => {
                self.render_display_icon_node(node, icon, buffer, x, y, w, h, module_id);
            }
            DisplayPaintContent::None => {}
        }
    }
    fn draw_border_clipped(
        &self,
        buffer: &mut PixelBuffer,
        bounds: ClipRect,
        border_widths: &Edges,
        radius: f32,
        color: Color,
        scale: f32,
        clip: ClipRect,
    ) {
        if border_widths.top <= 0.0 || color.a == 0 {
            return;
        }

        let border_width = (border_widths.top * scale).max(1.0) as i32;
        if self.stroke_rounded_rect_clipped(buffer, bounds, radius, border_width, color, clip) {
            return;
        }

        let x = bounds.x;
        let y = bounds.y;
        let w = bounds.width;
        let h = bounds.height;
        self.fill_rect_clipped(
            buffer,
            ClipRect {
                x,
                y,
                width: w,
                height: border_width,
            },
            color,
            clip,
        );
        self.fill_rect_clipped(
            buffer,
            ClipRect {
                x,
                y: y + h.saturating_sub(border_width),
                width: w,
                height: border_width,
            },
            color,
            clip,
        );
        self.fill_rect_clipped(
            buffer,
            ClipRect {
                x,
                y,
                width: border_width,
                height: h,
            },
            color,
            clip,
        );
        self.fill_rect_clipped(
            buffer,
            ClipRect {
                x: x + w.saturating_sub(border_width),
                y,
                width: border_width,
                height: h,
            },
            color,
            clip,
        );
    }
}

fn append_display_node_self_paint_commands(
    node: &DisplayPaintNode,
    scale: f32,
    bounds: ClipRect,
    clip: ClipRect,
    commands: &mut Vec<PainterCommand>,
) -> bool {
    let style = &node.style;
    let node_clip = intersect_clip(clip, bounds);
    if node_clip.width <= 0 || node_clip.height <= 0 {
        return false;
    }

    let start_len = commands.len();
    push_box_shadow_command(
        commands,
        bounds,
        style.border_radius * scale,
        style.box_shadow,
        clip,
    );
    push_backdrop_filter_command(
        commands,
        bounds,
        style.border_radius * scale,
        style.backdrop_filter,
        node_clip,
    );

    if style.background_color.a > 0 {
        let radius = style.border_radius * scale;
        let paint_clip = if style.filter.is_none() {
            node_clip
        } else {
            clip
        };
        push_fill_shape_command(
            commands,
            bounds,
            radius,
            style.background_color,
            paint_clip,
            style.filter,
        );
    }
    push_background_paint_command(
        commands,
        &style.background_paint,
        bounds,
        style.border_radius * scale,
        node_clip,
    );

    push_border_commands(
        commands,
        bounds,
        &style.border_width,
        style.border_radius * scale,
        style.border_color,
        scale,
        node_clip,
    );
    commands.len() > start_len
}

fn push_box_shadow_command(
    commands: &mut Vec<PainterCommand>,
    rect: ClipRect,
    radius: f32,
    shadow: BoxShadow,
    clip: ClipRect,
) {
    if shadow.is_none() || shadow.inset {
        return;
    }
    commands.push(PainterCommand::DrawShadow {
        rect,
        radius,
        shadow,
        clip,
    });
}

fn push_backdrop_filter_command(
    commands: &mut Vec<PainterCommand>,
    rect: ClipRect,
    radius: f32,
    filter: VisualFilter,
    clip: ClipRect,
) {
    if filter.is_none() {
        return;
    }
    commands.push(PainterCommand::ApplyFilter {
        rect,
        radius,
        filter: PainterFilter::Backdrop(filter),
        clip,
    });
}

fn push_fill_shape_command(
    commands: &mut Vec<PainterCommand>,
    rect: ClipRect,
    radius: f32,
    color: Color,
    clip: ClipRect,
    filter: VisualFilter,
) {
    let paint = PainterPaint::fill(color).with_filter(filter);
    if radius > 0.5 {
        commands.push(PainterCommand::DrawRoundedRect {
            rect,
            radius,
            paint,
            clip,
        });
    } else {
        commands.push(PainterCommand::DrawRect { rect, paint, clip });
    }
}

fn push_background_paint_command(
    commands: &mut Vec<PainterCommand>,
    paint: &BackgroundPaint,
    rect: ClipRect,
    radius: f32,
    clip: ClipRect,
) {
    match paint {
        BackgroundPaint::None => {}
        BackgroundPaint::Image(source) => commands.push(PainterCommand::DrawImage {
            image: PainterImage {
                source: PainterImageSource::Path(source.path.clone()),
            },
            rect,
            paint: PainterPaint::fill(Color::WHITE),
            clip,
        }),
        BackgroundPaint::LinearGradient(gradient) => {
            commands.push(PainterCommand::DrawLinearGradient {
                gradient: PainterLinearGradient {
                    from: gradient.from,
                    to: gradient.to,
                },
                rect,
                radius,
                clip,
            });
        }
    }
}

fn push_border_commands(
    commands: &mut Vec<PainterCommand>,
    bounds: ClipRect,
    border_widths: &Edges,
    radius: f32,
    color: Color,
    scale: f32,
    clip: ClipRect,
) {
    if border_widths.top <= 0.0 || color.a == 0 {
        return;
    }
    let border_width = (border_widths.top * scale).max(1.0);
    commands.push(PainterCommand::DrawRoundedRect {
        rect: bounds,
        radius,
        paint: PainterPaint::stroke(color, border_width),
        clip,
    });
}

fn scaled_display_node_bounds(node: &DisplayPaintNode, scale: f32) -> ClipRect {
    ClipRect {
        x: (node.layout.x * scale).round() as i32,
        y: (node.layout.y * scale).round() as i32,
        width: (node.layout.width * scale).round().max(0.0) as i32,
        height: (node.layout.height * scale).round().max(0.0) as i32,
    }
}

fn scaled_display_clip(clip: DisplayListClip, scale: f32) -> ClipRect {
    ClipRect {
        x: (clip.x as f32 * scale).round() as i32,
        y: (clip.y as f32 * scale).round() as i32,
        width: (clip.width as f32 * scale).round().max(0.0) as i32,
        height: (clip.height as f32 * scale).round().max(0.0) as i32,
    }
}
