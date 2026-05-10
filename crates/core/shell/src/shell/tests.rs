use super::{
    BackendLaunchCandidate, BackendRuntimeSlot, BackendRuntimeStatus, ComponentInput, CoreRequest,
    InterfaceProvider, InterfaceRegistry, KeyModifiers, ServiceCommandMsg, ServiceEvent, Shell,
    TabFocusTarget, backend_launch_candidates_from_graph, component_key_pressed_input,
    component_key_released_input,
    ipc::parse_ipc_command,
    service::{
        apply_service_update, script_events_to_requests, seed_service_state,
        service_name_from_interface,
    },
    shell_global_shortcut_request,
    surface_layout::{SurfaceSizePolicy, load_active_theme, load_frontend_module_settings},
};
use mesh_core_config::ShellConfig;
use mesh_core_debug::{
    ComponentInvalidationCounts, DisplayBatchBarrierSnapshot, ProfilingBackendStage,
    ProfilingInvalidationSnapshot, ProfilingStage, RetainedInvalidationCounts,
    RetainedPaintSnapshot, TextCacheSnapshot,
};
use mesh_core_elements::{LayoutRect, VariableStore, WidgetNode};
use mesh_core_interaction::measure_content_size;
use mesh_core_module::ModuleInstance;
use mesh_core_module::manifest::{
    CapabilitiesSection, CompatibilitySection, DependenciesSection, EntrypointsSection,
    ExportsSection, Manifest, ManifestSource, ModuleType, PackageSection, ProvidedInterface,
    SurfaceLayoutSection,
};
use mesh_core_module::package::{
    InstalledModuleGraph, LoadedModuleManifest, ModuleManifestSource, ModulePackageManifest,
    RootPackageManifest,
};
use mesh_core_scripting::{PublishedEvent, ScriptState};
use mesh_core_service::{
    ContractCapabilities, InterfaceContract, InterfaceMethod, contract::ContractStateField,
    parse_contract_version,
};
use mesh_core_wayland::{ClipboardError, ClipboardWriter};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

static SETTINGS_ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    key: &'static str,
    old: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &std::path::Path) -> Self {
        let old = std::env::var(key).ok();
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, old }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.old {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

fn node(tag: &str, x: f32, y: f32, width: f32, height: f32) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    node.layout = LayoutRect {
        x,
        y,
        width,
        height,
    };
    node
}

fn minimal_manifest(id: &str) -> Manifest {
    Manifest {
        package: PackageSection {
            id: id.to_string(),
            name: None,
            version: "0.1.0".into(),
            module_type: ModuleType::Surface,
            api_version: "0.1".into(),
            license: None,
            description: None,
            authors: Vec::new(),
            repository: None,
        },
        compatibility: CompatibilitySection::default(),
        dependencies: DependenciesSection::default(),
        capabilities: CapabilitiesSection::default(),
        entrypoints: EntrypointsSection::default(),
        accessibility: None,
        settings: None,
        i18n: None,
        theme: None,
        service: None,
        provides: Vec::new(),
        interface: None,
        extensions: Vec::new(),
        exports: ExportsSection::default(),
        provides_slots: HashMap::new(),
        slot_contributions: HashMap::new(),
        assets: None,
        icons: None,
        icon_pack: None,
        icon_requirements: mesh_core_module::IconRequirementsSection::default(),
        translations: HashMap::new(),
        surface_layout: None,
    }
}

fn minimal_backend_manifest(id: &str, entrypoint: Option<&str>) -> Manifest {
    let mut manifest = minimal_manifest(id);
    manifest.package.module_type = ModuleType::Backend;
    manifest.entrypoints.main = entrypoint.map(str::to_string);
    manifest.provides = vec![ProvidedInterface {
        interface: "mesh.audio".to_string(),
        version: Some("1.0".to_string()),
        base_module: None,
        backend_name: Some(id.to_string()),
        priority: 100,
        optional_capabilities: Vec::new(),
    }];
    manifest
}

fn module_instance(id: &str, entrypoint: Option<&str>) -> (tempfile::TempDir, ModuleInstance) {
    let dir = tempfile::tempdir().unwrap();
    if let Some(entrypoint) = entrypoint {
        let path = dir.path().join(entrypoint);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "function init()\nend\nfunction on_poll()\nend").unwrap();
    }
    let manifest = minimal_backend_manifest(id, entrypoint);
    let instance = ModuleInstance::new(
        manifest,
        dir.path().to_path_buf(),
        dir.path().join("package.json"),
        ManifestSource::ModuleJson,
    );
    (dir, instance)
}

fn test_config() -> ShellConfig {
    ShellConfig {
        shell: Default::default(),
        modules: HashMap::new(),
    }
}

fn loaded_module(json: &str) -> LoadedModuleManifest {
    LoadedModuleManifest {
        manifest: ModulePackageManifest::from_json_str(json).unwrap(),
        path: PathBuf::from("<test>/package.json"),
        source: ModuleManifestSource::PackageJson,
    }
}

fn graph_from_json(root: &str, modules: Vec<&str>) -> InstalledModuleGraph {
    InstalledModuleGraph::from_parts(
        RootPackageManifest::from_json_str(root).unwrap(),
        modules.into_iter().map(loaded_module).collect(),
    )
    .unwrap()
}

fn test_contract(interface: &str) -> InterfaceContract {
    InterfaceContract {
        interface: interface.to_string(),
        version: parse_contract_version("1.0").unwrap(),
        file_path: PathBuf::from("interface.toml"),
        state_fields: vec![
            ContractStateField {
                name: "available".to_string(),
                field_type: "boolean".to_string(),
                description: None,
            },
            ContractStateField {
                name: "percent".to_string(),
                field_type: "float".to_string(),
                description: None,
            },
            ContractStateField {
                name: "source_module".to_string(),
                field_type: "string".to_string(),
                description: None,
            },
        ],
        methods: vec![InterfaceMethod {
            name: "set_volume".to_string(),
            args: Vec::new(),
            returns: Some("Result".to_string()),
            coalesce: false,
        }],
        events: Vec::new(),
        types: HashMap::new(),
        capabilities: ContractCapabilities::default(),
    }
}

fn register_test_provider(interfaces: &InterfaceRegistry, interface: &str, provider_id: &str) {
    interfaces.register(InterfaceProvider {
        interface: interface.to_string(),
        version: Some("1.0".to_string()),
        base_module: Some("@mesh/test-interface".to_string()),
        provider_module: provider_id.to_string(),
        backend_name: provider_id.to_string(),
        priority: 100,
    });
}

fn backend_runtime_slot(
    runtime: &Runtime,
    interface: &str,
    provider_id: &str,
) -> (
    BackendRuntimeSlot,
    mpsc::UnboundedReceiver<ServiceCommandMsg>,
) {
    let (command_tx, command_rx) = mpsc::unbounded_channel();
    let task = runtime.spawn(async {
        std::future::pending::<()>().await;
    });
    (
        BackendRuntimeSlot {
            interface: interface.to_string(),
            provider_id: provider_id.to_string(),
            command_tx,
            task: task.abort_handle(),
        },
        command_rx,
    )
}

fn service_update(interface: &str, provider_id: &str, payload: serde_json::Value) -> ServiceEvent {
    ServiceEvent::Updated {
        service: interface.to_string(),
        source_module: provider_id.to_string(),
        payload,
    }
}

struct RecordingComponent {
    events: Arc<Mutex<Vec<ServiceEvent>>>,
}

impl RecordingComponent {
    fn new(events: Arc<Mutex<Vec<ServiceEvent>>>) -> Self {
        Self { events }
    }
}

impl super::types::ShellComponent for RecordingComponent {
    fn id(&self) -> &str {
        "@test/recording"
    }

    fn surface_id(&self) -> &str {
        "@test/recording"
    }

    fn mount(
        &mut self,
        _ctx: super::types::ComponentContext,
    ) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
        Ok(Vec::new())
    }

    fn handle_core_event(
        &mut self,
        _event: &super::types::CoreEvent,
    ) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
        Ok(Vec::new())
    }

    fn handle_service_event(
        &mut self,
        event: &ServiceEvent,
    ) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
        self.events.lock().unwrap().push(event.clone());
        Ok(Vec::new())
    }

    fn tick(&mut self) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
        Ok(Vec::new())
    }

    fn wants_render(&self) -> bool {
        false
    }

    fn render(
        &mut self,
        _surface: &mut dyn mesh_core_wayland::ShellSurface,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn paint(
        &mut self,
        _theme: &mesh_core_theme::Theme,
        _width: u32,
        _height: u32,
        _buffer: &mut mesh_core_render::PixelBuffer,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct FocusRecordingState {
    releases: usize,
    registered_popovers: Vec<(String, String)>,
    received_focus: Vec<(TabFocusTarget, Option<(String, String)>, bool)>,
    keyboard_mode_overrides: Vec<Option<mesh_core_wayland::KeyboardMode>>,
}

struct FocusRecordingComponent {
    surface_id: String,
    state: Arc<Mutex<FocusRecordingState>>,
}

impl FocusRecordingComponent {
    fn new(surface_id: &str, state: Arc<Mutex<FocusRecordingState>>) -> Self {
        Self {
            surface_id: surface_id.to_string(),
            state,
        }
    }
}

impl super::types::ShellComponent for FocusRecordingComponent {
    fn id(&self) -> &str {
        &self.surface_id
    }

    fn surface_id(&self) -> &str {
        &self.surface_id
    }

    fn mount(
        &mut self,
        _ctx: super::types::ComponentContext,
    ) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
        Ok(Vec::new())
    }

    fn handle_core_event(
        &mut self,
        _event: &super::types::CoreEvent,
    ) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
        Ok(Vec::new())
    }

    fn handle_service_event(
        &mut self,
        _event: &ServiceEvent,
    ) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
        Ok(Vec::new())
    }

    fn tick(&mut self) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
        Ok(Vec::new())
    }

    fn wants_render(&self) -> bool {
        false
    }

    fn render(
        &mut self,
        _surface: &mut dyn mesh_core_wayland::ShellSurface,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn paint(
        &mut self,
        _theme: &mesh_core_theme::Theme,
        _width: u32,
        _height: u32,
        _buffer: &mut mesh_core_render::PixelBuffer,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn receive_focus_transfer(
        &mut self,
        target: &TabFocusTarget,
        return_focus: Option<(String, String)>,
        close_on_focus_leave: bool,
    ) {
        self.state.lock().unwrap().received_focus.push((
            target.clone(),
            return_focus,
            close_on_focus_leave,
        ));
    }

    fn release_focus_for_transfer(&mut self) {
        self.state.lock().unwrap().releases += 1;
    }

    fn register_popover_trigger(&mut self, trigger_key: String, popover_surface: String) {
        self.state
            .lock()
            .unwrap()
            .registered_popovers
            .push((trigger_key, popover_surface));
    }

    fn set_keyboard_mode_override(&mut self, mode: Option<mesh_core_wayland::KeyboardMode>) {
        self.state
            .lock()
            .unwrap()
            .keyboard_mode_overrides
            .push(mode);
    }
}

struct RecordingClipboard {
    writes: Arc<Mutex<Vec<String>>>,
}

impl ClipboardWriter for RecordingClipboard {
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.writes.lock().unwrap().push(text.to_string());
        Ok(())
    }
}

#[test]
fn latest_service_state_is_keyed_by_interface() {
    let mut shell = Shell::new();

    shell
        .broadcast_service_event(service_update(
            "audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 42.0 }),
        ))
        .unwrap();

    assert!(shell.latest_service_state.contains_key("mesh.audio"));
    assert!(!shell.latest_service_state.contains_key("audio"));
    let latest = shell.latest_service_state.get("mesh.audio").unwrap();
    assert_eq!(latest.interface, "mesh.audio");
    assert_eq!(latest.state["percent"], serde_json::json!(42.0));
}

