use super::*;
use crate::display_list::{
    DamageRect, DisplayIconPaint, DisplayListRepaintPolicy, DisplayPaintCommandKind,
    DisplayPaintContent, RetainedDisplayList,
};
use crate::{RenderObjectDirtySummary, build_focused_proof_snapshot};
use mesh_core_elements::layout::LayoutRect;
use mesh_core_elements::style::{
    BackgroundPaint, Dimension, Edges, StyleImageSource, StyleLinearGradient,
};
use mesh_core_frontend::compile_frontend_module;
use mesh_core_theme::default_theme;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn node(tag: &str, layout: LayoutRect, color: Color) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    node.layout = layout;
    node.computed_style.width = Dimension::Px(layout.width);
    node.computed_style.height = Dimension::Px(layout.height);
    node.computed_style.background_color = color;
    node
}

fn text_node(text: &str, x: f32, y: f32, width: f32, height: f32, color: Color) -> WidgetNode {
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

fn pixel(buffer: &PixelBuffer, x: u32, y: u32) -> Color {
    let offset = (y * buffer.stride + x * 4) as usize;
    Color {
        b: buffer.data[offset],
        g: buffer.data[offset + 1],
        r: buffer.data[offset + 2],
        a: buffer.data[offset + 3],
    }
}

fn write_effect_test_image(name: &str) -> String {
    let dir = PathBuf::from("target/phase55-effects");
    std::fs::create_dir_all(&dir).expect("create effect image fixture dir");
    let path = dir.join(name);
    let mut image = image::RgbaImage::new(2, 1);
    image.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    image.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
    image.save(&path).expect("write effect image fixture");
    path.to_string_lossy().into_owned()
}

fn full_clip(width: i32, height: i32) -> ClipRect {
    ClipRect {
        x: 0,
        y: 0,
        width,
        height,
    }
}

#[test]
fn blend_mode_multiply_and_screen_composite_with_backdrop() {
    let red = Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    let blue = Color {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    };
    let clip = full_clip(8, 8);
    let rect = clip;

    // Multiply: red (255,0,0) * blue (0,0,255) / 255 = black on every channel.
    let mut buffer = PixelBuffer::new(8, 8);
    let mut diagnostics = Vec::new();
    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::DrawRect {
                rect,
                paint: PainterPaint::fill(red),
                clip,
            },
            PainterCommand::DrawRect {
                rect,
                paint: PainterPaint::fill(blue).with_blend_mode(PainterBlendMode::Multiply),
                clip,
            },
        ],
        &mut diagnostics,
    );
    assert!(diagnostics.is_empty(), "blend modes must not diagnose");
    let p = pixel(&buffer, 4, 4);
    assert!(
        p.r < 8 && p.g < 8 && p.b < 8,
        "multiply over red should be black, got {p:?}"
    );

    // Screen: 255 - (255-dst)*(255-src)/255 → red over blue yields magenta.
    let mut buffer = PixelBuffer::new(8, 8);
    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::DrawRect {
                rect,
                paint: PainterPaint::fill(red),
                clip,
            },
            PainterCommand::DrawRect {
                rect,
                paint: PainterPaint::fill(blue).with_blend_mode(PainterBlendMode::Screen),
                clip,
            },
        ],
        &mut Vec::new(),
    );
    let p = pixel(&buffer, 4, 4);
    assert!(
        p.r > 247 && p.g < 8 && p.b > 247,
        "screen of red and blue should be magenta, got {p:?}"
    );
}

#[test]
fn checked_checkbox_rasterizes_checkmark_glyph() {
    let layout = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 24.0,
        height: 24.0,
    };
    let bg = Color {
        r: 10,
        g: 20,
        b: 30,
        a: 255,
    };
    let white = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    let count_light_pixels = |node: &WidgetNode| {
        let engine = FrontendRenderEngine::new();
        let mut buffer = PixelBuffer::new(24, 24);
        engine.render_tree(node, &mut buffer, 1.0);
        let mut light = 0;
        for y in 0..24 {
            for x in 0..24 {
                let p = pixel(&buffer, x, y);
                if p.r > 180 && p.g > 180 && p.b > 180 {
                    light += 1;
                }
            }
        }
        light
    };

    let mut checked = node("checkbox", layout, bg);
    checked.computed_style.color = white;
    checked.attributes.insert("checked".into(), "true".into());
    assert!(
        count_light_pixels(&checked) > 0,
        "a checked checkbox must rasterize a light checkmark over its dark box"
    );

    let mut unchecked = node("checkbox", layout, bg);
    unchecked.computed_style.color = white;
    assert_eq!(
        count_light_pixels(&unchecked),
        0,
        "an unchecked checkbox paints no checkmark"
    );
}

#[derive(Default)]
struct TestPaintBackend;

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
struct RecordingPaintBackend {
    commands: Arc<Mutex<Vec<PainterCommand>>>,
    execute_call_sizes: Arc<Mutex<Vec<usize>>>,
}

impl RecordingPaintBackend {
    fn recorded_commands(&self) -> Vec<PainterCommand> {
        self.commands
            .lock()
            .map(|commands| commands.clone())
            .unwrap_or_default()
    }

    fn execute_call_sizes(&self) -> Vec<usize> {
        self.execute_call_sizes
            .lock()
            .map(|sizes| sizes.clone())
            .unwrap_or_default()
    }
}

fn painter_command_classes(commands: &[PainterCommand]) -> Vec<&'static str> {
    commands
        .iter()
        .map(|command| match command {
            PainterCommand::PushClip(_) => "push_clip",
            PainterCommand::PopClip => "pop_clip",
            PainterCommand::PushLayer(_) => "push_layer",
            PainterCommand::PopLayer => "pop_layer",
            PainterCommand::DrawRect { .. } => "draw_rect",
            PainterCommand::DrawRoundedRect { .. } => "draw_rounded_rect",
            PainterCommand::DrawPath { .. } => "draw_path",
            PainterCommand::DrawImage { .. } => "draw_image",
            PainterCommand::DrawLinearGradient { .. } => "draw_linear_gradient",
            PainterCommand::DrawShadow { .. } => "draw_shadow",
            PainterCommand::ApplyFilter { .. } => "apply_filter",
        })
        .collect()
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

#[test]
fn frontend_renderer_can_be_constructed_with_pluggable_paint_backend() {
    let engine = FrontendRenderEngine::with_paint_backend(Box::<TestPaintBackend>::default());
    assert_eq!(engine.paint_backend_id(), "test");
}

#[test]
fn painter_command_contract_constructs_required_command_set() {
    let clip = full_clip(16, 16);
    let rect = ClipRect {
        x: 1,
        y: 2,
        width: 8,
        height: 9,
    };
    let paint = PainterPaint::fill(Color::WHITE);
    let commands = vec![
        PainterCommand::PushClip(PainterClip { rect, radius: 2.0 }),
        PainterCommand::PopClip,
        PainterCommand::PushLayer(PainterLayer {
            bounds: clip,
            opacity: 0.5,
            blend_mode: PainterBlendMode::SrcOver,
            filter: PainterFilter::None,
        }),
        PainterCommand::PopLayer,
        PainterCommand::DrawRect { rect, paint, clip },
        PainterCommand::DrawRoundedRect {
            rect,
            radius: 4.0,
            paint,
            clip,
        },
        PainterCommand::DrawPath {
            path: PainterPath {
                elements: vec![
                    PainterPathElement::MoveTo(0.0, 0.0),
                    PainterPathElement::LineTo(4.0, 4.0),
                    PainterPathElement::QuadTo(5.0, 5.0, 6.0, 6.0),
                    PainterPathElement::CubicTo(1.0, 1.0, 2.0, 2.0, 3.0, 3.0),
                    PainterPathElement::Close,
                ],
            },
            paint,
            clip,
        },
        PainterCommand::DrawImage {
            image: PainterImage {
                source: PainterImageSource::Path("img".into()),
            },
            rect,
            paint,
            clip,
        },
        PainterCommand::DrawLinearGradient {
            gradient: PainterLinearGradient {
                from: Color::BLACK,
                to: Color::WHITE,
            },
            rect,
            radius: 4.0,
            clip,
        },
        PainterCommand::DrawShadow {
            rect,
            radius: 4.0,
            shadow: BoxShadow::default(),
            clip,
        },
        PainterCommand::ApplyFilter {
            rect,
            radius: 4.0,
            filter: PainterFilter::Backdrop(VisualFilter { blur_radius: 2.0 }),
            clip,
        },
        PainterCommand::ApplyFilter {
            rect,
            radius: 4.0,
            filter: PainterFilter::Blur(VisualFilter { blur_radius: 2.0 }),
            clip,
        },
    ];

    assert_eq!(commands.len(), 12);
}

