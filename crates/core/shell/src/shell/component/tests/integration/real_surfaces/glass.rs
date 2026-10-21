use super::*;
use crate::shell::ComponentContext;
use crate::shell::component::catalog::FrontendCatalog;
use mesh_core_diagnostics::Diagnostics;
use mesh_core_frontend::compile_frontend_module;
use mesh_core_theme::{Theme, ThemeModule, ThemeModuleLayer, ThemeSourceHandle};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

fn theme() -> Theme {
    let pack = mesh_core_theme::load_theme_from_source(
        &ThemeSourceHandle::new(root().join("modules/themes/violet-glass"), "theme.css").unwrap(),
        "violet-glass",
        "Violet Glass",
    )
    .unwrap();
    let material: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("modules/libraries/glass-material/module.json"))
            .unwrap(),
    )
    .unwrap();
    let material: ThemeModule = serde_json::from_value(material["mesh"]["theme"].clone()).unwrap();
    let mut layers = vec![ThemeModuleLayer {
        module_id: "@mesh/glass-material".into(),
        module: material,
        families: Vec::new(),
    }];
    for module in ["glass-bar", "glass-widgets", "glass-dock"] {
        layers.push(ThemeModuleLayer {
            module_id: format!("@mesh/{module}"),
            module: ThemeModule::default(),
            families: vec!["@mesh/glass-material".into()],
        });
    }
    Theme::compose_layers(
        &default_theme(),
        &pack,
        "@mesh/violet-glass",
        "dark",
        layers,
        &HashMap::new(),
    )
    .unwrap()
}

const GLASS_MODULES: [&str; 6] = [
    "glass-bar",
    "glass-widgets",
    "glass-dock",
    "glass-dashboard",
    "profile-switcher",
    "profile-menu",
];

/// Compiled glass modules, contracts and locale, shared by every surface the
/// tests mount. Building it compiles every module, so workloads that time a
/// surface open prepare it once outside the measured region.
struct GlassCatalog {
    entries: HashMap<String, FrontendCatalogEntry>,
    interfaces: InterfaceCatalog,
    locale: mesh_core_locale::LocaleEngine,
}

impl GlassCatalog {
    fn new() -> Self {
        let mut entries = HashMap::new();
        for module in GLASS_MODULES {
            let dir = root().join("modules/frontend").join(module);
            let manifest = mesh_core_module::manifest::load_canonical_manifest(&dir)
                .unwrap()
                .manifest;
            let compiled = compile_frontend_module(&manifest, &dir)
                .unwrap_or_else(|error| panic!("{module}: {error}"));
            entries.insert(
                format!("@mesh/{module}"),
                FrontendCatalogEntry {
                    module_dir: dir,
                    compiled: compiled.into(),
                },
            );
        }
        let mut interfaces = InterfaceCatalog::default();
        for interface in [
            "apps",
            "audio",
            "brightness",
            "media",
            "network",
            "notifications",
            "power",
            "recorder",
            "system",
            "visualizer",
            "weather",
            "wm",
        ] {
            let path = root()
                .join("modules/interfaces")
                .join(interface)
                .join("module.json");
            let json: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            let contract = &json["mesh"]["interface"];
            interfaces.register_contract(
                mesh_core_service::contract::parse_interface_contract(
                    contract["name"].as_str().unwrap(),
                    contract["version"].as_str().unwrap(),
                    &contract["contract"],
                )
                .unwrap(),
            );
            interfaces.register_provider(mesh_core_service::InterfaceProvider {
                interface: contract["name"].as_str().unwrap().into(),
                version: Some(contract["version"].as_str().unwrap().into()),
                base_module: None,
                provider_module: "@test/glass-services".into(),
                backend_name: interface.into(),
                priority: 1,
            });
        }
        interfaces.register_contract(
            mesh_core_service::contract::parse_interface_contract(
                "mesh.packages",
                "1.0",
                &serde_json::json!({"state": [
                {"name":"profiles","type":"string[]"}, {"name":"active_profile","type":"string"}
            ], "methods": [{"name":"switch_profile", "args":[{"name":"profile_id","type":"string"}]}]}),
            )
            .unwrap(),
        );
        let mut locale = mesh_core_locale::LocaleEngine::new("en");
        for module in ["glass-bar", "glass-dock", "glass-dashboard"] {
            let path = root()
                .join("modules/frontend")
                .join(module)
                .join("config/i18n/en.json");
            let catalog: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
            locale.load_module_catalog(
                &format!("@mesh/{module}"),
                mesh_core_locale::compile_catalog("en", &catalog),
            );
        }
        Self {
            entries,
            interfaces,
            locale,
        }
    }

