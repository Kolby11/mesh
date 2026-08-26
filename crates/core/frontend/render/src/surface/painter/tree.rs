use std::collections::HashSet;

use crate::display_list::{
    CheckmarkKind, DisplayCheckmarkPaint, DisplayListClip, DisplayPaintCommand,
    DisplayPaintCommandKind, DisplayPaintContent, DisplayPaintNode, RetainedDisplayList,
    SelectedDisplayListPaint,
};
use mesh_core_elements::AffineTransform;
use mesh_core_elements::style::{Corners, Edges};
use smallvec::SmallVec;

use super::*;
use crate::surface::{PaintCommandAttribution, PaintCommandClass};
use crate::{DeviceRect, FractionalScale};

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
        self.render_tree_display_list(
            root, buffer, scale, offset_x, offset_y, None, None, module_id,
        );
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
        self.render_tree_display_list(
            root,
            buffer,
            scale,
            offset_x,
            offset_y,
            Some(clip),
            None,
            module_id,
        );
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
        self.render_tree_display_list(
            root,
            buffer,
            scale,
            offset_x,
            offset_y,
            Some(clip),
            Some(paint_nodes),
            module_id,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn render_tree_display_list(
        &self,
        root: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
        offset_x: f32,
        offset_y: f32,
        clip: Option<(u32, u32, u32, u32)>,
        paint_nodes: Option<&HashSet<mesh_core_elements::NodeId>>,
        module_id: Option<&str>,
    ) {
        let mut display_list = RetainedDisplayList::default();
        display_list.set_backdrop_blur_policy(self.backdrop_blur_policy());
        display_list.update_at(
            root,
            offset_x,
            offset_y,
            buffer.width(),
            buffer.height(),
            true,
            true,
        );
        self.render_display_list_for_module(
            display_list.paint_commands(),
            buffer,
            scale,
            clip,
            paint_nodes,
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
            width: buffer.width() as i32,
            height: buffer.height() as i32,
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
        let mut session = PixelCanvasSession::new(buffer);
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
                self.execute_painter_commands_in_session(&mut session, &scratch.batched_commands);
                scratch.batched_commands.clear();
            }
            self.render_display_command(
                command,
                kind,
                &mut session,
                scale,
                paint_clip,
                paint_nodes,
                module_id,
                &mut scratch.node_commands,
            );
        }
        if !scratch.batched_commands.is_empty() {
            self.execute_painter_commands_in_session(&mut session, &scratch.batched_commands);
        }
        self.close_painter_layers(&mut session);
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
            width: buffer.width() as i32,
            height: buffer.height() as i32,
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
        let mut session = PixelCanvasSession::new(buffer);
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
                self.execute_painter_commands_in_session(&mut session, &scratch.batched_commands);
                scratch.batched_commands.clear();
            }
            self.render_display_command(
                command,
                kind,
                &mut session,
                scale,
                paint_clip,
                paint_nodes,
                module_id,
                &mut scratch.node_commands,
            );
        }
        if !scratch.batched_commands.is_empty() {
            self.execute_painter_commands_in_session(&mut session, &scratch.batched_commands);
        }
        self.close_painter_layers(&mut session);
    }

    /// Paint one selected command stream through several disjoint damage
    /// clips. The command stream is traversed once and the Skia canvas session
    /// is shared across every region; commands spanning more than one region
    /// are replayed only for the clips they intersect.
    pub fn render_selected_display_list_regions_for_module(
        &self,
        commands: &SelectedDisplayListPaint<'_>,
        buffer: &mut PixelBuffer,
        scale: f32,
        clips: &[(u32, u32, u32, u32)],
        paint_nodes: Option<&HashSet<mesh_core_elements::NodeId>>,
        module_id: Option<&str>,
    ) {
        let surface_clip = ClipRect {
            x: 0,
            y: 0,
            width: buffer.width() as i32,
            height: buffer.height() as i32,
        };
        let paint_clips = clipped_paint_regions(surface_clip, clips);
        if paint_clips.is_empty() {
            return;
        }

        let mut scratch = self.render_scratch.borrow_mut();
        scratch.prepare(
            commands
                .len()
                .saturating_mul(paint_clips.len())
                .min(MAX_RETAINED_BATCH_COMMANDS),
        );
        let mut session = PixelCanvasSession::new(buffer);
        if paint_regions_overlap(&paint_clips) || selection_has_layer_scopes(commands) {
            for &paint_clip in &paint_clips {
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
                        self.execute_painter_commands_in_session(
                            &mut session,
                            &scratch.batched_commands,
                        );
                        scratch.batched_commands.clear();
                    }
                    self.render_display_command(
                        command,
                        kind,
                        &mut session,
                        scale,
                        paint_clip,
                        paint_nodes,
                        module_id,
                        &mut scratch.node_commands,
                    );
                }
                if !scratch.batched_commands.is_empty() {
                    self.execute_painter_commands_in_session(
                        &mut session,
                        &scratch.batched_commands,
                    );
                    scratch.batched_commands.clear();
                }
                self.close_painter_layers(&mut session);
            }
            return;
        }
        for (command, kind) in commands.iter_with_kinds() {
            if paint_nodes.is_some_and(|nodes| !nodes.contains(&command.node.id)) {
                continue;
            }
            let command_clip = scaled_display_clip(command.clip, scale);
            for &paint_clip in &paint_clips {
                let effective_clip = intersect_clip(paint_clip, command_clip);
                if effective_clip.width <= 0 || effective_clip.height <= 0 {
                    continue;
                }
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
                    self.execute_painter_commands_in_session(
                        &mut session,
                        &scratch.batched_commands,
                    );
                    scratch.batched_commands.clear();
                }
                self.render_display_command(
                    command,
                    kind,
                    &mut session,
                    scale,
                    paint_clip,
                    paint_nodes,
                    module_id,
                    &mut scratch.node_commands,
                );
            }
        }
        if !scratch.batched_commands.is_empty() {
            self.execute_painter_commands_in_session(&mut session, &scratch.batched_commands);
        }
        self.close_painter_layers(&mut session);
    }

    /// Paint a selected display list while attributing raster time to a small,
    /// fixed set of command classes. Kept as a separate hot loop so normal
    /// painting pays no per-command clock or profiling branch cost.
    pub fn render_selected_display_list_for_module_with_attribution(
        &self,
        commands: &SelectedDisplayListPaint<'_>,
        buffer: &mut PixelBuffer,
        scale: f32,
        clip: Option<(u32, u32, u32, u32)>,
        paint_nodes: Option<&HashSet<mesh_core_elements::NodeId>>,
        module_id: Option<&str>,
    ) -> PaintCommandAttribution {
        let surface_clip = ClipRect {
            x: 0,
            y: 0,
            width: buffer.width() as i32,
            height: buffer.height() as i32,
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

        let mut attribution = PaintCommandAttribution::default();
        let mut scratch = self.render_scratch.borrow_mut();
        scratch.prepare(commands.len());
        let mut session = PixelCanvasSession::new(buffer);
        let mut batched_command_count = 0_u64;
        for (command, kind) in commands.iter_with_kinds() {
            if self.try_append_display_self_paint_batch(
                command,
                kind,
                scale,
                paint_clip,
                paint_nodes,
                &mut scratch.batched_commands,
            ) {
                batched_command_count = batched_command_count.saturating_add(1);
                continue;
            }
            if !scratch.batched_commands.is_empty() {
                let started = std::time::Instant::now();
                self.execute_painter_commands_in_session(&mut session, &scratch.batched_commands);
                attribution.record(
                    PaintCommandClass::Primitive,
                    batched_command_count,
                    started.elapsed(),
                );
                scratch.batched_commands.clear();
                batched_command_count = 0;
            }
            let class = paint_command_class(command, kind);
            let started = std::time::Instant::now();
            self.render_display_command(
                command,
                kind,
                &mut session,
                scale,
                paint_clip,
                paint_nodes,
                module_id,
                &mut scratch.node_commands,
            );
            attribution.record(class, 1, started.elapsed());
        }
        if !scratch.batched_commands.is_empty() {
            let started = std::time::Instant::now();
            self.execute_painter_commands_in_session(&mut session, &scratch.batched_commands);
            attribution.record(
                PaintCommandClass::Primitive,
                batched_command_count,
                started.elapsed(),
            );
        }
        self.close_painter_layers(&mut session);
        attribution
    }

    /// Multi-region counterpart to
    /// [`Self::render_selected_display_list_for_module_with_attribution`].
    /// Attribution counts actual command/clip replays, matching the sum that
    /// callers previously produced by painting each region independently.
    pub fn render_selected_display_list_regions_for_module_with_attribution(
        &self,
        commands: &SelectedDisplayListPaint<'_>,
        buffer: &mut PixelBuffer,
        scale: f32,
        clips: &[(u32, u32, u32, u32)],
        paint_nodes: Option<&HashSet<mesh_core_elements::NodeId>>,
        module_id: Option<&str>,
    ) -> PaintCommandAttribution {
        let surface_clip = ClipRect {
            x: 0,
            y: 0,
            width: buffer.width() as i32,
            height: buffer.height() as i32,
        };
        let paint_clips = clipped_paint_regions(surface_clip, clips);
        if paint_clips.is_empty() {
            return PaintCommandAttribution::default();
        }

        let mut attribution = PaintCommandAttribution::default();
        let mut scratch = self.render_scratch.borrow_mut();
        scratch.prepare(
            commands
                .len()
                .saturating_mul(paint_clips.len())
                .min(MAX_RETAINED_BATCH_COMMANDS),
        );
        let mut session = PixelCanvasSession::new(buffer);
        let mut batched_command_count = 0_u64;
        if paint_regions_overlap(&paint_clips) || selection_has_layer_scopes(commands) {
            for &paint_clip in &paint_clips {
                for (command, kind) in commands.iter_with_kinds() {
                    if self.try_append_display_self_paint_batch(
                        command,
                        kind,
                        scale,
                        paint_clip,
                        paint_nodes,
                        &mut scratch.batched_commands,
                    ) {
                        batched_command_count = batched_command_count.saturating_add(1);
                        continue;
                    }
                    if !scratch.batched_commands.is_empty() {
                        let started = std::time::Instant::now();
                        self.execute_painter_commands_in_session(
                            &mut session,
                            &scratch.batched_commands,
                        );
                        attribution.record(
                            PaintCommandClass::Primitive,
                            batched_command_count,
                            started.elapsed(),
                        );
                        scratch.batched_commands.clear();
                        batched_command_count = 0;
                    }
                    let class = paint_command_class(command, kind);
                    let started = std::time::Instant::now();
                    self.render_display_command(
                        command,
                        kind,
                        &mut session,
                        scale,
                        paint_clip,
                        paint_nodes,
                        module_id,
                        &mut scratch.node_commands,
                    );
                    attribution.record(class, 1, started.elapsed());
                }
                if !scratch.batched_commands.is_empty() {
                    let started = std::time::Instant::now();
                    self.execute_painter_commands_in_session(
                        &mut session,
                        &scratch.batched_commands,
                    );
                    attribution.record(
                        PaintCommandClass::Primitive,
                        batched_command_count,
                        started.elapsed(),
                    );
                    scratch.batched_commands.clear();
                    batched_command_count = 0;
                }
                self.close_painter_layers(&mut session);
            }
            return attribution;
        }
        for (command, kind) in commands.iter_with_kinds() {
            if paint_nodes.is_some_and(|nodes| !nodes.contains(&command.node.id)) {
                continue;
            }
            let command_clip = scaled_display_clip(command.clip, scale);
            for &paint_clip in &paint_clips {
                let effective_clip = intersect_clip(paint_clip, command_clip);
                if effective_clip.width <= 0 || effective_clip.height <= 0 {
                    continue;
                }
                if self.try_append_display_self_paint_batch(
                    command,
                    kind,
                    scale,
                    paint_clip,
                    paint_nodes,
                    &mut scratch.batched_commands,
                ) {
                    batched_command_count = batched_command_count.saturating_add(1);
                    continue;
                }
                if !scratch.batched_commands.is_empty() {
                    let started = std::time::Instant::now();
                    self.execute_painter_commands_in_session(
                        &mut session,
                        &scratch.batched_commands,
                    );
                    attribution.record(
                        PaintCommandClass::Primitive,
                        batched_command_count,
                        started.elapsed(),
                    );
                    scratch.batched_commands.clear();
                    batched_command_count = 0;
                }
                let class = paint_command_class(command, kind);
                let started = std::time::Instant::now();
                self.render_display_command(
                    command,
                    kind,
                    &mut session,
                    scale,
                    paint_clip,
                    paint_nodes,
                    module_id,
                    &mut scratch.node_commands,
                );
                attribution.record(class, 1, started.elapsed());
            }
        }
        if !scratch.batched_commands.is_empty() {
            let started = std::time::Instant::now();
            self.execute_painter_commands_in_session(&mut session, &scratch.batched_commands);
            attribution.record(
                PaintCommandClass::Primitive,
                batched_command_count,
                started.elapsed(),
            );
        }
        self.close_painter_layers(&mut session);
        attribution
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
        if requires_affine_paint(&command.node) {
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
        session: &mut PixelCanvasSession<'_>,
        scale: f32,
        paint_clip: ClipRect,
        paint_nodes: Option<&HashSet<mesh_core_elements::NodeId>>,
        module_id: Option<&str>,
        node_commands: &mut Vec<PainterCommand>,
    ) {
        // Layer scope commands are bookkeeping, not drawing: a push runs even
        // when its region is outside the damage clip or its node is filtered
        // out of this pass, because dropping it would either leave the pop
        // restoring a layer that was never opened or paint the subtree
        // unblurred.
        match kind {
            DisplayPaintCommandKind::ApplyBackdropFilterCompositor => {
                // The presentation backend owns the compositor region. No
                // client-side command is emitted, so the SHM pixels remain
                // unchanged and the selected policy is observable in the
                // retained topology.
                return;
            }
            DisplayPaintCommandKind::ApplyBackdropFilterInSurface => {
                if !self.paint_backend_supports_backdrop_blur() {
                    self.record_painter_diagnostic(PainterDiagnostic {
                        backend_id: self.paint_backend_id(),
                        feature: UnsupportedPainterFeature::BackdropBlur,
                        message: "in-surface backdrop blur is unsupported; painting flat"
                            .to_string(),
                        source: Some(PainterDiagnosticSource {
                            node_id: Some(command.node.id),
                            property: Some("backdrop-filter".to_string()),
                        }),
                    });
                    return;
                }
                let bounds = scaled_display_node_bounds(&command.node, scale);
                let clip = intersect_clip(paint_clip, scaled_display_clip(command.clip, scale));
                self.execute_painter_commands_in_session(
                    session,
                    &[PainterCommand::ApplyFilter {
                        rect: bounds,
                        radii: scale_corners(command.node.style.border_radius, scale),
                        filter: PainterFilter::Backdrop(scaled_visual_filter(
                            command.node.style.backdrop_filter,
                            scale,
                        )),
                        clip,
                    }],
                );
                return;
            }
            DisplayPaintCommandKind::ApplyBackdropFilterRejected => {
                self.record_painter_diagnostic(PainterDiagnostic {
                    backend_id: self.paint_backend_id(),
                    feature: UnsupportedPainterFeature::BackdropBlur,
                    message: "backdrop blur rejected by presentation policy; painting flat"
                        .to_string(),
                    source: Some(PainterDiagnosticSource {
                        node_id: Some(command.node.id),
                        property: Some("backdrop-filter".to_string()),
                    }),
                });
                return;
            }
            DisplayPaintCommandKind::PushCompositingLayer => {
                let bounds = intersect_clip(paint_clip, scaled_display_clip(command.clip, scale));
                self.execute_painter_commands_in_session(
                    session,
                    &[PainterCommand::PushLayer(PainterLayer::isolated(
                        bounds,
                        command.node.style.opacity,
                        PainterBlendMode::from_style(command.node.style.mix_blend_mode),
                    ))],
                );
                return;
            }
            DisplayPaintCommandKind::PopCompositingLayer => {
                self.execute_painter_commands_in_session(session, &[PainterCommand::PopLayer]);
                return;
            }
            DisplayPaintCommandKind::PushFilterLayer => {
                let bounds = intersect_clip(paint_clip, scaled_display_clip(command.clip, scale));
                self.execute_painter_commands_in_session(
                    session,
                    &[PainterCommand::PushLayer(PainterLayer::blurred(
                        bounds,
                        scaled_visual_filter(command.node.style.filter, scale),
                        self.blur_quality(),
                    ))],
                );
                return;
            }
            DisplayPaintCommandKind::PopFilterLayer => {
                self.execute_painter_commands_in_session(session, &[PainterCommand::PopLayer]);
                return;
            }
            _ => {}
        }
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
                    session,
                    scale,
                    node_bounds,
                    clip,
                    node_commands,
                    module_id,
                );
            }
            DisplayPaintCommandKind::Scrollbars => {
                let bounds = scaled_display_node_bounds(&command.node, scale);
                let node = &command.node;
                if requires_affine_paint(node) {
                    let local_bounds = scaled_display_local_bounds(node, scale);
                    let local_clip = local_clip_for(node.transform, scale, clip);
                    let save_count = self.paint_backend.begin_affine_node(
                        session,
                        node.transform,
                        &node.ancestor_clips,
                        scale,
                        clip,
                    );
                    self.render_display_scrollbars_in_session(
                        node,
                        session,
                        scale,
                        local_bounds,
                        local_clip,
                    );
                    self.paint_backend.end_affine_node(session, save_count);
                } else {
                    session.with_buffer(|buffer| {
                        self.render_display_scrollbars(node, buffer, scale, bounds, clip);
                    });
                }
            }
            DisplayPaintCommandKind::PushFilterLayer | DisplayPaintCommandKind::PopFilterLayer => {}
            DisplayPaintCommandKind::PushCompositingLayer
            | DisplayPaintCommandKind::PopCompositingLayer
            | DisplayPaintCommandKind::ApplyBackdropFilterCompositor
            | DisplayPaintCommandKind::ApplyBackdropFilterInSurface
            | DisplayPaintCommandKind::ApplyBackdropFilterRejected => {}
        }
    }

    fn render_display_node_self(
        &self,
        node: &DisplayPaintNode,
        session: &mut PixelCanvasSession<'_>,
        scale: f32,
        bounds: ClipRect,
        clip: ClipRect,
        node_commands: &mut Vec<PainterCommand>,
        module_id: Option<&str>,
    ) {
        if requires_affine_paint(node) {
            let local_bounds = scaled_display_local_bounds(node, scale);
            let local_clip = local_clip_for(node.transform, scale, clip);
            let save_count = self.paint_backend.begin_affine_node(
                session,
                node.transform,
                &node.ancestor_clips,
                scale,
                clip,
            );
            self.render_display_node_self_in_space(
                node,
                session,
                scale,
                local_bounds,
                local_clip,
                node_commands,
                module_id,
            );
            self.paint_backend.end_affine_node(session, save_count);
            return;
        }
        self.render_display_node_self_in_space(
            node,
            session,
            scale,
            bounds,
            clip,
            node_commands,
            module_id,
        );
    }

    fn render_display_node_self_in_space(
        &self,
        node: &DisplayPaintNode,
        session: &mut PixelCanvasSession<'_>,
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
            scale_corners(style.border_radius, scale),
            style.box_shadow,
            clip,
        );
        if style.background_color.a > 0 {
            push_fill_shape_command(
                node_commands,
                bounds,
                scale_corners(style.border_radius, scale),
                style.background_color,
                node_clip,
            );
        }
        push_background_paint_command(
            node_commands,
            &style.background_paint,
            bounds,
            scale_corners(style.border_radius, scale),
            node_clip,
        );

        push_border_commands(
            node_commands,
            bounds,
            &style.border_width,
            scale_corners(style.border_radius, scale),
            style.border_color,
            scale,
            node_clip,
        );
        self.execute_painter_commands_in_session(session, node_commands);
        node_commands.clear();

        match &node.content {
            DisplayPaintContent::Text(text) => {
                self.render_display_text_node(node, text, session, scale, x, y, node_clip);
            }
            DisplayPaintContent::Input(input) => {
                self.render_display_input_node(node, input, session, scale, x, y, node_clip);
            }
            DisplayPaintContent::Slider(slider) => {
                self.render_display_slider_node_in_session(
                    node, slider, session, scale, x, y, w, h, node_clip,
                );
            }
            DisplayPaintContent::Icon(icon) => {
                self.render_display_icon_node(
                    node,
                    icon,
                    session,
                    x,
                    y,
                    w,
                    h,
                    node.module_id.as_deref().or(module_id),
                );
            }
            DisplayPaintContent::Checkmark(mark) => {
                self.render_display_checkmark_node(node, *mark, session, bounds, node_clip);
            }
            DisplayPaintContent::None => {}
        }
    }

    /// Paints the selected-state glyph of a `checkbox`/`radio` as a vector
    /// path tinted with the element's content color (session path).
    fn render_display_checkmark_node(
        &self,
        node: &DisplayPaintNode,
        mark: DisplayCheckmarkPaint,
        session: &mut PixelCanvasSession<'_>,
        bounds: ClipRect,
        clip: ClipRect,
    ) {
        if let Some(command) = checkmark_command(mark.kind, bounds, node.style.color, clip) {
            self.execute_painter_commands_in_session(session, &[command]);
        }
    }
}

