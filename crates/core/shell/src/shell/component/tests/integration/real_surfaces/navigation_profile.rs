//! Frame cost of the shipped navigation bar, the always-on surface.
//!
//! Unlike Settings, the bar is painted for the whole session, so the frames it
//! repeats — a 1 Hz service poll, a pointer moving across it, and a plain
//! repaint — set the shell's idle and interaction cost floor.

use super::*;
use crate::ShellComponent;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 80;

/// The size the shell would settle the bar at, discovered by measuring it.
/// Painting at any other size leaves `SURFACE_CONFIG` raised every frame.
static SETTLED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(HEIGHT);

/// Frames per measured loop. Raise it via `MESH_BENCH_FRAMES` when sampling
/// under `perf`, so one-time module loading and Lua setup stop dominating.
fn frames() -> u32 {
    std::env::var("MESH_BENCH_FRAMES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60)
}

fn extent() -> SurfaceExtent {
    SurfaceExtent::unpadded(WIDTH, SETTLED.load(std::sync::atomic::Ordering::Relaxed))
}

fn publish(component: &mut FrontendSurfaceComponent, service: &str, payload: serde_json::Value) {
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: service.into(),
            source_module: "@mesh/bench".into(),
            payload,
        })
        .unwrap();
}

fn navigation_surface(theme: &Theme, buffer: &mut PixelBuffer) -> FrontendSurfaceComponent {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.visible = true;
    publish(
        &mut component,
        "mesh.locale",
        serde_json::json!({ "locale": "en", "current": "en" }),
    );
    publish(
        &mut component,
        "mesh.audio",
        serde_json::json!({ "volume": 40, "muted": false }),
    );
    publish(
        &mut component,
        "mesh.network",
        serde_json::json!({ "connected": true, "ssid": "bench", "strength": 72 }),
    );
    publish(
        &mut component,
        "mesh.power",
        serde_json::json!({ "percentage": 82, "charging": false }),
    );
    publish(
        &mut component,
        "mesh.brightness",
        serde_json::json!({ "level": 55 }),
    );
    component.paint(theme, extent(), buffer, 1.0).unwrap();
    component
}

fn dump_stages(label: &str, records: Vec<ComponentProfilingRecord>, frames: u32) {
    let mut totals: BTreeMap<String, (Duration, u32)> = BTreeMap::new();
    let mut attribution: BTreeMap<String, (Duration, u32)> = BTreeMap::new();
    for record in records {
        let trigger = record.trigger_kind.as_deref().unwrap_or("");
        if let Some(key) = trigger.strip_prefix("attribution:") {
            let entry = attribution
                .entry(format!("{}/{key}", record.stage.label()))
                .or_default();
            entry.0 += record.duration;
            entry.1 += 1;
            continue;
        }
        if trigger.starts_with("waste:") {
            continue;
        }
        let entry = totals.entry(record.stage.label().to_owned()).or_default();
        entry.0 += record.duration;
        entry.1 += 1;
    }
    eprintln!("--- {label} ({frames} frames) ---");
    let mut rows = totals.into_iter().collect::<Vec<_>>();
    rows.sort_by_key(|(_, (duration, _))| std::cmp::Reverse(*duration));
    for (stage, (duration, count)) in rows {
        eprintln!(
            "  {stage:<32} total {:>9.3}ms  n={count:<5} per-frame {:>7.3}ms",
            duration.as_secs_f64() * 1000.0,
            duration.as_secs_f64() * 1000.0 / frames as f64
        );
    }
    let mut rows = attribution.into_iter().collect::<Vec<_>>();
    rows.sort_by_key(|(_, (duration, _))| std::cmp::Reverse(*duration));
    rows.truncate(12);
    for (key, (duration, count)) in rows {
        eprintln!(
            "    * {key:<50} {:>7.3}ms/frame n={count}",
            duration.as_secs_f64() * 1000.0 / frames as f64
        );
    }
}

fn report(label: &str, elapsed: Duration, frames: u32) {
    eprintln!(
        "{label:<28} {:>9.3}ms total, {:>7.3}ms per frame",
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_secs_f64() * 1000.0 / frames as f64
    );
}

