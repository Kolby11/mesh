//! Live glass replay gate: the real `Shell` under the `glass` profile, on the
//! compositor in `WAYLAND_DISPLAY`, driven by scripted input and fixed service
//! state. See "Live glass replay" in `docs/performance-profiling.md`.

use super::*;
use crate::shell::CoreRequest;
use crate::shell::Shell;
use crate::shell::replay_hooks::{ReplayHarness, ReplayStep};
use crate::shell::runtime::profiling::CapturedRecord;
use mesh_core_debug::ProfilingStage;
use mesh_core_interaction::find_scrollable_at_with_limits;
use mesh_core_presentation::WindowEvent;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

const BOOT_MIN: Duration = Duration::from_secs(4);
const BOOT_LIMIT: Duration = Duration::from_secs(40);
const QUIET: Duration = Duration::from_millis(600);
const GAP: Duration = Duration::from_millis(500);
const CLOCK_WINDOW: Duration = Duration::from_secs(8);
const DOCK_TILE_MOVES: u32 = 6;
const DOCK_TILE_DWELL: Duration = Duration::from_millis(100);
const DOCK_SWEEP_TILES: u32 = 20;
const MOVE_INTERVAL: Duration = Duration::from_millis(16);
const DOCK_SWEEP_DURATION: Duration = DOCK_TILE_DWELL.saturating_mul(DOCK_SWEEP_TILES);

struct DockMotion {
    index: u32,
    tile: usize,
    jitter: f32,
    next_at: Duration,
}

fn dock_motion(elapsed: Duration) -> Option<DockMotion> {
    if elapsed >= DOCK_SWEEP_DURATION {
        return None;
    }
    let tile_step = (elapsed.as_nanos() / DOCK_TILE_DWELL.as_nanos()) as u32;
    let within_tile = elapsed - DOCK_TILE_DWELL * tile_step;
    let move_step =
        ((within_tile.as_nanos() / MOVE_INTERVAL.as_nanos()) as u32).min(DOCK_TILE_MOVES - 1);
    let index = tile_step * DOCK_TILE_MOVES + move_step;
    let step = tile_step as usize % 18;
    let tile = if step < 10 { step } else { 18 - step };
    let next_at = if move_step + 1 == DOCK_TILE_MOVES {
        DOCK_TILE_DWELL * (tile_step + 1)
    } else {
        DOCK_TILE_DWELL * tile_step + MOVE_INTERVAL * (move_step + 1)
    };
    Some(DockMotion {
        index,
        tile,
        jitter: move_step as f32 * 0.5,
        next_at,
    })
}

#[test]
fn dock_motion_keeps_absolute_dwell_and_coalesces_late_moves() {
    let start = dock_motion(Duration::ZERO).unwrap();
    assert_eq!((start.index, start.tile, start.jitter), (0, 0, 0.0));
    let late = dock_motion(Duration::from_millis(175)).unwrap();
    assert_eq!((late.index, late.tile, late.jitter), (10, 1, 2.0));
    assert_eq!(late.next_at, Duration::from_millis(180));
    let turnaround = dock_motion(Duration::from_millis(1050)).unwrap();
    assert_eq!(turnaround.tile, 8);
    assert_eq!(
        dock_motion(Duration::from_millis(1999)).unwrap().next_at,
        DOCK_SWEEP_DURATION
    );
    assert!(dock_motion(DOCK_SWEEP_DURATION).is_none());
}

const SPECTRUM_INTERVAL: Duration = Duration::from_millis(33);
const SPECTRUM_UPDATES: u32 = 120;
const SCROLL_FRAMES: u32 = 60;
const SCROLL_INTERVAL: Duration = Duration::from_millis(16);
const MOMENTUM_WINDOW: Duration = Duration::from_millis(700);
const OPEN_LIMIT: Duration = Duration::from_secs(10);
const SETTLE_AFTER_OPEN: Duration = Duration::from_millis(1200);

/// Keeps replay surfaces apart from a running desktop shell's in compositor
/// rules and tools such as `hyprctl layers`.
const REPLAY_NAMESPACE_SUFFIX: &str = ".mesh-replay";

const SURFACES: [&str; 4] = ["glass-bar", "glass-dock", "glass-dashboard", "settings"];

