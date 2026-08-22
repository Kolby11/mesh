use super::*;

// ---------------------------------------------------------------------------
// SHM size-class tests
// ---------------------------------------------------------------------------

#[test]
fn viewport_shm_size_classes_absorb_small_content_resizes() {
    let first = try_shm_pool_config_for(401, 199, true).unwrap();
    let second = try_shm_pool_config_for(447, 223, true).unwrap();

    assert_eq!(first, second);
    assert_eq!((first.width, first.height), (448, 256));
}

#[test]
fn viewport_size_classes_eliminate_content_resize_pool_churn() {
    let sizes = [
        (401, 199),
        (418, 205),
        (432, 217),
        (447, 223),
        (405, 201),
        (440, 220),
    ];
    let config_changes = |viewport_available| {
        sizes
            .into_iter()
            .map(|(width, height)| {
                try_shm_pool_config_for(width, height, viewport_available).unwrap()
            })
            .fold((None, 0usize), |(previous, changes), next| {
                (Some(next), changes + usize::from(previous != Some(next)))
            })
            .1
    };

    assert_eq!(config_changes(false), sizes.len());
    assert_eq!(config_changes(true), 1);
}

#[test]
fn shm_buffers_stay_exact_without_a_viewporter_crop() {
    let config = try_shm_pool_config_for(401, 199, false).unwrap();

    assert_eq!(config.width, 401);
    assert_eq!(config.height, 199);
    assert_eq!(config.stride, 401 * 4);
    assert_eq!(config.bytes, 401 * 199 * 4);
}

#[test]
fn shm_config_rejects_oversized_dimensions_and_byte_lengths() {
    assert!(try_shm_pool_config_for(MAX_SHM_DIMENSION + 1, 1, false).is_err());
    assert!(try_shm_pool_config_for(16_384, 1_025, false).is_err());
    assert!(try_shm_pool_config_for(u32::MAX, 1, true).is_err());
}

#[test]
fn shm_pool_growth_budget_accounts_for_slot_pool_doubling() {
    assert!(shm_pool_growth_allowed(256 * 1024, 4 * 1024 * 1024));
    assert!(!shm_pool_growth_allowed(MAX_SHM_POOL_BYTES / 2 + 1, 1));
    assert!(!shm_pool_growth_allowed(MAX_SHM_POOL_BYTES, 0));
}

#[test]
fn viewport_crop_uses_post_buffer_scale_coordinates() {
    assert_eq!(viewport_source_dimensions(800, 400, 2), (400.0, 200.0));
    assert_eq!(viewport_source_dimensions(600, 300, 2), (300.0, 150.0));
}

#[test]
fn full_copy_uses_the_allocation_stride_for_size_class_buffers() {
    let width = 3;
    let height = 2;
    let canvas_width = 4;
    let src: Vec<u8> = (0..width * height * 4).map(|value| value as u8).collect();
    let mut canvas = vec![0xff; canvas_width as usize * height as usize * 4];

    copy_bgra_to_canvas(&src, &mut canvas, width, height, canvas_width).unwrap();

    let short_src = &src[..src.len() - 1];
    assert_eq!(
        copy_bgra_to_canvas(short_src, &mut canvas, width, height, canvas_width),
        Err(BufferCopyError::SourceTooShort)
    );

    for row in 0..height as usize {
        let src_start = row * width as usize * 4;
        let canvas_start = row * canvas_width as usize * 4;
        assert_eq!(
            &canvas[canvas_start..canvas_start + width as usize * 4],
            &src[src_start..src_start + width as usize * 4],
        );
        assert_eq!(
            &canvas[canvas_start + width as usize * 4..canvas_start + canvas_width as usize * 4],
            &[0xff; 4],
        );
    }
}
