/// Wayland surface management and compositor abstraction for MESH.
///
/// This crate abstracts over compositor-specific protocol extensions so that
/// modules can create shell surfaces without knowing which compositor is running.
use std::io::Write;
use std::process::{Command, Stdio};

/// Screen edge for surface anchoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Layer for surface stacking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Background,
    Bottom,
    Top,
    Overlay,
}

/// Keyboard interactivity mode for a shell surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardMode {
    None,
    Exclusive,
    OnDemand,
}

/// Which compositor shell protocol realizes a surface.
///
/// `Layer` surfaces are shell chrome placed by the compositor
/// (`zwlr_layer_shell_v1`): panels, launchers, overlays. `Window` surfaces are
/// ordinary application windows (`xdg_toplevel`) that tile, float, move between
/// workspaces, and close like any other app — settings, module browsers, and
/// developer tools.
///
/// The two roles size in opposite directions. A layer surface tells the
/// compositor its CSS-measured size; a window is *told* its size by the
/// compositor's configure and lays content out into it. See
/// `docs/spec/01-module-system.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SurfaceRole {
    #[default]
    Layer,
    Window,
}

/// Who draws a window's title bar and borders.
///
/// MESH paints its own chrome, so `Client` is the default: the compositor is
/// asked to leave decoration to the module. `Server` opts into the
/// compositor's own decorations for users whose setup decorates uniformly.
/// Either way the compositor has the final say — it may answer a request with
/// the other mode, and [`WindowOptions::decorations`] records what was asked
/// for, not what was granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowDecorations {
    #[default]
    Client,
    Server,
}

/// Toplevel-only surface properties, resolved (title already localized).
///
/// Meaningless for [`SurfaceRole::Layer`]; the manifest layer rejects them on
/// layer surfaces rather than silently ignoring them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowOptions {
    /// Toplevel title shown by the compositor / task switcher.
    pub title: String,
    /// `xdg_toplevel.set_app_id` — what compositor window rules key off.
    pub app_id: String,
    /// When false the window is pinned to its content-measured size by
    /// reporting equal min and max sizes.
    pub resizable: bool,
    pub decorations: WindowDecorations,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: String::new(),
            app_id: String::new(),
            resizable: true,
            decorations: WindowDecorations::default(),
        }
    }
}

/// What the compositor last said a toplevel *is* — the `xdg_toplevel` states
/// carried by every configure.
///
/// These are decisions, not requests: a window cannot put itself in them, it
/// only learns about them and restyles. The shell projects each flag onto the
/// surface tree as a CSS state (`:fullscreen`, `:maximized`, `:activated`,
/// `:tiled`), so a module can size and decorate itself differently when it
/// fills the output than when it floats.
///
/// Meaningless for [`SurfaceRole::Layer`], which has no such protocol states;
/// a layer surface reports [`Self::default`] (everything false).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowStates {
    pub maximized: bool,
    pub fullscreen: bool,
    /// The window has keyboard focus as far as the compositor is concerned.
    pub activated: bool,
    /// Any edge is tiled — the window abuts a screen edge or neighbour under a
    /// tiling layout, so rounded corners and outer shadows are wrong.
    pub tiled: bool,
}

impl WindowStates {
    /// True when the window covers its whole allotment (fullscreen or
    /// maximized) — the case where content should stretch rather than keep a
    /// floating window's natural size.
    pub fn is_filling(self) -> bool {
        self.fullscreen || self.maximized
    }
}

/// Abstracted shell surface that maps to compositor-specific protocols.
pub trait ShellSurface {
    fn anchor(&mut self, edge: Edge);
    /// Select the compositor protocol backing this surface. Applied before the
    /// surface is created; changing it on a live surface re-creates the
    /// compositor object (see the shell's role-change path).
    fn set_role(&mut self, role: SurfaceRole);
    /// Apply toplevel-only properties. Ignored for [`SurfaceRole::Layer`].
    fn set_window_options(&mut self, options: WindowOptions);
    fn set_size(&mut self, width: u32, height: u32);
    fn set_exclusive_zone(&mut self, zone: i32);
    fn set_layer(&mut self, layer: Layer);
    fn set_keyboard_interactivity(&mut self, mode: KeyboardMode);
    fn set_margin(&mut self, top: i32, right: i32, bottom: i32, left: i32);
    /// Opt this surface into compositor blur. The presentation layer turns this
    /// into a `:blur`-suffixed layer-shell namespace a compositor rule targets.
    fn set_blur(&mut self, blur: bool);
    fn show(&mut self);
    fn hide(&mut self);
}

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("clipboard write failed: {message}")]
    WriteFailed { message: String },
}