#[test]
fn selection_input_contract_key_pressed_preserves_modifiers() {
    let input = component_key_pressed_input("C".into(), true, false, true);
    match input {
        ComponentInput::KeyPressed { key, modifiers } => {
            assert_eq!(key, "C");
            assert!(modifiers.ctrl);
            assert!(!modifiers.shift);
            assert!(modifiers.alt);
        }
        other => panic!("expected key press input, got {other:?}"),
    }
}

#[test]
fn selection_input_contract_key_released_preserves_modifiers() {
    let input = component_key_released_input(
        "Enter".into(),
        KeyModifiers {
            ctrl: true,
            shift: true,
            alt: false,
        },
    );
    match input {
        ComponentInput::KeyReleased { key, modifiers } => {
            assert_eq!(key, "Enter");
            assert!(modifiers.ctrl);
            assert!(modifiers.shift);
            assert!(!modifiers.alt);
        }
        other => panic!("expected key release input, got {other:?}"),
    }
}

#[test]
fn selection_input_contract_debug_shortcuts_remain_global() {
    assert!(matches!(
        shell_global_shortcut_request("d", true, true, false),
        Some(CoreRequest::ToggleDebugOverlay)
    ));
    assert!(matches!(
        shell_global_shortcut_request("Tab", true, false, true),
        Some(CoreRequest::CycleDebugTab)
    ));
    assert!(shell_global_shortcut_request("c", true, false, false).is_none());
}

#[test]
fn debug_profiling_request_toggles_independent_session_state() {
    let mut shell = Shell::new();

    assert!(!shell.debug.profiling_enabled);
    assert_eq!(shell.debug.profiling_session_id, 0);

    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    assert!(shell.debug.profiling_enabled);
    assert_eq!(shell.debug.profiling_session_id, 1);

    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    assert!(!shell.debug.profiling_enabled);
    assert_eq!(
        shell.debug.profiling_session_id, 1,
        "disabling profiling should not fabricate a new session"
    );

    shell
        .apply_request(CoreRequest::ToggleDebugOverlay)
        .unwrap();
    assert!(
        !shell.debug.profiling_enabled,
        "debug overlay visibility must remain independent from profiling state"
    );
}

#[test]
fn debug_snapshot_omits_profiling_payload_when_disabled() {
    let mut shell = Shell::new();
    let snapshot = shell.build_debug_snapshot();
    assert!(
        snapshot.profiling.is_none(),
        "profiling payload must be absent while profiling is disabled"
    );
}

#[test]
fn benchmark_snapshot_exposes_five_stable_scenarios() {
    let mut shell = Shell::new();
    let snapshot = shell.build_debug_snapshot();

    assert_eq!(snapshot.benchmarks.scenarios.len(), 5);
    assert_eq!(
        snapshot
            .benchmarks
            .scenarios
            .iter()
            .map(|scenario| scenario.id.id())
            .collect::<Vec<_>>(),
        vec![
            "hover",
            "surface_open_close",
            "pointer_update",
            "keyboard_traversal",
            "backend_update",
        ]
    );
    assert_eq!(
        snapshot
            .benchmarks
            .scenarios
            .last()
            .map(|scenario| scenario.label.as_str()),
        Some("Backend-driven update")
    );
}

#[test]
fn benchmark_payload_keeps_scenarios_inert_when_profiling_disabled() {
    let mut shell = Shell::new();
    let snapshot = shell.build_debug_snapshot();

    assert!(snapshot.profiling.is_none());
    assert!(
        !shell.debug.profiling_enabled,
        "building debug snapshots must not start profiling"
    );
    assert!(snapshot.benchmarks.scenarios.iter().all(|scenario| {
        scenario.status == mesh_core_debug::BenchmarkScenarioStatus::ProfilingOff
            && scenario.hint == "Start profiling first"
            && scenario.primary_metric == "No benchmark results yet"
            && scenario.secondary_metric == "No benchmark results yet"
    }));

    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("mesh.debug service state should include benchmark rows");
    assert!(latest.state["profiling"].is_null());
    let scenarios = latest.state["benchmarks"]["scenarios"]
        .as_array()
        .expect("benchmarks.scenarios should serialize as an array");
    assert_eq!(scenarios.len(), 5);
    assert!(scenarios.iter().all(|scenario| {
        scenario["status"] == serde_json::json!("Profiling off")
            && scenario["hint"] == serde_json::json!("Start profiling first")
    }));
}

#[test]
fn benchmark_payload_serializes_targets_statuses_and_metrics() {
    let mut shell = Shell::new();
    shell.build_debug_snapshot();

    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("mesh.debug service state should include benchmark payload");
    let scenarios = latest.state["benchmarks"]["scenarios"]
        .as_array()
        .expect("benchmarks.scenarios should serialize as an array");
    assert_eq!(scenarios.len(), 5);
    assert_eq!(
        scenarios
            .iter()
            .map(|scenario| scenario["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "hover",
            "surface_open_close",
            "pointer_update",
            "keyboard_traversal",
            "backend_update",
        ]
    );
    assert_eq!(
        scenarios
            .iter()
            .map(|scenario| scenario["target"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "@mesh/navigation-bar",
            "@mesh/audio-popover",
            "@mesh/navigation-bar audio controls",
            "@mesh/navigation-bar focus chain",
            "mesh.audio -> @mesh/pipewire-audio",
        ]
    );
    let backend_update = &scenarios[4];
    assert_eq!(
        backend_update["label"],
        serde_json::json!("Backend-driven update")
    );
    assert_eq!(backend_update["status"], serde_json::json!("Profiling off"));
    assert_eq!(
        backend_update["primary_metric"],
        serde_json::json!("No benchmark results yet")
    );
    assert_eq!(
        backend_update["secondary_metric"],
        serde_json::json!("No benchmark results yet")
    );
}

#[test]
fn benchmark_backend_update_correlates_backend_stage_with_surface_render_cost() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pipewire-audio",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(31),
        Some("broadcast_service_event"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(45),
        Some("service_update"),
    );

    let snapshot = shell.build_debug_snapshot();
    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");

    assert_eq!(
        backend_update.status,
        mesh_core_debug::BenchmarkScenarioStatus::Complete
    );
    assert_eq!(backend_update.target, "mesh.audio -> @mesh/pipewire-audio");
    assert!(backend_update.primary_metric.contains("mesh.audio"));
    assert!(
        backend_update
            .primary_metric
            .contains("@mesh/pipewire-audio")
    );
    assert!(
        backend_update
            .primary_metric
            .contains("state_publish_delivery")
    );
    assert!(
        backend_update
            .secondary_metric
            .contains("total_surface_render")
    );
    assert!(backend_update.secondary_metric.contains("45us"));

    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("mesh.debug service state should include benchmark payload");
    let scenarios = latest.state["benchmarks"]["scenarios"]
        .as_array()
        .expect("benchmarks.scenarios should serialize as an array");
    let payload = scenarios
        .iter()
        .find(|scenario| scenario["id"] == serde_json::json!("backend_update"))
        .expect("backend_update payload should serialize");
    assert_eq!(payload["status"], serde_json::json!("Complete"));
    assert_eq!(
        payload["target"],
        serde_json::json!("mesh.audio -> @mesh/pipewire-audio")
    );
    assert!(
        payload["primary_metric"]
            .as_str()
            .unwrap()
            .contains("state_publish_delivery")
    );
    assert!(
        payload["secondary_metric"]
            .as_str()
            .unwrap()
            .contains("frontend total_surface_render")
    );
}

#[test]
fn benchmark_backend_update_waits_when_surface_cost_is_missing() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pipewire-audio",
        ProfilingBackendStage::CommandHandling,
        std::time::Duration::from_micros(27),
        Some("service_command"),
    );

    let snapshot = shell.build_debug_snapshot();
    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");

    assert_eq!(
        backend_update.status,
        mesh_core_debug::BenchmarkScenarioStatus::WaitingForSamples
    );
    assert_eq!(
        backend_update.primary_metric,
        "Backend provider timing captured"
    );
    assert_eq!(
        backend_update.secondary_metric,
        "No frontend surface render samples yet"
    );
}

#[test]
fn benchmark_backend_update_waits_when_backend_stage_is_missing() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Running,
        "backend runtime started".to_string(),
    );

    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(53),
        Some("service_update"),
    );

    let snapshot = shell.build_debug_snapshot();
    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");

    assert_eq!(
        backend_update.status,
        mesh_core_debug::BenchmarkScenarioStatus::WaitingForSamples
    );
    assert_eq!(backend_update.target, "mesh.audio -> @mesh/pipewire-audio");
    assert_eq!(
        backend_update.primary_metric,
        "No backend provider samples yet"
    );
    assert_eq!(
        backend_update.secondary_metric,
        "frontend total_surface_render: 53us"
    );
}

#[test]
fn benchmark_service_event_maps_to_run_request() {
    let requests = script_events_to_requests(vec![PublishedEvent {
        channel: "shell.run-debug-benchmark".into(),
        payload: serde_json::json!({ "scenario_id": "hover" }),
        source_module_id: "@mesh/debug-inspector".into(),
        source_capabilities: mesh_core_capability::CapabilitySet::new(),
    }]);

    match requests.as_slice() {
        [CoreRequest::RunDebugBenchmark { scenario_id }] => {
            assert_eq!(scenario_id, "hover");
        }
        other => panic!("expected RunDebugBenchmark request, got {other:?}"),
    }
}

#[test]
fn benchmark_ipc_command_maps_to_run_request() {
    match parse_ipc_command("shell:debug_benchmark:pointer_update") {
        Some(CoreRequest::RunDebugBenchmark { scenario_id }) => {
            assert_eq!(scenario_id, "pointer_update");
        }
        other => panic!("expected RunDebugBenchmark request, got {other:?}"),
    }
}

#[test]
fn benchmark_run_request_does_not_enable_profiling() {
    let mut shell = Shell::new();

    let emitted = shell
        .apply_request(CoreRequest::RunDebugBenchmark {
            scenario_id: "surface_open_close".into(),
        })
        .unwrap();

    assert!(
        !shell.debug.profiling_enabled,
        "benchmark requests must not enable profiling"
    );
    assert_eq!(
        shell
            .debug
            .latest_benchmark_run
            .as_ref()
            .map(|run| run.scenario_id.id()),
        Some("surface_open_close")
    );
    assert_eq!(emitted.len(), 1);
    assert!(matches!(
        &emitted[0],
        CoreRequest::ToggleSurface { surface_id } if surface_id == "@mesh/audio-popover"
    ));

    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    assert!(shell.debug.profiling_enabled);
    shell
        .apply_request(CoreRequest::RunDebugBenchmark {
            scenario_id: "keyboard_traversal".into(),
        })
        .unwrap();
    assert!(
        shell.debug.profiling_enabled,
        "benchmark requests must preserve existing profiling state"
    );
    assert_eq!(
        shell
            .debug
            .latest_benchmark_run
            .as_ref()
            .map(|run| run.scenario_id.id()),
        Some("keyboard_traversal")
    );
}

#[test]
fn benchmark_backend_update_uses_active_audio_provider() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pulseaudio-audio".to_string(),
        BackendRuntimeStatus::Running,
        "backend runtime started".to_string(),
    );

    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pulseaudio-audio",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(29),
        Some("broadcast_service_event"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(41),
        Some("service_update"),
    );

    let snapshot = shell.build_debug_snapshot();
    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");

    assert_eq!(
        backend_update.status,
        mesh_core_debug::BenchmarkScenarioStatus::Complete
    );
    assert_eq!(
        backend_update.target,
        "mesh.audio -> @mesh/pulseaudio-audio"
    );
    assert!(
        backend_update
            .primary_metric
            .contains("@mesh/pulseaudio-audio")
    );
    assert!(
        backend_update
            .secondary_metric
            .contains("total_surface_render")
    );
}

