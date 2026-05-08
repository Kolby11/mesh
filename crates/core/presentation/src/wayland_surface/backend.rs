use super::*;

/// Configuration passed from the shell before each present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerSurfaceSizePolicy {
    Fixed,
    Flexible,
}

#[derive(Debug, Clone)]
pub struct LayerSurfaceConfig {
    pub edge: Option<Edge>,
    pub layer: MeshLayer,
    pub size_policy: LayerSurfaceSizePolicy,
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
            size_policy: LayerSurfaceSizePolicy::Fixed,
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

impl LayerSurfaceConfig {
    pub(super) fn with_keyboard_mode(&self, keyboard_mode: KeyboardMode) -> Self {
        let mut cfg = self.clone();
        cfg.keyboard_mode = keyboard_mode;
        cfg
    }
}

pub struct LayerShellBackend {
    _conn: Connection,
    event_queue: EventQueue<State>,
    state: State,
}

pub(super) struct SurfaceEntry {
    pub(super) layer_surface: LayerSurface,
    pub(super) cfg: LayerSurfaceConfig,
    pub(super) applied_keyboard_mode: KeyboardMode,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) configured: bool,
}

impl SurfaceEntry {
    fn new(
        layer_surface: LayerSurface,
        cfg: LayerSurfaceConfig,
        applied_keyboard_mode: KeyboardMode,
    ) -> Self {
        Self {
            layer_surface,
            width: cfg.width.max(1),
            height: cfg.height.max(1),
            cfg,
            applied_keyboard_mode,
            configured: false,
        }
    }

    fn needs_reconfigure(&self, cfg: &LayerSurfaceConfig, keyboard_mode: KeyboardMode) -> bool {
        self.cfg.edge != cfg.edge
            || self.cfg.layer != cfg.layer
            || self.cfg.exclusive_zone != cfg.exclusive_zone
            || self.applied_keyboard_mode != keyboard_mode
            || self.cfg.width != cfg.width
            || self.cfg.height != cfg.height
            || self.cfg.margin_top != cfg.margin_top
            || self.cfg.margin_right != cfg.margin_right
            || self.cfg.margin_bottom != cfg.margin_bottom
            || self.cfg.margin_left != cfg.margin_left
            || !self.configured
    }

    fn apply_config(&mut self, cfg: LayerSurfaceConfig, keyboard_mode: KeyboardMode) {
        let effective_cfg = cfg.with_keyboard_mode(keyboard_mode);
        apply_config(&self.layer_surface, &effective_cfg);
        self.layer_surface.commit();
        self.cfg = cfg;
        self.applied_keyboard_mode = keyboard_mode;
    }

    fn hide(&mut self) {
        let wl_surface = self.layer_surface.wl_surface();
        wl_surface.attach(None, 0, 0);
        self.layer_surface.commit();
        // Wait for a fresh configure before attaching a buffer again after remap.
        self.configured = false;
    }

    fn attach_buffer(
        &mut self,
        buffer: &smithay_client_toolkit::shm::slot::Buffer,
        width: u32,
        height: u32,
    ) {
        let wl_surface = self.layer_surface.wl_surface();
        wl_surface.damage_buffer(0, 0, width as i32, height as i32);
        buffer.attach_to(wl_surface).ok();
        self.layer_surface.commit();
        self.width = width;
        self.height = height;
    }
}

