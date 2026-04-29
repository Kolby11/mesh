/// Wayland surface management and compositor abstraction for MESH.
///
/// This crate abstracts over compositor-specific protocol extensions so that
/// plugins can create shell surfaces without knowing which compositor is running.

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

/// Abstracted shell surface that maps to compositor-specific protocols.
pub trait ShellSurface {
    fn anchor(&mut self, edge: Edge);
    fn set_size(&mut self, width: u32, height: u32);
    fn set_exclusive_zone(&mut self, zone: i32);
    fn set_layer(&mut self, layer: Layer);
    fn set_keyboard_interactivity(&mut self, mode: KeyboardMode);
    fn set_margin(&mut self, top: i32, right: i32, bottom: i32, left: i32);
    fn show(&mut self);
    fn hide(&mut self);
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
        }
    }
}

impl ShellSurface for StubSurface {
    fn anchor(&mut self, edge: Edge) {
        self.edge = Some(edge);
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

    fn show(&mut self) {
        self.visible = true;
    }

    fn hide(&mut self) {
        self.visible = false;
    }
}
