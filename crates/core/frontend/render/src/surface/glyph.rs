//! Font glyph rasterization for icon font packs (Material Symbols, Nerd
//! Font, etc.). Uses swash to render a single glyph at a requested pixel
//! size with optional variable-font axis settings (FILL, wght, GRAD,
//! opsz). Output is an 8-bit alpha mask that the painter blits with the
//! icon's tint color, so glyphs flow through the same theme-token coloring
//! path as monochrome SVG icons.

#[cfg(test)]
use super::PixelBuffer;
use super::profiling;
use super::{checked_pixel_bytes, reserve_render_resource_work, resource_generation_is_current};
use mesh_core_elements::lru::ByteLruCache;
use mesh_core_elements::style::Color;
use mesh_core_icon::SupportedAxes;
use mesh_core_resources::{ResourceFingerprint, resource_fingerprint, resource_revision};
use skia_safe::{
    AlphaType, Canvas, ColorType, Data, ImageInfo, Paint, Rect, SamplingOptions, images,
};
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
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
    resource_revision: u64,
    font_fingerprint: Option<ResourceFingerprint>,
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

struct GlyphJob {
    key: GlyphCacheKey,
    font_path: std::path::PathBuf,
    codepoint: u32,
    px: u32,
    axes: GlyphAxes,
    supported: SupportedAxes,
    _budget: mesh_core_resources::ResourceByteReservation,
}

struct GlyphJobResult {
    key: GlyphCacheKey,
    glyph: Option<CachedGlyph>,
    cancelled: bool,
    elapsed: std::time::Duration,
    _budget: mesh_core_resources::ResourceByteReservation,
}

struct GlyphRasterQueue {
    sender: Option<mpsc::SyncSender<GlyphJob>>,
    receiver: Receiver<GlyphJobResult>,
    pending: HashSet<GlyphCacheKey>,
}

static GLYPH_RASTER_QUEUE: OnceLock<Mutex<GlyphRasterQueue>> = OnceLock::new();
static GLYPH_RASTER_RESULTS_READY: AtomicBool = AtomicBool::new(false);