fn clipped_paint_regions(
    surface_clip: ClipRect,
    clips: &[(u32, u32, u32, u32)],
) -> SmallVec<[ClipRect; 16]> {
    clips
        .iter()
        .filter_map(|&(x, y, width, height)| {
            let clip = intersect_clip(
                surface_clip,
                ClipRect {
                    x: x.min(i32::MAX as u32) as i32,
                    y: y.min(i32::MAX as u32) as i32,
                    width: width.min(i32::MAX as u32) as i32,
                    height: height.min(i32::MAX as u32) as i32,
                },
            );
            (clip.width > 0 && clip.height > 0).then_some(clip)
        })
        .collect()
}

/// Whether the selected stream opens any layer scope. Layers span many
/// commands, so a per-command region walk cannot replay them correctly; the
/// caller falls back to one full pass per region.
fn selection_has_layer_scopes(commands: &SelectedDisplayListPaint<'_>) -> bool {
    commands
        .iter_with_kinds()
        .any(|(_, kind)| kind.is_layer_scope())
}

fn paint_regions_overlap(clips: &[ClipRect]) -> bool {
    clips.iter().enumerate().any(|(index, left)| {
        clips[index + 1..].iter().any(|right| {
            let overlap = intersect_clip(*left, *right);
            overlap.width > 0 && overlap.height > 0
        })
    })
}

