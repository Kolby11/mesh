use super::{
    BackendLaunchCandidate, BackendRuntimeSlot, BackendRuntimeStatus, ComponentInput, CoreRequest,
    InterfaceProvider, InterfaceRegistry, KeyModifiers, ServiceCommandMsg, ServiceEvent,
    ServiceInterfaceEventSubscription, ServiceObservationSummary, Shell, TabFocusTarget,
    backend_launch_candidates_from_graph, component_key_pressed_input,
    component_key_released_input,
    discovery::{
        discover_shell_module_manifest_dirs, load_shell_module_manifests,
        load_shell_module_manifests_serial,
    },
    ipc::parse_ipc_command,
    service::{
        apply_service_update, script_events_to_requests, seed_service_state,
        service_name_from_interface,
    },
    shell_global_shortcut_request,
    surface_layout::{load_active_theme, load_frontend_module_settings},
};
use mesh_core_config::ShellConfig;
use mesh_core_debug::{
    ComponentInvalidationCounts, DisplayBatchBarrierSnapshot, ProfilingBackendStage,
    ProfilingInvalidationSnapshot, ProfilingStage, RepaintPolicySnapshot,
    RetainedInvalidationCounts, RetainedPaintSnapshot, TextCacheSnapshot,
};
use mesh_core_elements::{LayoutRect, VariableStore, WidgetNode};
use mesh_core_interaction::measure_content_size;
use mesh_core_module::ModuleInstance;
use mesh_core_module::manifest::{
    CapabilitiesSection, CompatibilitySection, DependenciesSection, EntrypointsSection,
    ExportsSection, Manifest, ManifestSource, ModuleSection, ModuleType, ProvidedInterface,
    SurfaceLayoutSection,
};
use mesh_core_module::package::{
    InstalledModuleGraph, LoadedModuleManifest, ModuleManifest, ModuleManifestSource,
    RootModuleGraphManifest,
};
use mesh_core_scripting::{PublishedEvent, ScriptState};
use mesh_core_service::{
    ContractCapabilities, InterfaceArgument, InterfaceContract, InterfaceEvent, InterfaceMethod,
    contract::ContractStateField, parse_contract_version,
};
use mesh_core_wayland::{ClipboardError, ClipboardWriter};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
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
        package: ModuleSection {
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
        keybinds: mesh_core_module::KeybindsSection::default(),
        i18n: None,
        theme: None,
        service: None,
        provides: Vec::new(),
        interface: None,
        interfaces: Vec::new(),
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
        dir.path().join("module.json"),
        ManifestSource::CanonicalModuleJson,
    );
    (dir, instance)
}

fn write_shell_discovery_manifest(module_dir: &Path, id: &str, payload_count: usize) {
    fs::create_dir_all(module_dir).unwrap();
    let mut optional_capabilities = String::new();
    for index in 0..payload_count {
        optional_capabilities.push_str(&format!(r#""service.demo.{index}""#));
        if index + 1 < payload_count {
            optional_capabilities.push(',');
        }
    }
    fs::write(
        module_dir.join("module.json"),
        format!(
            r#"{{
  "name": "{id}",
  "version": "0.1.0",
  "mesh": {{
    "apiVersion": "0.1",
    "kind": "frontend",
    "entry": "src/main.mesh",
    "capabilities": {{
      "required": ["shell.surface"],
      "optional": [{optional_capabilities}]
    }}
  }}
}}"#
        ),
    )
    .unwrap();
}

#[test]
fn shell_module_manifest_discovery_is_deterministic_and_stops_at_module_roots() {
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    write_shell_discovery_manifest(&first_root.path().join("zeta"), "@test/zeta", 0);
    write_shell_discovery_manifest(&first_root.path().join("alpha"), "@test/alpha", 0);
    write_shell_discovery_manifest(
        &first_root.path().join("alpha/nested/ignored"),
        "@test/ignored",
        0,
    );
    write_shell_discovery_manifest(&second_root.path().join("beta"), "@test/beta", 0);

    let roots = vec![
        second_root.path().to_path_buf(),
        first_root.path().to_path_buf(),
    ];
    let discovered = discover_shell_module_manifest_dirs(&roots);
    let relative = discovered
        .iter()
        .map(|path| {
            if let Ok(path) = path.strip_prefix(second_root.path()) {
                format!("second/{}", path.display())
            } else {
                format!(
                    "first/{}",
                    path.strip_prefix(first_root.path()).unwrap().display()
                )
            }
        })
        .collect::<Vec<_>>();

    assert_eq!(relative, vec!["second/beta", "first/alpha", "first/zeta"]);
}

#[test]
#[ignore = "release-only shell discovery manifest loading microbenchmark"]
fn shell_module_manifest_parallel_loading_beats_serial_benchmark() {
    if rayon::current_num_threads() <= 1 {
        eprintln!("skipping benchmark: rayon has one worker thread");
        return;
    }

    let root = tempfile::tempdir().unwrap();
    let module_count = 192;
    for index in 0..module_count {
        write_shell_discovery_manifest(
            &root.path().join(format!("frontend/module-{index:03}")),
            &format!("@bench/shell-module-{index:03}"),
            24,
        );
    }

    let module_dirs = discover_shell_module_manifest_dirs(&[root.path().to_path_buf()]);
    assert_eq!(module_dirs.len(), module_count);
    let warmup = load_shell_module_manifests(&module_dirs);
    assert_eq!(warmup.len(), module_count);
    assert!(warmup.iter().all(|result| result.loaded.is_ok()));

    let iterations = 12;
    let serial_start = Instant::now();
    for _ in 0..iterations {
        let loaded = load_shell_module_manifests_serial(&module_dirs);
        assert_eq!(loaded.len(), module_count);
        assert!(loaded.iter().all(|result| result.loaded.is_ok()));
    }
    let serial_elapsed = serial_start.elapsed();

    let parallel_start = Instant::now();
    for _ in 0..iterations {
        let loaded = load_shell_module_manifests(&module_dirs);
        assert_eq!(loaded.len(), module_count);
        assert!(loaded.iter().all(|result| result.loaded.is_ok()));
    }
    let parallel_elapsed = parallel_start.elapsed();

    eprintln!(
        "shell manifest load over {iterations} iterations and {module_count} modules: serial {serial_elapsed:?}; parallel {parallel_elapsed:?}; ratio {:.1}x",
        serial_elapsed.as_secs_f64() / parallel_elapsed.as_secs_f64()
    );
    assert!(
        parallel_elapsed < serial_elapsed,
        "parallel shell manifest loading should beat serial loading"
    );
}

fn test_config() -> ShellConfig {
    ShellConfig {
        shell: Default::default(),
        modules: HashMap::new(),
    }
}

fn loaded_module(json: &str) -> LoadedModuleManifest {
    LoadedModuleManifest {
        manifest: ModuleManifest::from_json_str(json).unwrap(),
        path: PathBuf::from("<test>/module.json"),
        source: ModuleManifestSource::CanonicalModuleJson,
        diagnostics: Vec::new(),
    }
}

fn graph_from_json(root: &str, modules: Vec<&str>) -> InstalledModuleGraph {
    let root = format!(
        r#"{{
              "name": "@mesh/test-config",
              "version": "0.1.0",
              "mesh": {root}
            }}"#
    );
    InstalledModuleGraph::from_parts(
        RootModuleGraphManifest::from_json_str(&root).unwrap(),
        modules.into_iter().map(loaded_module).collect(),
    )
    .unwrap()
}

fn test_contract(interface: &str) -> InterfaceContract {
    InterfaceContract {
        interface: interface.to_string(),
        version: parse_contract_version("1.0").unwrap(),
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
                name: "muted".to_string(),
                field_type: "boolean".to_string(),
                description: None,
            },
            ContractStateField {
                name: "source_module".to_string(),
                field_type: "string".to_string(),
                description: None,
            },
        ],
        methods: vec![
            InterfaceMethod {
                name: "set_volume".to_string(),
                args: Vec::new(),
                returns: Some("Result".to_string()),
                coalesce: false,
                optimistic: None,
            },
            InterfaceMethod {
                name: "set_muted".to_string(),
                args: vec![
                    InterfaceArgument {
                        name: "device_id".to_string(),
                        arg_type: "string".to_string(),
                    },
                    InterfaceArgument {
                        name: "muted".to_string(),
                        arg_type: "boolean".to_string(),
                    },
                ],
                returns: Some("Result".to_string()),
                coalesce: false,
                optimistic: Some(mesh_core_service::OptimisticUpdate {
                    field: "muted".to_string(),
                    from_arg: Some("muted".to_string()),
                }),
            },
        ],
        events: vec![InterfaceEvent {
            name: "VolumeChanged".to_string(),
            payload: vec![
                InterfaceArgument {
                    name: "device_id".to_string(),
                    arg_type: "string".to_string(),
                },
                InterfaceArgument {
                    name: "level".to_string(),
                    arg_type: "float".to_string(),
                },
            ],
        }],
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
    keybinds: Vec<mesh_core_debug::DebugKeybindEntry>,
}

impl RecordingComponent {
    fn new(events: Arc<Mutex<Vec<ServiceEvent>>>) -> Self {
        Self {
            events,
            keybinds: Vec::new(),
        }
    }

    fn with_keybinds(
        events: Arc<Mutex<Vec<ServiceEvent>>>,
        keybinds: Vec<mesh_core_debug::DebugKeybindEntry>,
    ) -> Self {
        Self { events, keybinds }
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
        _scale: f32,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn debug_keybinds(&self) -> Vec<mesh_core_debug::DebugKeybindEntry> {
        self.keybinds.clone()
    }
}

#[derive(Default)]
struct IndexedRecordingState {
    observed: usize,
    handled: Vec<ServiceEvent>,
}

struct IndexedRecordingComponent {
    id: String,
    summary: Arc<Mutex<Option<ServiceObservationSummary>>>,
    state: Arc<Mutex<IndexedRecordingState>>,
}

impl IndexedRecordingComponent {
    fn new(
        id: &str,
        summary: Arc<Mutex<Option<ServiceObservationSummary>>>,
        state: Arc<Mutex<IndexedRecordingState>>,
    ) -> Self {
        Self {
            id: id.to_string(),
            summary,
            state,
        }
    }
}

impl super::types::ShellComponent for IndexedRecordingComponent {
    fn id(&self) -> &str {
        &self.id
    }

    fn surface_id(&self) -> &str {
        &self.id
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
        self.state.lock().unwrap().handled.push(event.clone());
        Ok(Vec::new())
    }

    fn observes_service_event(&self, _event: &ServiceEvent) -> bool {
        self.state.lock().unwrap().observed += 1;
        let Some(summary) = self.summary.lock().unwrap().clone() else {
            return true;
        };
        indexed_summary_observes_event(&summary, _event)
    }

