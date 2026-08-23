use super::*;
use mesh_core_render::DamageRect;

/// Configuration passed from the shell before each present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerSurfaceSizePolicy {
    Fixed,
    Flexible,
}

/// Logical pixels of a surface that exist only so the client has somewhere to
/// paint, and which must never take pointer input.
///
/// MESH deliberately asks the compositor for surfaces that are *larger* than
/// their content: a bar reserves room below itself so tooltips can escape its
/// content box, and a popover reserves a ring so descendant `box-shadow` /
/// `filter` overshoot has pixels instead of clipping at the buffer edge. Those
/// reserved pixels are transparent, so a compositor that routes input by
/// surface bounds hands MESH every click over them and the windows underneath
/// get a dead zone — the single most-reintroduced bug in this codebase.
///
/// The padding therefore travels *with* the size that it inflates, inside
/// [`SurfaceConfig`] and [`PopupConfig`](super::super::PopupConfig), and the
/// backend derives the input region from it on every commit. There is no second
/// call to forget, no shell-side cache that can go stale, and no way for a
/// surface to be inflated without saying which part of it is reserve: producing
/// the inflated size and producing this padding is one operation
/// (`shell::runtime::render::surface_geometry_with_overlay_reserve`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct SurfacePadding {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl SurfacePadding {
    /// Reserve on the trailing edges only — the shape a tooltip-overlay bar
    /// uses, where content sits at the surface origin.
    pub fn trailing(right: u32, bottom: u32) -> Self {
        Self {
            left: 0,
            top: 0,
            right,
            bottom,
        }
    }

    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }

    /// The content rect inside a surface of `width` x `height` logical pixels,
    /// or `None` when the whole surface is content (an all-zero padding, which
    /// the caller turns into "reset to the default whole-surface input region").
    ///
    /// Padding that would consume the entire surface is ignored rather than
    /// collapsing the rect: a zero-area input region makes a surface
    /// completely unclickable, which is a worse failure than a slightly
    /// oversized one, and it can legitimately happen for one frame while a
    /// surface is still being measured.
    pub fn content_rect(&self, width: u32, height: u32) -> Option<DamageRect> {
        if self.is_zero() {
            return None;
        }
        let content_width = width.saturating_sub(self.left.saturating_add(self.right));
        let content_height = height.saturating_sub(self.top.saturating_add(self.bottom));
        if content_width == 0 || content_height == 0 {
            return None;
        }
        Some(DamageRect {
            x: self.left,
            y: self.top,
            width: content_width,
            height: content_height,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceConfig {
    /// Which compositor protocol realizes the surface. `Layer` uses every
    /// placement field below; `Window` ignores them and uses [`Self::window`]
    /// instead — a toplevel is placed by the compositor, not by the client.
    pub role: SurfaceRole,
    /// Toplevel-only properties, already localized. Ignored for `Layer`.
    pub window: WindowOptions,
    pub edge: Option<Edge>,
    pub layer: MeshLayer,
    pub size_policy: LayerSurfaceSizePolicy,
    pub width: u32,
    pub height: u32,
    /// Which part of [`Self::width`] x [`Self::height`] is paint-only reserve
    /// rather than content. See [`SurfacePadding`]: this is what keeps the
    /// input region confined to the content, and it must be produced by
    /// whatever produced the inflated size above.
    pub padding: SurfacePadding,
    pub exclusive_zone: i32,
    pub keyboard_mode: KeyboardMode,
    pub namespace: String,
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    /// When true the compositor-facing namespace is suffixed with `:blur` so a
    /// single compositor rule can blur every opted-in MESH surface (Hyprland:
    /// `layerrule = blur, :blur$`). See [`SurfaceConfig::wayland_namespace`].
    pub blur: bool,
}

impl Default for SurfaceConfig {
    fn default() -> Self {
        Self {
            role: SurfaceRole::Layer,
            window: WindowOptions::default(),
            edge: Some(Edge::Top),
            layer: MeshLayer::Top,
            size_policy: LayerSurfaceSizePolicy::Fixed,
            width: 0,
            height: 0,
            padding: SurfacePadding::default(),
            exclusive_zone: 0,
            keyboard_mode: KeyboardMode::None,
            namespace: "mesh".to_string(),
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
            blur: false,
        }
    }
}

impl SurfaceConfig {
    pub(in crate::wayland_surface) fn with_keyboard_mode(
        &self,
        keyboard_mode: KeyboardMode,
    ) -> Self {
        let mut cfg = self.clone();
        cfg.keyboard_mode = keyboard_mode;
        cfg
    }

    /// The namespace handed to the compositor when creating the layer surface.
    /// Blur-opted surfaces get a `:blur` suffix so one compositor rule targets
    /// them all — MESH cannot request blur through a protocol on every
    /// compositor, so it encodes the intent in the namespace instead.
    pub(in crate::wayland_surface) fn wayland_namespace(&self) -> String {
        if self.blur {
            format!("{}:blur", self.namespace)
        } else {
            self.namespace.clone()
        }
    }
}

/// The protocol work implied by a change from one accepted surface intent to
/// another. Keeping this classification typed prevents a newly added config
/// field from silently disappearing from a hash and lets the caller choose the
/// safe transition instead of treating every change as the same reconfigure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SurfaceConfigChange {
    /// No compositor-facing or retained surface state changed.
    Unchanged,
    /// Apply the new live state to the existing role. This includes title,
    /// app-id, size hints, keyboard interactivity, and input-region padding.
    Live,
    /// Re-commit layer-shell geometry and wait for a fresh configure before
    /// attaching another buffer.
    Configure,
    /// The role object's creation-time identity or protocol negotiation
    /// changed; prepare a replacement object before destroying the old one.
    Recreate,
}

impl SurfaceConfigChange {
    pub(super) fn requires_recreation(self) -> bool {
        matches!(self, Self::Recreate)
    }

    pub(super) fn requires_fresh_configure(self) -> bool {
        matches!(self, Self::Configure | Self::Recreate)
    }
}

/// Classify a new shell surface intent against the intent currently applied to
/// the live compositor object.
pub(super) fn surface_config_change(
    previous: &SurfaceConfig,
    previous_keyboard_mode: KeyboardMode,
    next: &SurfaceConfig,
    next_keyboard_mode: KeyboardMode,
) -> SurfaceConfigChange {
    if previous.role != next.role {
        return SurfaceConfigChange::Recreate;
    }

    // A layer namespace is supplied only while creating the role, and blur is
    // encoded into that namespace. Decorations are negotiated while creating an
    // xdg toplevel. Neither can be repaired by a later live request.
    let creation_identity_changed = match previous.role {
        SurfaceRole::Layer => previous.namespace != next.namespace || previous.blur != next.blur,
        SurfaceRole::Window => previous.window.decorations != next.window.decorations,
    };
    if creation_identity_changed {
        return SurfaceConfigChange::Recreate;
    }

    let geometry_changed = match previous.role {
        SurfaceRole::Layer => {
            previous.edge != next.edge
                || previous.layer != next.layer
                || previous.exclusive_zone != next.exclusive_zone
                || previous.width != next.width
                || previous.height != next.height
                || previous.margin_top != next.margin_top
                || previous.margin_right != next.margin_right
                || previous.margin_bottom != next.margin_bottom
                || previous.margin_left != next.margin_left
        }
        SurfaceRole::Window => false,
    };
    if geometry_changed {
        return SurfaceConfigChange::Configure;
    }

    // `SurfaceConfig` is shared by layer and toplevel roles, so comparing the
    // whole struct here would treat fields ignored by the active protocol as
    // compositor work. Keep the semantic diff role-aware: only fields that
    // can reach the live role or the shared input/geometry state should wake
    // an already configured surface.
    let live_state_changed = match previous.role {
        SurfaceRole::Layer => {
            previous.padding != next.padding || previous_keyboard_mode != next_keyboard_mode
        }
        SurfaceRole::Window => {
            previous.window.title != next.window.title
                || previous.window.app_id != next.window.app_id
                || previous.window.resizable != next.window.resizable
                || previous.width != next.width
                || previous.height != next.height
                || previous.padding != next.padding
        }
    };
    if live_state_changed {
        SurfaceConfigChange::Live
    } else {
        SurfaceConfigChange::Unchanged
    }
}

/// Clamp a layer-surface config's size/margins to a known output's logical
/// size. `output_size` must be the size of the output *this surface is
/// actually on* — passing `None` (output not yet known) or another
/// surface's/output's size produces wrong geometry: clamping a
/// compositor-verified width down to a smaller, unrelated output's size
/// centers the surface with dead space on both sides instead of spanning its
/// real output edge-to-edge. See `output_logical_size_for_surface`.
pub(super) fn clamp_surface_config_to_output(
    mut cfg: SurfaceConfig,
    output_size: Option<(u32, u32)>,
) -> SurfaceConfig {
    // Only layer surfaces are clamped to their output. A window is placed and
    // sized by the compositor, which may legitimately make it larger than one
    // output (or put it on a different one than the shell last saw); clamping
    // would fight the window manager.
    if cfg.role == SurfaceRole::Window {
        return cfg;
    }

    let Some((output_width, output_height)) = output_size else {
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

pub(super) fn resolved_surface_size(
    entry: &SurfaceEntry,
    output_size: Option<(u32, u32)>,
) -> (u32, u32) {
    // Popups are sized by their positioner / compositor configure and windows
    // by their toplevel configure, not by the layer-shell edge-stretch rules —
    // report the configured size verbatim for both.
    if entry.role.is_popup() || entry.role.is_window() {
        return (entry.width.max(1), entry.height.max(1));
    }
    resolved_surface_size_for_config(&entry.cfg, entry.width, entry.height, output_size)
}

pub(super) fn resolved_surface_size_for_config(
    cfg: &SurfaceConfig,
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
