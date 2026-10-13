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

/// Frames for one named loop. `MESH_BENCH_ONLY=<name>` runs only that loop
/// (`audio`, `unread`, `pointer`, `paint`, `hooks`), so a `perf` sample
/// attributes to a single workload instead of the whole mix.
fn loop_frames(name: &str) -> u32 {
    match std::env::var("MESH_BENCH_ONLY") {
        Ok(only) if only != name => 0,
        _ => frames(),
    }
}

/// Paint only when the component asks for a frame, as `Shell::render_components`
/// does. Painting unconditionally measures a forced repaint of an unchanged
/// surface, which takes the conservative full-rebuild path and never happens in
/// a session. Returns whether a frame was painted.
fn paint_if_wanted(
    component: &mut FrontendSurfaceComponent,
    theme: &Theme,
    buffer: &mut PixelBuffer,
) -> bool {
    if !component.wants_render() {
        return false;
    }
    component.paint(theme, extent(), buffer, 1.0).unwrap();
    true
}

fn extent() -> SurfaceExtent {
    SurfaceExtent::unpadded(WIDTH, SETTLED.load(std::sync::atomic::Ordering::Relaxed))
}

/// The shape `@mesh/navigation-bar`'s volume button actually reads: its
/// `render()` hook takes `audio.percent` and `audio.muted`, and returns early
/// unless `audio.available` is set. Publishing any other field measures a poll
/// the bar ignores — which is what "media poll (unread)" below is for.
fn audio_payload(percent: u32) -> serde_json::Value {
    serde_json::json!({ "available": true, "percent": percent, "muted": false })
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
    publish(&mut component, "mesh.audio", audio_payload(40));
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
    // `MESH_BENCH_BACKDROP=in-surface` paints the bar's `backdrop-filter` in
    // the surface, as on a compositor without a blur protocol (Hyprland).
    // The default is the compositor-region policy, which paints no blur.
    if std::env::var("MESH_BENCH_BACKDROP").is_ok_and(|value| value == "in-surface") {
        mesh_core_render::set_backdrop_blur_policy(
            mesh_core_render::BackdropBlurPolicy::InSurfaceFilter,
        );
        eprintln!("backdrop policy: in-surface");
    }
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
        publish(&mut component, "mesh.audio", audio_payload(volume));
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    }

    // A 1 Hz audio poll: the bar's volume button reads it, everything else
    // must not care.
    let started = Instant::now();
    let mut audio_painted = 0u32;
    for volume in 0..loop_frames("audio") {
        publish(&mut component, "mesh.audio", audio_payload(volume % 100));
        audio_painted += u32::from(paint_if_wanted(&mut component, &theme, &mut buffer));
    }
    let audio_poll = started.elapsed();

    // A service nothing in the bar reads.
    let started = Instant::now();
    let mut unread_painted = 0u32;
    for tick in 0..loop_frames("unread") {
        publish(
            &mut component,
            "mesh.media",
            serde_json::json!({ "playing": false, "title": format!("track {tick}") }),
        );
        unread_painted += u32::from(paint_if_wanted(&mut component, &theme, &mut buffer));
    }
    let unread_poll = started.elapsed();

    // A pointer crossing the bar at 60 Hz through its vertical middle: hover
    // restyle plus repaint. (Before 2026-09-30 this used y = 40, the bottom
    // edge of the settled 40px bar, so it never hovered anything.)
    let settled_height = SETTLED.load(std::sync::atomic::Ordering::Relaxed);
    let pointer_y = settled_height as f32 / 2.0;
    let started = Instant::now();
    let mut pointer_painted = 0u32;
    for tick in 0..loop_frames("pointer") {
        let x = (tick as f32 / frames as f32) * WIDTH as f32;
        component
            .handle_input(
                &theme,
                WIDTH,
                settled_height,
                ComponentInput::PointerMove { x, y: pointer_y },
            )
            .unwrap();
        pointer_painted += u32::from(paint_if_wanted(&mut component, &theme, &mut buffer));
    }
    let hover = started.elapsed();
    // Leave the bar and let hover transitions finish, so the loops below
    // measure a bar at rest rather than one with the last button's tooltip up.
    component
        .handle_input(&theme, WIDTH, settled_height, ComponentInput::PointerLeave)
        .unwrap();
    // Transitions run on the wall clock, so settle at 60 Hz, not in a
    // tight loop. A bar still animating after two seconds is reported.
    let settle_started = Instant::now();
    while paint_if_wanted(&mut component, &theme, &mut buffer) {
        if settle_started.elapsed() > Duration::from_secs(2) {
            eprintln!("warning: bar still wants frames 2s after the pointer left");
            break;
        }
        std::thread::sleep(Duration::from_millis(16));
    }

    let started = Instant::now();
    for _ in 0..loop_frames("paint") {
        component.invalidate_paint();
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    }
    let paint_only = started.elapsed();
    eprintln!(
        "paint-only frame dirty flags: {:?}",
        component.last_dirty_types
    );

    let started = Instant::now();
    for _ in 0..loop_frames("hooks") {
        component.flush_reactive_effects();
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
        publish(&mut component, "mesh.audio", audio_payload(volume % 100));
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
        let dirty = component.last_dirty_types;
        let narrow_supported = component.selective_service_build_supported;
        // Take rather than read: a zero-damage frame leaves the accumulator
        // untouched, so reading it would re-report the previous loop's
        // full-surface rect and score a frame that repainted nothing as 100%.
        let frame_damage = component.take_present_damage();
        if volume == 0 {
            eprintln!(
                "volume-change frame: dirty={dirty:?} narrow_supported={narrow_supported} \
                 damage_rects={frame_damage:?}"
            );
        }
        damage_rects += frame_damage.len();
        let frame_area: u64 = frame_damage
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

    eprintln!(
        "frames painted: audio {audio_painted}, unread {unread_painted}, pointer {pointer_painted} \
         (per-frame costs below divide by all {frames} events)"
    );
    report("audio poll (read)", audio_poll, frames);
    report("media poll (unread)", unread_poll, frames);
    report("pointer move", hover, frames);
    report("paint only", paint_only, frames);
    report("reactive effects alone", hooks, frames);
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
    component.style_rule_attribution = true;

    let _ = component.take_profiling_records();
    for volume in 0..frames {
        publish(&mut component, "mesh.audio", audio_payload(volume % 100));
        paint_if_wanted(&mut component, &theme, &mut buffer);
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
                settled_height,
                ComponentInput::PointerMove { x, y: pointer_y },
            )
            .unwrap();
        paint_if_wanted(&mut component, &theme, &mut buffer);
    }
    dump_stages("pointer move", component.take_profiling_records(), frames);

    let _ = component.take_profiling_records();
    for _ in 0..frames {
        component.invalidate_paint();
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    }
    dump_stages("paint only", component.take_profiling_records(), frames);
}

