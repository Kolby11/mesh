//! Compositor abstraction: modules create shell surfaces without knowing which
//! compositor is running.

use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Background,
    Bottom,
    Top,
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardMode {
    None,
    Exclusive,
    OnDemand,
}

/// `Layer` is shell chrome placed by the compositor (`zwlr_layer_shell_v1`);
/// `Window` is an ordinary `xdg_toplevel`. They size in opposite directions: a
/// layer surface tells the compositor its CSS-measured size, a window is *told*
/// its size by configure. See `docs/spec/01-module-system.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SurfaceRole {
    #[default]
    Layer,
    Window,
}

/// MESH paints its own chrome, so `Client` is the default. Either way the
/// compositor has the final say: [`WindowOptions::decorations`] records what
/// was asked for, not what was granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowDecorations {
    #[default]
    Client,
    Server,
}

/// Toplevel-only surface properties, resolved (title already localized).
/// Rejected by the manifest layer on [`SurfaceRole::Layer`] surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowOptions {
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

/// The `xdg_toplevel` states carried by every configure — decisions, not
/// requests. The shell projects each onto the tree as a CSS state
/// (`:fullscreen`, `:maximized`, `:activated`, `:tiled`). Layer surfaces have
/// no such states and report [`Self::default`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowStates {
    pub maximized: bool,
    pub fullscreen: bool,
    pub activated: bool,
    /// Any edge abuts a screen edge or neighbour, so rounded corners and outer
    /// shadows are wrong.
    pub tiled: bool,
}

impl WindowStates {
    /// Covers its whole allotment, so content should stretch rather than keep
    /// a floating window's natural size.
    pub fn is_filling(self) -> bool {
        self.fullscreen || self.maximized
    }
}

/// Abstracted shell surface that maps to compositor-specific protocols.
pub trait ShellSurface {
    fn anchor(&mut self, edge: Edge);
    /// Applied before creation; changing it on a live surface re-creates the
    /// compositor object.
    fn set_role(&mut self, role: SurfaceRole);
    /// Ignored for [`SurfaceRole::Layer`].
    fn set_window_options(&mut self, options: WindowOptions);
    fn set_size(&mut self, width: u32, height: u32);
    fn set_exclusive_zone(&mut self, zone: i32);
    fn set_layer(&mut self, layer: Layer);
    fn set_keyboard_interactivity(&mut self, mode: KeyboardMode);
    fn set_margin(&mut self, top: i32, right: i32, bottom: i32, left: i32);
    /// Turned into a `:blur`-suffixed layer-shell namespace by presentation.
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

impl WaylandClipboard {
    pub fn with_command(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
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

        let write_result = child
            .stdin
            .as_mut()
            .expect("spawned with piped stdin")
            .write_all(text.as_bytes());
        // Drop stdin so the child observes EOF whether or not the write
        // succeeded, then always reap the process below. Every exit path
        // must call `wait()` or a write failure leaves the child running
        // unreaped.
        child.stdin = None;

        if let Err(source) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ClipboardError::WriteFailed {
                message: format!("failed to write clipboard payload: {source}"),
            });
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

pub trait CompositorCapabilities {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn supports(&self, protocol: &str) -> bool;
    fn supported_protocols(&self) -> Vec<String>;
}

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Number of zombie (`Z` state) children of the current process, read
    /// from `/proc`. Used to confirm a failed clipboard write still reaps
    /// its child instead of leaving it defunct.
    fn zombie_child_count() -> usize {
        let my_pid = std::process::id().to_string();
        let mut count = 0;
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return 0;
        };
        for entry in entries.flatten() {
            let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            // Fields after the "(comm)" part are space-separated; state is
            // field 3, ppid is field 4 in the whole-line numbering.
            let Some(after_comm) = stat.rsplit_once(") ") else {
                continue;
            };
            let fields: Vec<&str> = after_comm.1.split_whitespace().collect();
            let (Some(state), Some(ppid)) = (fields.first(), fields.get(1)) else {
                continue;
            };
            if *state == "Z" && *ppid == my_pid {
                count += 1;
            }
        }
        count
    }

    #[test]
    fn write_text_reaps_child_on_broken_pipe() {
        // `true` exits almost immediately without reading stdin, so writing
        // a payload larger than the pipe buffer reliably fails with a
        // broken pipe while the write is still in progress.
        let mut clipboard = WaylandClipboard::with_command("true");
        let payload = "x".repeat(16 * 1024 * 1024);

        let before = zombie_child_count();
        let result = clipboard.write_text(&payload);
        let after = zombie_child_count();

        assert!(result.is_err(), "expected broken-pipe write to fail");
        assert_eq!(
            after, before,
            "write_text left a zombie child after a write failure"
        );
    }

    #[test]
    fn write_text_reports_nonzero_exit() {
        let mut clipboard = WaylandClipboard::with_command("false");
        let result = clipboard.write_text("hello");
        assert!(matches!(result, Err(ClipboardError::WriteFailed { .. })));
    }

    #[test]
    fn write_text_reports_spawn_failure() {
        let mut clipboard = WaylandClipboard::with_command("mesh-nonexistent-clipboard-helper-xyz");
        let result = clipboard.write_text("hello");
        assert!(matches!(result, Err(ClipboardError::WriteFailed { .. })));
    }

    #[test]
    fn write_text_succeeds_for_reading_command() {
        let mut clipboard = WaylandClipboard::with_command("cat");
        let result = clipboard.write_text("hello");
        assert!(result.is_ok());
    }
}