#[test]
fn benchmark_backend_update_ignores_terminal_audio_runtime_when_running_provider_exists() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Stopped,
        "runtime stopped".to_string(),
    );
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pulseaudio-audio".to_string(),
        BackendRuntimeStatus::Running,
        "backend runtime started".to_string(),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pipewire-audio",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(99),
        Some("stale_service_update"),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pulseaudio-audio",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(29),
        Some("broadcast_service_event"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(41),
        Some("service_update"),
    );

    let snapshot = shell.build_debug_snapshot();
    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");

    assert_eq!(
        backend_update.target,
        "mesh.audio -> @mesh/pulseaudio-audio"
    );
    assert!(
        backend_update
            .primary_metric
            .contains("@mesh/pulseaudio-audio")
    );
    assert!(
        !backend_update
            .primary_metric
            .contains("@mesh/pipewire-audio")
    );
}

#[test]
fn benchmark_backend_update_reports_unavailable_for_failed_only_audio_runtime() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Failed,
        "runtime failed".to_string(),
    );

    let snapshot = shell.build_debug_snapshot();
    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");

    assert_eq!(
        backend_update.status,
        mesh_core_debug::BenchmarkScenarioStatus::Unavailable
    );
    assert_eq!(
        backend_update.primary_metric,
        "No backend provider samples yet"
    );
}

#[test]
fn phase18_baseline_ranks_hotspots_by_absolute_latency() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(142),
        Some("phase18_fresh_baseline"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(77),
        Some("phase18_fresh_baseline"),
    );
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Running,
        "phase18 backend baseline".to_string(),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pipewire-audio",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(109),
        Some("phase18_fresh_baseline"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(88),
        Some("phase18_backend_visible_frontend"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot
        .profiling
        .as_ref()
        .expect("profiling should be enabled for phase 18 baseline");
    let navigation_bar = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/navigation-bar")
        .expect("navigation bar surface sample should be recorded");
    let backend = profiling
        .backends
        .iter()
        .find(|backend| {
            backend.interface == "mesh.audio" && backend.provider_id == "@mesh/pipewire-audio"
        })
        .expect("backend sample should be recorded");
    let backend_visible_frontend = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/audio-popover")
        .expect("backend-driven frontend surface sample should be recorded");

    let nav_render = navigation_bar
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingStage::TotalSurfaceRender)
        .expect("navigation bar render stage should be recorded");
    let nav_paint = navigation_bar
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingStage::Paint)
        .expect("navigation bar paint stage should be recorded");
    let backend_publish = backend
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingBackendStage::StatePublishDelivery)
        .expect("backend publish stage should be recorded");

    let mut candidates = [
        (
            "surface_render:@mesh/navigation-bar",
            nav_render.max_micros,
            true,
        ),
        ("paint:@mesh/navigation-bar", nav_paint.max_micros, true),
        (
            "backend_publish:@mesh/pipewire-audio",
            backend_publish.max_micros,
            backend_visible_frontend.total_surface_render_time_micros > 0,
        ),
    ];
    candidates.sort_by(|left, right| right.1.cmp(&left.1));

    assert_eq!(candidates[0].0, "surface_render:@mesh/navigation-bar");
    assert_eq!(candidates[0].1, 142);
    assert!(
        candidates.iter().any(
            |(name, value, eligible)| name.starts_with("backend_publish")
                && *value == 109
                && *eligible
        ),
        "backend candidate should remain eligible only when frontend impact is visible"
    );
}

#[test]
fn phase18_benchmark_payload_preserves_render_visible_contract_after_lookup_cache() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::InputHandling,
        std::time::Duration::from_micros(21),
        Some("phase18_contract"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::StyleRestyle,
        std::time::Duration::from_micros(34),
        Some("phase18_contract"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Layout,
        std::time::Duration::from_micros(55),
        Some("phase18_contract"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(67),
        Some("phase18_contract"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(142),
        Some("phase18_contract"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(88),
        Some("phase18_contract"),
    );
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Running,
        "phase18 backend contract".to_string(),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pipewire-audio",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(109),
        Some("phase18_contract"),
    );

    let snapshot = shell.build_debug_snapshot();
    let scenario_ids: Vec<_> = snapshot
        .benchmarks
        .scenarios
        .iter()
        .map(|scenario| scenario.id.id())
        .collect();

    assert_eq!(
        scenario_ids,
        [
            "hover",
            "surface_open_close",
            "pointer_update",
            "keyboard_traversal",
            "backend_update"
        ]
    );

    let navigation_rows: Vec<_> = snapshot
        .benchmarks
        .scenarios
        .iter()
        .filter(|scenario| scenario.target.contains("@mesh/navigation-bar"))
        .collect();
    assert_eq!(navigation_rows.len(), 3);
    assert!(
        navigation_rows.iter().all(|scenario| {
            scenario.status == mesh_core_debug::BenchmarkScenarioStatus::Complete
        })
    );
    assert!(navigation_rows.iter().any(|scenario| {
        scenario.id.id() == "keyboard_traversal" && scenario.secondary_metric.contains("142us")
    }));

    let backend_update = snapshot
        .benchmarks
        .scenarios
        .iter()
        .find(|scenario| scenario.id.id() == "backend_update")
        .expect("backend_update benchmark row should exist");
    assert_eq!(
        backend_update.status,
        mesh_core_debug::BenchmarkScenarioStatus::Complete
    );
    assert_eq!(backend_update.target, "mesh.audio -> @mesh/pipewire-audio");
    assert!(
        backend_update
            .primary_metric
            .contains("@mesh/pipewire-audio")
    );
    assert!(backend_update.secondary_metric.contains("142us"));
}

#[test]
fn benchmark_run_request_rejects_unknown_scenario() {
    let mut shell = Shell::new();

    let emitted = shell
        .apply_request(CoreRequest::RunDebugBenchmark {
            scenario_id: "not_a_scenario".into(),
        })
        .unwrap();

    assert!(
        !shell.debug.profiling_enabled,
        "rejected benchmark requests must not enable profiling"
    );
    assert!(shell.debug.latest_benchmark_run.is_none());
    match emitted.as_slices().0 {
        [CoreRequest::PublishDiagnostics { message }] => {
            assert!(message.contains("unknown debug benchmark scenario"));
            assert!(message.contains("not_a_scenario"));
        }
        other => panic!("expected diagnostic for unknown benchmark scenario, got {other:?}"),
    }
}

#[test]
fn debug_snapshot_backfills_mesh_debug_service_state() {
    let mut shell = Shell::new();
    shell.debug.enabled = true;

    let snapshot = shell.build_debug_snapshot();
    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("mesh.debug service state should be backfilled from debug snapshots");

    assert_eq!(latest.provider_id, mesh_core_debug::DEBUG_SOURCE_MODULE_ID);
    assert_eq!(latest.state["overlay_enabled"], serde_json::json!(true));
    assert_eq!(latest.state["profiling_enabled"], serde_json::json!(false));
    assert_eq!(latest.state["profiling_session_id"], serde_json::json!(0));
    assert_eq!(latest.state["active_view"], serde_json::json!("overview"));
    assert_eq!(
        latest.state["active_surfaces"],
        serde_json::json!(snapshot.active_surfaces)
    );
    assert!(latest.state["profiling"].is_null());
}

#[test]
fn shell_registers_debug_provider_for_builtin_inspector_imports() {
    let shell = Shell::new();
    let resolution = shell
        .interfaces
        .resolve(mesh_core_debug::DEBUG_INTERFACE, Some(">=1.0"));

    assert_eq!(
        resolution
            .provider
            .as_ref()
            .map(|provider| provider.provider_module.as_str()),
        Some(mesh_core_debug::DEBUG_SOURCE_MODULE_ID)
    );
}

#[test]
fn shell_registers_theme_provider_for_frontend_theme_proxy() {
    let shell = Shell::new();
    let resolution = shell.interfaces.resolve("mesh.theme", None);

    assert_eq!(
        resolution
            .provider
            .as_ref()
            .map(|provider| provider.provider_module.as_str()),
        Some("@mesh/shell"),
        "frontend modules with theme.read must be able to resolve require(\"mesh.theme\")"
    );
}

#[test]
fn debug_snapshot_publish_delivers_mesh_debug_service_event() {
    let mut shell = Shell::new();
    shell.debug.enabled = true;
    let events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::new(events.clone())));

    let emitted = shell.publish_debug_snapshot().unwrap();

    assert!(emitted.is_empty());
    let events = events.lock().unwrap();
    let ServiceEvent::Updated {
        service,
        source_module,
        payload,
    } = events
        .last()
        .expect("debug snapshot should be delivered as a service update");
    assert_eq!(service, mesh_core_debug::DEBUG_INTERFACE);
    assert_eq!(source_module, mesh_core_debug::DEBUG_SOURCE_MODULE_ID);
    assert_eq!(payload["overlay_enabled"], serde_json::json!(true));
    assert!(payload["benchmarks"]["scenarios"].is_array());
}

#[test]
fn debug_overlay_toggle_does_not_enable_profiling_in_mesh_debug_payload() {
    let mut shell = Shell::new();

    shell
        .apply_request(CoreRequest::ToggleDebugOverlay)
        .unwrap();
    shell.build_debug_snapshot();
    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("mesh.debug state should exist after snapshot generation");

    assert_eq!(latest.state["overlay_enabled"], serde_json::json!(true));
    assert_eq!(latest.state["profiling_enabled"], serde_json::json!(false));
    assert_eq!(latest.state["profiling_session_id"], serde_json::json!(0));
    assert!(latest.state["profiling"].is_null());
}

#[test]
fn debug_overlay_toggle_controls_mesh_debug_inspector_visibility_without_enabling_profiling() {
    let mut shell = Shell::new();

    shell
        .apply_request(CoreRequest::ToggleDebugOverlay)
        .unwrap();

    let inspector = shell
        .core
        .surfaces
        .get("@mesh/debug-inspector")
        .expect("debug inspector surface should be tracked when overlay toggles on");
    assert!(shell.debug.enabled);
    assert!(inspector.visible);
    assert!(!shell.debug.profiling_enabled);

    shell
        .apply_request(CoreRequest::ToggleDebugOverlay)
        .unwrap();

    let inspector = shell
        .core
        .surfaces
        .get("@mesh/debug-inspector")
        .expect("debug inspector surface state should remain addressable");
    assert!(!shell.debug.enabled);
    assert!(!inspector.visible);
    assert!(!shell.debug.profiling_enabled);
}

#[test]
fn profiling_session_reset_discards_previous_samples() {
    let mut shell = Shell::new();

    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.profiling.record_shell_stage(
        ProfilingStage::RuntimeUpdateHandling,
        std::time::Duration::from_micros(25),
        Some("service_update"),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pulse",
        ProfilingBackendStage::PollUpdate,
        std::time::Duration::from_micros(9),
        Some("service_update"),
    );
    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    assert_eq!(profiling.session_id, 1);
    assert_eq!(profiling.shell.stages.len(), 1);
    assert_eq!(profiling.backends.len(), 1);

    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    let reset_snapshot = shell.build_debug_snapshot();
    let profiling = reset_snapshot
        .profiling
        .expect("profiling should be enabled after the second toggle");
    assert_eq!(profiling.session_id, 2);
    assert!(
        profiling.shell.stages.is_empty(),
        "enabling a fresh profiling session must clear previous samples"
    );
    assert!(
        profiling.backends.is_empty(),
        "enabling a fresh profiling session must also clear backend samples"
    );
}

