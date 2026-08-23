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
fn negotiated_capabilities_clamp_versions_and_gate_popup_reposition() {
    let capabilities = NegotiatedCapabilities::from_versions(1, 9, 2, 4, 3, 2, 2, 2, 8);

    assert_eq!(capabilities.generation, 1);
    assert_eq!(capabilities.layer_shell_version, 4);
    assert_eq!(capabilities.xdg_shell_version, 2);
    assert_eq!(capabilities.viewporter_version, 1);
    assert!(!capabilities.supports_xdg_popup_reposition());

    let capabilities = NegotiatedCapabilities::from_versions(2, 4, 3, 1, 1, 1, 1, 1, 3);
    assert_eq!(capabilities.generation, 2);
    assert!(capabilities.supports_xdg_popup_reposition());
}

#[test]
fn popup_reposition_tokens_are_nonzero_and_never_wrap() {
    assert_eq!(next_popup_reposition_token(0), Some(1));
    assert_eq!(next_popup_reposition_token(41), Some(42));
    assert_eq!(next_popup_reposition_token(u32::MAX), None);
}
