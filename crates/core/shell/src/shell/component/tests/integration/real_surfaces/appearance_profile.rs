//! Frame cost of the Settings Appearance page under unrelated service traffic.
//!
//! Appearance is the largest shipped page (bounded 24+24 resource rows plus the
//! theme grid), and the shipped graph polls audio and network at ~1 Hz while it
//! is open. Those polls must not re-instantiate the page.

use super::*;
use crate::ShellComponent;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Pages of `@mesh/settings` that do not read `mesh.audio`. An audio poll must
/// reuse every one of their memoized subtrees.
const PAGES_NOT_READING_AUDIO: u64 = 5;

fn seed_large_resource_catalog(settings: &mut FrontendSurfaceComponent, count: usize) {
    let icon_themes = (0..count)
        .map(|index| {
            serde_json::json!({
                "id": format!("icon-theme-{index:03}"),
                "name": format!("Icon Theme {index:03}"),
                "inherits": ["hicolor"]
            })
        })
        .collect::<Vec<_>>();
    let font_families = (0..count)
        .map(|index| {
            serde_json::json!({
                "name": format!("Font Family {index:03}"),
                "face_count": 4,
                "monospace": index % 5 == 0
            })
        })
        .collect::<Vec<_>>();
    settings
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.theme".into(),
            source_module: "@mesh/core-settings".into(),
            payload: serde_json::json!({
                "current": "mesh-default-dark",
                "theme_id": "mesh-default-dark",
                "is_dark": true,
                "themes": [{ "id": "mesh-default-dark", "label": "MESH Dark" }],
                "available": ["mesh-default-dark"],
                "system_resources": {
                    "active_icon_theme": "icon-theme-000",
                    "active_font_family": "Font Family 000",
                    "icon_themes": icon_themes,
                    "font_families": font_families
                }
            }),
        })
        .unwrap();
    settings.invalidate_script_state();
}

fn audio_payload(volume: u32) -> serde_json::Value {
    serde_json::json!({
        "volume": volume,
        "muted": false,
        "sink": "alsa_output.pci-0000_00_1f.3.analog-stereo",
        "sink_description": "Built-in Audio Analog Stereo",
    })
}

fn appearance_surface(theme: &Theme, buffer: &mut PixelBuffer) -> FrontendSurfaceComponent {
    let mut settings =
        real_frontend_module_component("@mesh/settings", super::super::debug::settings_catalog());
    settings
        .paint(theme, SurfaceExtent::unpadded(920, 900), buffer, 1.0)
        .unwrap();
    seed_large_resource_catalog(&mut settings, 240);
    settings
        .call_namespaced_handler("__mesh_embed__::@mesh/settings::showAppearance", &[])
        .unwrap();
    settings
        .paint(theme, SurfaceExtent::unpadded(920, 900), buffer, 1.0)
        .unwrap();
    settings
}

fn publish_audio(settings: &mut FrontendSurfaceComponent, volume: u32) {
    settings
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: audio_payload(volume),
        })
        .unwrap();
}

