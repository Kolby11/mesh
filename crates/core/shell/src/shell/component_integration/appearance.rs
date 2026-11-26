use super::*;
/// Scrolling Settings through the shell runtime, with the compositor
/// answering its configure at the padded size, keeps every frame paint-only.
/// The shell once read its content-sized surface record as a padded size, so
/// each frame observed the content height minus the tooltip reserve and then
/// the real height again: two full restyles and two paints per frame.
#[test]
fn settings_scroll_frames_through_the_shell_stay_paint_only() {
    use crate::shell::runtime::profiling::CapturedRecord;
    use mesh_core_debug::ProfilingStage;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(920, 700);
    let mut settings =
        real_frontend_module_component("@mesh/settings", settings_catalog());
    settings
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 700), &mut buffer, 1.0)
        .unwrap();
    seed_large_resource_catalog(&mut settings, 240);
    invoke_handler(&mut settings, "__mesh_embed__::@mesh/settings::showAppearance", &[]).unwrap();
    settings
        .paint_raster(&theme, SurfaceExtent::unpadded(920, 700), &mut buffer, 1.0)
        .unwrap();
    for handler in ["onToggleIcons", "onToggleFonts"] {
        invoke_handler(
            &mut settings,
            &format!("__mesh_embed__::@mesh/settings/local:AppearancePage::{handler}"),
            &[],
        ).unwrap();
    }
    let id = settings.surface_id().to_owned();
    let mut shell = crate::shell::Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.presentation_engine.testing_report_configured_sizes();
    shell.register_component(Box::new(settings));
    let mut emitted = shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: id.clone(),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    for _ in 0..6 {
        shell.render_components().unwrap();
    }

    shell.debug.profiling_enabled = true;
    shell.profiling.capture = Some(Vec::new());
    let index = shell
        .components
        .iter()
        .position(|runtime| runtime.surface_id == id)
        .unwrap();
    let tree = shell.components[index].component.last_widget_tree().unwrap();
    let scroller = first_node_with_class_token(tree, "settings-scroll").expect("page scroller");
    let rect = scroller.layout;
    let (x, y) = (1..20)
        .flat_map(|row| (1..8).map(move |column| (
            rect.x + rect.width * column as f32 / 8.0,
            rect.y + rect.height * row as f32 / 20.0,
        )))
        .find(|&(x, y)| {
            mesh_core_interaction::find_scrollable_at_with_limits(tree, x, y)
                .is_some_and(|hit| hit.node_id == scroller.id)
        })
        .expect("point owned by the page scroller");
    const FRAMES: usize = 6;
    for _ in 0..FRAMES {
        shell.components[index]
            .component
            .handle_input(
                shell.theme.active(),
                920,
                700,
                ComponentInput::TwoFingerScroll {
                    x,
                    y,
                    dx: 0.0,
                    dy: -10.0,
                },
            )
            .unwrap();
        shell.render_components().unwrap();
    }
    let records = shell.profiling.capture.take().unwrap();
    let count = |stage: ProfilingStage, trigger: &str| {
        records
            .iter()
            .filter(|record| {
                matches!(record, CapturedRecord::Stage { surface_id, stage: s, trigger: t, .. }
                    if *surface_id == id && *s == stage && t.as_deref() == Some(trigger))
            })
            .count()
    };
    let presents = records
        .iter()
        .filter(|record| {
            matches!(record, CapturedRecord::Stage { surface_id, stage: ProfilingStage::PresentCommit, .. }
                if *surface_id == id)
        })
        .count();
    assert_eq!(presents, FRAMES, "every scroll frame presents");
    // The first frame claims scroll ownership: one interaction relayout.
    assert!(count(ProfilingStage::Layout, "restyle") <= 1);
    assert_eq!(
        count(ProfilingStage::Paint, "rebuild") + count(ProfilingStage::Paint, "restyle"),
        FRAMES,
        "a scroll frame paints once"
    );
}