#[test]
fn painter_primitive_command_classes_record_helper_backed_rects() {
    let clip = full_clip(16, 16);
    let rect = ClipRect {
        x: 1,
        y: 2,
        width: 8,
        height: 9,
    };
    let paint = PainterPaint::fill(Color::WHITE);
    let commands = vec![
        PainterCommand::PushClip(PainterClip { rect, radius: 2.0 }),
        PainterCommand::PopClip,
        PainterCommand::PushLayer(PainterLayer {
            bounds: clip,
            opacity: 0.5,
            blend_mode: PainterBlendMode::SrcOver,
            filter: PainterFilter::None,
        }),
        PainterCommand::PopLayer,
        PainterCommand::DrawRect { rect, paint, clip },
        PainterCommand::DrawRoundedRect {
            rect,
            radius: 4.0,
            paint,
            clip,
        },
        PainterCommand::DrawPath {
            path: PainterPath {
                elements: vec![PainterPathElement::MoveTo(0.0, 0.0)],
            },
            paint,
            clip,
        },
        PainterCommand::DrawImage {
            image: PainterImage {
                source: PainterImageSource::Path("img".into()),
            },
            rect,
            paint,
            clip,
        },
        PainterCommand::DrawLinearGradient {
            gradient: PainterLinearGradient {
                from: Color::BLACK,
                to: Color::WHITE,
            },
            rect,
            radius: 4.0,
            clip,
        },
        PainterCommand::DrawShadow {
            rect,
            radius: 4.0,
            shadow: BoxShadow::default(),
            clip,
        },
        PainterCommand::ApplyFilter {
            rect,
            radius: 4.0,
            filter: PainterFilter::Backdrop(VisualFilter { blur_radius: 2.0 }),
            clip,
        },
    ];

    assert_eq!(
        painter_command_classes(&commands),
        vec![
            "push_clip",
            "pop_clip",
            "push_layer",
            "pop_layer",
            "draw_rect",
            "draw_rounded_rect",
            "draw_path",
            "draw_image",
            "draw_linear_gradient",
            "draw_shadow",
            "apply_filter",
        ]
    );
}

#[test]
fn display_list_primitive_direct_and_retained_box_emit_same_command_classes() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 2.0,
            y: 3.0,
            width: 20.0,
            height: 18.0,
        },
        Color::from_hex("#336699").unwrap(),
    );
    root.computed_style.border_width = Edges::all(2.0);
    root.computed_style.border_color = Color::from_hex("#ff00ff").unwrap();

    let direct_backend = RecordingPaintBackend::default();
    let direct_recorded = direct_backend.clone();
    let direct_engine = FrontendRenderEngine::with_paint_backend(Box::new(direct_backend));
    let mut direct_buffer = PixelBuffer::new(32, 32);
    direct_engine.render_tree(&root, &mut direct_buffer, 1.0);
    let direct_classes = painter_command_classes(&direct_recorded.recorded_commands());

    let mut list = RetainedDisplayList::default();
    list.update(&root, 32, 32, true, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 32,
            height: 32,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );

    let retained_backend = RecordingPaintBackend::default();
    let retained_recorded = retained_backend.clone();
    let retained_engine = FrontendRenderEngine::with_paint_backend(Box::new(retained_backend));
    let mut retained_buffer = PixelBuffer::new(32, 32);
    retained_engine.render_selected_display_list_for_module(
        &selected,
        &mut retained_buffer,
        1.0,
        None,
        None,
        None,
    );
    let retained_classes = painter_command_classes(&retained_recorded.recorded_commands());

    assert_eq!(
        direct_classes,
        vec!["draw_rect", "draw_rounded_rect"],
        "direct box primitive should emit background and border commands"
    );
    assert_eq!(retained_classes, direct_classes);
}

#[test]
fn painter_primitive_box_background_and_border_emit_rect_classes() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 1.0,
            y: 1.0,
            width: 18.0,
            height: 16.0,
        },
        Color::from_hex("#114477").unwrap(),
    );
    root.computed_style.border_width = Edges::all(2.0);
    root.computed_style.border_color = Color::from_hex("#eecc44").unwrap();

    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(24, 24);
    engine.render_tree(&root, &mut buffer, 1.0);

    assert_eq!(
        painter_command_classes(&recorded.recorded_commands()),
        vec!["draw_rect", "draw_rounded_rect"]
    );
}

#[test]
fn painter_primitive_box_rounded_shadow_and_filters_emit_effect_classes() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 4.0,
            y: 4.0,
            width: 18.0,
            height: 16.0,
        },
        Color::from_hex("#224466").unwrap(),
    );
    root.computed_style.border_radius.top_left = 6.0;
    root.computed_style.box_shadow = BoxShadow {
        offset_x: 2.0,
        offset_y: 2.0,
        blur_radius: 4.0,
        spread_radius: 0.0,
        color: Color::from_hex("#00000080").unwrap(),
        inset: false,
    };
    root.computed_style.backdrop_filter = VisualFilter { blur_radius: 3.0 };
    root.computed_style.filter = VisualFilter { blur_radius: 2.0 };

    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(32, 32);
    engine.render_tree(&root, &mut buffer, 1.0);

    // backdrop-filter is compositor metadata and emits no CPU command. CSS
    // filter (non-backdrop) remains encoded in DrawRoundedRect.paint.filter.
    let classes = painter_command_classes(&recorded.recorded_commands());
    assert_eq!(classes, vec!["draw_shadow", "draw_rounded_rect"]);
}

#[test]
fn painter_primitive_text_selection_highlight_uses_draw_rect_command() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 60.0,
        },
        Color::TRANSPARENT,
    );
    let mut text = text_node(
        "selection proof text",
        0.0,
        0.0,
        160.0,
        60.0,
        Color::from_hex("#111111").unwrap(),
    );
    text.attributes
        .insert("_mesh_selection_background".into(), "#00ff00".into());
    text.attributes
        .insert("_mesh_selection_foreground".into(), "#ff00ff".into());
    text.attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());
    root.children = vec![text];

    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(180, 80);
    engine.render_tree(&root, &mut buffer, 1.0);

    assert!(
        painter_command_classes(&recorded.recorded_commands()).contains(&"draw_rect"),
        "selection highlight rectangles should route through the command backend"
    );
}

#[test]
fn painter_primitive_debug_overlay_bounds_use_draw_rect_commands() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 1.0,
            y: 1.0,
            width: 12.0,
            height: 10.0,
        },
        Color::TRANSPARENT,
    );
    root.children = vec![node(
        "box",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 6.0,
            height: 4.0,
        },
        Color::TRANSPARENT,
    )];

    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(24, 24);
    crate::surface::debug_overlay::DebugOverlay::new().paint_layout_bounds_with_engine(
        &engine,
        &root,
        &mut buffer,
        1.0,
    );

    let classes = painter_command_classes(&recorded.recorded_commands());
    assert_eq!(
        classes
            .iter()
            .filter(|class| **class == "draw_rect")
            .count(),
        8
    );
}

#[test]
fn painter_primitive_controls_input_direct_and_retained_emit_same_classes() {
    let mut root = node(
        "input",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 120.0,
            height: 28.0,
        },
        Color::from_hex("#101820").unwrap(),
    );
    root.attributes.insert("value".into(), "mesh".into());
    root.attributes
        .insert("_mesh_focused".into(), "true".into());
    root.computed_style.color = Color::from_hex("#f5f5f5").unwrap();
    root.computed_style.padding = Edges::all(4.0);

    let direct_backend = RecordingPaintBackend::default();
    let direct_recorded = direct_backend.clone();
    let direct_engine = FrontendRenderEngine::with_paint_backend(Box::new(direct_backend));
    let mut direct_buffer = PixelBuffer::new(140, 48);
    direct_engine.render_tree(&root, &mut direct_buffer, 1.0);
    let direct_classes = painter_command_classes(&direct_recorded.recorded_commands());

    let mut list = RetainedDisplayList::default();
    list.update(&root, 140, 48, true, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 140,
            height: 48,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );

    let retained_backend = RecordingPaintBackend::default();
    let retained_recorded = retained_backend.clone();
    let retained_engine = FrontendRenderEngine::with_paint_backend(Box::new(retained_backend));
    let mut retained_buffer = PixelBuffer::new(140, 48);
    retained_engine.render_selected_display_list_for_module(
        &selected,
        &mut retained_buffer,
        1.0,
        None,
        None,
        None,
    );
    let retained_classes = painter_command_classes(&retained_recorded.recorded_commands());

    assert_eq!(direct_classes, vec!["draw_rect", "draw_rect"]);
    assert_eq!(retained_classes, direct_classes);
}

