use super::*;

pub(super) static SETTINGS_ENV_LOCK: Mutex<()> = Mutex::new(());

/// Serialize the tests that mutate process-wide settings environment variables.
///
/// The lock guards `()`, so a panic under it leaves nothing inconsistent —
/// recovering from poisoning keeps one genuine failure from re-reporting itself
/// as unrelated failures in every test that takes the lock afterwards.
pub(super) fn settings_env_lock() -> std::sync::MutexGuard<'static, ()> {
    SETTINGS_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) struct EnvGuard {
    pub(super) key: &'static str,
    pub(super) old: Option<String>,
}

impl EnvGuard {
    pub(super) fn set(key: &'static str, value: &std::path::Path) -> Self {
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

pub(super) fn node(tag: &str, x: f32, y: f32, width: f32, height: f32) -> WidgetNode {
    let mut node = WidgetNode::new(tag);
    node.layout = LayoutRect {
        x,
        y,
        width,
        height,
    };
    node
}

pub(super) fn minimal_manifest(id: &str) -> Manifest {
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

pub(super) fn minimal_backend_manifest(id: &str, entrypoint: Option<&str>) -> Manifest {
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

pub(super) fn module_instance(
    id: &str,
    entrypoint: Option<&str>,
) -> (tempfile::TempDir, ModuleInstance) {
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

pub(super) fn write_shell_discovery_manifest(module_dir: &Path, id: &str, payload_count: usize) {
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

/// A settings store with no stored overrides — backend candidates then carry
/// exactly the props their own manifests declare.
pub(super) fn test_settings() -> mesh_core_config::SettingsStore {
    mesh_core_config::SettingsStore::default()
}

pub(super) fn loaded_module(json: &str) -> LoadedModuleManifest {
    LoadedModuleManifest {
        manifest: ModuleManifest::from_json_str(json).unwrap(),
        path: PathBuf::from("<test>/module.json"),
        source: ModuleManifestSource::CanonicalModuleJson,
        diagnostics: Vec::new(),
    }
}

pub(super) fn graph_from_json(root: &str, modules: Vec<&str>) -> InstalledModuleGraph {
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

pub(super) fn test_contract(interface: &str) -> InterfaceContract {
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
                state_binding: None,
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
                state_binding: Some(mesh_core_service::StateBinding {
                    field: "muted".to_string(),
                    from_arg: Some("muted".to_string()),
                    toggle: false,
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

pub(super) fn register_test_provider(
    interfaces: &InterfaceRegistry,
    interface: &str,
    provider_id: &str,
) {
    interfaces.register(InterfaceProvider {
        interface: interface.to_string(),
        version: Some("1.0".to_string()),
        base_module: Some("@mesh/test-interface".to_string()),
        provider_module: provider_id.to_string(),
        backend_name: provider_id.to_string(),
        priority: 100,
    });
}

pub(super) fn backend_runtime_slot(
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
            event_provider_id: Arc::new(std::sync::RwLock::new(provider_id.to_string())),
            command_tx,
            task: task.abort_handle(),
        },
        command_rx,
    )
}

pub(super) fn service_update(
    interface: &str,
    provider_id: &str,
    payload: serde_json::Value,
) -> ServiceEvent {
    ServiceEvent::Updated {
        service: interface.to_string(),
        source_module: provider_id.to_string(),
        payload,
    }
}

/// Count the recorded `Updated` events for one interface.
///
/// A reload broadcasts every service whose state it touched — a theme reload
/// also publishes `mesh.settings` — so asserting on the total makes a test
/// about one service fail when an unrelated service starts broadcasting.
pub(super) fn recorded_updates_for(
    events: &Arc<Mutex<Vec<ServiceEvent>>>,
    interface: &str,
) -> usize {
    events
        .lock()
        .unwrap()
        .iter()
        .filter(
            |event| matches!(event, ServiceEvent::Updated { service, .. } if service == interface),
        )
        .count()
}

pub(super) struct RecordingComponent {
    pub(super) events: Arc<Mutex<Vec<ServiceEvent>>>,
    pub(super) keybinds: Vec<mesh_core_debug::DebugKeybindEntry>,
}

impl RecordingComponent {
    pub(super) fn new(events: Arc<Mutex<Vec<ServiceEvent>>>) -> Self {
        Self {
            events,
            keybinds: Vec::new(),
        }
    }

    pub(super) fn with_keybinds(
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
        _extent: crate::shell::types::SurfaceExtent,
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
pub(super) struct IndexedRecordingState {
    pub(super) observed: usize,
    pub(super) handled: Vec<ServiceEvent>,
    pub(super) cached: Vec<ServiceEvent>,
}

pub(super) struct IndexedRecordingComponent {
    pub(super) id: String,
    pub(super) summary: Arc<Mutex<Option<ServiceObservationSummary>>>,
    pub(super) state: Arc<Mutex<IndexedRecordingState>>,
}

impl IndexedRecordingComponent {
    pub(super) fn new(
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

    fn cache_service_payload(&mut self, event: &ServiceEvent) {
        self.state.lock().unwrap().cached.push(event.clone());
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
        _extent: crate::shell::types::SurfaceExtent,
        _buffer: &mut mesh_core_render::PixelBuffer,
        _scale: f32,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }
}

pub(super) fn indexed_summary_observes_event(
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
pub(super) struct FocusRecordingState {
    pub(super) releases: usize,
    pub(super) registered_popovers: Vec<(String, String)>,
    pub(super) received_focus: Vec<(TabFocusTarget, Option<(String, String)>, bool)>,
    pub(super) keyboard_mode_overrides: Vec<Option<mesh_core_wayland::KeyboardMode>>,
    pub(super) window_states: Vec<mesh_core_wayland::WindowStates>,
    /// Every role this surface was told it had been realized under, in order.
    pub(super) applied_roles: Vec<mesh_core_wayland::SurfaceRole>,
}

pub(super) struct FocusRecordingComponent {
    pub(super) surface_id: String,
    pub(super) state: Arc<Mutex<FocusRecordingState>>,
    pub(super) popover_margin_left: i32,
    pub(super) role: mesh_core_wayland::SurfaceRole,
    pub(super) promotable: bool,
}

impl FocusRecordingComponent {
    pub(super) fn new(surface_id: &str, state: Arc<Mutex<FocusRecordingState>>) -> Self {
        Self {
            surface_id: surface_id.to_string(),
            state,
            popover_margin_left: 0,
            role: mesh_core_wayland::SurfaceRole::Layer,
            promotable: false,
        }
    }

    /// A surface that declared `mesh.surface.promotable`, i.e. one the shell
    /// will move between chrome and a window at runtime.
    pub(super) fn promotable(surface_id: &str, state: Arc<Mutex<FocusRecordingState>>) -> Self {
        Self {
            promotable: true,
            ..Self::new(surface_id, state)
        }
    }

    pub(super) fn with_popover_margin_left(
        surface_id: &str,
        state: Arc<Mutex<FocusRecordingState>>,
        popover_margin_left: i32,
    ) -> Self {
        Self {
            popover_margin_left,
            ..Self::new(surface_id, state)
        }
    }
}

impl super::types::ShellComponent for FocusRecordingComponent {
    fn surface_window_states_changed(&mut self, states: mesh_core_wayland::WindowStates) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.window_states.last() == Some(&states) {
            return false;
        }
        state.window_states.push(states);
        true
    }

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
        _extent: crate::shell::types::SurfaceExtent,
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

    fn surface_role(&self) -> mesh_core_wayland::SurfaceRole {
        self.role
    }

    fn surface_promotable(&self) -> bool {
        self.promotable
    }

    fn surface_role_changed(&mut self, role: mesh_core_wayland::SurfaceRole) -> bool {
        self.role = role;
        self.state.lock().unwrap().applied_roles.push(role);
        true
    }
}

#[derive(Debug, Clone)]
pub(super) struct PopoverHarnessState {
    pub(super) open: bool,
    pub(super) node_key: String,
    pub(super) anchor_rect: (i32, i32, i32, i32),
    pub(super) content_size: (u32, u32),
    /// Reserve around the popover content for descendant shadow/filter
    /// overshoot: (left, top, right, bottom).
    pub(super) content_padding: (u32, u32, u32, u32),
    pub(super) painted_nodes: Vec<String>,
    pub(super) exiting_paints: Vec<bool>,
    pub(super) child_inputs: Vec<(String, ComponentInput)>,
    pub(super) surface_sizes: Vec<(u32, u32)>,
    pub(super) profiling_enabled: Vec<bool>,
    pub(super) hide_transition_ms: u64,
    pub(super) paint_generation: Option<u64>,
    pub(super) present_damage: Option<Vec<mesh_core_render::DamageRect>>,
}

impl Default for PopoverHarnessState {
    fn default() -> Self {
        Self {
            open: true,
            node_key: "root/popover".into(),
            anchor_rect: (8, 10, 40, 16),
            content_size: (72, 32),
            content_padding: (0, 0, 0, 0),
            painted_nodes: Vec::new(),
            exiting_paints: Vec::new(),
            child_inputs: Vec::new(),
            surface_sizes: Vec::new(),
            profiling_enabled: Vec::new(),
            hide_transition_ms: 0,
            paint_generation: None,
            present_damage: None,
        }
    }
}

pub(super) struct PopoverHarnessComponent {
    pub(super) surface_id: String,
    pub(super) state: Arc<Mutex<PopoverHarnessState>>,
}

impl PopoverHarnessComponent {
    pub(super) fn new(state: Arc<Mutex<PopoverHarnessState>>) -> Self {
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
        _extent: crate::shell::types::SurfaceExtent,
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
            content_padding: state.content_padding,
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

    fn child_surface_paint_generation(&self, _node_key: &str) -> Option<u64> {
        self.state.lock().unwrap().paint_generation
    }

    fn child_surface_present_damage(
        &self,
        _node_key: &str,
    ) -> Option<Vec<mesh_core_render::DamageRect>> {
        self.state.lock().unwrap().present_damage.clone()
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
        _content_offset: (f32, f32),
        input: ComponentInput,
    ) -> Result<Vec<super::types::CoreRequest>, super::types::ComponentError> {
        self.state
            .lock()
            .unwrap()
            .child_inputs
            .push((node_key.to_string(), input));
        Ok(Vec::new())
    }

    fn surface_size_changed(&mut self, width: u32, height: u32) -> bool {
        self.state
            .lock()
            .unwrap()
            .surface_sizes
            .push((width, height));
        true
    }

    fn set_profiling_enabled(&mut self, enabled: bool) {
        self.state.lock().unwrap().profiling_enabled.push(enabled);
    }
}

pub(super) struct RecordingClipboard {
    pub(super) writes: Arc<Mutex<Vec<String>>>,
}

#[derive(Debug, Default)]
pub(super) struct TransitionRecordingState {
    pub(super) exiting: Vec<bool>,
}

pub(super) struct TransitionRecordingComponent {
    pub(super) surface_id: String,
    pub(super) hide_transition_ms: u64,
    pub(super) state: Arc<Mutex<TransitionRecordingState>>,
}

impl TransitionRecordingComponent {
    pub(super) fn new(
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
        _extent: crate::shell::types::SurfaceExtent,
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
pub(super) struct InputSizeRecordingState {
    pub(super) sizes: Vec<(u32, u32)>,
}

pub(super) struct InputSizeRecordingComponent {
    pub(super) state: Arc<Mutex<InputSizeRecordingState>>,
    pub(super) content_size: (u32, u32),
}

impl InputSizeRecordingComponent {
    pub(super) fn new(
        state: Arc<Mutex<InputSizeRecordingState>>,
        content_size: (u32, u32),
    ) -> Self {
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
        _extent: crate::shell::types::SurfaceExtent,
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

pub(super) struct PopupGeometryRecordingComponent {
    pub(super) surface_id: String,
    pub(super) declared_size: (u32, u32),
    pub(super) stale_surface_size: (u32, u32),
}

impl PopupGeometryRecordingComponent {
    pub(super) fn new(
        surface_id: &str,
        declared_size: (u32, u32),
        stale_surface_size: (u32, u32),
    ) -> Self {
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
        _extent: crate::shell::types::SurfaceExtent,
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

pub(super) struct MeasuredLayerGeometryComponent {
    pub(super) surface_id: String,
    pub(super) declared_size: (u32, u32),
    pub(super) current_size: (u32, u32),
}

impl MeasuredLayerGeometryComponent {
    pub(super) fn new(
        surface_id: &str,
        declared_size: (u32, u32),
        initial_size: (u32, u32),
    ) -> Self {
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
        _extent: crate::shell::types::SurfaceExtent,
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

pub(super) struct DeadlineTickComponent {
    pub(super) surface_id: String,
    pub(super) deadline: Option<Instant>,
}

impl DeadlineTickComponent {
    pub(super) fn new(surface_id: &str, deadline: Option<Instant>) -> Self {
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
        _extent: crate::shell::types::SurfaceExtent,
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
pub(super) struct DirtyHiddenState {
    pub(super) render_calls: usize,
}

pub(super) struct DirtyHiddenComponent {
    pub(super) surface_id: String,
    pub(super) deadline: Option<Instant>,
    pub(super) state: Arc<Mutex<DirtyHiddenState>>,
}

impl DirtyHiddenComponent {
    pub(super) fn new(
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
        _extent: crate::shell::types::SurfaceExtent,
        _buffer: &mut mesh_core_render::PixelBuffer,
        _scale: f32,
    ) -> Result<(), super::types::ComponentError> {
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), super::types::ComponentError> {
        Ok(())
    }
}

pub(super) fn park_reload_deadlines(shell: &mut Shell) {
    let later = Instant::now() + Duration::from_secs(60);
    shell.next_theme_reload_check = later;
    shell.next_shell_settings_reload_check = later;
    shell.next_frontend_reload_check = later;
}

pub(super) fn manifest_dependencies(path: &Path) -> String {
    let manifest = std::fs::read_to_string(path).expect("read crate manifest");
    manifest_section(&manifest, "[dependencies]")
}

pub(super) fn manifest_section(manifest: &str, section: &str) -> String {
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
