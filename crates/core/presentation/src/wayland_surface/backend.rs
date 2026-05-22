use super::*;
use mesh_core_render::DamageRect;

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

const SHM_BUFFER_POOL_DEPTH: usize = 2;

#[derive(Debug)]
pub(super) struct SurfaceShmBuffer {
    buffer: Buffer,
    width: u32,
    height: u32,
    stride: i32,
    pending_damage: Option<DamageRect>,
}

pub(super) struct SurfaceEntry {
    pub(super) layer_surface: LayerSurface,
    pub(super) cfg: LayerSurfaceConfig,
    pub(super) applied_keyboard_mode: KeyboardMode,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) configured: bool,
    shm_buffers: Vec<SurfaceShmBuffer>,
    next_shm_buffer: usize,
    pub(super) frame_pending: bool,
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
            shm_buffers: Vec::new(),
            next_shm_buffer: 0,
            frame_pending: false,
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
        let requires_fresh_configure =
            surface_change_requires_fresh_configure(&self.cfg, &cfg, self.configured);
        let effective_cfg = cfg.with_keyboard_mode(keyboard_mode);
        apply_config(&self.layer_surface, &effective_cfg);
        self.layer_surface.commit();
        self.cfg = cfg;
        self.applied_keyboard_mode = keyboard_mode;
        // Geometry/layout reconfiguration can require a fresh layer-shell
        // configure before another buffer attach. Keyboard interactivity-only
        // changes do not, and some compositors never answer them with a new
        // configure event. Keeping `configured` true in that case avoids
        // dropping every subsequent present after a mouse-triggered focus
        // transition on an already visible surface.
        if requires_fresh_configure {
            self.configured = false;
        }
    }

    fn hide(&mut self) {
        self.frame_pending = false;
        let wl_surface = self.layer_surface.wl_surface();
        wl_surface.attach(None, 0, 0);
        self.layer_surface.commit();
        // Wait for a fresh configure before attaching a buffer again after remap.
        self.configured = false;
    }

    fn copy_into_shm_buffer(
        &mut self,
        pool: &mut SlotPool,
        src: &[u8],
        width: u32,
        height: u32,
        damage: Option<DamageRect>,
    ) -> Result<(usize, DamageRect), PresentationError> {
        let width = width.max(1);
        let height = height.max(1);
        let stride = width as i32 * 4;
        let full = full_damage(width, height);
        let frame_damage = damage
            .and_then(|rect| clip_damage(rect, full))
            .unwrap_or(full);
        if self
            .shm_buffers
            .iter()
            .any(|slot| slot.width != width || slot.height != height || slot.stride != stride)
        {
            self.shm_buffers.clear();
            self.next_shm_buffer = 0;
        }

        while self.shm_buffers.len() < SHM_BUFFER_POOL_DEPTH {
            self.shm_buffers
                .push(create_surface_shm_buffer(pool, width, height, stride)?);
        }

        for slot in &mut self.shm_buffers {
            slot.pending_damage = Some(union_damage(slot.pending_damage, frame_damage));
        }

        let len = self.shm_buffers.len();
        for offset in 0..len {
            let index = (self.next_shm_buffer + offset) % len;
            let copy_damage = self.shm_buffers[index]
                .pending_damage
                .unwrap_or(frame_damage);
            if let Some(canvas) = pool.canvas(&self.shm_buffers[index].buffer) {
                copy_bgra_damage_to_canvas(src, canvas, width, height, copy_damage);
                self.shm_buffers[index].pending_damage = None;
                self.next_shm_buffer = (index + 1) % self.shm_buffers.len();
                // When a buffer is reused while older frame callbacks are still
                // outstanding, `pending_damage` can be larger than the current
                // frame's damage. We must report the region we actually copied
                // into this buffer, otherwise the compositor may keep showing
                // stale pixels outside `frame_damage`.
                return Ok((index, copy_damage));
            }
        }

        let index = self.shm_buffers.len();
        let (wl_buffer, canvas) = pool
            .create_buffer(
                width as i32,
                height as i32,
                stride,
                wl_shm::Format::Argb8888,
            )
            .map_err(|e| PresentationError::BufferAlloc(format!("create_buffer: {e}")))?;
        copy_bgra_to_canvas(src, canvas, width, height);
        self.shm_buffers.push(SurfaceShmBuffer {
            buffer: wl_buffer,
            width,
            height,
            stride,
            pending_damage: None,
        });
        self.next_shm_buffer = (index + 1) % self.shm_buffers.len();
        Ok((index, full))
    }

    fn attach_shm_buffer(
        &mut self,
        qh: &QueueHandle<State>,
        index: usize,
        width: u32,
        height: u32,
        damage: DamageRect,
    ) {
        let buffer = &self.shm_buffers[index].buffer;
        let wl_surface = self.layer_surface.wl_surface();
        wl_surface.damage_buffer(
            damage.x as i32,
            damage.y as i32,
            damage.width as i32,
            damage.height as i32,
        );
        buffer.attach_to(wl_surface).ok();
        wl_surface.frame(qh, wl_surface.clone());
        self.frame_pending = true;
        self.layer_surface.commit();
        self.width = width;
        self.height = height;
    }
}

