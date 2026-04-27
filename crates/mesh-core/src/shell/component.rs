use super::layout::{
    annotate_overflow_tree, find_click_handler, find_focusable_at, find_node_bounds_by_key,
    find_node_by_key, find_node_path_at, find_scrollable_at, find_tooltip_text_by_key,
    is_input_key, is_slider_key, measure_content_size, namespace_event_handlers, node_tooltip_text,
    parse_namespaced_handler, scroll_limits,
};
use super::service::{apply_service_update, script_events_to_requests, seed_service_state};
use super::surface_layout::{
    SurfaceLayoutSettings, SurfaceSizePolicy, load_frontend_plugin_settings,
};
use super::types::{
    ComponentContext, ComponentError, ComponentInput, CoreEvent, CoreRequest, ServiceEvent,
    ShellComponent,
};
use mesh_capability::{Capability, CapabilitySet};
use mesh_component_backend::{
    CompiledFrontendPlugin, FrontendCompositionResolver, FrontendRenderMode,
    compile_frontend_plugin, root_accessibility_role,
};
use mesh_locale::LocaleEngine;
use mesh_plugin::PluginType;
use mesh_plugin::lifecycle::PluginInstance;
use mesh_plugin::manifest::PluginHost;
use mesh_renderer::{PixelBuffer, SharedTextMeasurer};
use mesh_runtime::protocol::HostRequest;
use mesh_scripting::{LocaleBoundState, ScriptContext};
use mesh_theme::{Theme, default_theme};
use mesh_ui::{ElementState, StyleContext, StyleResolver, VariableStore, WidgetNode};
use mesh_wayland::{Edge, KeyboardMode, Layer, ShellSurface};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock, mpsc as std_mpsc};
use std::time::Duration;

use crate::shell::ShellRunError;

/// Overlays resolved prop values on top of a local component's host state so
/// that template expressions like `{network_name}` resolve to the prop value
/// passed at the call site rather than falling back to the host variable.
struct LocalComponentStore<'a> {
    base: &'a dyn VariableStore,
    props: &'a HashMap<String, String>,
}

impl VariableStore for LocalComponentStore<'_> {
    fn get(&self, name: &str) -> Option<serde_json::Value> {
        if let Some(v) = self.props.get(name) {
            return Some(serde_json::Value::String(v.clone()));
        }
        self.base.get(name)
    }

    fn keys(&self) -> Vec<String> {
        let mut keys = self.base.keys();
        for k in self.props.keys() {
            if !keys.iter().any(|existing| existing == k) {
                keys.push(k.clone());
            }
        }
        keys
    }

    fn translate(&self, key: &str) -> Option<String> {
        self.base.translate(key)
    }
}

#[derive(Debug, Clone)]
pub(super) struct BackendServiceCandidate {
    pub(super) plugin_id: String,
    pub(super) priority: u32,
}

pub(super) struct HostedFrontendComponent {
    plugin_id: String,
    plugin_dir: PathBuf,
    frontend_entry: Option<String>,
    dev_url: Option<String>,
    surface_layout: SurfaceLayoutSettings,
    dev_server: Option<Child>,
    launch_attempted: bool,
}

impl HostedFrontendComponent {
    pub(super) fn new(
        plugin_id: String,
        plugin_dir: PathBuf,
        frontend_entry: Option<String>,
        dev_url: Option<String>,
        surface_layout: SurfaceLayoutSettings,
    ) -> Self {
        Self {
            plugin_id,
            plugin_dir,
            frontend_entry,
            dev_url,
            surface_layout,
            dev_server: None,
            launch_attempted: false,
        }
    }

    fn spawn_hosted_frontend(&mut self) -> Result<(), ComponentError> {
        if self.launch_attempted {
            return Ok(());
        }

        self.launch_attempted = true;
        ensure_hosted_frontend_dependencies(&self.plugin_id, &self.plugin_dir)?;
        let resolved_dev_url =
            resolve_hosted_frontend_url(&self.plugin_dir, self.dev_url.as_deref());

        if let Some((program, args)) = frontend_dev_server_command(&self.plugin_dir) {
            let mut command = Command::new(&program);
            command
                .args(&args)
                .current_dir(&self.plugin_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .env("MESH_PLUGIN_ID", &self.plugin_id);
            if let Some(frontend_entry) = &self.frontend_entry {
                command.env("MESH_FRONTEND_ENTRY", frontend_entry);
            }
            if let Some((host, port)) = resolved_dev_url.as_deref().and_then(parse_http_host_port) {
                command.env("MESH_DEV_HOST", host);
                command.env("MESH_DEV_PORT", port.to_string());
                command.env("PORT", port.to_string());
            }

            let child = command.spawn().map_err(|err| ComponentError::Failed {
                component_id: self.plugin_id.clone(),
                message: format!(
                    "failed to spawn frontend dev server '{}': {} {} ({err})",
                    self.plugin_id,
                    program,
                    args.join(" ")
                ),
            })?;

            tracing::info!(
                "spawned frontend dev server '{}' with command: {} {}",
                self.plugin_id,
                program,
                args.join(" ")
            );
            self.dev_server = Some(child);
        }

        let url = resolved_dev_url.ok_or_else(|| ComponentError::Failed {
            component_id: self.plugin_id.clone(),
            message: format!(
                "no hosted frontend URL or local index found in {}",
                self.plugin_dir.display()
            ),
        })?;

        if url.starts_with("http://") || url.starts_with("https://") {
            wait_for_http_endpoint(&url, Duration::from_secs(10)).map_err(|message| {
                ComponentError::Failed {
                    component_id: self.plugin_id.clone(),
                    message,
                }
            })?;
        }

        spawn_hosted_frontend_window(
            &self.plugin_id,
            &url,
            &self.surface_layout,
            self.surface_layout.visible_on_start,
        )
        .map_err(|message| ComponentError::Failed {
            component_id: self.plugin_id.clone(),
            message,
        })?;

        Ok(())
    }
}

pub(super) struct FrontendSurfaceComponent {
    pub(super) compiled: CompiledFrontendPlugin,
    pub(super) plugin_dir: PathBuf,
    plugin_settings_file: PathBuf,
    settings_json: serde_json::Value,
    pub(super) surface_layout: SurfaceLayoutSettings,
    pub(super) frontend_catalog: FrontendCatalog,
    pub(super) visible: bool,
    dirty: bool,
    last_service_update: Option<String>,
    focused_key: Option<String>,
    pointer_down_key: Option<String>,
    active_slider_key: Option<String>,
    last_audio_slider_percent: Option<u32>,
    input_values: HashMap<String, String>,
    slider_values: HashMap<String, f32>,
    pub(super) scroll_offsets: HashMap<String, ScrollOffsetState>,
    // Hover tracking for CSS :hover and tooltip system.
    hovered_key: Option<String>,
    hovered_path: Vec<String>,
    hovered_pos: (f32, f32),
    hover_start: Option<std::time::Instant>,
    runtimes: Arc<Mutex<HashMap<String, EmbeddedFrontendRuntime>>>,
    render_stack: RefCell<Vec<String>>,
    active_theme: RefCell<Theme>,
    measured_size: Option<(u32, u32)>,
    locale: LocaleEngine,
    interface_catalog: mesh_service::InterfaceCatalog,
    last_tree: Option<WidgetNode>,
    /// Desired visibility for surface portals (`<ImportedSurface hidden={...} />`).
    /// Updated during build_tree; compared to last_surface_states in tick().
    pending_surface_states: RefCell<HashMap<String, bool>>,
    /// Last visibility state emitted for each surface portal, to avoid redundant requests.
    last_surface_states: HashMap<String, bool>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ScrollOffsetState {
    pub(super) x: f32,
    pub(super) y: f32,
}

#[derive(Debug, Clone)]
pub(super) struct FrontendCatalog {
    pub(super) plugins: HashMap<String, FrontendCatalogEntry>,
    slot_contributions: HashMap<String, Vec<ResolvedSlotContribution>>,
}

#[derive(Debug, Clone)]
pub(super) struct FrontendCatalogEntry {
    pub(super) plugin_dir: PathBuf,
    pub(super) compiled: CompiledFrontendPlugin,
}

#[derive(Debug, Clone)]
struct ResolvedSlotContribution {
    source_plugin_id: String,
    widget_id: String,
    contribution_id: String,
    order: i64,
    props: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug)]
struct EmbeddedFrontendRuntime {
    plugin_id: String,
    script_ctx: ScriptContext,
}

impl FrontendCatalog {
    pub(super) fn from_plugins(
        plugins: &HashMap<String, PluginInstance>,
    ) -> Result<Self, ShellRunError> {
        let mut plugin_ids: Vec<String> = plugins.keys().cloned().collect();
        plugin_ids.sort();

        let mut catalog = Self {
            plugins: HashMap::new(),
            slot_contributions: HashMap::new(),
        };

        for plugin_id in plugin_ids {
            let Some(plugin) = plugins.get(&plugin_id) else {
                continue;
            };

            if !mesh_component_backend::is_frontend_plugin(&plugin.manifest) {
                continue;
            }

            if !matches!(plugin.manifest.runtime.host, Some(PluginHost::Tauri)) {
                tracing::info!(
                    "skipping non-tauri frontend plugin '{}'",
                    plugin.manifest.package.id
                );
                continue;
            }

            tracing::info!(
                "tauri frontend plugin '{}' discovered; legacy mesh frontend runtime is disabled",
                plugin.manifest.package.id
            );

            continue;
        }

        for (plugin_id, entry) in &catalog.plugins {
            for (slot_id, contributions) in &entry.compiled.manifest.slot_contributions {
                let bucket = catalog
                    .slot_contributions
                    .entry(slot_id.clone())
                    .or_default();
                for (index, contribution) in contributions.iter().enumerate() {
                    bucket.push(ResolvedSlotContribution {
                        source_plugin_id: plugin_id.clone(),
                        widget_id: contribution
                            .widget
                            .clone()
                            .unwrap_or_else(|| plugin_id.clone()),
                        contribution_id: contribution
                            .id
                            .clone()
                            .unwrap_or_else(|| format!("{plugin_id}:{slot_id}:{index}")),
                        order: contribution.order.unwrap_or(0),
                        props: contribution.props.clone(),
                    });
                }
            }
        }

        for contributions in catalog.slot_contributions.values_mut() {
            contributions.sort_by(|left, right| {
                left.order
                    .cmp(&right.order)
                    .then_with(|| left.widget_id.cmp(&right.widget_id))
                    .then_with(|| left.contribution_id.cmp(&right.contribution_id))
            });
        }

        for (plugin_id, entry) in &catalog.plugins {
            for component_tag in entry.compiled.referenced_component_tags() {
                // Tags that appear in the component's import map are resolved at render time.
                if entry
                    .compiled
                    .component
                    .imports
                    .contains_key(&component_tag)
                {
                    continue;
                }
                catalog
                    .resolve_component_plugin_id(&entry.compiled.manifest, &component_tag)
                    .map_err(|message| ShellRunError::FrontendComposition {
                        message: format!(
                            "plugin '{plugin_id}' cannot resolve <{component_tag}>: {message}"
                        ),
                    })?;
            }
        }

        Ok(catalog)
    }

