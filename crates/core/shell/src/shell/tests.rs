use super::{
    BackendLaunchCandidate, BackendRuntimeSlot, BackendRuntimeStatus, ComponentInput, CoreRequest,
    InterfaceProvider, InterfaceRegistry, KeyModifiers, ServiceCommandMsg, ServiceEvent, Shell,
    TabFocusTarget, backend_launch_candidates_from_graph, component_key_pressed_input,
    component_key_released_input,
    layout::measure_content_size,
    service::{apply_service_update, seed_service_state, service_name_from_interface},
    shell_global_shortcut_request,
    surface_layout::{SurfaceSizePolicy, load_active_theme, load_frontend_module_settings},
};
use mesh_core_config::ShellConfig;
use mesh_core_debug::ProfilingStage;
use mesh_core_elements::{LayoutRect, VariableStore, WidgetNode};
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
use mesh_core_scripting::ScriptState;
use mesh_core_service::{
    ContractCapabilities, InterfaceContract, InterfaceMethod, contract::ContractStateField,
    parse_contract_version,
};
use mesh_core_wayland::{ClipboardError, ClipboardWriter};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
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

    shell.apply_request(CoreRequest::ToggleDebugProfiling).unwrap();
    assert!(shell.debug.profiling_enabled);
    assert_eq!(shell.debug.profiling_session_id, 1);

    shell.apply_request(CoreRequest::ToggleDebugProfiling).unwrap();
    assert!(!shell.debug.profiling_enabled);
    assert_eq!(
        shell.debug.profiling_session_id, 1,
        "disabling profiling should not fabricate a new session"
    );

    shell.apply_request(CoreRequest::ToggleDebugOverlay).unwrap();
    assert!(
        !shell.debug.profiling_enabled,
        "debug overlay visibility must remain independent from profiling state"
    );
}

#[test]
fn debug_snapshot_omits_profiling_payload_when_disabled() {
    let shell = Shell::new();
    let snapshot = shell.build_debug_snapshot();
    assert!(
        snapshot.profiling.is_none(),
        "profiling payload must be absent while profiling is disabled"
    );
}

#[test]
fn profiling_session_reset_discards_previous_samples() {
    let mut shell = Shell::new();

    shell.apply_request(CoreRequest::ToggleDebugProfiling).unwrap();
    shell.profiling.record_shell_stage(
        ProfilingStage::RuntimeUpdateHandling,
        std::time::Duration::from_micros(25),
        Some("service_update"),
    );
    let snapshot = shell.build_debug_snapshot();
    let profiling = snapshot.profiling.expect("profiling should be enabled");
    assert_eq!(profiling.session_id, 1);
    assert_eq!(profiling.shell.stages.len(), 1);

    shell.apply_request(CoreRequest::ToggleDebugProfiling).unwrap();
    shell.apply_request(CoreRequest::ToggleDebugProfiling).unwrap();

    let reset_snapshot = shell.build_debug_snapshot();
    let profiling = reset_snapshot
        .profiling
        .expect("profiling should be enabled after the second toggle");
    assert_eq!(profiling.session_id, 2);
    assert!(
        profiling.shell.stages.is_empty(),
        "enabling a fresh profiling session must clear previous samples"
    );
}

#[test]
fn profiling_snapshot_tracks_bounded_surface_samples_and_redraw_counts() {
    let mut shell = Shell::new();
    shell.apply_request(CoreRequest::ToggleDebugProfiling).unwrap();

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

fn unique_test_file(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("mesh-{prefix}-{nanos}.json"))
}
