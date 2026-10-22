use super::*;
use crate::NegotiatedCapabilities;
use mesh_core_render::DamageRect;

#[test]
fn unchanged_blur_regions_are_committed_only_once() {
    let regions = vec![DamageRect {
        x: 0,
        y: 0,
        width: 800,
        height: 48,
    }];
    let mut current = Vec::new();
    let mut dirty = false;
    let mut commits = 0;

    for _ in 0..1_000 {
        set_pending_blur_regions(&mut current, &mut dirty, regions.clone());
        if dirty {
            commits += 1;
            dirty = false;
        }
    }
    assert_eq!(commits, 1);

    set_pending_blur_regions(&mut current, &mut dirty, Vec::new());
    assert!(dirty, "removing blur must produce a clearing commit");
}

#[test]
fn stale_frame_callbacks_cannot_release_a_newer_frame() {
    let mut generations = SurfaceGenerations::new(7);
    generations.accept_configure();
    let buffer = generations.allocate_buffer().unwrap();
    let first = generations.begin_frame("panel", buffer).unwrap();
    let second = generations.begin_frame("panel", buffer).unwrap();

    assert_eq!(generations.snapshot().configure, 1);
    assert_eq!(generations.snapshot().frame, 2);
    assert_eq!(first.buffer_generation, buffer);
    assert_eq!(second.buffer_generation, buffer);
    assert!(!generations.complete_frame(&first));
    assert!(generations.has_pending_frame());
    assert!(generations.complete_frame(&second));
    assert!(!generations.has_pending_frame());
}

#[test]
fn frame_callbacks_from_replaced_objects_are_ignored() {
    let mut old = SurfaceGenerations::new(7);
    let buffer = old.allocate_buffer().unwrap();
    let callback = old.begin_frame("panel", buffer).unwrap();
    let mut replacement = SurfaceGenerations::new(8);

    assert!(!replacement.complete_frame(&callback));
    assert!(!replacement.has_pending_frame());
}

#[test]
fn buffer_generations_identify_committed_slots_across_pool_replacement() {
    let mut generations = SurfaceGenerations::new(7);
    let first = generations.allocate_buffer().unwrap();
    let second = generations.allocate_buffer().unwrap();

    assert_eq!(first, 1);
    assert_eq!(second, 2);
    assert_eq!(generations.snapshot().buffer, 0);

    generations.commit_buffer(first);
    assert_eq!(generations.snapshot().buffer, first);

    // Clearing and rebuilding a SHM pool must not make a new slot look like
    // one of the old slots in diagnostics or resource handoff code.
    let replacement = generations.allocate_buffer().unwrap();
    generations.commit_buffer(replacement);
    assert_eq!(replacement, 3);
    assert_eq!(generations.snapshot().buffer, replacement);
}

#[test]
fn frame_rejects_an_unallocated_buffer_generation() {
    let mut generations = SurfaceGenerations::new(7);

    let error = generations
        .begin_frame("panel", 1)
        .expect_err("a frame cannot reference a slot that was never allocated");

    assert!(matches!(error, PresentationError::BufferAttach(_)));
}

#[test]
fn output_generation_advances_for_each_membership_revision() {
    let mut generations = SurfaceGenerations::new(7);

    assert_eq!(generations.snapshot().output, 0);
    assert!(generations.advance_output());
    assert_eq!(generations.snapshot().output, 1);
    assert!(generations.advance_output());
    assert_eq!(generations.snapshot().output, 2);
}

#[test]
fn output_membership_keeps_surviving_outputs_after_an_unordered_leave() {
    let mut membership = crate::OutputMembership::default();

    assert!(membership.enter(1));
    assert!(membership.enter(2));
    assert_eq!(membership.as_slice(), &[1, 2]);
    assert_eq!(membership.active(), Some(&2));

    // Leaving the older output must not clear the newer output's geometry
    // choice. Re-entering an existing output makes it the active choice again
    // without duplicating its membership.
    assert!(membership.leave(&1));
    assert_eq!(membership.as_slice(), &[2]);
    assert_eq!(membership.active(), Some(&2));
    assert!(membership.enter(1));
    assert_eq!(membership.as_slice(), &[2, 1]);
    assert_eq!(membership.active(), Some(&1));

    assert!(!membership.leave(&9));
    assert!(!membership.enter(1));
}

#[test]
fn negotiated_capabilities_clamp_versions_and_gate_popup_reposition() {
    let capabilities = NegotiatedCapabilities::from_versions(1, 9, 2, 4, 3, 2, 3, 2, 2, 8, 9);

    assert_eq!(capabilities.generation, 1);
    assert_eq!(capabilities.layer_shell_version, 4);
    assert_eq!(capabilities.xdg_shell_version, 2);
    assert_eq!(capabilities.viewporter_version, 1);
    assert_eq!(capabilities.background_effect_version, 1);
    assert!(!capabilities.supports_xdg_popup_reposition());
    assert!(!capabilities.supports_xdg_popup_reactive_positioner());

    let capabilities = NegotiatedCapabilities::from_versions(2, 4, 3, 1, 1, 1, 1, 1, 1, 3, 1);
    assert_eq!(capabilities.generation, 2);
    assert!(capabilities.supports_xdg_popup_reposition());
    assert!(capabilities.supports_xdg_popup_reactive_positioner());
}