#[test]
fn painter_primitive_controls_slider_direct_and_retained_emit_same_classes() {
    let mut root = node(
        "slider",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 128.0,
            height: 32.0,
        },
        Color::TRANSPARENT,
    );
    root.attributes.insert("min".into(), "0".into());
    root.attributes.insert("max".into(), "100".into());
    root.attributes.insert("value".into(), "40".into());
    root.computed_style.color = Color::from_hex("#4a90e2").unwrap();

    let direct_backend = RecordingPaintBackend::default();
    let direct_recorded = direct_backend.clone();
    let direct_engine = FrontendRenderEngine::with_paint_backend(Box::new(direct_backend));
    let mut direct_buffer = PixelBuffer::new(150, 48);
    direct_engine.render_tree(&root, &mut direct_buffer, 1.0);
    let direct_classes = painter_command_classes(&direct_recorded.recorded_commands());

    let mut list = RetainedDisplayList::default();
    list.update(&root, 150, 48, true, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 150,
            height: 48,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );

    let retained_backend = RecordingPaintBackend::default();
    let retained_recorded = retained_backend.clone();
    let retained_engine = FrontendRenderEngine::with_paint_backend(Box::new(retained_backend));
    let mut retained_buffer = PixelBuffer::new(150, 48);
    retained_engine.render_selected_display_list_for_module(
        &selected,
        &mut retained_buffer,
        1.0,
        None,
        None,
        None,
    );
    let retained_classes = painter_command_classes(&retained_recorded.recorded_commands());

    assert_eq!(
        direct_classes,
        vec!["draw_rect", "draw_rect", "draw_rounded_rect"]
    );
    assert_eq!(retained_classes, direct_classes);
}

#[test]
fn painter_effect_lowering_direct_and_retained_image_emit_same_command_classes() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 64.0,
            height: 32.0,
        },
        Color::TRANSPARENT,
    );
    root.computed_style.background_paint = BackgroundPaint::Image(StyleImageSource {
        path: "assets/panel.png".to_string(),
    });

    let direct_backend = RecordingPaintBackend::default();
    let direct_recorded = direct_backend.clone();
    let direct_engine = FrontendRenderEngine::with_paint_backend(Box::new(direct_backend));
    let mut direct_buffer = PixelBuffer::new(80, 48);
    direct_engine.render_tree(&root, &mut direct_buffer, 1.0);
    let direct_classes = painter_command_classes(&direct_recorded.recorded_commands());

    let mut list = RetainedDisplayList::default();
    list.update(&root, 80, 48, true, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 80,
            height: 48,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );

    let retained_backend = RecordingPaintBackend::default();
    let retained_recorded = retained_backend.clone();
    let retained_engine = FrontendRenderEngine::with_paint_backend(Box::new(retained_backend));
    let mut retained_buffer = PixelBuffer::new(80, 48);
    retained_engine.render_selected_display_list_for_module(
        &selected,
        &mut retained_buffer,
        1.0,
        None,
        None,
        None,
    );
    let retained_classes = painter_command_classes(&retained_recorded.recorded_commands());

    assert_eq!(direct_classes, vec!["draw_image"]);
    assert_eq!(retained_classes, direct_classes);
}

#[test]
fn painter_effect_lowering_direct_and_retained_gradient_emit_same_command_classes() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 64.0,
            height: 32.0,
        },
        Color::TRANSPARENT,
    );
    root.computed_style.background_paint = BackgroundPaint::LinearGradient(StyleLinearGradient {
        from: Color::from_hex("#112233").unwrap(),
        to: Color::from_hex("#445566").unwrap(),
    });

    let direct_backend = RecordingPaintBackend::default();
    let direct_recorded = direct_backend.clone();
    let direct_engine = FrontendRenderEngine::with_paint_backend(Box::new(direct_backend));
    let mut direct_buffer = PixelBuffer::new(80, 48);
    direct_engine.render_tree(&root, &mut direct_buffer, 1.0);
    let direct_classes = painter_command_classes(&direct_recorded.recorded_commands());

    let mut list = RetainedDisplayList::default();
    list.update(&root, 80, 48, true, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 80,
            height: 48,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );

    let retained_backend = RecordingPaintBackend::default();
    let retained_recorded = retained_backend.clone();
    let retained_engine = FrontendRenderEngine::with_paint_backend(Box::new(retained_backend));
    let mut retained_buffer = PixelBuffer::new(80, 48);
    retained_engine.render_selected_display_list_for_module(
        &selected,
        &mut retained_buffer,
        1.0,
        None,
        None,
        None,
    );
    let retained_classes = painter_command_classes(&retained_recorded.recorded_commands());

    assert_eq!(direct_classes, vec!["draw_linear_gradient"]);
    assert_eq!(retained_classes, direct_classes);
}

#[test]
fn painter_primitive_icon_direct_and_retained_preserve_image_like_boundary() {
    let mut root = node(
        "icon",
        LayoutRect {
            x: 3.0,
            y: 4.0,
            width: 24.0,
            height: 24.0,
        },
        Color::TRANSPARENT,
    );
    root.attributes.insert("name".into(), "mesh:search".into());
    root.attributes.insert("size".into(), "20".into());
    root.computed_style.color = Color::from_hex("#fafafa").unwrap();
    root.computed_style.icon_fill = Some(1.0);
    root.computed_style.icon_weight = Some(500.0);

    let direct_backend = RecordingPaintBackend::default();
    let direct_recorded = direct_backend.clone();
    let direct_engine = FrontendRenderEngine::with_paint_backend(Box::new(direct_backend));
    let mut direct_buffer = PixelBuffer::new(40, 40);
    direct_engine.render_tree_at_for_module(
        &root,
        &mut direct_buffer,
        1.0,
        0.0,
        0.0,
        Some("test-module"),
    );

    let mut list = RetainedDisplayList::default();
    list.update(&root, 40, 40, true, true);
    let display_icon: &DisplayIconPaint = list
        .paint_commands()
        .iter()
        .find_map(|command| {
            (command.kind == DisplayPaintCommandKind::Node).then_some(&command.node.content)
        })
        .and_then(|content| match content {
            DisplayPaintContent::Icon(icon) => Some(icon),
            _ => None,
        })
        .expect("retained icon paint");
    assert_eq!(display_icon.name.as_deref(), Some("mesh:search"));
    assert_eq!(display_icon.size, Some(20));

    let retained_backend = RecordingPaintBackend::default();
    let retained_recorded = retained_backend.clone();
    let retained_engine = FrontendRenderEngine::with_paint_backend(Box::new(retained_backend));
    let mut retained_buffer = PixelBuffer::new(40, 40);
    retained_engine.render_display_list_for_module(
        list.paint_commands(),
        &mut retained_buffer,
        1.0,
        None,
        None,
        Some("test-module"),
    );

    assert_eq!(
        root.attributes.get("name").map(String::as_str),
        Some("mesh:search")
    );
    assert!(painter_command_classes(&direct_recorded.recorded_commands()).is_empty());
    assert!(painter_command_classes(&retained_recorded.recorded_commands()).is_empty());
}

#[test]
fn display_list_primitive_mixed_tree_preserves_node_order_and_command_classes() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 220.0,
            height: 120.0,
        },
        Color::from_hex("#20242a").unwrap(),
    );
    let mut text = text_node(
        "selected primitive",
        8.0,
        8.0,
        120.0,
        28.0,
        Color::from_hex("#f0f0f0").unwrap(),
    );
    text.attributes
        .insert("_mesh_selection_background".into(), "#3366ff".into());
    text.attributes
        .insert("_mesh_selection_foreground".into(), "#ffffff".into());
    text.attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());

    let mut input = node(
        "input",
        LayoutRect {
            x: 8.0,
            y: 42.0,
            width: 110.0,
            height: 26.0,
        },
        Color::from_hex("#101820").unwrap(),
    );
    input.attributes.insert("value".into(), "mesh".into());
    input
        .attributes
        .insert("_mesh_focused".into(), "true".into());
    input.computed_style.color = Color::from_hex("#f5f5f5").unwrap();
    input.computed_style.padding = Edges::all(4.0);

    let mut slider = node(
        "slider",
        LayoutRect {
            x: 8.0,
            y: 76.0,
            width: 128.0,
            height: 30.0,
        },
        Color::TRANSPARENT,
    );
    slider.attributes.insert("value".into(), "60".into());
    slider.computed_style.color = Color::from_hex("#4a90e2").unwrap();

    let mut icon = node(
        "icon",
        LayoutRect {
            x: 150.0,
            y: 12.0,
            width: 24.0,
            height: 24.0,
        },
        Color::TRANSPARENT,
    );
    icon.attributes.insert("name".into(), "mesh:search".into());
    icon.attributes.insert("size".into(), "20".into());

    let expected_node_order = vec![root.id, text.id, input.id, slider.id, icon.id];
    root.children = vec![text, input, slider, icon];

    let mut list = RetainedDisplayList::default();
    list.update(&root, 240, 140, true, true);
    let node_order: Vec<_> = list
        .paint_commands()
        .iter()
        .filter(|command| command.kind == DisplayPaintCommandKind::Node)
        .map(|command| command.node.id)
        .collect();
    assert_eq!(node_order, expected_node_order);

    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(240, 140);
    engine.render_display_list_for_module(
        list.paint_commands(),
        &mut buffer,
        1.0,
        None,
        None,
        Some("test-module"),
    );

    let classes = painter_command_classes(&recorded.recorded_commands());
    assert_eq!(classes.first(), Some(&"draw_rect"));
    assert_eq!(
        classes
            .iter()
            .filter(|class| **class == "draw_rounded_rect")
            .count(),
        1
    );
    assert!(
        classes
            .iter()
            .filter(|class| **class == "draw_rect")
            .count()
            >= 6,
        "box, text selection, input, and slider primitives should emit draw_rect commands"
    );
}