    fn service_observation_summary(&self) -> Option<ServiceObservationSummary> {
        self.summary.lock().unwrap().clone()
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
        _scale: f32,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }
}

fn indexed_summary_observes_event(
    summary: &ServiceObservationSummary,
    event: &ServiceEvent,
) -> bool {
    match event {
        ServiceEvent::Updated { service, .. } => {
            let service_name = service_name_from_interface(service);
            summary
                .update_services
                .iter()
                .any(|observed| observed == &service_name)
        }
        ServiceEvent::InterfaceEvent { service, name, .. } => {
            let service_name = service_name_from_interface(service);
            summary
                .interface_events
                .iter()
                .any(|observed| observed.service == service_name && observed.event == *name)
        }
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
    popover_margin_left: i32,
}

impl FocusRecordingComponent {
    fn new(surface_id: &str, state: Arc<Mutex<FocusRecordingState>>) -> Self {
        Self {
            surface_id: surface_id.to_string(),
            state,
            popover_margin_left: 0,
        }
    }

    fn with_popover_margin_left(
        surface_id: &str,
        state: Arc<Mutex<FocusRecordingState>>,
        popover_margin_left: i32,
    ) -> Self {
        Self {
            surface_id: surface_id.to_string(),
            state,
            popover_margin_left,
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

    fn initial_visibility(&self) -> Option<bool> {
        Some(true)
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
        _scale: f32,
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

    fn popover_margin_left(&self) -> i32 {
        self.popover_margin_left
    }
}

#[derive(Debug, Clone)]
struct PopoverHarnessState {
    open: bool,
    node_key: String,
    anchor_rect: (i32, i32, i32, i32),
    content_size: (u32, u32),
    painted_nodes: Vec<String>,
    exiting_paints: Vec<bool>,
    child_inputs: Vec<(String, ComponentInput)>,
    profiling_enabled: Vec<bool>,
    hide_transition_ms: u64,
}

impl Default for PopoverHarnessState {
    fn default() -> Self {
        Self {
            open: true,
            node_key: "root/popover".into(),
            anchor_rect: (8, 10, 40, 16),
            content_size: (72, 32),
            painted_nodes: Vec::new(),
            exiting_paints: Vec::new(),
            child_inputs: Vec::new(),
            profiling_enabled: Vec::new(),
            hide_transition_ms: 0,
        }
    }
}

struct PopoverHarnessComponent {
    surface_id: String,
    state: Arc<Mutex<PopoverHarnessState>>,
}

impl PopoverHarnessComponent {
    fn new(state: Arc<Mutex<PopoverHarnessState>>) -> Self {
        Self {
            surface_id: "@test/popover-host".into(),
            state,
        }
    }
}

impl super::types::ShellComponent for PopoverHarnessComponent {
    fn id(&self) -> &str {
        &self.surface_id
    }

    fn surface_id(&self) -> &str {
        &self.surface_id
    }

    fn initial_visibility(&self) -> Option<bool> {
        Some(true)
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
        true
    }

    fn wants_immediate_rerender(&self) -> bool {
        false
    }

    fn render(
        &mut self,
        surface: &mut dyn mesh_core_wayland::ShellSurface,
    ) -> Result<(), super::types::ComponentError> {
        surface.set_size(120, 36);
        Ok(())
    }

    fn paint(
        &mut self,
        _theme: &mesh_core_theme::Theme,
        _width: u32,
        _height: u32,
        buffer: &mut mesh_core_render::PixelBuffer,
        _scale: f32,
    ) -> Result<(), super::types::ComponentError> {
        buffer.clear(mesh_core_elements::style::Color {
            r: 8,
            g: 8,
            b: 8,
            a: 255,
        });
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn child_surface_requests(&self) -> Vec<super::types::ChildSurfaceRequest> {
        let state = self.state.lock().unwrap();
        if !state.open {
            return Vec::new();
        }
        vec![super::types::ChildSurfaceRequest {
            node_key: state.node_key.clone(),
            kind: super::types::ChildSurfaceKind::Popover,
            anchor_rect: state.anchor_rect,
            content_size: state.content_size,
            content_padding: (0, 0, 0, 0),
            placement: mesh_core_elements::PopoverPlacement::default(),
        }]
    }

    fn paint_child_surface(
        &self,
        node_key: &str,
        buffer: &mut mesh_core_render::PixelBuffer,
        _scale: f32,
        _content_offset: (u32, u32),
        exiting: bool,
    ) -> Result<bool, super::types::ComponentError> {
        let mut state = self.state.lock().unwrap();
        state.painted_nodes.push(node_key.to_string());
        state.exiting_paints.push(exiting);
        buffer.clear(mesh_core_elements::style::Color {
            r: 24,
            g: 48,
            b: 96,
            a: 255,
        });
        Ok(true)
    }

    fn child_hide_transition_ms(&self, _node_key: &str) -> u64 {
        self.state.lock().unwrap().hide_transition_ms
    }

    fn handle_child_surface_input(
        &mut self,
        node_key: &str,
        _theme: &mesh_core_theme::Theme,
        _width: u32,
        _height: u32,
        input: ComponentInput,
    ) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
        self.state
            .lock()
            .unwrap()
            .child_inputs
            .push((node_key.to_string(), input));
        Ok(Vec::new())
    }

    fn set_profiling_enabled(&mut self, enabled: bool) {
        self.state.lock().unwrap().profiling_enabled.push(enabled);
    }
}

struct RecordingClipboard {
    writes: Arc<Mutex<Vec<String>>>,
}

#[derive(Debug, Default)]
struct TransitionRecordingState {
    exiting: Vec<bool>,
}

struct TransitionRecordingComponent {
    surface_id: String,
    hide_transition_ms: u64,
    state: Arc<Mutex<TransitionRecordingState>>,
}

impl TransitionRecordingComponent {
    fn new(
        surface_id: &str,
        hide_transition_ms: u64,
        state: Arc<Mutex<TransitionRecordingState>>,
    ) -> Self {
        Self {
            surface_id: surface_id.to_string(),
            hide_transition_ms,
            state,
        }
    }
}

impl super::types::ShellComponent for TransitionRecordingComponent {
    fn id(&self) -> &str {
        &self.surface_id
    }

    fn surface_id(&self) -> &str {
        &self.surface_id
    }

    fn initial_visibility(&self) -> Option<bool> {
        Some(true)
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
        _scale: f32,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn hide_transition_ms(&self) -> u64 {
        self.hide_transition_ms
    }

    fn set_surface_exiting(&mut self, exiting: bool) {
        self.state.lock().unwrap().exiting.push(exiting);
    }
}

#[derive(Default)]
struct InputSizeRecordingState {
    sizes: Vec<(u32, u32)>,
}

struct InputSizeRecordingComponent {
    state: Arc<Mutex<InputSizeRecordingState>>,
    content_size: (u32, u32),
}

impl InputSizeRecordingComponent {
    fn new(state: Arc<Mutex<InputSizeRecordingState>>, content_size: (u32, u32)) -> Self {
        Self {
            state,
            content_size,
        }
    }
}

impl super::types::ShellComponent for InputSizeRecordingComponent {
    fn id(&self) -> &str {
        "@test/input-size"
    }

    fn surface_id(&self) -> &str {
        "@test/input-size"
    }

    fn initial_visibility(&self) -> Option<bool> {
        Some(true)
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
        _scale: f32,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn handle_input(
        &mut self,
        _theme: &mesh_core_theme::Theme,
        width: u32,
        height: u32,
        _input: ComponentInput,
    ) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
        self.state.lock().unwrap().sizes.push((width, height));
        Ok(Vec::new())
    }

    fn content_input_size(&self) -> Option<(u32, u32)> {
        Some(self.content_size)
    }
}

struct PopupGeometryRecordingComponent {
    surface_id: String,
    declared_size: (u32, u32),
    stale_surface_size: (u32, u32),
}

impl PopupGeometryRecordingComponent {
    fn new(surface_id: &str, declared_size: (u32, u32), stale_surface_size: (u32, u32)) -> Self {
        Self {
            surface_id: surface_id.to_string(),
            declared_size,
            stale_surface_size,
        }
    }
}

impl super::types::ShellComponent for PopupGeometryRecordingComponent {
    fn id(&self) -> &str {
        &self.surface_id
    }

    fn surface_id(&self) -> &str {
        &self.surface_id
    }

    fn initial_visibility(&self) -> Option<bool> {
        Some(false)
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
        true
    }

    fn render(
        &mut self,
        surface: &mut dyn mesh_core_wayland::ShellSurface,
    ) -> Result<(), super::types::ComponentError> {
        surface.set_size(self.stale_surface_size.0, self.stale_surface_size.1);
        Ok(())
    }

    fn paint(
        &mut self,
        _theme: &mesh_core_theme::Theme,
        _width: u32,
        _height: u32,
        _buffer: &mut mesh_core_render::PixelBuffer,
        _scale: f32,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn declared_or_measured_size(&self) -> (u32, u32) {
        self.declared_size
    }
}

struct MeasuredLayerGeometryComponent {
    surface_id: String,
    declared_size: (u32, u32),
    current_size: (u32, u32),
}

impl MeasuredLayerGeometryComponent {
    fn new(surface_id: &str, declared_size: (u32, u32), initial_size: (u32, u32)) -> Self {
        Self {
            surface_id: surface_id.to_string(),
            declared_size,
            current_size: initial_size,
        }
    }
}

impl super::types::ShellComponent for MeasuredLayerGeometryComponent {
    fn id(&self) -> &str {
        &self.surface_id
    }

    fn surface_id(&self) -> &str {
        &self.surface_id
    }

    fn initial_visibility(&self) -> Option<bool> {
        Some(false)
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
        true
    }

    fn render(
        &mut self,
        surface: &mut dyn mesh_core_wayland::ShellSurface,
    ) -> Result<(), super::types::ComponentError> {
        surface.set_size(self.current_size.0, self.current_size.1);
        Ok(())
    }

    fn paint(
        &mut self,
        _theme: &mesh_core_theme::Theme,
        _width: u32,
        _height: u32,
        _buffer: &mut mesh_core_render::PixelBuffer,
        _scale: f32,
    ) -> Result<(), super::types::ComponentError> {
        self.current_size = self.declared_size;
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn declared_or_measured_size(&self) -> (u32, u32) {
        self.current_size
    }
}

impl ClipboardWriter for RecordingClipboard {
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.writes.lock().unwrap().push(text.to_string());
        Ok(())
    }
}

struct DeadlineTickComponent {
    surface_id: String,
    deadline: Option<Instant>,
}

impl DeadlineTickComponent {
    fn new(surface_id: &str, deadline: Option<Instant>) -> Self {
        Self {
            surface_id: surface_id.to_string(),
            deadline,
        }
    }
}

impl super::types::ShellComponent for DeadlineTickComponent {
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

    fn next_tick_deadline(&self) -> Option<Instant> {
        self.deadline
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
        _scale: f32,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }
}

#[derive(Default)]
struct DirtyHiddenState {
    render_calls: usize,
}

struct DirtyHiddenComponent {
    surface_id: String,
    deadline: Option<Instant>,
    state: Arc<Mutex<DirtyHiddenState>>,
}

impl DirtyHiddenComponent {
    fn new(
        surface_id: &str,
        deadline: Option<Instant>,
        state: Arc<Mutex<DirtyHiddenState>>,
    ) -> Self {
        Self {
            surface_id: surface_id.to_string(),
            deadline,
            state,
        }
    }
}

impl super::types::ShellComponent for DirtyHiddenComponent {
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

    fn wants_tick(&self) -> bool {
        true
    }

    fn next_tick_deadline(&self) -> Option<Instant> {
        self.deadline
    }

    fn tick(&mut self) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
        Ok(Vec::new())
    }

    fn wants_render(&self) -> bool {
        true
    }

    fn render(
        &mut self,
        _surface: &mut dyn mesh_core_wayland::ShellSurface,
    ) -> Result<(), super::types::ComponentError> {
        self.state.lock().unwrap().render_calls += 1;
        Ok(())
    }

    fn paint(
        &mut self,
        _theme: &mesh_core_theme::Theme,
        _width: u32,
        _height: u32,
        _buffer: &mut mesh_core_render::PixelBuffer,
        _scale: f32,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }
}

fn park_reload_deadlines(shell: &mut Shell) {
    let later = Instant::now() + Duration::from_secs(60);
    shell.next_theme_reload_check = later;
    shell.next_shell_settings_reload_check = later;
    shell.next_frontend_reload_check = later;
    shell.next_module_settings_reload_check = later;
}

#[test]
fn scheduler_uses_component_tick_deadline() {
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    let deadline = Instant::now() + Duration::from_millis(120);
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            DeadlineTickComponent::new("@test/deadline", Some(deadline)),
        )));

    let sleep = shell.next_runtime_sleep(false);

    assert!(sleep <= Duration::from_millis(120), "{sleep:?}");
    assert!(sleep >= Duration::from_millis(80), "{sleep:?}");
}

#[test]
fn scheduler_wakes_immediately_for_due_component_tick_deadline() {
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            DeadlineTickComponent::new(
                "@test/deadline",
                Some(Instant::now() - Duration::from_millis(1)),
            ),
        )));

    assert_eq!(shell.next_runtime_sleep(false), Duration::ZERO);
}

#[test]
fn scheduler_wakes_for_visible_dirty_component_even_without_previous_present() {
    let state = Arc::new(Mutex::new(DirtyHiddenState::default()));
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    shell.register_component(Box::new(DirtyHiddenComponent::new(
        "@test/dirty",
        None,
        Arc::clone(&state),
    )));
    {
        shell
            .core
            .surfaces
            .get_mut("@test/dirty")
            .expect("registered core surface")
            .visible = true;
        let surface = shell
            .surfaces
            .get_mut("@test/dirty")
            .expect("registered surface target");
        surface.visible = true;
        surface.width = 120;
        surface.height = 36;
    }
    shell.presented_last_frame = false;

    assert_eq!(shell.next_runtime_sleep(false), Duration::ZERO);
}

#[test]
fn scheduler_ignores_hidden_component_deadlines_and_render_dirtiness() {
    let state = Arc::new(Mutex::new(DirtyHiddenState::default()));
    let mut shell = Shell::new();
    park_reload_deadlines(&mut shell);
    shell.register_component(Box::new(DirtyHiddenComponent::new(
        "@test/hidden",
        Some(Instant::now()),
        Arc::clone(&state),
    )));
    shell
        .core
        .surfaces
        .get_mut("@test/hidden")
        .expect("hidden surface state")
        .visible = false;
    shell.presented_last_frame = true;

    let sleep = shell.next_runtime_sleep(false);

    assert!(
        sleep >= Duration::from_secs(30),
        "hidden dirty component should not force an immediate wake: {sleep:?}"
    );
}

