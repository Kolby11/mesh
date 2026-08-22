use super::*;
use mesh_core_render::DamageRect;
use smallvec::SmallVec;
use std::hash::{Hash, Hasher};

#[test]
#[ignore = "release-only present damage allocation benchmark"]
fn borrowed_protocol_damage_beats_cloned_passthrough() {
    use std::hint::black_box;
    use std::time::Instant;

    let rects = [
        DamageRect {
            x: 0,
            y: 0,
            width: 20,
            height: 20,
        },
        DamageRect {
            x: 200,
            y: 0,
            width: 20,
            height: 20,
        },
        DamageRect {
            x: 400,
            y: 0,
            width: 20,
            height: 20,
        },
        DamageRect {
            x: 600,
            y: 0,
            width: 20,
            height: 20,
        },
    ];
    let iterations = 1_000_000;

    let started = Instant::now();
    for _ in 0..iterations {
        black_box(black_box(&rects).to_vec());
    }
    let cloned = started.elapsed();

    let started = Instant::now();
    for _ in 0..iterations {
        black_box(protocol_damage_rects(black_box(&rects), 800, 48));
    }
    let borrowed = started.elapsed();

    eprintln!(
        "protocol damage passthrough over {iterations} iterations: cloned {cloned:?}, borrowed {borrowed:?}"
    );
}

#[test]
#[ignore = "release-only clipped damage scratch allocation benchmark"]
fn smallvec_clipped_damage_beats_heap_vec_scratch() {
    use std::hint::black_box;
    use std::time::Instant;

    let rects = [
        DamageRect {
            x: 0,
            y: 0,
            width: 20,
            height: 20,
        },
        DamageRect {
            x: 200,
            y: 0,
            width: 20,
            height: 20,
        },
        DamageRect {
            x: 400,
            y: 0,
            width: 20,
            height: 20,
        },
        DamageRect {
            x: 600,
            y: 0,
            width: 20,
            height: 20,
        },
    ];
    let copy_damage = [
        DamageRect {
            x: 0,
            y: 0,
            width: 20,
            height: 20,
        },
        DamageRect {
            x: 600,
            y: 0,
            width: 20,
            height: 20,
        },
    ];
    let iterations = 1_000_000;

    let started = Instant::now();
    for _ in 0..iterations {
        let mut clipped_damage: Vec<DamageRect> = black_box(&rects)
            .iter()
            .map(|r| scale_damage_rect_to_physical(*r, 1.0))
            .map(|r| clip_damage_rect_to_buffer(r, 800, 48))
            .collect();
        clipped_damage.extend(
            black_box(&copy_damage)
                .iter()
                .map(|rect| clip_damage_rect_to_buffer(*rect, 800, 48)),
        );
        black_box(protocol_damage_rects(&clipped_damage, 800, 48));
    }
    let heap_vec = started.elapsed();

    let started = Instant::now();
    for _ in 0..iterations {
        let mut clipped_damage: SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]> =
            black_box(&rects)
                .iter()
                .map(|r| scale_damage_rect_to_physical(*r, 1.0))
                .map(|r| clip_damage_rect_to_buffer(r, 800, 48))
                .collect();
        clipped_damage.extend(
            black_box(&copy_damage)
                .iter()
                .map(|rect| clip_damage_rect_to_buffer(*rect, 800, 48)),
        );
        black_box(protocol_damage_rects(&clipped_damage, 800, 48));
    }
    let inline = started.elapsed();

    eprintln!(
        "clipped damage scratch over {iterations} iterations: heap Vec {heap_vec:?}, SmallVec {inline:?}, ratio {:.1}x",
        heap_vec.as_secs_f64() / inline.as_secs_f64()
    );
    assert!(inline < heap_vec);
}