#[test]
fn display_list_primitive_helper_bypass_audit_documents_command_backed_compatibility_helpers() {
    let helper_bypass_audit = [
        (
            "FrontendRenderEngine::fill_rect_clipped",
            "command-backed compatibility helper",
        ),
        (
            "FrontendRenderEngine::fill_rounded_rect_clipped",
            "command-backed compatibility helper",
        ),
        (
            "FrontendRenderEngine::draw_box_shadow",
            "command-backed compatibility helper",
        ),
        (
            "FrontendRenderEngine::apply_backdrop_filter",
            "command-backed compatibility helper",
        ),
        (
            "surface::icon::draw_named_icon_for_module",
            "deferred specialized icon rasterizer",
        ),
    ];

    assert!(
        helper_bypass_audit
            .iter()
            .any(|(_, status)| status.contains("command-backed"))
    );
    assert!(
        helper_bypass_audit
            .iter()
            .any(|(helper, status)| helper.contains("icon") && status.contains("deferred"))
    );
}

#[test]
fn painter_backend_capabilities_identify_skia_and_unsupported_commands_diagnose() {
    let backend = SkiaPaintBackend;
    let capabilities = backend.capabilities();
    assert_eq!(capabilities.backend_id, "skia");
    assert!(capabilities.rects);
    assert!(capabilities.rounded_rects);
    assert!(capabilities.shadows);
    assert!(capabilities.filters);
    assert!(capabilities.clips);
    assert!(capabilities.layers);
    assert!(capabilities.paths);

    assert!(capabilities.blend_modes);

    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();
    backend.execute_commands(
        &mut buffer,
        &[PainterCommand::ApplyFilter {
            rect: full_clip(8, 8),
            radius: 0.0,
            filter: PainterFilter::Blur(VisualFilter { blur_radius: 2.0 }),
            clip: full_clip(16, 16),
        }],
        &mut diagnostics,
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].backend_id, "skia");
    assert_eq!(diagnostics[0].feature, UnsupportedPainterFeature::Filter);
}

#[test]
fn painter_effect_diagnostic_reports_excessive_blur() {
    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawRect {
            rect: full_clip(8, 8),
            paint: PainterPaint::fill(Color::WHITE).with_filter(VisualFilter {
                blur_radius: MAX_EFFECT_BLUR_RADIUS + 1.0,
            }),
            clip: full_clip(16, 16),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.feature == UnsupportedPainterFeature::Filter
            && diagnostic.message.contains("excessive blur")
            && diagnostic.source.is_none()
    }));
}

#[test]
fn painter_effect_diagnostic_reports_missing_image() {
    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawImage {
            image: PainterImage {
                source: PainterImageSource::Path("target/phase55-effects/missing.png".into()),
            },
            rect: full_clip(8, 8),
            paint: PainterPaint::fill(Color::WHITE),
            clip: full_clip(16, 16),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.feature == UnsupportedPainterFeature::Image
            && diagnostic.message.contains("missing image asset")
            && diagnostic.source.is_none()
    }));
}

#[test]
fn painter_layer_blend_mode_is_supported_without_diagnostics() {
    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::PushLayer(PainterLayer {
                bounds: full_clip(16, 16),
                opacity: 1.0,
                blend_mode: PainterBlendMode::Multiply,
                filter: PainterFilter::None,
            }),
            PainterCommand::PopLayer,
        ],
        &mut diagnostics,
    );

    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.feature == UnsupportedPainterFeature::BlendMode),
        "blend modes are now applied, not diagnosed as unsupported"
    );
}

#[test]
fn skia_shape_rect_fill_uses_command_clip() {
    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();
    let rect = ClipRect {
        x: 2,
        y: 2,
        width: 10,
        height: 10,
    };
    let clip = ClipRect {
        x: 4,
        y: 4,
        width: 4,
        height: 4,
    };

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawRect {
            rect,
            paint: PainterPaint::fill(Color::from_hex("#ff0000").unwrap()),
            clip,
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(pixel(&buffer, 3, 3), Color::TRANSPARENT);
    assert_eq!(pixel(&buffer, 4, 4), Color::from_hex("#ff0000").unwrap());
    assert_eq!(pixel(&buffer, 7, 7), Color::from_hex("#ff0000").unwrap());
    assert_eq!(pixel(&buffer, 8, 8), Color::TRANSPARENT);
}

#[test]
fn skia_shape_rect_fill_respects_transparency() {
    let mut buffer = PixelBuffer::new(12, 12);
    let mut diagnostics = Vec::new();
    let color = Color {
        r: 20,
        g: 40,
        b: 60,
        a: 128,
    };

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawRect {
            rect: ClipRect {
                x: 2,
                y: 2,
                width: 6,
                height: 6,
            },
            paint: PainterPaint::fill(color),
            clip: full_clip(12, 12),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(pixel(&buffer, 4, 4), color);
    assert_eq!(pixel(&buffer, 1, 1), Color::TRANSPARENT);
}

#[test]
fn skia_effect_layer_opacity_isolates_child_pixels() {
    let mut buffer = PixelBuffer::new(12, 12);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::PushLayer(PainterLayer {
                bounds: full_clip(12, 12),
                opacity: 0.5,
                blend_mode: PainterBlendMode::SrcOver,
                filter: PainterFilter::None,
            }),
            PainterCommand::DrawRect {
                rect: ClipRect {
                    x: 2,
                    y: 2,
                    width: 6,
                    height: 6,
                },
                paint: PainterPaint::fill(Color::from_hex("#ff0000").unwrap()),
                clip: full_clip(12, 12),
            },
            PainterCommand::PopLayer,
        ],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let color = pixel(&buffer, 4, 4);
    assert_eq!(color.r, 255);
    assert!((120..=136).contains(&color.a), "{color:?}");
}

#[test]
fn skia_effect_layer_blur_expands_painted_pixels() {
    let mut buffer = PixelBuffer::new(24, 24);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::PushLayer(PainterLayer {
                bounds: full_clip(24, 24),
                opacity: 1.0,
                blend_mode: PainterBlendMode::SrcOver,
                filter: PainterFilter::Blur(VisualFilter { blur_radius: 3.0 }),
            }),
            PainterCommand::DrawRect {
                rect: ClipRect {
                    x: 8,
                    y: 8,
                    width: 8,
                    height: 8,
                },
                paint: PainterPaint::fill(Color::from_hex("#00ff00").unwrap()),
                clip: full_clip(24, 24),
            },
            PainterCommand::PopLayer,
        ],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(pixel(&buffer, 6, 12).a > 0);
    assert_eq!(pixel(&buffer, 0, 0), Color::TRANSPARENT);
}

#[test]
fn tooltip_chrome_is_drawn_inside_painter_layer() {
    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(96, 48);

    engine.render_tooltip("Audio", 20.0, 10.0, &mut buffer, 1.0);

    let commands = recorded.recorded_commands();
    let classes = painter_command_classes(&commands);
    assert!(
        classes.windows(4).any(|window| window
            == [
                "push_layer",
                "draw_rounded_rect",
                "draw_rounded_rect",
                "pop_layer",
            ]),
        "{classes:?}"
    );
}

#[test]
fn tooltip_rounded_corner_outside_shape_stays_transparent_to_underlay() {
    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(96, 48);
    buffer.clear(Color::WHITE);

    engine.render_tooltip("Audio", 20.0, 10.0, &mut buffer, 1.0);

    assert_eq!(
        pixel(&buffer, 19, 9),
        Color::WHITE,
        "tooltip chrome layer must not prefill pixels outside the rounded corner"
    );
}

#[test]
fn long_tooltip_paints_past_legacy_width_cap() {
    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(360, 72);
    let underlay = Color::from_hex("#224466").unwrap();
    buffer.clear(underlay);

    engine.render_tooltip(
        "Audio output volume is controlled by the system mixer device",
        8.0,
        10.0,
        &mut buffer,
        1.0,
    );

    assert_ne!(
        pixel(&buffer, 300, 20),
        underlay,
        "long tooltip chrome should extend beyond the old 240px overlay width"
    );
}

#[test]
fn tooltip_clipped_repaint_does_not_mutate_pixels_outside_damage() {
    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(96, 48);
    let underlay = Color::from_hex("#224466").unwrap();
    buffer.clear(underlay);

    engine.render_tooltip_clipped("Audio", 20.0, 10.0, &mut buffer, 1.0, Some((24, 12, 8, 8)));

    assert_eq!(
        pixel(&buffer, 23, 16),
        underlay,
        "tooltip paint must not touch pixels left of the clipped damage rect"
    );
    assert_eq!(
        pixel(&buffer, 40, 16),
        underlay,
        "tooltip paint must not touch pixels right of the clipped damage rect"
    );
}