#[test]
fn render_skips_already_hidden_dirty_surface() {
    let state = Arc::new(Mutex::new(DirtyHiddenState::default()));
    let mut shell = Shell::new();
    shell.register_component(Box::new(DirtyHiddenComponent::new(
        "@test/hidden",
        None,
        Arc::clone(&state),
    )));
    shell
        .core
        .surfaces
        .get_mut("@test/hidden")
        .expect("hidden surface state")
        .visible = false;

    shell.render_components().unwrap();

    assert_eq!(state.lock().unwrap().render_calls, 0);
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
fn service_delivery_index_routes_updates_without_scanning_unrelated_components() {
    let audio_summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
        update_services: vec!["audio".to_string()],
        interface_events: Vec::new(),
    })));
    let power_summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
        update_services: vec!["power".to_string()],
        interface_events: Vec::new(),
    })));
    let fallback_summary = Arc::new(Mutex::new(None));
    let audio_state = Arc::new(Mutex::new(IndexedRecordingState::default()));
    let power_state = Arc::new(Mutex::new(IndexedRecordingState::default()));
    let fallback_state = Arc::new(Mutex::new(IndexedRecordingState::default()));

    let mut shell = Shell::new();
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/audio-observer",
        audio_summary,
        Arc::clone(&audio_state),
    )));
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/power-observer",
        power_summary,
        Arc::clone(&power_state),
    )));
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/fallback-observer",
        fallback_summary,
        Arc::clone(&fallback_state),
    )));

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 55.0 }),
        ))
        .unwrap();

    assert_eq!(audio_state.lock().unwrap().observed, 1);
    assert_eq!(audio_state.lock().unwrap().handled.len(), 1);
    assert_eq!(
        power_state.lock().unwrap().observed,
        0,
        "summarized components for other services should not be scanned"
    );
    assert!(power_state.lock().unwrap().handled.is_empty());
    assert_eq!(
        fallback_state.lock().unwrap().observed,
        1,
        "unknown-summary components keep the legacy observation gate"
    );
    assert_eq!(fallback_state.lock().unwrap().handled.len(), 1);
}

#[test]
fn service_delivery_index_routes_interface_events_by_name() {
    let summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
        update_services: Vec::new(),
        interface_events: vec![ServiceInterfaceEventSubscription {
            service: "audio".to_string(),
            event: "volume_changed".to_string(),
        }],
    })));
    let state = Arc::new(Mutex::new(IndexedRecordingState::default()));
    let mut shell = Shell::new();
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/interface-observer",
        summary,
        Arc::clone(&state),
    )));

    shell
        .deliver_service_event(&ServiceEvent::InterfaceEvent {
            service: "mesh.audio".to_string(),
            source_module: "@mesh/pipewire-audio".to_string(),
            name: "device_changed".to_string(),
            payload: serde_json::json!({}),
        })
        .unwrap();
    shell
        .deliver_service_event(&ServiceEvent::InterfaceEvent {
            service: "mesh.audio".to_string(),
            source_module: "@mesh/pipewire-audio".to_string(),
            name: "volume_changed".to_string(),
            payload: serde_json::json!({ "percent": 70.0 }),
        })
        .unwrap();

    let state = state.lock().unwrap();
    assert_eq!(state.observed, 1);
    assert_eq!(state.handled.len(), 1);
    assert!(matches!(
        &state.handled[0],
        ServiceEvent::InterfaceEvent { name, .. } if name == "volume_changed"
    ));
}

#[test]
fn service_delivery_index_rebuilds_when_marked_dirty() {
    let summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
        update_services: vec!["audio".to_string()],
        interface_events: Vec::new(),
    })));
    let state = Arc::new(Mutex::new(IndexedRecordingState::default()));
    let mut shell = Shell::new();
    shell.register_component(Box::new(IndexedRecordingComponent::new(
        "@test/dynamic-observer",
        Arc::clone(&summary),
        Arc::clone(&state),
    )));

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 55.0 }),
        ))
        .unwrap();
    *summary.lock().unwrap() = Some(ServiceObservationSummary {
        update_services: vec!["power".to_string()],
        interface_events: Vec::new(),
    });
    shell.service_delivery_index.mark_dirty();
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 60.0 }),
        ))
        .unwrap();
    shell
        .broadcast_service_event(service_update(
            "mesh.power",
            "@mesh/upower-power",
            serde_json::json!({ "available": true, "percentage": 88.0 }),
        ))
        .unwrap();

    let state = state.lock().unwrap();
    assert_eq!(state.observed, 2);
    assert_eq!(state.handled.len(), 2);
    assert!(matches!(
        &state.handled[1],
        ServiceEvent::Updated { service, .. } if service == "mesh.power"
    ));
}

#[test]
#[ignore = "release-only service delivery index microbenchmark"]
fn service_delivery_index_beats_full_component_scan_benchmark() {
    const COMPONENTS: usize = 256;
    const ITERATIONS: usize = 20_000;

    let mut shell = Shell::new();
    let mut states = Vec::new();
    for index in 0..COMPONENTS {
        let service = if index == 17 { "audio" } else { "power" };
        let summary = Arc::new(Mutex::new(Some(ServiceObservationSummary {
            update_services: vec![service.to_string()],
            interface_events: Vec::new(),
        })));
        let state = Arc::new(Mutex::new(IndexedRecordingState::default()));
        states.push(Arc::clone(&state));
        shell.register_component(Box::new(IndexedRecordingComponent::new(
            &format!("@test/indexed-observer-{index:03}"),
            summary,
            state,
        )));
    }
    let event = service_update(
        "mesh.audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 55.0 }),
    );

    let old_started = Instant::now();
    let mut old_hits = 0usize;
    for _ in 0..ITERATIONS {
        for runtime in &mut shell.components {
            if runtime
                .component
                .observes_service_event(std::hint::black_box(&event))
            {
                let _ = runtime.component.handle_service_event(&event).unwrap();
                old_hits += 1;
            }
        }
    }
    let old_elapsed = old_started.elapsed();

    shell.service_delivery_index.mark_dirty();
    shell.rebuild_service_delivery_index_if_needed();
    let new_started = Instant::now();
    for _ in 0..ITERATIONS {
        let _ = shell
            .deliver_service_event(std::hint::black_box(&event))
            .unwrap();
    }
    let new_elapsed = new_started.elapsed();
    let delivered_total: usize = states
        .iter()
        .map(|state| state.lock().unwrap().handled.len())
        .sum();

    eprintln!(
        "service delivery scan: old={old_elapsed:?} indexed={new_elapsed:?} old_hits={old_hits} delivered_total={delivered_total}"
    );
    assert_eq!(old_hits, ITERATIONS);
    assert_eq!(delivered_total, ITERATIONS * 2);
    assert!(
        new_elapsed < old_elapsed,
        "indexed delivery should beat full component scan"
    );
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
    assert!(!shell.debug.show_layout_bounds);

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
    assert!(
        !shell.debug.show_layout_bounds,
        "profiling changes must remain independent from layout-bounds debugging"
    );
}

#[test]
fn debug_layout_bounds_toggle_remains_independent_from_overlay_visibility() {
    let mut shell = Shell::new();

    shell
        .apply_request(CoreRequest::ToggleDebugLayoutBounds)
        .unwrap();
    assert!(shell.debug.show_layout_bounds);
    assert!(!shell.debug.enabled);
    assert!(!shell.debug.profiling_enabled);

    shell
        .apply_request(CoreRequest::ToggleDebugOverlay)
        .unwrap();
    assert!(shell.debug.show_layout_bounds);

    shell
        .apply_request(CoreRequest::ToggleDebugLayoutBounds)
        .unwrap();
    assert!(!shell.debug.show_layout_bounds);
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
fn debug_snapshot_exposes_module_object_instances() {
    let mut shell = Shell::new();
    shell.interfaces.register(InterfaceProvider {
        interface: "mesh.example".to_string(),
        version: Some("1.0".to_string()),
        base_module: Some("@mesh/example-interface".to_string()),
        provider_module: "@mesh/example-backend".to_string(),
        backend_name: "Example".to_string(),
        priority: 10,
    });
    shell.record_backend_runtime_status(
        "mesh.example".to_string(),
        "@mesh/example-backend".to_string(),
        BackendRuntimeStatus::Running,
        "backend runtime started".to_string(),
    );

    let snapshot = shell.build_debug_snapshot();

    assert!(snapshot.module_instances.iter().any(|entry| {
        entry.object_kind == "backend"
            && entry.instance_id == "mesh.example:@mesh/example-backend"
            && entry.module_id == "@mesh/example-backend"
            && entry.interface.as_deref() == Some("mesh.example")
            && entry.version.as_deref() == Some("1.0")
            && entry.lifecycle == "running"
    }));
}

#[test]
fn debug_snapshot_exposes_installed_module_graph_contracts() {
    let mut shell = Shell::new();
    shell.discover_modules();

    let snapshot = shell.build_debug_snapshot();
    let navigation = snapshot
        .module_graph
        .iter()
        .find(|entry| entry.module_id == "@mesh/navigation-bar")
        .expect("navigation module graph entry");

    assert!(navigation.uses_interfaces.contains(&"mesh.audio".into()));
    assert!(navigation.uses_interfaces.contains(&"mesh.power".into()));
    assert!(
        navigation
            .uses_optional_interfaces
            .contains(&"mesh.brightness".into())
    );
    assert!(
        navigation
            .uses_icon_packs
            .contains(&"@mesh/icons-default".into())
    );
    assert!(
        navigation
            .provides_settings
            .contains(&"@mesh/navigation-bar".into())
    );
    assert!(
        navigation
            .provides_i18n
            .iter()
            .any(|entry| entry == "en:config/i18n/en.json")
    );
    assert!(
        navigation
            .required_icons
            .contains(&"battery-caution".into())
    );
    assert!(navigation.keybind_actions.contains(&"mute".into()));
    assert!(
        navigation
            .active_providers
            .iter()
            .any(|entry| entry.starts_with("mesh.power="))
    );

    let pipewire = snapshot
        .module_graph
        .iter()
        .find(|entry| entry.module_id == "@mesh/pipewire-audio")
        .expect("pipewire module graph entry");
    assert!(pipewire.required_binaries.contains(&"wpctl".into()));
    assert!(pipewire.optional_binaries.contains(&"aplay".into()));
    assert!(
        pipewire
            .native_binaries
            .iter()
            .any(|binary| { binary.name == "wpctl" && !binary.optional })
    );

    let debug_payload = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("debug snapshot should backfill mesh.debug latest state");
    assert!(
        debug_payload.state["module_graph"]
            .as_array()
            .is_some_and(|entries| entries.iter().any(|entry| {
                entry["module_id"] == serde_json::json!("@mesh/navigation-bar")
                    && entry["uses"]["interfaces"]
                        .as_array()
                        .is_some_and(|interfaces| {
                            interfaces.contains(&serde_json::json!("mesh.audio"))
                        })
                    && entry["uses"]["keybinds"]
                        .as_array()
                        .is_some_and(|actions| actions.contains(&serde_json::json!("mute")))
            }))
    );
    let pipewire_json = debug_payload.state["module_graph"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry["module_id"] == serde_json::json!("@mesh/pipewire-audio"))
        })
        .expect("serialized pipewire graph entry");
    assert!(
        pipewire_json["uses"]["native_binaries"]
            .as_array()
            .is_some_and(|binaries| binaries.iter().any(|binary| {
                binary["name"] == serde_json::json!("wpctl")
                    && binary["optional"] == serde_json::json!(false)
                    && binary["available"].is_boolean()
            }))
    );
}

#[test]
fn debug_snapshot_resolves_module_graph_layout_label_with_active_locale() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/panel": { "kind": "frontend", "path": "@mesh/panel", "enabled": true }
              },
              "layout": { "entrypoint": "@mesh/panel:main" }
            }"#,
        vec![
            r#"{
                  "name": "@mesh/panel",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "frontend",
                    "entry": "src/main.mesh",
                    "provides": {
                      "layout": [
                        {
                          "id": "main",
                          "entrypoint": "src/main.mesh",
                          "label": { "t": "layout.main.label", "fallback": "Main panel" }
                        }
                      ]
                    },
                    "surfaceLayout": { "size_policy": "fixed" },
                    "accessibility": { "role": "toolbar" }
                  }
                }"#,
        ],
    );
    let mut shell = Shell::new();
    shell.installed_module_graph = Some(graph);
    shell
        .locale
        .load_translations(mesh_core_locale::TranslationSet {
            locale: "sk".into(),
            messages: HashMap::from([("layout.main.label".into(), "Hlavny panel".into())]),
        });
    shell.locale.set_locale("sk");

    let snapshot = shell.build_debug_snapshot();
    let panel = snapshot
        .module_graph
        .iter()
        .find(|entry| entry.module_id == "@mesh/panel")
        .expect("panel module graph entry");

    assert_eq!(panel.surface_layout_label.as_deref(), Some("Hlavny panel"));
    assert_eq!(
        panel.surface_layout_label_key.as_deref(),
        Some("layout.main.label")
    );
    assert_eq!(
        panel.surface_layout_label_fallback.as_deref(),
        Some("Main panel")
    );

    let debug_payload = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("debug snapshot should backfill mesh.debug latest state");
    let panel_json = debug_payload.state["module_graph"]
        .as_array()
        .and_then(|entries| {
            entries
                .iter()
                .find(|entry| entry["module_id"] == serde_json::json!("@mesh/panel"))
        })
        .expect("panel module graph JSON entry");
    assert_eq!(
        panel_json["surface"]["layout_label"],
        serde_json::json!("Hlavny panel")
    );
    assert_eq!(
        panel_json["surface"]["layout_label_key"],
        serde_json::json!("layout.main.label")
    );
    assert_eq!(
        panel_json["surface"]["layout_label_fallback"],
        serde_json::json!("Main panel")
    );
}