    fn slot_contributions_for(&self, slot_id: &str) -> &[ResolvedSlotContribution] {
        self.slot_contributions
            .get(slot_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(super) fn top_level_surfaces(&self) -> Vec<FrontendCatalogEntry> {
        let mut entries: Vec<FrontendCatalogEntry> = self
            .plugins
            .values()
            .filter(|entry| entry.compiled.manifest.package.plugin_type == PluginType::Surface)
            .cloned()
            .collect();
        entries.sort_by(|left, right| {
            left.compiled
                .manifest
                .package
                .id
                .cmp(&right.compiled.manifest.package.id)
        });
        entries
    }

    fn resolve_component_plugin_id(
        &self,
        host: &mesh_plugin::Manifest,
        tag: &str,
    ) -> Result<String, String> {
        let mut matches = Vec::new();

        for dependency_id in host.required_plugin_dependencies() {
            let Some(entry) = self.plugins.get(&dependency_id) else {
                continue;
            };

            if entry.compiled.manifest.package.plugin_type != PluginType::Widget {
                continue;
            }

            if entry.compiled.manifest.exported_component_tag() == Some(tag) {
                matches.push(dependency_id);
            }
        }

        match matches.len() {
            1 => Ok(matches.remove(0)),
            0 => Err(format!(
                "no required widget dependency exports that tag; add a plugin dependency whose plugin.json exports.component.tag is '{tag}'"
            )),
            _ => Err(format!(
                "multiple required widget dependencies export '{tag}': {matches:?}"
            )),
        }
    }
}

impl ShellComponent for HostedFrontendComponent {
    fn id(&self) -> &str {
        &self.plugin_id
    }

    fn surface_id(&self) -> &str {
        &self.plugin_id
    }

    fn initial_visibility(&self) -> Option<bool> {
        Some(self.surface_layout.visible_on_start)
    }

    fn mount(&mut self, _ctx: ComponentContext) -> Result<Vec<CoreRequest>, ComponentError> {
        self.spawn_hosted_frontend()?;
        tracing::info!(
            "hosted frontend component '{}' mounted with entry {:?}",
            self.plugin_id,
            self.frontend_entry
        );
        Ok(Vec::new())
    }

    fn handle_core_event(
        &mut self,
        event: &CoreEvent,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        if let CoreEvent::SurfaceVisibilityChanged { surface_id, visible } = event {
            if surface_id == &self.plugin_id {
                set_hosted_frontend_visible(&self.plugin_id, *visible);
            }
        }
        Ok(Vec::new())
    }

    fn handle_service_event(
        &mut self,
        _event: &ServiceEvent,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        Ok(Vec::new())
    }

    fn tick(&mut self) -> Result<Vec<CoreRequest>, ComponentError> {
        Ok(Vec::new())
    }

    fn wants_render(&self) -> bool {
        false
    }

    fn render(&mut self, _surface: &mut dyn ShellSurface) -> Result<(), ComponentError> {
        Ok(())
    }

    fn paint(
        &mut self,
        _theme: &Theme,
        _width: u32,
        _height: u32,
        _buffer: &mut PixelBuffer,
    ) -> Result<(), ComponentError> {
        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), ComponentError> {
        Ok(())
    }

    fn source_path(&self) -> Option<&Path> {
        None
    }

    fn plugin_settings_path(&self) -> Option<&Path> {
        None
    }
}

impl Drop for HostedFrontendComponent {
    fn drop(&mut self) {
        if let Some(child) = &mut self.dev_server {
            let _ = child.kill();
            let _ = child.wait();
        }
        destroy_hosted_frontend_window(&self.plugin_id);
    }
}

fn frontend_dev_server_command(plugin_dir: &Path) -> Option<(String, Vec<String>)> {
    let package_json_path = plugin_dir.join("package.json");
    let package_json = std::fs::read_to_string(&package_json_path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&package_json).ok()?;
    let scripts = value.get("scripts")?.as_object()?;

    let run_script = |name: &str| -> Option<(String, Vec<String>)> {
        if scripts.contains_key(name) {
            Some((
                "pnpm".to_string(),
                vec!["run".to_string(), name.to_string()],
            ))
        } else {
            None
        }
    };

    run_script("dev")
}

fn hosted_frontend_url(plugin_dir: &Path, dev_url: Option<&str>) -> Option<String> {
    if let Some(url) = dev_url.filter(|url| !url.trim().is_empty()) {
        return Some(url.to_string());
    }

    let dist_index = plugin_dir.join("dist").join("index.html");
    if dist_index.exists() {
        return Some(format!("file://{}", dist_index.display()));
    }

    let root_index = plugin_dir.join("index.html");
    if root_index.exists() {
        return Some(format!("file://{}", root_index.display()));
    }

    None
}

fn resolve_hosted_frontend_url(plugin_dir: &Path, dev_url: Option<&str>) -> Option<String> {
    let url = hosted_frontend_url(plugin_dir, dev_url)?;
    Some(resolve_available_http_url(&url).unwrap_or(url))
}

fn resolve_available_http_url(url: &str) -> Option<String> {
    let (host, port) = parse_http_host_port(url)?;
    let available_port = find_available_port(&host, port)?;

    if available_port == port {
        return Some(url.to_string());
    }

    Some(replace_url_port(url, available_port))
}

fn find_available_port(host: &str, preferred_port: u16) -> Option<u16> {
    for port in preferred_port..preferred_port.saturating_add(32) {
        if TcpListener::bind((host, port)).is_ok() {
            return Some(port);
        }
    }

    None
}

fn replace_url_port(url: &str, port: u16) -> String {
    let Some((host, current_port)) = parse_http_host_port(url) else {
        return url.to_string();
    };

    url.replacen(
        &format!("{host}:{current_port}"),
        &format!("{host}:{port}"),
        1,
    )
}

fn wait_for_http_endpoint(url: &str, timeout: Duration) -> Result<(), String> {
    let Some((host, port)) = parse_http_host_port(url) else {
        return Ok(());
    };

    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if std::net::TcpStream::connect((&*host, port)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(150));
    }

    Err(format!(
        "timed out waiting for hosted frontend endpoint {}:{}",
        host, port
    ))
}

fn parse_http_host_port(url: &str) -> Option<(String, u16)> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let authority = rest.split('/').next()?;
    let (host, port) = authority.rsplit_once(':')?;
    Some((host.to_string(), port.parse().ok()?))
}

fn ensure_hosted_frontend_dependencies(
    plugin_id: &str,
    plugin_dir: &Path,
) -> Result<(), ComponentError> {
    if !plugin_dir.join("package.json").exists() || plugin_dir.join("node_modules").exists() {
        return Ok(());
    }

    tracing::info!(
        "installing hosted frontend dependencies for '{}' in {}",
        plugin_id,
        plugin_dir.display()
    );

    let status = Command::new("pnpm")
        .arg("install")
        .current_dir(plugin_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|err| ComponentError::Failed {
            component_id: plugin_id.to_string(),
            message: format!(
                "failed to run 'pnpm install' for hosted frontend in {}: {err}",
                plugin_dir.display()
            ),
        })?;

    if status.success() {
        Ok(())
    } else {
        Err(ComponentError::Failed {
            component_id: plugin_id.to_string(),
            message: format!(
                "'pnpm install' failed for hosted frontend in {} with status {}",
                plugin_dir.display(),
                status
            ),
        })
    }
}

#[cfg(target_os = "linux")]
struct HostedFrontendWindow {
    window: gtk::Window,
    fixed: gtk::Fixed,
    webview: wry::WebView,
    configured_width: u32,
    configured_height: u32,
    edge: mesh_wayland::Edge,
}

#[cfg(target_os = "linux")]
enum HostedFrontendHostCommand {
    Create {
        plugin_id: String,
        url: String,
        surface_layout: SurfaceLayoutSettings,
        visible: bool,
    },
    Destroy {
        plugin_id: String,
    },
    SetVisible {
        plugin_id: String,
        visible: bool,
    },
    ContentSize {
        plugin_id: String,
        width: u32,
        height: u32,
    },
}

#[cfg(target_os = "linux")]
static HOSTED_FRONTEND_HOST: OnceLock<std_mpsc::Sender<HostedFrontendHostCommand>> =
    OnceLock::new();

#[cfg(target_os = "linux")]
fn spawn_hosted_frontend_window(
    plugin_id: &str,
    url: &str,
    surface_layout: &SurfaceLayoutSettings,
    visible: bool,
) -> Result<(), String> {
    hosted_frontend_host_sender()?
        .send(HostedFrontendHostCommand::Create {
            plugin_id: plugin_id.to_string(),
            url: url.to_string(),
            surface_layout: surface_layout.clone(),
            visible,
        })
        .map_err(|err| format!("failed to send hosted frontend create command: {err}"))
}

#[cfg(target_os = "linux")]
fn set_hosted_frontend_visible(plugin_id: &str, visible: bool) {
    if let Some(sender) = HOSTED_FRONTEND_HOST.get() {
        let _ = sender.send(HostedFrontendHostCommand::SetVisible {
            plugin_id: plugin_id.to_string(),
            visible,
        });
    }
}

#[cfg(target_os = "linux")]
fn destroy_hosted_frontend_window(plugin_id: &str) {
    if let Some(sender) = HOSTED_FRONTEND_HOST.get() {
        let _ = sender.send(HostedFrontendHostCommand::Destroy {
            plugin_id: plugin_id.to_string(),
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn spawn_hosted_frontend_window(
    plugin_id: &str,
    _url: &str,
    _surface_layout: &SurfaceLayoutSettings,
    _visible: bool,
) -> Result<(), String> {
    Err(format!(
        "gtk-layer-shell webview host is only implemented on Linux for plugin '{}'",
        plugin_id
    ))
}

#[cfg(not(target_os = "linux"))]
fn set_hosted_frontend_visible(_plugin_id: &str, _visible: bool) {}

#[cfg(not(target_os = "linux"))]
fn destroy_hosted_frontend_window(_plugin_id: &str) {}

#[cfg(target_os = "linux")]
fn hosted_frontend_host_sender()
-> Result<&'static std_mpsc::Sender<HostedFrontendHostCommand>, String> {
    if let Some(sender) = HOSTED_FRONTEND_HOST.get() {
        return Ok(sender);
    }

    let (tx, rx) = std_mpsc::channel();
    std::thread::spawn(move || run_hosted_frontend_host_loop(rx));
    HOSTED_FRONTEND_HOST
        .set(tx)
        .map_err(|_| "hosted frontend host already initialized".to_string())?;
    HOSTED_FRONTEND_HOST
        .get()
        .ok_or_else(|| "hosted frontend host unavailable".to_string())
}

#[cfg(target_os = "linux")]
fn run_hosted_frontend_host_loop(rx: std_mpsc::Receiver<HostedFrontendHostCommand>) {
    use gtk::prelude::*;

    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none()
    {
        tracing::info!("setting WEBKIT_DISABLE_COMPOSITING_MODE=1 for hosted frontends on Wayland");
        unsafe {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }
    }

    if let Err(err) = gtk::init() {
        tracing::error!("gtk init failed for hosted frontend host: {}", err);
        return;
    }
    if !gtk_layer_shell::is_supported() {
        tracing::error!("gtk-layer-shell is not supported by the current Wayland compositor");
        return;
    }

    let windows: Arc<Mutex<HashMap<String, HostedFrontendWindow>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let windows_for_commands = Arc::clone(&windows);

    gtk::glib::timeout_add_local(Duration::from_millis(16), move || {
        while let Ok(command) = rx.try_recv() {
            match command {
                HostedFrontendHostCommand::Create {
                    plugin_id,
                    url,
                    surface_layout,
                    visible,
                } => match build_hosted_frontend_window(&plugin_id, &url, &surface_layout) {
                    Ok(hosted_window) => {
                        if let Some(existing) =
                            windows_for_commands.lock().unwrap().remove(&plugin_id)
                        {
                            existing.window.close();
                        }
                        if visible {
                            hosted_window.window.show_all();
                        }
                        windows_for_commands
                            .lock()
                            .unwrap()
                            .insert(plugin_id, hosted_window);
                    }
                    Err(err) => {
                        tracing::error!("hosted frontend window create failed: {}", err);
                    }
                },
                HostedFrontendHostCommand::SetVisible { plugin_id, visible } => {
                    use gtk::prelude::WidgetExt;
                    let guard = windows_for_commands.lock().unwrap();
                    if let Some(hosted) = guard.get(&plugin_id) {
                        if visible {
                            hosted.window.show_all();
                        } else {
                            hosted.window.hide();
                        }
                    }
                }
                HostedFrontendHostCommand::Destroy { plugin_id } => {
                    if let Some(existing) = windows_for_commands.lock().unwrap().remove(&plugin_id)
                    {
                        existing.window.close();
                    }
                }
                HostedFrontendHostCommand::ContentSize {
                    plugin_id,
                    width,
                    height,
                } => {
                    use gtk::prelude::*;
                    use wry::dpi::{LogicalPosition, LogicalSize};
                    use wry::Rect;

                    let mut guard = windows_for_commands.lock().unwrap();
                    if let Some(hosted) = guard.get_mut(&plugin_id) {
                        // Resize the GTK Fixed container and webview bounds to fit the
                        // reported content. For Top/Bottom edges the compositor controls
                        // width via L+R anchors, so only the height changes; for Left/Right
                        // edges only the width changes.
                        let w = width.max(1);
                        let h = height.max(1);
                        let (new_w, new_h) = match hosted.edge {
                            mesh_wayland::Edge::Top | mesh_wayland::Edge::Bottom => {
                                (hosted.configured_width, h)
                            }
                            mesh_wayland::Edge::Left | mesh_wayland::Edge::Right => {
                                (w, hosted.configured_height)
                            }
                        };
                        hosted.fixed.set_size_request(new_w as i32, new_h as i32);
                        // Update geometry hints so the compositor sees the new size.
                        let (max_w, max_h) = match hosted.edge {
                            mesh_wayland::Edge::Top | mesh_wayland::Edge::Bottom => {
                                (32767i32, new_h as i32)
                            }
                            mesh_wayland::Edge::Left | mesh_wayland::Edge::Right => {
                                (new_w as i32, 32767i32)
                            }
                        };
                        let geometry = gtk::gdk::Geometry::new(
                            new_w as i32,
                            new_h as i32,
                            max_w,
                            max_h,
                            0, 0, 0, 0, 0.0, 0.0,
                            gtk::gdk::Gravity::NorthWest,
                        );
                        hosted.window.set_geometry_hints(
                            None::<&gtk::Widget>,
                            Some(&geometry),
                            gtk::gdk::WindowHints::MIN_SIZE | gtk::gdk::WindowHints::MAX_SIZE,
                        );
                        let _ = hosted.webview.set_bounds(Rect {
                            position: LogicalPosition::new(0, 0).into(),
                            size: LogicalSize::new(new_w, new_h).into(),
                        });
                    }
                }
            }
        }

        gtk::glib::ControlFlow::Continue
    });

    let main_loop = gtk::glib::MainLoop::new(None, false);
    main_loop.run();
}

#[cfg(target_os = "linux")]
fn build_hosted_frontend_window(
    plugin_id: &str,
    url: &str,
    surface_layout: &SurfaceLayoutSettings,
) -> Result<HostedFrontendWindow, String> {
    use gtk::prelude::*;
    use gtk_layer_shell::{Edge as LayerEdge, LayerShell};
    use wry::WebViewBuilderExtUnix;
    use wry::dpi::{LogicalPosition, LogicalSize};
    use wry::{Rect, WebViewBuilder};

    let (resolved_width, resolved_height) = resolve_hosted_frontend_window_size(surface_layout);
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title(plugin_id);
    window.set_default_size(resolved_width as i32, resolved_height as i32);
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_app_paintable(true);

    // Enable RGBA visual so the window background is transparent on Wayland.
    {
        use gtk::prelude::WidgetExt;
        if let Some(screen) = gtk::prelude::WidgetExt::screen(&window) {
            if let Some(visual) = screen.rgba_visual() {
                window.set_visual(Some(&visual));
            }
        }
    }
    window.connect_draw(|_, cr| {
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.0);
        cr.set_operator(gtk::cairo::Operator::Source);
        let _ = cr.paint();
        gtk::glib::Propagation::Proceed
    });

    window.init_layer_shell();
    window.set_namespace("mesh");
    window.set_layer(map_layer_shell_layer(surface_layout.layer));
    apply_layer_shell_anchors(&window, surface_layout.edge);
    window.set_layer_shell_margin(LayerEdge::Top, surface_layout.margin_top);
    window.set_layer_shell_margin(LayerEdge::Right, surface_layout.margin_right);
    window.set_layer_shell_margin(LayerEdge::Bottom, surface_layout.margin_bottom);
    window.set_layer_shell_margin(LayerEdge::Left, surface_layout.margin_left);
    window.set_exclusive_zone(surface_layout.exclusive_zone);
    window.set_keyboard_mode(map_layer_shell_keyboard_mode(surface_layout.keyboard_mode));

    let fixed = gtk::Fixed::new();
    fixed.set_size_request(resolved_width as i32, resolved_height as i32);
    window.add(&fixed);
    fixed.show();
    window.realize();

    let webview = WebViewBuilder::new()
        .with_url(url)
        .with_transparent(true)
        .with_devtools(true)
        .with_initialization_script(mesh_host_init_script())
        .with_bounds(Rect {
            position: LogicalPosition::new(0, 0).into(),
            size: LogicalSize::new(resolved_width, resolved_height).into(),
        })
        .with_ipc_handler({
            let plugin_id = plugin_id.to_string();
            move |request| {
                handle_hosted_frontend_ipc(&plugin_id, request.body());
            }
        })
        .build_gtk(&fixed)
        .map_err(|err| format!("build_gtk failed: {err}"))?;

    // Hard-clamp the GTK window to exactly the configured dimensions.  Without this,
    // WebKit's internal natural size (reported to GTK during layout) can make the layer
    // surface taller than configured.  Setting equal min/max geometry hints forces GTK to
    // allocate exactly resolved_height regardless of child preferred sizes.
    //
    // For Top/Bottom edges the compositor controls width (Left+Right anchors), so we leave
    // max_width unconstrained (32767) to avoid fighting the compositor's allocation.
    // For Left/Right edges the compositor controls height, so max_height is left open.
    {
        let (max_w, max_h) = match surface_layout.edge {
            Edge::Top | Edge::Bottom => (32767, resolved_height as i32),
            Edge::Left | Edge::Right => (resolved_width as i32, 32767),
        };
        let geometry = gtk::gdk::Geometry::new(
            resolved_width as i32,
            resolved_height as i32,
            max_w,
            max_h,
            0,
            0,
            0,
            0,
            0.0,
            0.0,
            gtk::gdk::Gravity::NorthWest,
        );
        window.set_geometry_hints(
            None::<&gtk::Widget>,
            Some(&geometry),
            gtk::gdk::WindowHints::MIN_SIZE | gtk::gdk::WindowHints::MAX_SIZE,
        );
    }

    Ok(HostedFrontendWindow {
        window,
        fixed,
        webview,
        configured_width: resolved_width,
        configured_height: resolved_height,
        edge: surface_layout.edge,
    })
}

#[cfg(target_os = "linux")]
fn resolve_hosted_frontend_window_size(surface_layout: &SurfaceLayoutSettings) -> (u32, u32) {
    use gtk::prelude::MonitorExt;

    let fallback_width = surface_layout.width.max(1);
    let fallback_height = surface_layout.height.max(1);

    let Some(display) = gtk::gdk::Display::default() else {
        return (fallback_width, fallback_height);
    };
    let Some(monitor) = display.primary_monitor() else {
        return (fallback_width, fallback_height);
    };

    let geometry = monitor.geometry();
    let monitor_width = u32::try_from(geometry.width())
        .unwrap_or(fallback_width)
        .max(1);
    let monitor_height = u32::try_from(geometry.height())
        .unwrap_or(fallback_height)
        .max(1);

    let resolved_width = match surface_layout.edge {
        Edge::Top | Edge::Bottom => monitor_width,
        Edge::Left | Edge::Right => fallback_width.min(monitor_width),
    };

    let resolved_height = match surface_layout.edge {
        Edge::Left | Edge::Right => monitor_height,
        Edge::Top | Edge::Bottom => fallback_height.min(monitor_height),
    };

    (resolved_width, resolved_height)
}

#[cfg(target_os = "linux")]
fn mesh_host_init_script() -> &'static str {
    r#"
if (!window.__MESH_CORE__) {
  const listeners = new Set();
  window.__MESH_CORE__ = Object.freeze({
    postMessage(message) {
      window.ipc.postMessage(JSON.stringify(message));
    },
    addEventListener(handler) {
      listeners.add(handler);
      return () => listeners.delete(handler);
    },
  });
  window.__dispatchMeshCoreEvent = (event) => {
    for (const handler of listeners) {
      try {
        handler(event);
      } catch (error) {
        console.error("mesh host event handler failed", error);
      }
    }
  };

  // Report content dimensions so the host can resize the webview window to fit.
  // Use offsetWidth/offsetHeight which exclude absolutely-positioned overflow (e.g.
  // popovers that extend beyond the bar) so the window stays nav-bar-height only.
  const reportContentSize = () => {
    const body = document.body;
    if (!body) return;
    const w = body.offsetWidth;
    const h = body.offsetHeight;
    if (w > 0 && h > 0) {
      window.ipc.postMessage(JSON.stringify({ kind: "content_size", width: w, height: h }));
    }
  };
  const ro = new ResizeObserver(reportContentSize);
  ro.observe(document.body);
}
"#
}

#[cfg(target_os = "linux")]
fn handle_hosted_frontend_ipc(plugin_id: &str, message: &str) {
    // Handle content-size reports sent by the ResizeObserver in the init script.
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(message) {
        if value.get("kind").and_then(|v| v.as_str()) == Some("content_size") {
            let width = value
                .get("width")
                .and_then(|v| v.as_f64())
                .map(|v| v.ceil() as u32)
                .unwrap_or(0);
            let height = value
                .get("height")
                .and_then(|v| v.as_f64())
                .map(|v| v.ceil() as u32)
                .unwrap_or(0);
            if width > 0 && height > 0 {
                if let Some(sender) = HOSTED_FRONTEND_HOST.get() {
                    let _ = sender.send(HostedFrontendHostCommand::ContentSize {
                        plugin_id: plugin_id.to_string(),
                        width,
                        height,
                    });
                }
            }
            return;
        }
    }

    let request = match serde_json::from_str::<HostRequest>(message) {
        Ok(request) => request,
        Err(err) => {
            tracing::warn!("hosted frontend emitted invalid IPC JSON: {}", err);
            return;
        }
    };

    if let Err(err) = dispatch_host_request_via_ipc_socket(request) {
        tracing::warn!("failed to dispatch hosted frontend IPC: {}", err);
    }
}

#[cfg(target_os = "linux")]
fn dispatch_host_request_via_ipc_socket(request: HostRequest) -> Result<(), String> {
    match request {
        HostRequest::InvokeCore { command, payload } => {
            dispatch_shell_command(&command, &payload)?;
        }
        HostRequest::EmitEvent { channel, payload } => {
            dispatch_shell_command(&channel, &payload)?;
        }
        HostRequest::RegisterFrontend { .. }
        | HostRequest::RegisterBackend { .. }
        | HostRequest::RegisterBindable { .. }
        | HostRequest::UpdateBindable { .. }
        | HostRequest::SubscribeBindable { .. }
        | HostRequest::UnsubscribeBindable { .. } => {}
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn dispatch_shell_command(command: &str, payload: &serde_json::Value) -> Result<(), String> {
    let ipc_command = match command {
        "shell.toggle-surface" => payload
            .get("surface_id")
            .and_then(serde_json::Value::as_str)
            .map(|surface_id| format!("shell:toggle_surface:{surface_id}")),
        "shell.show-surface" => payload
            .get("surface_id")
            .and_then(serde_json::Value::as_str)
            .map(|surface_id| format!("shell:show_surface:{surface_id}")),
        "shell.hide-surface" => payload
            .get("surface_id")
            .and_then(serde_json::Value::as_str)
            .map(|surface_id| format!("shell:hide_surface:{surface_id}")),
        "shell.position-surface" => {
            let surface_id = payload
                .get("surface_id")
                .and_then(serde_json::Value::as_str);
            let margin_top = payload
                .get("margin_top")
                .and_then(serde_json::Value::as_i64)
                .and_then(|value| i32::try_from(value).ok());
            let margin_left = payload
                .get("margin_left")
                .and_then(serde_json::Value::as_i64)
                .and_then(|value| i32::try_from(value).ok());

            match (surface_id, margin_top, margin_left) {
                (Some(surface_id), Some(margin_top), Some(margin_left)) => Some(format!(
                    "shell:position_surface:{surface_id}:{margin_top}:{margin_left}"
                )),
                _ => None,
            }
        }
        "shell:debug_overlay" | "shell:debug_cycle_tab" | "shell:shutdown" => {
            Some(command.to_string())
        }
        _ => None,
    };

    if let Some(ipc_command) = ipc_command {
        send_ipc_command(&ipc_command)?;
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn send_ipc_command(command: &str) -> Result<(), String> {
    let socket_path = super::default_ipc_socket_path();
    let mut stream = UnixStream::connect(&socket_path)
        .map_err(|err| format!("connect {}: {err}", socket_path.display()))?;
    stream
        .write_all(format!("{command}\n").as_bytes())
        .map_err(|err| format!("write {}: {err}", socket_path.display()))?;

    let mut response = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut response)
        .map_err(|err| format!("read {}: {err}", socket_path.display()))?;

    if response.starts_with("ok") {
        Ok(())
    } else {
        Err(response.trim().to_string())
    }
}

#[cfg(target_os = "linux")]
fn map_layer_shell_layer(layer: Layer) -> gtk_layer_shell::Layer {
    match layer {
        Layer::Background => gtk_layer_shell::Layer::Background,
        Layer::Bottom => gtk_layer_shell::Layer::Bottom,
        Layer::Top => gtk_layer_shell::Layer::Top,
        Layer::Overlay => gtk_layer_shell::Layer::Overlay,
    }
}

#[cfg(target_os = "linux")]
fn map_layer_shell_keyboard_mode(mode: KeyboardMode) -> gtk_layer_shell::KeyboardMode {
    match mode {
        KeyboardMode::None => gtk_layer_shell::KeyboardMode::None,
        KeyboardMode::Exclusive => gtk_layer_shell::KeyboardMode::Exclusive,
        KeyboardMode::OnDemand => gtk_layer_shell::KeyboardMode::OnDemand,
    }
}

#[cfg(target_os = "linux")]
fn apply_layer_shell_anchors(window: &gtk::Window, edge: Edge) {
    use gtk_layer_shell::{Edge as LayerEdge, LayerShell};

    match edge {
        Edge::Top => {
            window.set_anchor(LayerEdge::Top, true);
            window.set_anchor(LayerEdge::Left, true);
            window.set_anchor(LayerEdge::Right, true);
        }
        Edge::Bottom => {
            window.set_anchor(LayerEdge::Bottom, true);
            window.set_anchor(LayerEdge::Left, true);
            window.set_anchor(LayerEdge::Right, true);
        }
        Edge::Left => {
            window.set_anchor(LayerEdge::Left, true);
            window.set_anchor(LayerEdge::Top, true);
            window.set_anchor(LayerEdge::Bottom, true);
        }
        Edge::Right => {
            window.set_anchor(LayerEdge::Right, true);
            window.set_anchor(LayerEdge::Top, true);
            window.set_anchor(LayerEdge::Bottom, true);
        }
    }
}

impl FrontendSurfaceComponent {
    pub(super) fn new(
        compiled: CompiledFrontendPlugin,
        plugin_dir: PathBuf,
        frontend_catalog: FrontendCatalog,
        interface_catalog: mesh_service::InterfaceCatalog,
    ) -> Self {
        let plugin_settings_file = plugin_dir.join("config/settings.json");
        let settings_state =
            load_frontend_plugin_settings(&plugin_settings_file, &compiled.manifest);
        Self {
            compiled,
            plugin_dir,
            plugin_settings_file,
            settings_json: settings_state.raw,
            surface_layout: settings_state.layout.clone(),
            frontend_catalog,
            visible: settings_state.layout.visible_on_start,
            dirty: true,
            last_service_update: None,
            focused_key: None,
            pointer_down_key: None,
            active_slider_key: None,
            last_audio_slider_percent: None,
            input_values: HashMap::new(),
            slider_values: HashMap::new(),
            scroll_offsets: HashMap::new(),
            hovered_key: None,
            hovered_path: Vec::new(),
            hovered_pos: (0.0, 0.0),
            hover_start: None,
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            render_stack: RefCell::new(Vec::new()),
            active_theme: RefCell::new(default_theme()),
            measured_size: None,
            locale: LocaleEngine::new("en"),
            interface_catalog,
            last_tree: None,
            pending_surface_states: RefCell::new(HashMap::new()),
            last_surface_states: HashMap::new(),
        }
    }

    fn render_layout(&self, surface: &mut dyn ShellSurface) {
        surface.anchor(self.surface_layout.edge);
        surface.set_layer(self.surface_layout.layer);
        let (width, height) = match self.surface_layout.size_policy {
            SurfaceSizePolicy::Fixed => (self.surface_layout.width, self.surface_layout.height),
            SurfaceSizePolicy::ContentMeasured => self
                .measured_size
                .unwrap_or((self.surface_layout.width, self.surface_layout.height)),
        };
        surface.set_size(width, height);
        surface.set_exclusive_zone(self.surface_layout.exclusive_zone);
        surface.set_keyboard_interactivity(self.surface_layout.keyboard_mode);
        surface.set_margin(
            self.surface_layout.margin_top,
            self.surface_layout.margin_right,
            self.surface_layout.margin_bottom,
            self.surface_layout.margin_left,
        );
    }

    // Update host/root runtime with snapshots from imported embedded instances
    // when necessary. This is an internal helper called from tick() to
    // propagate reactive state changes from imported components.
    fn propagate_imported_state(&mut self) {
        // Collect any child runtimes that are import instances and are dirty.
        let mut updates: Vec<(String, serde_json::Value)> = Vec::new();
        {
            let runtimes_ref = self.runtimes.lock().unwrap();
            for (key, runtime) in runtimes_ref.iter() {
                // instance_key pattern: "{host}/import:{alias}"
                if let Some(rest) = key.strip_prefix(&format!("{}/import:", self.id())) {
                    let alias = rest.to_string();
                    let child_state = runtime.script_ctx.state();
                    if child_state.is_dirty() {
                        let mut obj = serde_json::Map::new();
                        for k in child_state.keys() {
                            if let Some(v) = child_state.get(&k) {
                                obj.insert(k.clone(), v);
                            }
                        }
                        let plugin_id = runtime.plugin_id.clone();
                        let alias_obj = serde_json::json!({
                            "plugin_id": plugin_id,
                            "state": serde_json::Value::Object(obj),
                        });
                        updates.push((alias, alias_obj));
                    }
                }
            }
        }

        if !updates.is_empty() {
            if let Some(root_runtime) = self.runtimes.lock().unwrap().get_mut(self.id()) {
                for (alias, value) in updates {
                    root_runtime.script_ctx.state_mut().set(alias, value);
                }
            }
            // Mark surface dirty so it will rebuild and reflect new state.
            self.dirty = true;
        }
    }

    fn build_tree(&mut self, theme: &Theme, width: u32, height: u32) -> WidgetNode {
        self.active_theme.replace(theme.clone());
        // Before building the tree, update the host/root runtime state with
        // snapshots of any imported embedded instances so that imported
        // aliases expose their internal variables via `Alias.state` in Luau
        // templates and scripts.
        // Register proxies on the root runtime so imported components expose
        // their variables directly in the host namespace and also as an
        // alias object. We create closures that forward reads/writes to the
        // child runtime's ScriptContext. Use an Rc clone of the runtimes map
        // so closures can access runtimes without holding borrows on `self`.
        let runtimes_rc = self.runtimes.clone();
        if let Some(root_runtime) = self.runtimes.lock().unwrap().get_mut(self.id()) {
            for (alias, plugin_id) in &self.compiled.component.imports {
                let instance_key = format!("{}/import:{}", self.id(), alias);
                let plugin_id_clone = plugin_id.clone();
                let runtimes_for_closure = runtimes_rc.clone();

                // Getter for the alias object: returns { plugin_id, state = { ... } }
                let instance_key_for_closure = instance_key.clone();
                let alias_getter = Box::new(move || {
                    let runtimes_ref = runtimes_for_closure.lock().unwrap();
                    if let Some(child_runtime) = runtimes_ref.get(&instance_key_for_closure) {
                        let mut obj = serde_json::Map::new();
                        let child_state = child_runtime.script_ctx.state();
                        for k in child_state.keys() {
                            if let Some(v) = child_state.get(&k) {
                                obj.insert(k.clone(), v);
                            }
                        }
                        return serde_json::json!({
                            "plugin_id": plugin_id_clone,
                            "state": serde_json::Value::Object(obj),
                        });
                    }
                    serde_json::json!({ "plugin_id": plugin_id_clone })
                });

                // Register alias proxy (read-only)
                root_runtime.script_ctx.state_mut().register_proxy(
                    alias.clone(),
                    alias_getter,
                    None,
                );

                // If a child instance exists, register per-variable proxies so
                // child variables appear in the root namespace. If a name would
                // collide with an existing root variable, register under
                // "{alias}.{name}" instead.
                //
                // Local-component aliases (plugin_id == self) have no child
                // runtime; skip the re-lock to avoid deadlocking the mutex
                // that is already held by root_runtime above.
                if plugin_id == self.id() {
                    continue;
                }

                let existing_keys: Vec<String> = root_runtime.script_ctx.state().keys();

                let runtimes_ref = self.runtimes.lock().unwrap();
                if let Some(child_runtime) = runtimes_ref.get(&instance_key) {
                    let child_state = child_runtime.script_ctx.state().clone();
                    for key in child_state.keys() {
                        let target_name = if existing_keys.contains(&key) {
                            format!("{}.{}", alias, key)
                        } else {
                            key.clone()
                        };

                        let runtimes_for_get = self.runtimes.clone();
                        let instance_key_get = instance_key.clone();
                        let key_clone = key.clone();
                        // Getter returns the child variable value or Null.
                        let getter = Box::new(move || {
                            let runtimes_ref = runtimes_for_get.lock().unwrap();
                            if let Some(child) = runtimes_ref.get(&instance_key_get) {
                                return child
                                    .script_ctx
                                    .state()
                                    .get(&key_clone)
                                    .unwrap_or(serde_json::Value::Null);
                            }
                            serde_json::Value::Null
                        });

                        // Setter forwards the write into the child runtime state.
                        let runtimes_for_set = self.runtimes.clone();
                        let instance_key_set = instance_key.clone();
                        let key_set = key.clone();
                        let setter = Box::new(move |v: serde_json::Value| {
                            if let Some(child) =
                                runtimes_for_set.lock().unwrap().get_mut(&instance_key_set)
                            {
                                child.script_ctx.state_mut().set(key_set.clone(), v);
                            }
                        });

                        // Only register if not already a proxy; register as proxy
                        // so reads/writes go to the child runtime.
                        if !root_runtime.script_ctx.state().has_proxy(&target_name) {
                            root_runtime.script_ctx.state_mut().register_proxy(
                                target_name,
                                getter,
                                Some(setter),
                            );
                        }
                    }
                }
            }
        }

        let root_state = self.runtime_state(self.id()).unwrap_or_default();
        let bound = LocaleBoundState::new(&root_state, &self.locale);
        {
            let mut stack = self.render_stack.borrow_mut();
            stack.clear();
            stack.push(self.id().to_string());
        }
        let measurer = SharedTextMeasurer;
        let mut tree = self.compiled.build_tree_with_state(
            theme,
            width,
            height,
            Some(&bound),
            FrontendRenderMode::Surface,
            self.id(),
            Some(self),
            Some(&measurer),
        );
        self.render_stack.borrow_mut().clear();
        annotate_runtime_tree(
            &mut tree,
            "root".to_string(),
            &self.focused_key,
            &self.hovered_path,
            &self.pointer_down_key,
            &self.input_values,
            &self.slider_values,
            &self.scroll_offsets,
        );
        annotate_overflow_tree(&mut tree, "root", &mut self.scroll_offsets);

        let rules = self
            .compiled
            .component
            .style
            .as_ref()
            .map(|s| s.rules.as_slice())
            .unwrap_or(&[]);
        let resolver = StyleResolver::new(theme);
        let context = StyleContext {
            container_width: width as f32,
            container_height: height as f32,
        };
        resolver.restyle_subtree(&mut tree, rules, context);

        tree
    }

    fn update_slider_from_position(
        &mut self,
        tree: &WidgetNode,
        slider_key: &str,
        x: f32,
        y: f32,
    ) -> Option<CoreRequest> {
        let Some(node) = find_node_by_key(tree, slider_key) else {
            return None;
        };
        let action = node.attributes.get("mesh-action").cloned();
        let is_vertical = node
            .attributes
            .get("orient")
            .map(|v| v == "vertical")
            .unwrap_or(false);
        let Some((left, top, right, bottom)) = find_node_bounds_by_key(tree, slider_key, 0.0, 0.0)
        else {
            return None;
        };

        let min = node
            .attributes
            .get("min")
            .and_then(|value: &String| value.parse::<f32>().ok())
            .unwrap_or(0.0);
        let max = node
            .attributes
            .get("max")
            .and_then(|value: &String| value.parse::<f32>().ok())
            .unwrap_or(100.0);

        if max <= min {
            return None;
        }

        let pct = if is_vertical {
            // Vertical: top = 100%, bottom = 0% (inverted Y axis).
            let height = (bottom - top).max(1.0);
            let local_y = (y - top).clamp(0.0, height);
            1.0 - (local_y / height).clamp(0.0, 1.0)
        } else {
            let width = (right - left).max(1.0);
            let local_x = (x - left).clamp(0.0, width);
            (local_x / width).clamp(0.0, 1.0)
        };
        let value = min + (max - min) * pct;
        self.slider_values.insert(slider_key.to_string(), value);
        if action.as_deref() == Some("audio-volume") {
            let percent = value.round().clamp(0.0, 100.0) as u32;
            self.update_local_audio_percent(percent);
            if self.last_audio_slider_percent != Some(percent) {
                self.last_audio_slider_percent = Some(percent);
                return Some(CoreRequest::ServiceCommand {
                    interface: "mesh.audio".to_string(),
                    command: "set-volume".to_string(),
                    payload: serde_json::json!({ "percent": percent }),
                });
            }
        }
        None
    }

    fn update_local_audio_percent(&self, percent: u32) {
        let percent = percent.min(100);
        for runtime in self.runtimes.lock().unwrap().values_mut() {
            if !runtime
                .script_ctx
                .capabilities
                .is_granted(&Capability::new("service.audio.read"))
            {
                continue;
            }
            let mut audio = runtime
                .script_ctx
                .state()
                .get("audio")
                .unwrap_or_else(|| serde_json::json!({}));
            if let Some(obj) = audio.as_object_mut() {
                obj.insert("percent".into(), percent.into());
            }
            runtime.script_ctx.state_mut().set("audio", audio);
        }
    }

    fn slider_release_request(&self, tree: &WidgetNode, slider_key: &str) -> Option<CoreRequest> {
        let node = find_node_by_key(tree, slider_key)?;
        match node.attributes.get("mesh-action").map(String::as_str) {
            Some("audio-volume") => {
                let value = self
                    .slider_values
                    .get(slider_key)
                    .copied()
                    .or_else(|| {
                        node.attributes
                            .get("value")
                            .and_then(|value| value.parse::<f32>().ok())
                    })
                    .unwrap_or(0.0);
                let percent = value.round().clamp(0.0, 100.0) as u32;
                Some(CoreRequest::ServiceCommand {
                    interface: "mesh.audio".to_string(),
                    command: "set-volume".to_string(),
                    payload: serde_json::json!({ "percent": percent }),
                })
            }
            _ => None,
        }
    }

    fn runtime_state(&self, instance_key: &str) -> Option<mesh_scripting::ScriptState> {
        self.runtimes
            .lock()
            .unwrap()
            .get(instance_key)
            .map(|runtime| runtime.script_ctx.state().clone())
    }

    /// Load translation files from `config/i18n/{locale}.json` inside the plugin directory.
    fn load_plugin_i18n_from_dir(&mut self, plugin_dir: &Path) {
        let i18n_dir = plugin_dir.join("config/i18n");
        let entries = match std::fs::read_dir(&i18n_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let messages: HashMap<String, String> = match serde_json::from_str(&content) {
                Ok(m) => m,
                Err(_) => {
                    tracing::warn!(
                        "plugin '{}': failed to parse i18n file {}",
                        self.id(),
                        path.display()
                    );
                    continue;
                }
            };
            tracing::debug!(
                "plugin '{}': loaded {} translations for locale '{}'",
                self.id(),
                messages.len(),
                stem
            );
            self.locale.load_translations(mesh_locale::TranslationSet {
                locale: stem.to_string(),
                messages,
            });
        }
    }

    fn load_plugin_i18n(&mut self) {
        let plugin_dir = self.plugin_dir.clone();
        self.load_plugin_i18n_from_dir(&plugin_dir);
    }

    fn load_catalog_i18n(&mut self) {
        let plugin_dirs: Vec<PathBuf> = self
            .frontend_catalog
            .plugins
            .values()
            .map(|entry| entry.plugin_dir.clone())
            .collect();
        for plugin_dir in plugin_dirs {
            self.load_plugin_i18n_from_dir(&plugin_dir);
        }
    }

    fn create_runtime(
        &self,
        compiled: &CompiledFrontendPlugin,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<EmbeddedFrontendRuntime, ComponentError> {
        let component_id = compiled.manifest.package.id.clone();
        let mut script_ctx = ScriptContext::new(
            component_id.clone(),
            grant_capabilities_from_manifest(&compiled.manifest),
        )
        .map_err(|source| ComponentError::Script {
            component_id: component_id.clone(),
            source,
        })?;
        script_ctx.set_interface_catalog(self.interface_catalog.clone());
        seed_service_state(script_ctx.state_mut());

        for (key, value) in props {
            script_ctx.state_mut().set(key.clone(), value.clone());
        }

        // Seed imported plugin aliases into script state so Luau scripts can
        // reference imported components/icons via their alias. The parser
        // already extracted `import` lines to `compiled.component.imports`.
        for (alias, plugin_id) in &compiled.component.imports {
            script_ctx
                .state_mut()
                .set(alias.clone(), serde_json::Value::String(plugin_id.clone()));
        }

        if let Some(script) = &compiled.component.script {
            script_ctx
                .load_script(&script.source)
                .map_err(|source| ComponentError::Script {
                    component_id: component_id.clone(),
                    source,
                })?;
            script_ctx
                .call_init()
                .map_err(|source| ComponentError::Script {
                    component_id: component_id.clone(),
                    source,
                })?;
        }

        Ok(EmbeddedFrontendRuntime {
            plugin_id: component_id,
            script_ctx,
        })
    }

    fn init_root_runtime(&self) -> Result<(), ComponentError> {
        let mut props = HashMap::new();
        props.insert("settings".into(), self.settings_json.clone());
        let runtime = self.create_runtime(&self.compiled, &props)?;
        self.runtimes
            .lock()
            .unwrap()
            .insert(self.id().to_string(), runtime);
        Ok(())
    }

    fn ensure_runtime(
        &self,
        instance_key: &str,
        plugin_id: &str,
        props: &HashMap<String, serde_json::Value>,
    ) -> Result<(), ComponentError> {
        if !self.runtimes.lock().unwrap().contains_key(instance_key) {
            let Some(entry) = self.frontend_catalog.plugins.get(plugin_id) else {
                return Err(ComponentError::Failed {
                    component_id: self.id().to_string(),
                    message: format!("missing embedded frontend plugin '{plugin_id}'"),
                });
            };
            let runtime = self.create_runtime(&entry.compiled, props)?;
            self.runtimes
                .lock()
                .unwrap()
                .insert(instance_key.to_string(), runtime);
            // Register proxies for this imported instance so its variables are
            // available to the host runtime immediately (not only on the next
            // build_tree()). This mirrors the logic used in build_tree.
            self.register_import_proxies(instance_key, plugin_id);
        }

        if let Some(runtime) = self.runtimes.lock().unwrap().get_mut(instance_key) {
            for (key, value) in props {
                runtime
                    .script_ctx
                    .state_mut()
                    .set(key.clone(), value.clone());
            }
        }

        Ok(())
    }

    /// Register proxies on the host/root runtime for an imported instance.
    ///
    /// This creates an alias proxy (alias -> { plugin_id, state }) and per-
    /// variable proxies that forward reads/writes into the child runtime's
    /// ScriptContext so imported variables appear in the same namespace.
    fn register_import_proxies(&self, instance_key: &str, plugin_id: &str) {
        let host_prefix = format!("{}/import:", self.id());
        let Some(alias) = instance_key.strip_prefix(&host_prefix) else {
            // Not an import instance; nothing to do.
            return;
        };

        let runtimes_rc = self.runtimes.clone();
        if let Some(root_runtime) = self.runtimes.lock().unwrap().get_mut(self.id()) {
            let alias = alias.to_string();
            let instance_key_owned = instance_key.to_string();
            let instance_key_for_alias_getter = instance_key_owned.clone();
            let instance_key_for_runtimes_ref = instance_key_owned.clone();
            let plugin_id_clone = plugin_id.to_string();

            // Alias getter returns { plugin_id, state: { ... } } when the
            // child exists, otherwise just { plugin_id }.
            let runtimes_for_alias = runtimes_rc.clone();
            let alias_getter = Box::new(move || {
                let runtimes_ref = runtimes_for_alias.lock().unwrap();
                if let Some(child_runtime) = runtimes_ref.get(&instance_key_for_alias_getter) {
                    let mut obj = serde_json::Map::new();
                    let child_state = child_runtime.script_ctx.state();
                    for k in child_state.keys() {
                        if let Some(v) = child_state.get(&k) {
                            obj.insert(k.clone(), v);
                        }
                    }
                    return serde_json::json!({
                        "plugin_id": plugin_id_clone,
                        "state": serde_json::Value::Object(obj),
                    });
                }
                serde_json::json!({ "plugin_id": plugin_id_clone })
            });

            if !root_runtime.script_ctx.state().has_proxy(&alias) {
                root_runtime.script_ctx.state_mut().register_proxy(
                    alias.clone(),
                    alias_getter,
                    None,
                );
            }

            // If the child runtime exists, register per-variable proxies so
            // child variables appear directly in the host namespace. If a
            // variable name collides with an existing host variable, register
            // it under "{alias}.{name}" instead.
            let existing_keys: Vec<String> = root_runtime.script_ctx.state().keys();
            let runtimes_ref = self.runtimes.lock().unwrap();
            if let Some(child_runtime) = runtimes_ref.get(&instance_key_for_runtimes_ref) {
                let child_state = child_runtime.script_ctx.state().clone();
                for key in child_state.keys() {
                    let target_name = if existing_keys.contains(&key) {
                        format!("{}.{}", alias, key)
                    } else {
                        key.clone()
                    };

                    let runtimes_for_get = self.runtimes.clone();
                    let instance_key_get = instance_key_owned.clone();
                    let key_clone = key.clone();
                    let getter = Box::new(move || {
                        let runtimes_ref = runtimes_for_get.lock().unwrap();
                        if let Some(child) = runtimes_ref.get(&instance_key_get) {
                            return child
                                .script_ctx
                                .state()
                                .get(&key_clone)
                                .unwrap_or(serde_json::Value::Null);
                        }
                        serde_json::Value::Null
                    });

                    let runtimes_for_set = self.runtimes.clone();
                    let instance_key_set = instance_key_owned.clone();
                    let key_set = key.clone();
                    let setter = Box::new(move |v: serde_json::Value| {
                        if let Some(child) =
                            runtimes_for_set.lock().unwrap().get_mut(&instance_key_set)
                        {
                            child.script_ctx.state_mut().set(key_set.clone(), v);
                        }
                    });

                    if !root_runtime.script_ctx.state().has_proxy(&target_name) {
                        root_runtime.script_ctx.state_mut().register_proxy(
                            target_name,
                            getter,
                            Some(setter),
                        );
                    }
                }
            }
        }
    }

    fn build_error_widget(&self, message: impl Into<String>) -> WidgetNode {
        let message = message.into();
        let mut node = WidgetNode::new("box");
        let mut text = WidgetNode::new("text");
        text.attributes.insert("content".into(), message.clone());
        node.attributes.insert("content".into(), message);
        node.children.push(text);
        node
    }

    fn render_embedded_instance(
        &self,
        instance_key: &str,
        plugin_id: &str,
        props: &HashMap<String, serde_json::Value>,
        container_width: f32,
        container_height: f32,
    ) -> WidgetNode {
        if self
            .render_stack
            .borrow()
            .iter()
            .filter(|ancestor| ancestor.as_str() == plugin_id)
            .count()
            >= 2
        {
            return self.build_error_widget(format!("composition cycle blocked for '{plugin_id}'"));
        }

        if let Err(err) = self.ensure_runtime(instance_key, plugin_id, props) {
            return self.build_error_widget(err.to_string());
        }

        let Some(entry) = self.frontend_catalog.plugins.get(plugin_id) else {
            return self.build_error_widget(format!("missing embedded plugin '{plugin_id}'"));
        };

        let state = self.runtime_state(instance_key).unwrap_or_default();
        let bound = LocaleBoundState::new(&state, &self.locale);
        let active_theme = self.active_theme.borrow().clone();
        self.render_stack.borrow_mut().push(plugin_id.to_string());
        let measurer = SharedTextMeasurer;
        let mut tree = entry.compiled.build_tree_with_state(
            &active_theme,
            container_width.max(0.0).ceil() as u32,
            container_height.max(0.0).ceil() as u32,
            Some(&bound),
            FrontendRenderMode::Embedded,
            instance_key,
            Some(self),
            Some(&measurer),
        );
        self.render_stack.borrow_mut().pop();
        namespace_event_handlers(&mut tree, instance_key);
        tree
    }

    fn call_namespaced_handler(
        &mut self,
        handler: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let (instance_key, handler_name, component_id) =
            if let Some((instance_key, handler_name)) = parse_namespaced_handler(handler) {
                let component_id = self
                    .runtimes
                    .lock()
                    .unwrap()
                    .get(instance_key)
                    .map(|runtime| runtime.plugin_id.clone())
                    .unwrap_or_else(|| self.id().to_string());
                (
                    instance_key.to_string(),
                    handler_name.to_string(),
                    component_id,
                )
            } else {
                (
                    self.id().to_string(),
                    handler.to_string(),
                    self.id().to_string(),
                )
            };

        let mut runtimes = self.runtimes.lock().unwrap();
        let Some(runtime) = runtimes.get_mut(&instance_key) else {
            return Ok(Vec::new());
        };
        runtime
            .script_ctx
            .call_handler(&handler_name, args)
            .map_err(|source| ComponentError::Script {
                component_id,
                source,
            })?;
        self.dirty = true;

        Ok(script_events_to_requests(
            runtime.script_ctx.drain_published_events(),
        ))
    }

    fn build_click_event(
        &self,
        tree: &WidgetNode,
        node_key: &str,
        x: f32,
        y: f32,
    ) -> serde_json::Value {
        let target = find_node_by_key(tree, node_key);
        let (left, top, right, bottom) =
            find_node_bounds_by_key(tree, node_key, 0.0, 0.0).unwrap_or((0.0, 0.0, 0.0, 0.0));
        let width = (right - left).max(0.0);
        let height = (bottom - top).max(0.0);
        let bounds = serde_json::json!({
            "left": left,
            "top": top,
            "right": right,
            "bottom": bottom,
            "width": width,
            "height": height,
        });
        let position = serde_json::json!({
            "margin_left": left as i32,
            "margin_top": (bottom - tree.layout.height).max(0.0) as i32,
        });
        let tag = target.map(|node| node.tag.clone()).unwrap_or_default();

        serde_json::json!({
            "type": "click",
            "pointer": {
                "x": x,
                "y": y,
            },
            "surface": {
                "id": self.surface_id(),
                "width": tree.layout.width,
                "height": tree.layout.height,
            },
            "current": {
                "key": node_key,
                "tag": tag,
                "bounds": bounds,
                "position": position,
            },
            "current_target": {
                "key": node_key,
                "tag": tag,
                "bounds": bounds,
                "position": position,
            }
        })
    }
}

impl FrontendCompositionResolver for FrontendSurfaceComponent {
    fn render_import(
        &self,
        host: &mesh_plugin::Manifest,
        host_instance_key: &str,
        alias: &str,
        props: &HashMap<String, String>,
        container_width: f32,
        container_height: f32,
    ) -> Option<WidgetNode> {
        // Check the host component's explicit import map first.
        let plugin_id =
            if let Some(host_entry) = self.frontend_catalog.plugins.get(&host.package.id) {
                if let Some(imported_id) = host_entry.compiled.component.imports.get(alias) {
                    imported_id.clone()
                } else {
                    match self
                        .frontend_catalog
                        .resolve_component_plugin_id(host, alias)
                    {
                        Ok(id) => id,
                        Err(message) => return Some(self.build_error_widget(message)),
                    }
                }
            } else {
                match self
                    .frontend_catalog
                    .resolve_component_plugin_id(host, alias)
                {
                    Ok(id) => id,
                    Err(message) => return Some(self.build_error_widget(message)),
                }
            };

        // Surface plugins are portals: their visibility is tracked via pending_surface_states
        // and translated to ShowSurface/HideSurface requests in tick(). They render nothing inline.
        let is_surface = self
            .frontend_catalog
            .plugins
            .get(&plugin_id)
            .map(|e| e.compiled.manifest.package.plugin_type == PluginType::Surface)
            .unwrap_or(false);
        // If the resolved plugin is the host itself, allow local components
        // shipped under `src/components/<alias>.mesh` to be rendered inline.
        if plugin_id == host.package.id {
            if let Some(entry) = self.frontend_catalog.plugins.get(&host.package.id) {
                if let Some(local) = entry.compiled.local_components.get(alias) {
                    let theme = self.active_theme.borrow().clone();
                    let host_state = self.runtime_state(host_instance_key).unwrap_or_default();
                    let bound = LocaleBoundState::new(&host_state, &self.locale);
                    let store = LocalComponentStore {
                        base: &bound,
                        props,
                    };
                    let host_rules = entry
                        .compiled
                        .component
                        .style
                        .as_ref()
                        .map(|s| s.rules.as_slice())
                        .unwrap_or(&[]);
                    let node = mesh_component_backend::build_widget_tree_from_component(
                        local,
                        host,
                        &theme,
                        container_width,
                        container_height,
                        Some(self),
                        host_instance_key,
                        Some(&store),
                        host_rules,
                    );
                    return Some(node);
                }
            }
        }
        if is_surface {
            let hidden = props
                .get("hidden")
                .map(|v| v == "true" || v == "True")
                .unwrap_or(false);
            self.pending_surface_states
                .borrow_mut()
                .insert(plugin_id, !hidden);
            return Some(WidgetNode::new("box")); // placeholder, takes no space
        }

        let props_json: HashMap<String, serde_json::Value> = props
            .iter()
            .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
            .collect();
        let instance_key = format!("{host_instance_key}/import:{alias}");
        Some(self.render_embedded_instance(
            &instance_key,
            &plugin_id,
            &props_json,
            container_width,
            container_height,
        ))
    }

    fn render_slot(
        &self,
        host: &mesh_plugin::Manifest,
        host_instance_key: &str,
        slot_name: Option<&str>,
        container_width: f32,
        container_height: f32,
    ) -> Vec<WidgetNode> {
        let Some(slot_name) = slot_name else {
            return Vec::new();
        };

        let slot_id = format!("{}:{slot_name}", host.package.id);
        let accepts_widget = host
            .provides_slots
            .get(slot_name)
            .and_then(|definition| definition.accepts.as_deref())
            .map(|accepts| accepts == "widget")
            .unwrap_or(false);

        let mut nodes = Vec::new();
        for contribution in self.frontend_catalog.slot_contributions_for(&slot_id) {
            let Some(entry) = self.frontend_catalog.plugins.get(&contribution.widget_id) else {
                nodes.push(self.build_error_widget(format!(
                    "slot '{slot_id}' references missing plugin '{}'",
                    contribution.widget_id
                )));
                continue;
            };

            if accepts_widget && entry.compiled.manifest.package.plugin_type != PluginType::Widget {
                nodes.push(self.build_error_widget(format!(
                    "slot '{slot_id}' accepts widgets, but '{}' is {}",
                    contribution.widget_id, entry.compiled.manifest.package.plugin_type
                )));
                continue;
            }

            let props_json: HashMap<String, serde_json::Value> = contribution
                .props
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            let instance_key = format!(
                "{host_instance_key}/slot:{slot_name}/{}",
                contribution.contribution_id
            );
            let mut node = self.render_embedded_instance(
                &instance_key,
                &contribution.widget_id,
                &props_json,
                container_width,
                container_height,
            );
            node.attributes.insert(
                "_mesh_slot_source".into(),
                contribution.source_plugin_id.clone(),
            );
            nodes.push(node);
        }

        nodes
    }
}

impl ShellComponent for FrontendSurfaceComponent {
    fn id(&self) -> &str {
        &self.compiled.manifest.package.id
    }

    fn surface_id(&self) -> &str {
        self.compiled.surface_id()
    }

    fn initial_visibility(&self) -> Option<bool> {
        Some(self.surface_layout.visible_on_start)
    }

    fn mount(&mut self, _ctx: ComponentContext) -> Result<Vec<CoreRequest>, ComponentError> {
        self.load_plugin_i18n();
        self.load_catalog_i18n();
        self.init_root_runtime()?;
        self.dirty = true;
        Ok(vec![CoreRequest::PublishDiagnostics {
            message: format!(
                "mounted frontend component '{}' from {}",
                self.id(),
                self.compiled.source_path.display()
            ),
        }])
    }

    fn handle_core_event(&mut self, event: &CoreEvent) -> Result<Vec<CoreRequest>, ComponentError> {
        if let CoreEvent::SurfaceVisibilityChanged {
            surface_id,
            visible,
        } = event
        {
            if surface_id == self.surface_id() {
                self.visible = *visible;
                self.dirty = true;
            }
        }
        Ok(Vec::new())
    }

    fn handle_service_event(
        &mut self,
        event: &ServiceEvent,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let ServiceEvent::Updated {
            service,
            source_plugin,
            payload,
        } = event;
        self.last_service_update = Some(format!("{service}:{source_plugin}"));
        for runtime in self.runtimes.lock().unwrap().values_mut() {
            let service_name = super::service::service_name_from_interface(service);
            let required = format!("service.{service_name}.read");
            let has_read = runtime
                .script_ctx
                .capabilities
                .is_granted(&Capability::new(&required));
            apply_service_update(
                runtime.script_ctx.state_mut(),
                has_read,
                service,
                source_plugin,
                payload.clone(),
            );
            if has_read {
                runtime
                    .script_ctx
                    .apply_service_bindings(&service_name, &payload);
                let _ = runtime.script_ctx.call_service_handlers(&service_name);
            }
        }
        self.dirty = true;
        Ok(Vec::new())
    }

    fn tick(&mut self) -> Result<Vec<CoreRequest>, ComponentError> {
        // Trigger a repaint once the tooltip delay has elapsed so the tooltip appears.
        if let Some(start) = self.hover_start {
            if start.elapsed() >= Duration::from_millis(500) && !self.dirty {
                self.dirty = true;
            }
        }

        // Emit Show/HideSurface requests for surface portals whose desired visibility changed.
        let pending = std::mem::take(&mut *self.pending_surface_states.borrow_mut());
        let mut requests = Vec::new();
        for (surface_id, visible) in pending {
            let was_visible = self.last_surface_states.get(&surface_id).copied();
            if was_visible != Some(visible) {
                self.last_surface_states.insert(surface_id.clone(), visible);
                if visible {
                    requests.push(CoreRequest::ShowSurface { surface_id });
                } else {
                    requests.push(CoreRequest::HideSurface { surface_id });
                }
            }
        }
        // Propagate any imported child state updates into the host runtime.
        self.propagate_imported_state();

        Ok(requests)
    }

    fn wants_render(&self) -> bool {
        self.dirty
    }

    fn render(&mut self, surface: &mut dyn ShellSurface) -> Result<(), ComponentError> {
        self.render_layout(surface);

        if self.visible {
            surface.show();
        } else {
            surface.hide();
        }

        let template_nodes = self
            .compiled
            .component
            .template
            .as_ref()
            .map(|template| template.root.len())
            .unwrap_or(0);
        let role = root_accessibility_role(&self.compiled.manifest, &self.compiled.component)
            .unwrap_or_else(|| "unknown".into());

        tracing::debug!(
            "rendered frontend '{}' visible={} nodes={} role={}{}",
            self.id(),
            self.visible,
            template_nodes,
            role,
            self.last_service_update
                .as_deref()
                .map(|summary| format!(" service={summary}"))
                .unwrap_or_default()
        );

        self.dirty = false;
        Ok(())
    }

    fn paint(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        buffer: &mut PixelBuffer,
    ) -> Result<(), ComponentError> {
        let tree = self.build_tree(theme, width, height);
        if self.surface_layout.size_policy == SurfaceSizePolicy::ContentMeasured {
            let surface_layout_manifest = self.compiled.manifest.surface_layout.as_ref();
            let measured_size = measure_content_size(&tree, width, height, surface_layout_manifest);
            if self.measured_size != Some(measured_size) {
                self.measured_size = Some(measured_size);
                self.dirty = true;
            }
        }
        buffer.clear(tree.computed_style.background_color);

        let tooltip = if let (Some(start), Some(hovered_key)) =
            (self.hover_start, self.hovered_key.as_ref())
        {
            if start.elapsed() >= Duration::from_millis(500) {
                find_tooltip_text_by_key(&tree, hovered_key).map(|text| {
                    let (cx, cy) = self.hovered_pos;
                    (text, cx, cy)
                })
            } else {
                None
            }
        } else {
            None
        };

        super::FRONTEND_PAINTER.with(|painter| {
            let painter = painter.borrow();
            painter.paint(&tree, buffer, 1.0);
            if let Some((tooltip_text, cx, cy)) = tooltip {
                painter.paint_tooltip(&tooltip_text, cx, cy, buffer, 1.0);
            }
        });
        self.last_tree = Some(tree);

        Ok(())
    }

    fn theme_changed(&mut self) -> Result<(), ComponentError> {
        self.dirty = true;
        Ok(())
    }

    fn locale_changed(&mut self, locale: &LocaleEngine) -> Result<(), ComponentError> {
        self.locale.set_locale(locale.current());
        self.runtimes.lock().unwrap().clear();
        self.init_root_runtime()?;
        self.dirty = true;
        Ok(())
    }

    fn source_path(&self) -> Option<&Path> {
        Some(self.compiled.source_path.as_path())
    }

    fn plugin_settings_path(&self) -> Option<&Path> {
        if self.plugin_settings_file.exists() {
            Some(self.plugin_settings_file.as_path())
        } else {
            None
        }
    }

    fn reload_plugin_settings(&mut self) -> Result<bool, ComponentError> {
        let settings_state =
            load_frontend_plugin_settings(&self.plugin_settings_file, &self.compiled.manifest);
        let layout_changed = self.surface_layout != settings_state.layout;
        let settings_changed = self.settings_json != settings_state.raw;

        self.surface_layout = settings_state.layout;
        self.settings_json = settings_state.raw;

        if settings_changed {
            if let Some(runtime) = self.runtimes.lock().unwrap().get_mut(self.id()) {
                runtime
                    .script_ctx
                    .state_mut()
                    .set("settings", self.settings_json.clone());
            }
        }

        let Some(locale) = self
            .settings_json
            .get("i18n")
            .and_then(|i18n| i18n.get("default_locale"))
            .and_then(|l| l.as_str())
        else {
            if layout_changed || settings_changed {
                self.dirty = true;
            }
            return Ok(layout_changed || settings_changed);
        };

        if self.locale.current() != locale {
            tracing::info!(
                "plugin '{}': applying locale '{}' from plugin settings",
                self.id(),
                locale
            );
            self.locale.set_locale(locale);
        }

        if layout_changed || settings_changed {
            self.dirty = true;
        }
        Ok(layout_changed || settings_changed)
    }

    fn reload_source(&mut self) -> Result<bool, ComponentError> {
        let manifest = self.compiled.manifest.clone();
        let recompiled = compile_frontend_plugin(&manifest, &self.plugin_dir).map_err(|err| {
            ComponentError::Failed {
                component_id: self.id().to_string(),
                message: format!("frontend recompile failed: {err}"),
            }
        })?;

        let component_id = self.id().to_string();
        self.compiled = recompiled;
        if let Some(entry) = self.frontend_catalog.plugins.get_mut(&component_id) {
            entry.compiled = self.compiled.clone();
        }
        self.runtimes.lock().unwrap().clear();
        self.init_root_runtime()?;
        self.dirty = true;
        Ok(true)
    }

    fn handle_input(
        &mut self,
        theme: &Theme,
        width: u32,
        height: u32,
        input: ComponentInput,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        tracing::trace!(
            "[hover] handle_input called: id={} visible={} input={:?}",
            self.id(),
            self.visible,
            std::mem::discriminant(&input)
        );
        if !self.visible {
            return Ok(Vec::new());
        }

        let tree = self
            .last_tree
            .clone()
            .unwrap_or_else(|| self.build_tree(theme, width, height));

        match input {
            ComponentInput::PointerButton { x, y, pressed } => {
                if pressed {
                    if let Some(node_key) = find_focusable_at(&tree, x, y) {
                        self.focused_key = Some(node_key.clone());
                        self.pointer_down_key = Some(node_key.clone());

                        if is_slider_key(&tree, &node_key) {
                            self.active_slider_key = Some(node_key.clone());
                            self.last_audio_slider_percent = None;
                            if let Some(request) =
                                self.update_slider_from_position(&tree, &node_key, x, y)
                            {
                                self.dirty = true;
                                return Ok(vec![request]);
                            }
                        } else {
                            self.active_slider_key = None;
                            self.last_audio_slider_percent = None;
                        }

                        self.dirty = true;
                    } else {
                        self.focused_key = None;
                        self.pointer_down_key = None;
                        self.active_slider_key = None;
                        self.last_audio_slider_percent = None;
                        self.dirty = true;
                    }
                } else {
                    let slider_request = self
                        .active_slider_key
                        .as_ref()
                        .and_then(|slider_key| self.slider_release_request(&tree, slider_key));
                    if let Some(node_key) = find_focusable_at(&tree, x, y) {
                        if self.pointer_down_key.as_deref() == Some(node_key.as_str()) {
                            if let Some(handler) = find_click_handler(&tree, &node_key) {
                                let click_event = self.build_click_event(&tree, &node_key, x, y);
                                return Ok(self.call_namespaced_handler(&handler, &[click_event])?);
                            }
                        }
                    }
                    self.pointer_down_key = None;
                    self.active_slider_key = None;
                    self.last_audio_slider_percent = None;
                    if let Some(request) = slider_request {
                        self.dirty = true;
                        return Ok(vec![request]);
                    }
                }
            }
            ComponentInput::PointerMove { x, y } => {
                if let Some(slider_key) = self.active_slider_key.clone() {
                    let request = self.update_slider_from_position(&tree, &slider_key, x, y);
                    self.dirty = true;
                    if let Some(request) = request {
                        return Ok(vec![request]);
                    }
                }

                // Update hover state for CSS :hover and the tooltip system.
                self.hovered_pos = (x, y);
                let new_path = find_node_path_at(&tree, x, y).unwrap_or_default();
                let new_key = new_path.last().cloned();
                tracing::trace!(
                    "[hover] pointer=({x:.1},{y:.1}) path={:?} hit={:?} prev={:?}",
                    new_path,
                    new_key,
                    self.hovered_key
                );
                if new_key != self.hovered_key || new_path != self.hovered_path {
                    self.hovered_key = new_key.clone();
                    self.hovered_path = new_path;
                    // Only start the tooltip timer when hovering a node with tooltip content.
                    self.hover_start = new_key
                        .as_ref()
                        .and_then(|k| find_node_by_key(&tree, k))
                        .and_then(|n| node_tooltip_text(n))
                        .map(|_| std::time::Instant::now());
                    self.dirty = true;
                }
            }
            ComponentInput::Scroll { x, y, dx, dy } => {
                if let Some(scroll_key) = find_scrollable_at(&tree, x, y) {
                    if let Some(node) = find_node_by_key(&tree, &scroll_key) {
                        let (max_x, max_y) = scroll_limits(node);
                        let current = self.scroll_offsets.entry(scroll_key).or_default();
                        let next_x = (current.x - dx * 28.0).clamp(0.0, max_x);
                        let next_y = (current.y - dy * 28.0).clamp(0.0, max_y);
                        if (next_x - current.x).abs() > f32::EPSILON
                            || (next_y - current.y).abs() > f32::EPSILON
                        {
                            current.x = next_x;
                            current.y = next_y;
                            self.dirty = true;
                        }
                    }
                }
            }
            ComponentInput::Char { ch } => {
                if let Some(focused_key) = self.focused_key.clone() {
                    if is_input_key(&tree, &focused_key) && !ch.is_control() {
                        self.input_values.entry(focused_key).or_default().push(ch);
                        self.dirty = true;
                    }
                }
            }
            ComponentInput::KeyPressed { key } => {
                if let Some(focused_key) = self.focused_key.clone() {
                    if is_input_key(&tree, &focused_key) {
                        let value = self.input_values.entry(focused_key).or_default();
                        match key.as_str() {
                            "Backspace" => {
                                value.pop();
                                self.dirty = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
            ComponentInput::KeyReleased { .. } => {}
        }

        Ok(Vec::new())
    }

    fn last_widget_tree(&self) -> Option<&WidgetNode> {
        self.last_tree.as_ref()
    }

    fn apply_position(&mut self, margin_top: i32, margin_left: i32) {
        self.surface_layout.edge = Edge::Left;
        self.surface_layout.margin_top = margin_top;
        self.surface_layout.margin_left = margin_left;
        self.dirty = true;
    }
}

pub(super) fn annotate_runtime_tree(
    node: &mut WidgetNode,
    key: String,
    focused_key: &Option<String>,
    hovered_path: &[String],
    active_key: &Option<String>,
    input_values: &HashMap<String, String>,
    slider_values: &HashMap<String, f32>,
    scroll_offsets: &HashMap<String, ScrollOffsetState>,
) {
    node.attributes.insert("_mesh_key".into(), key.clone());

    let key_str = key.as_str();
    node.state = ElementState {
        focused: focused_key.as_deref() == Some(key_str),
        hovered: hovered_path
            .iter()
            .any(|hovered_key| hovered_key == key_str),
        active: active_key.as_deref() == Some(key_str),
        disabled: false,
        checked: false,
    };
    if node.state.hovered {
        tracing::trace!(
            "[hover] annotate: key={key} tag={} set hovered=true",
            node.tag
        );
    }

    if node.state.focused {
        node.attributes
            .insert("_mesh_focused".into(), "true".into());
    }

    match node.tag.as_str() {
        "input" => {
            let value = input_values
                .get(&key)
                .cloned()
                .or_else(|| node.attributes.get("value").cloned())
                .unwrap_or_default();
            node.attributes.insert("value".into(), value);
        }
        "slider" => {
            let value = slider_values
                .get(&key)
                .copied()
                .or_else(|| {
                    node.attributes
                        .get("value")
                        .and_then(|value: &String| value.parse::<f32>().ok())
                })
                .unwrap_or(50.0);
            node.attributes
                .insert("value".into(), format!("{value:.2}"));
        }
        _ => {}
    }

    let offset = scroll_offsets.get(&key).copied().unwrap_or_default();
    node.attributes
        .insert("_mesh_scroll_x".into(), format!("{:.2}", offset.x));
    node.attributes
        .insert("_mesh_scroll_y".into(), format!("{:.2}", offset.y));

    for (index, child) in node.children.iter_mut().enumerate() {
        annotate_runtime_tree(
            child,
            format!("{key}/{index}"),
            focused_key,
            hovered_path,
            active_key,
            input_values,
            slider_values,
            scroll_offsets,
        );
    }
}

pub(super) fn grant_capabilities_from_manifest(manifest: &mesh_plugin::Manifest) -> CapabilitySet {
    let mut granted = CapabilitySet::new();

    for capability in &manifest.capabilities.required {
        granted.grant(Capability::new(capability.clone()));
    }

    for capability in &manifest.capabilities.optional {
        granted.grant(Capability::new(capability.clone()));
    }

    granted
}
