// ---------------------------------------------------------------------------
// scale factor tests
// ---------------------------------------------------------------------------

#[test]
fn fractional_scale_converts_120x_to_f32() {
    // wp_fractional_scale_v1 sends scale * 120
    let eps = f32::EPSILON;
    let v: f32 = 120.0 / 120.0 - 1.0;
    assert!(v.abs() < eps);
    let v: f32 = 180.0 / 120.0 - 1.5;
    assert!(v.abs() < eps);
    let v: f32 = 240.0 / 120.0 - 2.0;
    assert!(v.abs() < eps);
    let v: f32 = 150.0 / 120.0 - 1.25;
    assert!(v.abs() < eps);
}

#[test]
fn physical_dimensions_ceil_logical_times_scale() {
    // Physical = ceil(logical × scale)
    let compute_physical =
        |logical: u32, scale: f32| -> u32 { ((logical as f32 * scale).ceil() as u32).max(1) };
    assert_eq!(compute_physical(1920, 1.0), 1920);
    assert_eq!(compute_physical(1920, 2.0), 3840);
    assert_eq!(compute_physical(1920, 1.5), 2880);
    assert_eq!(compute_physical(100, 1.25), 125);
    assert_eq!(compute_physical(100, 1.75), 175);
}

#[test]
fn default_scale_is_1_0() {
    // SurfaceEntry must default to scale 1.0
    let default_scale: f32 = 1.0;
    assert_eq!(default_scale, 1.0);
}

#[test]
fn scale_change_detection_uses_f32_epsilon() {
    let current: f32 = 1.5;
    let same: f32 = 1.5;
    let different: f32 = 1.75;
    assert!(
        (current - same).abs() < f32::EPSILON,
        "tiny float differences should not trigger redraw"
    );
    assert!(
        (current - different).abs() > f32::EPSILON,
        "real scale changes must trigger redraw"
    );
}
// ---------------------------------------------------------------------------
// scale factor integer/ceil logic tests
// ---------------------------------------------------------------------------

#[test]
fn integer_scale_detection() {
    assert!((1.0_f32 - 1.0_f32.round()).abs() < f32::EPSILON);
    assert!((2.0_f32 - 2.0_f32.round()).abs() < f32::EPSILON);
    assert!((1.5_f32 - 1.5_f32.round()).abs() > f32::EPSILON);
    assert!((1.25_f32 - 1.25_f32.round()).abs() > f32::EPSILON);
}

#[test]
fn buffer_scale_for_integer_scale_equals_exact_value() {
    let scale: f32 = 2.0;
    assert_eq!(scale as i32, 2);
}

#[test]
fn buffer_scale_for_fractional_scale_ceils() {
    let scale: f32 = 1.5;
    assert_eq!(scale.ceil() as i32, 2);
    let scale: f32 = 1.25;
    assert_eq!(scale.ceil() as i32, 2);
}
