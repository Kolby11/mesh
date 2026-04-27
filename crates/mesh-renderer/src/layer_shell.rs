//! wlr-layer-shell backend.
//!
//! Replaces `dev_window` (minifb XDG-toplevel) with real layer-shell surfaces so
//! panels/launchers/overlays are placed by the compositor as shell chrome
//! instead of being tiled as windows.

use crate::dev_window::{DevWindowEvent, DevWindowKeyEvent};
use crate::{PixelBuffer, RenderError};
use mesh_wayland::{Edge, KeyboardMode, Layer as MeshLayer};
use rustix::event::{PollFd, PollFlags, poll};

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability as SeatCapability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
    shm::{Shm, ShmHandler, slot::SlotPool},
};
use std::collections::HashMap;
use std::io::ErrorKind;
use wayland_client::{
    Connection, EventQueue, QueueHandle,
    backend::WaylandError,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface},
};

/// Configuration passed from the shell before each present.
#[derive(Debug, Clone)]
pub struct LayerSurfaceConfig {
    pub edge: Option<Edge>,
    pub layer: MeshLayer,
    pub width: u32,
    pub height: u32,
    pub exclusive_zone: i32,
    pub keyboard_mode: KeyboardMode,
    pub namespace: String,
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
}

impl Default for LayerSurfaceConfig {
    fn default() -> Self {
        Self {
            edge: Some(Edge::Top),
            layer: MeshLayer::Top,
            width: 0,
            height: 0,
            exclusive_zone: 0,
            keyboard_mode: KeyboardMode::None,
            namespace: "mesh".to_string(),
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
        }
    }
}

pub struct LayerShellBackend {
    _conn: Connection,
    event_queue: EventQueue<State>,
    state: State,
}

struct State {
    registry_state: RegistryState,
    output_state: OutputState,
    compositor_state: CompositorState,
    shm: Shm,
    layer_shell: LayerShell,
    seat_state: SeatState,

    qh: QueueHandle<State>,
    pool: Option<SlotPool>,

    surfaces: HashMap<String, SurfaceEntry>,

    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer_focus: Option<String>,
    keyboard_focus: Option<String>,
    keyboard_mods: Modifiers,

    events: Vec<DevWindowEvent>,
}

struct SurfaceEntry {
    layer_surface: LayerSurface,
    cfg: LayerSurfaceConfig,
    width: u32,
    height: u32,
    configured: bool,
}

impl LayerShellBackend {
    pub fn new() -> Result<Self, RenderError> {
        let conn = Connection::connect_to_env()
            .map_err(|e| RenderError::WaylandConnect(format!("connect_to_env: {e}")))?;
        let (globals, event_queue) = registry_queue_init::<State>(&conn)
            .map_err(|e| RenderError::WaylandConnect(format!("registry_queue_init: {e}")))?;
        let qh = event_queue.handle();

        let registry_state = RegistryState::new(&globals);
        let output_state = OutputState::new(&globals, &qh);
        let compositor_state = CompositorState::bind(&globals, &qh)
            .map_err(|e| RenderError::ProtocolUnsupported(format!("wl_compositor: {e}")))?;
        let shm = Shm::bind(&globals, &qh)
            .map_err(|e| RenderError::ProtocolUnsupported(format!("wl_shm: {e}")))?;
        let layer_shell = LayerShell::bind(&globals, &qh)
            .map_err(|e| RenderError::ProtocolUnsupported(format!("zwlr_layer_shell_v1: {e}")))?;
        let seat_state = SeatState::new(&globals, &qh);

        let pool = SlotPool::new(256 * 256 * 4, &shm).ok();

        let state = State {
            registry_state,
            output_state,
            compositor_state,
            shm,
            layer_shell,
            seat_state,
            qh,
            pool,
            surfaces: HashMap::new(),
            pointer: None,
            keyboard: None,
            pointer_focus: None,
            keyboard_focus: None,
            keyboard_mods: Modifiers::default(),
            events: Vec::new(),
        };

        Ok(Self {
            _conn: conn,
            event_queue,
            state,
        })
    }

