use super::super::*;
use super::common::*;
use crate::display_list::{
    DamageRect, DisplayIconPaint, DisplayListRepaintPolicy, DisplayPaintCommandKind,
    DisplayPaintContent, RetainedDisplayList,
};
use mesh_core_elements::Prop;
use mesh_core_elements::layout::LayoutRect;
use mesh_core_elements::style::{BackgroundPaint, Edges, StyleImageSource, StyleLinearGradient};

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
        PainterCommand::PushClip(PainterClip {
            rect,
            radii: mesh_core_elements::style::Corners::all(2.0),
            shapes: Default::default(),
            mode: crate::surface::painter::PainterClipMode::Inside,
        }),
        PainterCommand::PopClip,
        PainterCommand::PushLayer(PainterLayer::isolated(clip, 0.5, PainterBlendMode::SrcOver)),
        PainterCommand::PopLayer,
        PainterCommand::DrawRect { rect, paint, clip },
        PainterCommand::DrawRoundedRect {
            rect,
            radii: mesh_core_elements::style::Corners::all(4.0),
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
                intermediate: Vec::new(),
                from: Color::BLACK,
                to: Color::WHITE,
            },
            rect,
            radii: mesh_core_elements::style::Corners::all(4.0),
            clip,
        },
        PainterCommand::DrawShadow {
            rect,
            radii: mesh_core_elements::style::Corners::all(4.0),
            shadow: BoxShadow::default(),
            clip,
        },
        PainterCommand::ApplyFilter {
            rect,
            radii: mesh_core_elements::style::Corners::all(4.0),
            filter: PainterFilter::Backdrop(VisualFilter::blur(2.0)),
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
fn compatibility_and_retained_box_paths_emit_same_command_classes() {
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
        vec!["draw_rect", "draw_border"],
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
        vec!["draw_rect", "draw_border"]
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
    root.computed_style.backdrop_filter = VisualFilter::blur(3.0);
    root.computed_style.filter = VisualFilter::blur(2.0);

    let backend = RecordingPaintBackend::default();
    let recorded = backend.clone();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend));
    let mut buffer = PixelBuffer::new(32, 32);
    engine.render_tree(&root, &mut buffer, 1.0);

    // Backdrop-filter is compositor metadata and emits no CPU command. The
    // authoritative display-list builder wraps CSS filter content in a layer.
    let classes = painter_command_classes(&recorded.recorded_commands());
    assert_eq!(
        classes,
        vec![
            "push_layer",
            "draw_shadow",
            "draw_rounded_rect",
            "pop_layer"
        ]
    );
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
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default()).background =
        "#00ff00".into();
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default()).foreground =
        "#ff00ff".into();
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .anchor
        .0 = 0.00;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .anchor
        .1 = 0.00;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .focus
        .0 = 1000.00;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .focus
        .1 = 1000.00;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .text_origin
        .0 = 0.00;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .text_origin
        .1 = 0.00;
    root.children = vec![text].into();

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
    )]
    .into();

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
fn compatibility_and_retained_input_paths_emit_same_classes() {
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
    root.attributes.insert(Prop::Value, "mesh".into());
    root.state.insert(mesh_core_elements::NodeState::FOCUSED);
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
fn compatibility_and_retained_control_paths_emit_same_classes() {
    let mut root = node(
        "checkbox",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 128.0,
            height: 32.0,
        },
        Color::TRANSPARENT,
    );
    root.attributes.insert(Prop::Checked, "true".into());
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

    assert_eq!(direct_classes, vec!["draw_path"]);
    assert_eq!(retained_classes, direct_classes);
}

#[test]
fn compatibility_and_retained_image_paths_emit_same_command_classes() {
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
fn compatibility_and_retained_gradient_paths_emit_same_command_classes() {
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
fn compatibility_and_retained_icon_paths_preserve_image_like_boundary() {
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
    root.attributes.insert(Prop::Name, "mesh:search".into());
    root.attributes.insert(Prop::Size, "20".into());
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
        root.attributes.get(Prop::Name).map(String::as_str),
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
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default()).background =
        "#3366ff".into();
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default()).foreground =
        "#ffffff".into();
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .anchor
        .0 = 0.00;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .anchor
        .1 = 0.00;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .focus
        .0 = 1000.00;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .focus
        .1 = 1000.00;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .text_origin
        .0 = 0.00;
    std::sync::Arc::make_mut(text.text_selection.get_or_insert_default())
        .text_origin
        .1 = 0.00;

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
    input.attributes.insert(Prop::Value, "mesh".into());
    input.state.insert(mesh_core_elements::NodeState::FOCUSED);
    input.computed_style.color = Color::from_hex("#f5f5f5").unwrap();
    input.computed_style.padding = Edges::all(4.0);

    // A control part paints as an ordinary rounded box.
    let mut thumb = node(
        "box",
        LayoutRect {
            x: 8.0,
            y: 76.0,
            width: 16.0,
            height: 16.0,
        },
        Color::from_hex("#4a90e2").unwrap(),
    );
    thumb.set_control_part(mesh_core_elements::ControlPart::SliderThumb);
    thumb.computed_style.border_radius = mesh_core_elements::style::Corners::all(8.0);

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
    icon.attributes.insert(Prop::Name, "mesh:search".into());
    icon.attributes.insert(Prop::Size, "20".into());

    let expected_node_order = vec![root.id, text.id, input.id, thumb.id, icon.id];
    root.children = vec![text, input, thumb, icon].into();

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
            >= 4,
        "box, text selection and input primitives should emit draw_rect commands"
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