fn collect_popover_triggers<'a>(node: &'a WidgetNode, out: &mut Vec<&'a WidgetNode>) {
    if node.attributes.get("aria-haspopup").map(String::as_str) == Some("true")
        && node.event_handlers.contains_key("pointerenter")
    {
        out.push(node);
    }
    for child in &node.children {
        collect_popover_triggers(child, out);
    }
}

fn percentile(sorted: &[Duration], fraction: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index]
}

// cargo test -p mesh-core-shell --release -- navigation_hover_profile --ignored --nocapture
//
// The hover-intent workload: the pointer enters each of the bar's popover
// triggers, the popover opens, the pointer crosses into the promoted popup,
// leaves it, and moves on to the next trigger before the close bridge fires.
// Every step is one input event followed by the frame the shell would paint.
#[test]
#[ignore = "release-only navigation-bar hover-intent profile"]
fn navigation_hover_profile() {
    let cycles = std::env::var("MESH_BENCH_CYCLES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(25u32);
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(WIDTH, HEIGHT);
    let mut component = navigation_surface(&theme, &mut buffer);
    if let Some((_, measured_height)) = component.measured_size {
        SETTLED.store(measured_height, std::sync::atomic::Ordering::Relaxed);
    }
    for _ in 0..4 {
        component.invalidate_paint();
        component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    }
    let height = SETTLED.load(std::sync::atomic::Ordering::Relaxed);

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation bar");
    let mut trigger_nodes = Vec::new();
    collect_popover_triggers(tree, &mut trigger_nodes);
    let triggers: Vec<(String, f32, f32)> = trigger_nodes
        .iter()
        .map(|node| {
            let key = node.mesh_key().expect("trigger key").to_owned();
            let (left, top, right, bottom) =
                mesh_core_interaction::find_node_bounds_by_key(tree, &key, 0.0, 0.0)
                    .expect("trigger bounds");
            (key, (left + right) / 2.0, (top + bottom) / 2.0)
        })
        .collect();
    assert_eq!(
        triggers.len(),
        4,
        "expected the volume, theme, language and settings triggers"
    );
    // A point on the bar that no trigger covers, between the start slot and
    // the right cluster.
    let gap = (WIDTH as f32 / 2.0, height as f32 / 2.0);

    let mut step_times = Vec::new();
    let mut handler_times = Vec::new();
    let mut opened = 0u32;
    let mut painted = 0u32;
    let events_before = component.drained_script_events;

    let mut step = |component: &mut FrontendSurfaceComponent,
                    painted: &mut u32,
                    input: &mut dyn FnMut(&mut FrontendSurfaceComponent)| {
        let started = Instant::now();
        input(component);
        let handled = started.elapsed();
        *painted += u32::from(paint_if_wanted(component, &theme, &mut buffer));
        step_times.push(started.elapsed());
        handler_times.push(handled);
    };

    let started = Instant::now();
    for _ in 0..cycles {
        for (key, x, y) in &triggers {
            let (x, y) = (*x, *y);
            // The trigger's popover is its sibling in the button shell.
            let shell_prefix = key.rsplit_once('/').map_or("", |(parent, _)| parent);
            step(&mut component, &mut painted, &mut |component| {
                component
                    .handle_input(&theme, WIDTH, height, ComponentInput::PointerMove { x, y })
                    .unwrap();
            });
            let Some(popup) = component
                .child_surface_requests()
                .into_iter()
                .find(|request| request.node_key.starts_with(shell_prefix))
            else {
                step(&mut component, &mut painted, &mut |component| {
                    component
                        .handle_input(
                            &theme,
                            WIDTH,
                            height,
                            ComponentInput::PointerMove { x: gap.0, y: gap.1 },
                        )
                        .unwrap();
                });
                continue;
            };
            opened += 1;
            let (cw, ch) = popup.content_size;
            let (pad_left, pad_top, ..) = popup.content_padding;
            let offset = (pad_left as f32, pad_top as f32);
            // The pointer leaves the bar surface for the popup surface, as
            // Wayland reports it: schedules the close bridge.
            step(&mut component, &mut painted, &mut |component| {
                component
                    .handle_input(&theme, WIDTH, height, ComponentInput::PointerLeave)
                    .unwrap();
            });
            // Enter the popup: cancels it.
            step(&mut component, &mut painted, &mut |component| {
                component
                    .handle_child_surface_input(
                        &popup.node_key,
                        &theme,
                        cw,
                        ch,
                        offset,
                        ComponentInput::PointerMove {
                            x: offset.0 + cw as f32 / 2.0,
                            y: offset.1 + ch as f32 / 2.0,
                        },
                    )
                    .unwrap();
            });
            // Leave the popup: schedules it again.
            step(&mut component, &mut painted, &mut |component| {
                component
                    .handle_child_surface_input(
                        &popup.node_key,
                        &theme,
                        cw,
                        ch,
                        offset,
                        ComponentInput::PointerLeave,
                    )
                    .unwrap();
            });
        }
    }
    let elapsed = started.elapsed();
    let publishes = component.drained_script_events - events_before;

    // Let the pending bridges fire.
    std::thread::sleep(Duration::from_millis(450));
    component.tick().unwrap();
    component.paint(&theme, extent(), &mut buffer, 1.0).unwrap();
    let still_open = component.child_surface_requests().len();

    let steps = step_times.len();
    step_times.sort();
    handler_times.sort();
    let mean = |times: &[Duration]| times.iter().sum::<Duration>() / times.len().max(1) as u32;
    let ms = |duration: Duration| duration.as_secs_f64() * 1000.0;
    eprintln!(
        "hover cycles: {cycles} x {} triggers, {opened} popover opens, {steps} steps, \
         {painted} frames painted, {:.3}ms total",
        triggers.len(),
        ms(elapsed)
    );
    eprintln!(
        "step (input + frame): p50 {:.4}ms p90 {:.4}ms mean {:.4}ms",
        ms(percentile(&step_times, 0.5)),
        ms(percentile(&step_times, 0.9)),
        ms(mean(&step_times))
    );
    eprintln!(
        "input dispatch only:  p50 {:.4}ms p90 {:.4}ms mean {:.4}ms",
        ms(percentile(&handler_times, 0.5)),
        ms(percentile(&handler_times, 0.9)),
        ms(mean(&handler_times))
    );
    eprintln!(
        "script publishes: {publishes} total, {:.2} per popover open; \
         {still_open} popovers open after the bridges fired",
        publishes as f64 / f64::from(opened.max(1))
    );
    eprintln!(
        "MESH_PERF metric=navigation_hover_step_p50_ms value={:.4}",
        ms(percentile(&step_times, 0.5))
    );
    eprintln!(
        "MESH_PERF metric=navigation_hover_dispatch_p50_ms value={:.4}",
        ms(percentile(&handler_times, 0.5))
    );
    eprintln!("MESH_PERF metric=navigation_hover_publishes value={publishes}");
    assert_eq!(
        still_open, 0,
        "every popover should close once its hover bridge expires"
    );
}
