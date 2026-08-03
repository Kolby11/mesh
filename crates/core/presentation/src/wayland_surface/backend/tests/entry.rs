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