#[test]
fn benchmark_snapshot_exposes_canonical_and_interaction_scenarios() {
    let mut shell = Shell::new();
    let snapshot = shell.build_debug_snapshot();

    assert_eq!(snapshot.benchmarks.scenarios.len(), 12);
    assert_eq!(
        snapshot
            .benchmarks
            .scenarios
            .iter()
            .map(|scenario| scenario.id.id())
            .collect::<Vec<_>>(),
        vec![
            "idle",
            "hover",
            "surface_open_close",
            "pointer_update",
            "text_update",
            "scroll",
            "icon_grid",
            "animation",
            "theme_reload",
            "resize",
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
fn benchmark_canonical_profiles_bind_expected_stages_and_surfaces() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    shell.record_shell_profiling_stage(
        ProfilingStage::SchedulerIdle,
        std::time::Duration::from_micros(500),
        Some("timeout"),
    );
    for (surface, stage, micros) in [
        ("@mesh/settings", ProfilingStage::InputHandling, 11),
        ("@mesh/settings", ProfilingStage::TreeBuild, 22),
        ("@mesh/settings", ProfilingStage::Layout, 33),
        ("@mesh/debug-inspector", ProfilingStage::IconImageRaster, 44),
        ("@mesh/debug-inspector", ProfilingStage::Paint, 55),
        ("@mesh/navigation-bar", ProfilingStage::InputHandling, 60),
        ("@mesh/navigation-bar", ProfilingStage::StyleRestyle, 66),
        ("@mesh/navigation-bar", ProfilingStage::TreeBuild, 77),
        ("@mesh/navigation-bar", ProfilingStage::Layout, 88),
        ("@mesh/navigation-bar", ProfilingStage::Paint, 99),
    ] {
        shell.record_surface_profiling_stage(
            surface,
            Some(surface),
            stage,
            std::time::Duration::from_micros(micros),
            Some("canonical_profile"),
        );
    }

    let snapshot = shell.build_debug_snapshot();
    let scenario = |id: &str| {
        snapshot
            .benchmarks
            .scenarios
            .iter()
            .find(|scenario| scenario.id.id() == id)
            .expect("canonical scenario")
    };
    for id in [
        "idle",
        "pointer_update",
        "text_update",
        "scroll",
        "icon_grid",
        "animation",
        "theme_reload",
        "resize",
    ] {
        assert_eq!(
            scenario(id).status,
            mesh_core_debug::BenchmarkScenarioStatus::Complete,
            "{id} should resolve from its canonical stage samples"
        );
    }
    assert!(
        scenario("idle")
            .primary_metric
            .starts_with("scheduler_idle:")
    );
    assert!(
        scenario("text_update")
            .secondary_metric
            .starts_with("tree_build:")
    );
    assert!(
        scenario("icon_grid")
            .primary_metric
            .starts_with("icon_image_raster:")
    );
    assert!(scenario("resize").primary_metric.starts_with("layout:"));
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
    assert_eq!(scenarios.len(), 12);
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
    assert_eq!(scenarios.len(), 12);
    assert_eq!(
        scenarios
            .iter()
            .map(|scenario| scenario["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "idle",
            "hover",
            "surface_open_close",
            "pointer_update",
            "text_update",
            "scroll",
            "icon_grid",
            "animation",
            "theme_reload",
            "resize",
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
            "shell scheduler",
            "@mesh/navigation-bar",
            "@mesh/audio-popover",
            "@mesh/navigation-bar audio controls",
            "@mesh/settings text controls",
            "@mesh/settings",
            "@mesh/debug-inspector",
            "@mesh/navigation-bar",
            "active theme + @mesh/navigation-bar",
            "@mesh/navigation-bar",
            "@mesh/navigation-bar focus chain",
            "mesh.audio -> @mesh/pipewire-audio",
        ]
    );
    let backend_update = &scenarios[11];
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
            "idle",
            "hover",
            "surface_open_close",
            "pointer_update",
            "text_update",
            "scroll",
            "icon_grid",
            "animation",
            "theme_reload",
            "resize",
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
    assert_eq!(navigation_rows.len(), 6);
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
    assert_eq!(
        latest.state["layout_bounds_enabled"],
        serde_json::json!(false)
    );
    assert_eq!(latest.state["profiling_enabled"], serde_json::json!(false));
    assert_eq!(latest.state["profiling_session_id"], serde_json::json!(0));
    assert_eq!(latest.state["active_view"], serde_json::json!("overview"));
    assert_eq!(
        latest.state["active_surfaces"],
        serde_json::json!(snapshot.active_surfaces)
    );
    assert!(latest.state["profiling"].is_null());
    assert!(latest.state["profiling_stream"].is_null());
}

#[test]
fn debug_snapshot_exposes_deduplicated_ordered_profiling_stream() {
    let mut shell = Shell::new();
    shell.debug.profiling_enabled = true;
    shell.debug.profiling_session_id = 1;
    shell.profiling.record_shell_stage(
        ProfilingStage::InputHandling,
        std::time::Duration::from_micros(7),
        Some("input"),
    );
    shell.profiling.record_surface_stage(
        "@test/surface",
        Some("@test/module"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(11),
        Some("paint"),
    );

    shell.build_debug_snapshot();
    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("debug snapshot should publish state");
    let stream = latest.state["profiling_stream"]
        .as_array()
        .expect("profiling stream should be an array");

    assert_eq!(
        stream.len(),
        2,
        "surface samples must not be duplicated from the shell roll-up"
    );
    assert!(
        stream.windows(2).all(|pair| {
            pair[0]["order"].as_u64().unwrap() < pair[1]["order"].as_u64().unwrap()
        })
    );
    assert!(
        stream
            .iter()
            .all(|sample| sample["timestamp_micros"].is_u64())
    );
    assert_eq!(stream[1]["surface_id"], serde_json::json!("@test/surface"));
    let trace = latest.state["chrome_trace"]["traceEvents"]
        .as_array()
        .expect("chrome trace should contain events");
    assert_eq!(trace.len(), 2);
    assert_eq!(trace[1]["ph"], serde_json::json!("X"));
    assert_eq!(trace[1]["tid"], serde_json::json!("@test/surface"));
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
fn shell_registers_interface_contracts_and_providers_from_installed_graph() {
    let interface_dir = tempfile::tempdir().unwrap();
    let root = RootModuleGraphManifest::from_json_str(
        r#"{
              "name": "@mesh/test-config",
              "version": "0.1.0",
              "mesh": {
                "schemaVersion": 1,
                "modulesDir": "modules",
                "modules": {
                  "@mesh/example-interface": { "kind": "interface", "path": "@mesh/example-interface", "enabled": true },
                  "@mesh/example-backend": { "kind": "backend", "path": "@mesh/example-backend", "enabled": true }
                },
                "providers": { "mesh.example": "@mesh/example-backend" }
              }
            }"#,
    )
    .unwrap();
    let interface = LoadedModuleManifest {
        manifest: ModuleManifest::from_json_str(
            r#"{
                  "name": "@mesh/example-interface",
                  "version": "1.0.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "interface",
                    "interface": {
                      "name": "mesh.example",
                      "version": "1.0",
                      "domain": "example",
                      "relationship": "base",
                      "contract": {
                        "methods": [{ "name": "read", "returns": "boolean" }],
                        "capabilities": { "required": ["service.example.read"] }
                      }
                    }
                  }
                }"#,
        )
        .unwrap(),
        path: interface_dir.path().join("module.json"),
        source: ModuleManifestSource::CanonicalModuleJson,
        diagnostics: Vec::new(),
    };
    let backend = LoadedModuleManifest {
        manifest: ModuleManifest::from_json_str(
            r#"{
                  "name": "@mesh/example-backend",
                  "version": "1.0.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [
                      {
                        "interface": "mesh.example",
                        "version": "1.0",
                        "baseModule": "@mesh/example-interface",
                        "provider": "example",
                        "priority": 10
                      }
                    ]
                  }
                }"#,
        )
        .unwrap(),
        path: PathBuf::from("<test>/backend/module.json"),
        source: ModuleManifestSource::CanonicalModuleJson,
        diagnostics: Vec::new(),
    };
    let graph = InstalledModuleGraph::from_parts(root, vec![interface, backend]).unwrap();
    let mut shell = Shell::new();

    shell.register_interfaces_from_graph(&graph);

    let contracts = shell.interfaces.contracts_for("mesh.example");
    assert_eq!(contracts.len(), 1);
    assert_eq!(
        contracts[0].capabilities.required,
        vec!["service.example.read".to_string()]
    );
    let providers = shell.interfaces.providers_for("mesh.example");
    assert!(providers.iter().any(|provider| {
        provider.provider_module == "@mesh/example-backend"
            && provider.backend_name == "example"
            && provider.base_module.as_deref() == Some("@mesh/example-interface")
    }));
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
        .expect("debug snapshot should be delivered as a service update")
    else {
        panic!("expected debug snapshot service update");
    };
    assert_eq!(service, mesh_core_debug::DEBUG_INTERFACE);
    assert_eq!(source_module, mesh_core_debug::DEBUG_SOURCE_MODULE_ID);
    assert_eq!(payload["overlay_enabled"], serde_json::json!(true));
    assert_eq!(payload["layout_bounds_enabled"], serde_json::json!(false));
    assert!(payload["benchmarks"]["scenarios"].is_array());
}