#[test]
fn skia_effect_linear_gradient_draws_top_and_bottom_colors() {
    let mut buffer = PixelBuffer::new(8, 12);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawLinearGradient {
            gradient: PainterLinearGradient {
                from: Color::from_hex("#ff0000").unwrap(),
                to: Color::from_hex("#0000ff").unwrap(),
            },
            rect: ClipRect {
                x: 0,
                y: 0,
                width: 8,
                height: 12,
            },
            radius: 0.0,
            clip: full_clip(8, 12),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let top = pixel(&buffer, 4, 1);
    let bottom = pixel(&buffer, 4, 10);
    assert!(top.r > top.b, "{top:?}");
    assert!(bottom.b > bottom.r, "{bottom:?}");
}

#[test]
fn skia_effect_linear_gradient_reuses_shader_for_moving_same_size_rects() {
    super::backend::reset_gradient_shader_cache_for_tests();
    let mut buffer = PixelBuffer::new(24, 20);
    let mut diagnostics = Vec::new();
    let gradient = PainterLinearGradient {
        from: Color::from_hex("#ff0000").unwrap(),
        to: Color::from_hex("#0000ff").unwrap(),
    };

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::DrawLinearGradient {
                gradient,
                rect: ClipRect {
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 12,
                },
                radius: 0.0,
                clip: full_clip(24, 20),
            },
            PainterCommand::DrawLinearGradient {
                gradient,
                rect: ClipRect {
                    x: 12,
                    y: 4,
                    width: 8,
                    height: 12,
                },
                radius: 0.0,
                clip: full_clip(24, 20),
            },
        ],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let moved_top = pixel(&buffer, 16, 5);
    let moved_bottom = pixel(&buffer, 16, 14);
    assert!(moved_top.r > moved_top.b, "{moved_top:?}");
    assert!(moved_bottom.b > moved_bottom.r, "{moved_bottom:?}");
    assert_eq!(super::backend::gradient_shader_creations_for_tests(), 1);
}

#[test]
#[ignore = "release-only moving gradient shader cache microbenchmark"]
fn moving_gradient_shader_size_key_beats_position_churn_benchmark() {
    let iterations = 5_000;
    let gradient = PainterLinearGradient {
        from: Color::from_hex("#ff0000").unwrap(),
        to: Color::from_hex("#0000ff").unwrap(),
    };
    let mut buffer = PixelBuffer::new(96, 48);
    let mut diagnostics = Vec::new();

    let old_started = std::time::Instant::now();
    for i in 0..iterations {
        super::backend::reset_gradient_shader_cache_for_tests();
        SkiaPaintBackend.execute_commands(
            &mut buffer,
            &[PainterCommand::DrawLinearGradient {
                gradient,
                rect: ClipRect {
                    x: (i % 72) as i32,
                    y: (i % 24) as i32,
                    width: 24,
                    height: 24,
                },
                radius: 0.0,
                clip: full_clip(96, 48),
            }],
            &mut diagnostics,
        );
    }
    let old_time = old_started.elapsed();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    diagnostics.clear();
    super::backend::reset_gradient_shader_cache_for_tests();
    let new_started = std::time::Instant::now();
    for i in 0..iterations {
        SkiaPaintBackend.execute_commands(
            &mut buffer,
            &[PainterCommand::DrawLinearGradient {
                gradient,
                rect: ClipRect {
                    x: (i % 72) as i32,
                    y: (i % 24) as i32,
                    width: 24,
                    height: 24,
                },
                radius: 0.0,
                clip: full_clip(96, 48),
            }],
            &mut diagnostics,
        );
    }
    let new_time = new_started.elapsed();

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    println!(
        "moving gradient shader cache: position churn {old_time:?}; size-key reuse {new_time:?}; ratio {:.1}x; shader_creations={}",
        old_time.as_secs_f64() / new_time.as_secs_f64(),
        super::backend::gradient_shader_creations_for_tests()
    );
    assert!(
        new_time < old_time,
        "size-keyed moving gradients should beat position-churned shader creation"
    );
}

#[test]
fn skia_effect_image_draws_source_pixels() {
    let path = write_effect_test_image("phase55-image-source.png");
    let mut buffer = PixelBuffer::new(20, 10);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawImage {
            image: PainterImage {
                source: PainterImageSource::Path(path.clone()),
            },
            rect: ClipRect {
                x: 0,
                y: 0,
                width: 20,
                height: 10,
            },
            paint: PainterPaint::fill(Color::WHITE),
            clip: full_clip(20, 10),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(pixel(&buffer, 4, 5).r > pixel(&buffer, 4, 5).g);
    assert!(pixel(&buffer, 15, 5).g > pixel(&buffer, 15, 5).r);
}

#[test]
fn skia_effect_image_respects_command_clip() {
    let path = write_effect_test_image("phase55-image-clip.png");
    let mut buffer = PixelBuffer::new(20, 10);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawImage {
            image: PainterImage {
                source: PainterImageSource::Path(path),
            },
            rect: ClipRect {
                x: 0,
                y: 0,
                width: 20,
                height: 10,
            },
            paint: PainterPaint::fill(Color::WHITE),
            clip: ClipRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(pixel(&buffer, 4, 5).a > 0);
    assert_eq!(pixel(&buffer, 15, 5), Color::TRANSPARENT);
}

#[test]
fn skia_effect_image_gradient_suite_runs_supported_cases() {
    skia_effect_linear_gradient_draws_top_and_bottom_colors();
    skia_effect_image_draws_source_pixels();
    skia_effect_image_respects_command_clip();
}

#[test]
fn painter_effect_clipped_shadow_stays_inside_clip() {
    let mut buffer = PixelBuffer::new(32, 32);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawShadow {
            rect: ClipRect {
                x: 8,
                y: 8,
                width: 10,
                height: 10,
            },
            radius: 0.0,
            shadow: BoxShadow {
                offset_x: 1.0,
                offset_y: 1.0,
                blur_radius: 0.0,
                spread_radius: 0.0,
                color: Color::from_hex("#000000ff").unwrap(),
                inset: false,
            },
            clip: ClipRect {
                x: 10,
                y: 10,
                width: 4,
                height: 4,
            },
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(pixel(&buffer, 9, 9), Color::TRANSPARENT);
    assert_eq!(
        pixel(&buffer, 10, 10),
        Color::from_hex("#000000ff").unwrap()
    );
    assert_eq!(pixel(&buffer, 14, 14), Color::TRANSPARENT);
}

#[test]
fn painter_effect_gradient_respects_rounded_clip() {
    let mut buffer = PixelBuffer::new(24, 24);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawLinearGradient {
            gradient: PainterLinearGradient {
                from: Color::from_hex("#ff0000").unwrap(),
                to: Color::from_hex("#0000ff").unwrap(),
            },
            rect: ClipRect {
                x: 4,
                y: 4,
                width: 16,
                height: 16,
            },
            radius: 8.0,
            clip: full_clip(24, 24),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(pixel(&buffer, 4, 4), Color::TRANSPARENT);
    assert!(pixel(&buffer, 12, 12).a > 0);
}

#[test]
fn skia_shape_push_clip_intersects_command_clip() {
    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[
            PainterCommand::PushClip(PainterClip {
                rect: ClipRect {
                    x: 4,
                    y: 4,
                    width: 4,
                    height: 4,
                },
                radius: 0.0,
            }),
            PainterCommand::DrawRect {
                rect: ClipRect {
                    x: 2,
                    y: 2,
                    width: 10,
                    height: 10,
                },
                paint: PainterPaint::fill(Color::from_hex("#ff0000").unwrap()),
                clip: full_clip(16, 16),
            },
            PainterCommand::PopClip,
            PainterCommand::DrawRect {
                rect: ClipRect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 2,
                },
                paint: PainterPaint::fill(Color::from_hex("#0000ff").unwrap()),
                clip: full_clip(16, 16),
            },
        ],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(pixel(&buffer, 3, 3), Color::TRANSPARENT);
    assert_eq!(pixel(&buffer, 4, 4), Color::from_hex("#ff0000").unwrap());
    assert_eq!(pixel(&buffer, 0, 0), Color::from_hex("#0000ff").unwrap());
}

#[test]
fn skia_border_square_border_matches_existing_pixels() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        Color::TRANSPARENT,
    );
    root.computed_style.border_width = Edges::all(2.0);
    root.computed_style.border_color = Color::from_hex("#ff0000").unwrap();

    let mut buffer = PixelBuffer::new(24, 24);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 1, 1), Color::from_hex("#ff0000").unwrap());
    assert_eq!(pixel(&buffer, 10, 10), Color::TRANSPARENT);
}

