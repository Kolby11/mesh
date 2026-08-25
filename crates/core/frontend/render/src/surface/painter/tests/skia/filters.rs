use super::super::super::*;
use super::super::common::*;
use crate::display_list::{
    BackdropBlurPolicy, DamageRect, DisplayListRepaintPolicy, RetainedDisplayList,
};
use mesh_core_elements::layout::LayoutRect;

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

#[test]
fn in_surface_backdrop_filter_uses_the_validated_renderer_fallback() {
    let red = Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    let root = backdrop_blur_scene(red);

    let mut list = RetainedDisplayList::default();
    list.set_backdrop_blur_policy(BackdropBlurPolicy::InSurfaceFilter);
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

    let outside = pixel(&buffer, 4, 16);
    assert!(
        outside.r > 247 && outside.b < 8,
        "pixels outside the fallback region stay unfiltered, got {outside:?}"
    );
    let inside = pixel(&buffer, 12, 16);
    assert!(
        inside.r > 0 && inside.b > 0 && inside.r < 255 && inside.b < 255,
        "in-surface fallback should mix the painted backdrop, got {inside:?}"
    );
    assert!(engine.painter_diagnostics().is_empty());
}

#[test]
fn unsupported_in_surface_backdrop_filter_is_rejected_with_diagnostic() {
    let root = backdrop_blur_scene(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    let mut list = RetainedDisplayList::default();
    list.set_backdrop_blur_policy(BackdropBlurPolicy::InSurfaceFilter);
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

    let backend = RecordingPaintBackend::default();
    let engine = FrontendRenderEngine::with_paint_backend(Box::new(backend.clone()));
    let mut buffer = PixelBuffer::new(32, 32);
    engine.render_selected_display_list_for_module(&selected, &mut buffer, 1.0, None, None, None);

    assert!(engine.painter_diagnostics().iter().any(|diagnostic| {
        diagnostic.feature == UnsupportedPainterFeature::BackdropBlur
            && diagnostic.source.as_ref().and_then(|source| source.node_id) == Some(4)
    }));
    assert!(
        engine
            .paint_backend_snapshot()
            .capabilities
            .iter()
            .any(|capability| capability.feature == "backdrop_blur" && !capability.supported)
    );
    assert!(
        backend
            .recorded_commands()
            .iter()
            .all(|command| !matches!(command, PainterCommand::ApplyFilter { .. }))
    );
}

#[test]
fn rejected_backdrop_filter_is_flat_and_diagnostic() {
    let root = backdrop_blur_scene(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    let mut list = RetainedDisplayList::default();
    list.set_backdrop_blur_policy(BackdropBlurPolicy::Rejected);
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

    let inside = pixel(&buffer, 12, 16);
    assert_eq!(inside.r, 255);
    assert_eq!(inside.g, 0);
    assert_eq!(inside.b, 0);
    assert!(engine.painter_diagnostics().iter().any(|diagnostic| {
        diagnostic.feature == UnsupportedPainterFeature::BackdropBlur
            && diagnostic.message.contains("rejected")
    }));
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
        list.expand_damage_for_blur_regions(&mut damage),
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
        buffer.data(),
        full_buffer.data(),
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
    list.expand_damage_for_blur_regions(&mut damage);
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

/// A red parent with `filter: blur()` and an opaque blue child inside it.
fn blurred_parent_with_child(blur_radius: f32) -> WidgetNode {
    let mut parent = node(
        "box",
        LayoutRect {
            x: 20.0,
            y: 20.0,
            width: 40.0,
            height: 40.0,
        },
        Color::from_hex("#ff0000").unwrap(),
    );
    parent.computed_style.filter = VisualFilter { blur_radius };
    parent.children = vec![node(
        "box",
        LayoutRect {
            x: 30.0,
            y: 30.0,
            width: 20.0,
            height: 20.0,
        },
        Color::from_hex("#0000ff").unwrap(),
    )]
    .into();
    parent
}

fn painted_display_list(root: &WidgetNode, size: u32) -> PixelBuffer {
    let mut list = RetainedDisplayList::default();
    list.update(root, size, size, true, true);
    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(size, size);
    engine.render_display_list_for_module(
        list.paint_commands(),
        &mut buffer,
        1.0,
        None,
        None,
        None,
    );
    buffer
}

#[test]
fn filter_blur_softens_descendants_not_only_the_filtered_element() {
    let sharp = painted_display_list(&blurred_parent_with_child(0.0), 80);
    let blurred = painted_display_list(&blurred_parent_with_child(6.0), 80);

    // Inside the child, away from every edge, the sharp render is pure blue.
    let sharp_center = sharp.get_pixel(40, 40);
    assert_eq!(
        (sharp_center.r, sharp_center.g, sharp_center.b),
        (0, 0, 255)
    );

    // The child's own edge must soften: a subtree blur mixes the child's blue
    // with its parent's red across the boundary, which a blur applied only to
    // the filtered element's own shape cannot do.
    let sharp_edge = sharp.get_pixel(29, 40);
    let blurred_edge = blurred.get_pixel(29, 40);
    assert_eq!(
        (sharp_edge.r, sharp_edge.b),
        (255, 0),
        "without a filter the pixel left of the child is the parent's flat red"
    );
    assert!(
        blurred_edge.b > 20 && blurred_edge.r > 20,
        "blurred child edge must mix child blue into parent red, got {blurred_edge:?}"
    );
}

#[test]
fn filter_blur_spills_past_the_filtered_element_box() {
    let sharp = painted_display_list(&blurred_parent_with_child(6.0), 80);
    // Just outside the parent's 20..60 box: unfiltered content stops dead at
    // the edge, a blurred layer fades out past it.
    let outside = sharp.get_pixel(17, 40);
    assert!(
        outside.a > 0,
        "blur must spill outside the element box, got {outside:?}"
    );
}

#[test]
fn blur_quality_passes_are_configurable_and_clamped() {
    assert_eq!(BlurQuality::default().resolved_passes(), 1);
    assert_eq!(
        BlurQuality {
            passes: 2,
            max_radius: 96.0,
        }
        .resolved_passes(),
        2
    );
    // An out-of-range setting clamps instead of disabling the blur.
    assert_eq!(
        BlurQuality {
            passes: 9,
            max_radius: 96.0,
        }
        .resolved_passes(),
        MAX_BLUR_PASSES
    );
}

#[test]
fn configured_blur_quality_still_blurs_the_subtree() {
    let root = blurred_parent_with_child(6.0);
    let mut list = RetainedDisplayList::default();
    list.update(&root, 80, 80, true, true);

    let mut painted = Vec::new();
    for quality in [
        BlurQuality {
            passes: 1,
            max_radius: 96.0,
        },
        BlurQuality {
            passes: 3,
            max_radius: 96.0,
        },
    ] {
        let engine = FrontendRenderEngine::new();
        engine.set_blur_quality(quality);
        let mut buffer = PixelBuffer::new(80, 80);
        engine.render_display_list_for_module(
            list.paint_commands(),
            &mut buffer,
            1.0,
            None,
            None,
            None,
        );
        painted.push(buffer);
    }

    // Both qualities blur the same subtree by the same requested radius, so
    // the softened child edge stays comparable — the setting buys frame time,
    // not a different look.
    for buffer in &painted {
        let edge = buffer.get_pixel(29, 40);
        assert!(
            edge.b > 10 && edge.r > 10,
            "every quality must still blur the child edge, got {edge:?}"
        );
    }
}

#[test]
fn blur_layers_deeper_than_the_cap_paint_unblurred_with_a_diagnostic() {
    let mut commands = Vec::new();
    for _ in 0..(MAX_BLUR_LAYER_DEPTH + 1) {
        commands.push(PainterCommand::PushLayer(PainterLayer::blurred(
            full_clip(32, 32),
            VisualFilter { blur_radius: 4.0 },
            BlurQuality::default(),
        )));
    }
    commands.push(PainterCommand::DrawRect {
        rect: full_clip(8, 8),
        paint: PainterPaint::fill(Color::WHITE),
        clip: full_clip(32, 32),
    });
    for _ in 0..(MAX_BLUR_LAYER_DEPTH + 1) {
        commands.push(PainterCommand::PopLayer);
    }

    let mut buffer = PixelBuffer::new(32, 32);
    let mut diagnostics = Vec::new();
    SkiaPaintBackend.execute_commands(&mut buffer, &commands, &mut diagnostics);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("nested deeper than")),
        "a runaway nest must be reported, got {diagnostics:?}"
    );
    assert!(
        buffer.get_pixel(4, 4).a > 0,
        "the capped nest must still paint its content"
    );
}

// cargo test -p mesh-core-render --release -- blur_layer_pass_cost --ignored --nocapture
#[test]
#[ignore = "release-only blur quality microbenchmark"]
fn blur_layer_pass_cost() {
    let iterations = 200;
    let size = 512;
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: size as f32,
            height: size as f32,
        },
        Color::from_hex("#202020").unwrap(),
    );
    let mut blurred = node(
        "box",
        LayoutRect {
            x: 40.0,
            y: 40.0,
            width: 420.0,
            height: 420.0,
        },
        Color::from_hex("#ff0000").unwrap(),
    );
    blurred.computed_style.filter = VisualFilter { blur_radius: 32.0 };
    blurred.children = (0..16)
        .map(|index| {
            node(
                "box",
                LayoutRect {
                    x: 60.0 + (index % 4) as f32 * 100.0,
                    y: 60.0 + (index / 4) as f32 * 100.0,
                    width: 80.0,
                    height: 80.0,
                },
                Color::from_hex("#0000ff").unwrap(),
            )
        })
        .collect::<Vec<_>>()
        .into();
    root.children = vec![blurred].into();

    let measure = |radius: f32, quality: BlurQuality| {
        let mut root = root.clone();
        let mut children = root.children.to_vec();
        children[0].computed_style.filter = VisualFilter {
            blur_radius: radius,
        };
        root.children = children.into();
        let mut list = RetainedDisplayList::default();
        list.update(&root, size, size, true, true);

        let engine = FrontendRenderEngine::new();
        engine.set_blur_quality(quality);
        let mut buffer = PixelBuffer::new(size, size);
        // Warm the first-frame allocations out of the measurement.
        engine.render_display_list_for_module(
            list.paint_commands(),
            &mut buffer,
            1.0,
            None,
            None,
            None,
        );
        let started = std::time::Instant::now();
        for _ in 0..iterations {
            engine.render_display_list_for_module(
                std::hint::black_box(list.paint_commands()),
                &mut buffer,
                1.0,
                None,
                None,
                None,
            );
        }
        started.elapsed()
    };

    let unfiltered = measure(
        0.0,
        BlurQuality {
            passes: 1,
            max_radius: 96.0,
        },
    );
    eprintln!("same tree with no filter, {iterations} frames: {unfiltered:?}");
    for radius in [8.0_f32, 32.0, 64.0] {
        let single = measure(
            radius,
            BlurQuality {
                passes: 1,
                max_radius: 96.0,
            },
        );
        let double = measure(
            radius,
            BlurQuality {
                passes: 2,
                max_radius: 96.0,
            },
        );
        eprintln!(
            "blur layer {size}x{size} radius {radius}, {iterations} frames: \
             1 pass {single:?}; 2 passes {double:?}; ratio {:.2}x",
            double.as_secs_f64() / single.as_secs_f64()
        );
        assert!(
            single < double,
            "the shipped default must be the cheapest option"
        );
    }
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