#[test]
fn debug_snapshot_payload_includes_resolved_keybind_metadata() {
    let mut shell = Shell::new();
    shell.debug.enabled = true;
    let events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::with_keybinds(
        events.clone(),
        vec![mesh_core_debug::DebugKeybindEntry {
            surface_id: "@mesh/navigation-bar".into(),
            module_id: "@mesh/navigation-bar".into(),
            action_id: "mute".into(),
            label: Some("Mute".into()),
            description: Some("Toggle audio output".into()),
            category: Some("Audio".into()),
            label_key: Some("keybind.mute.label".into()),
            description_key: Some("keybind.mute.description".into()),
            category_key: Some("keybind.category.audio".into()),
            key: "m".into(),
            modifiers: vec!["ctrl".into()],
            trigger_kind: "shortcut".into(),
            source: "module_default".into(),
            accessibility_shortcut: "Control+m".into(),
        }],
    )));

    shell.publish_debug_snapshot().unwrap();

    let events = events.lock().unwrap();
    let ServiceEvent::Updated { payload, .. } = events
        .last()
        .expect("debug snapshot should be delivered as a service update")
    else {
        panic!("expected debug snapshot service update");
    };
    assert_eq!(
        payload["keybinds"][0],
        serde_json::json!({
            "surface_id": "@mesh/navigation-bar",
            "module_id": "@mesh/navigation-bar",
            "action_id": "mute",
            "label": "Mute",
            "description": "Toggle audio output",
            "category": "Audio",
            "label_key": "keybind.mute.label",
            "description_key": "keybind.mute.description",
            "category_key": "keybind.category.audio",
            "key": "m",
            "modifiers": ["ctrl"],
            "trigger_kind": "shortcut",
            "source": "module_default",
            "accessibility_shortcut": "Control+m",
        })
    );
    assert!(
        payload["health"].is_array(),
        "debug payload should keep diagnostics health visible"
    );
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
    assert_eq!(
        latest.state["layout_bounds_enabled"],
        serde_json::json!(false)
    );
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
            narrow_path: false,
            affected_node_count: 0,
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
                omitted_subtrees: 2,
                omitted_nodes: 5,
                omitted_commands: 10,
                preclipped_descendants: 4,
                repaint_policy: RepaintPolicySnapshot::BoundingRect,
                filtered_span_count: 3,
                filtered_command_count: 4,
                filtered_commands_skipped: 1,
                batch_count: 2,
                batched_primitives: 5,
                barrier_count: 3,
                barriers: DisplayBatchBarrierSnapshot {
                    text: 1,
                    material_change: 2,
                    ..Default::default()
                },
                raster_cache_hits: 8,
                raster_cache_misses: 2,
                raster_cache_bypasses: 1,
                raster_cache_opaque_hits: 5,
                raster_cache_translucent_hits: 3,
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
    assert_eq!(invalidation.paint.omitted_subtrees, 2);
    assert_eq!(invalidation.paint.omitted_nodes, 5);
    assert_eq!(invalidation.paint.omitted_commands, 10);
    assert_eq!(invalidation.paint.preclipped_descendants, 4);
    assert_eq!(
        invalidation.paint.repaint_policy,
        RepaintPolicySnapshot::BoundingRect
    );
    assert_eq!(invalidation.paint.filtered_span_count, 3);
    assert_eq!(invalidation.paint.filtered_command_count, 4);
    assert_eq!(invalidation.paint.filtered_commands_skipped, 1);
    assert_eq!(invalidation.paint.filtered_fallback_count, 0);
    assert_eq!(invalidation.paint.batch_count, 2);
    assert_eq!(invalidation.paint.batched_primitives, 5);
    assert_eq!(invalidation.paint.barriers.text, 1);
    assert_eq!(invalidation.paint.barriers.material_change, 2);
    assert_eq!(invalidation.paint.raster_cache_hits, 8);
    assert_eq!(invalidation.paint.raster_cache_misses, 2);
    assert_eq!(invalidation.paint.raster_cache_bypasses, 1);
    assert_eq!(invalidation.paint.raster_cache_opaque_hits, 5);
    assert_eq!(invalidation.paint.raster_cache_translucent_hits, 3);
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
fn set_muted_command_broadcasts_optimistic_audio_state_until_backend_confirms() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    register_test_provider(&shell.interfaces, "mesh.audio", "@mesh/pipewire-audio");
    let (slot, mut rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::new(events.clone())));
    let mut capabilities = mesh_core_capability::CapabilitySet::new();
    capabilities.grant(mesh_core_capability::Capability::new(
        "service.audio.control",
    ));

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 42.0, "muted": true }),
        ))
        .unwrap();
    events.lock().unwrap().clear();

    let result = shell.dispatch_service_command(
        "mesh.audio",
        "set_muted",
        &serde_json::json!({ "device_id": "default", "muted": false }),
        "@mesh/audio-popover",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(result["optimistic"], serde_json::json!(true));
    assert_eq!(rx.try_recv().unwrap().command, "set_muted");
    assert_eq!(
        events.lock().unwrap().last().and_then(|event| match event {
            ServiceEvent::Updated { payload, .. } => payload.get("muted").cloned(),
            ServiceEvent::InterfaceEvent { .. } => None,
        }),
        Some(serde_json::json!(false)),
        "optimistic set_muted(false) should update frontend consumers immediately"
    );

    let delivered_events = events.lock().unwrap().len();
    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/stale-audio",
            serde_json::json!({ "available": true, "percent": 42.0, "muted": false }),
        ))
        .unwrap();
    assert_eq!(
        events.lock().unwrap().len(),
        delivered_events,
        "inactive providers must not deliver audio state while set_muted is pending"
    );
    assert_eq!(
        shell
            .pending_optimistic_state
            .get(&("mesh.audio".to_string(), "muted".to_string())),
        Some(&serde_json::json!(false)),
        "inactive provider updates must not clear pending mute state"
    );

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 42.0, "muted": true }),
        ))
        .unwrap();
    assert_eq!(
        events.lock().unwrap().last().and_then(|event| match event {
            ServiceEvent::Updated { payload, .. } => payload.get("muted").cloned(),
            ServiceEvent::InterfaceEvent { .. } => None,
        }),
        Some(serde_json::json!(false)),
        "stale backend muted=true must not flip UI while set_muted(false) is pending"
    );

    shell
        .broadcast_service_event(service_update(
            "mesh.audio",
            "@mesh/pipewire-audio",
            serde_json::json!({ "available": true, "percent": 42.0, "muted": false }),
        ))
        .unwrap();
    assert_eq!(
        shell
            .pending_optimistic_state
            .get(&("mesh.audio".to_string(), "muted".to_string())),
        None,
        "matching backend confirmation should clear pending mute state"
    );
}

#[test]
fn backend_supervision_quarantines_provider_after_exhausted_restart_cycles() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();

    // Each terminal failure of the current provider schedules a supervised
    // restart; after the restart budget is exhausted the provider is
    // quarantined for the session.
    for cycle in 0..3u32 {
        let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
        shell.replace_backend_runtime("mesh.audio".to_string(), slot);
        shell.handle_backend_lifecycle(
            "mesh.audio".to_string(),
            "@mesh/pipewire-audio".to_string(),
            "poll".to_string(),
            "failed".to_string(),
            format!("boom {cycle}"),
        );
        let state = shell.backend_supervision.get("mesh.audio").unwrap();
        assert_eq!(state.restart_count, cycle + 1);
        assert!(state.quarantined_providers.is_empty());
    }

    // Fourth consecutive failure exceeds the restart budget: quarantine.
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    shell.handle_backend_lifecycle(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        "poll".to_string(),
        "failed".to_string(),
        "boom final".to_string(),
    );
    let state = shell.backend_supervision.get("mesh.audio").unwrap();
    assert!(
        state.quarantined_providers.contains("@mesh/pipewire-audio"),
        "provider should be quarantined after exhausting restart cycles"
    );
    assert_eq!(
        state.restart_count, 0,
        "failover restarts with a fresh budget"
    );
    assert_eq!(
        shell
            .backend_runtime_status("mesh.audio", "@mesh/pipewire-audio")
            .map(|entry| entry.status),
        Some(BackendRuntimeStatus::Quarantined)
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
        ProfilingStage::RenderObjectSync,
        std::time::Duration::from_micros(16),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::RetainedDisplayListUpdate,
        std::time::Duration::from_micros(17),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::PaintTraversal,
        std::time::Duration::from_micros(18),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TextShaping,
        std::time::Duration::from_micros(19),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::IconImageRaster,
        std::time::Duration::from_micros(20),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(21),
        Some("rebuild"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::PresentCommit,
        std::time::Duration::from_micros(22),
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
        std::time::Duration::from_micros(23),
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
    assert!(stages.contains(&ProfilingStage::RenderObjectSync));
    assert!(stages.contains(&ProfilingStage::RetainedDisplayListUpdate));
    assert!(stages.contains(&ProfilingStage::PaintTraversal));
    assert!(stages.contains(&ProfilingStage::TextShaping));
    assert!(stages.contains(&ProfilingStage::IconImageRaster));
    assert!(stages.contains(&ProfilingStage::Paint));
    assert!(stages.contains(&ProfilingStage::PresentCommit));
    assert!(stages.contains(&ProfilingStage::RedrawCount));
    assert!(stages.contains(&ProfilingStage::TotalSurfaceRender));
}

#[test]
fn profiling_debug_payload_serializes_phase26_surface_attribution_labels() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::RenderObjectSync,
        std::time::Duration::from_micros(31),
        Some("hover"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::RetainedDisplayListUpdate,
        std::time::Duration::from_micros(32),
        Some("hover"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::PaintTraversal,
        std::time::Duration::from_micros(33),
        Some("hover"),
    );
    shell.record_surface_invalidation(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingInvalidationSnapshot {
            paint: RetainedPaintSnapshot {
                subtree_segments_reused: 7,
                subtree_segments_rebuilt: 2,
                subtree_commands_rebuilt: 5,
                full_fallback_count: 1,
                broad_dirty_fallback_count: 1,
                repaint_policy: RepaintPolicySnapshot::FullSurface,
                filtered_span_count: 4,
                filtered_command_count: 9,
                filtered_commands_skipped: 0,
                filtered_fallback_count: 1,
                raster_cache_hits: 6,
                raster_cache_misses: 2,
                raster_cache_bypasses: 1,
                raster_cache_opaque_hits: 4,
                raster_cache_translucent_hits: 2,
                ..Default::default()
            },
            text: TextCacheSnapshot {
                shaping_micros: 34,
                ..Default::default()
            },
            ..Default::default()
        },
    );

    shell.build_debug_snapshot();

    let latest = shell
        .latest_service_state
        .get(mesh_core_debug::DEBUG_INTERFACE)
        .expect("mesh.debug state should be published");
    let stages = latest.state["profiling"]["surfaces"][0]["stages"]
        .as_array()
        .expect("surface stages should serialize as an array");
    let labels: std::collections::HashSet<_> = stages
        .iter()
        .filter_map(|stage| stage["stage"].as_str())
        .collect();

    assert!(labels.contains("render_object_sync"));
    assert!(labels.contains("retained_display_list_update"));
    assert!(labels.contains("paint_traversal"));
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["text"]["shaping_micros"],
        serde_json::json!(34)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["omitted_subtrees"],
        serde_json::json!(0)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["subtree_segments_reused"],
        serde_json::json!(7)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["subtree_segments_rebuilt"],
        serde_json::json!(2)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["subtree_commands_rebuilt"],
        serde_json::json!(5)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["full_fallback_count"],
        serde_json::json!(1)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["broad_dirty_fallback_count"],
        serde_json::json!(1)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["repaint_policy"],
        serde_json::json!("full_surface")
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["filtered_span_count"],
        serde_json::json!(4)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["filtered_command_count"],
        serde_json::json!(9)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["filtered_commands_skipped"],
        serde_json::json!(0)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["filtered_fallback_count"],
        serde_json::json!(1)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["raster_cache_hits"],
        serde_json::json!(6)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["raster_cache_misses"],
        serde_json::json!(2)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["raster_cache_bypasses"],
        serde_json::json!(1)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["raster_cache_opaque_hits"],
        serde_json::json!(4)
    );
    assert_eq!(
        latest.state["profiling"]["surfaces"][0]["invalidation"]["paint"]["raster_cache_translucent_hits"],
        serde_json::json!(2)
    );
    assert_eq!(
        latest.state["benchmarks"]["scenarios"]
            .as_array()
            .expect("benchmark scenarios should stay serialized")
            .len(),
        5
    );
}

