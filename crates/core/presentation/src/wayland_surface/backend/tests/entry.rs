use super::*;
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
    let first = generations.begin_frame("panel").unwrap();
    let second = generations.begin_frame("panel").unwrap();

    assert_eq!(generations.snapshot().configure, 1);
    assert_eq!(generations.snapshot().frame, 2);
    assert!(!generations.complete_frame(&first));
    assert!(generations.has_pending_frame());
    assert!(generations.complete_frame(&second));
    assert!(!generations.has_pending_frame());
}

#[test]
fn frame_callbacks_from_replaced_objects_are_ignored() {
    let mut old = SurfaceGenerations::new(7);
    let callback = old.begin_frame("panel").unwrap();
    let mut replacement = SurfaceGenerations::new(8);

    assert!(!replacement.complete_frame(&callback));
    assert!(!replacement.has_pending_frame());
}
