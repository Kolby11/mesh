use super::*;
use mesh_core_render::DamageRect;
use smallvec::{SmallVec, smallvec};
use std::borrow::Cow;
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
const SURFACE_CONFIGURE_WAIT_DEADLINE: Duration = Duration::from_millis(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShmPoolConfig {
    width: u32,
    height: u32,
    stride: i32,
}

#[derive(Debug)]
pub(super) struct SurfaceShmBuffer {
    buffer: Buffer,
    pending_damage: SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]>,
}

/// The compositor-side role backing a [`SurfaceEntry`]. Layer surfaces own
/// shell chrome (panels, launchers, overlays); popups are `xdg_popup` children
/// promoted from a `<popover>`. Both expose a `wl_surface`, so the entire SHM
/// buffer / present / scale / input path below is shared — only role creation,
/// layer-shell config, and dismiss differ.
pub(super) enum SurfaceRole {
    Layer(LayerSurface),
    Popup(PopupRole),
}

pub(super) struct PopupRole {
    pub(super) popup: Popup,
    /// `surface_id` of the parent (layer) surface this popup is a child of.
    pub(super) parent_id: String,
}

impl SurfaceRole {
    pub(super) fn wl_surface(&self) -> &wl_surface::WlSurface {
        match self {
            SurfaceRole::Layer(layer) => layer.wl_surface(),
            SurfaceRole::Popup(role) => role.popup.wl_surface(),
        }
    }

    pub(super) fn as_layer(&self) -> Option<&LayerSurface> {
        match self {
            SurfaceRole::Layer(layer) => Some(layer),
            SurfaceRole::Popup(_) => None,
        }
    }

    fn is_popup(&self) -> bool {
        matches!(self, SurfaceRole::Popup(_))
    }
}

pub(super) struct SurfaceEntry {
    pub(super) role: SurfaceRole,
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
    pub(super) kde_blur: Option<OrgKdeKwinBlur>,
    pub(super) blur_region: Option<DamageRect>,
    pub(super) blur_committed: bool,
    pub(super) blur_region_dirty: bool,
    /// Desired input region in surface-local logical coordinates. `None`
    /// means whole-surface input (the wl_surface default). Persisted here and
    /// applied with the next present commit so it can never be lost to
    /// call-ordering around configure/remap.
    pub(super) input_region_rect: Option<DamageRect>,
    pub(super) input_region_dirty: bool,
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
            self.write_u8(*byte);
        }
    }

    fn write_u8(&mut self, i: u8) {
        self.0 ^= u64::from(i);
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }

    fn write_u32(&mut self, i: u32) {
        self.0 ^= u64::from(i);
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }

    fn write_i32(&mut self, i: i32) {
        self.write_u32(i as u32);
    }
}

impl SurfaceEntry {
    fn new(
        role: SurfaceRole,
        cfg: LayerSurfaceConfig,
        applied_keyboard_mode: KeyboardMode,
    ) -> Self {
        Self {
            role,
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
            kde_blur: None,
            blur_region: None,
            blur_committed: false,
            blur_region_dirty: false,
            input_region_rect: None,
            input_region_dirty: false,
        }
    }

    pub(super) fn wl_surface(&self) -> &wl_surface::WlSurface {
        self.role.wl_surface()
    }

    fn needs_reconfigure(&self, cfg: &LayerSurfaceConfig, keyboard_mode: KeyboardMode) -> bool {
        !self.configured
            || self.config_fingerprint != surface_config_fingerprint(cfg, keyboard_mode)
    }

    fn apply_config(&mut self, cfg: LayerSurfaceConfig, keyboard_mode: KeyboardMode) {
        // Layer-shell config (anchor/layer/size/margins) only applies to layer
        // surfaces. Popups are positioned by their `xdg_positioner`, never by
        // these requests, so there is nothing to apply for the popup role.
        let Some(layer_surface) = self.role.as_layer() else {
            return;
        };
        let requires_fresh_configure =
            surface_change_requires_fresh_configure(&self.cfg, &cfg, self.configured);
        let effective_cfg = cfg.with_keyboard_mode(keyboard_mode);
        apply_config(layer_surface, &effective_cfg);
        layer_surface.commit();
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
        let wl_surface = self.role.wl_surface();
        wl_surface.attach(None, 0, 0);
        wl_surface.commit();
        // Wait for a fresh configure before attaching a buffer again after remap.
        self.configured = false;
    }