#[test]
fn profiling_snapshot_tracks_bounded_surface_samples_and_redraw_counts() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.profiling.record_surface_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(120),
        Some("rebuild"),
    );
    shell.profiling.record_surface_redraw(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        Some("rebuild"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let surface = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/navigation-bar")
        .expect("surface snapshot should be recorded when work occurs");

    assert_eq!(surface.module_id.as_deref(), Some("@mesh/navigation-bar"));
    assert_eq!(surface.redraw_count, 1);
    assert_eq!(surface.total_surface_render_time_micros, 120);
    assert!(
        surface
            .stages
            .iter()
            .any(|stage| stage.stage == ProfilingStage::TotalSurfaceRender),
        "surface summaries must expose total surface render timing as a first-class stage"
    );
}

#[test]
fn profiling_snapshot_exposes_typed_surface_invalidation_counts() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_invalidation(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingInvalidationSnapshot {
            full_rebuild: false,
            retained_path: true,
            retained_generation: 7,
            component: ComponentInvalidationCounts {
                state: 1,
                style: 1,
                layout: 1,
                paint: 1,
                accessibility: 1,
                metrics: 1,
                ..Default::default()
            },
            retained: RetainedInvalidationCounts {
                style: 2,
                layout: 1,
                state: 1,
                ..Default::default()
            },
            paint: RetainedPaintSnapshot {
                entries_total: 5,
                entries_reused: 3,
                entries_rebuilt: 2,
                damage_area: 120,
                surface_area: 1_000,
                partial_present_supported: false,
                skipped_paint_pixels: 0,
                batch_count: 2,
                batched_primitives: 5,
                barrier_count: 3,
                barriers: DisplayBatchBarrierSnapshot {
                    text: 1,
                    material_change: 2,
                    ..Default::default()
                },
                ..Default::default()
            },
            text: TextCacheSnapshot {
                layout_hits: 4,
                layout_misses: 1,
                shaped_entries: 1,
                glyph_cache_active: true,
                ..Default::default()
            },
        },
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let surface = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/navigation-bar")
        .expect("surface snapshot should be recorded when invalidation occurs");
    let invalidation = surface
        .invalidation
        .as_ref()
        .expect("surface profiling should carry typed invalidation counts");

    assert!(!invalidation.full_rebuild);
    assert!(invalidation.retained_path);
    assert_eq!(invalidation.retained_generation, 7);
    assert_eq!(invalidation.component.style, 1);
    assert_eq!(invalidation.component.text, 0);
    assert_eq!(invalidation.retained.style, 2);
    assert_eq!(invalidation.retained.layout, 1);
    assert_eq!(invalidation.paint.entries_total, 5);
    assert_eq!(invalidation.paint.damage_area, 120);
    assert!(!invalidation.paint.partial_present_supported);
    assert_eq!(invalidation.paint.skipped_paint_pixels, 0);
    assert_eq!(invalidation.paint.batch_count, 2);
    assert_eq!(invalidation.paint.batched_primitives, 5);
    assert_eq!(invalidation.paint.barriers.text, 1);
    assert_eq!(invalidation.paint.barriers.material_change, 2);
    assert_eq!(invalidation.text.layout_hits, 4);
    assert_eq!(invalidation.text.layout_misses, 1);
    assert!(invalidation.text.glyph_cache_active);
}

#[test]
fn profiling_stage_surface_records_roll_up_into_shell_summary() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TreeBuild,
        std::time::Duration::from_micros(30),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Layout,
        std::time::Duration::from_micros(45),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::PresentCommit,
        std::time::Duration::from_micros(12),
        Some("present"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");

    let shell_stages: std::collections::HashMap<_, _> = profiling
        .shell
        .stages
        .iter()
        .map(|stage| (stage.stage, stage.total_micros))
        .collect();
    assert_eq!(shell_stages.get(&ProfilingStage::TreeBuild), Some(&30));
    assert_eq!(shell_stages.get(&ProfilingStage::Layout), Some(&45));
    assert_eq!(shell_stages.get(&ProfilingStage::PresentCommit), Some(&12));
}

#[test]
fn profiling_surface_snapshot_preserves_surface_and_module_identity_with_comparable_totals() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Layout,
        std::time::Duration::from_micros(45),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(30),
        Some("rebuild"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let surface = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/navigation-bar")
        .expect("worked surfaces must be keyed by surface_id");
    let shell_stages: std::collections::HashMap<_, _> = profiling
        .shell
        .stages
        .iter()
        .map(|stage| (stage.stage, stage.total_micros))
        .collect();
    let surface_stages: std::collections::HashMap<_, _> = surface
        .stages
        .iter()
        .map(|stage| (stage.stage, stage.total_micros))
        .collect();

    assert_eq!(surface.surface_id, "@mesh/navigation-bar");
    assert_eq!(surface.module_id.as_deref(), Some("@mesh/navigation-bar"));
    assert_eq!(shell_stages.get(&ProfilingStage::Layout), Some(&45));
    assert_eq!(surface_stages.get(&ProfilingStage::Layout), Some(&45));
    assert_eq!(shell_stages.get(&ProfilingStage::Paint), Some(&30));
    assert_eq!(surface_stages.get(&ProfilingStage::Paint), Some(&30));
}

#[test]
fn profiling_disabled_runtime_stage_helpers_remain_inert() {
    let mut shell = Shell::new();

    shell.record_shell_profiling_stage(
        ProfilingStage::InputHandling,
        std::time::Duration::from_micros(10),
        Some("key"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(20),
        Some("rebuild"),
    );
    shell.record_surface_redraw(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        Some("present"),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pulse",
        ProfilingBackendStage::CommandHandling,
        std::time::Duration::from_micros(8),
        Some("service_command"),
    );

    let snapshot = shell.build_debug_snapshot();
    assert!(
        snapshot.profiling.is_none(),
        "profiling-disabled helpers must not fabricate shell or surface snapshots"
    );
}

#[test]
fn profiling_disabled_backend_paths_do_not_fabricate_snapshots() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut pending = std::collections::VecDeque::new();
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::BackendServiceUpdate {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/pipewire-audio".to_string(),
                event: service_update(
                    "mesh.audio",
                    "@mesh/pipewire-audio",
                    serde_json::json!({ "available": true, "percent": 44.0 }),
                ),
            },
        )
        .unwrap();
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 45.0 }),
        ))
        .unwrap();

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "volume": 0.4 }),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(rx.try_recv().unwrap().command, "set_volume");
    assert!(
        shell.build_debug_snapshot().profiling.is_none(),
        "profiling-disabled backend attribution paths must stay silent"
    );
}

#[test]
fn profiling_snapshot_tracks_bounded_backend_samples_by_provider() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    for index in 0..20 {
        shell.record_backend_profiling_stage(
            "mesh.audio",
            "@mesh/pulse",
            ProfilingBackendStage::PollUpdate,
            std::time::Duration::from_micros(10 + index),
            Some("service_update"),
        );
    }
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pulse",
        ProfilingBackendStage::CommandHandling,
        std::time::Duration::from_micros(44),
        Some("service_command"),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pulse",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(55),
        Some("service_publish"),
    );
    shell.record_backend_profiling_stage(
        "mesh.network",
        "@mesh/networkmanager",
        ProfilingBackendStage::PollUpdate,
        std::time::Duration::from_micros(33),
        Some("service_update"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");

    assert_eq!(profiling.backends.len(), 2);

    let audio_backend = profiling
        .backends
        .iter()
        .find(|backend| backend.interface == "mesh.audio" && backend.provider_id == "@mesh/pulse")
        .expect("backend profiling should be keyed by interface and provider");

    let poll_update = audio_backend
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingBackendStage::PollUpdate)
        .expect("poll/update stage should be captured");
    assert_eq!(poll_update.sample_count, 20);
    assert_eq!(poll_update.max_micros, 29);
    assert_eq!(poll_update.recent_samples.len(), 16);
    assert_eq!(
        poll_update
            .recent_samples
            .first()
            .map(|sample| sample.order),
        Some(4),
        "backend recent samples should retain only the newest bounded window"
    );
    assert!(
        poll_update
            .recent_samples
            .iter()
            .all(|sample| sample.stage == ProfilingBackendStage::PollUpdate)
    );

    assert!(
        audio_backend
            .stages
            .iter()
            .any(|stage| stage.stage == ProfilingBackendStage::CommandHandling)
    );
    assert!(
        audio_backend
            .stages
            .iter()
            .any(|stage| stage.stage == ProfilingBackendStage::StatePublishDelivery)
    );
}

#[test]
fn profiling_snapshot_groups_backend_stage_proof_under_expected_provider_identity() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut pending = std::collections::VecDeque::new();
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::BackendServiceUpdate {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/pipewire-audio".to_string(),
                event: service_update(
                    "mesh.audio",
                    "@mesh/pipewire-audio",
                    serde_json::json!({ "available": true, "percent": 44.0 }),
                ),
            },
        )
        .unwrap();
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 45.0 }),
        ))
        .unwrap();

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "volume": 0.4 }),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(rx.try_recv().unwrap().command, "set_volume");

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let backend = profiling
        .backends
        .iter()
        .find(|backend| {
            backend.interface == "mesh.audio" && backend.provider_id == "@mesh/pipewire-audio"
        })
        .expect("backend stages should stay grouped under the accepted provider identity");
    let stages: std::collections::HashSet<_> =
        backend.stages.iter().map(|stage| stage.stage).collect();

    assert_eq!(backend.interface, "mesh.audio");
    assert_eq!(backend.provider_id, "@mesh/pipewire-audio");
    assert!(stages.contains(&ProfilingBackendStage::PollUpdate));
    assert!(stages.contains(&ProfilingBackendStage::CommandHandling));
    assert!(stages.contains(&ProfilingBackendStage::StatePublishDelivery));
}