    /// Apply a surface's desired config. Creates the layer surface lazily on first call.
    pub fn configure(&mut self, surface_id: &str, cfg: LayerSurfaceConfig) {
        let qh = self.state.qh.clone();
        let entry = self.state.surfaces.get_mut(surface_id);
        match entry {
            Some(entry) => {
                let config_changed = entry.cfg.edge != cfg.edge
                    || entry.cfg.layer != cfg.layer
                    || entry.cfg.exclusive_zone != cfg.exclusive_zone
                    || entry.cfg.keyboard_mode != cfg.keyboard_mode
                    || entry.cfg.width != cfg.width
                    || entry.cfg.height != cfg.height
                    || entry.cfg.margin_top != cfg.margin_top
                    || entry.cfg.margin_right != cfg.margin_right
                    || entry.cfg.margin_bottom != cfg.margin_bottom
                    || entry.cfg.margin_left != cfg.margin_left;
                if config_changed || !entry.configured {
                    // Re-commit to re-map the surface and prompt the compositor to
                    // send a fresh configure event before we attach a buffer.
                    apply_config(&entry.layer_surface, &cfg);
                    entry.layer_surface.commit();
                    entry.cfg = cfg;
                }
            }
            None => {
                let wl_surface = self.state.compositor_state.create_surface(&qh);
                let layer_surface = self.state.layer_shell.create_layer_surface(
                    &qh,
                    wl_surface,
                    map_layer(cfg.layer),
                    Some(cfg.namespace.clone()),
                    None,
                );
                apply_config(&layer_surface, &cfg);
                layer_surface.commit();
                self.state.surfaces.insert(
                    surface_id.to_string(),
                    SurfaceEntry {
                        layer_surface,
                        width: cfg.width.max(1),
                        height: cfg.height.max(1),
                        cfg,
                        configured: false,
                    },
                );
            }
        }
    }

    pub fn present(
        &mut self,
        surface_id: &str,
        _title: &str,
        visible: bool,
        buffer: &PixelBuffer,
    ) -> Result<(), RenderError> {
        if !visible {
            // Only detach a buffer (to hide) if the compositor has already configured this
            // surface. Before the first configure event the surface has no buffer attached
            // and is already invisible; committing a null buffer before configure arrives
            // triggers a Wayland protocol error.
            if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
                if entry.configured {
                    let wl_surface = entry.layer_surface.wl_surface();
                    wl_surface.attach(None, 0, 0);
                    entry.layer_surface.commit();
                    // Reset configured so that on the next show we wait for a fresh
                    // configure event before attaching a buffer. Some compositors send
                    // a new configure when a surface is re-mapped after a null commit.
                    entry.configured = false;
                }
            }
            self.dispatch_pending()?;
            return Ok(());
        }

        let Some(entry) = self.state.surfaces.get_mut(surface_id) else {
            // present() called before configure() — nothing to do.
            return Ok(());
        };

        if !entry.configured {
            // Roundtrip until we get our first configure.
            for _ in 0..10 {
                self.event_queue
                    .roundtrip(&mut self.state)
                    .map_err(|e| RenderError::SurfaceCreate(format!("roundtrip: {e}")))?;
                if self
                    .state
                    .surfaces
                    .get(surface_id)
                    .map(|e| e.configured)
                    .unwrap_or(false)
                {
                    break;
                }
            }
        }

        let Some(entry) = self.state.surfaces.get_mut(surface_id) else {
            return Ok(());
        };
        if !entry.configured {
            return Ok(());
        }

        let width = buffer.width.max(1);
        let height = buffer.height.max(1);
        let stride = width as i32 * 4;

        let pool = self
            .state
            .pool
            .as_mut()
            .ok_or_else(|| RenderError::BufferAlloc("shm pool not initialised".into()))?;

        let (wl_buffer, canvas) = pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .map_err(|e| RenderError::BufferAlloc(format!("create_buffer: {e}")))?;

        // Copy BGRA → ARGB8888 little-endian (wl_shm Argb8888 is B,G,R,A in memory).
        let src = &buffer.data;
        let len = (width as usize) * (height as usize) * 4;
        if canvas.len() >= len && src.len() >= len {
            canvas[..len].copy_from_slice(&src[..len]);
        }

        let wl_surface = entry.layer_surface.wl_surface();
        wl_surface.damage_buffer(0, 0, width as i32, height as i32);
        wl_buffer.attach_to(wl_surface).ok();
        entry.layer_surface.commit();
        entry.width = width;
        entry.height = height;