fn paint_command_class(
    command: &DisplayPaintCommand,
    kind: DisplayPaintCommandKind,
) -> PaintCommandClass {
    if kind == DisplayPaintCommandKind::Scrollbars {
        return PaintCommandClass::Scrollbar;
    }
    match command.node.content {
        DisplayPaintContent::None => PaintCommandClass::Primitive,
        DisplayPaintContent::Text(_) => PaintCommandClass::Text,
        DisplayPaintContent::Icon(_) => PaintCommandClass::Icon,
        DisplayPaintContent::Input(_)
        | DisplayPaintContent::Slider(_)
        | DisplayPaintContent::Checkmark(_) => PaintCommandClass::Control,
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
        scale_corners(style.border_radius, scale),
        style.box_shadow,
        clip,
    );
    if style.background_color.a > 0 {
        push_fill_shape_command(
            commands,
            bounds,
            scale_corners(style.border_radius, scale),
            style.background_color,
            node_clip,
        );
    }
    push_background_paint_command(
        commands,
        &style.background_paint,
        bounds,
        scale_corners(style.border_radius, scale),
        node_clip,
    );

    push_border_commands(
        commands,
        bounds,
        &style.border_width,
        scale_corners(style.border_radius, scale),
        style.border_color,
        scale,
        node_clip,
    );
    commands.len() > start_len
}

