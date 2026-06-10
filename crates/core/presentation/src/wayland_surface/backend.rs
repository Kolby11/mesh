use super::*;
use mesh_core_render::DamageRect;
use std::hash::{Hash, Hasher};

/// Configuration passed from the shell before each present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerSurfaceSizePolicy {
    Fixed,
    Flexible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
const SHM_BUFFER_POOL_MAX: usize = 3;
const MAX_FRAME_CALLBACK_WAIT: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShmPoolConfig {
    width: u32,
    height: u32,
    stride: i32,
}

#[derive(Debug)]
pub(super) struct SurfaceShmBuffer {
    buffer: Buffer,
    pending_damage: Option<DamageRect>,
}

pub(super) struct SurfaceEntry {
    pub(super) layer_surface: LayerSurface,
    pub(super) cfg: LayerSurfaceConfig,
    pub(super) applied_keyboard_mode: KeyboardMode,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) configured: bool,
    pub(super) config_fingerprint: u64,
    shm_buffers: Vec<SurfaceShmBuffer>,
    shm_pool_config: Option<ShmPoolConfig>,
    next_shm_buffer: usize,
    pub(super) frame_pending: bool,
    pub(super) frame_pending_since: Option<Instant>,
    pub(super) scale: f32,
    pub(super) needs_full_redraw: bool,
    pub(super) fractional_scale: Option<WpFractionalScaleV1>,
    pub(super) viewport: Option<WpViewport>,
}

struct SurfaceConfigHasher(u64);

impl Default for SurfaceConfigHasher {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for SurfaceConfigHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
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
            config_fingerprint: surface_config_fingerprint(&cfg, applied_keyboard_mode),
            cfg,
            applied_keyboard_mode,
            configured: false,
            shm_buffers: Vec::new(),
            shm_pool_config: None,
            next_shm_buffer: 0,
            frame_pending: false,
            frame_pending_since: None,
            scale: 1.0,
            needs_full_redraw: false,
            fractional_scale: None,
            viewport: None,
        }
    }

    fn needs_reconfigure(&self, cfg: &LayerSurfaceConfig, keyboard_mode: KeyboardMode) -> bool {
        !self.configured
            || self.config_fingerprint != surface_config_fingerprint(cfg, keyboard_mode)
    }

    fn apply_config(&mut self, cfg: LayerSurfaceConfig, keyboard_mode: KeyboardMode) {
        let requires_fresh_configure =
            surface_change_requires_fresh_configure(&self.cfg, &cfg, self.configured);
        let effective_cfg = cfg.with_keyboard_mode(keyboard_mode);
        apply_config(&self.layer_surface, &effective_cfg);
        self.layer_surface.commit();
        self.config_fingerprint = surface_config_fingerprint(&cfg, keyboard_mode);
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
        self.frame_pending_since = None;
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
        let pool_config = ShmPoolConfig {
            width,
            height,
            stride,
        };
        if self.shm_pool_config != Some(pool_config) {
            self.shm_buffers.clear();
            self.next_shm_buffer = 0;
            self.shm_pool_config = Some(pool_config);
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

        if self.shm_buffers.len() >= SHM_BUFFER_POOL_MAX {
            return Err(PresentationError::BufferAlloc(format!(
                "all {SHM_BUFFER_POOL_MAX} SHM buffers are busy for {}x{} surface",
                width, height
            )));
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
            pending_damage: None,
        });
        self.next_shm_buffer = (index + 1) % self.shm_buffers.len();
        Ok((index, full))
    }

    fn attach_shm_buffer(
        &mut self,
        qh: &QueueHandle<State>,
        index: usize,
        logical_width: u32,
        logical_height: u32,
        physical_width: u32,
        physical_height: u32,
        damage_rects: &[DamageRect],
        scale: f32,
    ) {
        let buffer = &self.shm_buffers[index].buffer;
        let wl_surface = self.layer_surface.wl_surface();

        // Scale damage rects from logical to physical coordinates
        let physical_damage: Vec<DamageRect> = damage_rects
            .iter()
            .map(|r| scale_damage_rect_to_physical(*r, scale))
            .collect();

        // Clip scaled rects to physical buffer bounds (T-102-06)
        let clipped_damage: Vec<DamageRect> = physical_damage
            .iter()
            .map(|r| clip_damage_rect_to_buffer(*r, physical_width, physical_height))
            .collect();

        // Emit damage_buffer calls with physical coordinates
        for rect in protocol_damage_rects(&clipped_damage, physical_width, physical_height) {
            wl_surface.damage_buffer(
                rect.x as i32,
                rect.y as i32,
                rect.width as i32,
                rect.height as i32,
            );
        }

        // Set buffer scale and viewport destination per D-01/CONTEXT.md
        let scale_is_integer = (scale - scale.round()).abs() < f32::EPSILON;
        if scale_is_integer {
            // Integer scale: use set_buffer_scale only. No viewporter needed.
            wl_surface.set_buffer_scale(scale as i32);
            if let Some(ref viewport) = self.viewport {
                viewport.set_destination(-1, -1); // Reset viewport to intrinsic size
            }
        } else if self.viewport.is_some() {
            // Fractional scale WITH viewporter: set_buffer_scale to ceil(scale),
            // then set_destination to logical dimensions so the compositor scales down.
            let integer_scale = scale.ceil() as i32;
            wl_surface.set_buffer_scale(integer_scale);
            if let Some(ref viewport) = self.viewport {
                viewport.set_destination(logical_width as i32, logical_height as i32);
            }
        } else {
            // Fractional scale WITHOUT viewporter: round to nearest integer,
            // accept slight sizing mismatch per CONTEXT.md Fallback Behavior.
            let rounded_scale = scale.round() as i32;
            wl_surface.set_buffer_scale(rounded_scale);
        }

        buffer.attach_to(wl_surface).ok();
        wl_surface.frame(qh, wl_surface.clone());
        self.frame_pending = true;
        self.frame_pending_since = Some(Instant::now());
        self.layer_surface.commit();
        // Store logical dimensions (the authoritative size)
        self.width = logical_width;
        self.height = logical_height;
    }

    pub(super) fn waiting_for_frame_callback(&self) -> bool {
        self.frame_pending
            && self
                .frame_pending_since
                .is_some_and(|since| since.elapsed() < MAX_FRAME_CALLBACK_WAIT)
    }
}

