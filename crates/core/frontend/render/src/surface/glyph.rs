//! Font glyph rasterization for icon font packs (Material Symbols, Nerd
//! Font, etc.). Uses swash to render a single glyph at a requested pixel
//! size with optional variable-font axis settings (FILL, wght, GRAD,
//! opsz). Output is an 8-bit alpha mask that the painter blits with the
//! icon's tint color, so glyphs flow through the same theme-token coloring
//! path as monochrome SVG icons.

use super::PixelBuffer;
use super::profiling;
use mesh_core_elements::lru::LruCache;
use mesh_core_elements::style::Color;
use mesh_core_icon::SupportedAxes;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;
use swash::{FontRef, GlyphId};

/// Variable-font axis settings sourced from CSS `--icon-*` custom
/// properties. Only fields whose corresponding axis is declared
/// `supported` by the font pack actually take effect; others are silently
/// ignored.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GlyphAxes {
    pub fill: Option<f32>,
    pub weight: Option<f32>,
    pub grade: Option<f32>,
    pub optical_size: Option<f32>,
}

impl GlyphAxes {
    pub fn is_default(&self) -> bool {
        self.fill.is_none()
            && self.weight.is_none()
            && self.grade.is_none()
            && self.optical_size.is_none()
    }
}

/// Raster cache key. Color is encoded so a recolor invalidates; size is
/// quantized to integer px since fractional sizes hash poorly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphCacheKey {
    font_path: u64,
    codepoint: u32,
    px: u32,
    color: u32,
    fill_q: i32,
    weight_q: i32,
    grade_q: i32,
    opsz_q: i32,
}

#[derive(Debug, Clone)]
struct CachedGlyph {
    width: u32,
    height: u32,
    placement_left: i32,
    pixels: Arc<[u8]>,
}

type FontBytesCache = Mutex<LruCache<Arc<Path>, Arc<[u8]>>>;

static FONT_BYTES: OnceLock<FontBytesCache> = OnceLock::new();
static GLYPH_CACHE: OnceLock<Mutex<LruCache<GlyphCacheKey, Option<CachedGlyph>>>> = OnceLock::new();
const FONT_BYTES_CACHE_CAPACITY: usize = 32;
const GLYPH_CACHE_CAPACITY: usize = 1024;

fn font_bytes_cache() -> &'static FontBytesCache {
    FONT_BYTES.get_or_init(|| Mutex::new(LruCache::new(FONT_BYTES_CACHE_CAPACITY)))
}

fn glyph_cache() -> &'static Mutex<LruCache<GlyphCacheKey, Option<CachedGlyph>>> {
    GLYPH_CACHE.get_or_init(|| Mutex::new(LruCache::new(GLYPH_CACHE_CAPACITY)))
}

fn font_bytes(path: &Path) -> Option<Arc<[u8]>> {
    let cache = font_bytes_cache();
    if let Ok(mut guard) = cache.lock()
        && let Some(bytes) = guard.get(path)
    {
        return Some(Arc::clone(bytes));
    }
    let bytes: Arc<[u8]> = std::fs::read(path).ok()?.into();
    if let Ok(mut guard) = cache.lock() {
        guard.insert(Arc::from(path), Arc::clone(&bytes));
    }
    Some(bytes)
}

fn rasterize(
    font_path: &Path,
    codepoint: u32,
    px: u32,
    axes: GlyphAxes,
    supported: SupportedAxes,
) -> Option<CachedGlyph> {
    let bytes = font_bytes(font_path)?;
    let font = FontRef::from_index(bytes.as_ref(), 0)?;
    let glyph_id = font.charmap().map(char::from_u32(codepoint)?);
    if glyph_id == 0 {
        return None;
    }
    let mut ctx = ScaleContext::new();
    let mut builder = ctx.builder(font).size(px as f32).hint(true);
    let mut variations: Vec<(&str, f32)> = Vec::new();
    if supported.fill
        && let Some(v) = axes.fill
    {
        variations.push(("FILL", v.clamp(0.0, 1.0)));
    }
    if supported.weight
        && let Some(v) = axes.weight
    {
        variations.push(("wght", v.clamp(100.0, 700.0)));
    }
    if supported.grade
        && let Some(v) = axes.grade
    {
        variations.push(("GRAD", v.clamp(-25.0, 200.0)));
    }
    if supported.optical_size
        && let Some(v) = axes.optical_size
    {
        variations.push(("opsz", v.clamp(20.0, 48.0)));
    }
    if !variations.is_empty() {
        builder = builder.variations(variations.iter().copied());
    }
    let mut scaler = builder.build();
    let image = Render::new(&[
        Source::ColorOutline(0),
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::Outline,
    ])
    .format(Format::Alpha)
    .render(&mut scaler, GlyphId::from(glyph_id))?;
    if !matches!(image.content, Content::Mask) {
        // Color/bitmap glyphs aren't supported by the monochrome blit path
        // yet; fall back to "missing" by returning None so the resolver chain
        // can show the built-in glyph.
        return None;
    }
    Some(CachedGlyph {
        width: image.placement.width,
        height: image.placement.height,
        placement_left: image.placement.left,
        pixels: image.data.into(),
    })
}