#[test]
fn phase26_baseline_proof_records_canonical_scenario_values_and_retained_hotspots() {
    let mut shell = Shell::new();
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();

    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::InputHandling,
        std::time::Duration::from_micros(24),
        Some("phase26_prechange"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::StyleRestyle,
        std::time::Duration::from_micros(61),
        Some("phase26_prechange"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::RuntimeUpdateHandling,
        std::time::Duration::from_micros(42),
        Some("phase26_prechange"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Layout,
        std::time::Duration::from_micros(94),
        Some("phase26_prechange"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::Paint,
        std::time::Duration::from_micros(149),
        Some("phase26_prechange"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(214),
        Some("phase26_prechange"),
    );

    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::RenderObjectSync,
        std::time::Duration::from_micros(34),
        Some("phase26_post"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::RetainedDisplayListUpdate,
        std::time::Duration::from_micros(57),
        Some("phase26_post"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::PaintTraversal,
        std::time::Duration::from_micros(91),
        Some("phase26_post"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::TextShaping,
        std::time::Duration::from_micros(12),
        Some("phase26_post"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingStage::IconImageRaster,
        std::time::Duration::from_micros(6),
        Some("phase26_post"),
    );
    shell.record_surface_invalidation(
        "@mesh/navigation-bar",
        Some("@mesh/navigation-bar"),
        ProfilingInvalidationSnapshot {
            text: TextCacheSnapshot {
                shaping_micros: 12,
                ..Default::default()
            },
            ..Default::default()
        },
    );

    shell.record_surface_redraw(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        Some("phase26_prechange"),
    );
    shell.record_surface_redraw(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        Some("phase26_prechange"),
    );
    shell.record_surface_redraw(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        Some("phase26_prechange"),
    );
    shell.record_surface_profiling_stage(
        "@mesh/audio-popover",
        Some("@mesh/audio-popover"),
        ProfilingStage::TotalSurfaceRender,
        std::time::Duration::from_micros(188),
        Some("phase26_prechange"),
    );

    shell.record_backend_runtime_status(
        "mesh.audio".to_string(),
        "@mesh/pipewire-audio".to_string(),
        BackendRuntimeStatus::Running,
        "phase26 benchmark runtime".to_string(),
    );
    shell.record_backend_profiling_stage(
        "mesh.audio",
        "@mesh/pipewire-audio",
        ProfilingBackendStage::StatePublishDelivery,
        std::time::Duration::from_micros(73),
        Some("phase26_prechange"),
    );

    let snapshot = shell.build_debug_snapshot();
    let scenario_by_id = |id: &str| -> &mesh_core_debug::BenchmarkScenarioSnapshot {
        snapshot
            .benchmarks
            .scenarios
            .iter()
            .find(|scenario| scenario.id.id() == id)
            .expect("benchmark scenario should exist")
    };

    let hover = scenario_by_id("hover");
    assert_eq!(hover.primary_metric, "input_handling: 1 samples, max 24us");
    assert_eq!(hover.secondary_metric, "style_restyle: 1 samples, max 61us");

    let surface_open_close = scenario_by_id("surface_open_close");
    assert_eq!(
        surface_open_close.primary_metric,
        "total_surface_render: 188us"
    );
    assert_eq!(surface_open_close.secondary_metric, "redraw_count: 3");

    let pointer_update = scenario_by_id("pointer_update");
    assert_eq!(
        pointer_update.primary_metric,
        "input_handling: 1 samples, max 24us"
    );
    assert_eq!(
        pointer_update.secondary_metric,
        "layout: 1 samples, max 94us"
    );

    let keyboard_traversal = scenario_by_id("keyboard_traversal");
    assert_eq!(
        keyboard_traversal.primary_metric,
        "input_handling: 1 samples, max 24us"
    );
    assert_eq!(
        keyboard_traversal.secondary_metric,
        "total_surface_render: 1 samples, max 214us"
    );

    let backend_update = scenario_by_id("backend_update");
    assert_eq!(
        backend_update.primary_metric,
        "mesh.audio -> @mesh/pipewire-audio state_publish_delivery: 1 samples, max 73us"
    );
    assert_eq!(
        backend_update.secondary_metric,
        "frontend total_surface_render: 214us"
    );

    let profiling = snapshot
        .profiling
        .as_ref()
        .expect("profiling should be enabled for phase 26 baseline proof");
    let navigation_bar = profiling
        .surfaces
        .iter()
        .find(|surface| surface.surface_id == "@mesh/navigation-bar")
        .expect("navigation bar surface sample should be recorded");
    let retained_hotspots: Vec<_> = navigation_bar
        .stages
        .iter()
        .filter_map(|stage| match stage.stage {
            ProfilingStage::PaintTraversal
            | ProfilingStage::RetainedDisplayListUpdate
            | ProfilingStage::RenderObjectSync
            | ProfilingStage::TextShaping
            | ProfilingStage::IconImageRaster => Some((stage.stage, stage.max_micros)),
            _ => None,
        })
        .collect();

    assert_eq!(
        retained_hotspots,
        vec![
            (ProfilingStage::RenderObjectSync, 34),
            (ProfilingStage::RetainedDisplayListUpdate, 57),
            (ProfilingStage::PaintTraversal, 91),
            (ProfilingStage::TextShaping, 12),
            (ProfilingStage::IconImageRaster, 6),
        ]
    );
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
fn activate_popover_uses_exact_left_edge_anchor_rect() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::with_popover_margin_left(
        "@mesh/language-popover",
        popover_state,
        724,
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/language-popover".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "language-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    let config = shell
        .components
        .iter()
        .find(|runtime| runtime.surface_id == "@mesh/language-popover")
        .and_then(|runtime| runtime.parent.popup_config.as_ref())
        .expect("legacy language popover should be marked for xdg_popup promotion");
    assert_eq!(config.placement.anchor_rect.0, 724);
    assert_eq!(config.placement.anchor_rect.2, 1);
}

#[test]
fn promoted_popover_config_uses_content_size_not_stale_surface_size() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(PopupGeometryRecordingComponent::new(
        "@mesh/theme-selector",
        (112, 74),
        (240, 154),
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/theme-selector".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "theme-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    shell.render_components().unwrap();

    let config = shell
        .presentation_engine
        .testing_popup_config("@mesh/theme-selector")
        .expect("promoted popover should be configured");
    assert_eq!(
        config.placement.size,
        (112, 74),
        "popup positioner geometry must use content size, not tooltip-padded surface size"
    );
}

#[test]
fn layer_surface_config_uses_content_size_not_stale_surface_size_on_first_show() {
    let mut shell = Shell::new();
    shell.register_component(Box::new(MeasuredLayerGeometryComponent::new(
        "@mesh/debug-inspector",
        (480, 640),
        (0, 0),
    )));

    shell
        .apply_request(CoreRequest::ShowSurface {
            surface_id: "@mesh/debug-inspector".into(),
        })
        .unwrap();
    shell.render_components().unwrap();

    let runtime = shell
        .components
        .iter()
        .find(|runtime| runtime.surface_id == "@mesh/debug-inspector")
        .expect("devtools runtime should be registered");
    let config = runtime
        .parent
        .last_surface_config
        .as_ref()
        .expect("layer surface should be configured on first show");
    assert_eq!(
        (config.width, config.height),
        (480, 640),
        "layer surface configure must use measured content size instead of the stale pre-measure size"
    );
    let buffer = runtime
        .parent
        .paint_buffer
        .as_ref()
        .expect("layer surface should allocate a paint buffer on first show");
    assert_eq!(
        (buffer.width, buffer.height),
        (480, 640),
        "first visible frame must allocate a paint buffer at the measured content size"
    );
}

#[test]
fn hover_bridge_hide_defers_promoted_popover_close_until_deadline() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/quick-settings",
        popover_state,
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/quick-settings".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "settings-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    let emitted = shell
        .apply_request(CoreRequest::HidePopover {
            surface_id: "@mesh/quick-settings".into(),
            defer_for_hover_bridge: true,
        })
        .unwrap();
    assert!(emitted.is_empty());
    assert!(
        shell
            .pending_popover_hides
            .contains_key("@mesh/quick-settings")
    );
    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/quick-settings")
            .is_some_and(|state| state.visible)
    );

    shell.pending_popover_hides.insert(
        "@mesh/quick-settings".into(),
        Instant::now() - Duration::from_millis(1),
    );
    let emitted = shell.complete_due_surface_transitions().unwrap();
    assert!(emitted.is_empty());
    assert!(
        !shell
            .pending_popover_hides
            .contains_key("@mesh/quick-settings")
    );
    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/quick-settings")
            .is_some_and(|state| !state.visible)
    );
}

#[test]
fn pointer_enter_cancels_hover_bridge_popover_hide() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/quick-settings",
        popover_state,
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/quick-settings".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "settings-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    shell
        .apply_request(CoreRequest::HidePopover {
            surface_id: "@mesh/quick-settings".into(),
            defer_for_hover_bridge: true,
        })
        .unwrap();
    assert!(
        shell
            .pending_popover_hides
            .contains_key("@mesh/quick-settings")
    );

    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerMove {
            surface_id: "@mesh/quick-settings".into(),
            x: 8.0,
            y: 8.0,
        },
    );
    shell.dispatch_wayland().unwrap();

    assert!(
        !shell
            .pending_popover_hides
            .contains_key("@mesh/quick-settings")
    );
    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/quick-settings")
            .is_some_and(|state| state.visible)
    );
}

#[test]
fn pointer_leave_from_promoted_popover_schedules_hover_bridge_hide() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/quick-settings",
        popover_state,
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/quick-settings".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "settings-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerLeave {
            surface_id: "@mesh/quick-settings".into(),
        },
    );
    shell.dispatch_wayland().unwrap();

    assert!(
        shell
            .pending_popover_hides
            .contains_key("@mesh/quick-settings")
    );
    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/quick-settings")
            .is_some_and(|state| state.visible)
    );
}

#[test]
fn activating_popover_closes_promoted_sibling_from_same_trigger_surface() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let quick_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let language_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/quick-settings",
        quick_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/language-popover",
        language_state,
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/quick-settings".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "settings-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();

    let emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/language-popover".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "language-button".into(),
            focus: false,
        })
        .unwrap();

    assert!(emitted.iter().any(|request| matches!(
        request,
        CoreRequest::HidePopover {
            surface_id,
            defer_for_hover_bridge: false,
        } if surface_id == "@mesh/quick-settings"
    )));
    assert!(emitted.iter().any(|request| matches!(
        request,
        CoreRequest::ShowSurface { surface_id } if surface_id == "@mesh/language-popover"
    )));
}

#[test]
fn dismissed_legacy_promoted_popover_hides_surface_state() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let trigger_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    let popover_state = Arc::new(Mutex::new(FocusRecordingState::default()));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/navigation-bar",
        trigger_state,
    )));
    shell.register_component(Box::new(FocusRecordingComponent::new(
        "@mesh/quick-settings",
        popover_state,
    )));

    let mut emitted = shell
        .apply_request(CoreRequest::ActivatePopover {
            surface_id: "@mesh/quick-settings".into(),
            trigger_surface: "@mesh/navigation-bar".into(),
            trigger_key: "settings-button".into(),
            focus: false,
        })
        .unwrap();
    shell.drain_requests(&mut emitted).unwrap();
    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/quick-settings")
            .is_some_and(|state| state.visible)
    );

    shell
        .presentation_engine
        .testing_push_dismissed_popup("@mesh/quick-settings");
    shell.render_components().unwrap();

    assert!(
        shell
            .core
            .surfaces
            .get("@mesh/quick-settings")
            .is_some_and(|state| !state.visible)
    );
    let runtime = shell
        .components
        .iter()
        .find(|runtime| runtime.surface_id == "@mesh/quick-settings")
        .expect("quick settings runtime should remain registered");
    assert!(runtime.parent.popup_parent_surface.is_none());
    assert!(runtime.parent.popup_config.is_none());
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
    shell
        .surfaces
        .get_mut("@mesh/audio-popover")
        .unwrap()
        .keyboard_mode = mesh_core_wayland::KeyboardMode::Exclusive;

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
    shell
        .surfaces
        .get_mut("@mesh/audio-popover")
        .unwrap()
        .keyboard_mode = mesh_core_wayland::KeyboardMode::OnDemand;
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
fn identical_service_state_is_deduplicated_before_delivery() {
    let mut shell = Shell::new();
    let event = service_update(
        "mesh.audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 65.0, "muted": false }),
    );

    assert!(shell.record_latest_service_state(&event));
    assert!(!shell.record_latest_service_state(&event));

    let changed = service_update(
        "mesh.audio",
        "@mesh/pipewire-audio",
        serde_json::json!({ "available": true, "percent": 66.0, "muted": false }),
    );
    assert!(shell.record_latest_service_state(&changed));
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
    } = &events[0]
    else {
        panic!("expected first service update");
    };
    assert_eq!(source_module, "@mesh/pipewire-audio");
    assert_eq!(payload["percent"], serde_json::json!(40.0));
    let ServiceEvent::Updated {
        source_module,
        payload,
        ..
    } = events.last().unwrap()
    else {
        panic!("expected last service update");
    };
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
            .get("__shell")
            .and_then(|shell| shell.get("theme"))
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
        .get("__shell")
        .and_then(|shell| shell.get("theme"))
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
        shell.components[0].parent.force_full_present,
        "theme changes must force a full present for already-painted surfaces"
    );
}

#[test]
fn component_runtime_resolves_parent_and_child_surface_targets() {
    // Proves the one-VM → parent + N child-surface plumbing: a single
    // ComponentRuntime owns a parent surface plus a synthetically injected
    // child popup surface, and the shell resolves *both* surface ids back to
    // the same component while distinguishing which target each names.
    let mut shell = Shell::new();
    let seen_events = Arc::new(Mutex::new(Vec::new()));
    shell
        .components
        .push(super::types::ComponentRuntime::new(Box::new(
            RecordingComponent::new(seen_events),
        )));

    let parent_id = "@test/recording".to_string();
    let child_id = "@test/recording#popover:0".to_string();

    // The parent target alone resolves before any child exists.
    assert_eq!(
        shell.component_target_for_surface(&parent_id),
        Some((0, super::types::TargetRef::Parent))
    );
    assert_eq!(shell.component_target_for_surface(&child_id), None);

    // Inject an auto-derived child surface (what the popover reconcile builds).
    shell.components[0]
        .children
        .push(super::types::ChildSurface {
            target: super::types::SurfaceTarget::new(
                child_id.clone(),
                mesh_core_presentation::LayerSurfaceSizePolicy::Flexible,
            ),
            node_key: "root/0/popover".to_string(),
            anchor_rect: (12, 0, 40, 56),
            content_padding: (0, 0, 0, 0),
            closing_until: None,
        });

    // Both surface ids now map to the same component, each tagged with its
    // target; targets() enumerates parent first, then the child.
    assert_eq!(
        shell.component_target_for_surface(&parent_id),
        Some((0, super::types::TargetRef::Parent))
    );
    assert_eq!(
        shell.component_target_for_surface(&child_id),
        Some((0, super::types::TargetRef::Child(0)))
    );
    let target_ids: Vec<&str> = shell.components[0]
        .targets()
        .map(|target| target.surface_id.as_str())
        .collect();
    assert_eq!(target_ids, vec![parent_id.as_str(), child_id.as_str()]);

    // The child carries the originating node key + anchor rect used by the
    // popup reconcile/positioner.
    assert_eq!(shell.components[0].children[0].node_key, "root/0/popover");
    assert_eq!(shell.components[0].children[0].anchor_rect, (12, 0, 40, 56));

    // The child target is independently addressable for per-surface state.
    shell.components[0]
        .target_mut(super::types::TargetRef::Child(0))
        .force_full_present = true;
    assert!(
        shell.components[0].children[0].target.force_full_present,
        "target_mut(Child) must address the child's own render state"
    );
}

