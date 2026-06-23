use super::glyph::{GlyphAxes, draw_font_glyph, draw_font_glyph_on_canvas};
use super::profiling;
use super::{PixelBuffer, PixelCanvasSession};
use image::imageops::FilterType;
use mesh_core_elements::lru::LruCache;
use mesh_core_elements::style::Color;
use mesh_core_icon::{IconResolution, MISSING_ICON_SVG, ResolvedTarget, resolve_icon_result};
use skia_safe::{
    AlphaType, Canvas, ColorType, Data, ImageInfo, Paint, Rect, SamplingOptions, images,
};
use std::cell::RefCell;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

static IMAGE_CACHE: OnceLock<Mutex<LruCache<Arc<Path>, CachedImage>>> = OnceLock::new();
static RASTER_CACHE: OnceLock<Mutex<LruCache<RasterCacheKey, RasterVariant>>> = OnceLock::new();
static SOURCE_IDENTITY_CACHE: OnceLock<Mutex<LruCache<Arc<Path>, CachedSourceIdentity>>> =
    OnceLock::new();
static SVG_CACHEABILITY_CACHE: OnceLock<Mutex<LruCache<Arc<Path>, CachedSvgCacheability>>> =
    OnceLock::new();
const RASTER_CACHE_CAPACITY: usize = 256;
const IMAGE_CACHE_CAPACITY: usize = 256;
const SOURCE_IDENTITY_CACHE_CAPACITY: usize = 1024;
const SVG_CACHEABILITY_CACHE_CAPACITY: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CachedResourceOpacity {
    Unknown,
    Opaque,
    Translucent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RasterSourceKind {
    File,
    MissingIcon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FileFreshness {
    len: u64,
    modified_nanos: u128,
}

#[derive(Debug, Clone)]
struct CachedImage {
    freshness: FileFreshness,
    image: Arc<image::RgbaImage>,
}

#[derive(Debug, Clone)]
struct CachedSourceIdentity {
    freshness: Option<FileFreshness>,
    identity: Arc<Path>,
}

#[derive(Debug, Clone, Copy)]
struct CachedSvgCacheability {
    freshness: FileFreshness,
    cacheable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RasterCacheKey {
    source_kind: RasterSourceKind,
    source_identity: Arc<Path>,
    width: u32,
    height: u32,
    tint: u32,
    multicolor: bool,
    freshness: Option<FileFreshness>,
}

#[derive(Debug, Clone)]
struct RasterVariant {
    width: u32,
    height: u32,
    /// BGRA pixels matching `PixelBuffer` memory order.
    pixels: Arc<[u8]>,
    fully_opaque: bool,
}

fn image_cache() -> &'static Mutex<LruCache<Arc<Path>, CachedImage>> {
    IMAGE_CACHE.get_or_init(|| Mutex::new(LruCache::new(IMAGE_CACHE_CAPACITY)))
}

fn raster_cache() -> &'static Mutex<LruCache<RasterCacheKey, RasterVariant>> {
    RASTER_CACHE.get_or_init(|| Mutex::new(LruCache::new(RASTER_CACHE_CAPACITY)))
}

fn source_identity_cache() -> &'static Mutex<LruCache<Arc<Path>, CachedSourceIdentity>> {
    SOURCE_IDENTITY_CACHE.get_or_init(|| Mutex::new(LruCache::new(SOURCE_IDENTITY_CACHE_CAPACITY)))
}

fn svg_cacheability_cache() -> &'static Mutex<LruCache<Arc<Path>, CachedSvgCacheability>> {
    SVG_CACHEABILITY_CACHE
        .get_or_init(|| Mutex::new(LruCache::new(SVG_CACHEABILITY_CACHE_CAPACITY)))
}

fn get_or_load(path: &Path) -> Option<Arc<image::RgbaImage>> {
    let Some(freshness) = file_freshness(path) else {
        return image::open(path)
            .ok()
            .map(|image| Arc::new(image.to_rgba8()));
    };
    if let Ok(mut guard) = image_cache().lock()
        && let Some(cached) = guard.get(path)
        && cached.freshness == freshness
    {
        return Some(Arc::clone(&cached.image));
    }
    let img = Arc::new(image::open(path).ok()?.to_rgba8());
    if let Ok(mut guard) = image_cache().lock() {
        guard.insert(
            Arc::from(path),
            CachedImage {
                freshness,
                image: Arc::clone(&img),
            },
        );
    }
    Some(img)
}