        self.dispatch_pending()?;
        Ok(())
    }

    pub fn pump(&mut self) {
        let _ = self.dispatch_available();
    }

    pub fn poll_events(&mut self) -> Vec<DevWindowEvent> {
        let _ = self.dispatch_available();
        let events = std::mem::take(&mut self.state.events);
        if !events.is_empty() {
            tracing::trace!(
                "[hover] layer_shell: draining {} input event(s)",
                events.len()
            );
        }
        events
    }

    fn dispatch_pending(&mut self) -> Result<(), RenderError> {
        self.event_queue
            .flush()
            .map_err(|e| RenderError::SurfaceCreate(format!("flush: {e}")))?;
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| RenderError::SurfaceCreate(format!("dispatch: {e}")))?;
        Ok(())
    }

    fn dispatch_available(&mut self) -> Result<(), RenderError> {
        self.event_queue
            .flush()
            .map_err(|e| RenderError::SurfaceCreate(format!("flush: {e}")))?;

        for _ in 0..32 {
            self.event_queue
                .dispatch_pending(&mut self.state)
                .map_err(|e| RenderError::SurfaceCreate(format!("dispatch: {e}")))?;

            let Some(read_guard) = self.event_queue.prepare_read() else {
                continue;
            };

            let poll_result = {
                let fd = read_guard.connection_fd();
                let mut fds = [PollFd::new(
                    &fd,
                    PollFlags::IN | PollFlags::ERR | PollFlags::HUP,
                )];
                poll(&mut fds, 0).map(|ready| {
                    if ready == 0 {
                        None
                    } else {
                        Some(fds[0].revents())
                    }
                })
            };

            match poll_result {
                Ok(None) => {
                    drop(read_guard);
                    break;
                }
                Ok(Some(revents)) => {
                    if !revents.intersects(PollFlags::IN | PollFlags::ERR | PollFlags::HUP) {
                        drop(read_guard);
                        break;
                    }

                    match read_guard.read() {
                        Ok(read_count) => {
                            tracing::trace!("read {read_count} Wayland event(s)");
                            if read_count == 0 {
                                break;
                            }
                        }
                        Err(WaylandError::Io(err)) if err.kind() == ErrorKind::WouldBlock => break,
                        Err(err) => {
                            return Err(RenderError::SurfaceCreate(format!("read: {err}")));
                        }
                    }
                }
                Err(rustix::io::Errno::INTR) => {
                    drop(read_guard);
                    break;
                }
                Err(err) => {
                    drop(read_guard);
                    return Err(RenderError::SurfaceCreate(format!("poll: {err}")));
                }
            }
        }

        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| RenderError::SurfaceCreate(format!("dispatch: {e}")))?;
        Ok(())
    }
}

fn apply_config(layer_surface: &LayerSurface, cfg: &LayerSurfaceConfig) {
    layer_surface.set_layer(map_layer(cfg.layer));
    layer_surface.set_anchor(map_anchor(cfg.edge));
    layer_surface.set_exclusive_zone(cfg.exclusive_zone);
    layer_surface.set_keyboard_interactivity(map_keyboard(cfg.keyboard_mode));
    layer_surface.set_size(cfg.width, cfg.height);
    layer_surface.set_margin(
        cfg.margin_top,
        cfg.margin_right,
        cfg.margin_bottom,
        cfg.margin_left,
    );
}

fn map_layer(layer: MeshLayer) -> Layer {
    match layer {
        MeshLayer::Background => Layer::Background,
        MeshLayer::Bottom => Layer::Bottom,
        MeshLayer::Top => Layer::Top,
        MeshLayer::Overlay => Layer::Overlay,
    }
}

fn map_anchor(edge: Option<Edge>) -> Anchor {
    match edge {
        // Treat a single edge as a normal shell placement, not a centered popup.
        // Top/bottom bars stretch across the output width, and left/right rails
        // pin to the top corner instead of floating in the vertical center.
        Some(Edge::Top) => Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
        Some(Edge::Bottom) => Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        Some(Edge::Left) => Anchor::TOP | Anchor::LEFT,
        Some(Edge::Right) => Anchor::TOP | Anchor::RIGHT,
        None => Anchor::empty(),
    }
}

fn map_keyboard(mode: KeyboardMode) -> KeyboardInteractivity {
    match mode {
        KeyboardMode::None => KeyboardInteractivity::None,
        KeyboardMode::Exclusive => KeyboardInteractivity::Exclusive,
        KeyboardMode::OnDemand => KeyboardInteractivity::OnDemand,
    }
}

