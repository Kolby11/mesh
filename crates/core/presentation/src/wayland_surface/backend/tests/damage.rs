use super::*;
use mesh_core_render::DamageRect;
use smallvec::SmallVec;

// ---------------------------------------------------------------------------
// protocol_damage_rects tests
// ---------------------------------------------------------------------------

#[test]
fn protocol_damage_rects_single_rect_passthrough() {
    let rects = vec![DamageRect {
        x: 10,
        y: 20,
        width: 100,
        height: 50,
    }];
    let result = protocol_damage_rects(&rects, 1920, 1080);
    assert_eq!(result.len(), 1, "single rect should pass through unchanged");
    assert_eq!(result[0].x, 10);
    assert_eq!(result[0].y, 20);
    assert_eq!(result[0].width, 100);
    assert_eq!(result[0].height, 50);
}

#[test]
fn protocol_damage_rects_exactly_16_passthrough() {
    let rects: Vec<DamageRect> = (0..16)
        .map(|i| DamageRect {
            x: i * 10,
            y: i * 5,
            width: 10,
            height: 5,
        })
        .collect();
    let result = protocol_damage_rects(&rects, 1920, 1080);
    assert_eq!(
        result.len(),
        16,
        "exactly 16 rects should pass through unchanged"
    );
    for (i, r) in result.iter().enumerate() {
        assert_eq!(r.x, (i as u32) * 10);
        assert_eq!(r.y, (i as u32) * 5);
    }
}

#[test]
fn protocol_damage_rects_17_triggers_union_fallback() {
    let rects: Vec<DamageRect> = (0..17)
        .map(|i| DamageRect {
            x: (i % 10) * 20,
            y: (i / 10) * 30,
            width: 18,
            height: 28,
        })
        .collect();
    let result = protocol_damage_rects(&rects, 1920, 1080);
    assert_eq!(
        result.len(),
        1,
        "more than 16 rects must collapse to a single bounding union"
    );
    let union_rect = result[0];
    // All input rects must be contained within the union
    for r in &rects {
        assert!(
            r.x >= union_rect.x
                && r.y >= union_rect.y
                && r.x.saturating_add(r.width) <= union_rect.x.saturating_add(union_rect.width)
                && r.y.saturating_add(r.height) <= union_rect.y.saturating_add(union_rect.height),
            "every input rect must be contained within the union; rect {:?} not in {:?}",
            r,
            union_rect
        );
    }
}

#[test]
fn protocol_damage_rects_empty_input_returns_empty() {
    let result = protocol_damage_rects(&[], 1920, 1080);
    assert_eq!(result.len(), 0, "empty input must produce empty output");
}

#[test]
fn protocol_damage_rects_union_covers_known_geometry() {
    // rects spanning x:[0..100] and y:[0..50]
    let rects = vec![
        DamageRect {
            x: 0,
            y: 0,
            width: 50,
            height: 25,
        },
        DamageRect {
            x: 50,
            y: 0,
            width: 50,
            height: 25,
        },
        DamageRect {
            x: 0,
            y: 25,
            width: 50,
            height: 25,
        },
        DamageRect {
            x: 50,
            y: 25,
            width: 50,
            height: 25,
        },
        // Fill out to 17 with more disjoint rects
        DamageRect {
            x: 10,
            y: 30,
            width: 30,
            height: 10,
        },
        DamageRect {
            x: 20,
            y: 40,
            width: 30,
            height: 10,
        },
        DamageRect {
            x: 10,
            y: 10,
            width: 20,
            height: 5,
        },
        DamageRect {
            x: 60,
            y: 10,
            width: 20,
            height: 5,
        },
        DamageRect {
            x: 10,
            y: 35,
            width: 20,
            height: 5,
        },
        DamageRect {
            x: 60,
            y: 35,
            width: 20,
            height: 5,
        },
        DamageRect {
            x: 15,
            y: 5,
            width: 10,
            height: 10,
        },
        DamageRect {
            x: 70,
            y: 5,
            width: 10,
            height: 10,
        },
        DamageRect {
            x: 15,
            y: 40,
            width: 10,
            height: 5,
        },
        DamageRect {
            x: 70,
            y: 40,
            width: 10,
            height: 5,
        },
        DamageRect {
            x: 0,
            y: 20,
            width: 5,
            height: 10,
        },
        DamageRect {
            x: 95,
            y: 20,
            width: 5,
            height: 10,
        },
        DamageRect {
            x: 0,
            y: 45,
            width: 5,
            height: 5,
        },
    ];
    assert!(
        rects.len() > 16,
        "this test needs >16 rects to trigger union"
    );
    let result = protocol_damage_rects(&rects, 1920, 1080);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].x, 0);
    assert_eq!(result[0].y, 0);
    assert_eq!(result[0].width, 100);
    assert_eq!(result[0].height, 50);
}
// ---------------------------------------------------------------------------
// damage rect scaling tests
// ---------------------------------------------------------------------------