fn push_box_shadow_command(
    commands: &mut Vec<PainterCommand>,
    rect: ClipRect,
    radii: Corners,
    shadow: BoxShadow,
    clip: ClipRect,
) {
    if shadow.is_none() || shadow.inset {
        return;
    }
    commands.push(PainterCommand::DrawShadow {
        rect,
        radii,
        shadow,
        clip,
    });
}

/// Builds the vector-path draw command for a `checkbox` tick or `radio` dot
/// within `bounds`, tinted `color`. Returns `None` for an empty box or fully
/// transparent color.
fn checkmark_command(
    kind: CheckmarkKind,
    bounds: ClipRect,
    color: Color,
    clip: ClipRect,
) -> Option<PainterCommand> {
    if color.a == 0 || bounds.width <= 0 || bounds.height <= 0 {
        return None;
    }
    let x = bounds.x as f32;
    let y = bounds.y as f32;
    let w = bounds.width as f32;
    let h = bounds.height as f32;
    let min = w.min(h);
    Some(match kind {
        CheckmarkKind::Check => PainterCommand::DrawPath {
            path: PainterPath {
                elements: vec![
                    PainterPathElement::MoveTo(x + w * 0.22, y + h * 0.52),
                    PainterPathElement::LineTo(x + w * 0.42, y + h * 0.72),
                    PainterPathElement::LineTo(x + w * 0.78, y + h * 0.28),
                ],
            },
            paint: PainterPaint::stroke(color, (min * 0.14).max(1.5)),
            clip,
        },
        CheckmarkKind::Dot => PainterCommand::DrawPath {
            path: circle_path(x + w * 0.5, y + h * 0.5, min * 0.28),
            paint: PainterPaint::fill(color),
            clip,
        },
    })
}