fn render_components_until_child_popup(shell: &mut Shell) {
    // Child popups stage one parent repaint with `mesh-surface-entering`
    // before the xdg_popup is created and painted.
    shell.render_components().unwrap();
    shell.render_components().unwrap();
}

#[test]
fn child_surface_reconcile_creates_popup_and_paints_subtree() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);

    assert_eq!(shell.components[0].children.len(), 1);
    let child_id = shell.components[0].children[0].target.surface_id.clone();
    let config = shell
        .presentation_engine
        .testing_popup_config(&child_id)
        .expect("child popover should configure an xdg_popup");
    assert_eq!(config.parent_surface_id, "@test/popover-host");
    assert_eq!(config.placement.anchor_rect, (8, 10, 40, 16));
    assert_eq!(config.placement.size, (72, 32));
    assert!(
        shell
            .presentation_engine
            .testing_presented_surfaces()
            .iter()
            .any(|surface| surface == &child_id),
        "child popup subtree should be presented separately"
    );
    assert_eq!(
        state.lock().unwrap().painted_nodes.as_slice(),
        ["root/popover"]
    );
}

#[test]
fn child_surface_presents_full_damage_every_frame() {
    // `paint_child_surface` clears and fully repaints the child buffer each
    // frame, so every child present must report full-surface damage —
    // anything narrower leaves stale pixels in the compositor and freezes
    // popover enter/exit transitions.
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state)));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();

    shell.render_components().unwrap();

    let child_damage = shell
        .presentation_engine
        .testing_presented_damage()
        .iter()
        .filter(|(surface, _)| surface == &child_id)
        .map(|(_, damage)| damage.as_slice())
        .collect::<Vec<_>>();
    assert!(!child_damage.is_empty(), "child popup should be presented");
    assert!(
        child_damage
            .iter()
            .all(|damage| damage.len() == 1 && damage[0].x == 0 && damage[0].y == 0),
        "every child popup present should carry full-surface damage, got {child_damage:?}"
    );
}

#[test]
fn child_surface_reconcile_removes_closed_popover() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();
    state.lock().unwrap().open = false;
    shell.render_components().unwrap();

    assert!(shell.components[0].children.is_empty());
    assert!(
        shell
            .presentation_engine
            .testing_destroyed_popups()
            .contains(&child_id)
    );
    assert!(!shell.core.surfaces.contains_key(&child_id));
    assert!(shell.component_target_for_surface(&child_id).is_none());
}

#[test]
fn hiding_parent_surface_destroys_child_popups_and_clears_child_keyboard_focus() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state)));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();
    shell.keyboard_focus_surface = Some(child_id.clone());

    shell
        .set_surface_visibility_now("@test/popover-host".to_string(), false)
        .unwrap();

    assert!(shell.components[0].children.is_empty());
    assert!(
        shell
            .presentation_engine
            .testing_destroyed_popups()
            .contains(&child_id)
    );
    assert!(!shell.core.surfaces.contains_key(&child_id));
    assert!(!shell.surfaces.contains_key(&child_id));
    assert!(shell.component_target_for_surface(&child_id).is_none());
    assert_eq!(shell.keyboard_focus_surface, None);
}

#[test]
fn child_surface_reconcile_plays_exit_transition_before_teardown() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState {
        hide_transition_ms: 120,
        ..PopoverHarnessState::default()
    }));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();

    state.lock().unwrap().open = false;
    shell.render_components().unwrap();

    // The child popup should still be alive and repainted with the exiting
    // class so its own CSS exit transition (opacity/transform) can animate,
    // instead of being torn down the instant `open` flips false.
    assert_eq!(
        shell.components[0].children.len(),
        1,
        "closing popover should stay mounted for its exit transition"
    );
    assert!(shell.components[0].children[0].closing_until.is_some());
    assert!(
        !shell
            .presentation_engine
            .testing_destroyed_popups()
            .contains(&child_id)
    );
    assert_eq!(
        state.lock().unwrap().exiting_paints.last(),
        Some(&true),
        "the closing repaint pass should mark the popover subtree as exiting"
    );

    // Simulate the exit-transition deadline having elapsed.
    shell.components[0].children[0].closing_until = Some(Instant::now() - Duration::from_millis(1));
    shell.render_components().unwrap();

    assert!(shell.components[0].children.is_empty());
    assert!(
        shell
            .presentation_engine
            .testing_destroyed_popups()
            .contains(&child_id)
    );
}

#[test]
fn child_surface_reopen_cancels_pending_exit_transition() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState {
        hide_transition_ms: 120,
        ..PopoverHarnessState::default()
    }));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);
    state.lock().unwrap().open = false;
    shell.render_components().unwrap();
    assert!(shell.components[0].children[0].closing_until.is_some());

    // Reopening before the grace period elapses should cancel the exit and
    // resume normal (non-exiting) repaints.
    state.lock().unwrap().open = true;
    shell.render_components().unwrap();

    assert_eq!(shell.components[0].children.len(), 1);
    assert!(shell.components[0].children[0].closing_until.is_none());
    assert_eq!(state.lock().unwrap().exiting_paints.last(), Some(&false));
}

#[test]
fn parent_pointer_leave_defers_child_popover_close() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();

    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerLeave {
            surface_id: "@test/popover-host".into(),
        },
    );
    shell.dispatch_wayland().unwrap();

    assert!(
        shell.pending_popover_hides.contains_key(&child_id),
        "leaving the parent trigger surface should arm a bridge hide for its child popup"
    );

    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerMove {
            surface_id: child_id.clone(),
            x: 4.0,
            y: 4.0,
        },
    );
    shell.dispatch_wayland().unwrap();

    assert!(
        !shell.pending_popover_hides.contains_key(&child_id),
        "entering the promoted child popup should cancel the bridge hide"
    );
}

#[test]
fn child_popover_hover_bridge_deadline_synthesizes_pointer_leave() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();

    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerLeave {
            surface_id: "@test/popover-host".into(),
        },
    );
    shell.dispatch_wayland().unwrap();
    shell
        .pending_popover_hides
        .insert(child_id.clone(), Instant::now() - Duration::from_millis(1));

    shell.complete_due_surface_transitions().unwrap();

    let inputs = &state.lock().unwrap().child_inputs;
    assert!(
        inputs.iter().any(|(node_key, input)| {
            node_key == "root/popover" && matches!(input, ComponentInput::PointerLeave)
        }),
        "bridge deadline should route PointerLeave into the child popup component"
    );
    assert!(
        !shell.pending_popover_hides.contains_key(&child_id),
        "bridge deadline should drain the pending hide"
    );
}

#[test]
fn dismissed_popup_drain_removes_child_surface() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));

    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();
    shell
        .presentation_engine
        .testing_push_dismissed_popup(child_id.clone());
    shell.render_components().unwrap();

    assert!(shell.components[0].children.is_empty());
    assert!(!shell.core.surfaces.contains_key(&child_id));
    assert!(shell.component_target_for_surface(&child_id).is_none());

    {
        let mut state = state.lock().unwrap();
        state.open = false;
    }
    shell.render_components().unwrap();
    {
        let mut state = state.lock().unwrap();
        state.open = true;
    }
    render_components_until_child_popup(&mut shell);
    assert_eq!(
        shell.components[0].children.len(),
        1,
        "a later close/open cycle should create a fresh popup"
    );
}

#[test]
fn child_surface_input_routes_to_local_child_handler_and_profiles() {
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(true);
    let state = Arc::new(Mutex::new(PopoverHarnessState::default()));
    shell.register_component(Box::new(PopoverHarnessComponent::new(state.clone())));
    shell
        .apply_request(CoreRequest::ToggleDebugProfiling)
        .unwrap();
    render_components_until_child_popup(&mut shell);
    let child_id = shell.components[0].children[0].target.surface_id.clone();

    shell
        .presentation_engine
        .testing_push_event(mesh_core_presentation::WindowEvent::Scroll {
            surface_id: child_id.clone(),
            x: 6.0,
            y: 7.0,
            dx: 0.0,
            dy: -1.0,
        });
    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerButton {
            surface_id: child_id.clone(),
            x: 10.0,
            y: 12.0,
            pressed: true,
        },
    );
    shell.dispatch_wayland().unwrap();

    let inputs = &state.lock().unwrap().child_inputs;
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[0].0, "root/popover");
    assert!(matches!(
        inputs[0].1,
        ComponentInput::Scroll {
            x: 6.0,
            y: 7.0,
            dx: 0.0,
            dy: -1.0
        }
    ));
    assert_eq!(inputs[1].0, "root/popover");
    assert!(matches!(
        inputs[1].1,
        ComponentInput::PointerButton {
            x: 10.0,
            y: 12.0,
            pressed: true
        }
    ));
    let snapshot = shell.build_debug_snapshot();
    let child_surface = snapshot
        .profiling
        .expect("profiling should be enabled")
        .surfaces
        .into_iter()
        .find(|surface| surface.surface_id == child_id)
        .expect("child input should be profiled against the popup surface");
    assert!(child_surface.stages.iter().any(|stage| {
        stage.stage == mesh_core_debug::ProfilingStage::InputHandling && stage.sample_count >= 2
    }));
}

#[test]
fn set_theme_loads_css_package_and_updates_runtime_setting() {
    let mut shell = Shell::new();

    shell.apply_set_theme("mesh-default-light").unwrap();

    assert_eq!(shell.theme.active().id, "mesh-default-light");
    assert_eq!(shell.settings.theme.active, "mesh-default-light");
    assert_eq!(
        shell
            .theme
            .active()
            .token("color.surface")
            .map(ToString::to_string),
        Some("#FFFBFE".into())
    );
    assert!(
        shell
            .theme_watch
            .path
            .ends_with("mesh-default-light/theme.css"),
        "theme watcher should follow the active CSS package"
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
    let ServiceEvent::Updated { payload, .. } = events.last().unwrap() else {
        panic!("expected theme service update");
    };
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
                    "implements": [{ "interface": "mesh.audio", "provider": "test" }]
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
fn backend_lifecycle_accepts_provider_without_consumer_capabilities() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": true }
              },
              "providers": { "mesh.example": "@mesh/backend" }
            }"#,
        vec![
            r#"{
                  "name": "@mesh/backend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [{ "interface": "mesh.example", "provider": "test" }]
                  }
                }"#,
        ],
    );
    let (_dir, module) = module_instance("@mesh/backend", Some("src/main.luau"));
    let modules = HashMap::from([("@mesh/backend".to_string(), module)]);
    let interfaces = InterfaceRegistry::new();
    let mut contract = test_contract("mesh.example");
    contract.capabilities.required = vec!["service.example.read".to_string()];
    interfaces.register_contract(contract);
    register_test_provider(&interfaces, "mesh.example", "@mesh/backend");

    let (candidates, statuses) =
        backend_launch_candidates_from_graph(&graph, &modules, &test_config(), &interfaces);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].interface, "mesh.example");
    assert!(
        statuses
            .iter()
            .all(|status| status.status != "missing_capability")
    );
}

#[test]
fn backend_lifecycle_accepts_valid_provider_with_contract() {
    let graph = graph_from_json(
        r#"{
              "schemaVersion": 1,
              "modulesDir": "modules",
              "modules": {
                "@mesh/backend": { "kind": "backend", "path": "@mesh/backend", "enabled": true }
              },
              "providers": { "mesh.example": "@mesh/backend" }
            }"#,
        vec![
            r#"{
                  "name": "@mesh/backend",
                  "version": "0.1.0",
                  "mesh": {
                    "apiVersion": "0.1",
                    "kind": "backend",
                    "capabilities": { "required": ["exec.example"] },
                    "entrypoints": { "main": "src/main.luau" },
                    "implements": [{ "interface": "mesh.example", "provider": "test" }]
                  }
                }"#,
        ],
    );
    let (_dir, mut module) = module_instance("@mesh/backend", Some("src/main.luau"));
    module.manifest.capabilities.required = vec!["exec.example".to_string()];
    let modules = HashMap::from([("@mesh/backend".to_string(), module)]);
    let interfaces = InterfaceRegistry::new();
    let mut contract = test_contract("mesh.example");
    contract.capabilities.required = vec!["service.example.read".to_string()];
    interfaces.register_contract(contract);
    register_test_provider(&interfaces, "mesh.example", "@mesh/backend");

    let (candidates, statuses) =
        backend_launch_candidates_from_graph(&graph, &modules, &test_config(), &interfaces);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].interface, "mesh.example");
    assert_eq!(candidates[0].capabilities, vec!["exec.example".to_string()]);
    assert!(
        statuses
            .iter()
            .all(|status| status.status != "missing_capability")
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
fn service_command_dispatch_records_debug_method_call() {
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
        &serde_json::json!({ "volume": 0.4 }),
        "@mesh/panel",
        &capabilities,
    );

    assert_eq!(result["ok"], serde_json::json!(true));
    assert_eq!(rx.try_recv().unwrap().command, "set_volume");
    let snapshot = shell.build_debug_snapshot();
    assert!(snapshot.method_calls.iter().any(|entry| {
        entry.interface == "mesh.audio"
            && entry.provider_id.as_deref() == Some("@mesh/pipewire-audio")
            && entry.source_module_id == "@mesh/panel"
            && entry.command == "set_volume"
            && entry.status == "queued"
            && entry.queued
    }));
}