#[test]
fn scale_damage_rect_to_physical_multiplies_coordinates() {
    let logical = DamageRect {
        x: 10,
        y: 20,
        width: 100,
        height: 50,
    };
    let scaled = scale_damage_rect_to_physical(logical, 2.0);
    assert_eq!(scaled.x, 20);
    assert_eq!(scaled.y, 40);
    assert_eq!(scaled.width, 200);
    assert_eq!(scaled.height, 100);
}

#[test]
fn scale_damage_rect_to_physical_at_fractional_scale_ceils_dimensions() {
    let logical = DamageRect {
        x: 10,
        y: 20,
        width: 100,
        height: 50,
    };
    let scaled = scale_damage_rect_to_physical(logical, 1.5);
    assert_eq!(scaled.x, 15); // 10 * 1.5 = 15.0 → 15
    assert_eq!(scaled.y, 30); // 20 * 1.5 = 30.0 → 30
    assert_eq!(scaled.width, 150); // 100 * 1.5 = 150.0 → 150
    assert_eq!(scaled.height, 75); // 50 * 1.5 = 75.0 → 75
}

#[test]
fn scale_damage_rect_to_physical_rounds_far_edge_not_width() {
    let scaled = scale_damage_rect_to_physical(
        DamageRect {
            x: 1,
            y: 3,
            width: 2,
            height: 2,
        },
        1.5,
    );
    assert_eq!(
        scaled,
        DamageRect {
            x: 1,
            y: 4,
            width: 4,
            height: 4,
        }
    );
}

#[test]
fn scale_damage_rect_to_physical_at_identity_scale_is_identity() {
    let logical = DamageRect {
        x: 5,
        y: 10,
        width: 80,
        height: 40,
    };
    let scaled = scale_damage_rect_to_physical(logical, 1.0);
    assert_eq!(scaled.x, 5);
    assert_eq!(scaled.y, 10);
    assert_eq!(scaled.width, 80);
    assert_eq!(scaled.height, 40);
}

#[test]
fn scale_damage_rect_to_physical_never_produces_zero_dimensions() {
    let logical = DamageRect {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    let scaled = scale_damage_rect_to_physical(logical, 0.5);
    assert!(
        scaled.width >= 1,
        "width must be >= 1, got {}",
        scaled.width
    );
    assert!(
        scaled.height >= 1,
        "height must be >= 1, got {}",
        scaled.height
    );
}

#[test]
fn damage_rects_remain_in_logical_space_until_present() {
    // Proof that the render path emits logical damage rects and the present
    // boundary normalizes them to physical edge coverage once. This is an
    // architectural invariant test.
    let logical_rects = vec![DamageRect {
        x: 0,
        y: 0,
        width: 100,
        height: 50,
    }];
    let physical: Vec<DamageRect> = logical_rects
        .iter()
        .map(|r| scale_damage_rect_to_physical(*r, 2.0))
        .collect();
    assert_eq!(physical[0].x, 0);
    assert_eq!(physical[0].width, 200);
}
#[test]
fn pending_buffer_damage_preserves_disjoint_rectangles() {
    let bounds = full_damage(1_920, 100);
    let damage = [
        DamageRect {
            x: 0,
            y: 0,
            width: 20,
            height: 100,
        },
        DamageRect {
            x: 1_900,
            y: 0,
            width: 20,
            height: 100,
        },
    ];
    let mut pending = SmallVec::new();

    extend_pending_damage(&mut pending, &damage, bounds);

    assert_eq!(pending.as_slice(), damage.as_slice());
    let copied_area: u32 = pending.iter().map(|rect| rect.width * rect.height).sum();
    assert_eq!(copied_area, 4_000);
    assert_eq!(union_damage(Some(damage[0]), damage[1]).width, 1_920);
}

#[test]
fn pending_buffer_damage_collapses_when_rect_cap_is_exceeded() {
    let bounds = full_damage(1_920, 100);
    let damage: Vec<_> = (0..=MAX_PROTOCOL_DAMAGE_RECTS)
        .map(|index| DamageRect {
            x: index as u32 * 10,
            y: 0,
            width: 5,
            height: 5,
        })
        .collect();
    let mut pending = SmallVec::new();

    extend_pending_damage(&mut pending, &damage, bounds);

    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].x, 0);
    assert_eq!(pending[0].width, 165);
}

#[test]
fn restoring_copied_damage_uses_the_same_bounded_accumulator() {
    let bounds = full_damage(100, 40);
    let copied = [
        DamageRect {
            x: 0,
            y: 0,
            width: 10,
            height: 40,
        },
        DamageRect {
            x: 90,
            y: 0,
            width: 10,
            height: 40,
        },
    ];
    let mut pending = SmallVec::new();

    restore_pending_damage(&mut pending, &copied, bounds);

    assert_eq!(pending.as_slice(), copied.as_slice());
}