fn compositor_reachable() -> bool {
    let Some(display) = std::env::var_os("WAYLAND_DISPLAY") else {
        return false;
    };
    let path = std::path::PathBuf::from(&display);
    let path = if path.is_absolute() {
        path
    } else {
        match std::env::var_os("XDG_RUNTIME_DIR") {
            Some(dir) => std::path::PathBuf::from(dir).join(path),
            None => return false,
        }
    };
    std::os::unix::net::UnixStream::connect(path).is_ok()
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A private copy of `config/` pointing at the shipped modules, so nothing the shell
/// persists reaches the checkout or the user's home, and a private IPC socket,
/// so a running desktop shell is never replaced.
struct Sandbox(std::path::PathBuf);

impl Sandbox {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("mesh-live-replay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        copy_dir(&super::root().join("config"), &root.join("config"));
        let graph_path = root.join("config/module.json");
        let mut graph: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&graph_path).unwrap()).unwrap();
        // The manifest accepts only relative module directories.
        let up = "../".repeat(root.join("config").components().count() - 1);
        let modules = super::root().join("modules");
        graph["mesh"]["modulesDir"] =
            format!("{up}{}", modules.strip_prefix("/").unwrap().display()).into();
        std::fs::write(&graph_path, serde_json::to_string_pretty(&graph).unwrap()).unwrap();
        std::fs::create_dir_all(root.join("state")).unwrap();
        std::fs::create_dir_all(root.join("home")).unwrap();
        // SAFETY: the replay is the only test in the process, and no other
        // thread reads the environment before the shell starts.
        unsafe {
            std::env::set_var("MESH_BACKEND", "layer-shell");
            std::env::set_var("MESH_MODULE_GRAPH_PATH", root.join("config/module.json"));
            std::env::set_var("MESH_SETTINGS_PATH", root.join("config/settings.json"));
            std::env::set_var("MESH_IPC_SOCKET", root.join("state/mesh.sock"));
            std::env::set_var("MESH_HOME", root.join("home"));
            std::env::set_var("XDG_STATE_HOME", root.join("state"));
        }
        Self(root)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn service(interface: &str, payload: serde_json::Value) -> ServiceEvent {
    ServiceEvent::Updated {
        service: interface.into(),
        source_module: "@test/glass-services".into(),
        payload,
    }
}

#[test]
fn replay_apps_injection_before_first_dock_paint_populates_tiles() {
    let component = host("glass-dock");
    let id = component.surface_id().to_owned();
    assert!(component.last_widget_tree().is_none());
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.register_component(Box::new(component));
    let requests = shell
        .broadcast_service_event(service("mesh.apps", glass_apps_payload(10, 150)))
        .unwrap();
    shell.enqueue_effects(requests);
    shell.render_components().unwrap();
    let tree = shell.components[0].component.last_widget_tree().unwrap();
    for index in 0..10 {
        assert!(
            first_node_with_ref(tree, &format!("app-{index}")).is_some(),
            "{id}: injected application {index} is missing on first paint"
        );
    }
}

fn spectrum_bars(index: u32) -> Vec<f64> {
    (0..32)
        .map(|bar| f64::from((index * 7 + bar * 13) % 100) / 100.0)
        .collect()
}

fn fixed_service_state() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        ("mesh.apps", glass_apps_payload(10, 150)),
        (
            "mesh.system",
            serde_json::json!({"available":true,"cpu":37,"memory":52,"uptime":93_784}),
        ),
        (
            "mesh.media",
            serde_json::json!({"available":true,"playing":true,"title":"Track","artist":"Artist","position":42,"length":215}),
        ),
        (
            "mesh.weather",
            serde_json::json!({"available":true,"location":"Bratislava","temperature":17,"feels_like":16,"unit":"C","condition":"cloudy","is_day":true,"hourly":[
                {"time":1800000000,"temperature":18,"condition":"clear"},
                {"time":1800003600,"temperature":19,"condition":"cloudy"},
                {"time":1800007200,"temperature":17,"condition":"rain"},
                {"time":1800010800,"temperature":16,"condition":"rain"}]}),
        ),
        (
            "mesh.audio",
            serde_json::json!({"available":true,"percent":50,"muted":false}),
        ),
        (
            "mesh.brightness",
            serde_json::json!({"available":true,"level":70}),
        ),
        (
            "mesh.visualizer",
            serde_json::json!({"available":true,"active":false,"bars":spectrum_bars(0)}),
        ),
    ]
}

fn nth_with_class<'a>(node: &'a WidgetNode, class: &str, n: usize) -> Option<&'a WidgetNode> {
    fn collect<'a>(node: &'a WidgetNode, class: &str, out: &mut Vec<&'a WidgetNode>) {
        let has = node
            .attributes
            .get(mesh_core_elements::Prop::Class)
            .is_some_and(|c| c.split_whitespace().any(|t| t == class));
        if has {
            out.push(node);
        }
        node.children.iter().for_each(|c| collect(c, class, out));
    }
    let mut out = Vec::new();
    collect(node, class, &mut out);
    out.get(n).copied()
}