pub(crate) fn load_image_rgba(path: &Path) -> Option<Arc<image::RgbaImage>> {
    get_or_load(path)
}

fn encode_tint(color: Color) -> u32 {
    ((color.r as u32) << 24) | ((color.g as u32) << 16) | ((color.b as u32) << 8) | color.a as u32
}

fn file_freshness(path: &Path) -> Option<FileFreshness> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let modified_nanos = modified
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(FileFreshness {
        len: metadata.len(),
        modified_nanos,
    })
}

fn source_identity(path: &Path, freshness: Option<FileFreshness>) -> Arc<Path> {
    let cache = source_identity_cache();
    if let Ok(mut guard) = cache.lock()
        && let Some(cached) = guard.get(path)
        && cached.freshness == freshness
    {
        return Arc::clone(&cached.identity);
    }

    let identity: Arc<Path> = match std::fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(_) if path.is_absolute() => path.to_path_buf(),
        Err(_) => std::env::current_dir()
            .ok()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|| path.to_path_buf()),
    }
    .into();

    if let Ok(mut guard) = cache.lock() {
        guard.insert(
            Arc::from(path),
            CachedSourceIdentity {
                freshness,
                identity: Arc::clone(&identity),
            },
        );
    }

    identity
}

fn raster_file_key(
    path: &Path,
    width: u32,
    height: u32,
    tint: Color,
    multicolor: bool,
) -> Option<RasterCacheKey> {
    let freshness = file_freshness(path)?;
    Some(raster_file_key_with_freshness(
        path, width, height, tint, multicolor, freshness,
    ))
}

fn raster_file_key_with_freshness(
    path: &Path,
    width: u32,
    height: u32,
    tint: Color,
    multicolor: bool,
    freshness: FileFreshness,
) -> RasterCacheKey {
    RasterCacheKey {
        source_kind: RasterSourceKind::File,
        source_identity: source_identity(path, Some(freshness)),
        width,
        height,
        tint: encode_tint(tint),
        multicolor,
        freshness: Some(freshness),
    }
}

fn svg_file_cacheability(path: &Path) -> Option<(bool, FileFreshness)> {
    let freshness = file_freshness(path)?;
    let cache = svg_cacheability_cache();
    if let Ok(mut guard) = cache.lock()
        && let Some(cached) = guard.get(path)
        && cached.freshness == freshness
    {
        return Some((cached.cacheable, freshness));
    }

    let Ok(svg_data) = std::fs::read_to_string(path) else {
        return None;
    };
    let cacheable = !svg_has_external_resource_reference(&svg_data);
    if let Ok(mut guard) = cache.lock() {
        guard.insert(
            Arc::from(path),
            CachedSvgCacheability {
                freshness,
                cacheable,
            },
        );
    }
    Some((cacheable, freshness))
}

fn svg_has_external_resource_reference(svg_data: &str) -> bool {
    let mut remaining = svg_data;
    while let Some(index) = remaining.find("href") {
        remaining = &remaining[index + "href".len()..];
        let trimmed = remaining.trim_start();
        let trimmed = trimmed.strip_prefix('=').unwrap_or(trimmed).trim_start();
        let Some(quote) = trimmed
            .chars()
            .next()
            .filter(|ch| *ch == '"' || *ch == '\'')
        else {
            continue;
        };
        let value = &trimmed[quote.len_utf8()..];
        let Some(end) = value.find(quote) else {
            return true;
        };
        let reference = value[..end].trim();
        if !reference.is_empty() && !reference.starts_with('#') && !reference.starts_with("data:") {
            return true;
        }
        remaining = &value[end + quote.len_utf8()..];
    }

    let mut remaining = svg_data;
    while let Some(index) = remaining.find("url(") {
        let after = &remaining[index + "url(".len()..];
        let Some(end) = after.find(')') else {
            return true;
        };
        let reference = after[..end]
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim();
        if !reference.is_empty() && !reference.starts_with('#') && !reference.starts_with("data:") {
            return true;
        }
        remaining = &after[end + 1..];
    }

    false
}