// cargo test -p mesh-core-shell --release -- navigation_frame_cost_profile --ignored --nocapture
#[test]
#[ignore = "release-only navigation-bar frame-cost profile"]
fn navigation_frame_cost_profile() {
    let frames = frames();
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(WIDTH, HEIGHT);
    let mut component = navigation_surface(&theme, &mut buffer);

    fn count_nodes(node: &WidgetNode) -> usize {
        1 + node.children.iter().map(count_nodes).sum::<usize>()
    }
    eprintln!(
        "navigation tree nodes: {}",
        count_nodes(component.last_tree.as_ref().unwrap())
    );

    // A surface whose measured content disagrees with the extent it was
    // painted at re-raises SURFACE_CONFIG every frame, which pushes each
    // following frame off the paint-only restyle path. Report it rather than
    // hiding it: the numbers below are only "settled surface" numbers when
    // this agrees.
    eprintln!(
        "measured content size: {:?} (painted at {WIDTH}x{HEIGHT})",
        component.measured_size
    );

    // Paint at the size the shell would have reconfigured the surface to.
    // Otherwise `observe_surface_size` re-raises STYLE|LAYOUT|PAINT|METRICS on
    // every single paint and no frame ever reaches a fast path.
    if let Some((_, measured_height)) = component.measured_size {
        SETTLED.store(measured_height, std::sync::atomic::Ordering::Relaxed);
    }
    for _ in 0..4 {
        component.invalidate_paint();
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    }
    eprintln!(
        "settled extent: {WIDTH}x{}, dirty flags now {:?}",
        SETTLED.load(std::sync::atomic::Ordering::Relaxed),
        component.last_dirty_types
    );

    // Settle: the first payload of each service is genuinely new.
    for volume in 0..10u32 {
        publish(
            &mut component,
            "mesh.audio",
            serde_json::json!({ "volume": volume, "muted": false }),
        );
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    }

    // A 1 Hz audio poll: the bar's volume button reads it, everything else
    // must not care.
    let started = Instant::now();
    for volume in 0..frames {
        publish(
            &mut component,
            "mesh.audio",
            serde_json::json!({ "volume": volume % 100, "muted": false }),
        );
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    }
    let audio_poll = started.elapsed();

    // A service nothing in the bar reads.
    let started = Instant::now();
    for tick in 0..frames {
        publish(
            &mut component,
            "mesh.media",
            serde_json::json!({ "playing": false, "title": format!("track {tick}") }),
        );
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    }
    let unread_poll = started.elapsed();

    // A pointer crossing the bar at 60 Hz: hover restyle plus repaint.
    let started = Instant::now();
    for tick in 0..frames {
        let x = (tick as f32 / frames as f32) * WIDTH as f32;
        component
            .handle_input(
                &theme,
                WIDTH,
                HEIGHT,
                ComponentInput::PointerMove { x, y: 40.0 },
            )
            .unwrap();
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    }
    let hover = started.elapsed();

    let started = Instant::now();
    for _ in 0..frames {
        component.invalidate_paint();
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    }
    let paint_only = started.elapsed();
    eprintln!(
        "paint-only frame dirty flags: {:?}",
        component.last_dirty_types
    );

    let started = Instant::now();
    for _ in 0..frames {
        component.call_render_hooks();
    }
    let hooks = started.elapsed();

    // How much of the surface a realistic narrow change actually repaints.
    // A root-level `backdrop-filter` makes every damage rect union with the
    // whole blurred region, so partial damage collapses to the full surface —
    // invisible to the wall-clock loops above, which force full damage anyway.
    let surface_area =
        u64::from(WIDTH) * u64::from(SETTLED.load(std::sync::atomic::Ordering::Relaxed));
    let mut damage_area = 0u64;
    let mut damage_rects = 0usize;
    let mut full_surface_frames = 0u32;
    let sample_frames = frames.min(240);
    for volume in 0..sample_frames {
        publish(
            &mut component,
            "mesh.audio",
            serde_json::json!({ "volume": volume % 100, "muted": false }),
        );
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
        if volume == 0 {
            eprintln!(
                "volume-change frame: dirty={:?} narrow_supported={} damage_rects={:?}",
                component.last_dirty_types,
                component.selective_service_build_supported,
                component.last_present_damage_rects,
            );
        }
        damage_rects += component.last_present_damage_rects.len();
        let frame_area: u64 = component
            .last_present_damage_rects
            .iter()
            .map(|rect| u64::from(rect.width) * u64::from(rect.height))
            .sum();
        damage_area += frame_area;
        if frame_area >= surface_area {
            full_surface_frames += 1;
        }
    }
    eprintln!(
        "damage on a volume change: {:.1}% of surface per frame, {:.2} rects/frame, \
         {full_surface_frames}/{sample_frames} frames full-surface",
        (damage_area as f64 / sample_frames as f64) / surface_area as f64 * 100.0,
        damage_rects as f64 / sample_frames as f64,
    );

    report("audio poll (read)", audio_poll, frames);
    report("media poll (unread)", unread_poll, frames);
    report("pointer move", hover, frames);
    report("paint only", paint_only, frames);
    report("render hooks alone", hooks, frames);
    eprintln!(
        "MESH_PERF metric=navigation_audio_frame_ms value={:.4}",
        audio_poll.as_secs_f64() * 1000.0 / frames as f64
    );
    eprintln!(
        "MESH_PERF metric=navigation_pointer_frame_ms value={:.4}",
        hover.as_secs_f64() * 1000.0 / frames as f64
    );

    // Instrumented passes: attribution costs real time, so they follow the
    // wall-clock numbers rather than replacing them. Under `perf` they are
    // skipped entirely — per-rule attribution formats a selector string per
    // rule per frame, which would show up as a production hotspot it is not.
    if std::env::var_os("MESH_BENCH_FRAMES").is_some() {
        eprintln!("MESH_BENCH_FRAMES set: skipping instrumented passes");
        return;
    }
    component.set_profiling_enabled(true);

    let _ = component.take_profiling_records();
    for volume in 0..frames {
        publish(
            &mut component,
            "mesh.audio",
            serde_json::json!({ "volume": volume % 100, "muted": false }),
        );
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    }
    dump_stages(
        "audio poll (read)",
        component.take_profiling_records(),
        frames,
    );

    let _ = component.take_profiling_records();
    for tick in 0..frames {
        let x = (tick as f32 / frames as f32) * WIDTH as f32;
        component
            .handle_input(
                &theme,
                WIDTH,
                HEIGHT,
                ComponentInput::PointerMove { x, y: 40.0 },
            )
            .unwrap();
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    }
    dump_stages("pointer move", component.take_profiling_records(), frames);

    let _ = component.take_profiling_records();
    for _ in 0..frames {
        component.invalidate_paint();
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    }
    dump_stages("paint only", component.take_profiling_records(), frames);
}