/// Builds a closed circle path centered at (`cx`, `cy`) using the standard
/// four-cubic-bezier approximation (control-point ratio kappa ≈ 0.5523).
fn circle_path(cx: f32, cy: f32, r: f32) -> PainterPath {
    let k = r * 0.552_284_8;
    PainterPath {
        elements: vec![
            PainterPathElement::MoveTo(cx, cy - r),
            PainterPathElement::CubicTo(cx + k, cy - r, cx + r, cy - k, cx + r, cy),
            PainterPathElement::CubicTo(cx + r, cy + k, cx + k, cy + r, cx, cy + r),
            PainterPathElement::CubicTo(cx - k, cy + r, cx - r, cy + k, cx - r, cy),
            PainterPathElement::CubicTo(cx - r, cy - k, cx - k, cy - r, cx, cy - r),
            PainterPathElement::Close,
        ],
    }
}

fn push_fill_shape_command(
    commands: &mut Vec<PainterCommand>,
    rect: ClipRect,
    radii: Corners,
    color: Color,
    clip: ClipRect,
) {
    let paint = PainterPaint::fill(color);
    if corners_have_radius(radii) {
        commands.push(PainterCommand::DrawRoundedRect {
            rect,
            radii,
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
    radii: Corners,
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
                radii,
                clip,
            });
        }
    }
}