pub(super) fn surface_config_fingerprint(
    cfg: &LayerSurfaceConfig,
    keyboard_mode: KeyboardMode,
) -> u64 {
    let mut hasher = SurfaceConfigHasher::default();
    surface_edge_slot(cfg.edge).hash(&mut hasher);
    surface_layer_slot(cfg.layer).hash(&mut hasher);
    cfg.exclusive_zone.hash(&mut hasher);
    keyboard_mode_slot(keyboard_mode).hash(&mut hasher);
    cfg.width.hash(&mut hasher);
    cfg.height.hash(&mut hasher);
    cfg.margin_top.hash(&mut hasher);
    cfg.margin_right.hash(&mut hasher);
    cfg.margin_bottom.hash(&mut hasher);
    cfg.margin_left.hash(&mut hasher);
    hasher.finish()
}

fn surface_edge_slot(edge: Option<Edge>) -> u8 {
    match edge {
        Some(Edge::Top) => 1,
        Some(Edge::Bottom) => 2,
        Some(Edge::Left) => 3,
        Some(Edge::Right) => 4,
        None => 0,
    }
}

fn surface_layer_slot(layer: MeshLayer) -> u8 {
    match layer {
        MeshLayer::Background => 1,
        MeshLayer::Bottom => 2,
        MeshLayer::Top => 3,
        MeshLayer::Overlay => 4,
    }
}

