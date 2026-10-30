use super::super::*;
use super::common::*;
use crate::PaintCommandClass;
use crate::display_list::{DamageRect, DisplayListRepaintPolicy, RetainedDisplayList};
use mesh_core_elements::layout::LayoutRect;

#[test]
fn multi_region_selected_paint_matches_repeated_single_region_replay() {
    let (list, clips) = multi_region_fixture();
    let damages: Vec<_> = clips
        .iter()
        .map(|&(x, y, width, height)| DamageRect {
            x,
            y,
            width,
            height,
        })
        .collect();
    let selected =
        list.select_paint_commands_for_rects(&damages, DisplayListRepaintPolicy::MinimalDamage);
    let engine = FrontendRenderEngine::new();
    let mut repeated = PixelBuffer::new(512, 512);
    let mut batched = PixelBuffer::new(512, 512);
    let mut attributed = PixelBuffer::new(512, 512);

    for &clip in &clips {
        engine.render_selected_display_list_for_module(
            &selected,
            &mut repeated,
            1.0,
            Some(clip),
            None,
            None,
        );
    }
    engine.render_selected_display_list_regions_for_module(
        &selected,
        &mut batched,
        1.0,
        &clips,
        None,
        None,
    );
    let attribution = engine.render_selected_display_list_regions_for_module_with_attribution(
        &selected,
        &mut attributed,
        1.0,
        &clips,
        None,
        None,
    );

    assert_eq!(batched.data(), repeated.data());
    assert_eq!(attributed.data(), repeated.data());
    assert!(attribution.get(PaintCommandClass::Primitive).command_count > 0);
}

#[test]
fn overlapping_multi_region_paint_preserves_region_major_replay() {
    let (list, _) = multi_region_fixture();
    let clips = [(0, 0, 48, 32), (24, 0, 48, 32)];
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 72,
            height: 32,
        }),
        DisplayListRepaintPolicy::MinimalDamage,
    );
    let engine = FrontendRenderEngine::new();
    let mut repeated = PixelBuffer::new(512, 512);
    let mut batched = PixelBuffer::new(512, 512);

    for &clip in &clips {
        engine.render_selected_display_list_for_module(
            &selected,
            &mut repeated,
            1.0,
            Some(clip),
            None,
            None,
        );
    }
    engine.render_selected_display_list_regions_for_module(
        &selected,
        &mut batched,
        1.0,
        &clips,
        None,
        None,
    );

    assert_eq!(batched.data(), repeated.data());
}

#[test]
fn attributed_selected_paint_preserves_pixels_and_classifies_commands() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 96.0,
            height: 32.0,
        },
        Color {
            r: 20,
            g: 30,
            b: 40,
            a: 255,
        },
    );
    root.children
        .push(text_node("profile me", 4.0, 4.0, 80.0, 20.0, Color::WHITE));

    let mut list = RetainedDisplayList::default();
    list.update(&root, 96, 32, true, true);
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 96,
            height: 32,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );
    let engine = FrontendRenderEngine::new();
    let mut normal = PixelBuffer::new(96, 32);
    engine.render_selected_display_list_for_module(&selected, &mut normal, 1.0, None, None, None);
    let mut attributed = PixelBuffer::new(96, 32);
    let metrics = engine.render_selected_display_list_for_module_with_attribution(
        &selected,
        &mut attributed,
        1.0,
        None,
        None,
        None,
    );

    assert_eq!(attributed.data(), normal.data());
    assert!(
        metrics.get(PaintCommandClass::Primitive).command_count > 0,
        "the root's batched self paint must be attributed as a primitive"
    );
    assert!(
        metrics.get(PaintCommandClass::Text).command_count > 0,
        "the text node must be attributed as text"
    );
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
fn node_opacity_isolated_from_descendant_primitives() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 16.0,
        },
        Color::WHITE,
    );
    let mut group = node(
        "box",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 12.0,
            height: 12.0,
        },
        Color::from_hex("#ff0000").unwrap(),
    );
    group.computed_style.opacity = 0.5;
    group.children.push(node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 12.0,
            height: 12.0,
        },
        Color::from_hex("#0000ff").unwrap(),
    ));
    root.children.push(group);

    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(16, 16);
    engine.render_tree(&root, &mut buffer, 1.0);

    // The child covers the group's red background before the group is
    // composited. Applying opacity per primitive would leave an opaque blue
    // child; an isolated group produces blue at half opacity over white.
    let color = pixel(&buffer, 8, 8);
    assert!((120..=136).contains(&color.r), "{color:?}");
    assert!((120..=136).contains(&color.g), "{color:?}");
    assert!(color.b > 247, "{color:?}");
}