fn push_border_commands(
    commands: &mut Vec<PainterCommand>,
    bounds: ClipRect,
    border_widths: &Edges,
    radii: Corners,
    color: Color,
    scale: f32,
    clip: ClipRect,
) {
    if color.a == 0
        || [
            border_widths.top,
            border_widths.right,
            border_widths.bottom,
            border_widths.left,
        ]
        .iter()
        .all(|width| *width <= 0.0)
    {
        return;
    }
    let widths = Edges {
        top: (border_widths.top * scale).max(0.0),
        right: (border_widths.right * scale).max(0.0),
        bottom: (border_widths.bottom * scale).max(0.0),
        left: (border_widths.left * scale).max(0.0),
    };
    commands.push(PainterCommand::DrawBorder {
        rect: bounds,
        radii,
        widths,
        paint: PainterPaint::fill(color),
        clip,
    });
}

fn scale_corners(corners: Corners, scale: f32) -> Corners {
    Corners {
        top_left: corners.top_left * scale,
        top_right: corners.top_right * scale,
        bottom_right: corners.bottom_right * scale,
        bottom_left: corners.bottom_left * scale,
    }
}

fn corners_have_radius(corners: Corners) -> bool {
    corners.top_left > 0.5
        || corners.top_right > 0.5
        || corners.bottom_right > 0.5
        || corners.bottom_left > 0.5
}