#[test]
fn profiling_snapshot_includes_required_shell_stage_buckets() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_shell_profiling_stage(
        ProfilingStage::InputHandling,
        std::time::Duration::from_micros(11),
        Some("key"),
    );
    shell.record_shell_profiling_stage(
        ProfilingStage::RuntimeUpdateHandling,
        std::time::Duration::from_micros(12),
        Some("service_event"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TreeBuild,
        std::time::Duration::from_micros(13),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::StyleRestyle,
        std::time::Duration::from_micros(14),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Layout,
        std::time::Duration::from_micros(15),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(16),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::PresentCommit,
        std::time::Duration::from_micros(17),
        Some("present"),
    );
    shell.record_surface_redraw(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        Some("present"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(18),
        Some("rebuild"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let stages: std::collections::HashSet<_> = profiling
        .shell
        .stages
        .iter()
        .map(|stage| stage.stage)
        .collect();

    assert!(stages.contains(&ProfilingStage::InputHandling));
    assert!(stages.contains(&ProfilingStage::RuntimeUpdateHandling));
    assert!(stages.contains(&ProfilingStage::TreeBuild));
    assert!(stages.contains(&ProfilingStage::StyleRestyle));
    assert!(stages.contains(&ProfilingStage::Layout));
    assert!(stages.contains(&ProfilingStage::Paint));
    assert!(stages.contains(&ProfilingStage::PresentCommit));
    assert!(stages.contains(&ProfilingStage::RedrawCount));
    assert!(stages.contains(&ProfilingStage::TotalSurfaceRender));
}

#[test]
fn profiling_snapshot_uses_surface_id_as_canonical_key_and_skips_unworked_surfaces() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_shell_profiling_stage(
        ProfilingStage::InputHandling,
        std::time::Duration::from_micros(9),
        Some("key"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    assert!(
        profiling.surfaces.is_empty(),
        "shell-only work must not fabricate per-surface summaries"
    );

    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(22),
        Some("rebuild"),
    );
    shell.record_surface_redraw(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        Some("present"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let surface = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/audio-popover")
        .expect("worked surfaces must use surface_id as the canonical key");
    assert_eq!(surface.module_id.as_deref(), Some("@mesh/audio-popover"));
    assert_eq!(surface.redraw_count, 1);
}

#[test]
fn profiling_snapshot_backfills_surface_module_id_after_empty_stage_metadata() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some(""),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(22),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        ProfilingStage::PresentCommit,
        std::time::Duration::from_micros(9),
        Some("present"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let surface = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/audio-popover")
        .expect("worked surfaces must retain their canonical surface key");
    assert_eq!(surface.module_id.as_deref(), Some("@mesh/audio-popover"));
    assert!(
        surface.stages.iter().any(|stage| stage
            .recent_samples
            .iter()
            .all(|sample| { sample.surface_id.as_deref() == Some("@mesh/audio-popover") })),
        "surface samples must retain explicit surface keys while module ids recover"
    );
}

#[test]
fn debug_snapshot_orders_backend_and_surface_profiling_deterministically() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_profiling_stage(
        "@mesh/z-popover",
        Some("@mesh/z-popover"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(30),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/a-panel",
        Some("@mesh/a-panel"),
        ProfilingStage::Layout,
        std::time::Duration::from_micros(12),
        Some("rebuild"),
    );
    shell.record_backend_profiling_stage(
        "mesh.network",
        "@mesh/networkmanager",
        ProfilingBackendStage::PollUpdate,
        std::time::Duration::from_micros(25),
        Some("service_update"),
    );
    shell.record_backend_state_publish_delivery(
        "mesh.audio",
        "@mesh/pipewire-audio",
        std::time::Duration::from_micros(18),
        Some("broadcast_service_event"),
    );

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");

    assert_eq!(
        profiling
            .surfaces
            .iter()
            .map(|surface| surface.surface_id.as_str())
            .collect::<Vec<_>>(),
        vec!["@mesh/a-panel", "@mesh/z-popover"]
    );
    assert_eq!(
        profiling
            .backends
            .iter()
            .map(|backend| (backend.interface.as_str(), backend.provider_id.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("mesh.audio", "@mesh/pipewire-audio"),
            ("mesh.network", "@mesh/networkmanager"),
        ]
    );
    assert_eq!(
        profiling
            .shell
            .stages
            .iter()
            .find(|stage| stage.stage == ProfilingStage::Paint)
            .map(|stage| stage.total_micros),
        Some(30)
    );
    assert_eq!(
        profiling
            .surfaces
            .iter()
            .find(|surface| surface.surface_id == "@mesh/a-panel")
            .and_then(|surface| surface.module_id.as_deref()),
        Some("@mesh/a-panel")
    );
    assert!(
        profiling.backends.iter().any(|backend| {
            backend.interface == "mesh.audio"
                && backend
                    .stages
                    .iter()
                    .any(|stage| stage.stage == ProfilingBackendStage::StatePublishDelivery)
        }),
        "backend summaries must coexist beside shell and per-surface totals in one snapshot"
    );
}

#[test]
fn keyboard_shortcuts_shell_global_shortcuts_still_win() {
    assert!(matches!(
        shell_global_shortcut_request("d", true, true, false),
        Some(CoreRequest::ToggleDebugOverlay)
    ));
    assert!(matches!(
        shell_global_shortcut_request("Tab", true, false, true),
        Some(CoreRequest::CycleDebugTab)
    ));
}

#[test]
fn keyboard_regression_shell_global_shortcut_precedence_stays_global() {
    assert!(matches!(
        shell_global_shortcut_request("d", true, true, false),
        Some(CoreRequest::ToggleDebugOverlay)
    ));
    assert!(matches!(
        shell_global_shortcut_request("Tab", true, false, true),
        Some(CoreRequest::CycleDebugTab)
    ));
    assert!(shell_global_shortcut_request("m", false, false, false).is_none());
}

#[test]
fn selection_clipboard_shell_request_writes_text() {
    let mut shell = Shell::new();
    let writes = Arc::new(Mutex::new(Vec::new()));
    shell.clipboard = Box::new(RecordingClipboard {
        writes: writes.clone(),
    });

    shell
        .apply_request(CoreRequest::WriteClipboard {
            text: "proof copy".into(),
        })
        .unwrap();

    assert_eq!(writes.lock().unwrap().as_slice(), ["proof copy"]);
}

#[test]
fn activate_popover_can_immediately_enter_focus_chain() {
    let mut shell = Shell::new();
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state.clone(),
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/audio-popover",
        popover_state.clone(),
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/audio-popover".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "volume-button".into(),
            focus: true,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    assert_eq!(
        trigger_state.lock().unwrap().registered_popovers.as_slice(),
        [("volume-button".into(), "@mesh/audio-popover".into())]
    );
    assert_eq!(trigger_state.lock().unwrap().releases, 1);
    assert_eq!(
        popover_state.lock().unwrap().received_focus.as_slice(),
        [(
            TabFocusTarget::First,
            Some(("@mesh/navigation-bar".into(), "volume-button".into())),
            true,
        )]
    );
    assert_eq!(
        shell.keyboard_focus_surface.as_deref(),
        Some("@mesh/audio-popover")
    );
}

#[test]
fn leaving_popover_keeps_return_surface_as_keyboard_owner() {
    let mut shell = Shell::new();
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state.clone(),
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/audio-popover",
        popover_state.clone(),
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::TransferTabFocus {
            from_surface: "@mesh/audio-popover".into(),
            to_surface: "@mesh/navigation-bar".into(),
            target: TabFocusTarget::AtKey("volume-button".into()),
            return_target: None,
            target_closes_on_leave: false,
            close_source: Some("@mesh/audio-popover".into()),
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    assert_eq!(
        shell.keyboard_focus_surface.as_deref(),
        Some("@mesh/navigation-bar")
    );
    assert_eq!(
        shell
            .surfaces
            .get("@mesh/navigation-bar")
            .map(|surface| surface.keyboard_mode),
        Some(mesh_core_wayland::KeyboardMode::Exclusive)
    );
    assert_eq!(
        trigger_state.lock().unwrap().received_focus.as_slice(),
        [(TabFocusTarget::AtKey("volume-button".into()), None, false)]
    );
}

#[test]
fn pointer_click_claims_keyboard_owner_without_forcing_exclusive_mode() {
    let mut shell = Shell::new();
    let nav_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        nav_state.clone(),
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/audio-popover",
        popover_state.clone(),
    )));
    shell.keyboard_focus_surface = Some("@mesh/audio-popover".into());
    shell
        .surfaces
        .get_mut("@mesh/navigation-bar")
        .unwrap()
        .keyboard_mode = mesh_core_wayland::KeyboardMode::OnDemand;

    shell.claim_keyboard_focus_for_surface("@mesh/navigation-bar");

    assert_eq!(
        shell.keyboard_focus_surface.as_deref(),
        Some("@mesh/navigation-bar")
    );
    assert_eq!(
        shell
            .surfaces
            .get("@mesh/navigation-bar")
            .map(|surface| surface.keyboard_mode),
        Some(mesh_core_wayland::KeyboardMode::OnDemand)
    );
    assert_eq!(
        shell
            .surfaces
            .get("@mesh/audio-popover")
            .map(|surface| surface.keyboard_mode),
        Some(mesh_core_wayland::KeyboardMode::Exclusive)
    );
    assert_eq!(
        popover_state
            .lock()
            .unwrap()
            .keyboard_mode_overrides
            .as_slice(),
        [None]
    );
    assert_eq!(
        nav_state.lock().unwrap().keyboard_mode_overrides.as_slice(),
        [None]
    );
}

#[test]
fn pointer_click_inside_keyboard_owner_preserves_exclusive_override() {
    let mut shell = Shell::new();
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/audio-popover",
        popover_state.clone(),
    )));
    shell.keyboard_focus_surface = Some("@mesh/audio-popover".into());

    shell.claim_keyboard_focus_for_surface("@mesh/audio-popover");

    assert_eq!(
        shell.keyboard_focus_surface.as_deref(),
        Some("@mesh/audio-popover")
    );
    assert!(
        popover_state
            .lock()
            .unwrap()
            .keyboard_mode_overrides
            .is_empty(),
        "clicking the already-focused popover must not clear its Exclusive keyboard override"
    );
}

#[test]
fn pointer_click_after_transfer_clears_transfer_forced_exclusive_override() {
    let mut shell = Shell::new();
    let nav_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        nav_state.clone(),
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/audio-popover",
        popover_state.clone(),
    )));
    shell.keyboard_focus_surface = Some("@mesh/navigation-bar".into());
    let mut emitted = shell
        .apply_request(CoreRequest::TransferTabFocus {
            from_surface: "@mesh/navigation-bar".into(),
            to_surface: "@mesh/audio-popover".into(),
            target: TabFocusTarget::First,
            return_target: Some(("@mesh/navigation-bar".into(), "volume-button".into())),
            target_closes_on_leave: true,
            close_source: None,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    assert_eq!(
        shell
            .surfaces
            .get("@mesh/audio-popover")
            .map(|surface| surface.keyboard_mode),
        Some(mesh_core_wayland::KeyboardMode::Exclusive)
    );
    assert_eq!(
        popover_state
            .lock()
            .unwrap()
            .keyboard_mode_overrides
            .as_slice(),
        [Some(mesh_core_wayland::KeyboardMode::Exclusive)]
    );

    shell.claim_keyboard_focus_for_surface("@mesh/audio-popover");

    assert_eq!(
        popover_state
            .lock()
            .unwrap()
            .keyboard_mode_overrides
            .as_slice(),
        [Some(mesh_core_wayland::KeyboardMode::Exclusive), None]
    );
    assert_eq!(
        shell
            .surfaces
            .get("@mesh/audio-popover")
            .map(|surface| surface.keyboard_mode),
        Some(mesh_core_wayland::KeyboardMode::OnDemand)
    );
}

#[test]
fn latest_service_state_tracks_provider_metadata_separately() {
    let mut shell = Shell::new();

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 65.0, "muted": false }),
        ))
        .unwrap();

    let latest = shell.latest_service_state.get("mesh.audio").unwrap();
    assert_eq!(latest.provider_id, "@mesh/pipewire-audio");
    assert_eq!(latest.state["available"], serde_json::json!(true));
    assert!(latest.state.get("source_module").is_none());
}

#[test]
fn provider_swap_replaces_interface_latest_state() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (pipewire_slot, _pipewire_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), pipewire_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 40.0 }),
        ))
        .unwrap();

    let (pulse_slot, _pulse_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pulseaudio-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), pulse_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pulseaudio-audio",
            serde_json::json!({ "available": true, "percent": 55.0 }),
        ))
        .unwrap();

    assert_eq!(shell.latest_service_state.len(), 1);
    assert!(shell.latest_service_state.contains_key("mesh.audio"));
    let latest = shell.latest_service_state.get("mesh.audio").unwrap();
    assert_eq!(latest.interface, "mesh.audio");
    assert_eq!(latest.provider_id, "@mesh/pulseaudio-audio");
    assert_eq!(latest.state["percent"], serde_json::json!(55.0));
    assert!(
        !shell
            .latest_service_state
            .values()
            .any(|latest| latest.provider_id == "@mesh/pipewire-audio")
    );
}

#[test]
fn stale_provider_update_does_not_replace_current_latest_state() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (pipewire_slot, _pipewire_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), pipewire_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 40.0 }),
        ))
        .unwrap();

    let (pulse_slot, _pulse_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pulseaudio-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), pulse_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pulseaudio-audio",
            serde_json::json!({ "available": true, "percent": 55.0 }),
        ))
        .unwrap();
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 5.0 }),
        ))
        .unwrap();

    let latest = shell.latest_service_state.get("mesh.audio").unwrap();
    assert_eq!(latest.provider_id, "@mesh/pulseaudio-audio");
    assert_eq!(latest.state["percent"], serde_json::json!(55.0));
}

#[test]
fn stale_provider_update_does_not_reach_components() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 40.0 }),
        ))
        .unwrap();

    let (new_slot, _new_rx) =
        backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pulseaudio-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pulseaudio-audio",
            serde_json::json!({ "available": true, "percent": 55.0 }),
        ))
        .unwrap();
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 5.0 }),
        ))
        .unwrap();

    let events = seen_events.lock().unwrap();
    assert_eq!(events.len(), 2);
    let ServiceEvent::Updated {
        source_module,
        payload,
        ..
    } = &events[0];
    assert_eq!(source_module, "@mesh/pipewire-audio");
    assert_eq!(payload["percent"], serde_json::json!(40.0));
    let ServiceEvent::Updated {
        source_module,
        payload,
        ..
    } = events.last().unwrap();
    assert_eq!(source_module, "@mesh/pulseaudio-audio");
    assert_eq!(payload["percent"], serde_json::json!(55.0));
}