pub trait ClipboardWriter: Send {
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError>;
}

#[derive(Debug, Clone)]
pub struct WaylandClipboard {
    command: String,
}

impl Default for WaylandClipboard {
    fn default() -> Self {
        Self {
            command: "wl-copy".to_string(),
        }
    }
}

impl ClipboardWriter for WaylandClipboard {
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        let mut child = Command::new(&self.command)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|source| ClipboardError::WriteFailed {
                message: format!("failed to spawn {}: {source}", self.command),
            })?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|source| ClipboardError::WriteFailed {
                    message: format!("failed to write clipboard payload: {source}"),
                })?;
        }

        let status = child.wait().map_err(|source| ClipboardError::WriteFailed {
            message: format!("failed waiting for {}: {source}", self.command),
        })?;
        if !status.success() {
            return Err(ClipboardError::WriteFailed {
                message: format!("{} exited with status {status}", self.command),
            });
        }

        Ok(())
    }
}

/// Reports what the current compositor supports.
pub trait CompositorCapabilities {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn supports(&self, protocol: &str) -> bool;
    fn supported_protocols(&self) -> Vec<String>;
}

/// Placeholder compositor backend for development and testing.
#[derive(Debug)]
pub struct StubCompositor;

impl CompositorCapabilities for StubCompositor {
    fn name(&self) -> &str {
        "stub"
    }

    fn version(&self) -> &str {
        "0.0.0"
    }

    fn supports(&self, _protocol: &str) -> bool {
        false
    }

    fn supported_protocols(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Placeholder shell surface for development and testing.
#[derive(Debug)]
pub struct StubSurface {
    pub visible: bool,
    pub width: u32,
    pub height: u32,
    pub edge: Option<Edge>,
    pub layer: Option<Layer>,
    pub exclusive_zone: i32,
    pub keyboard_mode: KeyboardMode,
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    pub blur: bool,
    pub role: SurfaceRole,
    pub window: WindowOptions,
}

#[derive(Debug, Default)]
pub struct StubClipboard {
    pub last_written: Option<String>,
    pub fail_message: Option<String>,
}

impl ClipboardWriter for StubClipboard {
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        if let Some(message) = self.fail_message.clone() {
            return Err(ClipboardError::WriteFailed { message });
        }
        self.last_written = Some(text.to_string());
        Ok(())
    }
}

impl Default for StubSurface {
    fn default() -> Self {
        Self {
            visible: true,
            width: 0,
            height: 0,
            edge: None,
            layer: None,
            exclusive_zone: 0,
            keyboard_mode: KeyboardMode::None,
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
            blur: false,
            role: SurfaceRole::Layer,
            window: WindowOptions::default(),
        }
    }
}

impl ShellSurface for StubSurface {
    fn anchor(&mut self, edge: Edge) {
        self.edge = Some(edge);
    }

    fn set_role(&mut self, role: SurfaceRole) {
        self.role = role;
    }

    fn set_window_options(&mut self, options: WindowOptions) {
        self.window = options;
    }

    fn set_size(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    fn set_exclusive_zone(&mut self, zone: i32) {
        self.exclusive_zone = zone;
    }

    fn set_layer(&mut self, layer: Layer) {
        self.layer = Some(layer);
    }

    fn set_keyboard_interactivity(&mut self, mode: KeyboardMode) {
        self.keyboard_mode = mode;
    }

    fn set_margin(&mut self, top: i32, right: i32, bottom: i32, left: i32) {
        self.margin_top = top;
        self.margin_right = right;
        self.margin_bottom = bottom;
        self.margin_left = left;
    }

    fn set_blur(&mut self, blur: bool) {
        self.blur = blur;
    }

    fn show(&mut self) {
        self.visible = true;
    }

    fn hide(&mut self) {
        self.visible = false;
    }
}