fn surface_change_requires_fresh_configure(
    previous: &LayerSurfaceConfig,
    next: &LayerSurfaceConfig,
    configured: bool,
) -> bool {
    !configured
        || previous.edge != next.edge
        || previous.layer != next.layer
        || previous.exclusive_zone != next.exclusive_zone
        || previous.width != next.width
        || previous.height != next.height
        || previous.margin_top != next.margin_top
        || previous.margin_right != next.margin_right
        || previous.margin_bottom != next.margin_bottom
        || previous.margin_left != next.margin_left
}

fn resolved_surface_size(entry: &SurfaceEntry, output_size: Option<(u32, u32)>) -> (u32, u32) {
    resolved_surface_size_for_config(&entry.cfg, entry.width, entry.height, output_size)
}

fn resolved_surface_size_for_config(
    cfg: &LayerSurfaceConfig,
    configured_width: u32,
    configured_height: u32,
    output_size: Option<(u32, u32)>,
) -> (u32, u32) {
    let (output_width, output_height) = output_size.unwrap_or((0, 0));
    let width = match cfg.edge {
        Some(Edge::Top) | Some(Edge::Bottom) if cfg.width == 0 => {
            configured_width.max(output_width).max(1)
        }
        _ => configured_width.max(1),
    };
    let height = match cfg.edge {
        Some(Edge::Left) | Some(Edge::Right) if cfg.height == 0 => {
            configured_height.max(output_height).max(1)
        }
        _ => configured_height.max(1),
    };
    (width, height)
}

fn create_surface_shm_buffer(
    pool: &mut SlotPool,
    width: u32,
    height: u32,
    stride: i32,
) -> Result<SurfaceShmBuffer, PresentationError> {
    let (buffer, _) = pool
        .create_buffer(
            width as i32,
            height as i32,
            stride,
            wl_shm::Format::Argb8888,
        )
        .map_err(|e| PresentationError::BufferAlloc(format!("create_buffer: {e}")))?;
    Ok(SurfaceShmBuffer {
        buffer,
        width,
        height,
        stride,
        pending_damage: Some(full_damage(width, height)),
    })
}

fn copy_bgra_to_canvas(src: &[u8], canvas: &mut [u8], width: u32, height: u32) {
    // wl_shm Argb8888 is B,G,R,A in little-endian memory, matching PixelBuffer.
    let len = (width as usize) * (height as usize) * 4;
    if canvas.len() >= len && src.len() >= len {
        canvas[..len].copy_from_slice(&src[..len]);
    }
}