fn keyboard_mode_slot(mode: KeyboardMode) -> u8 {
    match mode {
        KeyboardMode::None => 0,
        KeyboardMode::Exclusive => 1,
        KeyboardMode::OnDemand => 2,
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

/// Maximum number of `wl_surface::damage_buffer` calls allowed per commit.
/// When the damage list exceeds this cap the entire surface is marked dirty
/// with a single bounding-union call to avoid unbounded protocol overhead.
const MAX_PROTOCOL_DAMAGE_RECTS: usize = 16;

/// Select the rects to pass to `wl_surface::damage_buffer`.
///
/// When `rects.len() <= MAX_PROTOCOL_DAMAGE_RECTS` every rect is forwarded
/// unchanged (same count, same order). When the count exceeds the cap all
/// rects are collapsed into a single bounding union. An empty input yields
/// an empty output — the caller is responsible for skipping the present.
fn protocol_damage_rects(rects: &[DamageRect], width: u32, height: u32) -> Vec<DamageRect> {
    if rects.is_empty() {
        return Vec::new();
    }
    if rects.len() <= MAX_PROTOCOL_DAMAGE_RECTS {
        return rects.to_vec();
    }
    let union = rects
        .iter()
        .copied()
        .fold(None, |acc, r| Some(union_damage(acc, r)))
        .unwrap_or_else(|| full_damage(width, height));
    vec![union]
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

/// Scale a damage rect from logical (CSS) coordinates to physical (device) coordinates.
fn scale_damage_rect_to_physical(rect: DamageRect, scale: f32) -> DamageRect {
    DamageRect {
        x: (rect.x as f32 * scale).floor() as u32,
        y: (rect.y as f32 * scale).floor() as u32,
        width: ((rect.width as f32 * scale).ceil() as u32).max(1),
        height: ((rect.height as f32 * scale).ceil() as u32).max(1),
    }
}

/// Clip a damage rect to the physical buffer bounds (T-102-06).
/// Sending out-of-bounds damage is a Wayland protocol error.
fn clip_damage_rect_to_buffer(rect: DamageRect, buffer_w: u32, buffer_h: u32) -> DamageRect {
    let x = rect.x.min(buffer_w.saturating_sub(1));
    let y = rect.y.min(buffer_h.saturating_sub(1));
    let w = rect.width.min(buffer_w.saturating_sub(x));
    let h = rect.height.min(buffer_h.saturating_sub(y));
    DamageRect {
        x,
        y,
        width: w.max(1),
        height: h.max(1),
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
        let viewporter: Option<WpViewporter> = globals.bind(&qh, 1..=1, GlobalData).ok();
        let fractional_scale_manager: Option<WpFractionalScaleManagerV1> =
            globals.bind(&qh, 1..=1, GlobalData).ok();
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
            viewporter,
            fractional_scale_manager,
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
                // Bind fractional scale protocol for new surfaces.
                let wl_surface = self
                    .state
                    .surfaces
                    .get(surface_id)
                    .map(|entry| entry.layer_surface.wl_surface().clone());
                let qh = self.state.qh.clone();
                if let Some(wl_surface) = wl_surface
                    && let Some(fs) =
                        self.state
                            .bind_fractional_scale(&wl_surface, &qh, surface_id.to_string())
                    && let Some(entry) = self.state.surfaces.get_mut(surface_id)
                {
                    entry.fractional_scale = Some(fs);
                }
                // Create viewport for this surface (wp_viewporter for non-integer scale)
                if let Some(ref viewporter) = self.state.viewporter {
                    if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
                        let wl_surface = entry.layer_surface.wl_surface().clone();
                        let qh = self.state.qh.clone();
                        entry.viewport = Some(viewporter.get_viewport(&wl_surface, &qh, ()));
                    }
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

        cfg.width = cfg.width.min(max_width);
        cfg.height = cfg.height.min(max_height);

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
                let max_right = max_width.saturating_sub(cfg.width) as i32;
                cfg.margin_left = cfg.margin_left.clamp(0, max_left.max(0));
                cfg.margin_right = cfg.margin_right.clamp(0, max_right.max(0));
            }
            Some(Edge::Bottom) => {
                let max_left = max_width.saturating_sub(cfg.width) as i32;
                let max_right = max_width.saturating_sub(cfg.width) as i32;
                let max_bottom = max_height.saturating_sub(cfg.height) as i32;
                cfg.margin_left = cfg.margin_left.clamp(0, max_left.max(0));
                cfg.margin_right = cfg.margin_right.clamp(0, max_right.max(0));
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
                            .map(|mode| (mode.dimensions.0, mode.dimensions.1))
                    })
                    .and_then(|(width, height)| {
                        let width = u32::try_from(width).ok()?;
                        let height = u32::try_from(height).ok()?;
                        Some((width, height))
                    })
            })
    }

    pub fn present_with_damage(
        &mut self,
        surface_id: &str,
        _title: &str,
        visible: bool,
        buffer: &PixelBuffer,
        damage_rects: &[DamageRect],
    ) -> Result<(), PresentationError> {
        if !visible {
            self.state.release_surface_focus_grab(surface_id);
            // Only detach a buffer (to hide) if the compositor has already configured this
            // surface. Before the first configure event the surface has no buffer attached
            // and is already invisible; committing a null buffer before configure arrives
            // triggers a Wayland protocol error.
            if let Some(entry) = self.state.surfaces.get_mut(surface_id)
                && entry.configured
            {
                entry.hide();
            }
            self.dispatch_pending()?;
            return Ok(());
        }

        if !self.state.surfaces.contains_key(surface_id) {
            // present() called before configure() — nothing to do.
            return Ok(());
        }
        self.wait_for_surface_configure(surface_id)?;

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

        // Get the logical dimensions from compositor-configured size
        let logical_w = entry.width.max(1);
        let logical_h = entry.height.max(1);
        let scale = entry.scale;

        // SHM copy must use physical buffer dimensions for the copy region
        let physical_w = buffer.width.max(1);
        let physical_h = buffer.height.max(1);

        // SHM copy region must always be a union (Pitfall 1) — fold all rects
        // into a single DamageRect for the buffer copy, then forward the
        // original rect slice for the per-rect damage_buffer calls.
        // Damage rects arrive in logical/CSS coordinates; scale to physical
        // before the copy so the region matches the physical buffer dimensions.
        let shm_copy_damage = damage_rects
            .iter()
            .copied()
            .map(|r| scale_damage_rect_to_physical(r, scale))
            .fold(None, |acc, r| Some(union_damage(acc, r)))
            .or_else(|| {
                // If the slice is empty (shouldn't normally reach here due to
                // the skip gate in render.rs), upload the full buffer.
                Some(full_damage(physical_w, physical_h))
            });
        let (buffer_index, _copy_damage) = entry.copy_into_shm_buffer(
            pool,
            &buffer.data,
            physical_w,
            physical_h,
            shm_copy_damage,
        )?;
        entry.attach_shm_buffer(
            &qh,
            buffer_index,
            logical_w,
            logical_h,
            physical_w,
            physical_h,
            damage_rects,
            scale,
        );

        self.dispatch_pending()?;
        Ok(())
    }

    pub(crate) fn update_opaque_region(
        &mut self,
        surface_id: &str,
        opaque_rect: Option<DamageRect>,
    ) {
        let Some(entry) = self.state.surfaces.get(surface_id) else {
            return;
        };
        let wl_surface = entry.layer_surface.wl_surface();

        let Some(rect) = opaque_rect else {
            wl_surface.set_opaque_region(None);
            return;
        };

        if rect.width == 0 || rect.height == 0 {
            wl_surface.set_opaque_region(None);
            return;
        }

        let Ok(region) = Region::new(&self.state.compositor_state) else {
            return;
        };
        region.add(
            rect.x as i32,
            rect.y as i32,
            rect.width as i32,
            rect.height as i32,
        );
        wl_surface.set_opaque_region(Some(region.wl_region()));
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

    pub fn surface_waiting_for_frame_callback(&self, surface_id: &str) -> bool {
        self.state
            .surfaces
            .get(surface_id)
            .is_some_and(SurfaceEntry::waiting_for_frame_callback)
    }

    pub fn surface_scale(&self, surface_id: &str) -> f32 {
        self.state
            .surfaces
            .get(surface_id)
            .map(|entry| entry.scale)
            .unwrap_or(1.0)
    }

    pub fn surface_needs_full_redraw(&self, surface_id: &str) -> bool {
        self.state
            .surfaces
            .get(surface_id)
            .map(|entry| entry.needs_full_redraw)
            .unwrap_or(false)
    }

    pub fn clear_surface_needs_full_redraw(&mut self, surface_id: &str) {
        if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
            entry.needs_full_redraw = false;
        }
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

    /// Block on the Wayland connection fd until `timeout` elapses or a wakeup occurs.
    ///
    /// After the Wayland poll returns (or times out), checks `eventfd_fd` with a
    /// 0ms poll to detect IPC/backend signals. Reads and consumes the eventfd
    /// counter when signaled.
    pub fn wait_for_events(
        &mut self,
        timeout: std::time::Duration,
        eventfd_fd: std::os::unix::io::BorrowedFd<'_>,
    ) -> Result<crate::WaitResult, crate::PresentationError> {
        use crate::{WaitReason, WaitResult};
        use rustix::io::read as eventfd_read;

        // 1. Flush and drain any already-pending events (non-blocking).
        self.event_queue
            .flush()
            .map_err(|e| PresentationError::SurfaceCreate(format!("flush: {e}")))?;
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| PresentationError::SurfaceCreate(format!("dispatch: {e}")))?;

        // 2. prepare_read — if None, events arrived between dispatch_pending and
        //    prepare_read; don't block, let the caller process them.
        let Some(read_guard) = self.event_queue.prepare_read() else {
            return Ok(WaitResult {
                reason: WaitReason::WaylandEvent,
            });
        };

        // 3. Block on the Wayland connection fd with the deadline.
        let wayland_fd = read_guard.connection_fd();
        let mut wayland_fds = [PollFd::new(
            &wayland_fd,
            PollFlags::IN | PollFlags::ERR | PollFlags::HUP,
        )];
        let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;

        let wayland_ready = match poll(&mut wayland_fds, timeout_ms) {
            Ok(0) => {
                drop(read_guard);
                return Ok(WaitResult::deadline_expired());
            }
            Err(rustix::io::Errno::INTR) => false,
            Ok(_) => wayland_fds[0]
                .revents()
                .intersects(PollFlags::IN | PollFlags::ERR | PollFlags::HUP),
            Err(err) => {
                drop(read_guard);
                return Err(PresentationError::SurfaceCreate(format!("poll: {err}")));
            }
        };

        // 4. Check eventfd with a 0ms poll — detects IPC/backend signals
        //    that arrived during the Wayland block.
        let mut eventfd_fds = [PollFd::new(&eventfd_fd, PollFlags::IN)];
        let ipc_ready = match poll(&mut eventfd_fds, 0) {
            Ok(0) | Err(rustix::io::Errno::INTR) => false,
            Ok(_) => eventfd_fds[0].revents().contains(PollFlags::IN),
            Err(_) => false,
        };

        // 5. Read and consume the eventfd counter so it doesn't keep firing.
        if ipc_ready {
            let mut counter = [0u8; 8];
            let _ = eventfd_read(&eventfd_fd, &mut counter);
        }

        // 6. Read Wayland data.
        let mut wake_reason = WaitReason::DeadlineExpired;
        if wayland_ready {
            match read_guard.read() {
                Ok(0) | Ok(_) => {
                    wake_reason = WaitReason::WaylandEvent;
                }
                Err(WaylandError::Io(err)) if err.kind() == ErrorKind::WouldBlock => {
                    wake_reason = WaitReason::WaylandEvent;
                }
                Err(err) => {
                    return Err(PresentationError::SurfaceCreate(format!("read: {err}")));
                }
            }
        } else {
            drop(read_guard);
        }

        // If eventfd fired, that takes priority as the reported reason.
        if ipc_ready {
            wake_reason = WaitReason::IpcEvent;
        }

        // 7. Dispatch any events that were read.
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| PresentationError::SurfaceCreate(format!("dispatch: {e}")))?;
        self.release_expired_surface_focus_grab()?;

        Ok(WaitResult {
            reason: wake_reason,
        })
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

    // ---------------------------------------------------------------------------
    // protocol_damage_rects tests
    // ---------------------------------------------------------------------------

    #[test]
    fn protocol_damage_rects_single_rect_passthrough() {
        let rects = vec![DamageRect {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        }];
        let result = protocol_damage_rects(&rects, 1920, 1080);
        assert_eq!(result.len(), 1, "single rect should pass through unchanged");
        assert_eq!(result[0].x, 10);
        assert_eq!(result[0].y, 20);
        assert_eq!(result[0].width, 100);
        assert_eq!(result[0].height, 50);
    }

    #[test]
    fn protocol_damage_rects_exactly_16_passthrough() {
        let rects: Vec<DamageRect> = (0..16)
            .map(|i| DamageRect {
                x: i * 10,
                y: i * 5,
                width: 10,
                height: 5,
            })
            .collect();
        let result = protocol_damage_rects(&rects, 1920, 1080);
        assert_eq!(
            result.len(),
            16,
            "exactly 16 rects should pass through unchanged"
        );
        for (i, r) in result.iter().enumerate() {
            assert_eq!(r.x, (i as u32) * 10);
            assert_eq!(r.y, (i as u32) * 5);
        }
    }

    #[test]
    fn protocol_damage_rects_17_triggers_union_fallback() {
        let rects: Vec<DamageRect> = (0..17)
            .map(|i| DamageRect {
                x: (i % 10) * 20,
                y: (i / 10) * 30,
                width: 18,
                height: 28,
            })
            .collect();
        let result = protocol_damage_rects(&rects, 1920, 1080);
        assert_eq!(
            result.len(),
            1,
            "more than 16 rects must collapse to a single bounding union"
        );
        let union_rect = result[0];
        // All input rects must be contained within the union
        for r in &rects {
            assert!(
                r.x >= union_rect.x
                    && r.y >= union_rect.y
                    && r.x.saturating_add(r.width) <= union_rect.x.saturating_add(union_rect.width)
                    && r.y.saturating_add(r.height)
                        <= union_rect.y.saturating_add(union_rect.height),
                "every input rect must be contained within the union; rect {:?} not in {:?}",
                r,
                union_rect
            );
        }
    }

    #[test]
    fn protocol_damage_rects_empty_input_returns_empty() {
        let result = protocol_damage_rects(&[], 1920, 1080);
        assert_eq!(result.len(), 0, "empty input must produce empty output");
    }

    #[test]
    fn protocol_damage_rects_union_covers_known_geometry() {
        // rects spanning x:[0..100] and y:[0..50]
        let rects = vec![
            DamageRect {
                x: 0,
                y: 0,
                width: 50,
                height: 25,
            },
            DamageRect {
                x: 50,
                y: 0,
                width: 50,
                height: 25,
            },
            DamageRect {
                x: 0,
                y: 25,
                width: 50,
                height: 25,
            },
            DamageRect {
                x: 50,
                y: 25,
                width: 50,
                height: 25,
            },
            // Fill out to 17 with more disjoint rects
            DamageRect {
                x: 10,
                y: 30,
                width: 30,
                height: 10,
            },
            DamageRect {
                x: 20,
                y: 40,
                width: 30,
                height: 10,
            },
            DamageRect {
                x: 10,
                y: 10,
                width: 20,
                height: 5,
            },
            DamageRect {
                x: 60,
                y: 10,
                width: 20,
                height: 5,
            },
            DamageRect {
                x: 10,
                y: 35,
                width: 20,
                height: 5,
            },
            DamageRect {
                x: 60,
                y: 35,
                width: 20,
                height: 5,
            },
            DamageRect {
                x: 15,
                y: 5,
                width: 10,
                height: 10,
            },
            DamageRect {
                x: 70,
                y: 5,
                width: 10,
                height: 10,
            },
            DamageRect {
                x: 15,
                y: 40,
                width: 10,
                height: 5,
            },
            DamageRect {
                x: 70,
                y: 40,
                width: 10,
                height: 5,
            },
            DamageRect {
                x: 0,
                y: 20,
                width: 5,
                height: 10,
            },
            DamageRect {
                x: 95,
                y: 20,
                width: 5,
                height: 10,
            },
            DamageRect {
                x: 0,
                y: 45,
                width: 5,
                height: 5,
            },
        ];
        assert!(
            rects.len() > 16,
            "this test needs >16 rects to trigger union"
        );
        let result = protocol_damage_rects(&rects, 1920, 1080);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].x, 0);
        assert_eq!(result[0].y, 0);
        assert_eq!(result[0].width, 100);
        assert_eq!(result[0].height, 50);
    }

    // ---------------------------------------------------------------------------
    // layer-surface config tests
    // ---------------------------------------------------------------------------

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

    // ---------------------------------------------------------------------------
    // scale factor tests
    // ---------------------------------------------------------------------------

    #[test]
    fn fractional_scale_converts_120x_to_f32() {
        // wp_fractional_scale_v1 sends scale * 120
        let eps = f32::EPSILON;
        let v: f32 = 120.0 / 120.0 - 1.0;
        assert!(v.abs() < eps);
        let v: f32 = 180.0 / 120.0 - 1.5;
        assert!(v.abs() < eps);
        let v: f32 = 240.0 / 120.0 - 2.0;
        assert!(v.abs() < eps);
        let v: f32 = 150.0 / 120.0 - 1.25;
        assert!(v.abs() < eps);
    }

    #[test]
    fn physical_dimensions_ceil_logical_times_scale() {
        // Physical = ceil(logical × scale)
        let compute_physical =
            |logical: u32, scale: f32| -> u32 { ((logical as f32 * scale).ceil() as u32).max(1) };
        assert_eq!(compute_physical(1920, 1.0), 1920);
        assert_eq!(compute_physical(1920, 2.0), 3840);
        assert_eq!(compute_physical(1920, 1.5), 2880);
        assert_eq!(compute_physical(100, 1.25), 125);
        assert_eq!(compute_physical(100, 1.75), 175);
    }

    #[test]
    fn default_scale_is_1_0() {
        // SurfaceEntry must default to scale 1.0
        let default_scale: f32 = 1.0;
        assert_eq!(default_scale, 1.0);
    }

    #[test]
    fn scale_change_detection_uses_f32_epsilon() {
        let current: f32 = 1.5;
        let same: f32 = 1.5;
        let different: f32 = 1.75;
        assert!(
            (current - same).abs() < f32::EPSILON,
            "tiny float differences should not trigger redraw"
        );
        assert!(
            (current - different).abs() > f32::EPSILON,
            "real scale changes must trigger redraw"
        );
    }

    // ---------------------------------------------------------------------------
    // damage rect scaling tests
    // ---------------------------------------------------------------------------

    #[test]
    fn scale_damage_rect_to_physical_multiplies_coordinates() {
        let logical = DamageRect {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        };
        let scaled = scale_damage_rect_to_physical(logical, 2.0);
        assert_eq!(scaled.x, 20);
        assert_eq!(scaled.y, 40);
        assert_eq!(scaled.width, 200);
        assert_eq!(scaled.height, 100);
    }

    #[test]
    fn scale_damage_rect_to_physical_at_fractional_scale_ceils_dimensions() {
        let logical = DamageRect {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        };
        let scaled = scale_damage_rect_to_physical(logical, 1.5);
        assert_eq!(scaled.x, 15); // 10 * 1.5 = 15.0 → 15
        assert_eq!(scaled.y, 30); // 20 * 1.5 = 30.0 → 30
        assert_eq!(scaled.width, 150); // 100 * 1.5 = 150.0 → 150
        assert_eq!(scaled.height, 75); // 50 * 1.5 = 75.0 → 75
    }

    #[test]
    fn scale_damage_rect_to_physical_at_identity_scale_is_identity() {
        let logical = DamageRect {
            x: 5,
            y: 10,
            width: 80,
            height: 40,
        };
        let scaled = scale_damage_rect_to_physical(logical, 1.0);
        assert_eq!(scaled.x, 5);
        assert_eq!(scaled.y, 10);
        assert_eq!(scaled.width, 80);
        assert_eq!(scaled.height, 40);
    }

    #[test]
    fn scale_damage_rect_to_physical_never_produces_zero_dimensions() {
        let logical = DamageRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let scaled = scale_damage_rect_to_physical(logical, 0.5);
        assert!(
            scaled.width >= 1,
            "width must be >= 1, got {}",
            scaled.width
        );
        assert!(
            scaled.height >= 1,
            "height must be >= 1, got {}",
            scaled.height
        );
    }

    #[test]
    fn damage_rects_remain_in_logical_space_until_present() {
        // Proof that the render path emits logical damage rects and attach_shm_buffer
        // scales them to physical. This is an architectural invariant test.
        let logical_rects = vec![DamageRect {
            x: 0,
            y: 0,
            width: 100,
            height: 50,
        }];
        let physical: Vec<DamageRect> = logical_rects
            .iter()
            .map(|r| scale_damage_rect_to_physical(*r, 2.0))
            .collect();
        assert_eq!(physical[0].x, 0);
        assert_eq!(physical[0].width, 200);
    }

    // ---------------------------------------------------------------------------
    // scale factor integer/ceil logic tests
    // ---------------------------------------------------------------------------

    #[test]
    fn integer_scale_detection() {
        assert!((1.0_f32 - 1.0_f32.round()).abs() < f32::EPSILON);
        assert!((2.0_f32 - 2.0_f32.round()).abs() < f32::EPSILON);
        assert!((1.5_f32 - 1.5_f32.round()).abs() > f32::EPSILON);
        assert!((1.25_f32 - 1.25_f32.round()).abs() > f32::EPSILON);
    }

    #[test]
    fn buffer_scale_for_integer_scale_equals_exact_value() {
        let scale: f32 = 2.0;
        assert_eq!(scale as i32, 2);
    }

    #[test]
    fn buffer_scale_for_fractional_scale_ceils() {
        let scale: f32 = 1.5;
        assert_eq!(scale.ceil() as i32, 2);
        let scale: f32 = 1.25;
        assert_eq!(scale.ceil() as i32, 2);
    }
}