#[test]
fn profiling_backend_poll_update_attributes_accepted_backend_messages() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut pending = std::collections::VecDeque::new();

    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::BackendServiceUpdate {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/pipewire-audio".to_string(),
                event: service_update(
                    "mesh.audio",
                    "@mesh/pipewire-audio",
                    serde_json::json!({ "available": true, "percent": 44.0 }),
                ),
            },
        )
        .unwrap();

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let backend = profiling
        .backends
        .iter()
        .find(|backend| {
            backend.interface == "mesh.audio" && backend.provider_id == "@mesh/pipewire-audio"
        })
        .expect("accepted backend updates should record provider-attributed profiling");
    let stage = backend
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingBackendStage::PollUpdate)
        .expect("poll/update stage should be recorded for accepted backend work");
    assert_eq!(stage.sample_count, 1);
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.audio")
            .unwrap()
            .provider_id,
        "@mesh/pipewire-audio"
    );
}

#[test]
fn profiling_backend_poll_update_ignores_stale_backend_messages() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);
    let mut pending = std::collections::VecDeque::new();

    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::BackendServiceUpdate {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/old-audio".to_string(),
                event: service_update(
                    "mesh.audio",
                    "@mesh/old-audio",
                    serde_json::json!({ "available": true, "percent": 12.0 }),
                ),
            },
        )
        .unwrap();

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    assert!(
        profiling.backends.is_empty(),
        "stale backend updates must not create poll/update samples"
    );
}

#[test]
fn profiling_state_publish_delivery_attributes_accepted_service_updates() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 40.0 }),
        ))
        .unwrap();

    assert_eq!(seen_events.lock().unwrap().len(), 1);
    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let backend = profiling
        .backends
        .iter()
        .find(|backend| {
            backend.interface == "mesh.audio" && backend.provider_id == "@mesh/pipewire-audio"
        })
        .expect("accepted service updates should record backend publish/delivery profiling");
    let stage = backend
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingBackendStage::StatePublishDelivery)
        .expect("publish/delivery stage should be recorded for accepted service updates");
    assert_eq!(stage.sample_count, 1);
    assert!(
        stage
            .recent_samples
            .iter()
            .all(|sample| sample.trigger_kind.as_deref() == Some("broadcast_service_event"))
    );
}

#[test]
fn profiling_state_publish_delivery_ignores_stale_service_updates() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/old-audio",
            serde_json::json!({ "available": true, "percent": 12.0 }),
        ))
        .unwrap();

    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    assert!(
        profiling.backends.is_empty(),
        "stale service updates must not create publish/delivery samples"
    );
}

#[test]
fn terminal_provider_update_does_not_replace_latest_state_or_reach_components() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 40.0 }),
        ))
        .unwrap();

    shell.stop_backend_runtime("mesh.audio");
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 5.0 }),
        ))
        .unwrap();

    assert_eq!(seen_events.lock().unwrap().len(), 1);
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.audio")
            .and_then(|state| state.state.get("percent")),
        Some(&serde_json::json!(40.0))
    );
}

#[test]
fn shell_theme_update_is_authoritative_when_theme_provider_is_active() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), slot);

    shell
        .broadcast_service_event(service_update(
            "mesh.theme",
            "@mesh/shell",
            serde_json::json!({
                "current": "mesh-default-light",
                "theme_id": "mesh-default-light",
                "is_dark": false,
            }),
        ))
        .unwrap();

    assert_eq!(seen_events.lock().unwrap().len(), 1);
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("current")),
        Some(&serde_json::json!("mesh-default-light"))
    );
}

#[test]
fn shell_theme_backend_candidate_receives_resolved_active_theme_setting() {
    let mut shell = Shell::new();
    shell.settings.theme.active = "missing-theme".to_string();
    let (theme, theme_watch) = load_active_theme(&shell.settings);
    shell.theme = theme;
    shell.theme_watch = theme_watch;
    let mut candidate = BackendLaunchCandidate {
        module_id: "@mesh/shell-theme".to_string(),
        interface: "mesh.theme".to_string(),
        service_name: "theme".to_string(),
        entrypoint_path: PathBuf::from("src/main.luau"),
        script_source: String::new(),
        capabilities: Vec::new(),
        settings: serde_json::json!({}),
    };

    shell.apply_shell_runtime_settings(&mut candidate);

    assert_eq!(shell.theme.active().id, "mesh-default-dark");
    assert_eq!(
        candidate
            .settings
            .get("current_theme")
            .and_then(|value| value.as_str()),
        Some("mesh-default-dark")
    );
}

#[test]
fn shell_theme_fallback_backend_restart_keeps_latest_state_on_resolved_theme() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell.settings.theme.active = "missing-theme".to_string();
    let (theme, theme_watch) = load_active_theme(&shell.settings);
    shell.theme = theme;
    shell.theme_watch = theme_watch;

    let mut candidate = BackendLaunchCandidate {
        module_id: "@mesh/shell-theme".to_string(),
        interface: "mesh.theme".to_string(),
        service_name: "theme".to_string(),
        entrypoint_path: PathBuf::from("src/main.luau"),
        script_source: String::new(),
        capabilities: Vec::new(),
        settings: serde_json::json!({}),
    };
    shell.apply_shell_runtime_settings(&mut candidate);
    let current_theme = candidate
        .settings
        .get("current_theme")
        .and_then(|value| value.as_str())
        .unwrap()
        .to_string();

    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.theme",
            "@mesh/shell-theme",
            serde_json::json!({
                "current": current_theme,
                "is_dark": true,
                "available": ["mesh-default-dark", "mesh-default-light"],
            }),
        ))
        .unwrap();

    let (replacement_slot, _replacement_rx) =
        backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), replacement_slot);
    shell
        .broadcast_service_event(service_update(
            "mesh.theme",
            "@mesh/shell-theme",
            serde_json::json!({
                "current": "mesh-default-dark",
                "is_dark": true,
                "available": ["mesh-default-dark", "mesh-default-light"],
            }),
        ))
        .unwrap();

    let latest = shell.latest_service_state.get("mesh.theme").unwrap();
    assert_eq!(shell.theme.active().id, "mesh-default-dark");
    assert_eq!(
        latest.state["current"],
        serde_json::json!("mesh-default-dark")
    );
    assert_eq!(latest.state["is_dark"], serde_json::json!(true));
}

#[test]
fn settings_theme_reload_syncs_theme_service_state() {
    let _env_lock = SETTINGS_ENV_LOCK.lock().unwrap();
    let runtime = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("shell-settings.json");
    fs::write(
        &settings_path,
        r#"{"theme":{"active":"mesh-default-dark"}}"#,
    )
    .unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), slot);

    fs::write(
        &settings_path,
        r#"{"theme":{"active":"mesh-default-light"}}"#,
    )
    .unwrap();
    shell.settings_watch.modified_at = None;
    shell.reload_locale_if_settings_changed().unwrap();

    assert_eq!(shell.settings.theme.active, "mesh-default-light");
    assert_eq!(seen_events.lock().unwrap().len(), 1);
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("current")),
        Some(&serde_json::json!("mesh-default-light"))
    );
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("is_dark")),
        Some(&serde_json::json!(false))
    );
}

#[test]
fn set_theme_forces_full_present_on_existing_components() {
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events),
        )));

    let mut light = mesh_core_theme::default_theme();
    light.id = "test-light-present".into();
    light.name = "test-light-present".into();
    shell.theme.register_theme(light);

    shell.apply_set_theme("test-light-present").unwrap();

    assert!(
        shell.components[0].force_full_present,
        "theme changes must force a full present for already-painted surfaces"
    );
}

#[test]
fn settings_theme_reload_publishes_resolved_fallback_theme_state() {
    let _env_lock = SETTINGS_ENV_LOCK.lock().unwrap();
    let runtime = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let settings_path = dir.path().join("shell-settings.json");
    fs::write(
        &settings_path,
        r#"{"theme":{"active":"mesh-default-dark"}}"#,
    )
    .unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), slot);

    fs::write(&settings_path, r#"{"theme":{"active":"missing-theme"}}"#).unwrap();
    shell.settings_watch.modified_at = None;
    shell.reload_locale_if_settings_changed().unwrap();

    assert_eq!(shell.settings.theme.active, "missing-theme");
    assert_eq!(shell.theme.active().id, "mesh-default-dark");
    assert_eq!(seen_events.lock().unwrap().len(), 1);
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("current")),
        Some(&serde_json::json!("mesh-default-dark"))
    );
}

#[test]
fn theme_file_recovery_syncs_mesh_theme_latest_state_and_components() {
    let _env_lock = SETTINGS_ENV_LOCK.lock().unwrap();
    let runtime = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let theme_dir = dir.path().join("themes");
    fs::create_dir_all(&theme_dir).unwrap();
    let settings_path = dir.path().join("shell-settings.json");
    fs::write(
        &settings_path,
        r#"{"theme":{"active":"mesh-recovered-light"}}"#,
    )
    .unwrap();
    let _settings_path = EnvGuard::set("MESH_SETTINGS_PATH", &settings_path);
    let _theme_dir = EnvGuard::set("MESH_THEME_DIR", &theme_dir);
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events.clone()),
        )));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.theme", "@mesh/shell-theme");
    shell.replace_backend_runtime("mesh.theme".to_string(), slot);

    assert_eq!(shell.settings.theme.active, "mesh-recovered-light");
    assert_eq!(shell.theme.active().id, "mesh-default-dark");
    let fallback_theme_id = shell.theme.active().id.clone();
    shell.sync_theme_service_state(&fallback_theme_id).unwrap();
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("current")),
        Some(&serde_json::json!("mesh-default-dark"))
    );
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("is_dark")),
        Some(&serde_json::json!(true))
    );

    fs::write(
        theme_dir.join("mesh-recovered-light.json"),
        r#"{"id":"mesh-recovered-light","name":"Recovered Light","tokens":{}}"#,
    )
    .unwrap();
    let requests = shell.reload_theme_if_changed().unwrap();

    assert!(requests.is_empty());
    assert_eq!(shell.theme.active().id, "mesh-recovered-light");
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("current")),
        Some(&serde_json::json!("mesh-recovered-light"))
    );
    assert_eq!(
        shell
            .latest_service_state
            .get("mesh.theme")
            .and_then(|state| state.state.get("is_dark")),
        Some(&serde_json::json!(false))
    );

    let events = seen_events.lock().unwrap();
    assert_eq!(events.len(), 2);
    let ServiceEvent::Updated { payload, .. } = events.last().unwrap();
    assert_eq!(
        payload["current"],
        serde_json::json!("mesh-recovered-light")
    );
    assert_eq!(payload["is_dark"], serde_json::json!(false));
}

#[test]
fn service_contract_provider_declaration_requires_provider_pair() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": true }
              },
              "providers": { "mesh.audio": "@mesh/backend" }
            }"#,
        vec![
            r#"{
                  "name": "@mesh/backend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "provides": [{ "interface": "mesh.audio", "provider": "test" }]
                  }
                }"#,
        ],
    );
    let (_dir, module) = module_instance("@mesh/backend", Some("src/main.luau"));
    let modules = HashMap::from([("@mesh/backend".to_string(), module)]);
    let interfaces = InterfaceRegistry::new();
    interfaces.register_contract(test_contract("mesh.audio"));

    let (candidates, statuses) =
        backend_launch_candidates_from_graph(&graph, &modules, &test_config(), &interfaces);

    assert!(candidates.is_empty());
    assert!(statuses.iter().any(|status| {
        status.status == "invalid_manifest"
            && status.provider_id.as_deref() == Some("@mesh/backend")
            && status.message.contains("not registered")
    }));

    register_test_provider(&interfaces, "mesh.audio", "@mesh/backend");
    let (candidates, statuses) =
        backend_launch_candidates_from_graph(&graph, &modules, &test_config(), &interfaces);

    assert_eq!(candidates.len(), 1);
    assert!(
        statuses
            .iter()
            .all(|status| status.provider_id.as_deref() != Some("@mesh/backend"))
    );
}