#[test]
fn node_blend_mode_isolated_from_descendant_primitives() {
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 16.0,
            height: 16.0,
        },
        Color::from_hex("#ff0000").unwrap(),
    );
    let mut group = node(
        "box",
        LayoutRect {
            x: 2.0,
            y: 2.0,
            width: 12.0,
            height: 12.0,
        },
        Color::from_hex("#00ff00").unwrap(),
    );
    group.computed_style.mix_blend_mode = mesh_core_elements::BlendMode::Multiply;
    group.children.push(node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 12.0,
            height: 12.0,
        },
        Color::from_hex("#0000ff").unwrap(),
    ));
    root.children.push(group);

    let engine = FrontendRenderEngine::new();
    let mut buffer = PixelBuffer::new(16, 16);
    engine.render_tree(&root, &mut buffer, 1.0);

    // The group's source is blue after its child paints over its green
    // background. Multiplying that complete group with the red backdrop is
    // black; per-primitive blend mode would leave the child blue.
    let color = pixel(&buffer, 8, 8);
    assert!(color.r < 8 && color.g < 8 && color.b < 8, "{color:?}");
}

#[test]
fn checked_checkbox_indicator_rasterizes_checkmark_glyph() {
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

    let indicator = |checked: bool| {
        let mut indicator = node("box", layout, bg);
        indicator.set_control_part(mesh_core_elements::ControlPart::CheckboxIndicator);
        indicator
            .state
            .set(mesh_core_elements::NodeState::CHECKED, checked);
        indicator.computed_style.color = white;
        indicator
    };
    let checked = indicator(true);
    assert!(
        count_light_pixels(&checked) > 0,
        "a checked checkbox must rasterize a light checkmark over its dark box"
    );

    let unchecked = indicator(false);
    assert_eq!(
        count_light_pixels(&unchecked),
        0,
        "an unchecked checkbox paints no checkmark"
    );
}

#[test]
fn damage_that_overlaps_after_device_rounding_repaints_translucent_content_once() {
    // RND-01: at 1.5x, logical rects ending at 11 and starting at 10 round
    // out to device columns 0..17 and 15..30, which share two columns. The
    // final damage stage makes them disjoint, so the translucent children
    // are composited once, exactly as a full repaint draws them.
    let mut root = node(
        "box",
        LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 32.0,
        },
        mesh_core_elements::style::Color {
            r: 30,
            g: 30,
            b: 30,
            a: 160,
        },
    );
    root.id = 1;
    let mut list = RetainedDisplayList::default();
    list.update(&root, 64, 32, true, true);
    let scale = 1.5;
    let size = 96;
    let selected = list.select_paint_commands(
        Some(DamageRect {
            x: 0,
            y: 0,
            width: 64,
            height: 32,
        }),
        DisplayListRepaintPolicy::MinimalDamage,
    );
    let engine = FrontendRenderEngine::new();
    let mut full = PixelBuffer::new(size, size);
    engine.render_selected_display_list_for_module(&selected, &mut full, scale, None, None, None);

    let logical = [
        DamageRect {
            x: 0,
            y: 0,
            width: 11,
            height: 20,
        },
        DamageRect {
            x: 10,
            y: 0,
            width: 10,
            height: 20,
        },
    ];
    let device = crate::DeviceDamage::from_logical(&logical, scale, size, size);
    let mut partial = full.clone();
    let mut regions = Vec::new();
    for rect in device.rects() {
        partial.clear_rect(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            mesh_core_elements::style::Color::TRANSPARENT,
        );
        regions.push((rect.x, rect.y, rect.width, rect.height));
    }
    engine.render_selected_display_list_regions_for_module(
        &selected,
        &mut partial,
        scale,
        &regions,
        None,
        None,
    );

    assert_eq!(partial.data(), full.data());
}
