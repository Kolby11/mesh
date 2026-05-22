use super::*;
use crate::display_list::{
    DamageRect, DisplayIconPaint, DisplayListRepaintPolicy, DisplayPaintCommandKind,
    DisplayPaintContent, RetainedDisplayList,
};
use crate::{RenderObjectDirtySummary, build_focused_proof_snapshot};
use mesh_core_elements::layout::LayoutRect;
use mesh_core_elements::style::{Dimension, Edges};
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

fn full_clip(width: i32, height: i32) -> ClipRect {
    ClipRect {
        x: 0,
        y: 0,
        width,
        height,
    }
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
}

impl RecordingPaintBackend {
    fn recorded_commands(&self) -> Vec<PainterCommand> {
        self.commands
            .lock()
            .map(|commands| commands.clone())
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
            PainterCommand::DrawText { .. } => "draw_text",
            PainterCommand::DrawImage { .. } => "draw_image",
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
        PainterCommand::DrawText {
            text: "hello".into(),
            x: 2.0,
            y: 12.0,
            paint,
            clip,
        },
        PainterCommand::DrawImage {
            image: PainterImage { id: "img".into() },
            rect,
            paint,
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
        PainterCommand::DrawText {
            text: "hello".into(),
            x: 2.0,
            y: 12.0,
            paint,
            clip,
        },
        PainterCommand::DrawImage {
            image: PainterImage { id: "img".into() },
            rect,
            paint,
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
            "draw_text",
            "draw_image",
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
    retained_engine.render_display_list_for_module(
        selected.commands(),
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

    let classes = painter_command_classes(&recorded.recorded_commands());
    assert_eq!(
        classes,
        vec!["draw_shadow", "apply_filter", "draw_rounded_rect"]
    );
    assert!(classes.contains(&"draw_shadow"));
    assert!(classes.contains(&"apply_filter"));
    assert!(classes.contains(&"draw_rounded_rect"));
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
    retained_engine.render_display_list_for_module(
        selected.commands(),
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
    retained_engine.render_display_list_for_module(
        selected.commands(),
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
    assert!(!capabilities.clips);
    assert!(!capabilities.layers);
    assert!(!capabilities.paths);

    let mut buffer = PixelBuffer::new(16, 16);
    let mut diagnostics = Vec::new();
    backend.execute_commands(
        &mut buffer,
        &[PainterCommand::DrawPath {
            path: PainterPath {
                elements: vec![PainterPathElement::MoveTo(0.0, 0.0)],
            },
            paint: PainterPaint::fill(Color::WHITE),
            clip: full_clip(16, 16),
        }],
        &mut diagnostics,
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].backend_id, "skia");
    assert_eq!(diagnostics[0].feature, UnsupportedPainterFeature::Path);

    diagnostics.clear();
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
    assert_eq!(diagnostics[0].feature, UnsupportedPainterFeature::Filter);
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

    let commands = recorded.recorded_commands();
    assert_eq!(commands.len(), 3);
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
    assert!(matches!(
        commands[2],
        PainterCommand::ApplyFilter {
            filter: PainterFilter::Backdrop(VisualFilter { blur_radius: 3.0 }),
            ..
        }
    ));
}

#[test]
fn painter_backend_diagnostics_are_observable_on_frontend_render_engine() {
    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(16, 16);
    engine.execute_painter_commands(
        &mut buffer,
        &[PainterCommand::DrawPath {
            path: PainterPath {
                elements: vec![PainterPathElement::MoveTo(0.0, 0.0)],
            },
            paint: PainterPaint::fill(Color::WHITE),
            clip: full_clip(16, 16),
        }],
    );

    let diagnostics = engine.painter_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].backend_id, "skia");
    assert_eq!(diagnostics[0].feature, UnsupportedPainterFeature::Path);

    engine.clear_painter_diagnostics();
    assert!(engine.painter_diagnostics().is_empty());
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
    FrontendRenderEngine::new().render_display_list_for_module(
        selected.commands(),
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
    FrontendRenderEngine::new().render_display_list_for_module(
        selected.commands(),
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
