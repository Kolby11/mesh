use crate::buffer::PixelBuffer;
use mesh_ui::style::Color;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

// Cache decoded raster images (PNG/JPEG/etc.) keyed by absolute PathBuf
static IMAGE_CACHE: OnceLock<Mutex<HashMap<std::path::PathBuf, image::RgbaImage>>> =
    OnceLock::new();

fn get_image_cache() -> &'static Mutex<HashMap<std::path::PathBuf, image::RgbaImage>> {
    IMAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_or_load(path: &std::path::Path) -> Option<image::RgbaImage> {
    let cache = get_image_cache();
    let mut guard = cache.lock().unwrap();
    if let Some(img) = guard.get(path) {
        return Some(img.clone());
    }
    let img = image::open(path).ok()?.to_rgba8();
    guard.insert(path.to_path_buf(), img.clone());
    Some(img)
}

/// Draw an icon from path into the buffer at dest rect (already in pixel coords).
/// Supports PNG/JPEG/BMP via cached decoding and SVG via resvg.
pub fn draw_icon_from_path(
    buffer: &mut PixelBuffer,
    path: &Path,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
) {
    if !path.exists() {
        return;
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "bmp" => {
                if let Some(img) = get_or_load(path) {
                    let (w, h) = img.dimensions();
                    // Simple nearest-neighbor scale/blit
                    for y in 0..dest_h.max(0) as u32 {
                        for x in 0..dest_w.max(0) as u32 {
                            let sx =
                                ((x as f32) * (w as f32) / (dest_w.max(1) as f32)).floor() as u32;
                            let sy =
                                ((y as f32) * (h as f32) / (dest_h.max(1) as f32)).floor() as u32;
                            if sx < w && sy < h {
                                let p = img.get_pixel(sx, sy);
                                if p[3] == 0 {
                                    continue;
                                }
                                let px = dest_x.saturating_add(x as i32);
                                let py = dest_y.saturating_add(y as i32);
                                buffer.blend_pixel(
                                    px as u32,
                                    py as u32,
                                    Color {
                                        r: tint.r,
                                        g: tint.g,
                                        b: tint.b,
                                        a: p[3],
                                    },
                                    255,
                                );
                            }
                        }
                    }
                }
            }
            "svg" => {
                // Rasterize SVG using resvg/usvg + tiny-skia
                if let Ok(svg_data) = std::fs::read_to_string(path) {
                    let mut opt = resvg::usvg::Options::default();
                    opt.resources_dir = path.parent().map(|p| p.to_path_buf());
                    if let Ok(tree) = resvg::usvg::Tree::from_str(&svg_data, &opt) {
                        let w = dest_w.max(1) as u32;
                        let h = dest_h.max(1) as u32;
                        if let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(w, h) {
                            let scale_x = w as f32 / tree.size().width();
                            let scale_y = h as f32 / tree.size().height();
                            let transform =
                                resvg::tiny_skia::Transform::from_scale(scale_x, scale_y);
                            resvg::render(&tree, transform, &mut pixmap.as_mut());
                            for py in 0..h {
                                for px in 0..w {
                                    let idx = (py * w + px) as usize * 4;
                                    let data = pixmap.data();
                                    if data[idx + 3] == 0 {
                                        continue;
                                    }
                                    buffer.blend_pixel(
                                        (dest_x + px as i32) as u32,
                                        (dest_y + py as i32) as u32,
                                        Color {
                                            r: tint.r,
                                            g: tint.g,
                                            b: tint.b,
                                            a: data[idx + 3],
                                        },
                                        255,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