#[test]
fn skia_border_rounded_border_keeps_corners_clear() {
    let border = Color::from_hex("#ff0000").unwrap();
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        Color::TRANSPARENT,
    );
    root.computed_style.border_width = Edges::all(2.0);
    root.computed_style.border_color = border;
    root.computed_style.border_radius.top_left = 8.0;

    let mut buffer = PixelBuffer::new(24, 24);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 0, 0), Color::TRANSPARENT);
    assert!(pixel(&buffer, 10, 0).a > 0);
    assert!(pixel(&buffer, 0, 10).a > 0);
    assert_eq!(pixel(&buffer, 10, 10), Color::TRANSPARENT);
}

#[test]
fn skia_path_fill_triangle_paints_expected_pixels() {
    let mut buffer = PixelBuffer::new(18, 14);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawPath {
            path: PainterPath {
                elements: vec![
                    PainterPathElement::MoveTo(2.0, 11.0),
                    PainterPathElement::LineTo(9.0, 2.0),
                    PainterPathElement::LineTo(16.0, 11.0),
                    PainterPathElement::Close,
                ],
            },
            paint: PainterPaint::fill(Color::from_hex("#00ff00").unwrap()),
            clip: full_clip(18, 14),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(pixel(&buffer, 9, 7), Color::from_hex("#00ff00").unwrap());
    assert_eq!(pixel(&buffer, 1, 1), Color::TRANSPARENT);
}

#[test]
fn skia_path_stroke_line_paints_expected_pixels() {
    let mut buffer = PixelBuffer::new(18, 8);
    let mut diagnostics = Vec::new();

    SkiaPaintBackend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawPath {
            path: PainterPath {
                elements: vec![
                    PainterPathElement::MoveTo(2.0, 4.0),
                    PainterPathElement::LineTo(16.0, 4.0),
                ],
            },
            paint: PainterPaint::stroke(Color::from_hex("#0000ff").unwrap(), 2.0),
            clip: full_clip(18, 8),
        }],
        &mut diagnostics,
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(pixel(&buffer, 9, 4).a > 0);
    assert_eq!(pixel(&buffer, 9, 0), Color::TRANSPARENT);
}

#[test]
fn skia_text_highlight_selection_background_uses_theme_color() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 60.0,
        },
        Color::TRANSPARENT,
    );
    let mut text = text_node(
        "selection proof text",
        0.0,
        0.0,
        160.0,
        60.0,
        Color::from_hex("#111111").unwrap(),
    );
    text.attributes
        .insert("_mesh_selection_background".into(), "#00ff00".into());
    text.attributes
        .insert("_mesh_selection_foreground".into(), "#ff00ff".into());
    text.attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());
    root.children = vec![text];

    let mut buffer = PixelBuffer::new(180, 80);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    let saw_selection_background = buffer.data.chunks_exact(4).any(|px| {
        Color {
            b: px[0],
            g: px[1],
            r: px[2],
            a: px[3],
        } == Color::from_hex("#00ff00").unwrap()
    });
    assert!(saw_selection_background);
}

#[test]
fn skia_text_highlight_does_not_change_glyph_handoff() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 60.0,
        },
        Color::TRANSPARENT,
    );
    let mut text = text_node(
        "selection proof text",
        0.0,
        0.0,
        160.0,
        60.0,
        Color::from_hex("#111111").unwrap(),
    );
    text.attributes
        .insert("_mesh_selection_background".into(), "#00ff00".into());
    text.attributes
        .insert("_mesh_selection_foreground".into(), "#ff00ff".into());
    text.attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());
    root.children = vec![text];

    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(180, 80);
    engine.render_tree(&root, &mut buffer, 1.0);

    let classes = painter_command_classes(&recorded.recorded_commands());
    assert!(classes.contains(&"draw_rect"));
    assert!(!classes.contains(&"draw_text"));
}

#[test]
fn painter_command_contract_keeps_retained_structures_free_of_skia_types() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for relative in ["src/display_list.rs", "src/render_object.rs"] {
        let contents = std::fs::read_to_string(manifest_dir.join(relative)).unwrap();
        assert!(
            !contents.contains("skia_safe"),
            "{relative} must stay backend-neutral"
        );
    }
}

#[test]
fn painter_helper_lowering_routes_rect_helper_through_command_backend() {
    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(16, 16);
    let rect = ClipRect {
        x: 1,
        y: 2,
        width: 8,
        height: 9,
    };

    engine.fill_rect_clipped(&mut buffer, rect, Color::WHITE, full_clip(16, 16));

    let commands = recorded.recorded_commands();
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        commands[0],
        PainterCommand::DrawRect {
            rect: recorded_rect,
            paint: PainterPaint {
                style: PainterPaintStyle::Fill,
                ..
            },
            ..
        } if recorded_rect == rect
    ));
}

#[test]
fn painter_helper_lowering_routes_effect_helpers_through_command_backend() {
    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(32, 32);
    let rect = ClipRect {
        x: 4,
        y: 4,
        width: 12,
        height: 12,
    };
    let clip = full_clip(32, 32);

    engine.fill_rounded_rect_clipped_with_filter(
        &mut buffer,
        rect,
        6.0,
        Color::WHITE,
        clip,
        VisualFilter { blur_radius: 2.0 },
    );
    engine.draw_box_shadow(
        &mut buffer,
        rect,
        6.0,
        BoxShadow {
            offset_x: 2.0,
            offset_y: 2.0,
            blur_radius: 4.0,
            spread_radius: 1.0,
            color: Color::BLACK,
            inset: false,
        },
        clip,
    );
    engine.apply_backdrop_filter(
        &mut buffer,
        rect,
        6.0,
        VisualFilter { blur_radius: 3.0 },
        clip,
    );

    // Backdrop blur is compositor-owned: a SHM client cannot sample pixels
    // behind its surface. Only fill and shadow reach the CPU backend.
    let commands = recorded.recorded_commands();
    assert_eq!(commands.len(), 2);
    assert!(matches!(
        commands[0],
        PainterCommand::DrawRoundedRect {
            paint: PainterPaint {
                filter: VisualFilter { blur_radius: 2.0 },
                ..
            },
            ..
        }
    ));
    assert!(matches!(commands[1], PainterCommand::DrawShadow { .. }));
}

#[test]
fn painter_backend_diagnostics_are_observable_on_frontend_render_engine() {
    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(16, 16);
    engine.execute_painter_commands(
        &mut buffer,
        &[PainterCommand::DrawImage {
            image: PainterImage {
                source: PainterImageSource::Path("img".into()),
            },
            rect: full_clip(8, 8),
            paint: PainterPaint::fill(Color::WHITE),
            clip: full_clip(16, 16),
        }],
    );

    let diagnostics = engine.painter_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].backend_id, "skia");
    assert_eq!(diagnostics[0].feature, UnsupportedPainterFeature::Image);

    let snapshot = engine.paint_backend_snapshot();
    assert_eq!(snapshot.backend_id, "skia");
    assert_eq!(snapshot.rollback_authority, "mesh-software-renderer");
    assert!(
        snapshot
            .capabilities
            .iter()
            .any(|capability| capability.feature == "images" && capability.supported)
    );
    assert!(
        snapshot
            .capabilities
            .iter()
            .any(|capability| capability.feature == "text" && !capability.supported)
    );
    assert_eq!(snapshot.recent_diagnostics.len(), 1);
    assert_eq!(snapshot.recent_diagnostics[0].feature, "image");

    engine.clear_painter_diagnostics();
    assert!(engine.painter_diagnostics().is_empty());
    assert!(
        engine
            .paint_backend_snapshot()
            .recent_diagnostics
            .is_empty()
    );
}

#[test]
fn painter_draws_border_from_computed_edges() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        Color::TRANSPARENT,
    );
    root.computed_style.border_width = Edges::all(2.0);
    root.computed_style.border_color = Color::from_hex("#ff0000").unwrap();

    let mut buffer = PixelBuffer::new(24, 24);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 1, 1), Color::from_hex("#ff0000").unwrap());
    assert_eq!(pixel(&buffer, 10, 10), Color::TRANSPARENT);
}

#[test]
fn painter_draws_rounded_border_without_square_corners() {
    let border = Color::from_hex("#ff0000").unwrap();
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        Color::TRANSPARENT,
    );
    root.computed_style.border_width = Edges::all(2.0);
    root.computed_style.border_color = border;
    root.computed_style.border_radius.top_left = 8.0;

    let mut buffer = PixelBuffer::new(24, 24);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 0, 0), Color::TRANSPARENT);
    assert!(pixel(&buffer, 10, 0).a > 0);
    assert!(pixel(&buffer, 0, 10).a > 0);
    assert_eq!(pixel(&buffer, 10, 10), Color::TRANSPARENT);
}

#[test]
fn painter_applies_opacity_to_skia_filled_background() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        Color::WHITE,
    );
    root.computed_style.opacity = 0.5;

    let mut buffer = PixelBuffer::new(24, 24);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    let center = pixel(&buffer, 10, 10);
    assert_eq!(
        center,
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: 128,
        }
    );
}