fn glyph_raster_queue() -> &'static Mutex<GlyphRasterQueue> {
    GLYPH_RASTER_QUEUE.get_or_init(|| {
        const GLYPH_RASTER_QUEUE_CAPACITY: usize = 64;
        let (request_sender, request_receiver) =
            mpsc::sync_channel::<GlyphJob>(GLYPH_RASTER_QUEUE_CAPACITY);
        let (result_sender, result_receiver) = mpsc::channel::<GlyphJobResult>();
        let worker = std::thread::Builder::new()
            .name("mesh-glyph-raster".into())
            .spawn(move || {
                while let Ok(job) = request_receiver.recv() {
                    let started = std::time::Instant::now();
                    let initial_current = resource_generation_is_current(job.key.resource_revision);
                    let glyph = if initial_current {
                        rasterize(
                            &job.font_path,
                            job.codepoint,
                            job.px,
                            job.axes,
                            job.supported,
                        )
                    } else {
                        None
                    };
                    let cancelled = !initial_current
                        || !resource_generation_is_current(job.key.resource_revision);
                    if result_sender
                        .send(GlyphJobResult {
                            key: job.key,
                            glyph: if cancelled { None } else { glyph },
                            cancelled,
                            elapsed: started.elapsed(),
                            _budget: job._budget,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .ok();

        Mutex::new(GlyphRasterQueue {
            sender: worker.map(|_| request_sender),
            receiver: result_receiver,
            pending: HashSet::new(),
        })
    })
}

fn drain_glyph_raster_jobs() -> bool {
    let results = {
        let Some(queue) = GLYPH_RASTER_QUEUE.get() else {
            return false;
        };
        let Ok(mut queue) = queue.lock() else {
            return false;
        };
        let mut results = Vec::new();
        while let Ok(result) = queue.receiver.try_recv() {
            queue.pending.remove(&result.key);
            results.push(result);
        }
        results
    };

    let mut completed = false;
    for result in results {
        if result.cancelled || !resource_generation_is_current(result.key.resource_revision) {
            continue;
        }
        profiling::record_icon_image_raster(result.elapsed);
        cache_store(result.key, result.glyph);
        completed = true;
    }
    if completed {
        GLYPH_RASTER_RESULTS_READY.store(true, Ordering::Release);
    }
    completed
}

pub fn poll_glyph_raster_jobs() -> bool {
    drain_glyph_raster_jobs() || GLYPH_RASTER_RESULTS_READY.swap(false, Ordering::AcqRel)
}

pub fn glyph_raster_jobs_pending() -> bool {
    let pending = GLYPH_RASTER_QUEUE
        .get()
        .and_then(|queue| queue.lock().ok())
        .is_some_and(|queue| !queue.pending.is_empty());
    pending || GLYPH_RASTER_RESULTS_READY.load(Ordering::Acquire)
}

/// Queue one font glyph for off-thread byte loading and swash rasterization.
/// `None` means the worker is unavailable; `Some(false)` means an equivalent
/// job is already pending or the bounded queue is full.
fn schedule_glyph_raster_job(
    key: GlyphCacheKey,
    font_path: &Path,
    codepoint: u32,
    px: u32,
    axes: GlyphAxes,
    supported: SupportedAxes,
) -> Option<bool> {
    let Some(work_bytes) = glyph_work_bytes(px) else {
        return Some(false);
    };
    let Ok(mut queue) = glyph_raster_queue().lock() else {
        return None;
    };
    let Some(sender) = queue.sender.as_ref().cloned() else {
        return None;
    };
    if !queue.pending.insert(key.clone()) {
        return Some(false);
    }
    let Some(budget) = reserve_render_resource_work(work_bytes) else {
        queue.pending.remove(&key);
        return Some(false);
    };
    let job = GlyphJob {
        key,
        font_path: font_path.to_path_buf(),
        codepoint,
        px,
        axes,
        supported,
        _budget: budget,
    };
    match sender.try_send(job) {
        Ok(()) => Some(true),
        Err(mpsc::TrySendError::Full(job)) => {
            queue.pending.remove(&job.key);
            Some(false)
        }
        Err(mpsc::TrySendError::Disconnected(job)) => {
            queue.pending.remove(&job.key);
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FontBytesCacheKey {
    path: Arc<Path>,
    resource_revision: u64,
    fingerprint: ResourceFingerprint,
}

type FontBytesCache = Mutex<ByteLruCache<FontBytesCacheKey, Arc<[u8]>>>;

static FONT_BYTES: OnceLock<FontBytesCache> = OnceLock::new();
static GLYPH_CACHE: OnceLock<Mutex<ByteLruCache<GlyphCacheKey, Option<CachedGlyph>>>> =
    OnceLock::new();
const FONT_BYTES_CACHE_CAPACITY: usize = 32;
const GLYPH_CACHE_CAPACITY: usize = 1024;
const FONT_BYTES_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;
const GLYPH_CACHE_MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_GLYPH_PX: u32 = 2048;

// Thread-local Skia image cache for icon-font glyph renders, keyed by GlyphCacheKey.
// Avoids rebuilding Skia A8 images on every paint frame.
const ICON_GLYPH_SKIA_CAPACITY: usize = 512;
const ICON_GLYPH_SKIA_MAX_BYTES: usize = 8 * 1024 * 1024;
thread_local! {
    static ICON_GLYPH_SKIA: RefCell<ByteLruCache<GlyphCacheKey, skia_safe::Image>> =
        RefCell::new(ByteLruCache::new(ICON_GLYPH_SKIA_CAPACITY, ICON_GLYPH_SKIA_MAX_BYTES));
}

fn font_bytes_cache() -> &'static FontBytesCache {
    FONT_BYTES.get_or_init(|| {
        Mutex::new(ByteLruCache::new(
            FONT_BYTES_CACHE_CAPACITY,
            FONT_BYTES_CACHE_MAX_BYTES,
        ))
    })
}

fn glyph_cache() -> &'static Mutex<ByteLruCache<GlyphCacheKey, Option<CachedGlyph>>> {
    GLYPH_CACHE.get_or_init(|| {
        Mutex::new(ByteLruCache::new(
            GLYPH_CACHE_CAPACITY,
            GLYPH_CACHE_MAX_BYTES,
        ))
    })
}

fn font_bytes(path: &Path) -> Option<Arc<[u8]>> {
    let revision = resource_revision();
    let fingerprint = resource_fingerprint(path)?;
    let key = FontBytesCacheKey {
        path: Arc::from(path),
        resource_revision: revision,
        fingerprint,
    };
    let cache = font_bytes_cache();
    let entries = match cache.lock() {
        Ok(mut guard) => {
            let entries = guard.len();
            if let Some(bytes) = guard.get(&key) {
                profiling::record_font_bytes_cache_lookup(true, entries, FONT_BYTES_CACHE_CAPACITY);
                return Some(Arc::clone(bytes));
            }
            entries
        }
        Err(_) => 0,
    };
    profiling::record_font_bytes_cache_lookup(false, entries, FONT_BYTES_CACHE_CAPACITY);
    let bytes: Arc<[u8]> = std::fs::read(path).ok()?.into();
    if let Ok(mut guard) = cache.lock() {
        guard.insert(key, Arc::clone(&bytes), bytes.len());
        profiling::update_font_bytes_cache_entries(guard.len(), FONT_BYTES_CACHE_CAPACITY);
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
    glyph_work_bytes(px)?;
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
    let mut guard = cache.lock().ok()?;
    let value = guard.get(&key).cloned();
    profiling::record_glyph_cache_lookup(value.is_some(), guard.len(), GLYPH_CACHE_CAPACITY);
    value
}

fn cache_store(key: GlyphCacheKey, value: Option<CachedGlyph>) {
    let cache = glyph_cache();
    if let Ok(mut guard) = cache.lock() {
        let weight = value.as_ref().map_or(1, |glyph| glyph.pixels.len().max(1));
        guard.insert(key, value, weight);
        profiling::update_glyph_cache_entries(guard.len(), GLYPH_CACHE_CAPACITY);
    }
}

fn glyph_work_bytes(px: u32) -> Option<usize> {
    if px == 0 || px > MAX_GLYPH_PX {
        return None;
    }
    checked_pixel_bytes(px, px, 1)
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

fn glyph_cache_key(
    font_path: &Path,
    codepoint: u32,
    px: u32,
    supported_axes: SupportedAxes,
    axes: GlyphAxes,
    tint: Color,
) -> GlyphCacheKey {
    GlyphCacheKey {
        font_path: hash_path(font_path),
        resource_revision: resource_revision(),
        font_fingerprint: resource_fingerprint(font_path),
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
    }
}

/// Render a glyph from a font pack into the buffer at the given destination
/// rectangle, recoloring the alpha mask to `tint`. Returns `false` when the
/// glyph couldn't be rasterized (font missing, unmapped codepoint, color
/// glyph) so the caller can fall back to the built-in missing-icon glyph.
#[cfg(test)]
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
    let key = glyph_cache_key(font_path, codepoint, px, supported_axes, axes, tint);

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

/// Render a glyph from a font pack onto a Skia canvas, using a thread-local
/// Skia image cache to avoid rebuilding the A8 image on every paint frame.
/// Returns `false` when the glyph couldn't be rasterized.
pub fn draw_font_glyph_on_canvas(
    canvas: &Canvas,
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
    drain_glyph_raster_jobs();
    let px = dest_w.max(dest_h).max(1) as u32;
    let key = glyph_cache_key(font_path, codepoint, px, supported_axes, axes, tint);

    let glyph = match cache_lookup(key) {
        Some(value) => value,
        None => {
            #[cfg(test)]
            {
                let raster_started = std::time::Instant::now();
                let value = rasterize(font_path, codepoint, px, axes, supported_axes);
                profiling::record_icon_image_raster(raster_started.elapsed());
                cache_store(key, value.clone());
                value
            }
            #[cfg(not(test))]
            {
                match schedule_glyph_raster_job(key, font_path, codepoint, px, axes, supported_axes)
                {
                    Some(_) => None,
                    None => {
                        let raster_started = std::time::Instant::now();
                        let value = rasterize(font_path, codepoint, px, axes, supported_axes);
                        profiling::record_icon_image_raster(raster_started.elapsed());
                        cache_store(key, value.clone());
                        value
                    }
                }
            }
        }
    };
    let Some(glyph) = glyph else {
        return false;
    };

    let skia_image = ICON_GLYPH_SKIA.with(|cell| {
        let mut atlas = cell.borrow_mut();
        let entries = atlas.len();
        if let Some(img) = atlas.get(&key) {
            profiling::record_skia_glyph_cache_lookup(true, entries, ICON_GLYPH_SKIA_CAPACITY);
            return Some(img.clone());
        }
        profiling::record_skia_glyph_cache_lookup(false, entries, ICON_GLYPH_SKIA_CAPACITY);
        let info = ImageInfo::new(
            (glyph.width as i32, glyph.height as i32),
            ColorType::Alpha8,
            AlphaType::Premul,
            None,
        );
        let row_bytes = glyph.width as usize;
        let data = Data::new_copy(&glyph.pixels);
        let img = images::raster_from_data(&info, data, row_bytes)?;
        atlas.insert(key, img.clone(), glyph.pixels.len());
        profiling::update_skia_glyph_cache_entries(atlas.len(), ICON_GLYPH_SKIA_CAPACITY);
        Some(img)
    });
    let Some(skia_image) = skia_image else {
        return false;
    };

    let pad_x = ((dest_w - glyph.width as i32).max(0)) / 2;
    let pad_y = ((dest_h - glyph.height as i32).max(0)) / 2;
    let origin_x = dest_x + pad_x + glyph.placement_left.min(0).abs();
    let origin_y = dest_y + pad_y;

    let dest_rect = Rect::from_xywh(
        origin_x as f32,
        origin_y as f32,
        glyph.width as f32,
        glyph.height as f32,
    );
    let mut paint = Paint::default();
    paint.set_anti_alias(false);
    paint.set_argb(tint.a, tint.r, tint.g, tint.b);
    canvas.draw_image_rect_with_sampling_options(
        &skia_image,
        None,
        dest_rect,
        SamplingOptions::default(),
        &paint,
    );
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
        glyph_cache_key(font_path, 'a' as u32, px, supported_axes, axes, tint)
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
        assert_eq!(metrics.glyph_cache_hits, 1);
        assert_eq!(metrics.glyph_cache_misses, 0);
        assert_eq!(metrics.glyph_cache_entries, 1);
        assert_eq!(metrics.glyph_cache_capacity, GLYPH_CACHE_CAPACITY as u64);
        assert!(
            buffer.data.iter().any(|channel| *channel > 0),
            "cached glyph draw should still paint into the destination buffer"
        );

        clear_glyph_cache();
    }

    #[test]
    fn bundled_material_symbols_font_rasterizes_with_variable_axes() {
        let font_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../../../modules/icon-packs/material-symbols/assets/MaterialSymbolsRounded.ttf",
        );
        assert!(font_path.is_file());

        let glyph = rasterize(
            &font_path,
            0xe8b8,
            24,
            GlyphAxes {
                fill: Some(1.0),
                weight: Some(500.0),
                grade: Some(0.0),
                optical_size: Some(24.0),
            },
            SupportedAxes {
                fill: true,
                weight: true,
                grade: true,
                optical_size: true,
            },
        )
        .expect("settings glyph should rasterize");

        assert!(glyph.width > 0 && glyph.height > 0);
        assert!(glyph.pixels.iter().any(|alpha| *alpha > 0));
    }

    #[test]
    fn glyph_raster_worker_transfers_font_glyph_to_cache() {
        let _guard = glyph_test_lock();
        clear_glyph_cache();
        let font_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../../../modules/icon-packs/material-symbols/assets/MaterialSymbolsRounded.ttf",
        );
        let supported = SupportedAxes {
            fill: true,
            weight: true,
            grade: true,
            optical_size: true,
        };
        let axes = GlyphAxes {
            fill: Some(1.0),
            weight: Some(500.0),
            grade: Some(0.0),
            optical_size: Some(24.0),
        };
        let tint = Color {
            r: 32,
            g: 96,
            b: 180,
            a: 255,
        };
        let key = glyph_cache_key(&font_path, 0xe8b8, 24, supported, axes, tint);

        assert_eq!(
            schedule_glyph_raster_job(key, &font_path, 0xe8b8, 24, axes, supported,),
            Some(true)
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let prepared = loop {
            drain_glyph_raster_jobs();
            if matches!(cache_lookup(key), Some(Some(_))) {
                break true;
            }
            if std::time::Instant::now() >= deadline {
                break false;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert!(prepared, "worker did not publish the font glyph");
        assert!(poll_glyph_raster_jobs());
        assert!(!glyph_raster_jobs_pending());
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

    #[test]
    fn glyph_work_budget_rejects_oversized_raster_requests() {
        assert_eq!(glyph_work_bytes(1), Some(1));
        assert!(glyph_work_bytes(MAX_GLYPH_PX).is_some());
        assert!(glyph_work_bytes(MAX_GLYPH_PX + 1).is_none());
    }
}