fn missing_icon_key(width: u32, height: u32, tint: Color) -> RasterCacheKey {
    RasterCacheKey {
        source_kind: RasterSourceKind::MissingIcon,
        source_identity: Arc::from(Path::new("builtin:missing-icon")),
        width,
        height,
        tint: encode_tint(tint),
        multicolor: false,
        freshness: None,
    }
}

fn cached_variant(key: &RasterCacheKey) -> Option<RasterVariant> {
    raster_cache().lock().ok()?.get(key).cloned()
}

pub(crate) fn cached_file_resource_opacity(
    path: &Path,
    width: u32,
    height: u32,
    tint: Color,
    multicolor: bool,
) -> CachedResourceOpacity {
    let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
        return CachedResourceOpacity::Unknown;
    };
    let ext = ext.to_ascii_lowercase();
    let key = match ext.as_str() {
        "svg" => {
            let Some((true, freshness)) = svg_file_cacheability(path) else {
                return CachedResourceOpacity::Unknown;
            };
            raster_file_key_with_freshness(path, width, height, tint, multicolor, freshness)
        }
        "png" | "jpg" | "jpeg" | "bmp" => {
            let Some(freshness) = file_freshness(path) else {
                return CachedResourceOpacity::Unknown;
            };
            raster_file_key_with_freshness(path, width, height, tint, multicolor, freshness)
        }
        _ => return CachedResourceOpacity::Unknown,
    };
    let Some(variant) = cached_variant(&key) else {
        return CachedResourceOpacity::Unknown;
    };
    if variant.fully_opaque {
        CachedResourceOpacity::Opaque
    } else {
        CachedResourceOpacity::Translucent
    }
}

fn store_variant(key: RasterCacheKey, variant: RasterVariant) {
    if let Ok(mut cache) = raster_cache().lock() {
        cache.insert(key, variant);
    }
}

const ICON_SKIA_CACHE_CAPACITY: usize = 256;

struct CachedIconImage {
    _keep_alive: Arc<[u8]>,
    image: skia_safe::Image,
}

thread_local! {
    /// Skia images derived from cached `RasterVariant::pixels`. Keyed by
    /// the raw pointer of the `Arc<[u8]>` so distinct cache misses on the
    /// same logical icon (same variant Arc) share one Skia upload. Hits
    /// reuse the Image; the cache holds a strong `Arc` reference so the
    /// underlying allocation cannot be freed while the Image is live.
    static ICON_SKIA_CACHE: RefCell<LruCache<usize, CachedIconImage>> =
        RefCell::new(LruCache::new(ICON_SKIA_CACHE_CAPACITY));
}

fn cached_skia_image_for_variant(variant: &RasterVariant) -> Option<skia_safe::Image> {
    if variant.width == 0 || variant.height == 0 || variant.pixels.is_empty() {
        return None;
    }
    let key = variant.pixels.as_ptr() as usize;
    ICON_SKIA_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        if let Some(entry) = cache.get(&key) {
            return Some(entry.image.clone());
        }
        let info = ImageInfo::new(
            (variant.width as i32, variant.height as i32),
            ColorType::BGRA8888,
            AlphaType::Unpremul,
            None,
        );
        let row_bytes = (variant.width as usize) * 4;
        // SAFETY of the Arc keep-alive: skia-safe's `images::raster_from_data`
        // takes an owned `Data`. Below we build it from the Arc's bytes
        // via `Data::new_copy`, so Skia owns its own copy and the Arc
        // strong reference in the cache is just for cache identity
        // (pointer key stability).
        let data = Data::new_copy(variant.pixels.as_ref());
        let image = images::raster_from_data(&info, data, row_bytes)?;
        cache.insert(
            key,
            CachedIconImage {
                _keep_alive: Arc::clone(&variant.pixels),
                image: image.clone(),
            },
        );
        Some(image)
    })
}

fn blit_variant_on_canvas(canvas: &Canvas, variant: &RasterVariant, dest_x: i32, dest_y: i32) {
    let Some(image) = cached_skia_image_for_variant(variant) else {
        return;
    };
    let dest = Rect::from_xywh(
        dest_x as f32,
        dest_y as f32,
        variant.width as f32,
        variant.height as f32,
    );
    let mut paint = Paint::default();
    paint.set_anti_alias(false);
    canvas.draw_image_rect_with_sampling_options(
        &image,
        None,
        dest,
        SamplingOptions::default(),
        &paint,
    );
}