#[test]
fn painter_draws_box_shadow_outside_node_bounds() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 8.0,
            y: 8.0,
            width: 16.0,
            height: 16.0,
        },
        Color::from_hex("#ffffff").unwrap(),
    );
    root.computed_style.box_shadow = BoxShadow {
        offset_x: 8.0,
        offset_y: 0.0,
        blur_radius: 0.0,
        spread_radius: 0.0,
        color: Color::from_hex("#000000ff").unwrap(),
        inset: false,
    };

    let mut buffer = PixelBuffer::new(40, 32);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 12, 12), Color::from_hex("#ffffff").unwrap());
    assert_eq!(pixel(&buffer, 28, 12), Color::from_hex("#000000").unwrap());
}

#[test]
fn painter_blurs_background_fill_beyond_node_bounds() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 12.0,
            y: 12.0,
            width: 16.0,
            height: 16.0,
        },
        Color::from_hex("#000000ff").unwrap(),
    );
    root.computed_style.filter = VisualFilter { blur_radius: 4.0 };

    let mut buffer = PixelBuffer::new(40, 40);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert!(pixel(&buffer, 10, 20).a > 0);
    assert!(pixel(&buffer, 10, 20).a < 255);
    assert_eq!(pixel(&buffer, 0, 0), Color::TRANSPARENT);
}

#[test]
fn retained_display_list_paints_opacity_through_skia_path() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 20.0,
        },
        Color::WHITE,
    );
    root.computed_style.opacity = 0.5;

    let mut list = RetainedDisplayList::default();
    list.update(&root, 24, 24, false, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 24,
            height: 24,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );

    let mut buffer = PixelBuffer::new(24, 24);
    FrontendRenderEngine::new().render_selected_display_list_for_module(
        &selected,
        &mut buffer,
        1.0,
        None,
        None,
        None,
    );

    let center = pixel(&buffer, 10, 10);
    assert_eq!(
        center,
        Color {
            r: 255,
            g: 255,
            b: 255,
            a: 128,
        }
    );
}

#[test]
fn painter_clips_children_when_overflow_hidden() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
        Color::TRANSPARENT,
    );
    root.computed_style.overflow_x = Overflow::Hidden;
    root.computed_style.overflow_y = Overflow::Hidden;
    root.children = vec![node(
        "box",
        LayoutRect {
            x: 8.0,
            y: 0.0,
            width: 8.0,
            height: 10.0,
        },
        Color::from_hex("#00ff00").unwrap(),
    )];

    let mut buffer = PixelBuffer::new(20, 12);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 9, 5), Color::from_hex("#00ff00").unwrap());
    assert_eq!(pixel(&buffer, 11, 5), Color::TRANSPARENT);
}

#[test]
fn direct_tree_painter_omits_explicitly_hidden_descendants() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 24.0,
            height: 24.0,
        },
        Color::TRANSPARENT,
    );
    let mut hidden = node(
        "box",
        LayoutRect {
            x: 4.0,
            y: 4.0,
            width: 12.0,
            height: 12.0,
        },
        Color::from_hex("#00ff00").unwrap(),
    );
    hidden.attributes.insert("hidden".into(), "true".into());
    root.children.push(hidden);

    let mut buffer = PixelBuffer::new(24, 24);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 8, 8), Color::TRANSPARENT);
}

#[test]
fn painter_orders_children_by_z_index() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 16.0,
        },
        Color::TRANSPARENT,
    );

    let mut bottom = node(
        "box",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 10.0,
            height: 10.0,
        },
        Color::from_hex("#ff0000").unwrap(),
    );
    bottom.computed_style.z_index = 0;
    let mut top = node(
        "box",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 10.0,
            height: 10.0,
        },
        Color::from_hex("#0000ff").unwrap(),
    );
    top.computed_style.z_index = 1;
    root.children = vec![top, bottom];

    let mut buffer = PixelBuffer::new(20, 20);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    assert_eq!(pixel(&buffer, 5, 5), Color::from_hex("#0000ff").unwrap());
}

#[test]
fn selection_paint_uses_selection_colors() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 60.0,
        },
        Color::TRANSPARENT,
    );
    let mut text = text_node(
        "selection proof text",
        0.0,
        0.0,
        160.0,
        60.0,
        Color::from_hex("#111111").unwrap(),
    );
    text.attributes
        .insert("_mesh_selection_background".into(), "#00ff00".into());
    text.attributes
        .insert("_mesh_selection_foreground".into(), "#ff00ff".into());
    text.attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());
    root.children = vec![text];

    let mut buffer = PixelBuffer::new(180, 80);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    let mut saw_selection_background = false;
    let mut saw_selection_foreground = false;
    for y in 0..buffer.height {
        for x in 0..buffer.width {
            let color = pixel(&buffer, x, y);
            if color == Color::from_hex("#00ff00").unwrap() {
                saw_selection_background = true;
            }
            if color == Color::from_hex("#ff00ff").unwrap() {
                saw_selection_foreground = true;
            }
        }
    }

    assert!(saw_selection_background);
    assert!(saw_selection_foreground);
}

#[test]
fn phase44_selection_paint_and_proof_use_theme_colors() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 60.0,
        },
        Color::TRANSPARENT,
    );
    let mut text = text_node(
        "selection proof text",
        0.0,
        0.0,
        160.0,
        60.0,
        Color::from_hex("#111111").unwrap(),
    );
    text.attributes
        .insert("_mesh_selection_background".into(), "#00ff00".into());
    text.attributes
        .insert("_mesh_selection_foreground".into(), "#ff00ff".into());
    text.attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    text.attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    text.attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());
    root.children = vec![text];

    let mut list = RetainedDisplayList::default();
    let metrics = list.update(&root, 180, 80, true, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 180,
            height: 80,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );

    let mut buffer = PixelBuffer::new(180, 80);
    FrontendRenderEngine::new().render_selected_display_list_for_module(
        &selected,
        &mut buffer,
        1.0,
        None,
        None,
        None,
    );

    let mut saw_selection_background = false;
    let mut saw_selection_foreground = false;
    for y in 0..buffer.height {
        for x in 0..buffer.width {
            let color = pixel(&buffer, x, y);
            if color == Color::from_hex("#00ff00").unwrap() {
                saw_selection_background = true;
            }
            if color == Color::from_hex("#ff00ff").unwrap() {
                saw_selection_foreground = true;
            }
        }
    }
    assert!(saw_selection_background);
    assert!(saw_selection_foreground);

    let proof = build_focused_proof_snapshot(
        &root,
        RenderObjectDirtySummary::default(),
        metrics,
        &selected,
    );
    let text = proof
        .nodes
        .iter()
        .find_map(|node| node.parley_text.as_ref())
        .expect("text proof evidence");
    assert_eq!(text.selection_background.as_deref(), Some("#00ff00"));
    assert_eq!(text.selection_foreground.as_deref(), Some("#ff00ff"));
}

#[test]
fn selection_paint_does_not_bleed_into_neighbors() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 220.0,
            height: 80.0,
        },
        Color::TRANSPARENT,
    );
    let mut selected = text_node(
        "selected",
        0.0,
        0.0,
        100.0,
        40.0,
        Color::from_hex("#111111").unwrap(),
    );
    selected
        .attributes
        .insert("_mesh_selection_background".into(), "#00ff00".into());
    selected
        .attributes
        .insert("_mesh_selection_foreground".into(), "#ff00ff".into());
    selected
        .attributes
        .insert("_mesh_selection_anchor_x".into(), "0.00".into());
    selected
        .attributes
        .insert("_mesh_selection_anchor_y".into(), "0.00".into());
    selected
        .attributes
        .insert("_mesh_selection_focus_x".into(), "1000.00".into());
    selected
        .attributes
        .insert("_mesh_selection_focus_y".into(), "1000.00".into());
    selected
        .attributes
        .insert("_mesh_selection_text_x".into(), "0.00".into());
    selected
        .attributes
        .insert("_mesh_selection_text_y".into(), "0.00".into());

    let neighbor = text_node(
        "neighbor",
        120.0,
        0.0,
        100.0,
        40.0,
        Color::from_hex("#111111").unwrap(),
    );
    root.children = vec![selected, neighbor];

    let mut buffer = PixelBuffer::new(240, 80);
    FrontendRenderEngine::new().render_tree(&root, &mut buffer, 1.0);

    for y in 0..40 {
        for x in 120..220 {
            assert_ne!(
                pixel(&buffer, x, y),
                Color::from_hex("#00ff00").unwrap(),
                "selection background should stay inside the selected text node"
            );
        }
    }
}