fn cache_lookup(key: GlyphCacheKey) -> Option<Option<CachedGlyph>> {
    let cache = glyph_cache();
    cache.lock().ok()?.get(&key).cloned()
}

fn cache_store(key: GlyphCacheKey, value: Option<CachedGlyph>) {
    let cache = glyph_cache();
    if let Ok(mut guard) = cache.lock() {
        guard.insert(key, value);
    }
}

fn hash_path(path: &Path) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

fn encode_color(color: Color) -> u32 {
    ((color.r as u32) << 24) | ((color.g as u32) << 16) | ((color.b as u32) << 8) | (color.a as u32)
}

fn quantize(value: Option<f32>) -> i32 {
    match value {
        Some(v) => (v * 100.0).round() as i32,
        None => i32::MIN,
    }
}

/// Render a glyph from a font pack into the buffer at the given destination
/// rectangle, recoloring the alpha mask to `tint`. Returns `false` when the
/// glyph couldn't be rasterized (font missing, unmapped codepoint, color
/// glyph) so the caller can fall back to the built-in missing-icon glyph.
pub fn draw_font_glyph(
    buffer: &mut PixelBuffer,
    font_path: &Path,
    codepoint: u32,
    supported_axes: SupportedAxes,
    axes: GlyphAxes,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
) -> bool {
    let px = dest_w.max(dest_h).max(1) as u32;
    let key = GlyphCacheKey {
        font_path: hash_path(font_path),
        codepoint,
        px,
        color: encode_color(tint),
        fill_q: if supported_axes.fill {
            quantize(axes.fill)
        } else {
            i32::MIN
        },
        weight_q: if supported_axes.weight {
            quantize(axes.weight)
        } else {
            i32::MIN
        },
        grade_q: if supported_axes.grade {
            quantize(axes.grade)
        } else {
            i32::MIN
        },
        opsz_q: if supported_axes.optical_size {
            quantize(axes.optical_size)
        } else {
            i32::MIN
        },
    };

    let glyph = match cache_lookup(key) {
        Some(value) => value,
        None => {
            let raster_started = std::time::Instant::now();
            let value = rasterize(font_path, codepoint, px, axes, supported_axes);
            profiling::record_icon_image_raster(raster_started.elapsed());
            cache_store(key, value.clone());
            value
        }
    };
    let Some(glyph) = glyph else {
        return false;
    };

    // Center the rasterized bitmap inside the destination box, accounting
    // for the glyph's own placement offset (typographic top-of-line vs.
    // visual ink box).
    let pad_x = ((dest_w - glyph.width as i32).max(0)) / 2;
    let pad_y = ((dest_h - glyph.height as i32).max(0)) / 2;
    let origin_x = dest_x + pad_x + glyph.placement_left.min(0).abs();
    let origin_y = dest_y + pad_y;

    for row in 0..glyph.height {
        for col in 0..glyph.width {
            let alpha = glyph.pixels[(row * glyph.width + col) as usize];
            if alpha == 0 {
                continue;
            }
            let x = origin_x + col as i32;
            let y = origin_y + row as i32;
            if x < 0 || y < 0 {
                continue;
            }
            let pixel = Color {
                r: tint.r,
                g: tint.g,
                b: tint.b,
                a: ((tint.a as u32 * alpha as u32) / 255) as u8,
            };
            buffer.blend_pixel(x as u32, y as u32, pixel, 255);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn glyph_test_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn clear_glyph_cache() {
        if let Some(cache) = GLYPH_CACHE.get() {
            cache.lock().unwrap().clear();
        }
    }

    fn cached_test_key(
        font_path: &Path,
        tint: Color,
        px: u32,
        supported_axes: SupportedAxes,
        axes: GlyphAxes,
    ) -> GlyphCacheKey {
        GlyphCacheKey {
            font_path: hash_path(font_path),
            codepoint: 'a' as u32,
            px,
            color: encode_color(tint),
            fill_q: if supported_axes.fill {
                quantize(axes.fill)
            } else {
                i32::MIN
            },
            weight_q: if supported_axes.weight {
                quantize(axes.weight)
            } else {
                i32::MIN
            },
            grade_q: if supported_axes.grade {
                quantize(axes.grade)
            } else {
                i32::MIN
            },
            opsz_q: if supported_axes.optical_size {
                quantize(axes.optical_size)
            } else {
                i32::MIN
            },
        }
    }

    #[test]
    fn cached_font_glyph_hits_do_not_record_raster_time() {
        let _guard = glyph_test_lock();
        clear_glyph_cache();
        profiling::reset_raster_metrics();

        let font_path = Path::new("/tmp/phase26-cached-glyph.ttf");
        let tint = Color {
            r: 32,
            g: 96,
            b: 180,
            a: 255,
        };
        let supported_axes = SupportedAxes::default();
        let axes = GlyphAxes::default();
        let key = cached_test_key(font_path, tint, 12, supported_axes, axes);
        cache_store(
            key,
            Some(CachedGlyph {
                width: 2,
                height: 2,
                placement_left: 0,
                pixels: vec![255, 128, 64, 255].into(),
            }),
        );

        let mut buffer = PixelBuffer::new(8, 8);
        assert!(draw_font_glyph(
            &mut buffer,
            font_path,
            'a' as u32,
            supported_axes,
            axes,
            1,
            1,
            12,
            12,
            tint,
        ));

        let metrics = profiling::raster_metrics();
        assert_eq!(
            metrics.icon_image_raster_micros, 0,
            "cache hits should keep cached glyph blits out of icon_image_raster timing"
        );
        assert!(
            buffer.data.iter().any(|channel| *channel > 0),
            "cached glyph draw should still paint into the destination buffer"
        );

        clear_glyph_cache();
    }

    #[test]
    fn glyph_cache_key_separates_tint_size_and_axes() {
        let _guard = glyph_test_lock();
        clear_glyph_cache();

        let font_path = Path::new("/tmp/phase30-cached-glyph.ttf");
        let tint = Color {
            r: 32,
            g: 96,
            b: 180,
            a: 255,
        };
        let alternate_tint = Color {
            r: 200,
            g: 40,
            b: 64,
            a: 255,
        };
        let supported_axes = SupportedAxes {
            fill: true,
            weight: true,
            grade: false,
            optical_size: false,
        };
        let axes = GlyphAxes {
            fill: Some(0.0),
            weight: Some(400.0),
            ..Default::default()
        };
        let base = cached_test_key(font_path, tint, 12, supported_axes, axes);
        let alternate_tint_key =
            cached_test_key(font_path, alternate_tint, 12, supported_axes, axes);
        let alternate_size_key = cached_test_key(font_path, tint, 14, supported_axes, axes);
        let alternate_axes_key = cached_test_key(
            font_path,
            tint,
            12,
            supported_axes,
            GlyphAxes {
                fill: Some(1.0),
                weight: Some(400.0),
                ..Default::default()
            },
        );

        assert_ne!(base, alternate_tint_key);
        assert_ne!(base, alternate_size_key);
        assert_ne!(base, alternate_axes_key);

        let mut buffer = PixelBuffer::new(16, 16);
        assert!(cache_lookup(alternate_tint_key).is_none());
        assert!(!draw_font_glyph(
            &mut buffer,
            font_path,
            'a' as u32,
            supported_axes,
            axes,
            1,
            1,
            12,
            12,
            alternate_tint,
        ));
        assert!(matches!(cache_lookup(alternate_tint_key), Some(None)));
    }
}