fn full_damage(width: u32, height: u32) -> DamageRect {
    DamageRect {
        x: 0,
        y: 0,
        width: width.max(1),
        height: height.max(1),
    }
}

fn clip_damage(rect: DamageRect, bounds: DamageRect) -> Option<DamageRect> {
    let x1 = rect.x.max(bounds.x);
    let y1 = rect.y.max(bounds.y);
    let x2 = rect
        .x
        .saturating_add(rect.width)
        .min(bounds.x.saturating_add(bounds.width));
    let y2 = rect
        .y
        .saturating_add(rect.height)
        .min(bounds.y.saturating_add(bounds.height));
    (x2 > x1 && y2 > y1).then_some(DamageRect {
        x: x1,
        y: y1,
        width: x2 - x1,
        height: y2 - y1,
    })
}

fn union_damage(current: Option<DamageRect>, next: DamageRect) -> DamageRect {
    let Some(current) = current else {
        return next;
    };
    if current.width == 0 || current.height == 0 {
        return next;
    }
    if next.width == 0 || next.height == 0 {
        return current;
    }
    let left = current.x.min(next.x);
    let top = current.y.min(next.y);
    let right = current
        .x
        .saturating_add(current.width)
        .max(next.x.saturating_add(next.width));
    let bottom = current
        .y
        .saturating_add(current.height)
        .max(next.y.saturating_add(next.height));
    DamageRect {
        x: left,
        y: top,
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
    }
}