#[test]
fn audio_polls_reuse_every_settings_page_appearance_does_not_read() {
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(920, 900);
    let mut settings = appearance_surface(&theme, &mut buffer);

    // Settle: the first audio payload is genuinely new to every runtime.
    publish_audio(&mut settings, 10);
    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();

    let hits_before = settings.component_memo_hit_count();
    publish_audio(&mut settings, 11);
    settings
        .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
        .unwrap();
    let reused = settings.component_memo_hit_count() - hits_before;

    assert_eq!(
        reused, PAGES_NOT_READING_AUDIO,
        "an audio poll must reuse the memoized subtree of every page that does \
         not read mesh.audio; reused {reused}"
    );

    // The reuse must not cost correctness: Appearance is still the rendered page.
    let text = settings
        .display_list_paint_commands()
        .iter()
        .filter_map(|command| match &command.node.content {
            mesh_core_render::display_list::DisplayPaintContent::Text(text) => {
                Some(text.text.to_string())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        text.iter().any(|text| text == "Color theme"),
        "Appearance content must survive a memoized audio poll frame: {text:?}"
    );
    assert!(
        text.iter().any(|text| text.starts_with("Icon Theme")),
        "the bounded icon-theme rows must survive a memoized audio poll frame"
    );
}

fn dump_stages(label: &str, records: Vec<ComponentProfilingRecord>, frames: u32) {
    let mut totals: BTreeMap<&'static str, (Duration, u32)> = BTreeMap::new();
    let mut attribution: BTreeMap<String, (Duration, u32)> = BTreeMap::new();
    for record in records {
        let trigger = record.trigger_kind.as_deref().unwrap_or("");
        // Attribution/waste records break a primary record down; counting them
        // alongside it double-counts the frame.
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
        let entry = totals.entry(record.stage.label()).or_default();
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

// cargo test -p mesh-core-shell --release -- appearance_frame_cost_profile --ignored --nocapture
#[test]
#[ignore = "release-only Appearance frame-cost profile"]
fn appearance_frame_cost_profile() {
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(920, 900);
    let mut settings = appearance_surface(&theme, &mut buffer);

    fn count_nodes(node: &WidgetNode) -> usize {
        1 + node.children.iter().map(count_nodes).sum::<usize>()
    }
    eprintln!(
        "appearance tree nodes: {}",
        count_nodes(settings.last_tree.as_ref().unwrap())
    );

    for volume in 0..10u32 {
        publish_audio(&mut settings, volume);
        settings
            .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
            .unwrap();
    }

    const FRAMES: u32 = 60;

    // Profiling attribution costs real time, so take the wall-clock numbers
    // first and use the instrumented pass only for the breakdown.
    let started = Instant::now();
    for volume in 0..FRAMES {
        publish_audio(&mut settings, volume % 100);
        settings
            .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
            .unwrap();
    }
    let service = started.elapsed();

    let started = Instant::now();
    for _ in 0..FRAMES {
        settings.invalidate_paint();
        settings
            .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
            .unwrap();
    }
    let paint_only = started.elapsed();

    let scroll_id =
        first_node_with_class_token(settings.last_tree.as_ref().unwrap(), "resource-list")
            .expect("resource list")
            .id;
    let started = Instant::now();
    for tick in 0..FRAMES {
        settings.scroll_offsets.insert(
            scroll_id,
            mesh_core_interaction::ScrollOffsetState {
                x: 0.0,
                y: (tick % 30) as f32 * 4.0,
            },
        );
        settings.invalidate(ComponentDirtyFlags::PAINT | ComponentDirtyFlags::METRICS);
        settings
            .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
            .unwrap();
    }
    let scroll = started.elapsed();

    let started = Instant::now();
    for _ in 0..FRAMES {
        settings.call_render_hooks();
    }
    let hooks = started.elapsed();

    for (label, elapsed) in [
        ("unrelated service update", service),
        ("paint only", paint_only),
        ("resource-list scroll", scroll),
        ("render hooks alone", hooks),
    ] {
        eprintln!(
            "{label:<26} {:>9.3}ms total, {:>7.3}ms per frame",
            elapsed.as_secs_f64() * 1000.0,
            elapsed.as_secs_f64() * 1000.0 / FRAMES as f64
        );
    }
    eprintln!(
        "MESH_PERF metric=appearance_service_frame_ms value={:.4}",
        service.as_secs_f64() * 1000.0 / FRAMES as f64
    );

    settings.set_profiling_enabled(true);
    settings.style_rule_attribution = true;
    let _ = settings.take_profiling_records();
    for volume in 0..FRAMES {
        publish_audio(&mut settings, volume % 100);
        settings
            .paint(&theme, SurfaceExtent::unpadded(920, 900), &mut buffer, 1.0)
            .unwrap();
    }
    dump_stages(
        "unrelated service update",
        settings.take_profiling_records(),
        FRAMES,
    );
}

/// One presented frame as `Shell::render_components_inner` drives it for a
/// settled surface: paint only when the component wants a frame, and take the
/// loop's single corrective pass when `wants_immediate_rerender` asks for it.
/// Returns how many passes ran.
fn render_loop_frame(
    settings: &mut FrontendSurfaceComponent,
    theme: &Theme,
    buffer: &mut PixelBuffer,
) -> u32 {
    if !settings.wants_render() {
        return 0;
    }
    let mut passes = 0;
    loop {
        settings
            .paint(theme, SurfaceExtent::unpadded(920, 900), buffer, 1.0)
            .unwrap();
        passes += 1;
        if !settings.wants_immediate_rerender() || passes >= 2 {
            return passes;
        }
        settings.invalidate_surface_config();
    }
}

fn percentile(sorted: &[Duration], fraction: f64) -> Duration {
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index]
}

/// Touchpad scroll of the Appearance page through real `TwoFingerScroll`
/// input and the render loop's rerender decision, then the momentum tail.
/// Unlike `paint_only_frame_speedup`, which invalidates `PAINT` directly, this
/// sees what the scroll input and the momentum ticks invalidate and whether
/// each frame is painted more than once.
///
// cargo test -p mesh-core-shell --release -- settings_scroll_frame_gate --ignored --nocapture
#[test]
#[ignore = "release-only Settings touchpad-scroll frame gate"]
fn settings_scroll_frame_gate() {
    const FINGER_FRAMES: u32 = 60;
    const MOMENTUM_FRAMES: u32 = 30;
    const FRAME_INTERVAL: Duration = Duration::from_millis(16);

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(920, 900);
    let mut settings = appearance_surface(&theme, &mut buffer);
    for _ in 0..4 {
        render_loop_frame(&mut settings, &theme, &mut buffer);
    }

    // A point the page scroller owns, not one of the nested resource lists.
    let (page_id, x, y) = {
        let tree = settings.last_tree.as_ref().unwrap();
        let page = first_node_with_class_token(tree, "settings-scroll").expect("page scroller");
        let layout = &page.layout;
        let candidates = (1..20).flat_map(|row| {
            (1..8).map(move |column| {
                (
                    layout.x + layout.width * column as f32 / 8.0,
                    layout.y + layout.height * row as f32 / 20.0,
                )
            })
        });
        let (x, y) = candidates
            .into_iter()
            .find(|&(x, y)| {
                find_scrollable_at_with_limits(tree, x, y).is_some_and(|hit| hit.node_id == page.id)
            })
            .expect("a point owned by the page scroller");
        (page.id, x, y)
    };

    // In a session the pointer is already over the page when the fingers go
    // down. Hover it first and let hover transitions finish, so the frames
    // below measure scrolling, not the first hover.
    settings
        .handle_input(&theme, 920, 900, ComponentInput::PointerMove { x, y })
        .unwrap();
    let settle_started = Instant::now();
    while render_loop_frame(&mut settings, &theme, &mut buffer) > 0 {
        assert!(
            settle_started.elapsed() < Duration::from_secs(2),
            "the page still wants frames 2s after the pointer arrived"
        );
        std::thread::sleep(FRAME_INTERVAL);
    }

    let mut frame_times = Vec::new();
    let mut passes = 0u32;
    let mut frames = 0u32;
    let mut style_frames = 0u32;
    let mut run_frame = |settings: &mut FrontendSurfaceComponent,
                         frame_times: &mut Vec<Duration>| {
        let started = Instant::now();
        let frame_passes = render_loop_frame(settings, &theme, &mut buffer);
        if frame_passes > 0 {
            frame_times.push(started.elapsed());
            frames += 1;
            passes += frame_passes;
            // The gesture's first frame claims scroll ownership, a one-time
            // interaction restyle. Every later frame must stay paint-only.
            if frames > 1
                && settings
                    .last_dirty_types
                    .contains(ComponentDirtyFlags::STYLE)
            {
                style_frames += 1;
            }
        }
    };

    for _ in 0..FINGER_FRAMES {
        let frame_started = Instant::now();
        settings
            .handle_input(
                &theme,
                920,
                900,
                ComponentInput::TwoFingerScroll {
                    x,
                    y,
                    dx: 0.0,
                    dy: -4.0,
                },
            )
            .unwrap();
        run_frame(&mut settings, &mut frame_times);
        std::thread::sleep(FRAME_INTERVAL.saturating_sub(frame_started.elapsed()));
    }
    let after_fingers = settings.scroll_offsets[&page_id].y;
    for _ in 0..MOMENTUM_FRAMES {
        let frame_started = Instant::now();
        run_frame(&mut settings, &mut frame_times);
        std::thread::sleep(FRAME_INTERVAL.saturating_sub(frame_started.elapsed()));
    }
    let after_momentum = settings.scroll_offsets[&page_id].y;

    frame_times.sort();
    let p50 = percentile(&frame_times, 0.5);
    let p90 = percentile(&frame_times, 0.9);
    eprintln!(
        "settings scroll: {frames} frames, {passes} passes, {style_frames} restyled after the first; \
         offset {after_fingers:.1} after fingers, {after_momentum:.1} after momentum; \
         frame p50 {:.3}ms p90 {:.3}ms",
        p50.as_secs_f64() * 1000.0,
        p90.as_secs_f64() * 1000.0,
    );
    eprintln!(
        "MESH_PERF metric=settings_scroll_frame_ms value={:.4} p90={:.4} frames={frames} passes={passes} style_frames={style_frames}",
        p50.as_secs_f64() * 1000.0,
        p90.as_secs_f64() * 1000.0,
    );

    assert!(after_fingers > 0.0, "finger input must scroll the page");
    assert!(
        after_momentum > after_fingers,
        "momentum must carry the page after the fingers lift"
    );
    assert_eq!(passes, frames, "a scroll frame must be painted once");
    assert_eq!(
        style_frames, 0,
        "no scroll frame after the gesture's first may restyle"
    );
}
