use super::*;
use crate::display_list::{DamageRect, DisplayListRepaintPolicy, RetainedDisplayList};
use crate::{RenderObjectDirtySummary, build_focused_proof_snapshot};
use mesh_core_elements::layout::LayoutRect;
use mesh_core_elements::style::{Dimension, Edges};
use mesh_core_frontend::compile_frontend_module;
use mesh_core_theme::default_theme;
use std::path::PathBuf;

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
fn painter_backend_capabilities_identify_skia_and_unsupported_commands_diagnose() {
    let backend = SkiaPaintBackend;
    let capabilities = backend.capabilities();
    assert_eq!(capabilities.backend_id, "skia");
    assert!(capabilities.rects);
    assert!(capabilities.rounded_rects);
    assert!(capabilities.shadows);
    assert!(capabilities.filters);
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
}

#[test]
fn painter_command_contract_keeps_retained_structures_free_of_skia_types() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for relative in [
        "src/display_list.rs",
        "src/render_object.rs",
    ] {
        let contents = std::fs::read_to_string(manifest_dir.join(relative)).unwrap();
        assert!(
            !contents.contains("skia_safe"),
            "{relative} must stay backend-neutral"
        );
    }
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