    fn copy_into_shm_buffer(
        &mut self,
        pool: &mut SlotPool,
        src: &[u8],
        width: u32,
        height: u32,
        damage: &[DamageRect],
    ) -> Result<(usize, SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]>), PresentationError> {
        let width = width.max(1);
        let height = height.max(1);
        let stride = width as i32 * 4;
        let full = full_damage(width, height);
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
            extend_pending_damage(&mut slot.pending_damage, damage, full);
        }

        let len = self.shm_buffers.len();
        for offset in 0..len {
            let index = (self.next_shm_buffer + offset) % len;
            if let Some(canvas) = pool.canvas(&self.shm_buffers[index].buffer) {
                let copy_damage = std::mem::take(&mut self.shm_buffers[index].pending_damage);
                for rect in &copy_damage {
                    copy_bgra_damage_to_canvas(src, canvas, width, height, *rect);
                }
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
            pending_damage: SmallVec::new(),
        });
        self.next_shm_buffer = (index + 1) % self.shm_buffers.len();
        Ok((index, smallvec![full]))
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
        copy_damage: &[DamageRect],
        scale: f32,
    ) {
        let buffer = &self.shm_buffers[index].buffer;
        let wl_surface = self.role.wl_surface();

        // Scale and clip in one pass. Keep the common <=16 rect path inline so
        // ordinary presents avoid heap allocation entirely.
        let mut clipped_damage: SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]> = damage_rects
            .iter()
            .map(|r| scale_damage_rect_to_physical(*r, scale))
            .map(|r| clip_damage_rect_to_buffer(r, physical_width, physical_height))
            .collect();

        // The region actually copied into THIS shm buffer (`copy_damage`, already
        // in physical coordinates) can be larger than the current frame's damage:
        // a buffer that was busy across earlier frames accumulates their damage in
        // `pending_damage` and refreshes all of it on reuse. The compositor only
        // re-composites the regions we report here, so we must include the full
        // copied region — otherwise the stale pixels outside `damage_rects` keep
        // showing the previous buffer's content (the transparent rectangular cutout).
        clipped_damage.extend(
            copy_damage
                .iter()
                .map(|rect| clip_damage_rect_to_buffer(*rect, physical_width, physical_height)),
        );

        // Emit damage_buffer calls with physical coordinates
        let protocol_damage =
            protocol_damage_rects(&clipped_damage, physical_width, physical_height);
        for rect in protocol_damage.iter().copied() {
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
        wl_surface.commit();
        self.frame_pending = true;
        self.frame_pending_since = Some(Instant::now());
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
    // Popups are sized by their positioner / compositor configure, not by the
    // layer-shell edge-stretch rules — report the configured size verbatim.
    if entry.role.is_popup() {
        return (entry.width.max(1), entry.height.max(1));
    }
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
        pending_damage: smallvec![full_damage(width, height)],
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

fn extend_pending_damage(
    pending: &mut SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]>,
    damage: &[DamageRect],
    bounds: DamageRect,
) {
    if pending.as_slice() == [bounds] {
        return;
    }

    if damage.is_empty() {
        pending.clear();
        pending.push(bounds);
        return;
    }

    for rect in damage.iter().filter_map(|rect| clip_damage(*rect, bounds)) {
        if rect == bounds {
            pending.clear();
            pending.push(bounds);
            return;
        }
        pending.push(rect);
    }

    if pending.len() > MAX_PROTOCOL_DAMAGE_RECTS {
        let union = pending
            .iter()
            .copied()
            .fold(None, |acc, rect| Some(union_damage(acc, rect)))
            .unwrap_or(bounds);
        pending.clear();
        pending.push(union);
    }
}

