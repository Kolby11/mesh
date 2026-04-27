use super::PixelBuffer;
use image::imageops::FilterType;
use mesh_ui::style::Color;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

static IMAGE_CACHE: OnceLock<Mutex<HashMap<std::path::PathBuf, image::RgbaImage>>> =
    OnceLock::new();

fn image_cache() -> &'static Mutex<HashMap<std::path::PathBuf, image::RgbaImage>> {
    IMAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_or_load(path: &Path) -> Option<image::RgbaImage> {
    let mut guard = image_cache().lock().unwrap();
    if let Some(img) = guard.get(path) {
        return Some(img.clone());
    }
    let img = image::open(path).ok()?.to_rgba8();
    guard.insert(path.to_path_buf(), img.clone());
    Some(img)
}

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
        match ext.to_ascii_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "bmp" => {
                if let Some(img) = get_or_load(path) {
                    let dest_w = dest_w.max(1) as u32;
                    let dest_h = dest_h.max(1) as u32;
                    let scaled =
                        image::imageops::resize(&img, dest_w, dest_h, FilterType::Lanczos3);
                    for y in 0..dest_h {
                        for x in 0..dest_w {
                            let p = scaled.get_pixel(x, y);
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
            "svg" => {
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

use mesh_icon::resolve_icon as resolve_icon_path;

pub fn draw_named_icon(
    buffer: &mut PixelBuffer,
    name: &str,
    size: u32,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
) {
    if let Some(path) = resolve_icon_path(name, size) {
        draw_icon_from_path(buffer, &path, dest_x, dest_y, dest_w, dest_h, tint);
    }
}
