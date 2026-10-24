use super::*;
use crate::{
    DisplayListRepaintPolicy, FrontendRenderEngine, PixelBuffer, RenderObjectTree,
    RetainedDisplayList,
};
use mesh_core_elements::{Color, Prop, WidgetNode};

fn node(id: u64, x: f32, y: f32) -> WidgetNode {
    let mut node = WidgetNode::new("box");
    node.id = id;
    node.layout = mesh_core_elements::LayoutRect {
        x,
        y,
        width: 6.0,
        height: 6.0,
    };
    node.computed_style.background_color = Color {
        r: 70,
        g: 110,
        b: 180,
        a: 180,
    };
    node
}

fn tree(siblings: usize) -> WidgetNode {
    let mut root = node(1, 0., 0.);
    root.layout.width = 256.;
    root.layout.height = 256.;
    root.children = (0..siblings)
        .map(|i| node(i as u64 + 2, (i % 32) as f32 * 8., (i / 32) as f32 * 8.))
        .collect();
    root
}

fn record(
    engine: &FrontendRenderEngine,
    list: &RetainedDisplayList,
    scene: &mut SceneCanvas,
    scale: f32,
    module: Option<&str>,
) -> Arc<PreparedScene> {
    scene.begin(list.frame_paint_plan());
    let selected = list.select_paint_commands(
        Some(crate::DamageRect {
            x: 0,
            y: 0,
            width: 256,
            height: 256,
        }),
        DisplayListRepaintPolicy::FullSurface,
    );
    engine.render_selected_display_list_for_module_in_session(
        &selected,
        &mut scene.session(),
        scale,
        None,
        None,
        module,
    );
    scene.snapshot(crate::FractionalScale::new(scale)).unwrap();
    Arc::clone(scene.prepared.as_ref().unwrap())
}

fn raster(scene: &PreparedScene) -> PixelBuffer {
    let mut pixels = PixelBuffer::new(scene.size.0, scene.size.1);
    PixelCanvasSession::new(&mut pixels).with_canvas(|canvas| scene.draw(canvas));
    pixels
}

fn assert_full_parity(
    engine: &FrontendRenderEngine,
    list: &RetainedDisplayList,
    cached: &PreparedScene,
    scale: f32,
) {
    let mut fresh = SceneCanvas::new(cached.size.0, cached.size.1).unwrap();
    fresh.set_node_cache_enabled(false);
    let full = record(engine, list, &mut fresh, scale, None);
    assert_eq!(raster(cached).data(), raster(&full).data());
    let mut direct = PixelBuffer::new(cached.size.0, cached.size.1);
    engine.render_selected_display_list_for_module(
        &list.select_paint_commands(
            Some(crate::DamageRect {
                x: 0,
                y: 0,
                width: 256,
                height: 256,
            }),
            DisplayListRepaintPolicy::FullSurface,
        ),
        &mut direct,
        scale,
        None,
        None,
        None,
    );
    assert_eq!(raster(cached).data(), direct.data());
}

#[test]
fn scene_preparation_records_only_changed_nodes() {
    if crate::isolate_resource_state_test(
        "prepared_scene::retained_tests::scene_preparation_records_only_changed_nodes",
    ) {
        return;
    }
    let _guard = crate::resource_state_test_lock();
    let engine = FrontendRenderEngine::new();
    for size in [16, 1024] {
        let mut root = tree(size);
        let mut objects = RenderObjectTree::default();
        objects.update(&root);
        let mut list = RetainedDisplayList::default();
        list.update(&root, 256, 256, true, true);
        let mut scene = SceneCanvas::new(320, 320).unwrap();
        let first = record(&engine, &list, &mut scene, 1.25, None);
        let first_pixels = raster(&first);
        assert_eq!(
            first.picture.approximate_op_count(),
            size + 1,
            "one reference per node"
        );
        record(&engine, &list, &mut scene, 1.25, None);
        assert_eq!(scene.preparation_metrics().recorded_nodes, 0);
        assert_eq!(scene.preparation_metrics().reused_nodes, size + 1);
        let mut operation_counts = Vec::new();
        for color in [Color::WHITE, Color::BLACK, Color::TRANSPARENT] {
            root.children[0].computed_style.background_color = color;
            let dirty = objects.update(&root);
            list.update_with_dirty_nodes(
                &root,
                dirty,
                objects.dirty_node_ids(),
                256,
                256,
                false,
                true,
            );
            let picture = record(&engine, &list, &mut scene, 1.25, None);
            let metrics = scene.preparation_metrics();
            assert_eq!(metrics.recorded_nodes, 1, "{size}: {metrics:?}");
            assert_eq!(metrics.reused_nodes, size);
            operation_counts.push(metrics.recorded_operations);
            assert_full_parity(&engine, &list, &picture, 1.25);
        }
        assert!(operation_counts.iter().all(|count| *count <= 8));
        // Submitted scenes remain complete and immutable after subsequent edits.
        assert_eq!(raster(&first).data(), first_pixels.data());
    }
}