    fn mount(&self, name: &str) -> FrontendSurfaceComponent {
        let id = format!("@mesh/{name}");
        let entry = self.entries[&id].clone();
        let catalog = FrontendCatalog {
            modules: self.entries.clone(),
            ..Default::default()
        };
        let mut component = FrontendSurfaceComponent::new_for_test(
            (*entry.compiled).clone(),
            entry.module_dir,
            catalog,
            self.interfaces.clone(),
            test_settings_store(),
        );
        component.locale = self.locale.snapshot();
        component
            .mount(ComponentContext {
                component_id: id.clone(),
                surface_id: id.clone(),
                diagnostics: Diagnostics::new(id),
            })
            .unwrap();
        component.visible = true;
        component
    }
}

fn host(name: &str) -> FrontendSurfaceComponent {
    GlassCatalog::new().mount(name)
}

fn paint(component: &mut FrontendSurfaceComponent, width: u32, height: u32) -> PixelBuffer {
    let mut pixels = PixelBuffer::new(width, height);
    component
        .paint_raster(
            &theme(),
            SurfaceExtent::unpadded(width, height),
            &mut pixels,
            1.0,
        )
        .unwrap();
    let text = rendered_text(component);
    assert!(
        !text
            .iter()
            .any(|text| text.contains("frontend runtime") || text.contains("script error")),
        "{text:?}"
    );
    pixels
}

fn reference(component: &FrontendSurfaceComponent, name: &str) -> String {
    first_node_with_ref(component.last_tree.as_ref().unwrap(), name)
        .unwrap_or_else(|| panic!("missing {name}"))
        .mesh_key()
        .unwrap()
        .to_owned()
}

fn press(component: &mut FrontendSurfaceComponent, name: &str, width: u32, height: u32) {
    component.focused_key = Some(reference(component, name));
    component
        .handle_input(
            &theme(),
            width,
            height,
            ComponentInput::KeyPressed {
                key: "Enter".into(),
                modifiers: Default::default(),
            },
        )
        .unwrap();
    paint(component, width, height);
}

fn update(component: &mut FrontendSurfaceComponent, service: &str, payload: serde_json::Value) {
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: service.into(),
            source_module: "@test/glass-services".into(),
            payload,
        })
        .unwrap();
}

#[test]
fn liquid_glass_calendar_and_controls_are_promoted_local_popovers() {
    let mut component = host("glass-bar");
    paint(&mut component, 1920, 100);
    assert!(component.child_surface_requests().is_empty());
    press(&mut component, "glassCalendar", 1920, 100);
    let requests = component.child_surface_requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].content_size.0 >= 360);
    assert!(requests[0].content_size.1 >= 300);
    component
        .handle_input(
            &theme(),
            1920,
            100,
            ComponentInput::KeyPressed {
                key: "Escape".into(),
                modifiers: Default::default(),
            },
        )
        .unwrap();
    paint(&mut component, 1920, 100);
    assert!(component.child_surface_requests().is_empty());
    update(
        &mut component,
        "mesh.audio",
        serde_json::json!({"available":true,"percent":50,"muted":false}),
    );
    update(
        &mut component,
        "mesh.brightness",
        serde_json::json!({"available":true,"level":70}),
    );
    press(&mut component, "glassControls", 1920, 100);
    assert_eq!(component.child_surface_requests().len(), 1);
    preview(&mut component, 1920, 100, "controls");
    for (reference_name, interface, command, field) in [
        ("glassVolume", "mesh.audio", "set_volume", "percent"),
        ("glassBrightness", "mesh.brightness", "set", "level"),
    ] {
        let key = reference(&component, reference_name);
        let tree = component.last_tree.take().unwrap();
        let requests = component
            .call_node_handler(&tree, &key, "change", &[serde_json::json!(37)])
            .unwrap();
        component.last_tree = Some(tree);
        assert!(
            requests
                .iter()
                .any(|request| service_request_parts(request).is_some_and(
                    |(service, method, args)| service == interface
                        && method == command
                        && args[field] == 37
                )),
            "{requests:?}"
        );
    }
}