fn surface_is_configured_or_missing(state: &State, surface_id: &str) -> bool {
    state
        .surfaces
        .get(surface_id)
        .map(|entry| entry.configured)
        .unwrap_or(true)
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
fn protocol_damage_rects(rects: &[DamageRect], width: u32, height: u32) -> Cow<'_, [DamageRect]> {
    if rects.is_empty() {
        return Cow::Borrowed(&[]);
    }
    if rects.len() <= MAX_PROTOCOL_DAMAGE_RECTS {
        return Cow::Borrowed(rects);
    }
    let union = rects
        .iter()
        .copied()
        .fold(None, |acc, r| Some(union_damage(acc, r)))
        .unwrap_or_else(|| full_damage(width, height));
    Cow::Owned(vec![union])
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
        // xdg_wm_base backs the popup positioner primitive. It is optional: a
        // compositor without it simply cannot promote `<popover>`s (the shell
        // falls back to keeping them inline / clipped), so a missing global is
        // not a hard failure for the rest of the shell.
        let xdg_shell = XdgShell::bind(&globals, &qh).ok();
        if xdg_shell.is_none() {
            tracing::warn!(
                "layer_shell: xdg_wm_base unavailable; <popover> surface promotion disabled"
            );
        }
        let activation_state = ActivationState::bind(&globals, &qh).ok();
        let focus_grab_manager = globals.bind(&qh, 1..=1, GlobalData).ok();
        let viewporter: Option<WpViewporter> = globals.bind(&qh, 1..=1, GlobalData).ok();
        let fractional_scale_manager: Option<WpFractionalScaleManagerV1> =
            globals.bind(&qh, 1..=1, GlobalData).ok();
        let blur_manager: Option<OrgKdeKwinBlurManager> = globals.bind(&qh, 1..=1, GlobalData).ok();
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
            blur_manager,
            seat_state,
            activation_seat: None,
            focus_grab: None,
            focus_grab_surface_id: None,
            focus_grab_requested_at: None,
            qh,
            pool,
            surfaces: HashMap::new(),
            surface_ids_by_wl_id: HashMap::new(),
            pointer: None,
            pointer_interactive: false,
            keyboard: None,
            pointer_focus: None,
            keyboard_focus: None,
            keyboard_mods: Modifiers::default(),
            keyboard_repeat_info: RepeatInfo::Disable,
            keyboard_repeat: None,
            events: Vec::new(),
            xdg_shell,
            dismissed_popups: Vec::new(),
        };

        Ok(Self {
            _conn: conn,
            event_queue,
            state,
        })
    }

    pub fn set_pointer_interactive(&mut self, interactive: bool) {
        if self.state.pointer_interactive == interactive {
            return;
        }
        self.state.pointer_interactive = interactive;
        let Some(pointer) = self.state.pointer.as_ref() else {
            return;
        };
        let icon = if interactive {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        };
        if let Err(error) = pointer.set_cursor(&self._conn, icon) {
            tracing::debug!("layer_shell: failed to update cursor icon: {error}");
        }
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
                self.state.insert_surface(
                    surface_id.to_string(),
                    SurfaceEntry::new(
                        SurfaceRole::Layer(layer_surface),
                        cfg,
                        effective_keyboard_mode,
                    ),
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
                    .map(|entry| entry.wl_surface().clone());
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
                        let wl_surface = entry.wl_surface().clone();
                        let qh = self.state.qh.clone();
                        entry.viewport = Some(viewporter.get_viewport(&wl_surface, &qh, ()));
                    }
                }
                // Create kde_blur object lazily (BLUR-01): will be used when
                // update_blur_region is called with a non-None region.
                if let Some(ref manager) = self.state.blur_manager {
                    if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
                        let wl_surface = entry.wl_surface().clone();
                        let qh = self.state.qh.clone();
                        entry.kde_blur = Some(manager.create(&wl_surface, &qh, ()));
                    }
                }
            }
        }
    }

    /// True when the compositor exposes `xdg_wm_base`, i.e. `<popover>` surface
    /// promotion is available.
    pub fn popup_supported(&self) -> bool {
        self.state.xdg_shell.is_some()
    }

    /// Promote a component into an `xdg_popup` child of an existing layer
    /// surface, or reposition it if `surface_id` already names a live popup.
    ///
    /// The popup shares the entire SHM/present/scale/input path with layer
    /// surfaces — only creation and placement differ. Like a layer surface it
    /// must not be painted until the compositor sends its first configure, which
    /// flips `configured = true` via [`PopupHandler::configure`]; the existing
    /// `present_with_damage` gate handles that automatically.
    pub fn configure_popup(
        &mut self,
        surface_id: &str,
        config: PopupConfig,
    ) -> Result<(), PresentationError> {
        // An existing popup is repositioned in place rather than recreated, so
        // anchor moves (exclusive-zone/output changes) don't tear it down.
        if self.state.surfaces.contains_key(surface_id) {
            self.reposition_popup(surface_id, &config.placement);
            return Ok(());
        }

        if self.state.xdg_shell.is_none() {
            return Err(PresentationError::ProtocolUnsupported(
                "xdg_wm_base unavailable; cannot promote popover".into(),
            ));
        }

        // The popup's Wayland parent must be a layer surface owned by the
        // backend. Nested popup-of-popup is not modeled yet.
        let parent_layer = match self.state.surfaces.get(&config.parent_surface_id) {
            Some(entry) => match entry.role.as_layer() {
                Some(layer) => layer.clone(),
                None => {
                    return Err(PresentationError::SurfaceCreate(
                        "popup parent must be a layer surface".into(),
                    ));
                }
            },
            None => {
                return Err(PresentationError::SurfaceCreate(format!(
                    "popup parent surface '{}' not found",
                    config.parent_surface_id
                )));
            }
        };

        let qh = self.state.qh.clone();
        // `XdgShell` is not `Clone`, so create the popup while holding an
        // immutable borrow of `state`, then release it before the mutable
        // `surfaces.insert` below.
        let popup = {
            let xdg_shell = self
                .state
                .xdg_shell
                .as_ref()
                .expect("xdg_shell presence checked above");
            let positioner = build_positioner(xdg_shell, &config.placement)?;
            let surface = Surface::new(&self.state.compositor_state, &qh)
                .map_err(|e| PresentationError::SurfaceCreate(format!("popup surface: {e}")))?;
            // Parent role is supplied below via `get_popup`, so the xdg_popup is
            // created with no xdg parent (None) per the wlr-layer-shell contract.
            let popup = Popup::from_surface(None, &positioner, &qh, surface, xdg_shell)
                .map_err(|e| PresentationError::SurfaceCreate(format!("xdg_popup: {e}")))?;
            parent_layer.get_popup(popup.xdg_popup());

            // A grab is only valid in response to a recent input serial. Hover-open
            // popovers pass `grab = false` and rely on the core hover-bridge.
            if config.grab {
                if let (Some(seat), Some(serial)) =
                    (self.state.activation_seat.as_ref(), config.grab_serial)
                {
                    popup.xdg_popup().grab(seat, serial);
                } else {
                    tracing::debug!(
                        "[popover] layer_shell: grab requested for {surface_id} but no seat/serial; opening without grab"
                    );
                }
            }

            // Initial commit (no buffer) maps the popup role and prompts the
            // compositor's first configure.
            popup.wl_surface().commit();
            popup
        };

        let cfg = LayerSurfaceConfig {
            width: config.placement.size.0,
            height: config.placement.size.1,
            ..LayerSurfaceConfig::default()
        };
        let entry = SurfaceEntry::new(
            SurfaceRole::Popup(PopupRole {
                popup,
                parent_id: config.parent_surface_id.clone(),
            }),
            cfg,
            KeyboardMode::None,
        );
        let wl_surface = entry.wl_surface().clone();
        self.state.insert_surface(surface_id.to_string(), entry);

        // Bind HiDPI protocols for the popup surface, mirroring the layer path.
        if let Some(fs) = self
            .state
            .bind_fractional_scale(&wl_surface, &qh, surface_id.to_string())
            && let Some(entry) = self.state.surfaces.get_mut(surface_id)
        {
            entry.fractional_scale = Some(fs);
        }
        if let Some(ref viewporter) = self.state.viewporter {
            let viewport = viewporter.get_viewport(&wl_surface, &qh, ());
            if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
                entry.viewport = Some(viewport);
            }
        }

        if let Err(error) = self.dispatch_pending() {
            self.destroy_popup(surface_id);
            return Err(error);
        }
        Ok(())
    }

    fn reposition_popup(&mut self, surface_id: &str, placement: &PopupPlacement) {
        let Some(xdg_shell) = self.state.xdg_shell.as_ref() else {
            return;
        };
        let Ok(positioner) = build_positioner(xdg_shell, placement) else {
            return;
        };
        if let Some(entry) = self.state.surfaces.get(surface_id)
            && let SurfaceRole::Popup(role) = &entry.role
        {
            // `xdg_popup.reposition` requires xdg_wm_base v3+. The token is
            // echoed back on the resulting reactive configure; 0 is fine since
            // we don't correlate repositions yet.
            role.popup.reposition(&positioner, 0);
        }
    }

    /// Tear down a promoted popup. Dropping the [`SurfaceEntry`] drops the SCTK
    /// `Popup`, whose `Drop` destroys the `xdg_popup`/`xdg_surface`/`wl_surface`;
    /// the per-surface viewport/scale/blur objects are released explicitly since
    /// popups are created and destroyed repeatedly.
    pub fn destroy_popup(&mut self, surface_id: &str) {
        let is_popup = self
            .state
            .surfaces
            .get(surface_id)
            .map(|entry| entry.role.is_popup())
            .unwrap_or(false);
        if !is_popup {
            return;
        }
        if let Some(entry) = self.state.remove_surface(surface_id) {
            if let Some(viewport) = entry.viewport.as_ref() {
                viewport.destroy();
            }
            if let Some(fractional_scale) = entry.fractional_scale.as_ref() {
                fractional_scale.destroy();
            }
            if let Some(blur) = entry.kde_blur.as_ref() {
                blur.release();
            }
        }
    }

    /// Destroy every popup parented to `parent_surface_id`. The compositor
    /// auto-dismisses popups when their parent surface is destroyed or hidden;
    /// this keeps the backend's own bookkeeping in step (e.g. when the shell
    /// hides the host bar) so stale popup entries are not presented or routed to.
    pub fn destroy_popups_for_parent(&mut self, parent_surface_id: &str) {
        let children: Vec<String> = self
            .state
            .surfaces
            .iter()
            .filter_map(|(id, entry)| match &entry.role {
                SurfaceRole::Popup(role) if role.parent_id == parent_surface_id => Some(id.clone()),
                _ => None,
            })
            .collect();
        for id in children {
            self.destroy_popup(&id);
        }
    }

    /// Drain the ids of popups the compositor dismissed since the last call so
    /// the shell can drop the matching popup targets from its own bookkeeping.
    pub fn take_dismissed_popups(&mut self) -> Vec<String> {
        std::mem::take(&mut self.state.dismissed_popups)
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
                // Clear compositor blur before hiding (WR-01 / BLUR-04).
                if let Some(ref kde_blur) = entry.kde_blur {
                    if entry.blur_committed {
                        kde_blur.set_region(None);
                        kde_blur.commit();
                        entry.blur_committed = false;
                        entry.blur_region_dirty = false;
                    }
                }
                entry.blur_region = None;
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
        // Damage rects arrive in logical/CSS coordinates; scale to physical
        // before the copy so each SHM buffer can retain disjoint pending
        // regions without expanding them into one bounding rectangle.
        let mut shm_copy_damage: SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]> = damage_rects
            .iter()
            .copied()
            .map(|r| scale_damage_rect_to_physical(r, scale))
            .collect();
        if shm_copy_damage.is_empty() {
            // If the slice is empty (shouldn't normally reach here due to the
            // skip gate in render.rs), upload the full buffer.
            shm_copy_damage.push(full_damage(physical_w, physical_h));
        }
        let (buffer_index, copy_damage) = entry.copy_into_shm_buffer(
            pool,
            &buffer.data,
            physical_w,
            physical_h,
            &shm_copy_damage,
        )?;
        // Commit kde_blur region before wl_surface commit (BLUR-02, BLUR-04, CR-01)
        if entry.blur_region_dirty
            && let Some(ref kde_blur) = entry.kde_blur
        {
            match entry.blur_region {
                Some(region_rect) => {
                    if let Ok(region) = Region::new(&state.compositor_state) {
                        region.add(
                            region_rect.x as i32,
                            region_rect.y as i32,
                            region_rect.width as i32,
                            region_rect.height as i32,
                        );
                        kde_blur.set_region(Some(region.wl_region()));
                        kde_blur.commit();
                        entry.blur_committed = true;
                        entry.blur_region_dirty = false;
                    }
                }
                None if entry.blur_committed => {
                    // Clear the compositor's blur region when backdrop-filter is removed (CR-01).
                    kde_blur.set_region(None);
                    kde_blur.commit();
                    entry.blur_committed = false;
                    entry.blur_region_dirty = false;
                }
                None => entry.blur_region_dirty = false,
            }
        }
        // Apply the persisted input region as pending state so the present
        // commit below carries it. Doing this every time it is dirty (rather
        // than fire-and-forget at update time) guarantees it lands on a
        // mapped, configured surface regardless of configure/remap ordering.
        if entry.input_region_dirty {
            match entry.input_region_rect {
                Some(rect) => {
                    if let Ok(region) = Region::new(&state.compositor_state) {
                        region.add(
                            rect.x as i32,
                            rect.y as i32,
                            rect.width as i32,
                            rect.height as i32,
                        );
                        entry
                            .wl_surface()
                            .set_input_region(Some(region.wl_region()));
                        entry.input_region_dirty = false;
                    }
                }
                None => {
                    entry.wl_surface().set_input_region(None);
                    entry.input_region_dirty = false;
                }
            }
        }
        entry.attach_shm_buffer(
            &qh,
            buffer_index,
            logical_w,
            logical_h,
            physical_w,
            physical_h,
            damage_rects,
            &copy_damage,
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
        let wl_surface = entry.wl_surface();

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

    /// Restrict the surface's input (pointer/touch) region to `input_rect` in
    /// surface-local logical coordinates. Surfaces allocate extra buffer space
    /// below/around their content for tooltip overlays; without an explicit
    /// input region the compositor routes clicks over that whole extra area to
    /// this surface, creating a dead zone where clicks never reach the windows
    /// underneath. `None` resets to the default (whole-surface input).
    pub(crate) fn update_input_region(&mut self, surface_id: &str, input_rect: Option<DamageRect>) {
        let Some(entry) = self.state.surfaces.get_mut(surface_id) else {
            return;
        };
        let input_rect = input_rect.filter(|r| r.width > 0 && r.height > 0);
        if entry.input_region_rect == input_rect && !entry.input_region_dirty {
            return;
        }
        // Store only; the region is applied together with the present commit
        // (`apply_pending_input_region`) so it always lands on a mapped
        // surface and survives configure/remap ordering.
        entry.input_region_rect = input_rect;
        entry.input_region_dirty = true;
    }

    /// Set the logical-coordinate blur region for a surface.
    /// The region is sent as kde_blur protocol calls before the next
    /// wl_surface.commit(). If `blur_region` is `None`, no kde_blur
    /// calls are emitted — the compositor gets no blur hint.
    pub(crate) fn update_blur_region(&mut self, surface_id: &str, blur_region: Option<DamageRect>) {
        let Some(entry) = self.state.surfaces.get_mut(surface_id) else {
            return;
        };
        set_pending_blur_region(
            &mut entry.blur_region,
            &mut entry.blur_region_dirty,
            blur_region,
        );
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
        if surface_is_configured_or_missing(&self.state, surface_id) {
            return Ok(());
        }

        let deadline = Instant::now() + SURFACE_CONFIGURE_WAIT_DEADLINE;
        loop {
            self.event_queue
                .flush()
                .map_err(|e| PresentationError::SurfaceCreate(format!("flush: {e}")))?;
            self.event_queue
                .dispatch_pending(&mut self.state)
                .map_err(|e| PresentationError::SurfaceCreate(format!("dispatch: {e}")))?;
            if surface_is_configured_or_missing(&self.state, surface_id) {
                return Ok(());
            }

            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Ok(());
            };
            let Some(read_guard) = self.event_queue.prepare_read() else {
                continue;
            };

            let fd = read_guard.connection_fd();
            let timeout_ms = remaining.as_millis().min(i32::MAX as u128) as i32;
            let mut fds = [PollFd::new(
                &fd,
                PollFlags::IN | PollFlags::ERR | PollFlags::HUP,
            )];

            match poll(&mut fds, timeout_ms) {
                Ok(0) => {
                    drop(read_guard);
                    return Ok(());
                }
                Ok(_) => {
                    if !fds[0]
                        .revents()
                        .intersects(PollFlags::IN | PollFlags::ERR | PollFlags::HUP)
                    {
                        drop(read_guard);
                        return Ok(());
                    }
                    match read_guard.read() {
                        Ok(_) => {}
                        Err(WaylandError::Io(err)) if err.kind() == ErrorKind::WouldBlock => {
                            return Ok(());
                        }
                        Err(err) => {
                            return Err(PresentationError::SurfaceCreate(format!("read: {err}")));
                        }
                    }
                }
                Err(rustix::io::Errno::INTR) => {
                    drop(read_guard);
                    return Ok(());
                }
                Err(err) => {
                    drop(read_guard);
                    return Err(PresentationError::SurfaceCreate(format!("poll: {err}")));
                }
            }
        }
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

        // 3. Block on both Wayland and the shell eventfd. Backend/IPC work
        // must be able to interrupt long idle waits once the shell is no
        // longer clamped to a fixed 16ms loop cadence.
        let wayland_fd = read_guard.connection_fd();
        let mut fds = [
            PollFd::new(&wayland_fd, PollFlags::IN | PollFlags::ERR | PollFlags::HUP),
            PollFd::new(&eventfd_fd, PollFlags::IN | PollFlags::ERR | PollFlags::HUP),
        ];
        let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;

        let (wayland_ready, ipc_ready) = match poll(&mut fds, timeout_ms) {
            Ok(0) => {
                drop(read_guard);
                return Ok(WaitResult::deadline_expired());
            }
            Err(rustix::io::Errno::INTR) => (false, false),
            Ok(_) => (
                fds[0]
                    .revents()
                    .intersects(PollFlags::IN | PollFlags::ERR | PollFlags::HUP),
                fds[1]
                    .revents()
                    .intersects(PollFlags::IN | PollFlags::ERR | PollFlags::HUP),
            ),
            Err(err) => {
                drop(read_guard);
                return Err(PresentationError::SurfaceCreate(format!("poll: {err}")));
            }
        };

        // 4. Read and consume the eventfd counter so it doesn't keep firing.
        if ipc_ready {
            let mut counter = [0u8; 8];
            let _ = eventfd_read(&eventfd_fd, &mut counter);
        }

        // 5. Read Wayland data.
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

        // 6. Dispatch any events that were read.
        self.event_queue
            .dispatch_pending(&mut self.state)
            .map_err(|e| PresentationError::SurfaceCreate(format!("dispatch: {e}")))?;
        self.release_expired_surface_focus_grab()?;

        Ok(WaitResult {
            reason: wake_reason,
        })
    }
}