#[test]
#[ignore = "release-only surface config fingerprint microbenchmark"]
fn primitive_surface_config_hashing_beats_byte_writes() {
    use std::hint::black_box;
    use std::time::Instant;

    struct OldHasher(u64);
    impl Default for OldHasher {
        fn default() -> Self {
            Self(0xcbf2_9ce4_8422_2325)
        }
    }
    impl Hasher for OldHasher {
        fn finish(&self) -> u64 {
            self.0
        }

        fn write(&mut self, bytes: &[u8]) {
            for byte in bytes {
                self.0 ^= u64::from(*byte);
                self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }

    fn old_surface_config_fingerprint(cfg: &SurfaceConfig, keyboard_mode: KeyboardMode) -> u64 {
        let mut hasher = OldHasher::default();
        surface_edge_slot(cfg.edge).hash(&mut hasher);
        surface_layer_slot(cfg.layer).hash(&mut hasher);
        cfg.exclusive_zone.hash(&mut hasher);
        keyboard_mode_slot(keyboard_mode).hash(&mut hasher);
        cfg.width.hash(&mut hasher);
        cfg.height.hash(&mut hasher);
        cfg.margin_top.hash(&mut hasher);
        cfg.margin_right.hash(&mut hasher);
        cfg.margin_bottom.hash(&mut hasher);
        cfg.margin_left.hash(&mut hasher);
        hasher.finish()
    }

    let cfg = SurfaceConfig {
        role: SurfaceRole::Layer,
        window: WindowOptions::default(),
        edge: Some(Edge::Top),
        layer: MeshLayer::Overlay,
        size_policy: LayerSurfaceSizePolicy::Fixed,
        width: 1_920,
        height: 48,
        padding: SurfacePadding::default(),
        exclusive_zone: 48,
        keyboard_mode: KeyboardMode::OnDemand,
        namespace: "benchmark".into(),
        margin_top: 2,
        margin_right: 4,
        margin_bottom: 6,
        margin_left: 8,
        blur: false,
    };
    let iterations = 2_000_000;

    let started = Instant::now();
    let mut old_hash = 0;
    for _ in 0..iterations {
        old_hash ^=
            old_surface_config_fingerprint(black_box(&cfg), black_box(KeyboardMode::OnDemand));
    }
    let old = started.elapsed();

    let started = Instant::now();
    let mut new_hash = 0;
    for _ in 0..iterations {
        new_hash ^= surface_config_fingerprint(black_box(&cfg), black_box(KeyboardMode::OnDemand));
    }
    let new = started.elapsed();

    black_box((old_hash, new_hash));
    eprintln!(
        "surface config fingerprint over {iterations} configs: byte writes {old:?}, primitive writes {new:?}, ratio {:.1}x",
        old.as_secs_f64() / new.as_secs_f64()
    );
    assert!(new < old);
}
#[test]
#[ignore = "release-only disjoint SHM copy benchmark"]
fn disjoint_damage_copy_beats_bounding_union_copy() {
    use std::hint::black_box;
    use std::time::Instant;

    let width = 1_920;
    let height = 100;
    let src = vec![0x7f; width as usize * height as usize * 4];
    let mut canvas = vec![0; src.len()];
    let left = DamageRect {
        x: 0,
        y: 0,
        width: 20,
        height,
    };
    let right = DamageRect {
        x: width - 20,
        y: 0,
        width: 20,
        height,
    };
    let union = union_damage(Some(left), right);
    let iterations = 1_000;

    let started = Instant::now();
    for _ in 0..iterations {
        copy_bgra_damage_to_canvas(
            black_box(&src),
            black_box(&mut canvas),
            width,
            height,
            width,
            union,
        )
        .unwrap();
    }
    let union_elapsed = started.elapsed();

    let started = Instant::now();
    for _ in 0..iterations {
        for rect in [left, right] {
            copy_bgra_damage_to_canvas(
                black_box(&src),
                black_box(&mut canvas),
                width,
                height,
                width,
                rect,
            )
            .unwrap();
        }
    }
    let disjoint_elapsed = started.elapsed();

    eprintln!(
        "SHM copy over {iterations} disjoint frames: bounding union {union_elapsed:?}, rect list {disjoint_elapsed:?}"
    );
}

// cargo test -p mesh-core-presentation --release -- fractional_sparse_damage_copy_beats_full_surface_upload --ignored --nocapture
#[test]
#[ignore = "release-only fractional-scale SHM upload benchmark"]
fn fractional_sparse_damage_copy_beats_full_surface_upload() {
    use std::hint::black_box;
    use std::time::Instant;

    let width = 1_920;
    let height = 1_080;
    let src = vec![0x7f; width as usize * height as usize * 4];
    let mut canvas = vec![0; src.len()];
    let logical_damage = DamageRect {
        x: 101,
        y: 73,
        width: 24,
        height: 20,
    };
    let physical_damage = scale_damage_rect_to_physical(logical_damage, 1.5);
    let iterations = 100;

    let full_started = Instant::now();
    for _ in 0..iterations {
        copy_bgra_damage_to_canvas(
            black_box(&src),
            black_box(&mut canvas),
            width,
            height,
            width,
            full_damage(width, height),
        )
        .unwrap();
    }
    let full_time = full_started.elapsed();

    let sparse_started = Instant::now();
    for _ in 0..iterations {
        copy_bgra_damage_to_canvas(
            black_box(&src),
            black_box(&mut canvas),
            width,
            height,
            width,
            physical_damage,
        )
        .unwrap();
    }
    let sparse_time = sparse_started.elapsed();

    eprintln!(
        "fractional SHM upload: full {full_time:?}; sparse {sparse_time:?}; ratio {:.1}x; physical_damage={physical_damage:?}",
        full_time.as_secs_f64() / sparse_time.as_secs_f64()
    );
    assert!(sparse_time * 10 < full_time);
}
