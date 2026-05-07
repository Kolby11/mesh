use super::PixelBuffer;
use image::imageops::FilterType;
use mesh_core_elements::style::Color;
use super::glyph::{GlyphAxes, draw_font_glyph};
use mesh_core_icon::{IconResolution, MISSING_ICON_SVG, ResolvedTarget, resolve_icon_result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static IMAGE_CACHE: OnceLock<Mutex<HashMap<PathBuf, image::RgbaImage>>> = OnceLock::new();

fn image_cache() -> &'static Mutex<HashMap<PathBuf, image::RgbaImage>> {
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
    draw_icon_from_path_with_options(buffer, path, dest_x, dest_y, dest_w, dest_h, tint, false);
}

pub fn draw_icon_from_path_with_options(
    buffer: &mut PixelBuffer,
    path: &Path,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
    multicolor: bool,
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
                            let color = if multicolor {
                                Color {
                                    r: p[0],
                                    g: p[1],
                                    b: p[2],
                                    a: p[3],
                                }
                            } else {
                                Color {
                                    r: tint.r,
                                    g: tint.g,
                                    b: tint.b,
                                    a: p[3],
                                }
                            };
                            blend_icon_pixel(
                                buffer,
                                dest_x.saturating_add(x as i32),
                                dest_y.saturating_add(y as i32),
                                color,
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
                            let data = pixmap.data();
                            for py in 0..h {
                                for px in 0..w {
                                    let idx = (py * w + px) as usize * 4;
                                    if data[idx + 3] == 0 {
                                        continue;
                                    }
                                    let color = if multicolor {
                                        Color {
                                            r: data[idx],
                                            g: data[idx + 1],
                                            b: data[idx + 2],
                                            a: data[idx + 3],
                                        }
                                    } else {
                                        Color {
                                            r: tint.r,
                                            g: tint.g,
                                            b: tint.b,
                                            a: data[idx + 3],
                                        }
                                    };
                                    blend_icon_pixel(
                                        buffer,
                                        dest_x.saturating_add(px as i32),
                                        dest_y.saturating_add(py as i32),
                                        color,
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

fn blend_icon_pixel(buffer: &mut PixelBuffer, x: i32, y: i32, color: Color) {
    if x < 0 || y < 0 {
        return;
    }
    buffer.blend_pixel(x as u32, y as u32, color, 255);
}

/// Draw the built-in "missing icon" glyph. Rasterizes the embedded SVG via
/// resvg and tints it with the icon's text color, so it follows the same
/// theming rules as a regular monochrome icon.
pub fn draw_missing_icon_fallback(
    buffer: &mut PixelBuffer,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
) {
    let w = dest_w.max(1) as u32;
    let h = dest_h.max(1) as u32;

    let mut opt = resvg::usvg::Options::default();
    let Ok(tree) = resvg::usvg::Tree::from_str(MISSING_ICON_SVG, &opt) else {
        return;
    };
    opt.resources_dir = None;
    let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(w, h) else {
        return;
    };
    let scale_x = w as f32 / tree.size().width();
    let scale_y = h as f32 / tree.size().height();
    let transform = resvg::tiny_skia::Transform::from_scale(scale_x, scale_y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let data = pixmap.data();
    for py in 0..h {
        for px in 0..w {
            let idx = (py * w + px) as usize * 4;
            let alpha = data[idx + 3];
            if alpha == 0 {
                continue;
            }
            let color = Color {
                r: tint.r,
                g: tint.g,
                b: tint.b,
                a: alpha,
            };
            blend_icon_pixel(
                buffer,
                dest_x.saturating_add(px as i32),
                dest_y.saturating_add(py as i32),
                color,
            );
        }
    }
}

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
    draw_icon_resolution(
        buffer,
        resolve_icon_result(name, size),
        dest_x,
        dest_y,
        dest_w,
        dest_h,
        tint,
    );
}

/// Draw a named icon using the calling module's icon bindings (preferred
/// pack, declared mappings, user overrides) before falling back to shell-
/// wide profile defaults and finally the built-in missing-icon glyph.
/// `axes` carries CSS `--icon-*` values for variable-font axis settings;
/// pass [`GlyphAxes::default()`] when no axis state is available.
pub fn draw_named_icon_for_module(
    buffer: &mut PixelBuffer,
    module_id: &str,
    name: &str,
    size: u32,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
    axes: GlyphAxes,
) {
    draw_icon_resolution_with_axes(
        buffer,
        mesh_core_icon::resolve_icon_for_module(module_id, name, size),
        dest_x,
        dest_y,
        dest_w,
        dest_h,
        tint,
        axes,
    );
}

#[cfg(test)]
fn draw_named_icon_with_registry(
    buffer: &mut PixelBuffer,
    registry: &mut mesh_core_icon::IconRegistry,
    name: &str,
    size: u32,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
) {
    draw_icon_resolution(
        buffer,
        mesh_core_icon::resolve_icon_with_registry(registry, name, size),
        dest_x,
        dest_y,
        dest_w,
        dest_h,
        tint,
    );
}

fn draw_icon_resolution(
    buffer: &mut PixelBuffer,
    resolution: IconResolution,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
) {
    draw_icon_resolution_with_axes(
        buffer,
        resolution,
        dest_x,
        dest_y,
        dest_w,
        dest_h,
        tint,
        GlyphAxes::default(),
    );
}

/// Like `draw_icon_resolution` but lets the caller supply variable-font
/// axis values (parsed from CSS `--icon-*` custom properties). Axes are
/// silently ignored for file targets and for font targets whose pack
/// doesn't declare support for the requested axis.
pub fn draw_icon_resolution_with_axes(
    buffer: &mut PixelBuffer,
    resolution: IconResolution,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
    axes: GlyphAxes,
) {
    match resolution {
        IconResolution::Found {
            target: ResolvedTarget::File(path),
            multicolor,
            ..
        } => draw_icon_from_path_with_options(
            buffer, &path, dest_x, dest_y, dest_w, dest_h, tint, multicolor,
        ),
        IconResolution::Found {
            target:
                ResolvedTarget::Glyph {
                    font_path,
                    codepoint,
                    supported_axes,
                },
            ..
        } => {
            let drew = draw_font_glyph(
                buffer,
                &font_path,
                codepoint,
                supported_axes,
                axes,
                dest_x,
                dest_y,
                dest_w,
                dest_h,
                tint,
            );
            if !drew {
                draw_missing_icon_fallback(buffer, dest_x, dest_y, dest_w, dest_h, tint);
            }
        }
        IconResolution::Missing { .. } => {
            draw_missing_icon_fallback(buffer, dest_x, dest_y, dest_w, dest_h, tint)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use std::fs;

    fn tint() -> Color {
        Color {
            r: 16,
            g: 120,
            b: 220,
            a: 255,
        }
    }

    fn pixel(buffer: &PixelBuffer, x: u32, y: u32) -> Color {
        let offset = (y * buffer.stride + x * 4) as usize;
        Color {
            b: buffer.data[offset],
            g: buffer.data[offset + 1],
            r: buffer.data[offset + 2],
            a: buffer.data[offset + 3],
        }
    }

    fn non_transparent_pixels(buffer: &PixelBuffer) -> Vec<(u32, u32, Color)> {
        let mut pixels = Vec::new();
        for y in 0..buffer.height {
            for x in 0..buffer.width {
                let color = pixel(buffer, x, y);
                if color.a > 0 {
                    pixels.push((x, y, color));
                }
            }
        }
        pixels
    }

    #[test]
    fn svg_icon_rasterizes_and_tints() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("symbol.svg");
        fs::write(
            &path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect x="1" y="1" width="6" height="6" fill="black"/></svg>"#,
        )
        .unwrap();

        let mut buffer = PixelBuffer::new(24, 24);
        draw_icon_from_path(&mut buffer, &path, 4, 3, 14, 12, tint());

        let pixels = non_transparent_pixels(&buffer);
        assert!(!pixels.is_empty());
        assert!(
            pixels
                .iter()
                .all(|(x, y, _)| *x >= 4 && *x < 18 && *y >= 3 && *y < 15)
        );
        assert!(pixels.iter().any(|(_, _, color)| {
            color.a == 255 && color.r == tint().r && color.g == tint().g && color.b == tint().b
        }));
    }

    #[test]
    fn raster_icon_decodes_resizes_and_tints() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("symbol.png");
        let image = ImageBuffer::from_fn(2, 2, |_, _| Rgba([255u8, 0, 0, 255]));
        image.save(&path).unwrap();

        let mut buffer = PixelBuffer::new(16, 16);
        draw_icon_from_path(&mut buffer, &path, 2, 2, 9, 7, tint());

        let pixels = non_transparent_pixels(&buffer);
        assert!(!pixels.is_empty());
        assert!(pixels.iter().all(|(x, y, color)| {
            *x >= 2
                && *x < 11
                && *y >= 2
                && *y < 9
                && color.r == tint().r
                && color.g == tint().g
                && color.b == tint().b
        }));
    }

    #[test]
    fn multicolor_raster_preserves_source_colors() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("logo.png");
        let image = ImageBuffer::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([255u8, 0, 0, 255])
            } else {
                Rgba([0u8, 255, 0, 255])
            }
        });
        image.save(&path).unwrap();

        let mut buffer = PixelBuffer::new(8, 4);
        draw_icon_from_path_with_options(&mut buffer, &path, 0, 0, 2, 1, tint(), true);

        assert_eq!(pixel(&buffer, 0, 0).r, 255);
        assert_eq!(pixel(&buffer, 0, 0).g, 0);
        assert_eq!(pixel(&buffer, 1, 0).r, 0);
        assert_eq!(pixel(&buffer, 1, 0).g, 255);
    }

    #[test]
    fn missing_icon_paints_builtin_fallback() {
        let mut buffer = PixelBuffer::new(30, 30);
        draw_missing_icon_fallback(&mut buffer, 6, 5, 18, 18, tint());

        let pixels = non_transparent_pixels(&buffer);
        assert!(!pixels.is_empty());
        assert!(
            pixels
                .iter()
                .all(|(x, y, _)| *x >= 6 && *x < 24 && *y >= 5 && *y < 23)
        );
    }

    #[test]
    fn draw_named_icon_uses_destination_box_for_missing_fallback() {
        let td = tempfile::tempdir().unwrap();
        let config = mesh_core_icon::IconConfig::from_toml_str(&format!(
            r#"
active_profile = "material"

[[packs]]
id = "material"
root = "{}"

[profiles.material.icons]
missing-proof = ["material:not-present"]
"#,
            td.path().display()
        ))
        .unwrap();
        let mut registry = mesh_core_icon::IconRegistry::from_config(config).unwrap();
        let mut buffer = PixelBuffer::new(40, 36);

        draw_named_icon_with_registry(
            &mut buffer,
            &mut registry,
            "missing-proof",
            18,
            4,
            3,
            30,
            28,
            tint(),
        );

        let pixels = non_transparent_pixels(&buffer);
        assert!(!pixels.is_empty());
        assert!(
            pixels
                .iter()
                .all(|(x, y, _)| *x >= 4 && *x < 34 && *y >= 3 && *y < 31)
        );
        assert!(pixels.iter().any(|(x, _, _)| *x >= 22));
    }
}
