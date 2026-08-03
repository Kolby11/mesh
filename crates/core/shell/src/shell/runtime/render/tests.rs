use super::*;
use mesh_core_elements::{
    BoxShadow, LayoutRect, VisualFilter,
    style::{BackgroundPaint, Color, Edges, Overflow, TextAlign, TextDirection, TextOverflow},
};
use mesh_core_render::{
    DamageRect, DisplayListClip, DisplayPaintCommand, DisplayPaintCommandKind,
    display_list::DisplayPaintNode,
};
use std::sync::Arc;

fn make_cmd(x: f32, y: f32, width: f32, height: f32, blur_radius: f32) -> DisplayPaintCommand {
    use mesh_core_render::display_list::{
        DisplayPaintContent, DisplayPaintStyle, DisplayScrollbars,
    };
    DisplayPaintCommand {
        node: Arc::new(DisplayPaintNode {
            id: 1,
            layout: LayoutRect {
                x,
                y,
                width,
                height,
            },
            style: DisplayPaintStyle {
                background_color: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                },
                background_paint: BackgroundPaint::None,
                border_color: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                },
                border_width: Edges::zero(),
                border_radius: 0.0,
                color: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                padding: Edges::zero(),
                overflow_x: Overflow::Visible,
                overflow_y: Overflow::Visible,
                font_family: Arc::from(""),
                font_size: 16.0,
                font_weight: 400,
                line_height: 1.0,
                text_align: TextAlign::Left,
                text_overflow: TextOverflow::Clip,
                text_direction: TextDirection::default(),
                opacity: 1.0,
                box_shadow: BoxShadow::default(),
                filter: VisualFilter::NONE,
                backdrop_filter: VisualFilter { blur_radius },
                mix_blend_mode: mesh_core_elements::BlendMode::Normal,
                icon_fill: None,
                icon_weight: None,
                icon_grade: None,
                icon_optical_size: None,
            },
            content: DisplayPaintContent::None,
            scrollbars: DisplayScrollbars::default(),
        }),
        clip: DisplayListClip {
            x: 0,
            y: 0,
            width: (x.max(0.0) + width).ceil() as i32,
            height: (y.max(0.0) + height).ceil() as i32,
        },
        kind: DisplayPaintCommandKind::Node,
    }
}

#[test]
fn test_compute_blur_regions_single_node() {
    let cmds = vec![make_cmd(10.0, 20.0, 100.0, 50.0, 4.0)];
    assert_eq!(
        compute_blur_regions(&cmds),
        vec![DamageRect {
            x: 10,
            y: 20,
            width: 100,
            height: 50
        }]
    );
}

#[test]
fn test_compute_blur_regions_no_blur_nodes() {
    let cmds = vec![make_cmd(0.0, 0.0, 100.0, 100.0, 0.0)];
    assert!(compute_blur_regions(&cmds).is_empty());
}

#[test]
fn test_compute_blur_regions_negative_coords() {
    // x=-10, y=-5, w=100, h=80 → x=0, y=0, width=90, height=75
    let cmds = vec![make_cmd(-10.0, -5.0, 100.0, 80.0, 4.0)];
    assert_eq!(
        compute_blur_regions(&cmds),
        vec![DamageRect {
            x: 0,
            y: 0,
            width: 90,
            height: 75
        }]
    );
}

#[test]
fn test_compute_blur_regions_keep_disjoint_nodes_separate() {
    let cmds = vec![
        make_cmd(0.0, 0.0, 50.0, 50.0, 4.0),
        make_cmd(100.0, 100.0, 50.0, 50.0, 4.0),
    ];
    assert_eq!(
        compute_blur_regions(&cmds),
        vec![
            DamageRect {
                x: 0,
                y: 0,
                width: 50,
                height: 50
            },
            DamageRect {
                x: 100,
                y: 100,
                width: 50,
                height: 50
            },
        ]
    );
}

#[test]
fn test_compute_blur_regions_follow_rounded_element_shape() {
    let mut command = make_cmd(10.0, 20.0, 36.0, 36.0, 14.0);
    Arc::make_mut(&mut command.node).style.border_radius = 18.0;
    command.clip = DisplayListClip {
        x: 10,
        y: 20,
        width: 36,
        height: 36,
    };

    let regions = compute_blur_regions(&[command]);
    let area: u64 = regions.iter().copied().map(DamageRect::area).sum();
    assert!(regions.len() > 1, "a circle needs multiple wl_region bands");
    assert!(
        area > 900 && area < 1_200,
        "36px circular mask should track its painted area, got {area}px"
    );
    assert!(regions.iter().all(|region| {
        region.x >= 10
            && region.y >= 20
            && region.x + region.width <= 46
            && region.y + region.height <= 56
    }));
}

// cargo test -p mesh-core-shell --release -- cached_region_state_beats_command_scan --ignored --nocapture
#[test]
#[ignore = "release-only derived-region microbenchmark"]
fn cached_region_state_beats_command_scan() {
    use std::hint::black_box;
    use std::time::Instant;

    let commands: Vec<_> = (0..500)
        .map(|index| {
            make_cmd(
                index as f32,
                index as f32,
                20.0,
                20.0,
                if index % 50 == 0 { 4.0 } else { 0.0 },
            )
        })
        .collect();
    let cached = (7_u64, Some((1920, 1080)), Some((1920, 56)));
    let iterations = 20_000;

    let scan_started = Instant::now();
    for _ in 0..iterations {
        black_box(compute_blur_regions(black_box(&commands)));
    }
    let scan = scan_started.elapsed();

    let cache_started = Instant::now();
    for _ in 0..iterations {
        black_box(black_box(Some(cached)) == Some(cached));
    }
    let cache = cache_started.elapsed();

    eprintln!(
        "region command scan: {scan:?}; generation/geometry cache check: {cache:?}; ratio: {:.1}x",
        scan.as_secs_f64() / cache.as_secs_f64()
    );
    assert!(cache < scan);
}