/// A point the Settings page scroller owns, not one of its nested lists.
fn scroll_point(tree: &mesh_core_elements::WidgetNode) -> Option<(f32, f32)> {
    let page = first_node_with_class_token(tree, "settings-scroll")?;
    let layout = &page.layout;
    (1..20)
        .flat_map(|row| {
            (1..8).map(move |column| {
                (
                    layout.x + layout.width * column as f32 / 8.0,
                    layout.y + layout.height * row as f32 / 20.0,
                )
            })
        })
        .find(|&(x, y)| {
            find_scrollable_at_with_limits(tree, x, y).is_some_and(|hit| hit.node_id == page.id)
        })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Boot,
    Clock,
    DockWarmup,
    DockSettle,
    DockSweep,
    DashboardOpen,
    DashboardSpectrum,
    DashboardClose,
    SettingsOpen,
    SettingsPage,
    SettingsScroll,
    SettingsMomentum,
    Done,
}

#[derive(Default)]
struct Outcome {
    started: Option<Instant>,
    windows: Vec<(&'static str, Instant, Instant)>,
    opens: Vec<(&'static str, Instant)>,
    failure: Option<String>,
    gpu: bool,
    surfaces: BTreeMap<&'static str, String>,
}

struct Replay {
    phase: Phase,
    /// When the current phase's stimulus began; a phase entered with a future
    /// instant does nothing until then, which leaves a quiet gap.
    phase_started: Instant,
    next_at: Instant,
    counter: u32,
    stage: u8,
    tiles: Vec<(f32, f32)>,
    scroll_point: (f32, f32),
    surface_ids: BTreeMap<&'static str, String>,
    outcome: Rc<RefCell<Outcome>>,
}

impl Replay {
    fn surface(&self, short: &str) -> Arc<str> {
        self.surface_ids[short].as_str().into()
    }

    fn enter(&mut self, phase: Phase, at: Instant) {
        self.phase = phase;
        self.phase_started = at;
        self.next_at = at;
        self.counter = 0;
        self.stage = 0;
    }

    fn close_window(&self, name: &'static str, end: Instant) {
        self.outcome
            .borrow_mut()
            .windows
            .push((name, self.phase_started, end));
    }

    fn fail(&mut self, shell: &mut Shell, message: String) -> ReplayStep {
        self.outcome.borrow_mut().failure.get_or_insert(message);
        shell.enqueue_effects(std::iter::once(CoreRequest::Shutdown));
        self.phase = Phase::Done;
        ReplayStep { wake_at: None }
    }

    fn records<'a>(&self, shell: &'a Shell) -> &'a [CapturedRecord] {
        shell.profiling.capture.as_deref().unwrap_or(&[])
    }

    fn last_render_at(&self, shell: &Shell, skip: &str) -> Option<Instant> {
        self.records(shell)
            .iter()
            .rev()
            .find_map(|record| match record {
                CapturedRecord::Stage {
                    at,
                    surface_id,
                    stage: ProfilingStage::TotalSurfaceRender,
                    ..
                } if !surface_id.contains(skip) => Some(*at),
                _ => None,
            })
    }

    fn presented_since(&self, shell: &Shell, short: &str, since: Instant) -> bool {
        let surface = &self.surface_ids[short];
        first_present_after(
            &self.records(shell),
            surface,
            since,
            self.outcome.borrow().gpu,
        )
        .is_some()
    }