#[test]
fn state_shape_mismatch_records_service_contract_warning() {
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": "loud" }),
        ))
        .unwrap();

    let snapshot = shell.diagnostics.snapshot();
    assert!(snapshot.iter().any(|(module_id, health)| {
        module_id == "@mesh/pipewire-audio"
            && health.to_string().contains("service_contract_warning")
    }));
    let latest = shell.latest_service_state.get("mesh.audio").unwrap();
    assert!(latest.state.get("source_module").is_none());
}

#[test]
fn service_contract_unknown_service_command_returns_failure_result() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "explode",
        &serde_json::json!({}),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(false));
    assert_eq!(
        result["status"],
        serde_json::json!("unsupported_service_command")
    );
    assert!(rx.try_recv().is_err());
    assert!(
        shell
            .diagnostics
            .snapshot()
            .iter()
            .any(|(module_id, health)| {
                module_id == "@mesh/panel"
                    && health.to_string().contains("unsupported_service_command")
            })
    );
}

#[test]
fn closed_service_command_channel_returns_unavailable_result() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    drop(rx);
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "volume": 0.4 }),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(false));
    assert_eq!(result["status"], serde_json::json!("service_unavailable"));
}

#[test]
fn profiling_service_command_attributes_active_provider_dispatch() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "volume": 0.4 }),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(
        rx.try_recv().unwrap().command,
        "set_volume",
        "the existing command dispatch path must stay intact"
    );
    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    let backend = profiling
        .backends
        .iter()
        .find(|backend| {
            backend.interface == "mesh.audio" && backend.provider_id == "@mesh/pipewire-audio"
        })
        .expect("active provider dispatch should be attributed");
    let stage = backend
        .stages
        .iter()
        .find(|stage| stage.stage == ProfilingBackendStage::CommandHandling)
        .expect("command-handling stage should be recorded");
    assert_eq!(stage.sample_count, 1);
}

#[test]
fn profiling_service_command_stays_silent_when_disabled() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_volume",
        &serde_json::json!({ "volume": 0.2 }),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(rx.try_recv().unwrap().command, "set_volume");
    assert!(
        shell.build_debug_snapshot().profiling.is_none(),
        "command attribution must stay inert while profiling is disabled"
    );
}

#[test]
fn launcher_content_size_ignores_root_surface_bounds() {
    let mut root = node("root", 0.0, 0.0, 640.0, 360.0);
    root.children.push(node("column", 12.0, 12.0, 336.0, 332.0));

    let launcher_layout = SurfaceLayoutSection {
        size_policy: Some("content_measured".into()),
        prefers_content_children_sizing: Some(true),
        min_width: Some(320),
        max_width: Some(640),
        min_height: Some(180),
        max_height: Some(420),
    };
    assert_eq!(
        measure_content_size(&root, 640, 360, Some(&launcher_layout)),
        (348, 344)
    );
}

#[test]
fn non_widget_surfaces_keep_fallback_size() {
    let mut root = node("root", 0.0, 0.0, 1920.0, 32.0);
    root.children.push(node("row", 0.0, 0.0, 640.0, 32.0));

    assert_eq!(measure_content_size(&root, 1920, 32, None), (1920, 32));
}

#[test]
fn installed_module_graph_exposes_shell_package_choices() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let graph = mesh_core_module::package::load_installed_module_graph(
        &workspace_root.join("config/package.json"),
    )
    .unwrap();

    assert_eq!(
        graph.active_provider("mesh.audio").unwrap().module_id,
        "@mesh/pipewire-audio"
    );
    assert_eq!(graph.backend_providers_for_interface("mesh.audio").len(), 2);
    assert!(graph.active_provider("mesh.network").is_none());
    assert!(graph.active_provider("mesh.power").is_none());
    assert_eq!(
        graph.backend_providers_for_interface("mesh.network").len(),
        0
    );
    assert_eq!(graph.backend_providers_for_interface("mesh.power").len(), 0);
    assert!(
        graph
            .frontend_modules()
            .iter()
            .all(|module| module.id != "@mesh/text-selection-proof")
    );
    assert_eq!(
        graph.module("@mesh/text-selection-proof").unwrap().enabled,
        false
    );

    let layout = graph.layout_entrypoint().unwrap();
    assert_eq!(layout.module_id, "@mesh/navigation-bar");
    assert_eq!(layout.entrypoint_id, "main");
}

#[test]
fn load_frontend_components_keeps_shell_shipped_debug_inspector_even_when_not_in_package_graph() {
    let mut shell = Shell::new();
    shell.discover_modules();
    shell.resolve_modules().unwrap();
    shell.load_frontend_components().unwrap();

    assert!(
        shell
            .components
            .iter()
            .any(|runtime| runtime.surface_id == "@mesh/debug-inspector"),
        "built-in debug inspector should load as a shell surface even when absent from config/package.json"
    );
}

#[test]
fn backend_lifecycle_uses_explicit_active_provider_from_package_graph() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let graph = mesh_core_module::package::load_installed_module_graph(
        &workspace_root.join("config/package.json"),
    )
    .unwrap();
    let (_pipewire_dir, pipewire) = module_instance("@mesh/pipewire-audio", Some("src/main.luau"));
    let (_pulse_dir, pulse) = module_instance("@mesh/pulseaudio-audio", Some("src/main.luau"));
    let modules = HashMap::from([
        ("@mesh/pipewire-audio".to_string(), pipewire),
        ("@mesh/pulseaudio-audio".to_string(), pulse),
    ]);

    let (candidates, statuses) = backend_launch_candidates_from_graph(
        &graph,
        &modules,
        &test_config(),
        &InterfaceRegistry::new(),
    );

    assert!(
        statuses
            .iter()
            .all(|status| status.status != "invalid_manifest")
    );
    assert_eq!(candidates.len(), 1);
    let audio = candidates
        .iter()
        .find(|candidate| candidate.interface == "mesh.audio")
        .unwrap();
    assert_eq!(audio.module_id, "@mesh/pipewire-audio");
    assert_eq!(audio.service_name, "audio");
    assert!(audio.entrypoint_path.ends_with("src/main.luau"));
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.interface == "mesh.audio")
    );
    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.module_id == "@mesh/pulseaudio-audio")
    );
}

#[test]
fn backend_lifecycle_rejects_missing_backend_entrypoint_before_launch() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": true }
              },
              "providers": { "mesh.audio": "@mesh/backend" }
            }"#,
        vec![
            r#"{
                  "name": "@mesh/backend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "provides": [{ "interface": "mesh.audio", "provider": "test" }]
                  }
                }"#,
        ],
    );
    let (_dir, module) = module_instance("@mesh/backend", None);
    let modules = HashMap::from([("@mesh/backend".to_string(), module)]);

    let (candidates, statuses) = backend_launch_candidates_from_graph(
        &graph,
        &modules,
        &test_config(),
        &InterfaceRegistry::new(),
    );

    assert!(candidates.is_empty());
    assert!(statuses.iter().any(|status| {
        status.status == "missing_entrypoint"
            && status.provider_id.as_deref() == Some("@mesh/backend")
    }));
}

#[test]
fn backend_lifecycle_excludes_disabled_backend_modules() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/frontend": { "kind": "frontend", "path": "@mesh/frontend", "enabled": true },
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": false }
              },
              "providers": {}
            }"#,
        vec![
            r#"{
                  "name": "@mesh/frontend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "frontend",
                    "dependencies": { "backend": { "mesh.audio": ">=1.0.0" } }
                  }
                }"#,
            r#"{
                  "name": "@mesh/backend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "provides": [{ "interface": "mesh.audio", "provider": "test" }]
                  }
                }"#,
        ],
    );
    let (_dir, module) = module_instance("@mesh/backend", Some("src/main.luau"));
    let modules = HashMap::from([("@mesh/backend".to_string(), module)]);

    let (candidates, statuses) = backend_launch_candidates_from_graph(
        &graph,
        &modules,
        &test_config(),
        &InterfaceRegistry::new(),
    );

    assert!(candidates.is_empty());
    assert!(statuses.iter().any(|status| {
        status.status == "unmet_backend_requirement" && status.interface == "mesh.audio"
    }));
}

#[test]
fn backend_lifecycle_reports_frontend_requirement_without_active_provider() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/frontend": { "kind": "frontend", "path": "@mesh/frontend", "enabled": true },
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": true }
              },
              "providers": {}
            }"#,
        vec![
            r#"{
                  "name": "@mesh/frontend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "frontend",
                    "dependencies": { "backend": { "mesh.audio": ">=1.0.0" } }
                  }
                }"#,
            r#"{
                  "name": "@mesh/backend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "provides": [{ "interface": "mesh.audio", "provider": "test" }]
                  }
                }"#,
        ],
    );
    let (_dir, module) = module_instance("@mesh/backend", Some("src/main.luau"));
    let modules = HashMap::from([("@mesh/backend".to_string(), module)]);

    let (candidates, statuses) = backend_launch_candidates_from_graph(
        &graph,
        &modules,
        &test_config(),
        &InterfaceRegistry::new(),
    );

    assert!(candidates.is_empty());
    assert!(statuses.iter().any(|status| {
        status.status == "no_active_provider" && status.interface == "mesh.audio"
    }));
}

#[test]
fn backend_lifecycle_reports_frontend_requirement_without_installed_provider() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/frontend": { "kind": "frontend", "path": "@mesh/frontend", "enabled": true }
              },
              "providers": {}
            }"#,
        vec![
            r#"{
                  "name": "@mesh/frontend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "frontend",
                    "dependencies": { "backend": { "mesh.network": ">=1.0.0" } }
                  }
                }"#,
        ],
    );

    let (candidates, statuses) = backend_launch_candidates_from_graph(
        &graph,
        &HashMap::new(),
        &test_config(),
        &InterfaceRegistry::new(),
    );

    assert!(candidates.is_empty());
    assert!(statuses.iter().any(|status| {
        status.status == "unmet_backend_requirement" && status.interface == "mesh.network"
    }));
}

#[test]
fn backend_lifecycle_status_names_match_phase_contract() {
    let statuses = [
        BackendRuntimeStatus::NoActiveProvider,
        BackendRuntimeStatus::UnmetBackendRequirement,
        BackendRuntimeStatus::InvalidManifest,
        BackendRuntimeStatus::MissingEntrypoint,
        BackendRuntimeStatus::MissingBinary,
        BackendRuntimeStatus::InitFailed,
        BackendRuntimeStatus::Running,
        BackendRuntimeStatus::PollFailed,
        BackendRuntimeStatus::Failed,
        BackendRuntimeStatus::Stopped,
    ]
    .map(BackendRuntimeStatus::as_str);

    assert_eq!(
        statuses,
        [
            "no_active_provider",
            "unmet_backend_requirement",
            "invalid_manifest",
            "missing_entrypoint",
            "missing_binary",
            "init_failed",
            "running",
            "poll_failed",
            "failed",
            "stopped",
        ]
    );
}

#[test]
fn backend_lifecycle_replacement_removes_old_command_sender_before_insert() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    let old_sender = old_slot.command_tx.clone();
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);

    let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    let new_sender = new_slot.command_tx.clone();
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);

    assert!(!old_sender.is_closed());
    assert!(!new_sender.is_closed());
    assert_eq!(
        shell
            .backend_runtimes
            .get("mesh.audio")
            .map(|slot| slot.provider_id.as_str()),
        Some("@mesh/new-audio")
    );
    assert!(shell.service_handlers.contains_key("mesh.audio"));
}

#[test]
fn backend_lifecycle_replacement_records_stopped_after_transient_poll_failure() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/old-audio".to_string(),
        BackendRuntimeStatus::PollFailed,
        "temporary poll failure".to_string(),
    );

    let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);

    assert_eq!(
        shell
            .backend_runtime_statuses
            .get(&("mesh.audio".to_string(), "@mesh/old-audio".to_string()))
            .map(|entry| entry.status.as_str()),
        Some("stopped")
    );
}

