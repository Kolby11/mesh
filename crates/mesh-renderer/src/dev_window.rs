use crate::PixelBuffer;
use minifb::{InputCallback, Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum DevWindowEvent {
    PointerMove {
        surface_id: String,
        x: f32,
        y: f32,
    },
    PointerButton {
        surface_id: String,
        x: f32,
        y: f32,
        pressed: bool,
    },
    Scroll {
        surface_id: String,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
    },
    Key {
        surface_id: String,
        event: DevWindowKeyEvent,
    },
    Char {
        surface_id: String,
        ch: char,
    },
}

#[derive(Debug, Clone, Default)]
pub struct KeyMods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Debug, Clone)]
pub enum DevWindowKeyEvent {
    Pressed(String, KeyMods),
    Released(String),
}

#[derive(Default)]
pub struct DevWindowBackend {
    windows: HashMap<String, WindowSurface>,
}

struct WindowSurface {
    window: Window,
    frame: Vec<u32>,
    width: u32,
    height: u32,
    last_mouse_pos: Option<(f32, f32)>,
    last_left_down: bool,
    chars: Arc<Mutex<Vec<char>>>,
}

impl DevWindowBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn present(
        &mut self,
        surface_id: &str,
        title: &str,
        visible: bool,
        buffer: &PixelBuffer,
    ) -> Result<(), crate::RenderError> {
        if !visible {
            self.windows.remove(surface_id);
            return Ok(());
        }

        let width = buffer.width.max(1);
        let height = buffer.height.max(1);
        let needs_new_window = self
            .windows
            .get(surface_id)
            .map(|surface| surface.width != width || surface.height != height)
            .unwrap_or(true);

        if needs_new_window {
            let surface = create_window_surface(title, width, height)
                .map_err(crate::RenderError::SurfaceCreate)?;
            self.windows.insert(surface_id.to_string(), surface);
        }

        let surface = self.windows.get_mut(surface_id).ok_or_else(|| {
            crate::RenderError::SurfaceCreate("window missing after creation".into())
        })?;

        surface.window.set_title(title);
        surface.frame.resize((width * height) as usize, 0);
        convert_bgra_to_u32(buffer, &mut surface.frame);
        surface
            .window
            .update_with_buffer(&surface.frame, width as usize, height as usize)
            .map_err(|err: minifb::Error| crate::RenderError::SurfaceCreate(format!("{err:?}")))?;

        Ok(())
    }

    pub fn pump(&mut self) {
        self.windows.retain(|_, surface| {
            surface.window.update();
            surface.window.is_open()
        });
    }

    pub fn poll_events(&mut self) -> Vec<DevWindowEvent> {
        let mut events = Vec::new();

        for (surface_id, surface) in &mut self.windows {
            let current_mouse_pos = normalized_mouse_pos(surface);
            if current_mouse_pos != surface.last_mouse_pos {
                if let Some((x, y)) = current_mouse_pos {
                    events.push(DevWindowEvent::PointerMove {
                        surface_id: surface_id.clone(),
                        x,
                        y,
                    });
                }
                surface.last_mouse_pos = current_mouse_pos;
            }

            let current_left_down = surface.window.get_mouse_down(MouseButton::Left);
            if current_left_down != surface.last_left_down {
                let (x, y) = current_mouse_pos.unwrap_or((0.0, 0.0));
                events.push(DevWindowEvent::PointerButton {
                    surface_id: surface_id.clone(),
                    x,
                    y,
                    pressed: current_left_down,
                });
                surface.last_left_down = current_left_down;
            }

            if let Some((dx, dy)) = surface.window.get_scroll_wheel() {
                if dx.abs() > f32::EPSILON || dy.abs() > f32::EPSILON {
                    let (x, y) = current_mouse_pos.unwrap_or((0.0, 0.0));
                    events.push(DevWindowEvent::Scroll {
                        surface_id: surface_id.clone(),
                        x,
                        y,
                        dx,
                        dy,
                    });
                }
            }

            let mods = KeyMods {
                ctrl: surface.window.is_key_down(Key::LeftCtrl)
                    || surface.window.is_key_down(Key::RightCtrl),
                shift: surface.window.is_key_down(Key::LeftShift)
                    || surface.window.is_key_down(Key::RightShift),
                alt: surface.window.is_key_down(Key::LeftAlt)
                    || surface.window.is_key_down(Key::RightAlt),
            };
            for key in surface.window.get_keys_pressed(KeyRepeat::Yes) {
                events.push(DevWindowEvent::Key {
                    surface_id: surface_id.clone(),
                    event: DevWindowKeyEvent::Pressed(key_name(key), mods.clone()),
                });
            }

            for key in surface.window.get_keys_released() {
                events.push(DevWindowEvent::Key {
                    surface_id: surface_id.clone(),
                    event: DevWindowKeyEvent::Released(key_name(key)),
                });
            }

            if let Ok(mut chars) = surface.chars.lock() {
                for ch in chars.drain(..) {
                    events.push(DevWindowEvent::Char {
                        surface_id: surface_id.clone(),
                        ch,
                    });
                }
            }
        }

        events
    }
}

fn create_window_surface(title: &str, width: u32, height: u32) -> Result<WindowSurface, String> {
    let mut window = Window::new(
        title,
        width as usize,
        height as usize,
        WindowOptions {
            resize: false,
            ..WindowOptions::default()
        },
    )
    .map_err(|err| format!("{err:?}"))?;
    window.set_target_fps(60);
    let chars = Arc::new(Mutex::new(Vec::new()));
    window.set_input_callback(Box::new(WindowInputCallback {
        chars: chars.clone(),
    }));

    Ok(WindowSurface {
        window,
        frame: vec![0; (width * height) as usize],
        width,
        height,
        last_mouse_pos: None,
        last_left_down: false,
        chars,
    })
}

fn convert_bgra_to_u32(buffer: &PixelBuffer, out: &mut [u32]) {
    for (chunk, pixel) in buffer.data.chunks_exact(4).zip(out.iter_mut()) {
        let b = chunk[0] as u32;
        let g = chunk[1] as u32;
        let r = chunk[2] as u32;
        *pixel = (r << 16) | (g << 8) | b;
    }
}

#[derive(Debug)]
struct WindowInputCallback {
    chars: Arc<Mutex<Vec<char>>>,
}

impl InputCallback for WindowInputCallback {
    fn add_char(&mut self, uni_char: u32) {
        let Some(ch) = char::from_u32(uni_char) else {
            return;
        };

        if let Ok(mut chars) = self.chars.lock() {
            chars.push(ch);
        }
    }
}

fn key_name(key: Key) -> String {
    format!("{key:?}")
}

fn normalized_mouse_pos(surface: &WindowSurface) -> Option<(f32, f32)> {
    let (window_w, window_h) = surface.window.get_size();
    if window_w == 0 || window_h == 0 {
        return None;
    }

    let (raw_x, raw_y) = surface.window.get_unscaled_mouse_pos(MouseMode::Discard)?;
    let scale_x = surface.width as f32 / window_w as f32;
    let scale_y = surface.height as f32 / window_h as f32;

    Some((
        (raw_x * scale_x).clamp(0.0, surface.width.saturating_sub(1) as f32),
        (raw_y * scale_y).clamp(0.0, surface.height.saturating_sub(1) as f32),
    ))
}
