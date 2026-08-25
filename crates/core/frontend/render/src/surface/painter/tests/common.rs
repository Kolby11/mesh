use super::super::*;
use crate::display_list::RetainedDisplayList;
use mesh_core_elements::layout::LayoutRect;
use mesh_core_elements::style::{Dimension, Edges};
use std::sync::{Arc, Mutex};

pub(super) fn node(tag: &str, layout: LayoutRect, color: Color) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    node.layout = layout;
    node.computed_style.width = Dimension::Px(layout.width);
    node.computed_style.height = Dimension::Px(layout.height);
    node.computed_style.background_color = color;
    node
}

pub(super) fn text_node(
    text: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: Color,
) -> WidgetNode {
    let mut node = node(
        "text",
        LayoutRect {
            x,
            y,
            width,
            height,
        },
        Color::TRANSPARENT,
    );
    node.attributes.insert("content".into(), text.into());
    node.attributes.insert("selectable".into(), "true".into());
    node.computed_style.color = color;
    node.computed_style.font_size = 14.0;
    node.computed_style.line_height = 1.4;
    node.computed_style.padding = Edges::zero();
    node
}

pub(super) fn pixel(buffer: &PixelBuffer, x: u32, y: u32) -> Color {
    buffer.get_pixel(x, y)
}

pub(super) fn multi_region_fixture() -> (RetainedDisplayList, Vec<(u32, u32, u32, u32)>) {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 512.0,
            height: 512.0,
        },
        Color {
            r: 12,
            g: 18,
            b: 24,
            a: 255,
        },
    );
    root.id = 1;
    let mut clips = Vec::with_capacity(16);
    for row in 0..16 {
        for column in 0..16 {
            let x = (column * 32) as f32;
            let y = (row * 32) as f32;
            let mut child = node(
                "box",
                LayoutRect {
                    x,
                    y,
                    width: 24.0,
                    height: 24.0,
                },
                Color {
                    r: 40 + row as u8 * 3,
                    g: 60 + column as u8 * 2,
                    b: 90,
                    a: 220,
                },
            );
            child.id = 2 + (row * 16 + column) as u64;
            root.children.push(child);
        }
        clips.push(((row * 32 + 4) as u32, (row * 32 + 4) as u32, 12, 12));
    }
    let mut list = RetainedDisplayList::default();
    list.update(&root, 512, 512, true, true);
    (list, clips)
}

pub(super) fn clear_regions(buffer: &mut PixelBuffer, clips: &[(u32, u32, u32, u32)]) {
    for &(x, y, width, height) in clips {
        buffer.clear_rect(x, y, width, height, Color::TRANSPARENT);
    }
}

pub(super) fn paint_regions_repeated(
    selected: &crate::SelectedDisplayListPaint<'_>,
    buffer: &mut PixelBuffer,
    clips: &[(u32, u32, u32, u32)],
) {
    clear_regions(buffer, clips);
    for &clip in clips {
        let _ = crate::surface::paint_selected_display_list_for_module_with_profiling_metrics(
            selected,
            buffer,
            1.0,
            Some(clip),
            None,
            None,
            None,
        );
    }
}

pub(super) fn paint_regions_batched(
    selected: &crate::SelectedDisplayListPaint<'_>,
    buffer: &mut PixelBuffer,
    clips: &[(u32, u32, u32, u32)],
) {
    clear_regions(buffer, clips);
    let _ = crate::surface::paint_selected_display_list_regions_for_module_with_profiling_metrics_and_attribution(
        selected,
        buffer,
        1.0,
        clips,
        None,
        None,
        None,
        false,
    );
}

pub(super) fn full_clip(width: i32, height: i32) -> ClipRect {
    ClipRect {
        x: 0,
        y: 0,
        width,
        height,
    }
}

pub(super) fn write_effect_test_image(name: &str) -> (tempfile::TempDir, String) {
    // The aggregate effect-suite test calls the individual image tests while
    // the harness may run those same tests on other threads. A private tempdir
    // prevents one save from truncating another test's fixture between the
    // metadata lookup and image decode, and cleans the fixture after the test.
    let dir = tempfile::tempdir().expect("create effect image fixture dir");
    let path = dir.path().join(name);
    let mut image = image::RgbaImage::new(2, 1);
    image.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    image.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
    image.save(&path).expect("write effect image fixture");
    let path = path.to_string_lossy().into_owned();
    (dir, path)
}