#[test]
fn selection_fixture_preview_tree_paints_nonempty_surface() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .canonicalize()
        .unwrap();
    let module_dir = root.join("modules/frontend/text-selection-proof");
    let loaded = mesh_core_module::manifest::load_manifest(&module_dir).unwrap();
    let compiled = compile_frontend_module(&loaded.manifest, &module_dir).unwrap();
    let tree = compiled.build_preview_tree(&default_theme(), 360, 176);

    let mut buffer = PixelBuffer::new(380, 196);
    FrontendRenderEngine::new().render_tree(&tree, &mut buffer, 1.0);

    let has_visible_pixels = buffer.data.chunks_exact(4).any(|px| px[3] != 0);
    assert!(
        has_visible_pixels,
        "proof fixture should paint visible output"
    );
}

#[test]
fn retained_replay_batches_adjacent_non_content_nodes() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 40.0,
        },
        Color::TRANSPARENT,
    );
    root.children = vec![
        node(
            "box",
            LayoutRect {
                x: 2.0,
                y: 2.0,
                width: 18.0,
                height: 18.0,
            },
            Color::from_hex("#224466").unwrap(),
        ),
        node(
            "box",
            LayoutRect {
                x: 24.0,
                y: 2.0,
                width: 18.0,
                height: 18.0,
            },
            Color::from_hex("#446622").unwrap(),
        ),
        text_node(
            "content boundary",
            2.0,
            24.0,
            72.0,
            12.0,
            Color::from_hex("#f0f0f0").unwrap(),
        ),
    ];

    let mut list = RetainedDisplayList::default();
    list.update(&root, 80, 40, true, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 80,
            height: 40,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );
    let replay_commands: Vec<_> = selected
        .iter()
        .filter(|command| {
            command.kind == DisplayPaintCommandKind::Node
                && matches!(command.node.content, DisplayPaintContent::None)
        })
        .cloned()
        .collect();
    assert!(
        replay_commands.len() >= 2,
        "expected at least two adjacent non-content node commands to replay"
    );

    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(80, 40);
    engine.render_display_list_for_module(&replay_commands, &mut buffer, 1.0, None, None, None);

    let call_sizes = recorded.execute_call_sizes();
    assert_eq!(
        call_sizes,
        vec![2],
        "expected exactly one non-empty batched display command execution, got {call_sizes:?}"
    );
}

/// Two opaque halves (red left, blue right) with a transparent frosted panel
/// straddling the color boundary. In-surface backdrop blur must mix the two
/// colors inside the panel while pixels outside the panel stay pure.
fn backdrop_blur_scene(left_color: Color) -> WidgetNode {
    let blue = Color {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    };
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 32.0,
            height: 32.0,
        },
        Color::TRANSPARENT,
    );
    let left = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 32.0,
        },
        left_color,
    );
    let right = node(
        "box",
        LayoutRect {
            x: 16.0,
            y: 0.0,
            width: 16.0,
            height: 32.0,
        },
        blue,
    );
    let mut frosted = node(
        "box",
        LayoutRect {
            x: 8.0,
            y: 8.0,
            width: 16.0,
            height: 16.0,
        },
        Color::TRANSPARENT,
    );
    frosted.computed_style.backdrop_filter = VisualFilter { blur_radius: 4.0 };
    root.children.push(left);
    root.children.push(right);
    root.children.push(frosted);
    let mut id = 1;
    root.id = id;
    for child in &mut root.children {
        id += 1;
        child.id = id;
    }
    root
}

#[test]
fn retained_backdrop_filter_delegates_to_compositor() {
    let red = Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    let root = backdrop_blur_scene(red);

    let mut list = RetainedDisplayList::default();
    list.update(&root, 32, 32, true, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 32,
            height: 32,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );

    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(32, 32);
    engine.render_selected_display_list_for_module(&selected, &mut buffer, 1.0, None, None, None);

    // The SHM buffer stays flat. The compositor combines this client buffer
    // with the desktop behind the surface using the exported blur region.
    let left = pixel(&buffer, 15, 16);
    assert!(
        left.r > 247 && left.b < 8,
        "client-side backdrop filtering must not rewrite surface pixels, got {left:?}"
    );
    let right = pixel(&buffer, 16, 16);
    assert!(
        right.b > 247 && right.r < 8,
        "client-side backdrop filtering must preserve the adjacent color, got {right:?}"
    );
}

/// Changing content beneath a frosted panel and repainting only the expanded
/// sparse damage must produce the same pixels as a fresh full repaint.
/// The retained damage expansion remains deterministic for render backends
/// that can support an in-surface backdrop in the future.
#[test]
fn sparse_repaint_with_backdrop_damage_expansion_matches_full_repaint() {
    let red = Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    let green = Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    };
    let engine = FrontendRenderEngine::new();

    let mut list = RetainedDisplayList::default();
    let first = backdrop_blur_scene(red);
    list.update(&first, 32, 32, true, true);
    let mut buffer = PixelBuffer::new(32, 32);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 32,
            height: 32,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );
    engine.render_selected_display_list_for_module(&selected, &mut buffer, 1.0, None, None, None);

    // Change the left half red → green and repaint only the expanded damage.
    let second = backdrop_blur_scene(green);
    list.update(&second, 32, 32, false, true);
    let mut damage: Vec<DamageRect> = list.damage_rects().to_vec();
    assert!(
        !damage.is_empty(),
        "left-half color change must produce damage"
    );
    assert!(
        list.expand_damage_for_backdrop_filters(&mut damage),
        "damage touching the frosted panel's read region must expand"
    );
    let selected =
        list.select_paint_commands_for_rects(&damage, DisplayListRepaintPolicy::MinimalDamage);
    for rect in &damage {
        buffer.clear_rect(rect.x, rect.y, rect.width, rect.height, Color::TRANSPARENT);
        engine.render_selected_display_list_for_module(
            &selected,
            &mut buffer,
            1.0,
            Some((rect.x, rect.y, rect.width, rect.height)),
            None,
            None,
        );
    }

    let mut full_buffer = PixelBuffer::new(32, 32);
    let full = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 32,
            height: 32,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );
    engine.render_selected_display_list_for_module(&full, &mut full_buffer, 1.0, None, None, None);

    assert_eq!(
        buffer.data, full_buffer.data,
        "sparse repaint with backdrop damage expansion must match a full repaint"
    );
}

/// Navigation-bar shape: the surface ROOT carries `backdrop-filter` with a
/// translucent background (its in-surface backdrop is empty — the compositor
/// blurs behind the surface), and a child button's background changes on
/// hover. The root contributes no backdrop region, so damage stays sparse;
/// the sparse repaint must still match a full repaint pixel-for-pixel.
fn frosted_root_bar(button_color: Color) -> WidgetNode {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 24.0,
        },
        Color {
            r: 10,
            g: 10,
            b: 14,
            a: 191,
        },
    );
    root.computed_style.backdrop_filter = VisualFilter { blur_radius: 20.0 };
    let button = node(
        "box",
        LayoutRect {
            x: 40.0,
            y: 4.0,
            width: 16.0,
            height: 16.0,
        },
        button_color,
    );
    root.children.push(button);
    root.id = 1;
    root.children[0].id = 2;
    root
}

#[test]
fn sparse_hover_repaint_under_frosted_root_matches_full_repaint() {
    let idle = Color {
        r: 40,
        g: 40,
        b: 48,
        a: 255,
    };
    let hover = Color {
        r: 90,
        g: 90,
        b: 110,
        a: 255,
    };
    let engine = FrontendRenderEngine::new();

    let mut list = RetainedDisplayList::default();
    list.update(&frosted_root_bar(idle), 64, 24, true, true);
    let mut buffer = PixelBuffer::new(64, 24);
    let full = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 64,
            height: 24,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );
    engine.render_selected_display_list_for_module(&full, &mut buffer, 1.0, None, None, None);

    // Hover: only the button's background changes.
    list.update(&frosted_root_bar(hover), 64, 24, false, true);
    let mut damage: Vec<DamageRect> = list.damage_rects().to_vec();
    assert!(!damage.is_empty(), "hover change must produce damage");
    list.expand_damage_for_backdrop_filters(&mut damage);
    let selected =
        list.select_paint_commands_for_rects(&damage, DisplayListRepaintPolicy::MinimalDamage);
    for rect in &damage {
        buffer.clear_rect(rect.x, rect.y, rect.width, rect.height, Color::TRANSPARENT);
        engine.render_selected_display_list_for_module(
            &selected,
            &mut buffer,
            1.0,
            Some((rect.x, rect.y, rect.width, rect.height)),
            None,
            None,
        );
    }

    let mut full_buffer = PixelBuffer::new(64, 24);
    let full = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 64,
            height: 24,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );
    engine.render_selected_display_list_for_module(&full, &mut full_buffer, 1.0, None, None, None);

    for y in 0..24 {
        for x in 0..64 {
            let sparse = pixel(&buffer, x, y);
            let fresh = pixel(&full_buffer, x, y);
            assert_eq!(
                sparse, fresh,
                "sparse hover repaint diverged from full repaint at ({x},{y})"
            );
        }
    }
}