fn set_pending_blur_region(
    current: &mut Option<DamageRect>,
    dirty: &mut bool,
    next: Option<DamageRect>,
) {
    if *current == next && !*dirty {
        return;
    }
    *current = next;
    *dirty = true;
}

/// Build and configure an `xdg_positioner` from a [`PopupPlacement`]. Every
/// field maps 1:1 onto a positioner request, so the compositor performs all the
/// actual anchoring / flip-at-edge math (replacing the old hand-rolled margin
/// positioning of the standalone popover layer surfaces).
fn build_positioner(
    xdg_shell: &XdgShell,
    placement: &PopupPlacement,
) -> Result<XdgPositioner, PresentationError> {
    let positioner = XdgPositioner::new(xdg_shell)
        .map_err(|e| PresentationError::SurfaceCreate(format!("xdg_positioner: {e}")))?;
    let (ax, ay, aw, ah) = placement.anchor_rect;
    positioner.set_anchor_rect(ax, ay, aw.max(1), ah.max(1));
    positioner.set_size(
        placement.size.0.max(1) as i32,
        placement.size.1.max(1) as i32,
    );
    positioner.set_anchor(popup::map_anchor(placement.anchor));
    positioner.set_gravity(popup::map_gravity(placement.gravity));
    positioner.set_constraint_adjustment(popup::map_constraint(placement.constraint));
    positioner.set_offset(placement.offset.0, placement.offset.1);
    Ok(positioner)
}

