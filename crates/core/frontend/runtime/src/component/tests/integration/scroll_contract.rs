use super::*;
use mesh_core_interaction::find_scrollable_at_with_limits;
use mesh_core_render::{FractionalScale, PixelBuffer};
use mesh_core_theme::Theme;

#[test]
fn scroll_contract_handles_repeated_input_without_script_or_theme_rules() {
    let theme = Theme::new("empty", "Empty");
    for tag in ["scroll-area", "scroll", "scroll-view"] {
        for horizontal in [false, true] {
            for scale in [1.0, 1.25, 2.0] {
                let axis_style = if horizontal {
                    "overflow-x: auto; overflow-y: hidden; flex-direction: row;"
                } else {
                    ""
                };
                let source = format!(
                    r#"
<template><box class="host"><{tag} class="viewport"><box class="content" /></{tag}></box></template>
<style>
.host {{ width: 200px; height: 140px; }}
.viewport {{ width: 120px; height: 80px; {axis_style} }}
.content {{ width: 600px; height: 480px; flex-shrink: 0; }}
</style>
"#
                );
                let mut component = test_frontend_component(&source);
                Arc::make_mut(&mut component.settings)
                    .set_namespace("shell", serde_json::json!({"motion":{"reduced":true}}));
                let scaling = FractionalScale::new(scale);
                let mut pixels =
                    PixelBuffer::new(scaling.physical_extent(200), scaling.physical_extent(140));
                let extent = SurfaceExtent::unpadded(200, 140);
                component
                    .paint_raster(&theme, extent, &mut pixels, scale)
                    .unwrap();
                let tree = component.last_tree.as_ref().unwrap();
                let viewport = first_node_by_tag(tree, "scroll").unwrap();
                let id = viewport.id;
                let layout = viewport.layout;
                let content_layout = viewport.children[0].layout;
                let metrics = viewport.resolved_scroll_metrics();
                let max = if horizontal {
                    metrics.max_x
                } else {
                    metrics.max_y
                };
                assert!(max > 0.0, "<{tag}> must scroll with an empty theme");
                let (x, y) = (layout.x + 10.0, layout.y + 10.0);
                let mut expected = 0.0_f32;
                for step in 0..160 {
                    let delta = if step < 80 { 9.25 } else { -9.25 };
                    let hit =
                        find_scrollable_at_with_limits(component.last_tree.as_ref().unwrap(), x, y)
                            .expect("scroll ownership survives boundary reversal");
                    assert_eq!(hit.node_id, id);
                    let wheel = step % 3 == 0;
                    let distance = if wheel { delta / 28.0 } else { delta };
                    let (dx, dy) = if horizontal {
                        (-distance, 0.0)
                    } else {
                        (0.0, -distance)
                    };
                    let input = if wheel {
                        ComponentInput::Scroll { x, y, dx, dy }
                    } else {
                        ComponentInput::TwoFingerScroll { x, y, dx, dy }
                    };
                    component.handle_input(&theme, 200, 140, input).unwrap();
                    component
                        .paint_raster(&theme, extent, &mut pixels, scale)
                        .unwrap();
                    expected = (expected + delta).clamp(0.0, max);
                    let viewport =
                        first_node_by_tag(component.last_tree.as_ref().unwrap(), "scroll").unwrap();
                    let metrics = viewport.resolved_scroll_metrics();
                    let actual = if horizontal { metrics.x } else { metrics.y };
                    assert!(
                        (actual - expected).abs() < 0.001,
                        "<{tag}>, horizontal={horizontal}, scale={scale}, step={step}: {actual} != {expected}"
                    );
                    assert_eq!(viewport.layout, layout);
                    assert_eq!(viewport.children[0].layout, content_layout);
                    assert_eq!(
                        if horizontal {
                            metrics.max_x
                        } else {
                            metrics.max_y
                        },
                        max
                    );
                }
                assert!(expected.abs() < 0.001, "reverse input reaches the start");
            }
        }
    }
}
