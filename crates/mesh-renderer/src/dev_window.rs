use crate::PixelBuffer;
use minifb::{Window, WindowOptions};
use std::collections::HashMap;

#[derive(Default)]
pub struct DevWindowBackend {
    windows: HashMap<String, WindowSurface>,
}

struct WindowSurface {
    window: Window,
    frame: Vec<u32>,
    width: u32,
    height: u32,
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
            let surface =
                create_window_surface(title, width, height).map_err(crate::RenderError::SurfaceCreate)?;
            self.windows.insert(surface_id.to_string(), surface);
        }

        let surface = self
            .windows
            .get_mut(surface_id)
            .ok_or_else(|| crate::RenderError::SurfaceCreate("window missing after creation".into()))?;

        surface.window.set_title(title);
        surface.frame.resize((width * height) as usize, 0);
        convert_bgra_to_u32(buffer, &mut surface.frame);
        surface
            .window
            .update_with_buffer(&surface.frame, width as usize, height as usize)
            .map_err(|err: minifb::Error| {
                crate::RenderError::SurfaceCreate(format!("{err:?}"))
            })?;

        Ok(())
    }

    pub fn pump(&mut self) {
        self.windows.retain(|_, surface| {
            surface.window.update();
            surface.window.is_open()
        });
    }
}

fn create_window_surface(
    title: &str,
    width: u32,
    height: u32,
) -> Result<WindowSurface, String> {
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

    Ok(WindowSurface {
        window,
        frame: vec![0; (width * height) as usize],
        width,
        height,
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