#[test]
fn backend_lifecycle_init_failure_removes_command_handler() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);

    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        "init".to_string(),
        "init_failed".to_string(),
        "init boom".to_string(),
    );

    assert!(!shell.service_handlers.contains_key("mesh.audio"));
    assert!(!shell.backend_runtimes.contains_key("mesh.audio"));
    assert_eq!(
        shell
            .backend_runtime_statuses
            .get(&("mesh.audio".to_string(), "@mesh/pipewire-audio".to_string()))
            .map(|entry| entry.status.as_str()),
        Some("init_failed")
    );
}

#[test]
fn stale_backend_lifecycle_event_does_not_stop_current_provider() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);

    let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);

    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/old-audio".to_string(),
        "poll".to_string(),
        "failed".to_string(),
        "old provider failed after replacement".to_string(),
    );

    assert!(shell.service_handlers.contains_key("mesh.audio"));
    assert_eq!(
        shell
            .backend_runtimes
            .get("mesh.audio")
            .map(|slot| slot.provider_id.as_str()),
        Some("@mesh/new-audio")
    );
    assert_eq!(
        shell
            .backend_runtime_statuses
            .get(&("mesh.audio".to_string(), "@mesh/old-audio".to_string()))
            .map(|entry| entry.status.as_str()),
        Some("failed")
    );
}

#[test]
fn backend_lifecycle_failed_runtime_does_not_start_fallback_provider() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);

    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        "poll".to_string(),
        "failed".to_string(),
        "poll boom".to_string(),
    );

    assert!(!shell.service_handlers.contains_key("mesh.audio"));
    assert!(
        !shell
            .backend_runtimes
            .values()
            .any(|slot| slot.provider_id == "@mesh/pulseaudio-audio")
    );
    assert_eq!(
        shell
            .backend_runtime_statuses
            .get(&("mesh.audio".to_string(), "@mesh/pipewire-audio".to_string()))
            .map(|entry| entry.status.as_str()),
        Some("failed")
    );
}

#[test]
fn debug_snapshot_includes_backend_lifecycle_status() {
    let mut shell = Shell::new();
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Running,
        "backend runtime started".to_string(),
    );

    let snapshot = shell.build_debug_snapshot();
    assert!(snapshot.backend_runtimes.iter().any(|entry| {
        entry.interface == "mesh.audio"
            && entry.provider_id == "@mesh/pipewire-audio"
            && entry.status == "running"
    }));
}

#[test]
fn backend_lifecycle_debug_snapshot_includes_failure_counts() {
    let mut shell = Shell::new();
    // Record three poll failures for the same provider.
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::PollFailed,
        "poll failure 1".to_string(),
    );
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::PollFailed,
        "poll failure 2".to_string(),
    );
    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::PollFailed,
        "poll failure 3".to_string(),
    );

    let snapshot = shell.build_debug_snapshot();
    let entry = snapshot
        .backend_runtimes
        .iter()
        .find(|e| e.interface == "mesh.audio" && e.provider_id == "@mesh/pipewire-audio")
        .expect("backend runtime entry must be present in debug snapshot");

    assert_eq!(
        entry.failure_count, 3,
        "debug snapshot must include cumulative failure count for the provider"
    );
    assert_eq!(entry.status, "poll_failed");
    assert!(
        !entry.provider_id.is_empty(),
        "debug snapshot must include provider identity"
    );
}

#[test]
fn backend_runtime_status_records_provider_identity_for_failures() {
    let mut shell = Shell::new();
    shell.record_backend_runtime_status(
        "mesh.network".to_string(),
        "@mesh/networkmanager-network".to_string(),
        BackendRuntimeStatus::InitFailed,
        "dbus connection refused".to_string(),
    );

    // The runtime status map must record both provider identity and status.
    let key = (
        "mesh.network".to_string(),
        "@mesh/networkmanager-network".to_string(),
    );
    let entry = shell
        .backend_runtime_statuses
        .get(&key)
        .expect("runtime status must be recorded for the failed provider");
    assert_eq!(
        entry.provider_id, "@mesh/networkmanager-network",
        "runtime status must identify the failed provider"
    );
    assert_eq!(
        entry.interface, "mesh.network",
        "runtime status must identify the interface"
    );
    assert_eq!(
        entry.status.as_str(),
        "init_failed",
        "runtime status must record the lifecycle stage"
    );
    assert_eq!(
        entry.failure_count, 1,
        "first failure must set failure_count to 1"
    );
    assert!(
        entry.message.contains("dbus connection refused"),
        "runtime status must preserve the failure message"
    );

    // Additional failure increments the count.
    shell.record_backend_runtime_status(
        "mesh.network".to_string(),
        "@mesh/networkmanager-network".to_string(),
        BackendRuntimeStatus::InitFailed,
        "still failing".to_string(),
    );
    let entry = shell.backend_runtime_statuses.get(&key).unwrap();
    assert_eq!(
        entry.failure_count, 2,
        "repeated failure must increment failure_count"
    );
}

#[test]
fn active_provider_failure_clears_latest_service_state() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);

    // Inject a healthy service state for the active provider.
    let healthy_event = service_update(
        "mesh.audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 75, "muted": false }),
    );
    shell.record_latest_service_state(&healthy_event);
    {
        let latest = shell.latest_service_state.get("mesh.audio").unwrap();
        assert_eq!(latest.state["available"], true);
    }

    // Provider fails — should replace stale state with unavailable payload.
    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        "poll".to_string(),
        "failed".to_string(),
        "poll boom".to_string(),
    );

    let latest = shell.latest_service_state.get("mesh.audio").unwrap();
    assert_eq!(
        latest.state["available"], false,
        "active provider failure must set available=false in latest_service_state"
    );
    assert_eq!(latest.provider_id, "@mesh/pipewire-audio");
}

#[test]
fn stale_provider_failure_does_not_clear_new_provider_state() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    let (old_slot, _old_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/old-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), old_slot);

    let (new_slot, _new_rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/new-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), new_slot);

    // New provider emits healthy state.
    let healthy_event = service_update(
        "mesh.audio",
        "@mesh/new-audio",
        serde_json::json!({ "available": true, "percent": 50 }),
    );
    shell.record_latest_service_state(&healthy_event);

    // Old (stale) provider reports failure — must NOT clear new provider's state.
    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/old-audio".to_string(),
        "poll".to_string(),
        "failed".to_string(),
        "old provider late failure".to_string(),
    );

    // New provider's state must remain intact.
    let latest = shell.latest_service_state.get("mesh.audio").unwrap();
    assert_eq!(
        latest.provider_id, "@mesh/new-audio",
        "stale provider failure must not replace new provider state"
    );
    assert_eq!(latest.state["available"], true);
}

#[test]
fn frontend_settings_override_surface_layout_defaults() {
    let path = unique_test_file("surface-layout");
    fs::write(
        &path,
        r#"{
  "surface": {
    "anchor": "left",
    "layer": "overlay",
    "width": 960,
    "height": 640,
    "exclusive_zone": 12,
    "keyboard_mode": "exclusive",
    "visible_on_start": true
  }
}"#,
    )
    .unwrap();

    let manifest = minimal_manifest("@mesh/base-surface");
    let settings = load_frontend_module_settings(&path, &manifest);
    fs::remove_file(&path).ok();

    assert_eq!(settings.layout.edge, mesh_core_wayland::Edge::Left);
    assert_eq!(settings.layout.layer, mesh_core_wayland::Layer::Overlay);
    assert_eq!(settings.layout.width, 960);
    assert_eq!(settings.layout.height, 640);
    assert_eq!(settings.layout.exclusive_zone, 12);
    assert_eq!(
        settings.layout.keyboard_mode,
        mesh_core_wayland::KeyboardMode::Exclusive
    );
    assert!(settings.layout.visible_on_start);
    assert_eq!(settings.layout.size_policy, SurfaceSizePolicy::Fixed);
}

#[test]
fn service_update_populates_frontend_state() {
    let mut state = ScriptState::new();
    seed_service_state(&mut state);
    apply_service_update(
        &mut state,
        true,
        "audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 65, "label": "65%" }),
    );

    let audio = state.get("audio").expect("audio state should exist");
    assert_eq!(audio.get("label").and_then(|v| v.as_str()), Some("65%"));
    assert_eq!(audio.get("percent").and_then(|v| v.as_u64()), Some(65));
}

#[test]
fn service_update_gated_by_capability() {
    let mut state = ScriptState::new();
    seed_service_state(&mut state);
    apply_service_update(
        &mut state,
        false, // no audio.read capability
        "audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 99 }),
    );
    assert!(state.get("audio").is_none());
}

#[test]
fn service_update_accepts_canonical_interface_name() {
    let mut state = ScriptState::new();
    seed_service_state(&mut state);
    apply_service_update(
        &mut state,
        true,
        "mesh.audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 42 }),
    );
    assert_eq!(
        state
            .get("last_service_update")
            .and_then(|v| v.get("name").cloned())
            .and_then(|v| v.as_str().map(str::to_string)),
        Some("audio".to_string())
    );
}

#[test]
fn normalizes_service_names_from_interfaces() {
    assert_eq!(service_name_from_interface("mesh.audio"), "audio");
    assert_eq!(service_name_from_interface("audio"), "audio");
}

#[test]
fn core_crate_boundaries_do_not_regress() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");

    let frontend = manifest_dependencies(&root.join("crates/core/frontend/compiler/Cargo.toml"));
    assert!(!frontend.contains("mesh-core-shell"));
    assert!(!frontend.contains("mesh-core-render"));
    assert!(!frontend.contains("mesh-core-presentation"));

    let frontend_host = manifest_dependencies(&root.join("crates/core/frontend/host/Cargo.toml"));
    assert!(frontend_host.contains("mesh-core-render"));
    assert!(frontend_host.contains("mesh-core-wayland"));
    assert!(!frontend_host.contains("mesh-core-shell"));
    assert!(!frontend_host.contains("mesh-core-frontend"));
    assert!(!frontend_host.contains("mesh-core-presentation"));

    let animation = manifest_dependencies(&root.join("crates/core/ui/animation/Cargo.toml"));
    assert!(animation.contains("mesh-core-elements"));
    assert!(!animation.contains("mesh-core-shell"));
    assert!(!animation.contains("mesh-core-frontend"));
    assert!(!animation.contains("mesh-core-render"));

    let interaction = manifest_dependencies(&root.join("crates/core/ui/interaction/Cargo.toml"));
    assert!(interaction.contains("mesh-core-elements"));
    assert!(!interaction.contains("mesh-core-shell"));
    assert!(!interaction.contains("mesh-core-render"));
    assert!(!interaction.contains("mesh-core-presentation"));

    let render = manifest_dependencies(&root.join("crates/core/frontend/render/Cargo.toml"));
    assert!(render.contains("mesh-core-elements"));
    assert!(render.contains("mesh-core-icon"));
    assert!(!render.contains("mesh-core-shell"));
    assert!(!render.contains("mesh-core-frontend"));
    assert!(!render.contains("mesh-core-presentation"));

    let presentation = manifest_dependencies(&root.join("crates/core/presentation/Cargo.toml"));
    assert!(presentation.contains("mesh-core-render"));
    assert!(presentation.contains("mesh-core-wayland"));
    assert!(!presentation.contains("mesh-core-shell"));
    assert!(!presentation.contains("mesh-core-frontend"));

    let surface_config = manifest_dependencies(&root.join("crates/core/surface-config/Cargo.toml"));
    assert!(surface_config.contains("mesh-core-module"));
    assert!(surface_config.contains("mesh-core-wayland"));
    assert!(!surface_config.contains("mesh-core-shell"));
    assert!(!surface_config.contains("mesh-core-render"));
}

fn manifest_dependencies(path: &Path) -> String {
    let manifest = std::fs::read_to_string(path).expect("read crate manifest");
    manifest_section(&manifest, "[dependencies]")
}

fn manifest_section(manifest: &str, section: &str) -> String {
    let mut output = String::new();
    let mut in_section = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if in_section {
                break;
            }
            in_section = trimmed == section;
            continue;
        }
        if in_section {
            output.push_str(line);
            output.push('\n');
        }
    }
    output
}

fn unique_test_file(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("mesh-{prefix}-{nanos}.json"))
}