fn scaled_visual_filter(filter: VisualFilter, scale: f32) -> VisualFilter {
    VisualFilter {
        blur_radius: filter.blur_radius * scale,
    }
}

fn requires_affine_paint(node: &DisplayPaintNode) -> bool {
    node.transform.m12.abs() > 0.0001
        || node.transform.m21.abs() > 0.0001
        || node.transform.m11 < -0.0001
        || node.transform.m22 < -0.0001
}

fn scaled_display_local_bounds(node: &DisplayPaintNode, scale: f32) -> ClipRect {
    device_rect_to_clip(FractionalScale::new(scale).device_layout_rect(
        mesh_core_elements::LayoutRect {
            x: 0.0,
            y: 0.0,
            width: node.local_layout.width,
            height: node.local_layout.height,
        },
    ))
}

fn local_clip_for(transform: AffineTransform, scale: f32, clip: ClipRect) -> ClipRect {
    let device_transform = AffineTransform::scale(scale, scale).then(transform);
    let Some(inverse) = device_transform.inverse() else {
        return ClipRect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
    };
    let local = inverse.transform_rect(mesh_core_elements::LayoutRect {
        x: clip.x as f32,
        y: clip.y as f32,
        width: clip.width.max(0) as f32,
        height: clip.height.max(0) as f32,
    });
    device_rect_to_clip(FractionalScale::identity().device_layout_rect(local))
}

fn scaled_display_node_bounds(node: &DisplayPaintNode, scale: f32) -> ClipRect {
    device_rect_to_clip(FractionalScale::new(scale).device_layout_rect(node.layout))
}

fn scaled_display_clip(clip: DisplayListClip, scale: f32) -> ClipRect {
    device_rect_to_clip(FractionalScale::new(scale).device_layout_rect(
        mesh_core_elements::LayoutRect {
            x: clip.x as f32,
            y: clip.y as f32,
            width: clip.width.max(0) as f32,
            height: clip.height.max(0) as f32,
        },
    ))
}

fn device_rect_to_clip(rect: DeviceRect) -> ClipRect {
    ClipRect {
        x: rect.x,
        y: rect.y,
        width: rect.width.max(0),
        height: rect.height.max(0),
    }
}