    fn tree<'a>(&self, shell: &'a Shell, short: &str) -> Option<&'a WidgetNode> {
        let id = &self.surface_ids[short];
        shell
            .components
            .iter()
            .find(|runtime| &runtime.surface_id == id)
            .and_then(|runtime| runtime.component.last_widget_tree())
    }

    fn show(&self, shell: &mut Shell, short: &str) {
        shell.enqueue_effects(std::iter::once(CoreRequest::ShowSurface {
            surface_id: self.surface_ids[short].clone(),
        }));
    }

    fn hide(&self, shell: &mut Shell, short: &str) {
        shell.enqueue_effects(std::iter::once(CoreRequest::HideSurface {
            surface_id: self.surface_ids[short].clone(),
        }));
    }

    fn inject(&mut self, shell: &mut Shell, interface: &str, payload: serde_json::Value) -> bool {
        match shell.broadcast_service_event(service(interface, payload)) {
            Ok(requests) => {
                shell.enqueue_effects(requests);
                true
            }
            Err(error) => {
                self.fail(shell, format!("{interface} injection: {error}"));
                false
            }
        }
    }

    fn start(&mut self, shell: &mut Shell, now: Instant) -> bool {
        {
            let mut outcome = self.outcome.borrow_mut();
            outcome.started = Some(now);
            outcome.gpu = shell.presentation_engine.gpu_canvas_available();
        }
        for short in SURFACES {
            let found = shell
                .components
                .iter()
                .find(|runtime| runtime.surface_id.contains(short))
                .map(|runtime| runtime.surface_id.to_string());
            let Some(id) = found else {
                self.fail(shell, format!("the glass profile has no {short} surface"));
                return false;
            };
            self.surface_ids.insert(short, id);
        }
        self.outcome.borrow_mut().surfaces = self.surface_ids.clone();
        shell.profiling.capture = Some(Vec::new());
        shell.enqueue_effects(std::iter::once(CoreRequest::ToggleDebugProfiling));
        for (interface, payload) in fixed_service_state() {
            if !self.inject(shell, interface, payload) {
                return false;
            }
        }
        self.phase_started = now;
        true
    }

    /// Waits for `short` to present after its show request, then for its
    /// open animation to settle. Returns true once it has.
    fn opened(&mut self, shell: &mut Shell, now: Instant, short: &'static str) -> bool {
        match self.stage {
            0 => {
                self.phase_started = now;
                self.outcome.borrow_mut().opens.push((short, now));
                self.show(shell, short);
                self.stage = 1;
            }
            1 if self.presented_since(shell, short, self.phase_started) => {
                self.stage = 2;
                self.next_at = now + SETTLE_AFTER_OPEN;
            }
            1 if now - self.phase_started > OPEN_LIMIT => {
                self.fail(shell, format!("{short} never presented"));
            }
            2 if now >= self.next_at => return true,
            _ => {}
        }
        false
    }

    fn step(&mut self, shell: &mut Shell) -> ReplayStep {
        let now = Instant::now();
        if self.phase == Phase::Done {
            return ReplayStep { wake_at: None };
        }
        if self.outcome.borrow().started.is_none() && !self.start(shell, now) {
            return ReplayStep { wake_at: None };
        }
        if now < self.phase_started {
            return ReplayStep {
                wake_at: Some(self.phase_started),
            };
        }

        match self.phase {
            Phase::Boot => {
                let elapsed = now - self.phase_started;
                let quiet = self
                    .last_render_at(shell, "glass-bar")
                    .is_none_or(|at| now - at > QUIET);
                if elapsed > BOOT_LIMIT {
                    return self.fail(shell, "boot did not settle".into());
                } else if elapsed > BOOT_MIN && quiet {
                    self.enter(Phase::Clock, now);
                }
            }
            Phase::Clock => {
                if now - self.phase_started >= CLOCK_WINDOW {
                    self.close_window("clock", now);
                    let Some(tree) = self.tree(shell, "glass-dock") else {
                        return self.fail(shell, "the dock has no tree".into());
                    };
                    let mut tiles = Vec::new();
                    for index in 0..10 {
                        let bounds = first_node_with_ref(tree, &format!("app-{index}"))
                            .and_then(|node| node.mesh_key())
                            .and_then(|key| find_node_bounds_by_key(tree, key, 0.0, 0.0));
                        let Some((left, top, right, bottom)) = bounds else {
                            let found = first_node_with_ref(tree, &format!("app-{index}"));
                            fn refs(node: &WidgetNode, out: &mut Vec<String>) {
                                if let Some(r) = node.attributes.get(mesh_core_elements::Prop::Ref)
                                {
                                    out.push(r.to_string());
                                }
                                node.children.iter().for_each(|c| refs(c, out));
                            }
                            let mut all = Vec::new();
                            refs(tree, &mut all);
                            eprintln!("refs: {all:?}");
                            fn tiles(node: &WidgetNode) -> usize {
                                usize::from(
                                    node.attributes
                                        .get(mesh_core_elements::Prop::Class)
                                        .is_some_and(|c| c.split_whitespace().any(|t| t == "tile")),
                                ) + node.children.iter().map(tiles).sum::<usize>()
                            }
                            eprintln!("tile nodes: {}", tiles(tree));
                            return self.fail(
                                shell,
                                format!(
                                    "dock tile app-{index} is missing (node {}, key {:?}, {} nodes, latest apps state {:?})",
                                    found.is_some(),
                                    found.and_then(|node| node.mesh_key()),
                                    count_nodes(tree),
                                    shell.latest_service_state.get("mesh.apps").map(|s| s.state.to_string().chars().take(120).collect::<String>())
                                ),
                            );
                        };
                        tiles.push(((left + right) / 2.0, (top + bottom) / 2.0));
                    }
                    self.tiles = tiles;
                    self.enter(Phase::DockWarmup, now + GAP);
                }
            }
            Phase::DockWarmup | Phase::DockSweep => {
                if let Some(motion) = dock_motion(now - self.phase_started) {
                    if self.counter <= motion.index {
                        let (x, y) = self.tiles[motion.tile];
                        shell
                            .pending_wayland_events
                            .push_back(WindowEvent::PointerMove {
                                surface_id: self.surface("glass-dock"),
                                x: x + motion.jitter,
                                y,
                            });
                        self.counter = motion.index + 1;
                    }
                    self.next_at = self.phase_started + motion.next_at;
                } else {
                    shell
                        .pending_wayland_events
                        .push_back(WindowEvent::PointerLeave {
                            surface_id: self.surface("glass-dock"),
                        });
                    if self.phase == Phase::DockWarmup {
                        self.enter(Phase::DockSettle, now);
                        self.next_at = now + QUIET;
                    } else {
                        self.close_window("dock_hover", now);
                        self.enter(Phase::DashboardOpen, now + GAP);
                    }
                }
            }
            Phase::DockSettle => {
                let surface = &self.surface_ids["glass-dock"];
                let last_paint = self
                    .records(shell)
                    .iter()
                    .rev()
                    .find_map(|record| match record {
                        CapturedRecord::Stage {
                            at,
                            surface_id,
                            stage: ProfilingStage::TotalSurfaceRender,
                            ..
                        } if surface_id == surface => Some(*at),
                        _ => None,
                    })
                    .unwrap_or(self.phase_started);
                if now.saturating_duration_since(last_paint) >= QUIET {
                    self.enter(Phase::DockSweep, now + GAP);
                } else if now - self.phase_started > BOOT_LIMIT {
                    return self.fail(shell, "dock warm-up did not settle".into());
                } else {
                    self.next_at = last_paint + QUIET;
                }
            }
            Phase::DashboardOpen => {
                if self.opened(shell, now, "glass-dashboard") {
                    self.close_window("dashboard_open", now);
                    self.enter(Phase::DashboardSpectrum, now);
                }
            }
            Phase::DashboardSpectrum => {
                if self.counter >= SPECTRUM_UPDATES {
                    self.close_window("dashboard_spectrum", now);
                    self.enter(Phase::DashboardClose, now);
                } else if now >= self.next_at {
                    let payload = serde_json::json!({"available":true,"active":true,"bars":spectrum_bars(self.counter)});
                    if !self.inject(shell, "mesh.visualizer", payload) {
                        return ReplayStep { wake_at: None };
                    }
                    self.counter += 1;
                    self.next_at = now + SPECTRUM_INTERVAL;
                }
            }
            Phase::DashboardClose => {
                let payload =
                    serde_json::json!({"available":true,"active":false,"bars":spectrum_bars(0)});
                if !self.inject(shell, "mesh.visualizer", payload) {
                    return ReplayStep { wake_at: None };
                }
                self.hide(shell, "glass-dashboard");
                self.enter(Phase::SettingsOpen, now + GAP * 2);
            }
            Phase::SettingsOpen => {
                if self.opened(shell, now, "settings") {
                    self.close_window("settings_open", now);
                    self.enter(Phase::SettingsPage, now + GAP);
                }
            }
            Phase::SettingsPage => {
                // The default page is too short to scroll; Appearance is the
                // long page the D9 scroll gate measures.
                if self.stage == 0 {
                    let center = self.tree(shell, "settings").and_then(|tree| {
                        let key = nth_with_class(tree, "settings-nav-item", 3)?.mesh_key()?;
                        let (l, t, r, b) = find_node_bounds_by_key(tree, key, 0.0, 0.0)?;
                        Some(((l + r) / 2.0, (t + b) / 2.0))
                    });
                    let Some((x, y)) = center else {
                        return self.fail(shell, "Settings has no Appearance nav item".into());
                    };
                    let surface_id = self.surface("settings");
                    let events = &mut shell.pending_wayland_events;
                    events.push_back(WindowEvent::PointerMove {
                        surface_id: surface_id.clone(),
                        x,
                        y,
                    });
                    for pressed in [true, false] {
                        events.push_back(WindowEvent::PointerButton {
                            surface_id: surface_id.clone(),
                            x,
                            y,
                            button: mesh_core_presentation::PRIMARY_POINTER_BUTTON,
                            pressed,
                        });
                    }
                    self.stage = 1;
                    self.next_at = now + Duration::from_millis(2500);
                } else if now >= self.next_at {
                    let Some(point) = self.tree(shell, "settings").and_then(scroll_point) else {
                        return self.fail(shell, "Settings has no scrollable page point".into());
                    };
                    self.scroll_point = point;
                    shell
                        .pending_wayland_events
                        .push_back(WindowEvent::PointerMove {
                            surface_id: self.surface("settings"),
                            x: point.0,
                            y: point.1,
                        });
                    // Hover transitions settle before the gesture starts.
                    self.enter(Phase::SettingsScroll, now + GAP * 2);
                }
            }
            Phase::SettingsScroll => {
                if self.counter >= SCROLL_FRAMES {
                    // Momentum belongs to the same scroll window.
                    self.phase = Phase::SettingsMomentum;
                    self.next_at = now;
                } else if now >= self.next_at {
                    let (x, y) = self.scroll_point;
                    shell
                        .pending_wayland_events
                        .push_back(WindowEvent::TwoFingerScroll {
                            surface_id: self.surface("settings"),
                            x,
                            y,
                            dx: 0.0,
                            dy: -4.0,
                        });
                    self.counter += 1;
                    self.next_at = now + SCROLL_INTERVAL;
                }
            }
            Phase::SettingsMomentum => {
                if self.stage == 0 {
                    self.stage = 1;
                    self.next_at = now + MOMENTUM_WINDOW;
                } else if now >= self.next_at {
                    self.close_window("settings_scroll", now);
                    self.hide(shell, "settings");
                    shell.enqueue_effects(std::iter::once(CoreRequest::Shutdown));
                    self.phase = Phase::Done;
                    return ReplayStep { wake_at: None };
                }
            }
            Phase::Done => {}
        }
        ReplayStep {
            wake_at: Some(self.next_at.clamp(now, now + Duration::from_millis(50))),
        }
    }
}