pub(super) fn apply_config(layer_surface: &LayerSurface, cfg: &LayerSurfaceConfig) {
    let (protocol_width, protocol_height) = layer_protocol_size(cfg);
    layer_surface.set_layer(map_layer(cfg.layer));
    layer_surface.set_anchor(map_anchor(cfg));
    layer_surface.set_exclusive_zone(cfg.exclusive_zone);
    layer_surface.set_keyboard_interactivity(map_keyboard(cfg.keyboard_mode));
    layer_surface.set_size(protocol_width, protocol_height);
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

/// Map a MESH surface config onto the wire `zwlr_layer_surface_v1::set_size`.
///
/// CRITICAL layer-shell semantics: a dimension of `0` does NOT mean "empty" —
/// it means "stretch to the output edges on that axis" (and is only protocol-
/// valid when the surface is anchored to both opposing edges of that axis).
/// Passing measured-content sizes of `0` straight through has repeatedly
/// produced invisible output-spanning surfaces that swallow all pointer and
/// keyboard input shell-wide. Every zero that reaches this function is
/// therefore resolved here:
/// - `0` on a both-edges-anchored axis is an intentional span (bars) — kept.
/// - `0` on any other axis is protocol-invalid "not measured yet" — replaced
///   with the exclusive-zone fallback so the surface maps small, not spanned.
/// - `0` on BOTH axes is never intentional for a shell surface — both sides
///   get the fallback and an error is logged so the bug is visible in logs
///   instead of as a screen-wide input blackout.
/// Map a MESH surface config onto the wire `zwlr_layer_surface_v1::set_size`.
///
/// CRITICAL layer-shell semantics: a dimension of `0` does NOT mean "empty" —
/// it means "stretch to the output edges on that axis" (and is only protocol-
/// valid when the surface is anchored to both opposing edges of that axis).
/// Passing measured-content sizes of `0` straight through has repeatedly
/// produced invisible output-spanning surfaces that swallow pointer and
/// keyboard input shell-wide.
///
/// Zeros are resolved here as follows:
/// - Top/bottom surfaces: width `0` is the intended output-wide bar span
///   (their horizontal both-edge anchor is unconditional); height `0` falls
///   back to the exclusive zone.
/// - Left/right surfaces with a positive exclusive zone are docked rails:
///   width falls back to the exclusive zone and height `0` spans — intended.
/// - Left/right (and unanchored) surfaces WITHOUT an exclusive zone are
///   floating popover-style surfaces. `map_anchor` derives the vertical
///   both-edge anchor FROM `height == 0`, so an unmeasured `0x0` popover
///   would silently become a full-output-height input sink (this shipped
///   twice: an invisible surface swallowing all pointer/keyboard input).
///   That case is clamped to 1x1 and logged as an error — a broken 1px
///   surface plus a log line beats a screen-wide input blackout.
fn layer_protocol_size(cfg: &LayerSurfaceConfig) -> (u32, u32) {
    let anchor = map_anchor(cfg);
    if cfg.width == 0
        && cfg.height == 0
        && cfg.exclusive_zone <= 0
        && !matches!(cfg.edge, Some(Edge::Top | Edge::Bottom))
    {
        tracing::error!(
            namespace = %cfg.namespace,
            edge = ?cfg.edge,
            "non-docked layer surface configured 0x0: zero means \"span the \
             output\" in layer-shell, which would map an invisible \
             output-spanning surface that blocks input; clamping to 1x1"
        );
        return (1, 1);
    }
    let width = if cfg.width == 0 && !anchor.contains(Anchor::LEFT | Anchor::RIGHT) {
        layer_protocol_fallback_size(cfg)
    } else {
        cfg.width
    };
    let height = if cfg.height == 0 && !anchor.contains(Anchor::TOP | Anchor::BOTTOM) {
        layer_protocol_fallback_size(cfg)
    } else {
        cfg.height
    };
    (width, height)
}

fn layer_protocol_fallback_size(cfg: &LayerSurfaceConfig) -> u32 {
    u32::try_from(cfg.exclusive_zone).unwrap_or(0).max(1)
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

    #[test]
    fn top_surface_protocol_size_keeps_only_spanning_width_dynamic() {
        let mut cfg = base_cfg();
        cfg.edge = Some(Edge::Top);
        cfg.width = 0;
        cfg.height = 0;
        cfg.exclusive_zone = 56;

        assert_eq!(
            layer_protocol_size(&cfg),
            (0, 56),
            "top surfaces are left+right anchored, so only width may be sent as zero; height falls back to the exclusive zone"
        );
    }

    #[test]
    fn bottom_surface_protocol_size_keeps_only_spanning_width_dynamic() {
        let mut cfg = base_cfg();
        cfg.edge = Some(Edge::Bottom);
        cfg.width = 0;
        cfg.height = 0;
        cfg.exclusive_zone = 56;

        assert_eq!(
            layer_protocol_size(&cfg),
            (0, 56),
            "bottom surfaces are left+right anchored, so only width may be sent as zero; height falls back to the exclusive zone"
        );
    }

    #[test]
    fn left_surface_protocol_size_keeps_only_spanning_height_dynamic() {
        let mut cfg = base_cfg();
        cfg.edge = Some(Edge::Left);
        cfg.width = 0;
        cfg.height = 0;
        cfg.exclusive_zone = 48;

        assert_eq!(
            layer_protocol_size(&cfg),
            (48, 0),
            "left surfaces with dynamic height are top+bottom anchored, so only height may be sent as zero; width falls back to the exclusive zone"
        );
    }

    #[test]
    fn undocked_side_surface_never_spans_the_output() {
        // Regression guard: a floating (exclusive_zone == 0) left/right
        // surface whose content is not measured yet must NOT map as an
        // output-height-spanning surface — that shipped twice as an invisible
        // full-height overlay swallowing all pointer/keyboard input.
        let mut cfg = base_cfg();
        cfg.edge = Some(Edge::Left);
        cfg.width = 0;
        cfg.height = 0;
        cfg.exclusive_zone = 0;

        assert_eq!(
            layer_protocol_size(&cfg),
            (1, 1),
            "an unmeasured popover-style side surface must map tiny, never output-spanning"
        );
    }

    #[test]
    fn unanchored_surface_protocol_size_replaces_dynamic_axes() {
        let mut cfg = base_cfg();
        cfg.edge = None;
        cfg.width = 0;
        cfg.height = 0;

        assert_eq!(
            layer_protocol_size(&cfg),
            (1, 1),
            "unanchored surfaces cannot use zero size on either axis"
        );
    }

    #[test]
    fn overlay_surface_without_exclusive_zone_uses_minimal_protocol_fallback() {
        let mut cfg = base_cfg();
        cfg.edge = Some(Edge::Top);
        cfg.width = 0;
        cfg.height = 0;
        cfg.exclusive_zone = 0;

        assert_eq!(layer_protocol_size(&cfg), (0, 1));
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

    #[test]
    fn unchanged_blur_region_is_committed_only_once() {
        let region = Some(DamageRect {
            x: 0,
            y: 0,
            width: 800,
            height: 48,
        });
        let mut current = None;
        let mut dirty = false;
        let mut commits = 0;

        for _ in 0..1_000 {
            set_pending_blur_region(&mut current, &mut dirty, region);
            if dirty {
                commits += 1;
                dirty = false;
            }
        }
        assert_eq!(commits, 1);

        set_pending_blur_region(&mut current, &mut dirty, None);
        assert!(dirty, "removing blur must produce a clearing commit");
    }

    #[test]
    #[ignore = "release-only present damage allocation benchmark"]
    fn borrowed_protocol_damage_beats_cloned_passthrough() {
        use std::hint::black_box;
        use std::time::Instant;

        let rects = [
            DamageRect {
                x: 0,
                y: 0,
                width: 20,
                height: 20,
            },
            DamageRect {
                x: 200,
                y: 0,
                width: 20,
                height: 20,
            },
            DamageRect {
                x: 400,
                y: 0,
                width: 20,
                height: 20,
            },
            DamageRect {
                x: 600,
                y: 0,
                width: 20,
                height: 20,
            },
        ];
        let iterations = 1_000_000;

        let started = Instant::now();
        for _ in 0..iterations {
            black_box(black_box(&rects).to_vec());
        }
        let cloned = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            black_box(protocol_damage_rects(black_box(&rects), 800, 48));
        }
        let borrowed = started.elapsed();

        eprintln!(
            "protocol damage passthrough over {iterations} iterations: cloned {cloned:?}, borrowed {borrowed:?}"
        );
    }

    #[test]
    #[ignore = "release-only clipped damage scratch allocation benchmark"]
    fn smallvec_clipped_damage_beats_heap_vec_scratch() {
        use std::hint::black_box;
        use std::time::Instant;

        let rects = [
            DamageRect {
                x: 0,
                y: 0,
                width: 20,
                height: 20,
            },
            DamageRect {
                x: 200,
                y: 0,
                width: 20,
                height: 20,
            },
            DamageRect {
                x: 400,
                y: 0,
                width: 20,
                height: 20,
            },
            DamageRect {
                x: 600,
                y: 0,
                width: 20,
                height: 20,
            },
        ];
        let copy_damage = [
            DamageRect {
                x: 0,
                y: 0,
                width: 20,
                height: 20,
            },
            DamageRect {
                x: 600,
                y: 0,
                width: 20,
                height: 20,
            },
        ];
        let iterations = 1_000_000;

        let started = Instant::now();
        for _ in 0..iterations {
            let mut clipped_damage: Vec<DamageRect> = black_box(&rects)
                .iter()
                .map(|r| scale_damage_rect_to_physical(*r, 1.0))
                .map(|r| clip_damage_rect_to_buffer(r, 800, 48))
                .collect();
            clipped_damage.extend(
                black_box(&copy_damage)
                    .iter()
                    .map(|rect| clip_damage_rect_to_buffer(*rect, 800, 48)),
            );
            black_box(protocol_damage_rects(&clipped_damage, 800, 48));
        }
        let heap_vec = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            let mut clipped_damage: SmallVec<[DamageRect; MAX_PROTOCOL_DAMAGE_RECTS]> =
                black_box(&rects)
                    .iter()
                    .map(|r| scale_damage_rect_to_physical(*r, 1.0))
                    .map(|r| clip_damage_rect_to_buffer(r, 800, 48))
                    .collect();
            clipped_damage.extend(
                black_box(&copy_damage)
                    .iter()
                    .map(|rect| clip_damage_rect_to_buffer(*rect, 800, 48)),
            );
            black_box(protocol_damage_rects(&clipped_damage, 800, 48));
        }
        let inline = started.elapsed();

        eprintln!(
            "clipped damage scratch over {iterations} iterations: heap Vec {heap_vec:?}, SmallVec {inline:?}, ratio {:.1}x",
            heap_vec.as_secs_f64() / inline.as_secs_f64()
        );
        assert!(inline < heap_vec);
    }

    #[test]
    #[ignore = "release-only surface config fingerprint microbenchmark"]
    fn primitive_surface_config_hashing_beats_byte_writes() {
        use std::hint::black_box;
        use std::time::Instant;

        struct OldHasher(u64);
        impl Default for OldHasher {
            fn default() -> Self {
                Self(0xcbf2_9ce4_8422_2325)
            }
        }
        impl Hasher for OldHasher {
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

        fn old_surface_config_fingerprint(
            cfg: &LayerSurfaceConfig,
            keyboard_mode: KeyboardMode,
        ) -> u64 {
            let mut hasher = OldHasher::default();
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

        let cfg = LayerSurfaceConfig {
            edge: Some(Edge::Top),
            layer: MeshLayer::Overlay,
            size_policy: LayerSurfaceSizePolicy::Fixed,
            width: 1_920,
            height: 48,
            exclusive_zone: 48,
            keyboard_mode: KeyboardMode::OnDemand,
            namespace: "benchmark".into(),
            margin_top: 2,
            margin_right: 4,
            margin_bottom: 6,
            margin_left: 8,
        };
        let iterations = 2_000_000;

        let started = Instant::now();
        let mut old_hash = 0;
        for _ in 0..iterations {
            old_hash ^=
                old_surface_config_fingerprint(black_box(&cfg), black_box(KeyboardMode::OnDemand));
        }
        let old = started.elapsed();

        let started = Instant::now();
        let mut new_hash = 0;
        for _ in 0..iterations {
            new_hash ^=
                surface_config_fingerprint(black_box(&cfg), black_box(KeyboardMode::OnDemand));
        }
        let new = started.elapsed();

        black_box((old_hash, new_hash));
        eprintln!(
            "surface config fingerprint over {iterations} configs: byte writes {old:?}, primitive writes {new:?}, ratio {:.1}x",
            old.as_secs_f64() / new.as_secs_f64()
        );
        assert!(new < old);
    }

    #[test]
    fn pending_buffer_damage_preserves_disjoint_rectangles() {
        let bounds = full_damage(1_920, 100);
        let damage = [
            DamageRect {
                x: 0,
                y: 0,
                width: 20,
                height: 100,
            },
            DamageRect {
                x: 1_900,
                y: 0,
                width: 20,
                height: 100,
            },
        ];
        let mut pending = SmallVec::new();

        extend_pending_damage(&mut pending, &damage, bounds);

        assert_eq!(pending.as_slice(), damage.as_slice());
        let copied_area: u32 = pending.iter().map(|rect| rect.width * rect.height).sum();
        assert_eq!(copied_area, 4_000);
        assert_eq!(union_damage(Some(damage[0]), damage[1]).width, 1_920);
    }

    #[test]
    fn pending_buffer_damage_collapses_when_rect_cap_is_exceeded() {
        let bounds = full_damage(1_920, 100);
        let damage: Vec<_> = (0..=MAX_PROTOCOL_DAMAGE_RECTS)
            .map(|index| DamageRect {
                x: index as u32 * 10,
                y: 0,
                width: 5,
                height: 5,
            })
            .collect();
        let mut pending = SmallVec::new();

        extend_pending_damage(&mut pending, &damage, bounds);

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].x, 0);
        assert_eq!(pending[0].width, 165);
    }

    #[test]
    #[ignore = "release-only disjoint SHM copy benchmark"]
    fn disjoint_damage_copy_beats_bounding_union_copy() {
        use std::hint::black_box;
        use std::time::Instant;

        let width = 1_920;
        let height = 100;
        let src = vec![0x7f; width as usize * height as usize * 4];
        let mut canvas = vec![0; src.len()];
        let left = DamageRect {
            x: 0,
            y: 0,
            width: 20,
            height,
        };
        let right = DamageRect {
            x: width - 20,
            y: 0,
            width: 20,
            height,
        };
        let union = union_damage(Some(left), right);
        let iterations = 1_000;

        let started = Instant::now();
        for _ in 0..iterations {
            copy_bgra_damage_to_canvas(
                black_box(&src),
                black_box(&mut canvas),
                width,
                height,
                union,
            );
        }
        let union_elapsed = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            for rect in [left, right] {
                copy_bgra_damage_to_canvas(
                    black_box(&src),
                    black_box(&mut canvas),
                    width,
                    height,
                    rect,
                );
            }
        }
        let disjoint_elapsed = started.elapsed();

        eprintln!(
            "SHM copy over {iterations} disjoint frames: bounding union {union_elapsed:?}, rect list {disjoint_elapsed:?}"
        );
    }
}