#[test]
fn liquid_glass_dock_search_and_neighbor_magnification_work() {
    let mut component = host("glass-dock");
    update(
        &mut component,
        "mesh.apps",
        serde_json::json!({"available":true,"apps":[
            {"id":"browser","name":"Browser","icon":"","pinned":true,"running":true,"focused":true,"windows":1},
            {"id":"terminal","name":"Terminal","icon":"","pinned":true,"running":false,"focused":false,"windows":0}
        ]}),
    );
    paint(&mut component, 1920, 180);
    let browser = reference(&component, "browser");
    let (left, top, right, bottom) =
        find_node_bounds_by_key(component.last_tree.as_ref().unwrap(), &browser, 0.0, 0.0).unwrap();
    component
        .handle_input(
            &theme(),
            1920,
            180,
            ComponentInput::PointerMove {
                x: (left + right) / 2.0,
                y: (top + bottom) / 2.0,
            },
        )
        .unwrap();
    paint(&mut component, 1920, 180);
    assert!(first_node_by_class(component.last_tree.as_ref().unwrap(), "tile-neighbor").is_some());
    press(&mut component, "glassLauncher", 1920, 180);
    assert_eq!(component.child_surface_requests().len(), 1);
    preview(&mut component, 1920, 180, "launcher");
    let tree = component.last_tree.as_ref().unwrap();
    let search = first_node_by_class(tree, "search")
        .expect("search input")
        .mesh_key()
        .unwrap()
        .to_owned();
    component.focused_key = Some(search);
    component
        .handle_input(
            &theme(),
            1920,
            180,
            ComponentInput::TextInput {
                text: "term".into(),
            },
        )
        .unwrap();
    paint(&mut component, 1920, 180);
    let texts = rendered_text(&component);
    assert!(texts.iter().any(|text| text == "Terminal"));
    assert!(!texts.iter().any(|text| text == "Browser"));
}

#[test]
fn liquid_glass_desktop_cards_fit_laptop_and_large_outputs() {
    for width in [640, 800, 960, 1200, 1440, 1920, 2880] {
        let mut component = host("glass-widgets");
        update(
            &mut component,
            "mesh.weather",
            serde_json::json!({"available":true,"location":"Bratislava","temperature":17,"feels_like":16,"unit":"C","condition":"cloudy","is_day":true,"hourly":[
                {"time":1800000000,"temperature":18,"condition":"clear"},
                {"time":1800003600,"temperature":19,"condition":"cloudy"},
                {"time":1800007200,"temperature":17,"condition":"rain"},
                {"time":1800010800,"temperature":16,"condition":"rain"}
            ]}),
        );
        paint(&mut component, width, 1000);
        if width == 1920 {
            preview(&mut component, width, 1000, "widgets");
        }
        let tree = component.last_tree.as_ref().unwrap();
        let media = first_node_by_class(tree, "slot").expect("media slot");
        let weather = first_node_by_class(tree, "slot-weather").expect("weather slot");
        for card in [media, weather] {
            assert!(card.layout.x >= 0.0);
            assert!(
                card.layout.x + card.layout.width <= width as f32 + 1.0,
                "{width}: {:?}",
                card.layout
            );
        }
        let media_card = first_node_by_class(tree, "media-card").unwrap();
        let weather_card = first_node_by_class(tree, "weather-card").unwrap();
        assert_eq!(
            media_card.layout.width,
            if width <= 800 { 360.0 } else { 480.0 }
        );
        assert_eq!(
            weather_card.layout.width,
            if width <= 800 { 360.0 } else { 384.0 }
        );
        if width <= 960 {
            assert!(weather.layout.y >= media.layout.y + media.layout.height);
        } else {
            assert!(weather.layout.x >= media.layout.x + media.layout.width);
        }
        let texts = rendered_text(&component);
        assert!(texts.iter().any(|text| text == "Bratislava"), "{texts:?}");
    }
}