fn blit_variant(buffer: &mut PixelBuffer, variant: &RasterVariant, dest_x: i32, dest_y: i32) {
    let src_x = (-dest_x).max(0) as u32;
    let src_y = (-dest_y).max(0) as u32;
    let dst_x = dest_x.max(0) as u32;
    let dst_y = dest_y.max(0) as u32;
    if src_x >= variant.width
        || src_y >= variant.height
        || dst_x >= buffer.width
        || dst_y >= buffer.height
    {
        return;
    }

    let copy_w = (variant.width - src_x).min(buffer.width - dst_x);
    let copy_h = (variant.height - src_y).min(buffer.height - dst_y);
    if copy_w == 0 || copy_h == 0 {
        return;
    }

    let src_stride = variant.width as usize * 4;
    let row_bytes = copy_w as usize * 4;
    let src_x_offset = src_x as usize * 4;
    let dst_x_offset = dst_x as usize * 4;
    if variant.fully_opaque {
        for row in 0..copy_h as usize {
            let src_start = (src_y as usize + row) * src_stride + src_x_offset;
            let dst_start = (dst_y as usize + row) * buffer.stride as usize + dst_x_offset;
            let src_end = src_start + row_bytes;
            let dst_end = dst_start + row_bytes;
            if src_end <= variant.pixels.len() && dst_end <= buffer.data.len() {
                buffer.data[dst_start..dst_end]
                    .copy_from_slice(&variant.pixels[src_start..src_end]);
            }
        }
        return;
    }

    let dst_stride = buffer.stride as usize;
    for row in 0..copy_h as usize {
        let src_row_start = (src_y as usize + row) * src_stride + src_x_offset;
        let dst_row_start = (dst_y as usize + row) * dst_stride + dst_x_offset;
        let src_row_end = src_row_start + row_bytes;
        let dst_row_end = dst_row_start + row_bytes;
        if src_row_end > variant.pixels.len() || dst_row_end > buffer.data.len() {
            continue;
        }

        let src_row = &variant.pixels[src_row_start..src_row_end];
        let dst_row = &mut buffer.data[dst_row_start..dst_row_end];
        for (src_px, dst_px) in src_row.chunks_exact(4).zip(dst_row.chunks_exact_mut(4)) {
            let src_alpha = u16::from(src_px[3]);
            if src_alpha == 0 {
                continue;
            }
            let inv_alpha = 255u16.saturating_sub(src_alpha);
            let dst_b = u16::from(dst_px[0]);
            let dst_g = u16::from(dst_px[1]);
            let dst_r = u16::from(dst_px[2]);
            let dst_a = u16::from(dst_px[3]);

            dst_px[0] = ((u16::from(src_px[0]) * src_alpha + dst_b * inv_alpha) / 255) as u8;
            dst_px[1] = ((u16::from(src_px[1]) * src_alpha + dst_g * inv_alpha) / 255) as u8;
            dst_px[2] = ((u16::from(src_px[2]) * src_alpha + dst_r * inv_alpha) / 255) as u8;
            dst_px[3] = (src_alpha + ((dst_a * inv_alpha) / 255)).min(255) as u8;
        }
    }
}

fn variant_from_bgra(width: u32, height: u32, pixels: Vec<u8>) -> RasterVariant {
    let fully_opaque = pixels.chunks_exact(4).all(|pixel| pixel[3] == 255);
    RasterVariant {
        width,
        height,
        pixels: Arc::from(pixels.into_boxed_slice()),
        fully_opaque,
    }
}

fn raster_bitmap_variant(
    path: &Path,
    width: u32,
    height: u32,
    tint: Color,
    multicolor: bool,
) -> Option<RasterVariant> {
    let img = get_or_load(path)?;
    let scaled = image::imageops::resize(img.as_ref(), width, height, FilterType::Lanczos3);
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let p = scaled.get_pixel(x, y);
            if multicolor {
                pixels.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            } else {
                pixels.extend_from_slice(&[tint.b, tint.g, tint.r, p[3]]);
            }
        }
    }
    Some(variant_from_bgra(width, height, pixels))
}