impl LayerShellBackend {
    pub fn new() -> Result<Self, PresentationError> {
        let conn = Connection::connect_to_env()
            .map_err(|e| PresentationError::WaylandConnect(format!("connect_to_env: {e}")))?;
        let (globals, event_queue) = registry_queue_init::<State>(&conn)
            .map_err(|e| PresentationError::WaylandConnect(format!("registry_queue_init: {e}")))?;
        let qh = event_queue.handle();

        let registry_state = RegistryState::new(&globals);
        let output_state = OutputState::new(&globals, &qh);
        let compositor_state = CompositorState::bind(&globals, &qh)
            .map_err(|e| PresentationError::ProtocolUnsupported(format!("wl_compositor: {e}")))?;
        let shm = Shm::bind(&globals, &qh)
            .map_err(|e| PresentationError::ProtocolUnsupported(format!("wl_shm: {e}")))?;
        let layer_shell = LayerShell::bind(&globals, &qh).map_err(|e| {
            PresentationError::ProtocolUnsupported(format!("zwlr_layer_shell_v1: {e}"))
        })?;
        let activation_state = ActivationState::bind(&globals, &qh).ok();
        let focus_grab_manager = globals.bind(&qh, 1..=1, GlobalData).ok();
        let seat_state = SeatState::new(&globals, &qh);

        let pool = SlotPool::new(256 * 256 * 4, &shm).ok();

        let state = State {
            registry_state,
            output_state,
            compositor_state,
            shm,
            layer_shell,
            activation_state,
            focus_grab_manager,
            seat_state,
            activation_seat: None,
            focus_grab: None,
            focus_grab_surface_id: None,
            qh,
            pool,
            surfaces: HashMap::new(),
            pointer: None,
            keyboard: None,
            pointer_focus: None,
            keyboard_focus: None,
            keyboard_mods: Modifiers::default(),
            keyboard_repeat_info: RepeatInfo::Disable,
            keyboard_repeat: None,
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
        let cfg = self.clamp_surface_config(cfg);
        let qh = self.state.qh.clone();
        let effective_keyboard_mode = self
            .state
            .effective_keyboard_mode_for(surface_id, cfg.keyboard_mode);
        match self.state.surfaces.get_mut(surface_id) {
            Some(entry) => {
                if entry.needs_reconfigure(&cfg, effective_keyboard_mode) {
                    // Re-commit to re-map the surface and prompt the compositor to
                    // send a fresh configure event before we attach a buffer.
                    entry.apply_config(cfg, effective_keyboard_mode);
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
                self.state.surfaces.insert(
                    surface_id.to_string(),
                    SurfaceEntry::new(layer_surface, cfg, effective_keyboard_mode),
                );
                if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
                    let cfg = entry.cfg.clone();
                    entry.apply_config(cfg, effective_keyboard_mode);
                }
            }
        }
    }

    fn clamp_surface_config(&self, mut cfg: LayerSurfaceConfig) -> LayerSurfaceConfig {
        let Some((output_width, output_height)) = self.output_logical_size() else {
            return cfg;
        };

        if cfg.width == 0 || cfg.height == 0 {
            return cfg;
        }

        let max_width = output_width.max(1);
        let max_height = output_height.max(1);

        if cfg.size_policy == LayerSurfaceSizePolicy::Flexible {
            cfg.width = cfg.width.min(max_width);
            cfg.height = cfg.height.min(max_height);
        } else {
            cfg.width = cfg.width.min(max_width);
            cfg.height = cfg.height.min(max_height);
        }

        match cfg.edge {
            Some(Edge::Left) | None => {
                let max_left = max_width.saturating_sub(cfg.width) as i32;
                let max_top = max_height.saturating_sub(cfg.height) as i32;
                cfg.margin_left = cfg.margin_left.clamp(0, max_left.max(0));
                cfg.margin_top = cfg.margin_top.clamp(0, max_top.max(0));
            }
            Some(Edge::Right) => {
                let max_right = max_width.saturating_sub(cfg.width) as i32;
                let max_top = max_height.saturating_sub(cfg.height) as i32;
                cfg.margin_right = cfg.margin_right.clamp(0, max_right.max(0));
                cfg.margin_top = cfg.margin_top.clamp(0, max_top.max(0));
            }
            Some(Edge::Top) => {
                let max_left = max_width.saturating_sub(cfg.width) as i32;
                cfg.margin_left = cfg.margin_left.clamp(0, max_left.max(0));
            }
            Some(Edge::Bottom) => {
                let max_left = max_width.saturating_sub(cfg.width) as i32;
                let max_bottom = max_height.saturating_sub(cfg.height) as i32;
                cfg.margin_left = cfg.margin_left.clamp(0, max_left.max(0));
                cfg.margin_bottom = cfg.margin_bottom.clamp(0, max_bottom.max(0));
            }
        }

        cfg
    }

    fn output_logical_size(&self) -> Option<(u32, u32)> {
        self.state
            .output_state
            .outputs()
            .into_iter()
            .find_map(|output| self.state.output_state.info(&output))
            .and_then(|info| {
                info.logical_size
                    .or_else(|| {
                        info.modes
                            .iter()
                            .find(|mode| mode.current)
                            .map(|mode| (mode.dimensions.0 as i32, mode.dimensions.1 as i32))
                    })
                    .and_then(|(width, height)| {
                        let width = u32::try_from(width).ok()?;
                        let height = u32::try_from(height).ok()?;
                        Some((width, height))
                    })
            })
    }

    pub fn present(
        &mut self,
        surface_id: &str,
        _title: &str,
        visible: bool,
        buffer: &PixelBuffer,
    ) -> Result<(), PresentationError> {
        if !visible {
            self.state.release_surface_focus_grab(surface_id);
            // Only detach a buffer (to hide) if the compositor has already configured this
            // surface. Before the first configure event the surface has no buffer attached
            // and is already invisible; committing a null buffer before configure arrives
            // triggers a Wayland protocol error.
            if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
                if entry.configured {
                    entry.hide();
                }
            }
            self.dispatch_pending()?;
            return Ok(());
        }

        if !self.state.surfaces.contains_key(surface_id) {
            // present() called before configure() — nothing to do.
            return Ok(());
        }
        self.wait_for_surface_configure(surface_id)?;

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
            .ok_or_else(|| PresentationError::BufferAlloc("shm pool not initialised".into()))?;

        let (wl_buffer, canvas) = pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .map_err(|e| PresentationError::BufferAlloc(format!("create_buffer: {e}")))?;

        // Copy BGRA -> ARGB8888 little-endian (wl_shm Argb8888 is B,G,R,A in memory).
        let src = &buffer.data;
        let len = (width as usize) * (height as usize) * 4;
        if canvas.len() >= len && src.len() >= len {
            canvas[..len].copy_from_slice(&src[..len]);
        }

        entry.attach_buffer(&wl_buffer, width, height);

        self.dispatch_pending()?;
        Ok(())
    }