#[test]
fn scene_cache_invalidates_external_paint_inputs() {
    if crate::isolate_resource_state_test(
        "prepared_scene::retained_tests::scene_cache_invalidates_external_paint_inputs",
    ) {
        return;
    }
    let _guard = crate::resource_state_test_lock();
    let engine = FrontendRenderEngine::new();
    let mut list = RetainedDisplayList::default();
    list.update(&tree(16), 256, 256, true, true);
    let mut scene = SceneCanvas::new(512, 512).unwrap();
    for (scale, module) in [(1.0, None), (1.25, None), (1.25, Some("other"))] {
        record(&engine, &list, &mut scene, scale, module);
        assert_eq!(scene.preparation_metrics().recorded_nodes, 17);
        record(&engine, &list, &mut scene, scale, module);
        assert_eq!(scene.preparation_metrics().recorded_nodes, 0);
    }
    resource_completed();
    let picture = record(&engine, &list, &mut scene, 1.25, None);
    assert_eq!(scene.preparation_metrics().recorded_nodes, 17);
    assert_full_parity(&engine, &list, &picture, 1.25);
    // A partial preparation cannot replace a full-extent node recording.
    scene.begin(list.frame_paint_plan());
    engine.render_selected_display_list_for_module_in_session(
        &list.select_paint_commands(
            Some(crate::DamageRect {
                x: 0,
                y: 0,
                width: 256,
                height: 256,
            }),
            DisplayListRepaintPolicy::FullSurface,
        ),
        &mut scene.session(),
        1.25,
        Some((0, 0, 5, 5)),
        None,
        None,
    );
    scene.snapshot(crate::FractionalScale::new(1.25)).unwrap();
    let picture = record(&engine, &list, &mut scene, 1.25, None);
    assert_full_parity(&engine, &list, &picture, 1.25);
}

#[test]
fn scene_cache_preserves_effects_scroll_and_tooltip_inputs() {
    use mesh_core_elements::style::{BlendMode, Overflow};
    let _guard = crate::resource_state_test_lock();
    let engine = FrontendRenderEngine::new();
    let mut root = tree(16);
    let mut group = node(100, 30., 30.);
    group.layout.width = 140.;
    group.layout.height = 140.;
    group.computed_style.opacity = 0.65;
    group.computed_style.transform.rotation = 0.08;
    group.computed_style.overflow_x = Overflow::Hidden;
    group.computed_style.mix_blend_mode = BlendMode::Multiply;
    group.computed_style.filter.blur_radius = 2.;
    group.computed_style.backdrop_filter.blur_radius = 3.;
    let mut text = WidgetNode::new("text");
    text.id = 101;
    text.layout = mesh_core_elements::LayoutRect {
        x: 8.,
        y: 8.,
        width: 110.,
        height: 20.,
    };
    text.attributes
        .insert(Prop::Content, "Retained picture".into());
    text.computed_style.color = Color::WHITE;
    group.children.push(text);
    let mut scroll = tree(16);
    scroll.id = 200;
    for child in &mut scroll.children {
        child.id += 200;
    }
    scroll.layout.x = 50.;
    scroll.layout.y = 170.;
    scroll.layout.width = 100.;
    scroll.layout.height = 50.;
    scroll.computed_style.overflow_y = Overflow::Hidden;
    scroll.scroll_metrics = Some(mesh_core_elements::WidgetScrollMetrics {
        max_y: 100.,
        content_height: 150.,
        content_width: 100.,
        ..Default::default()
    });
    root.children.extend([group, scroll]);
    let mut list = RetainedDisplayList::default();
    list.set_backdrop_blur_policy(crate::BackdropBlurPolicy::InSurfaceFilter);
    let mut scene = SceneCanvas::new(320, 320).unwrap();
    for scroll in [0., 12.5, 42., 0.] {
        root.children
            .last_mut()
            .unwrap()
            .scroll_metrics
            .as_mut()
            .unwrap()
            .y = scroll;
        list.update(&root, 256, 256, false, true);
        for _ in 0..2 {
            let picture = record(&engine, &list, &mut scene, 1.25, None);
            assert!(scene.preparation_metrics().live_commands > 0);
            assert_full_parity(&engine, &list, &picture, 1.25);
        }
    }
    // Tooltips are outside the retained node stream and must be redrawn.
    let mut pixels = Vec::new();
    for opacity in [1.0, 0.4] {
        engine.set_tooltip_opacity(opacity);
        scene.begin(list.frame_paint_plan());
        let selected = list.select_paint_commands(
            Some(crate::DamageRect {
                x: 0,
                y: 0,
                width: 256,
                height: 256,
            }),
            DisplayListRepaintPolicy::FullSurface,
        );
        engine.render_selected_display_list_for_module_in_session(
            &selected,
            &mut scene.session(),
            1.25,
            None,
            None,
            None,
        );
        engine.render_tooltip_clipped_in_session(
            "Tooltip",
            10.,
            10.,
            &mut scene.session(),
            1.25,
            None,
        );
        pixels.push(raster(
            scene
                .snapshot(crate::FractionalScale::new(1.25))
                .unwrap()
                .resources(),
        ));
    }
    assert_ne!(pixels[0].data(), pixels[1].data());
}

#[test]
#[ignore = "release scene-preparation comparison"]
fn scene_preparation_profile() {
    let _guard = crate::resource_state_test_lock();
    let engine = FrontendRenderEngine::new();
    for size in [16, 1024] {
        let root = tree(size);
        let mut list = RetainedDisplayList::default();
        list.update(&root, 256, 256, true, true);
        for enabled in [false, true] {
            let mut scene = SceneCanvas::new(320, 320).unwrap();
            scene.set_node_cache_enabled(enabled);
            for _ in 0..4 {
                record(&engine, &list, &mut scene, 1.25, None);
            }
            for sample in 0..3 {
                let started = std::time::Instant::now();
                for _ in 0..120 {
                    record(&engine, &list, &mut scene, 1.25, None);
                }
                eprintln!(
                    "scene prep siblings={size} cache={enabled} sample={sample} mean_ms={:.4} work={:?}",
                    started.elapsed().as_secs_f64() * 1000. / 120.,
                    scene.preparation_metrics()
                );
            }
        }
    }
}