#[test]
fn backend_command_result_records_debug_method_result() {
    let mut shell = Shell::new();
    let mut pending = VecDeque::new();

    shell
        .handle_shell_message(
            &mut pending,
            super::types::ShellMessage::BackendCommandResult {
                interface: "mesh.audio".to_string(),
                provider_id: "@mesh/pipewire-audio".to_string(),
                command: "set_volume".to_string(),
                result: serde_json::json!({ "ok": true, "volume": 0.4 }),
            },
        )
        .unwrap();

    let snapshot = shell.build_debug_snapshot();
    assert!(snapshot.method_calls.iter().any(|entry| {
        entry.interface == "mesh.audio"
            && entry.provider_id.as_deref() == Some("@mesh/pipewire-audio")
            && entry.source_module_id == "<backend>"
            && entry.command == "set_volume"
            && entry.status == "completed"
            && !entry.queued
    }));
}

#[test]
fn backend_interface_event_validates_and_delivers_to_components() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::new(events.clone())));

    let requests = shell
        .broadcast_backend_interface_event(
            "mesh.audio".to_string(),
            "@mesh/pipewire-audio".to_string(),
            "VolumeChanged".to_string(),
            serde_json::json!({ "device_id": "default", "level": 42.0 }),
        )
        .unwrap();

    assert!(requests.is_empty());
    let events = events.lock().unwrap();
    assert_eq!(events.len(), 1);
    let ServiceEvent::InterfaceEvent {
        service,
        source_module,
        name,
        payload,
    } = events.last().unwrap()
    else {
        panic!("expected interface event");
    };
    assert_eq!(service, "mesh.audio");
    assert_eq!(source_module, "@mesh/pipewire-audio");
    assert_eq!(name, "VolumeChanged");
    assert_eq!(payload["level"], serde_json::json!(42.0));
}

#[test]
fn backend_interface_event_drops_invalid_payload_with_diagnostic() {
    let runtime = Runtime::new().unwrap();
    let mut shell = Shell::new();
    shell
        .interfaces
        .register_contract(test_contract("mesh.audio"));
    let (slot, _rx) = backend_runtime_slot(&runtime, "mesh.audio", "@mesh/pipewire-audio");
    shell.replace_backend_runtime("mesh.audio".to_string(), slot);
    let events = Arc::new(Mutex::new(Vec::new()));
    shell.register_component(Box::new(RecordingComponent::new(events.clone())));

    shell
        .broadcast_backend_interface_event(
            "mesh.audio".to_string(),
            "@mesh/pipewire-audio".to_string(),
            "VolumeChanged".to_string(),
            serde_json::json!({ "device_id": "default", "level": "loud" }),
        )
        .unwrap();

    assert!(events.lock().unwrap().is_empty());
    assert!(
        shell
            .diagnostics
            .snapshot()
            .iter()
            .any(|(module_id, health)| {
                module_id == "@mesh/pipewire-audio"
                    && health
                        .to_string()
                        .contains("payload field 'level' expected float")
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
fn content_size_is_the_root_laid_out_box() {
    // Sizing is fully CSS-driven now: the surface root's own laid-out box is
    // the measured size. The layout engine resolved the root's CSS width/height
    // (here `fit-content` shrank it to 336x332) with its clamps already
    // applied, so measurement just reads that box — no manifest inputs.
    let mut root = node("root", 0.0, 0.0, 336.0, 332.0);
    root.children.push(node("column", 12.0, 12.0, 312.0, 308.0));

    assert_eq!(measure_content_size(&root, 640, 360), (336, 332));
}

#[test]
fn content_size_falls_back_when_root_has_no_extent() {
    // A degenerate first frame (root not laid out yet) falls back to the
    // available size passed by the caller.
    let root = node("root", 0.0, 0.0, 0.0, 0.0);

    assert_eq!(measure_content_size(&root, 1920, 32), (1920, 32));
}

#[test]
fn installed_module_graph_exposes_shell_package_choices() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let graph = mesh_core_module::package::load_installed_module_graph(
        &workspace_root.join("config/module.json"),
    )
    .unwrap();

    assert_eq!(
        graph.declared_interface("mesh.audio").unwrap().module_id,
        "@mesh/audio-interface"
    );
    assert_eq!(
        graph.active_provider("mesh.audio").unwrap().module_id,
        "@mesh/pipewire-audio"
    );
    assert_eq!(graph.backend_providers_for_interface("mesh.audio").len(), 2);
    assert!(
        graph
            .backend_providers_for_interface("mesh.audio")
            .iter()
            .any(|provider| provider.module_id == "@mesh/pulseaudio-audio")
    );
    assert!(
        graph
            .icon_pack_contributions()
            .iter()
            .any(|icon_pack| icon_pack.module_id == "@mesh/icons-default")
    );
    assert!(graph.active_provider("mesh.network").is_none());
    assert_eq!(
        graph.active_provider("mesh.power").unwrap().module_id,
        "@mesh/upower-power"
    );
    assert_eq!(
        graph.backend_providers_for_interface("mesh.network").len(),
        0
    );
    assert_eq!(graph.backend_providers_for_interface("mesh.power").len(), 1);
    assert!(
        graph
            .frontend_modules()
            .iter()
            .any(|module| module.id == "@mesh/navigation-bar")
    );
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

    let mut shell = Shell::new();
    shell.register_interfaces_from_graph(&graph);
    let contracts = shell.interfaces.contracts_for("mesh.audio");
    assert!(contracts.iter().any(|contract| {
        contract.interface == "mesh.audio"
            && contract
                .state_fields
                .iter()
                .any(|field| field.name == "available")
    }));
    let providers = shell.interfaces.providers_for("mesh.audio");
    assert_eq!(providers.len(), 2);
    assert!(providers.iter().any(|provider| {
        provider.provider_module == "@mesh/pipewire-audio"
            && provider.backend_name == "pipewire"
            && provider.base_module.as_deref() == Some("@mesh/audio-interface")
    }));
    assert!(providers.iter().any(|provider| {
        provider.provider_module == "@mesh/pulseaudio-audio"
            && provider.backend_name == "pulseaudio"
            && provider.base_module.as_deref() == Some("@mesh/audio-interface")
    }));
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
        "built-in debug inspector should load as a shell surface even when absent from config/module.json"
    );
}

#[test]
fn backend_lifecycle_uses_explicit_active_provider_from_package_graph() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let graph = mesh_core_module::package::load_installed_module_graph(
        &workspace_root.join("config/module.json"),
    )
    .unwrap();
    let (_pipewire_dir, pipewire) = module_instance("@mesh/pipewire-audio", Some("src/main.luau"));
    let (_pulse_dir, pulse) = module_instance("@mesh/pulseaudio-audio", Some("src/main.luau"));
    let (_upower_dir, upower) = module_instance("@mesh/upower-power", Some("src/main.luau"));
    let (_hyprland_dir, hyprland) = module_instance("@mesh/hyprland-wm", Some("src/main.luau"));
    let modules = HashMap::from([
        ("@mesh/pipewire-audio".to_string(), pipewire),
        ("@mesh/pulseaudio-audio".to_string(), pulse),
        ("@mesh/upower-power".to_string(), upower),
        ("@mesh/hyprland-wm".to_string(), hyprland),
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
    assert_eq!(candidates.len(), 3);
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
            .any(|candidate| candidate.interface == "mesh.power"
                && candidate.module_id == "@mesh/upower-power")
    );
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.interface == "mesh.wm"
                && candidate.module_id == "@mesh/hyprland-wm")
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
                    "implements": [{ "interface": "mesh.audio", "provider": "test" }]
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
                    "implements": [{ "interface": "mesh.audio", "provider": "test" }]
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
                    "implements": [{ "interface": "mesh.audio", "provider": "test" }]
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

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].interface, "mesh.audio");
    assert!(
        statuses
            .iter()
            .all(|status| status.status != "no_active_provider")
    );
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
        BackendRuntimeStatus::MissingCapability,
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
            "missing_capability",
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
            .backend_runtime_status("mesh.audio", "@mesh/old-audio")
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
            .backend_runtime_status("mesh.audio", "@mesh/pipewire-audio")
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
            .backend_runtime_status("mesh.audio", "@mesh/old-audio")
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
            .backend_runtime_status("mesh.audio", "@mesh/pipewire-audio")
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
    let entry = shell
        .backend_runtime_status("mesh.network", "@mesh/networkmanager-network")
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
    let entry = shell
        .backend_runtime_status("mesh.network", "@mesh/networkmanager-network")
        .unwrap();
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
    assert_eq!(settings.layout.exclusive_zone, 12);
    assert_eq!(
        settings.layout.keyboard_mode,
        mesh_core_wayland::KeyboardMode::Exclusive
    );
    assert!(settings.layout.visible_on_start);
}

#[test]
fn hide_surface_uses_configured_exit_transition_before_unmapping() {
    let state = Arc::new(Mutex::new(TransitionRecordingState::default()));
    let mut shell = Shell::new();
    shell.register_component(Box::new(TransitionRecordingComponent::new(
        "@test/transition",
        120,
        Arc::clone(&state),
    )));

    let emitted = shell
        .apply_request(CoreRequest::HideSurface {
            surface_id: "@test/transition".into(),
        })
        .unwrap();
    assert!(
        emitted.is_empty(),
        "hide transition should not broadcast hidden until the timer elapses"
    );
    let surface = shell.core.surfaces.get("@test/transition").unwrap();
    assert!(surface.visible);
    assert!(surface.closing_until.is_some());
    assert_eq!(state.lock().unwrap().exiting, vec![true]);

    shell
        .core
        .surfaces
        .get_mut("@test/transition")
        .unwrap()
        .closing_until = Some(std::time::Instant::now() - std::time::Duration::from_millis(1));
    let emitted = shell.complete_due_surface_transitions().unwrap();
    assert!(emitted.is_empty());
    let surface = shell.core.surfaces.get("@test/transition").unwrap();
    assert!(!surface.visible);
    assert!(surface.closing_until.is_none());
    assert_eq!(state.lock().unwrap().exiting, vec![true, false]);
}

#[test]
fn hide_surface_without_transition_unmaps_immediately() {
    let state = Arc::new(Mutex::new(TransitionRecordingState::default()));
    let mut shell = Shell::new();
    shell.register_component(Box::new(TransitionRecordingComponent::new(
        "@test/immediate",
        0,
        Arc::clone(&state),
    )));

    let emitted = shell
        .apply_request(CoreRequest::HideSurface {
            surface_id: "@test/immediate".into(),
        })
        .unwrap();
    assert!(emitted.is_empty());
    let surface = shell.core.surfaces.get("@test/immediate").unwrap();
    assert!(!surface.visible);
    assert!(surface.closing_until.is_none());
    assert_eq!(state.lock().unwrap().exiting, vec![false]);
}

#[test]
fn wayland_parent_input_uses_content_size_not_tooltip_inflated_surface_size() {
    let state = Arc::new(Mutex::new(InputSizeRecordingState::default()));
    let mut shell = Shell::new();
    shell.presentation_engine =
        mesh_core_presentation::PresentationEngine::testing_with_popup_support(false);
    shell.register_component(Box::new(InputSizeRecordingComponent::new(
        Arc::clone(&state),
        (100, 50),
    )));
    let surface = shell
        .surfaces
        .get_mut("@test/input-size")
        .expect("registered test surface");
    surface.width = 100;
    surface.height = 350;

    shell.presentation_engine.testing_push_event(
        mesh_core_presentation::WindowEvent::PointerMove {
            surface_id: "@test/input-size".into(),
            x: 20.0,
            y: 25.0,
        },
    );
    shell.dispatch_wayland().unwrap();

    assert_eq!(
        state.lock().unwrap().sizes,
        vec![(100, 50)],
        "parent input must rebuild/hit-test against the real content size, not the tooltip-padded buffer"
    );
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