    pub fn surface_size(
        &mut self,
        surface_id: &str,
    ) -> Result<Option<(u32, u32)>, PresentationError> {
        self.wait_for_surface_configure(surface_id)?;

        Ok(self
            .state
            .surfaces
            .get(surface_id)
            .map(|entry| (entry.width.max(1), entry.height.max(1))))
    }

    pub fn pump(&mut self) {
        let _ = self.dispatch_available();
    }

    pub fn poll_events(&mut self) -> Vec<DevWindowEvent> {
        let _ = self.dispatch_available();
        self.state.push_due_keyboard_repeats();
        let events = std::mem::take(&mut self.state.events);
        if !events.is_empty() {
            tracing::trace!(
                "[hover] layer_shell: draining {} input event(s)",
                events.len()
            );
        }
        events
    }

    fn dispatch_pending(&mut self) -> Result<(), PresentationError> {
        self.event_queue
            .flush()
            .map_err(|e| PresentationError::SurfaceCreate(format!("flush: {e}")))?;
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| PresentationError::SurfaceCreate(format!("dispatch: {e}")))?;
        Ok(())
    }

    fn wait_for_surface_configure(&mut self, surface_id: &str) -> Result<(), PresentationError> {
        let needs_configure = self
            .state
            .surfaces
            .get(surface_id)
            .map(|entry| !entry.configured)
            .unwrap_or(false);
        if !needs_configure {
            return Ok(());
        }

        for _ in 0..10 {
            self.event_queue
                .roundtrip(&mut self.state)
                .map_err(|e| PresentationError::SurfaceCreate(format!("roundtrip: {e}")))?;
            if self
                .state
                .surfaces
                .get(surface_id)
                .map(|entry| entry.configured)
                .unwrap_or(false)
            {
                break;
            }
        }

        Ok(())
    }

    fn dispatch_available(&mut self) -> Result<(), PresentationError> {
        self.event_queue
            .flush()
            .map_err(|e| PresentationError::SurfaceCreate(format!("flush: {e}")))?;

        for _ in 0..32 {
            self.event_queue
                .dispatch_pending(&mut self.state)
                .map_err(|e| PresentationError::SurfaceCreate(format!("dispatch: {e}")))?;

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
                            return Err(PresentationError::SurfaceCreate(format!("read: {err}")));
                        }
                    }
                }
                Err(rustix::io::Errno::INTR) => {
                    drop(read_guard);
                    break;
                }
                Err(err) => {
                    drop(read_guard);
                    return Err(PresentationError::SurfaceCreate(format!("poll: {err}")));
                }
            }
        }

        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| PresentationError::SurfaceCreate(format!("dispatch: {e}")))?;
        Ok(())
    }
}

pub(super) fn apply_config(layer_surface: &LayerSurface, cfg: &LayerSurfaceConfig) {
    layer_surface.set_layer(map_layer(cfg.layer));
    layer_surface.set_anchor(map_anchor(cfg));
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

fn map_anchor(cfg: &LayerSurfaceConfig) -> Anchor {
    match cfg.edge {
        // Treat a single edge as a normal shell placement, not a centered popup.
        // Top/bottom bars stretch across the output width, and left/right rails
        // pin to the top corner instead of floating in the vertical center.
        // If a left/right rail requests `height == 0`, layer-shell expects it
        // to be anchored to both top and bottom so the compositor can stretch
        // it vertically across the output.
        Some(Edge::Top) => Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
        Some(Edge::Bottom) => Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        Some(Edge::Left) if cfg.height == 0 => Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT,
        Some(Edge::Right) if cfg.height == 0 => Anchor::TOP | Anchor::BOTTOM | Anchor::RIGHT,
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