fn preview(component: &mut FrontendSurfaceComponent, width: u32, height: u32, label: &str) {
    let Some(dir) = std::env::var_os("MESH_GLASS_PREVIEW_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(750));
    let pixels = paint(component, width, height);
    std::fs::write(
        dir.join(format!("{label}-tree.txt")),
        format!("{:#?}", component.last_tree),
    )
    .unwrap();
    std::fs::write(
        dir.join(format!("{label}-{width}x{height}.bgra")),
        pixels.data(),
    )
    .unwrap();
    for request in component.child_surface_requests() {
        let (w, h) = request.content_size;
        let mut pixels = PixelBuffer::new(w, h);
        component
            .paint_child_surface_raster(&request.node_key, &mut pixels, 1.0, (0, 0), false)
            .unwrap();
        std::fs::write(
            dir.join(format!("{label}-popup-{w}x{h}.bgra")),
            pixels.data(),
        )
        .unwrap();
    }
}

// cargo test -p mesh-core-shell --release --lib glass_bar_clock_tick_profile -- --ignored --nocapture
#[test]
#[ignore = "release-only glass-bar clock-tick frame-cost profile"]
fn glass_bar_clock_tick_profile() {
    assert!(!cfg!(debug_assertions), "run with --release");
    const WIDTH: u32 = 2880;
    const HEIGHT: u32 = 100;
    const TICKS: u32 = 60;
    let theme = theme();
    let mut component = host("glass-bar");
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    // Task sleeps re-queue from the wall clock, so a tick time that runs
    // ahead of it would re-run the clock task until real time caught up.
    let mut tick = |component: &mut FrontendSurfaceComponent, _second: u32| {
        let due = component.next_script_task_deadline().expect("clock task");
        component.run_due_script_tasks(due).unwrap();
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
    };
    for second in 1..=4 {
        tick(&mut component, second);
    }
    for run in 0..3 {
        component.set_profiling_enabled(true);
        let _ = component.take_profiling_records();
        let mut frames = Vec::new();
        for second in 0..TICKS {
            let started = std::time::Instant::now();
            tick(&mut component, 5 + run * TICKS + second);
            frames.push(started.elapsed());
        }
        frames.sort();
        let total: Duration = frames.iter().sum();
        eprintln!(
            "glass-bar clock tick run={run}: mean {:.3}ms p50 {:.3}ms p90 {:.3}ms",
            total.as_secs_f64() * 1000.0 / f64::from(TICKS),
            frames[frames.len() / 2].as_secs_f64() * 1000.0,
            frames[frames.len() * 9 / 10].as_secs_f64() * 1000.0,
        );
        super::navigation_profile::dump_stages(
            "glass-bar clock tick",
            component.take_profiling_records(),
            TICKS,
        );
        component.set_profiling_enabled(false);
    }
}

fn glass_apps_payload(pinned: usize, installed: usize) -> serde_json::Value {
    let app = |index: usize, pinned: bool| {
        serde_json::json!({
            "id": format!("app-{index}"),
            "name": format!("Application {index}"),
            "icon": "",
            "pinned": pinned,
            "running": index % 3 == 0,
            "focused": index == 0,
            "windows": usize::from(index % 3 == 0),
        })
    };
    serde_json::json!({
        "available": true,
        "apps": (0..pinned).map(|index| app(index, true)).collect::<Vec<_>>(),
        "installed": (0..installed).map(|index| app(index, index < pinned)).collect::<Vec<_>>(),
    })
}

fn count_nodes(node: &WidgetNode) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}