// --- Handler impls ----------------------------------------------------------

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _o: wl_output::WlOutput) {}
    fn update_output(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _o: wl_output::WlOutput) {}
    fn output_destroyed(
        &mut self,
        _c: &Connection,
        _q: &QueueHandle<Self>,
        _o: wl_output::WlOutput,
    ) {
    }
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl LayerShellHandler for State {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        let id = self
            .surfaces
            .iter()
            .find(|(_, e)| e.layer_surface.wl_surface() == layer.wl_surface())
            .map(|(k, _)| k.clone());
        if let Some(id) = id {
            self.surfaces.remove(&id);
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let entry = self
            .surfaces
            .values_mut()
            .find(|e| e.layer_surface.wl_surface() == layer.wl_surface());
        if let Some(entry) = entry {
            let (w, h) = configure.new_size;
            if w > 0 {
                entry.width = w;
            }
            if h > 0 {
                entry.height = h;
            }
            entry.configured = true;
        }
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: SeatCapability,
    ) {
        if capability == SeatCapability::Pointer && self.pointer.is_none() {
            if let Ok(ptr) = self.seat_state.get_pointer(qh, &seat) {
                tracing::debug!("[hover] layer_shell: pointer capability acquired");
                self.pointer = Some(ptr);
            }
        }
        if capability == SeatCapability::Keyboard && self.keyboard.is_none() {
            if let Ok(kbd) = self.seat_state.get_keyboard(qh, &seat, None) {
                self.keyboard = Some(kbd);
            }
        }
    }
    fn remove_capability(
        &mut self,
        _c: &Connection,
        _q: &QueueHandle<Self>,
        _s: wl_seat::WlSeat,
        capability: SeatCapability,
    ) {
        if capability == SeatCapability::Pointer {
            if let Some(p) = self.pointer.take() {
                p.release();
            }
        }
        if capability == SeatCapability::Keyboard {
            if let Some(k) = self.keyboard.take() {
                k.release();
            }
        }
    }
    fn remove_seat(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: wl_seat::WlSeat) {}
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let surface_id = match self.surface_id_for_wl_surface(&event.surface) {
                Some(id) => id,
                None => continue,
            };
            match event.kind {
                PointerEventKind::Enter { .. } => {
                    tracing::debug!("[hover] layer_shell: pointer enter surface_id={surface_id}");
                    self.pointer_focus = Some(surface_id.clone());
                }
                PointerEventKind::Leave { .. } => {
                    tracing::debug!("[hover] layer_shell: pointer leave surface_id={surface_id}");
                    if self.pointer_focus.as_deref() == Some(&surface_id) {
                        self.pointer_focus = None;
                    }
                }
                PointerEventKind::Motion { .. } => {
                    let (x, y) = (event.position.0 as f32, event.position.1 as f32);
                    tracing::trace!(
                        "[hover] layer_shell: pointer motion surface_id={surface_id} x={x:.1} y={y:.1}"
                    );
                    self.events
                        .push(DevWindowEvent::PointerMove { surface_id, x, y });
                }
                PointerEventKind::Press { button, .. } => {
                    if button == 0x110 {
                        let (x, y) = (event.position.0 as f32, event.position.1 as f32);
                        tracing::debug!(
                            "[hover] layer_shell: pointer press surface_id={surface_id} x={x:.1} y={y:.1}"
                        );
                        self.events.push(DevWindowEvent::PointerButton {
                            surface_id,
                            x,
                            y,
                            pressed: true,
                        });
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    if button == 0x110 {
                        let (x, y) = (event.position.0 as f32, event.position.1 as f32);
                        tracing::debug!(
                            "[hover] layer_shell: pointer release surface_id={surface_id} x={x:.1} y={y:.1}"
                        );
                        self.events.push(DevWindowEvent::PointerButton {
                            surface_id,
                            x,
                            y,
                            pressed: false,
                        });
                    }
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    let (x, y) = (event.position.0 as f32, event.position.1 as f32);
                    let dx = -horizontal.absolute as f32;
                    let dy = -vertical.absolute as f32;
                    if dx.abs() > f32::EPSILON || dy.abs() > f32::EPSILON {
                        self.events.push(DevWindowEvent::Scroll {
                            surface_id,
                            x,
                            y,
                            dx,
                            dy,
                        });
                    }
                }
            }
        }
    }
}

impl KeyboardHandler for State {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[Keysym],
    ) {
        self.keyboard_focus = self.surface_id_for_wl_surface(surface);
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
        self.keyboard_focus = None;
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        let Some(surface_id) = self.keyboard_focus.clone() else {
            return;
        };
        let name = keysym_name(event.keysym);
        let mods = crate::dev_window::KeyMods {
            ctrl: self.keyboard_mods.ctrl,
            shift: self.keyboard_mods.shift,
            alt: self.keyboard_mods.alt,
        };
        self.events.push(DevWindowEvent::Key {
            surface_id: surface_id.clone(),
            event: DevWindowKeyEvent::Pressed(name, mods),
        });
        if let Some(ch) = event
            .utf8
            .as_deref()
            .and_then(|s| s.chars().next())
            .filter(|c| !c.is_control())
        {
            self.events.push(DevWindowEvent::Char { surface_id, ch });
        }
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        let Some(surface_id) = self.keyboard_focus.clone() else {
            return;
        };
        let name = keysym_name(event.keysym);
        self.events.push(DevWindowEvent::Key {
            surface_id,
            event: DevWindowKeyEvent::Released(name),
        });
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: Modifiers,
        _layout: u32,
    ) {
        self.keyboard_mods = modifiers;
    }
}

fn keysym_name(sym: Keysym) -> String {
    sym.name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{:#x}", sym.raw()))
}

impl State {
    fn surface_id_for_wl_surface(&self, surface: &wl_surface::WlSurface) -> Option<String> {
        self.surfaces
            .iter()
            .find(|(_, e)| e.layer_surface.wl_surface() == surface)
            .map(|(k, _)| k.clone())
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);
delegate_layer!(State);
delegate_seat!(State);
delegate_pointer!(State);
delegate_keyboard!(State);
delegate_registry!(State);