pub(super) fn painter_command_classes(commands: &[PainterCommand]) -> Vec<&'static str> {
    commands
        .iter()
        .map(|command| match command {
            PainterCommand::PushClip(_) => "push_clip",
            PainterCommand::PopClip => "pop_clip",
            PainterCommand::PushLayer(_) => "push_layer",
            PainterCommand::PopLayer => "pop_layer",
            PainterCommand::DrawRect { .. } => "draw_rect",
            PainterCommand::DrawRoundedRect { .. } => "draw_rounded_rect",
            PainterCommand::DrawBorder { .. } => "draw_border",
            PainterCommand::DrawPath { .. } => "draw_path",
            PainterCommand::DrawImage { .. } => "draw_image",
            PainterCommand::DrawLinearGradient { .. } => "draw_linear_gradient",
            PainterCommand::DrawShadow { .. } => "draw_shadow",
            PainterCommand::ApplyFilter { .. } => "apply_filter",
        })
        .collect()
}

#[derive(Default)]
pub(super) struct TestPaintBackend;

impl PaintBackend for TestPaintBackend {
    fn id(&self) -> &'static str {
        "test"
    }

    fn capabilities(&self) -> PainterBackendCapabilities {
        let mut capabilities = SkiaPaintBackend.capabilities();
        capabilities.backend_id = self.id();
        capabilities
    }

    fn execute_commands(
        &self,
        buffer: &mut PixelBuffer,
        commands: &[PainterCommand],
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) {
        SkiaPaintBackend.execute_commands(buffer, commands, diagnostics);
    }

    fn fill_rect(&self, buffer: &mut PixelBuffer, rect: ClipRect, color: Color, clip: ClipRect) {
        SkiaPaintBackend.fill_rect(buffer, rect, color, clip);
    }

    fn fill_rounded_rect(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        color: Color,
        clip: ClipRect,
    ) {
        SkiaPaintBackend.fill_rounded_rect(buffer, rect, radius, color, clip);
    }

    fn stroke_rounded_rect(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        stroke_width: i32,
        color: Color,
        clip: ClipRect,
    ) -> bool {
        SkiaPaintBackend.stroke_rounded_rect(buffer, rect, radius, stroke_width, color, clip)
    }

    fn draw_box_shadow(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        shadow: BoxShadow,
        clip: ClipRect,
    ) {
        SkiaPaintBackend.draw_box_shadow(buffer, rect, radius, shadow, clip);
    }

    fn apply_backdrop_filter(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        filter: VisualFilter,
        clip: ClipRect,
    ) {
        SkiaPaintBackend.apply_backdrop_filter(buffer, rect, radius, filter, clip);
    }
}

#[derive(Clone, Default)]
pub(super) struct RecordingPaintBackend {
    commands: Arc<Mutex<Vec<PainterCommand>>>,
    execute_call_sizes: Arc<Mutex<Vec<usize>>>,
}

impl RecordingPaintBackend {
    pub(super) fn recorded_commands(&self) -> Vec<PainterCommand> {
        self.commands
            .lock()
            .map(|commands| commands.clone())
            .unwrap_or_default()
    }

    pub(super) fn execute_call_sizes(&self) -> Vec<usize> {
        self.execute_call_sizes
            .lock()
            .map(|sizes| sizes.clone())
            .unwrap_or_default()
    }
}

impl PaintBackend for RecordingPaintBackend {
    fn id(&self) -> &'static str {
        "recording"
    }

    fn capabilities(&self) -> PainterBackendCapabilities {
        PainterBackendCapabilities {
            backend_id: self.id(),
            clips: true,
            layers: true,
            rects: true,
            rounded_rects: true,
            paths: false,
            text: false,
            images: false,
            shadows: true,
            filters: true,
            backdrop_blur: false,
            blend_modes: true,
        }
    }

    fn execute_commands(
        &self,
        _buffer: &mut PixelBuffer,
        commands: &[PainterCommand],
        _diagnostics: &mut Vec<PainterDiagnostic>,
    ) {
        self.execute_call_sizes.lock().unwrap().push(commands.len());
        self.commands.lock().unwrap().extend_from_slice(commands);
    }
}