fn copy_bgra_damage_to_canvas(
    src: &[u8],
    canvas: &mut [u8],
    width: u32,
    height: u32,
    damage: DamageRect,
) {
    let Some(damage) = clip_damage(damage, full_damage(width, height)) else {
        return;
    };
    let stride = width as usize * 4;
    let row_bytes = damage.width as usize * 4;
    let x_offset = damage.x as usize * 4;
    for row in damage.y as usize..damage.y.saturating_add(damage.height) as usize {
        let start = row * stride + x_offset;
        let end = start + row_bytes;
        if end <= src.len() && end <= canvas.len() {
            canvas[start..end].copy_from_slice(&src[start..end]);
        }
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
            focus_grab_requested_at: None,
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
        if cfg.keyboard_mode != KeyboardMode::OnDemand {
            self.state.release_surface_focus_grab(surface_id);
        }
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
        title: &str,
        visible: bool,
        buffer: &PixelBuffer,
    ) -> Result<(), PresentationError> {
        self.present_with_damage(surface_id, title, visible, buffer, None)
    }

    pub fn present_with_damage(
        &mut self,
        surface_id: &str,
        _title: &str,
        visible: bool,
        buffer: &PixelBuffer,
        damage: Option<DamageRect>,
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

        let width = buffer.width.max(1);
        let height = buffer.height.max(1);
        if self
            .state
            .surfaces
            .get(surface_id)
            .is_some_and(|entry| entry.frame_pending)
        {
            // Frame callbacks are throttling hints, not correctness gates.
            // Some layer-shell compositors can leave a callback pending while
            // the surface remains otherwise usable; hard-deferring every
            // repaint behind that flag makes later theme/focus/drag updates
            // invisible until a remap path happens to clear the surface state.
            // Drain any available callback, then commit the latest buffer even
            // if the old callback is still pending.
            self.dispatch_available()?;
        }
        let qh = self.state.qh.clone();
        let state = &mut self.state;
        let pool = state
            .pool
            .as_mut()
            .ok_or_else(|| PresentationError::BufferAlloc("shm pool not initialised".into()))?;
        let Some(entry) = state.surfaces.get_mut(surface_id) else {
            return Ok(());
        };
        if !entry.configured {
            return Ok(());
        }
        let (buffer_index, damage) =
            entry.copy_into_shm_buffer(pool, &buffer.data, width, height, damage)?;
        entry.attach_shm_buffer(&qh, buffer_index, width, height, damage);

        self.dispatch_pending()?;
        Ok(())
    }

    pub fn surface_size(
        &mut self,
        surface_id: &str,
    ) -> Result<Option<(u32, u32)>, PresentationError> {
        self.wait_for_surface_configure(surface_id)?;

        Ok(self.surface_size_if_known(surface_id))
    }

    pub fn surface_size_if_known(&self, surface_id: &str) -> Option<(u32, u32)> {
        self.state
            .surfaces
            .get(surface_id)
            .filter(|entry| entry.configured)
            .map(|entry| resolved_surface_size(entry, self.output_logical_size()))
    }

    pub fn pump(&mut self) {
        let _ = self.dispatch_available();
        let _ = self.release_expired_surface_focus_grab();
    }

    pub fn poll_events(&mut self) -> Vec<DevWindowEvent> {
        let _ = self.dispatch_available();
        let _ = self.release_expired_surface_focus_grab();
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

    fn release_expired_surface_focus_grab(&mut self) -> Result<(), PresentationError> {
        if self.state.release_expired_surface_focus_grab() {
            self.event_queue
                .flush()
                .map_err(|e| PresentationError::SurfaceCreate(format!("flush: {e}")))?;
        }
        Ok(())
    }

    fn dispatch_pending(&mut self) -> Result<(), PresentationError> {
        self.event_queue
            .flush()
            .map_err(|e| PresentationError::SurfaceCreate(format!("flush: {e}")))?;
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| PresentationError::SurfaceCreate(format!("dispatch: {e}")))?;
        self.release_expired_surface_focus_grab()?;
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
        self.release_expired_surface_focus_grab()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn base_cfg() -> LayerSurfaceConfig {
        LayerSurfaceConfig {
            edge: Some(Edge::Left),
            layer: MeshLayer::Overlay,
            size_policy: LayerSurfaceSizePolicy::Fixed,
            width: 280,
            height: 164,
            exclusive_zone: 0,
            keyboard_mode: KeyboardMode::OnDemand,
            namespace: "@mesh/audio-popover".into(),
            margin_top: 24,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 24,
        }
    }

    #[test]
    fn keyboard_mode_only_reconfigure_keeps_surface_configured() {
        let previous = base_cfg();
        let mut next = previous.clone();
        next.keyboard_mode = KeyboardMode::Exclusive;

        assert!(
            !surface_change_requires_fresh_configure(&previous, &next, true),
            "keyboard interactivity-only changes must not force a fresh configure for an already-visible surface"
        );
    }

    #[test]
    fn geometry_reconfigure_still_requires_fresh_configure() {
        let previous = base_cfg();
        let mut next = previous.clone();
        next.width = 320;

        assert!(surface_change_requires_fresh_configure(
            &previous, &next, true
        ));
    }

    #[test]
    fn unconfigured_surface_still_requires_initial_configure() {
        let previous = base_cfg();
        let next = previous.clone();

        assert!(surface_change_requires_fresh_configure(
            &previous, &next, false
        ));
    }

    #[test]
    fn dynamic_top_surface_uses_output_width_when_configure_width_is_unspecified() {
        let mut cfg = base_cfg();
        cfg.edge = Some(Edge::Top);
        cfg.width = 0;
        cfg.height = 50;

        assert_eq!(
            resolved_surface_size_for_config(&cfg, 1, 50, Some((1920, 1080))),
            (1920, 50),
            "top bars with width=0 must paint across the output even when the compositor leaves configure width unspecified"
        );
    }

    #[test]
    fn dynamic_left_surface_uses_output_height_when_configure_height_is_unspecified() {
        let mut cfg = base_cfg();
        cfg.edge = Some(Edge::Left);
        cfg.width = 56;
        cfg.height = 0;

        assert_eq!(
            resolved_surface_size_for_config(&cfg, 56, 1, Some((1920, 1080))),
            (56, 1080),
            "left rails with height=0 must paint across the output height when the compositor leaves configure height unspecified"
        );
    }
}