#[derive(Default, Clone)]
struct Frames {
    render_ms: Vec<f64>,
    visits: Vec<BTreeMap<String, u64>>,
}

fn frames_in(records: &[CapturedRecord], surface: &str, from: Instant, to: Instant) -> Frames {
    let mut frames = Frames::default();
    for record in records {
        match record {
            CapturedRecord::Stage {
                at,
                surface_id,
                stage: ProfilingStage::TotalSurfaceRender,
                micros,
                ..
            } if surface_id == surface && (from..to).contains(at) => {
                frames.render_ms.push(*micros as f64 / 1000.0);
            }
            CapturedRecord::Invalidation {
                at,
                surface_id,
                node_visits,
            } if surface_id == surface && (from..to).contains(at) && !node_visits.is_empty() => {
                frames.visits.push(node_visits.clone());
            }
            _ => {}
        }
    }
    frames
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * fraction).round() as usize]
}

fn slug(surface: &str) -> String {
    surface
        .trim_start_matches("@mesh/")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn metric(name: &str, value: f64) {
    println!("MESH_PERF metric={name} value={value:.6}");
}

fn mean_visits(frames: &[BTreeMap<String, u64>]) -> BTreeMap<String, f64> {
    let mut totals = BTreeMap::<String, u64>::new();
    for visits in frames {
        for (pass, count) in visits {
            *totals.entry(pass.clone()).or_default() += count;
        }
    }
    totals
        .into_iter()
        .map(|(pass, total)| (pass, total as f64 / frames.len() as f64))
        .collect()
}

fn report_window(records: &[CapturedRecord], workload: &str, from: Instant, to: Instant) {
    let mut surfaces: Vec<&str> = records
        .iter()
        .filter_map(|record| match record {
            CapturedRecord::Stage {
                at,
                surface_id,
                stage: ProfilingStage::TotalSurfaceRender,
                ..
            } if (from..to).contains(at) => Some(surface_id.as_str()),
            _ => None,
        })
        .collect();
    surfaces.sort_unstable();
    surfaces.dedup();
    for surface in surfaces {
        let frames = frames_in(records, surface, from, to);
        let mut sorted = frames.render_ms.clone();
        sorted.sort_by(f64::total_cmp);
        let name = format!("live_{workload}_{}", slug(surface));
        let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
        metric(&format!("{name}_frames"), sorted.len() as f64);
        metric(&format!("{name}_render_mean_ms"), mean);
        metric(&format!("{name}_render_p50_ms"), percentile(&sorted, 0.5));
        metric(&format!("{name}_render_p90_ms"), percentile(&sorted, 0.9));
        let mut stages = BTreeMap::<&str, u64>::new();
        for record in records {
            if let CapturedRecord::Stage {
                at,
                surface_id,
                stage,
                micros,
                trigger,
            } = record
                && surface_id == surface
                && (from..to).contains(at)
                && !trigger.as_deref().is_some_and(|trigger| {
                    trigger.starts_with("attribution:") || trigger.starts_with("waste:")
                })
            {
                *stages.entry(stage.label()).or_default() += micros;
                if *stage == ProfilingStage::Paint {
                    let label = if trigger.as_deref() == Some("deferred_raster") {
                        "cpu_raster"
                    } else {
                        "paint_record_or_immediate"
                    };
                    *stages.entry(label).or_default() += micros;
                }
            }
        }
        for (stage, micros) in stages {
            metric(
                &format!("{name}_stage_{stage}_mean_ms"),
                micros as f64 / 1000.0 / sorted.len() as f64,
            );
        }

        if !frames.visits.is_empty() {
            metric(&format!("{name}_visit_frames"), frames.visits.len() as f64);
            for (pass, mean) in mean_visits(&frames.visits) {
                metric(&format!("{name}_visits_{pass}"), mean);
            }
        }
    }
}

fn first_after(
    records: &[CapturedRecord],
    surface: &str,
    stage: ProfilingStage,
    since: Instant,
) -> Option<Instant> {
    records.iter().find_map(|record| match record {
        CapturedRecord::Stage {
            at,
            surface_id,
            stage: s,
            ..
        } if surface_id == surface && *s == stage && *at >= since => Some(*at),
        _ => None,
    })
}

fn first_present_after(
    records: &[CapturedRecord],
    surface: &str,
    since: Instant,
    gpu: bool,
) -> Option<Instant> {
    if !gpu {
        return first_after(records, surface, ProfilingStage::PresentCommit, since);
    }
    records.iter().find_map(|record| match record {
        CapturedRecord::SceneCompletion {
            at,
            surface_id,
            presented: true,
        } if surface_id == surface && *at >= since => Some(*at),
        _ => None,
    })
}

#[test]
fn replay_first_present_waits_for_a_successful_gpu_commit() {
    let requested = Instant::now();
    let submitted = requested + Duration::from_millis(1);
    let committed = requested + Duration::from_millis(5);
    let mut records = vec![CapturedRecord::Stage {
        at: submitted,
        surface_id: "dock".into(),
        stage: ProfilingStage::PresentCommit,
        micros: 1,
        trigger: None,
    }];
    assert_eq!(
        first_present_after(&records, "dock", requested, false),
        Some(submitted)
    );
    assert_eq!(first_present_after(&records, "dock", requested, true), None);
    for (surface, at, presented) in [
        ("dock", requested - Duration::from_millis(1), true),
        ("other", submitted, true),
        ("dock", submitted, false),
    ] {
        records.push(CapturedRecord::SceneCompletion {
            at,
            surface_id: surface.into(),
            presented,
        });
    }
    assert_eq!(first_present_after(&records, "dock", requested, true), None);
    records.push(CapturedRecord::SceneCompletion {
        at: committed,
        surface_id: "dock".into(),
        presented: true,
    });
    assert_eq!(
        first_present_after(&records, "dock", requested, true),
        Some(committed)
    );
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum FrameClass {
    HoverChange,
    AnimationOnly,
    Full,
}

fn classify(visits: &BTreeMap<String, u64>) -> FrameClass {
    let at = |pass: &str| visits.get(pass).copied().unwrap_or(0);
    if at("annotation") >= 200 {
        FrameClass::Full
    } else if at("restyle_scan") > 0 {
        FrameClass::HoverChange
    } else {
        FrameClass::AnimationOnly
    }
}

type Envelope = BTreeMap<FrameClass, BTreeMap<String, u64>>;

fn envelope(frames: &[BTreeMap<String, u64>]) -> Envelope {
    let mut envelope = Envelope::new();
    for visits in frames {
        let class = envelope.entry(classify(visits)).or_default();
        for (pass, count) in visits {
            let max = class.entry(pass.clone()).or_default();
            *max = (*max).max(*count);
        }
    }
    envelope
}

/// CPU counterpart of the live sweep, with the same wall-clock input deadlines.
/// Raster work may drop sampling frames, but it cannot stretch the tile dwell.
fn cpu_dock_fixture_visits() -> Vec<BTreeMap<String, u64>> {
    const WIDTH: u32 = 2880;
    const HEIGHT: u32 = 180;
    let theme = theme();
    let mut component = host("glass-dock");
    update(&mut component, "mesh.apps", glass_apps_payload(10, 150));
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
    let tiles = settled_dock_tiles(&mut component, &theme, extent, &mut pixels);
    let mut steady = Vec::new();
    for run in 0..2 {
        leave_dock(&mut component, &theme, extent, &mut pixels);
        component.set_profiling_enabled(true);
        let _ = component.take_profiling_records();
        let started = Instant::now();
        let mut previous_move = None;
        while let Some(motion) = dock_motion(started.elapsed()) {
            if previous_move != Some(motion.index) {
                let (left, top, right, bottom) = tiles[motion.tile];
                component
                    .handle_input(
                        &theme,
                        WIDTH,
                        HEIGHT,
                        ComponentInput::PointerMove {
                            x: (left + right) / 2.0 + motion.jitter,
                            y: (top + bottom) / 2.0,
                        },
                    )
                    .unwrap();
                previous_move = Some(motion.index);
            }
            if component.wants_render() {
                component
                    .paint_raster(&theme, extent, &mut pixels, 1.0)
                    .unwrap();
                if let Some(snapshot) = component.take_invalidation_snapshot()
                    && !snapshot.node_visits.is_empty()
                    && run == 1
                {
                    steady.push(snapshot.node_visits);
                }
            }
            let elapsed = started.elapsed();
            let next_tick =
                MOVE_INTERVAL * ((elapsed.as_nanos() / MOVE_INTERVAL.as_nanos()) as u32 + 1);
            std::thread::sleep(
                (started + next_tick.min(DOCK_SWEEP_DURATION))
                    .saturating_duration_since(Instant::now()),
            );
        }
        component.set_profiling_enabled(false);
    }
    steady
}

// WAYLAND_DISPLAY=wayland-1 taskset -c 6 cargo test -p mesh-core-shell --release --lib \
//   live_glass_replay -- --ignored --nocapture --test-threads=1
#[test]
#[ignore = "live compositor: replays the glass workloads against the real shell"]
fn live_glass_replay() {
    assert!(!cfg!(debug_assertions), "run with --release");
    if !compositor_reachable() {
        eprintln!("skipping live glass replay: no reachable compositor on WAYLAND_DISPLAY");
        return;
    }
    let cpu_visits = cpu_dock_fixture_visits();
    let cpu_envelope = envelope(&cpu_visits);
    eprintln!(
        "cpu fixture dock: {} steady paints, classes {:?}",
        cpu_visits.len(),
        cpu_envelope.keys().collect::<Vec<_>>()
    );

    let _sandbox = Sandbox::new();
    let mut shell = Shell::new();
    if !shell.presentation_engine.supports_blocking_dispatch() {
        eprintln!("skipping live glass replay: the Wayland surface backend did not initialise");
        return;
    }
    shell
        .presentation_engine
        .set_layer_namespace_suffix(REPLAY_NAMESPACE_SUFFIX);
    assert_eq!(
        shell.active_profile_id.as_deref(),
        Some("glass"),
        "the replay needs config/active-profile to name the glass profile: {:?}",
        shell.composition_mode
    );

    let now = Instant::now();
    let outcome = Rc::new(RefCell::new(Outcome::default()));
    let mut replay = Replay {
        phase: Phase::Boot,
        phase_started: now,
        next_at: now,
        counter: 0,
        stage: 0,
        tiles: Vec::new(),
        scroll_point: (0.0, 0.0),
        surface_ids: BTreeMap::new(),
        outcome: Rc::clone(&outcome),
    };
    shell.replay = Some(ReplayHarness {
        suppress_backends: true,
        driver: Box::new(move |shell| replay.step(shell)),
        wake_at: None,
    });
    shell.run().expect("the replayed shell must exit cleanly");

    let outcome = outcome.borrow();
    if let Some(failure) = &outcome.failure {
        panic!("live glass replay failed: {failure}");
    }
    let records = shell.profiling.capture.clone().unwrap_or_default();
    let started = outcome.started.expect("the replay never started");
    eprintln!("raster path: {}", shell.presentation_engine.render_path());
    metric("live_gpu_raster", f64::from(u8::from(outcome.gpu)));

    let mut boot_surfaces: Vec<&str> = records
        .iter()
        .filter_map(|record| match record {
            CapturedRecord::Stage { surface_id, .. } => Some(surface_id.as_str()),
            _ => None,
        })
        .collect();
    boot_surfaces.sort_unstable();
    boot_surfaces.dedup();
    for surface in boot_surfaces {
        if let Some(at) = first_present_after(&records, surface, started, outcome.gpu) {
            metric(
                &format!("live_boot_{}_first_present_ms", slug(surface)),
                (at - started).as_secs_f64() * 1000.0,
            );
        }
    }
    for (short, requested) in &outcome.opens {
        let surface = outcome.surfaces[short].clone();
        for (stage, label) in [
            (ProfilingStage::PresentCommit, "first_present"),
            (ProfilingStage::TotalSurfaceRender, "first_render"),
        ] {
            let at = if stage == ProfilingStage::PresentCommit {
                first_present_after(&records, &surface, *requested, outcome.gpu)
            } else {
                first_after(&records, &surface, stage, *requested)
            };
            if let Some(at) = at {
                metric(
                    &format!("live_open_{}_{label}_ms", slug(&surface)),
                    (at - *requested).as_secs_f64() * 1000.0,
                );
            }
        }
    }
    for (name, from, to) in &outcome.windows {
        report_window(&records, name, *from, *to);
    }

    let (_, from, to) = outcome
        .windows
        .iter()
        .find(|(name, ..)| *name == "dock_hover")
        .expect("dock window");
    let dock = outcome.surfaces["glass-dock"].clone();
    let live = frames_in(&records, &dock, *from, *to);
    let mut outliers = 0usize;
    let mut classes = BTreeMap::<FrameClass, usize>::new();
    for visits in &live.visits {
        let class = classify(visits);
        *classes.entry(class).or_default() += 1;
        let within = cpu_envelope.get(&class).is_some_and(|bound| {
            visits
                .iter()
                .all(|(pass, count)| *count <= bound.get(pass).copied().unwrap_or(0))
        });
        if !within {
            outliers += 1;
            if outliers <= 3 {
                eprintln!("dock outlier ({class:?}): {visits:?}");
            }
        }
    }
    for (class, bound) in &cpu_envelope {
        eprintln!("cpu envelope {class:?}: {bound:?}");
    }
    for (class, count) in &classes {
        eprintln!("live dock frames {class:?}: {count}");
    }
    metric("live_dock_visit_outliers", outliers as f64);
    metric("live_dock_visit_frames", live.visits.len() as f64);
    assert!(
        live.visits.len() >= 30,
        "the dock sweep must paint measurable frames: {}",
        live.visits.len()
    );
    assert_eq!(
        outliers, 0,
        "live dock frames visited nodes outside the CPU fixture's frame classes"
    );
}

mod popup_validation;