// cargo test -p mesh-core-shell --release --lib glass_dock_hover_animation_profile -- --ignored --nocapture
#[test]
#[ignore = "release-only glass-dock hover-animation frame-cost profile"]
fn glass_dock_hover_animation_profile() {
    assert!(!cfg!(debug_assertions), "run with --release");
    const WIDTH: u32 = 2880;
    const HEIGHT: u32 = 180;
    const FRAMES: u32 = 60;
    let installed = std::env::var("MESH_BENCH_APPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(150);
    let theme = theme();
    let mut component = host("glass-dock");
    update(
        &mut component,
        "mesh.apps",
        glass_apps_payload(10, installed),
    );
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
    // Transitions run on the wall clock; back-to-back paints sample them
    // mid-flight, which is the per-frame work an animation pays.
    // Like the shell, paint only frames the component asks for.
    let mut paint_frame = |component: &mut FrontendSurfaceComponent| {
        if component.wants_render() {
            component
                .paint_raster(&theme, extent, &mut pixels, 1.0)
                .unwrap();
        }
    };
    paint_frame(&mut component);
    let nodes = count_nodes(component.last_tree.as_ref().unwrap());
    let tiles: Vec<_> = (0..10)
        .map(|index| {
            let key = reference(&component, &format!("app-{index}"));
            find_node_bounds_by_key(component.last_tree.as_ref().unwrap(), &key, 0.0, 0.0).unwrap()
        })
        .collect();
    eprintln!("glass-dock: {installed} installed apps, {nodes} nodes");
    for run in 0..3 {
        component.set_profiling_enabled(true);
        let _ = component.take_profiling_records();
        let mut times = Vec::new();
        let mut visits = std::collections::BTreeMap::<String, u64>::new();
        let mut measured_frames = 0u64;
        for step in 0..FRAMES {
            // Cross to the next tile every six frames. The first run also
            // covers the shelf's 560ms `dock-in` entrance animation.
            let (left, top, right, bottom) = tiles[(step as usize / 6) % tiles.len()];
            let begun = std::time::Instant::now();
            if step % 6 == 0 {
                component
                    .handle_input(
                        &theme,
                        WIDTH,
                        HEIGHT,
                        ComponentInput::PointerMove {
                            x: (left + right) / 2.0,
                            y: (top + bottom) / 2.0,
                        },
                    )
                    .unwrap();
            }
            paint_frame(&mut component);
            times.push(begun.elapsed());
            if let Some(snapshot) = component.take_invalidation_snapshot() {
                if !snapshot.node_visits.is_empty() {
                    measured_frames += 1;
                    for (pass, count) in snapshot.node_visits {
                        *visits.entry(pass).or_default() += count;
                    }
                }
            }
        }
        times.sort();
        let total: Duration = times.iter().sum();
        eprintln!(
            "glass-dock hover run={run}: mean {:.3}ms p50 {:.3}ms p90 {:.3}ms",
            total.as_secs_f64() * 1000.0 / f64::from(FRAMES),
            times[times.len() / 2].as_secs_f64() * 1000.0,
            times[times.len() * 9 / 10].as_secs_f64() * 1000.0,
        );
        super::navigation_profile::dump_stages(
            "glass-dock hover",
            component.take_profiling_records(),
            FRAMES,
        );
        for (pass, count) in visits {
            eprintln!(
                "node-visits {pass}: total={count} frames={measured_frames} mean={:.1}",
                count as f64 / measured_frames.max(1) as f64
            );
        }
        component.set_profiling_enabled(false);
    }
}

fn d9_report(metric: &str, mut samples: Vec<Duration>) {
    assert!(!samples.is_empty());
    samples.sort();
    let at = |fraction: f64| {
        samples[((samples.len() - 1) as f64 * fraction).round() as usize].as_secs_f64() * 1000.0
    };
    println!("MESH_PERF metric={metric} value={:.6}", at(0.9));
    eprintln!(
        "{metric}: n={} p50={:.3}ms p90={:.3}ms",
        samples.len(),
        at(0.5),
        at(0.9)
    );
}

fn dashboard_payloads(component: &mut FrontendSurfaceComponent) {
    update(
        component,
        "mesh.system",
        serde_json::json!({"available":true,"cpu":37,"memory":52,"uptime":93_784}),
    );
    update(
        component,
        "mesh.media",
        serde_json::json!({"available":true,"playing":true,"title":"Track","artist":"Artist","position":42,"length":215}),
    );
    update(
        component,
        "mesh.weather",
        serde_json::json!({"available":true,"location":"Bratislava","temperature":17,"unit":"C","condition":"cloudy","is_day":true}),
    );
}

// Each sample mounts the dashboard, delivers its services and paints the
// first frame: the work a first open pays before anything is on screen.
#[test]
#[ignore = "D9 release workload budget: glass dashboard mount through first raster"]
fn d9_glass_dashboard_open() {
    assert!(!cfg!(debug_assertions), "run with --release");
    const WIDTH: u32 = 1920;
    const HEIGHT: u32 = 1080;
    let theme = theme();
    let catalog = GlassCatalog::new();
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut samples = Vec::new();
    for _ in 0..12 {
        let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
        let started = std::time::Instant::now();
        let mut component = catalog.mount("glass-dashboard");
        dashboard_payloads(&mut component);
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
        samples.push(started.elapsed());
        assert!(
            pixels.data().chunks_exact(4).any(|pixel| pixel[3] > 0),
            "opened dashboard must paint pixels"
        );
        let texts = rendered_text(&component);
        assert!(texts.iter().any(|text| text == "Bratislava"), "{texts:?}");
    }
    d9_report("d9_glass_dashboard_open_p90_ms", samples);
}

// Only frames the component asks for are timed, like the shell's frame
// callbacks; the first pass covers the shelf's `dock-in` entrance.
#[test]
#[ignore = "D9 release workload budget: glass dock hover animation frames"]
fn d9_glass_dock_animation() {
    assert!(!cfg!(debug_assertions), "run with --release");
    const WIDTH: u32 = 2880;
    const HEIGHT: u32 = 180;
    let theme = theme();
    let mut component = host("glass-dock");
    update(&mut component, "mesh.apps", glass_apps_payload(10, 150));
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    let tiles: Vec<_> = (0..10)
        .map(|index| {
            let key = reference(&component, &format!("app-{index}"));
            find_node_bounds_by_key(component.last_tree.as_ref().unwrap(), &key, 0.0, 0.0).unwrap()
        })
        .collect();
    let mut samples = Vec::new();
    for step in 0..180usize {
        let started = std::time::Instant::now();
        if step % 6 == 0 {
            let (left, top, right, bottom) = tiles[(step / 6) % tiles.len()];
            component
                .handle_input(
                    &theme,
                    WIDTH,
                    HEIGHT,
                    ComponentInput::PointerMove {
                        x: (left + right) / 2.0,
                        y: (top + bottom) / 2.0,
                    },
                )
                .unwrap();
        }
        if component.wants_render() {
            component
                .paint_raster(&theme, extent, &mut pixels, 1.0)
                .unwrap();
            samples.push(started.elapsed());
        }
    }
    assert!(
        samples.len() >= 30,
        "hover must animate: {} frames",
        samples.len()
    );
    d9_report("d9_glass_dock_frame_p90_ms", samples);
}

#[test]
#[ignore = "D9 release workload budget: glass bar clock tick through raster"]
fn d9_glass_bar_clock_tick() {
    assert!(!cfg!(debug_assertions), "run with --release");
    const WIDTH: u32 = 2880;
    const HEIGHT: u32 = 100;
    let theme = theme();
    let mut component = host("glass-bar");
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
    component
        .paint_raster(&theme, extent, &mut pixels, 1.0)
        .unwrap();
    // Task sleeps re-queue from the wall clock, so ticks use the clock
    // task's own deadline rather than a synthetic time that runs ahead.
    let mut samples = Vec::new();
    for second in 1..=64u64 {
        let due = component.next_script_task_deadline().expect("clock task");
        let started = std::time::Instant::now();
        component.run_due_script_tasks(due).unwrap();
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
        if second > 4 {
            samples.push(started.elapsed());
        }
    }
    d9_report("d9_glass_clock_tick_p90_ms", samples);
}

fn launcher_wrapper(component: &FrontendSurfaceComponent) -> &WidgetNode {
    fn find(node: &WidgetNode) -> Option<&WidgetNode> {
        if node.is_promoted_popover()
            && node.children.first().is_some_and(|popover| {
                popover
                    .attributes
                    .get(Prop::Class)
                    .is_some_and(|class| class.contains("launcher-shell"))
            })
        {
            return Some(node);
        }
        node.children.iter().find_map(find)
    }
    find(component.last_tree.as_ref().unwrap()).expect("launcher popover wrapper")
}

#[test]
fn closed_popover_subtree_is_skipped_until_it_opens_or_exits() {
    let mut component = host("glass-dock");
    update(&mut component, "mesh.apps", glass_apps_payload(4, 24));
    paint(&mut component, 1920, 180);
    paint(&mut component, 1920, 180);
    let wrapper = launcher_wrapper(&component);
    assert!(component.is_dormant_popover_wrapper(wrapper));
    let popover = &wrapper.children[0];
    let popover_id = popover.id;
    let popover_key = popover.mesh_key().unwrap().to_owned();

    press(&mut component, "glassLauncher", 1920, 180);
    assert_eq!(component.child_surface_requests().len(), 1);
    assert!(!component.is_dormant_popover_wrapper(launcher_wrapper(&component)));
    paint(&mut component, 1920, 180);

    component
        .handle_input(
            &theme(),
            1920,
            180,
            ComponentInput::KeyPressed {
                key: "Escape".into(),
                modifiers: Default::default(),
            },
        )
        .unwrap();
    paint(&mut component, 1920, 180);
    assert!(component.child_surface_requests().is_empty());
    assert!(component.is_dormant_popover_wrapper(launcher_wrapper(&component)));

    // The shell keeps the popup mapped for its exit: the subtree must sample
    // again, transitioning from the resting style it showed while closed.
    component.set_closing_child_keys([popover_key].into_iter().collect());
    assert!(!component.is_dormant_popover_wrapper(launcher_wrapper(&component)));
    paint(&mut component, 1920, 180);
    assert!(
        component.transitions.contains_key(popover_id),
        "the exit class must start the launcher's own transition"
    );
}

// cargo test -p mesh-core-shell --release --lib glass_dashboard_spectrum_profile -- --ignored --nocapture
#[test]
#[ignore = "release-only glass-dashboard spectrum frame-cost profile"]
fn glass_dashboard_spectrum_profile() {
    assert!(!cfg!(debug_assertions), "run with --release");
    const WIDTH: u32 = 1920;
    const HEIGHT: u32 = 1080;
    const FRAMES: u32 = 120;
    let theme = theme();
    let mut component = host("glass-dashboard");
    component.force_full_template_build = std::env::var_os("MESH_BENCH_FORCE_TEMPLATE").is_some();
    dashboard_payloads(&mut component);
    let extent = SurfaceExtent::unpadded(WIDTH, HEIGHT);
    let mut pixels = PixelBuffer::new(WIDTH, HEIGHT);
    let mut frame = |component: &mut FrontendSurfaceComponent, index: u32| {
        let bars: Vec<f64> = (0..32)
            .map(|bar| (f64::from((index * 7 + bar * 13) % 100)) / 100.0)
            .collect();
        update(
            component,
            "mesh.visualizer",
            serde_json::json!({"available":true,"active":true,"bars":bars}),
        );
        component
            .paint_raster(&theme, extent, &mut pixels, 1.0)
            .unwrap();
    };
    for index in 0..30 {
        frame(&mut component, index);
    }
    eprintln!(
        "glass-dashboard: {} nodes",
        count_nodes(component.last_tree.as_ref().unwrap())
    );
    for run in 0..3 {
        component.set_profiling_enabled(true);
        let _ = component.take_profiling_records();
        let mut times = Vec::new();
        let mut visits = std::collections::BTreeMap::<String, u64>::new();
        let mut measured_frames = 0u64;
        for index in 0..FRAMES {
            let begun = std::time::Instant::now();
            frame(&mut component, 30 + run * FRAMES + index);
            times.push(begun.elapsed());
            if let Some(snapshot) = component.take_invalidation_snapshot() {
                if !snapshot.node_visits.is_empty() {
                    measured_frames += 1;
                    for (pass, count) in snapshot.node_visits {
                        *visits.entry(pass).or_default() += count;
                    }
                }
            }
        }
        times.sort();
        let total: Duration = times.iter().sum();
        eprintln!(
            "glass-dashboard spectrum run={run}: mean {:.3}ms p50 {:.3}ms p90 {:.3}ms",
            total.as_secs_f64() * 1000.0 / f64::from(FRAMES),
            times[times.len() / 2].as_secs_f64() * 1000.0,
            times[times.len() * 9 / 10].as_secs_f64() * 1000.0,
        );
        super::navigation_profile::dump_stages(
            "glass-dashboard spectrum",
            component.take_profiling_records(),
            FRAMES,
        );
        for (pass, count) in visits {
            eprintln!(
                "node-visits {pass}: total={count} frames={measured_frames} mean={:.1}",
                count as f64 / measured_frames.max(1) as f64
            );
        }
        component.set_profiling_enabled(false);
    }
}