fn raster_svg_variant(
    path: &Path,
    width: u32,
    height: u32,
    tint: Color,
    multicolor: bool,
) -> Option<RasterVariant> {
    let svg_data = std::fs::read_to_string(path).ok()?;
    let opt = resvg::usvg::Options {
        resources_dir: path.parent().map(|p| p.to_path_buf()),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(&svg_data, &opt).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    let scale_x = width as f32 / tree.size().width();
    let scale_y = height as f32 / tree.size().height();
    let transform = resvg::tiny_skia::Transform::from_scale(scale_x, scale_y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(variant_from_pixmap(
        width,
        height,
        pixmap.data(),
        tint,
        multicolor,
    ))
}

fn raster_missing_icon_variant(width: u32, height: u32, tint: Color) -> Option<RasterVariant> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(MISSING_ICON_SVG, &opt).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    let scale_x = width as f32 / tree.size().width();
    let scale_y = height as f32 / tree.size().height();
    let transform = resvg::tiny_skia::Transform::from_scale(scale_x, scale_y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(variant_from_pixmap(
        width,
        height,
        pixmap.data(),
        tint,
        false,
    ))
}

fn variant_from_pixmap(
    width: u32,
    height: u32,
    data: &[u8],
    tint: Color,
    multicolor: bool,
) -> RasterVariant {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for py in 0..height {
        for px in 0..width {
            let idx = (py * width + px) as usize * 4;
            if multicolor {
                pixels.extend_from_slice(&[data[idx + 2], data[idx + 1], data[idx], data[idx + 3]]);
            } else {
                pixels.extend_from_slice(&[tint.b, tint.g, tint.r, data[idx + 3]]);
            }
        }
    }
    variant_from_bgra(width, height, pixels)
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
    if let Some(variant) = resolve_file_variant(path, dest_w, dest_h, tint, multicolor) {
        blit_variant(buffer, &variant, dest_x, dest_y);
    }
}

pub fn draw_icon_from_path_with_options_on_canvas(
    canvas: &Canvas,
    path: &Path,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
    multicolor: bool,
) {
    if let Some(variant) = resolve_file_variant(path, dest_w, dest_h, tint, multicolor) {
        blit_variant_on_canvas(canvas, &variant, dest_x, dest_y);
    }
}

/// Resolve a file-backed icon to a (possibly cached) `RasterVariant`,
/// recording cache hit/miss/bypass metrics. Used by both the buffer and
/// canvas blit paths so they share one rasterizer and cache.
fn resolve_file_variant(
    path: &Path,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
    multicolor: bool,
) -> Option<RasterVariant> {
    let ext = path.extension().and_then(|e| e.to_str())?;
    let width = dest_w.max(1) as u32;
    let height = dest_h.max(1) as u32;
    let ext = ext.to_ascii_lowercase();
    let key = match ext.as_str() {
        "svg" => {
            let (cacheable, freshness) = svg_file_cacheability(path)?;
            if cacheable {
                Some(raster_file_key_with_freshness(
                    path, width, height, tint, multicolor, freshness,
                ))
            } else {
                None
            }
        }
        "png" | "jpg" | "jpeg" | "bmp" => raster_file_key(path, width, height, tint, multicolor),
        _ => None,
    };
    if let Some(key) = key.as_ref()
        && let Some(variant) = cached_variant(key)
    {
        profiling::record_raster_cache_hit(variant.fully_opaque);
        return Some(variant);
    }

    let variant = match ext.as_str() {
        "png" | "jpg" | "jpeg" | "bmp" => {
            if key.is_some() {
                profiling::record_raster_cache_miss();
            } else {
                profiling::record_raster_cache_bypass();
            }
            let raster_started = std::time::Instant::now();
            let variant = raster_bitmap_variant(path, width, height, tint, multicolor);
            profiling::record_icon_image_raster(raster_started.elapsed());
            variant
        }
        "svg" => {
            if key.is_some() {
                profiling::record_raster_cache_miss();
            } else {
                profiling::record_raster_cache_bypass();
            }
            let raster_started = std::time::Instant::now();
            let variant = raster_svg_variant(path, width, height, tint, multicolor);
            profiling::record_icon_image_raster(raster_started.elapsed());
            variant
        }
        _ => None,
    }?;

    if let Some(key) = key {
        store_variant(key, variant.clone());
    }
    Some(variant)
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
    if let Some(variant) = resolve_missing_icon_variant(dest_w, dest_h, tint) {
        blit_variant(buffer, &variant, dest_x, dest_y);
    }
}

pub fn draw_missing_icon_fallback_on_canvas(
    canvas: &Canvas,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
) {
    if let Some(variant) = resolve_missing_icon_variant(dest_w, dest_h, tint) {
        blit_variant_on_canvas(canvas, &variant, dest_x, dest_y);
    }
}

fn resolve_missing_icon_variant(dest_w: i32, dest_h: i32, tint: Color) -> Option<RasterVariant> {
    let width = dest_w.max(1) as u32;
    let height = dest_h.max(1) as u32;
    let key = missing_icon_key(width, height, tint);
    if let Some(variant) = cached_variant(&key) {
        profiling::record_raster_cache_hit(variant.fully_opaque);
        return Some(variant);
    }

    profiling::record_raster_cache_miss();
    let raster_started = std::time::Instant::now();
    let variant = raster_missing_icon_variant(width, height, tint)?;
    profiling::record_icon_image_raster(raster_started.elapsed());

    store_variant(key, variant.clone());
    Some(variant)
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
    draw_icon_resolution_with_axes(
        buffer,
        resolve_icon_result(name, size),
        dest_x,
        dest_y,
        dest_w,
        dest_h,
        tint,
        GlyphAxes::default(),
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

/// Session-aware variant of [`draw_icon_from_path`]. File-backed icons
/// route through the active canvas; the session is unchanged for callers
/// that interleave further Skia draws.
pub fn draw_icon_from_path_in_session(
    session: &mut PixelCanvasSession<'_>,
    path: &Path,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
) {
    session.with_canvas(|canvas| {
        draw_icon_from_path_with_options_on_canvas(
            canvas, path, dest_x, dest_y, dest_w, dest_h, tint, false,
        );
    });
}

/// Session-aware variant of [`draw_named_icon`]. File-backed and icon-font
/// glyph targets both route through the active canvas.
pub fn draw_named_icon_in_session(
    session: &mut PixelCanvasSession<'_>,
    name: &str,
    size: u32,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
) {
    draw_icon_resolution_with_axes_in_session(
        session,
        resolve_icon_result(name, size),
        dest_x,
        dest_y,
        dest_w,
        dest_h,
        tint,
        GlyphAxes::default(),
    );
}

pub fn draw_named_icon_for_module_in_session(
    session: &mut PixelCanvasSession<'_>,
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
    draw_icon_resolution_with_axes_in_session(
        session,
        mesh_core_icon::resolve_icon_for_module(module_id, name, size),
        dest_x,
        dest_y,
        dest_w,
        dest_h,
        tint,
        axes,
    );
}

fn draw_icon_resolution_with_axes_in_session(
    session: &mut PixelCanvasSession<'_>,
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
        } => {
            session.with_canvas(|canvas| {
                draw_icon_from_path_with_options_on_canvas(
                    canvas, &path, dest_x, dest_y, dest_w, dest_h, tint, multicolor,
                );
            });
        }
        IconResolution::Found {
            target:
                ResolvedTarget::Glyph {
                    font_path,
                    codepoint,
                    supported_axes,
                },
            ..
        } => {
            let drew = session
                .with_canvas(|canvas| {
                    draw_font_glyph_on_canvas(
                        canvas,
                        &font_path,
                        codepoint,
                        supported_axes,
                        axes,
                        dest_x,
                        dest_y,
                        dest_w,
                        dest_h,
                        tint,
                    )
                })
                .unwrap_or(false);
            if !drew {
                session.with_canvas(|canvas| {
                    draw_missing_icon_fallback_on_canvas(
                        canvas, dest_x, dest_y, dest_w, dest_h, tint,
                    );
                });
            }
        }
        IconResolution::Missing { .. } => {
            session.with_canvas(|canvas| {
                draw_missing_icon_fallback_on_canvas(canvas, dest_x, dest_y, dest_w, dest_h, tint);
            });
        }
    }
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
    draw_icon_resolution_with_axes(
        buffer,
        mesh_core_icon::resolve_icon_with_registry(registry, name, size),
        dest_x,
        dest_y,
        dest_w,
        dest_h,
        tint,
        GlyphAxes::default(),
    );
}

/// Draws a resolved icon. Lets the caller supply variable-font
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
    use std::sync::{Mutex, MutexGuard, OnceLock};
    #[cfg(unix)]
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    fn icon_test_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

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

    fn clear_icon_caches() {
        if let Some(cache) = RASTER_CACHE.get() {
            cache.lock().unwrap().clear();
        }
        if let Some(cache) = IMAGE_CACHE.get() {
            cache.lock().unwrap().clear();
        }
        profiling::reset_raster_metrics();
    }

    fn write_test_svg(path: &Path) {
        fs::write(
            path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect x="1" y="1" width="6" height="6" fill="black"/></svg>"#,
        )
        .unwrap();
    }

    #[test]
    fn svg_external_resource_references_bypass_raster_cache() {
        let _guard = icon_test_lock();
        clear_icon_caches();
        let td = tempfile::tempdir().unwrap();
        let image_path = td.path().join("linked.png");
        let svg_path = td.path().join("linked.svg");
        ImageBuffer::from_fn(2, 2, |_, _| Rgba([255u8, 0, 0, 255]))
            .save(&image_path)
            .unwrap();
        fs::write(
            &svg_path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><image href="linked.png" width="8" height="8"/></svg>"#,
        )
        .unwrap();

        let mut buffer = PixelBuffer::new(16, 16);
        draw_icon_from_path_with_options(&mut buffer, &svg_path, 0, 0, 8, 8, tint(), true);
        draw_icon_from_path_with_options(&mut buffer, &svg_path, 0, 0, 8, 8, tint(), true);

        let metrics = profiling::raster_metrics();
        assert_eq!(metrics.raster_cache_hits, 0);
        assert_eq!(metrics.raster_cache_misses, 0);
        assert_eq!(metrics.raster_cache_bypasses, 2);
    }

    #[cfg(unix)]
    #[test]
    fn source_identity_preserves_distinct_non_utf8_paths() {
        let _guard = icon_test_lock();
        clear_icon_caches();
        let td = tempfile::tempdir().unwrap();
        let first = td
            .path()
            .join(OsString::from_vec(vec![b'i', 0xff, b'.', b'p', b'n', b'g']));
        let second = td
            .path()
            .join(OsString::from_vec(vec![b'i', 0xfe, b'.', b'p', b'n', b'g']));
        ImageBuffer::from_fn(1, 1, |_, _| Rgba([255u8, 0, 0, 255]))
            .save(&first)
            .unwrap();
        ImageBuffer::from_fn(1, 1, |_, _| Rgba([0u8, 255, 0, 255]))
            .save(&second)
            .unwrap();

        let first_key = raster_file_key(&first, 8, 8, tint(), true).unwrap();
        let second_key = raster_file_key(&second, 8, 8, tint(), true).unwrap();

        assert_ne!(first_key.source_identity, second_key.source_identity);
        assert_ne!(first_key, second_key);
    }

    #[test]
    fn svg_icon_rasterizes_and_tints() {
        let _guard = icon_test_lock();
        clear_icon_caches();
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("symbol.svg");
        write_test_svg(&path);

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
        let _guard = icon_test_lock();
        clear_icon_caches();
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
        let _guard = icon_test_lock();
        clear_icon_caches();
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
        let _guard = icon_test_lock();
        clear_icon_caches();
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
    fn svg_raster_variant_cache_reuses_unchanged_key() {
        let _guard = icon_test_lock();
        clear_icon_caches();
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("symbol.svg");
        write_test_svg(&path);

        let mut first = PixelBuffer::new(24, 24);
        draw_icon_from_path(&mut first, &path, 4, 4, 12, 12, tint());
        let first_metrics = profiling::raster_metrics();
        assert_eq!(first_metrics.raster_cache_misses, 1);
        assert_eq!(first_metrics.raster_cache_hits, 0);

        profiling::reset_raster_metrics();
        let mut second = PixelBuffer::new(24, 24);
        draw_icon_from_path(&mut second, &path, 4, 4, 12, 12, tint());
        let second_metrics = profiling::raster_metrics();
        assert_eq!(second_metrics.raster_cache_hits, 1);
        assert_eq!(second_metrics.raster_cache_misses, 0);
        assert_eq!(second_metrics.icon_image_raster_micros, 0);
    }

    #[test]
    fn raster_variant_cache_separates_tint_and_multicolor_keys() {
        let _guard = icon_test_lock();
        clear_icon_caches();
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("symbol.svg");
        write_test_svg(&path);

        let mut buffer = PixelBuffer::new(24, 24);
        draw_icon_from_path(&mut buffer, &path, 2, 2, 12, 12, tint());

        profiling::reset_raster_metrics();
        let alternate_tint = Color {
            r: 240,
            g: 40,
            b: 80,
            a: 255,
        };
        draw_icon_from_path(&mut buffer, &path, 2, 2, 12, 12, alternate_tint);
        let tint_metrics = profiling::raster_metrics();
        assert_eq!(tint_metrics.raster_cache_hits, 0);
        assert_eq!(tint_metrics.raster_cache_misses, 1);

        profiling::reset_raster_metrics();
        draw_icon_from_path_with_options(&mut buffer, &path, 2, 2, 12, 12, tint(), true);
        let multicolor_metrics = profiling::raster_metrics();
        assert_eq!(multicolor_metrics.raster_cache_hits, 0);
        assert_eq!(multicolor_metrics.raster_cache_misses, 1);
    }

    #[test]
    fn bitmap_raster_variant_cache_invalidates_when_file_freshness_changes() {
        let _guard = icon_test_lock();
        clear_icon_caches();
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("logo.png");
        let image = ImageBuffer::from_fn(2, 2, |_, _| Rgba([255u8, 0, 0, 255]));
        image.save(&path).unwrap();

        let mut buffer = PixelBuffer::new(24, 24);
        draw_icon_from_path_with_options(&mut buffer, &path, 2, 2, 12, 12, tint(), true);

        let replacement = ImageBuffer::from_fn(5, 4, |_, _| Rgba([0u8, 255, 0, 255]));
        replacement.save(&path).unwrap();

        profiling::reset_raster_metrics();
        draw_icon_from_path_with_options(&mut buffer, &path, 2, 2, 12, 12, tint(), true);
        let metrics = profiling::raster_metrics();
        assert_eq!(metrics.raster_cache_hits, 0);
        assert_eq!(metrics.raster_cache_misses, 1);
    }

    #[test]
    fn raster_variant_cache_reports_opaque_and_translucent_hits() {
        let _guard = icon_test_lock();
        clear_icon_caches();
        let td = tempfile::tempdir().unwrap();
        let opaque_path = td.path().join("opaque.png");
        let translucent_path = td.path().join("translucent.png");
        ImageBuffer::from_fn(2, 2, |_, _| Rgba([255u8, 0, 0, 255]))
            .save(&opaque_path)
            .unwrap();
        ImageBuffer::from_fn(2, 2, |x, _| {
            if x == 0 {
                Rgba([0u8, 255, 0, 255])
            } else {
                Rgba([0u8, 255, 0, 96])
            }
        })
        .save(&translucent_path)
        .unwrap();

        let mut buffer = PixelBuffer::new(24, 24);
        draw_icon_from_path(&mut buffer, &opaque_path, 0, 0, 10, 10, tint());
        draw_icon_from_path(&mut buffer, &translucent_path, 12, 0, 10, 10, tint());

        profiling::reset_raster_metrics();
        draw_icon_from_path(&mut buffer, &opaque_path, 0, 0, 10, 10, tint());
        draw_icon_from_path(&mut buffer, &translucent_path, 12, 0, 10, 10, tint());
        let metrics = profiling::raster_metrics();
        assert_eq!(metrics.raster_cache_hits, 2);
        assert_eq!(metrics.raster_cache_misses, 0);
        assert_eq!(metrics.raster_cache_opaque_hits, 1);
        assert_eq!(metrics.raster_cache_translucent_hits, 1);
    }

    #[test]
    fn missing_icon_fallback_uses_raster_variant_cache() {
        let _guard = icon_test_lock();
        clear_icon_caches();
        let mut buffer = PixelBuffer::new(24, 24);
        draw_missing_icon_fallback(&mut buffer, 2, 2, 16, 16, tint());

        profiling::reset_raster_metrics();
        draw_missing_icon_fallback(&mut buffer, 2, 2, 16, 16, tint());
        let metrics = profiling::raster_metrics();
        assert_eq!(metrics.raster_cache_hits, 1);
        assert_eq!(metrics.raster_cache_misses, 0);
    }

    #[test]
    fn draw_named_icon_uses_destination_box_for_missing_fallback() {
        let _guard = icon_test_lock();
        clear_icon_caches();
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