#[test]
fn popup_reposition_tokens_are_nonzero_and_never_wrap() {
    assert_eq!(next_popup_reposition_token(0), Some(1));
    assert_eq!(next_popup_reposition_token(41), Some(42));
    assert_eq!(next_popup_reposition_token(u32::MAX), None);
}

#[test]
fn blur_prefers_background_effect_only_while_it_can_blur() {
    use BlurProtocol::{BackgroundEffect, Kde};
    assert_eq!(
        preferred_blur_protocol(true, true, true),
        Some(BackgroundEffect)
    );
    assert_eq!(
        preferred_blur_protocol(true, true, false),
        Some(BackgroundEffect)
    );
    // Bound, but no `blur` capability yet (or withdrawn).
    assert_eq!(preferred_blur_protocol(true, false, true), Some(Kde));
    assert_eq!(preferred_blur_protocol(true, false, false), None);
    assert_eq!(preferred_blur_protocol(false, false, true), Some(Kde));
    assert_eq!(preferred_blur_protocol(false, false, false), None);
}

#[test]
fn padded_popup_content_stays_at_output_edge_on_live_compositor() {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return;
    }
    let mut backend = WaylandSurfaceBackend::new().unwrap();
    let wait = |backend: &mut WaylandSurfaceBackend, id: &str| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !backend.state.surfaces[id].configured {
            assert!(
                std::time::Instant::now() < deadline,
                "configure timed out: {id}"
            );
            backend.pump().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    for (edge, anchor, gravity, offset_y) in [
        (Edge::Top, PopupAnchor::Bottom, PopupGravity::Bottom, 12),
        (Edge::Bottom, PopupAnchor::Top, PopupGravity::Top, -12),
    ] {
        let content = ContentExtent::from_size((800, 58)).unwrap();
        backend
            .configure(
                "anchor-parent",
                SurfaceConfig {
                    namespace: "@mesh/popup-anchor-regression".into(),
                    edge: Some(edge),
                    exclusive_zone: -1,
                    extent: SurfaceExtent::from_content_and_padding(
                        content,
                        SurfacePadding::default(),
                    )
                    .unwrap(),
                    wire_size: LayerWireSize {
                        width: LayerWireExtent::Span,
                        height: LayerWireExtent::fixed(58).unwrap(),
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        wait(&mut backend, "anchor-parent");
        let (width, height) = backend.surface_size_if_known("anchor-parent").unwrap();
        let mut canvas = mesh_core_render::SurfaceCanvas::from(mesh_core_render::PixelBuffer::new(
            width, height,
        ));
        assert_eq!(
            backend
                .present_with_damage(
                    "anchor-parent",
                    "",
                    true,
                    &mut canvas,
                    &[DamageRect {
                        x: 0,
                        y: 0,
                        width,
                        height
                    }]
                )
                .unwrap(),
            PresentStatus::Presented
        );
        backend.pump().unwrap();
        // Layer-map animations move the parent while constraints are resolved.
        let settled = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while std::time::Instant::now() < settled {
            backend.pump().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let mut config = PopupConfig {
            parent_surface_id: "anchor-parent".into(),
            placement: PopupPlacement {
                anchor_rect: (width as i32 - 36, 6, 36, 36),
                size: (772, 835),
                anchor,
                gravity,
                offset: (0, offset_y),
                ..Default::default()
            },
            padding: SurfacePadding {
                left: 188,
                top: 170,
                right: 188,
                bottom: 206,
            },
            input_passthrough: false,
            grab: false,
            grab_identity: None,
        };
        backend
            .configure_popup("padded-popup", config.clone())
            .unwrap();
        wait(&mut backend, "padded-popup");
        let entry = &backend.state.surfaces["padded-popup"];
        let WaylandRole::Popup(role) = &entry.role else {
            unreachable!()
        };
        let right_gap = width as i32 - (role.position.0 + 396);
        assert!(
            (0..=36).contains(&right_gap),
            "{edge:?}: shadow pushed content inward: {right_gap}"
        );
        let expected_y = if edge == Edge::Top {
            42 + 12
        } else {
            6 - 459 - 12
        };
        assert_eq!(role.position.1, expected_y, "{edge:?}: vertical anchor gap");
        assert_eq!(
            (entry.width, entry.height),
            (772, 835),
            "configure must preserve paint reserve"
        );
        let mut canvas =
            mesh_core_render::SurfaceCanvas::from(mesh_core_render::PixelBuffer::new(772, 835));
        assert_eq!(
            backend
                .present_with_damage(
                    "padded-popup",
                    "",
                    true,
                    &mut canvas,
                    &[DamageRect {
                        x: 0,
                        y: 0,
                        width: 772,
                        height: 835
                    }]
                )
                .unwrap(),
            PresentStatus::Presented
        );
        config.placement.anchor_rect.0 = 0;
        backend.configure_popup("padded-popup", config).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            backend.pump().unwrap();
            let WaylandRole::Popup(role) = &backend.state.surfaces["padded-popup"].role else {
                unreachable!()
            };
            if role.pending_reposition_token.is_none() {
                assert!(
                    (0..=36).contains(&role.position.0),
                    "shadow constrained reposition: {:?}",
                    role.position
                );
                break;
            }
            assert!(std::time::Instant::now() < deadline, "reposition timed out");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        backend.destroy_surface("anchor-parent");
    }
}
