use super::*;
use mesh_core_elements::{
    BoxShadow, LayoutRect, VisualFilter,
    style::{
        BackgroundPaint, Color, Corners, Edges, FontStyle, Overflow, TextAlign, TextDirection,
        TextOverflow,
    },
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
            in_scroll_content: false,
            id: 1,
            module_id: None,
            transform: mesh_core_elements::AffineTransform::translation(x, y),
            local_layout: LayoutRect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            layout: LayoutRect {
                x,
                y,
                width,
                height,
            },
            ancestor_clips: Vec::new().into(),
            style: DisplayPaintStyle {
                resource_revision: mesh_core_resources::resource_revision(),
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
                border_radius: Corners::zero(),
                corner_shape: Default::default(),
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
                font_style: FontStyle::Normal,
                letter_spacing: 0.0,
                line_height: 1.0,
                text_align: TextAlign::Left,
                text_overflow: TextOverflow::Clip,
                text_direction: TextDirection::default(),
                white_space: mesh_core_elements::style::WhiteSpace::default(),
                language: Arc::from(""),
                shaping_features: Arc::from(""),
                opacity: 1.0,
                box_shadow: BoxShadow::default(),
                additional_box_shadows: Default::default(),
                filter: VisualFilter::NONE,
                backdrop_filter: VisualFilter::blur(blur_radius),
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
    Arc::make_mut(&mut command.node).style.border_radius = Corners::all(18.0);
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

#[test]
fn spanning_and_docked_surfaces_keep_their_deliberate_zero_dimension() {
    use mesh_core_wayland::Edge;

    // A top/bottom bar is anchored to both horizontal edges unconditionally, so
    // a zero width is the protocol spelling for its span.
    assert!(layer_configure_size_is_resolved(Some(Edge::Top), 0, 0, 56));
    // A docked rail asked for its exclusive zone, so resolving a zero axis to
    // that zone gives it the size it named.
    assert!(layer_configure_size_is_resolved(Some(Edge::Left), 48, 0, 0));
    // Nothing backs these: the cross axis of a bar, and either axis of a
    // floating surface. Sending one makes the protocol layer substitute a size,
    // which is a visible collapse into the anchor corner.
    assert!(!layer_configure_size_is_resolved(
        Some(Edge::Top),
        0,
        1920,
        0
    ));
    assert!(!layer_configure_size_is_resolved(
        Some(Edge::Right),
        0,
        920,
        0
    ));
    assert!(!layer_configure_size_is_resolved(None, 0, 0, 700));
}

/// Paints its whole surface one colour through a canvas session and records
/// whether that paint was deferred for parallel replay.
struct SolidSurface {
    surface_id: String,
    color: Color,
    paint_delay: std::time::Duration,
    dirty: bool,
    deferred_paints: Arc<std::sync::Mutex<Vec<bool>>>,
}

impl crate::shell::types::ShellComponent for SolidSurface {
    fn id(&self) -> &str {
        &self.surface_id
    }

    fn surface_id(&self) -> &str {
        &self.surface_id
    }

    fn initial_visibility(&self) -> Option<bool> {
        Some(true)
    }

    fn mount(
        &mut self,
        _ctx: crate::shell::types::ComponentContext,
    ) -> Result<Vec<CoreRequest>, crate::shell::types::ComponentError> {
        Ok(Vec::new())
    }

    fn handle_core_event(
        &mut self,
        _event: &crate::shell::types::CoreEvent,
    ) -> Result<Vec<CoreRequest>, crate::shell::types::ComponentError> {
        Ok(Vec::new())
    }

    fn handle_service_event(
        &mut self,
        _event: &ServiceEvent,
    ) -> Result<Vec<CoreRequest>, crate::shell::types::ComponentError> {
        Ok(Vec::new())
    }

    fn tick(&mut self) -> Result<Vec<CoreRequest>, crate::shell::types::ComponentError> {
        Ok(Vec::new())
    }

    fn request_paint(&mut self) {
        self.dirty = true;
    }

    fn wants_render(&self) -> bool {
        self.dirty
    }

    fn wants_immediate_rerender(&self) -> bool {
        false
    }

    fn render(
        &mut self,
        surface: &mut dyn mesh_core_wayland::ShellSurface,
    ) -> Result<(), crate::shell::types::ComponentError> {
        surface.set_size(64, 24);
        Ok(())
    }

    fn paint(
        &mut self,
        _theme: &mesh_core_theme::Theme,
        _extent: crate::shell::types::SurfaceExtent,
        canvas: &mut mesh_core_render::SurfaceCanvas,
        _scale: f32,
    ) -> Result<(), crate::shell::types::ComponentError> {
        std::thread::sleep(self.paint_delay);
        let (width, height) = (canvas.width(), canvas.height());
        canvas.session().clear_rect(
            DamageRect {
                x: 0,
                y: 0,
                width,
                height,
            },
            self.color,
        );
        let deferred = matches!(
            canvas,
            mesh_core_render::SurfaceCanvas::Raster(raster) if raster.has_pending()
        );
        self.deferred_paints.lock().unwrap().push(deferred);
        self.dirty = false;
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), crate::shell::types::ComponentError> {
        Ok(())
    }
}

fn solid_surface_shell(colors: &[Color]) -> (Shell, Vec<Arc<std::sync::Mutex<Vec<bool>>>>) {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    let mut paints = Vec::new();
    for (index, &color) in colors.iter().enumerate() {
        let deferred_paints = Arc::new(std::sync::Mutex::new(Vec::new()));
        shell.register_component(Box::new(SolidSurface {
            surface_id: format!("@test/solid-{index}"),
            color,
            paint_delay: std::time::Duration::ZERO,
            dirty: true,
            deferred_paints: deferred_paints.clone(),
        }));
        paints.push(deferred_paints);
    }
    (shell, paints)
}

fn assert_presented_color(shell: &mut Shell, index: usize, color: Color) {
    let canvas = shell.components[index]
        .parent
        .paint_buffer
        .as_mut()
        .expect("painted canvas");
    let mesh_core_render::SurfaceCanvas::Raster(raster) = canvas else {
        panic!("test canvases are CPU buffers");
    };
    assert!(!raster.has_pending(), "frame replayed every recording");
    let pixel = raster.buffer_mut().get_pixel(10, 10);
    assert_eq!(
        (pixel.r, pixel.g, pixel.b, pixel.a),
        (color.r, color.g, color.b, color.a)
    );
}

#[test]
fn surfaces_painting_in_one_frame_rasterize_after_the_vm_phase() {
    let colors = [
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        },
        Color {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        },
    ];
    let (mut shell, paints) = solid_surface_shell(&colors);

    shell.render_components().unwrap();

    for (index, color) in colors.iter().enumerate() {
        // A first configure takes a second, measured pass in the same frame;
        // both recordings replay in order.
        let paints = paints[index].lock().unwrap();
        assert!(!paints.is_empty() && paints.iter().all(|&deferred| deferred));
        drop(paints);
        assert_presented_color(&mut shell, index, *color);
    }
    let presented = shell.presentation_engine.testing_presented_surfaces();
    for index in 0..colors.len() {
        assert!(presented.contains(&format!("@test/solid-{index}")));
    }
}

#[test]
fn a_lone_painting_surface_rasterizes_in_place() {
    let color = Color {
        r: 40,
        g: 80,
        b: 120,
        a: 255,
    };
    let (mut shell, paints) = solid_surface_shell(&[color, color]);
    shell.render_components().unwrap();

    let first_frame_paints = [0, 1].map(|index| paints[index].lock().unwrap().len());

    shell.components[1].component.request_paint();
    shell.render_components().unwrap();

    assert_eq!(paints[0].lock().unwrap().len(), first_frame_paints[0]);
    let lone = paints[1].lock().unwrap();
    assert_eq!(lone.len(), first_frame_paints[1] + 1);
    assert_eq!(lone.last(), Some(&false));
    drop(lone);
    assert_presented_color(&mut shell, 1, color);
}

#[test]
fn surface_totals_exclude_other_surfaces_preparation() {
    use mesh_core_debug::ProfilingStage;
    use std::time::Duration;

    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    for (index, paint_delay) in [Duration::ZERO, Duration::from_millis(100)]
        .into_iter()
        .enumerate()
    {
        shell.register_component(Box::new(SolidSurface {
            surface_id: format!("@test/solid-{index}"),
            color: Color {
                r: 40,
                g: 80,
                b: 120,
                a: 255,
            },
            paint_delay,
            dirty: true,
            deferred_paints: Arc::new(std::sync::Mutex::new(Vec::new())),
        }));
    }
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.render_components().unwrap();

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.unwrap();
    let totals: Vec<_> = (0..2)
        .map(|index| {
            let surface = profiling
                .surfaces
                .iter()
                .find(|surface| surface.surface_id == format!("@test/solid-{index}"))
                .unwrap();
            let raster = surface
                .stages
                .iter()
                .find(|stage| stage.stage == ProfilingStage::Paint)
                .unwrap();
            assert!(surface.total_surface_render_time_micros >= raster.total_micros);
            surface.total_surface_render_time_micros
        })
        .collect();
    assert!(
        totals[1] >= 100_000,
        "slow surface must include its own preparation: {totals:?}"
    );
    assert!(
        totals[0] < totals[1] / 2,
        "fast surface must exclude the slow surface's preparation: {totals:?}"
    );
}

#[cfg(feature = "gpu")]
#[test]
fn scene_submission_waits_for_completion_and_retries_unpresented_frames() {
    let (mut shell, _) = solid_surface_shell(&[Color::WHITE, Color::BLACK]);
    shell.presentation_engine.testing_set_async_scenes(true);
    shell.profiling.capture = Some(Vec::new());
    shell.render_components().unwrap();
    assert!(
        shell
            .presentation_engine
            .testing_presented_surfaces()
            .is_empty()
    );
    for runtime in &shell.components {
        assert!(
            !runtime.component.wants_render(),
            "submission consumed paint work"
        );
        assert!(!runtime.parent.force_full_present);
    }
    let success = shell.components[0].surface_id.clone();
    let failed = shell.components[1].surface_id.clone();
    assert!(
        shell
            .presentation_engine
            .testing_complete_scene(&success, true)
    );
    assert!(
        shell
            .presentation_engine
            .testing_complete_scene(&failed, false)
    );
    assert!(
        !shell
            .presentation_engine
            .testing_complete_scene(&failed, true)
    );
    shell.reconcile_scene_completions();
    assert!(!shell.components[0].component.wants_render());
    assert!(!shell.components[0].parent.force_full_present);
    assert!(shell.components[1].component.wants_render());
    assert!(shell.components[1].parent.force_full_present);
    assert_eq!(
        shell.presentation_engine.testing_presented_surfaces(),
        [success.as_str()]
    );
    let completions = shell
        .profiling
        .capture
        .as_ref()
        .unwrap()
        .iter()
        .filter(|record| {
            matches!(
                record,
                crate::shell::runtime::profiling::CapturedRecord::SceneCompletion { .. }
            )
        })
        .count();
    assert_eq!(completions, 2);
    shell.reconcile_scene_completions();
    assert!(
        shell
            .presentation_engine
            .take_scene_completions()
            .is_empty()
    );
    shell.render_components().unwrap();
    assert!(!shell.components[1].component.wants_render());
    assert!(
        shell
            .presentation_engine
            .testing_complete_scene(&failed, true)
    );
    shell.reconcile_scene_completions();
    assert!(!shell.components[1].component.wants_render());
    assert_eq!(
        shell.presentation_engine.testing_presented_surfaces(),
        [success, failed]
    );
}
