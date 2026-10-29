#[cfg(test)]
use super::PixelBuffer;
use super::PixelCanvasSession;
#[cfg(test)]
use super::glyph::draw_font_glyph;
use super::glyph::{GlyphAxes, draw_font_glyph_on_canvas};
use super::profiling;
use super::resource_broker::{PendingResourceConsumers, ResourceBroker, ResourceBrokerContext};
use super::{checked_pixel_bytes, resource_generation_is_current};
use image::imageops::FilterType;
use mesh_core_elements::lru::ByteLruCache;
use mesh_core_elements::style::Color;
#[cfg(test)]
use mesh_core_icon::MISSING_ICON_SVG;
#[cfg(test)]
use mesh_core_icon::resolve_icon_result;
use mesh_core_icon::{IconResolution, ResolvedTarget};
use mesh_core_resources::blob::{
    BLOB_SCHEME, Blob, BlobFormat, EncodedImageFormat, RawPixelLayout, lookup_blob,
};
use mesh_core_resources::{
    ResourceByteReservation, ResourceFingerprint, resource_fingerprint, resource_revision,
};
#[cfg(not(test))]
use skia_safe::PaintStyle;
use skia_safe::{
    AlphaType, Canvas, ColorType, Data, ImageInfo, Paint, Rect, SamplingOptions, images,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};

static IMAGE_CACHE: OnceLock<Mutex<ByteLruCache<Arc<Path>, CachedImage>>> = OnceLock::new();
static RASTER_CACHE: OnceLock<Mutex<ByteLruCache<RasterCacheKey, RasterVariant>>> = OnceLock::new();
static SOURCE_IDENTITY_CACHE: OnceLock<Mutex<ByteLruCache<Arc<Path>, CachedSourceIdentity>>> =
    OnceLock::new();
#[cfg(test)]
static SVG_CACHEABILITY_CACHE: OnceLock<Mutex<ByteLruCache<Arc<Path>, CachedSvgCacheability>>> =
    OnceLock::new();
const RASTER_CACHE_CAPACITY: usize = 256;
const IMAGE_CACHE_CAPACITY: usize = 256;
const SOURCE_IDENTITY_CACHE_CAPACITY: usize = 1024;
#[cfg(test)]
const SVG_CACHEABILITY_CACHE_CAPACITY: usize = 1024;
const RASTER_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;
const IMAGE_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;
const METADATA_CACHE_MAX_BYTES: usize = 256 * 1024;
const MAX_ICON_DIMENSION: u32 = 2048;
const MAX_ICON_RASTER_BYTES: usize = 16 * 1024 * 1024;
const MAX_TRANSFORM_RASTER_SCALE: f32 = 4.0;
const MAX_SOURCE_IMAGE_DIMENSION: u32 = 8192;
const MAX_SOURCE_IMAGE_BYTES: usize = 64 * 1024 * 1024;
#[cfg(test)]
static FILE_FRESHNESS_STAT_PROBES: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CachedResourceOpacity {
    Unknown,
    Opaque,
    Translucent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RasterSourceKind {
    File,
    /// A provider blob; its identity is the content-addressed handle.
    Blob,
    #[cfg(test)]
    MissingIcon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IconFileKind {
    Svg,
    Bitmap,
}

fn icon_file_kind(path: &Path) -> Option<IconFileKind> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("svg") {
        Some(IconFileKind::Svg)
    } else if ["png", "jpg", "jpeg", "bmp"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    {
        Some(IconFileKind::Bitmap)
    } else {
        None
    }
}

type FileFreshness = ResourceFingerprint;

#[derive(Debug, Clone)]
struct CachedImage {
    resource_revision: u64,
    freshness: FileFreshness,
    image: Arc<image::RgbaImage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ImageDecodeKey {
    path: Arc<Path>,
    freshness: FileFreshness,
}

#[derive(Debug, Clone)]
struct CachedSourceIdentity {
    resource_revision: u64,
    freshness: Option<FileFreshness>,
    identity: Arc<Path>,
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct CachedSvgCacheability {
    resource_revision: u64,
    freshness: FileFreshness,
    cacheable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RasterCacheKey {
    resource_revision: u64,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct LinkedResourceFingerprint {
    path: Arc<Path>,
    fingerprint: Option<FileFreshness>,
}

struct RasterJobResult {
    key: RasterCacheKey,
    variant: Option<RasterVariant>,
    cacheable: bool,
    linked_resources: Vec<LinkedResourceFingerprint>,
    cancelled: bool,
    elapsed: std::time::Duration,
    _budget: ResourceByteReservation,
}

struct IconRasterQueue {
    result_sender: Sender<RasterJobResult>,
    receiver: Receiver<RasterJobResult>,
    pending: PendingResourceConsumers<RasterCacheKey>,
    ready: HashMap<
        RasterCacheKey,
        (
            RasterVariant,
            Vec<LinkedResourceFingerprint>,
            ResourceByteReservation,
        ),
    >,
}

static ICON_RASTER_QUEUE: OnceLock<Mutex<IconRasterQueue>> = OnceLock::new();
static ICON_RASTER_RESULTS_READY: AtomicBool = AtomicBool::new(false);

struct ImageDecodeJobResult {
    key: ImageDecodeKey,
    image: Option<Arc<image::RgbaImage>>,
    cancelled: bool,
    _budget: ResourceByteReservation,
}

struct ImageDecodeQueue {
    result_sender: Sender<ImageDecodeJobResult>,
    receiver: Receiver<ImageDecodeJobResult>,
    pending: PendingResourceConsumers<ImageDecodeKey>,
    ready: HashMap<ImageDecodeKey, (Arc<image::RgbaImage>, ResourceByteReservation)>,
}

static IMAGE_DECODE_QUEUE: OnceLock<Mutex<ImageDecodeQueue>> = OnceLock::new();
static IMAGE_DECODE_RESULTS_READY: AtomicBool = AtomicBool::new(false);

#[cfg(not(test))]
const ICON_RESOLUTION_WORK_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct IconResolutionKey {
    resource_revision: u64,
    module_id: String,
    semantic_name: String,
    size: u32,
}

struct IconResolutionJobResult {
    key: IconResolutionKey,
    resolution: Option<IconResolution>,
    cancelled: bool,
    _budget: ResourceByteReservation,
}

struct IconResolutionQueue {
    #[cfg(not(test))]
    result_sender: Sender<IconResolutionJobResult>,
    receiver: Receiver<IconResolutionJobResult>,
    pending: PendingResourceConsumers<IconResolutionKey>,
    ready: HashMap<IconResolutionKey, (IconResolution, ResourceByteReservation)>,
    /// Keys whose job also found nothing at this resource revision. Without
    /// it every paint of a missing icon resubmits the job, and each settled
    /// job repaints its consumers, so the surface never idles.
    missing: HashSet<IconResolutionKey>,
}

static ICON_RESOLUTION_QUEUE: OnceLock<Mutex<IconResolutionQueue>> = OnceLock::new();
static ICON_RESOLUTION_RESULTS_READY: AtomicBool = AtomicBool::new(false);

fn icon_raster_queue() -> &'static Mutex<IconRasterQueue> {
    ICON_RASTER_QUEUE.get_or_init(|| {
        let (result_sender, result_receiver) = mpsc::channel::<RasterJobResult>();
        Mutex::new(IconRasterQueue {
            result_sender,
            receiver: result_receiver,
            pending: PendingResourceConsumers::default(),
            ready: HashMap::new(),
        })
    })
}

fn image_decode_queue() -> &'static Mutex<ImageDecodeQueue> {
    IMAGE_DECODE_QUEUE.get_or_init(|| {
        let (result_sender, result_receiver) = mpsc::channel::<ImageDecodeJobResult>();
        Mutex::new(ImageDecodeQueue {
            result_sender,
            receiver: result_receiver,
            pending: PendingResourceConsumers::default(),
            ready: HashMap::new(),
        })
    })
}

#[cfg(not(test))]
fn icon_resolution_queue() -> &'static Mutex<IconResolutionQueue> {
    ICON_RESOLUTION_QUEUE.get_or_init(|| {
        #[cfg(not(test))]
        let (result_sender, result_receiver) = mpsc::channel::<IconResolutionJobResult>();
        #[cfg(test)]
        let (_, result_receiver) = mpsc::channel::<IconResolutionJobResult>();
        Mutex::new(IconResolutionQueue {
            #[cfg(not(test))]
            result_sender,
            receiver: result_receiver,
            pending: PendingResourceConsumers::default(),
            ready: HashMap::new(),
            missing: HashSet::new(),
        })
    })
}

#[cfg(not(test))]
fn missing_icon_resolution(name: &str) -> IconResolution {
    IconResolution::Missing {
        semantic_name: name.to_string(),
        tried: Vec::new(),
    }
}

fn drain_icon_resolution_jobs() -> bool {
    let results = {
        let Some(queue) = ICON_RESOLUTION_QUEUE.get() else {
            return false;
        };
        let Ok(mut queue) = queue.lock() else {
            return false;
        };
        let current_generation = resource_revision();
        queue
            .ready
            .retain(|key, _| key.resource_revision == current_generation);
        queue
            .missing
            .retain(|key| key.resource_revision == current_generation);
        let mut results = Vec::new();
        while let Ok(result) = queue.receiver.try_recv() {
            let waiters = queue.pending.remove(&result.key);
            results.push((result, waiters));
        }
        results
    };

    let mut completed = false;
    if let Some(queue) = ICON_RESOLUTION_QUEUE.get()
        && let Ok(mut queue) = queue.lock()
    {
        for (result, waiters) in results {
            if !resource_generation_is_current(result.key.resource_revision) {
                continue;
            }
            if result.cancelled {
                queue.missing.insert(result.key);
                waiters.notify();
                completed = true;
                continue;
            }
            match result.resolution {
                Some(IconResolution::Missing { .. }) => {
                    queue.missing.insert(result.key);
                    waiters.notify();
                    completed = true;
                }
                Some(resolution) => {
                    let prepared = mesh_core_icon::prepared_icon_for_module(
                        &result.key.module_id,
                        &result.key.semantic_name,
                        result.key.size,
                    )
                    .is_some();
                    waiters.notify();
                    // The registry already owns admitted results. Only uncached
                    // deliveries retain a one-shot reservation for their painter.
                    if !prepared {
                        queue.ready.insert(result.key, (resolution, result._budget));
                    }
                    completed = true;
                }
                None => {}
            }
        }
    }
    if completed {
        ICON_RESOLUTION_RESULTS_READY.store(true, Ordering::Release);
    }
    completed
}

#[cfg(not(test))]
fn take_ready_icon_resolution(key: &IconResolutionKey) -> Option<IconResolution> {
    let queue = ICON_RESOLUTION_QUEUE.get()?;
    queue
        .lock()
        .ok()?
        .ready
        .remove(key)
        .map(|(resolution, _budget)| resolution)
}

pub fn poll_icon_resolution_jobs() -> bool {
    drain_icon_resolution_jobs() || ICON_RESOLUTION_RESULTS_READY.swap(false, Ordering::AcqRel)
}

pub fn icon_resolution_jobs_pending() -> bool {
    let pending = ICON_RESOLUTION_QUEUE
        .get()
        .and_then(|queue| queue.lock().ok())
        .is_some_and(|queue| !queue.pending.is_empty());
    pending || ICON_RESOLUTION_RESULTS_READY.load(Ordering::Acquire)
}

#[cfg(not(test))]
fn resolve_icon_for_render(module_id: &str, name: &str, size: u32) -> Option<IconResolution> {
    drain_icon_resolution_jobs();
    let key = IconResolutionKey {
        resource_revision: resource_revision(),
        module_id: module_id.into(),
        semantic_name: name.into(),
        size,
    };
    if let Some(resolution) = take_ready_icon_resolution(&key) {
        return Some(resolution);
    }
    if let Some(resolution) = mesh_core_icon::prepared_icon_for_module(module_id, name, size) {
        return Some(resolution);
    }
    let Some(broker) = ResourceBroker::global() else {
        return Some(mesh_core_icon::resolve_icon_for_module(
            module_id, name, size,
        ));
    };
    let Ok(mut queue) = icon_resolution_queue().lock() else {
        return None;
    };
    if queue.missing.contains(&key) {
        return Some(missing_icon_resolution(name));
    }
    if !queue.pending.insert(key.clone()) {
        return None;
    }
    let result_sender = queue.result_sender.clone();
    let result_key = key.clone();
    let cancellation_key = key.clone();
    let cancellation_sender = result_sender.clone();
    let job_module_id = module_id.to_string();
    let job_name = name.to_string();
    let scheduled = broker.submit(
        ICON_RESOLUTION_WORK_BYTES,
        move |context: ResourceBrokerContext| {
            let resolution = context.is_current().then(|| {
                mesh_core_icon::resolve_icon_for_module_on_worker(&job_module_id, &job_name, size)
            });
            let cancelled = !context.is_current() || resolution.is_none();
            let reservation = context.into_reservation();
            let _ = result_sender.send(IconResolutionJobResult {
                key: result_key,
                resolution,
                cancelled,
                _budget: reservation,
            });
        },
        move |reservation| {
            let _ = cancellation_sender.send(IconResolutionJobResult {
                key: cancellation_key,
                resolution: None,
                cancelled: true,
                _budget: reservation,
            });
        },
    );
    if !matches!(scheduled, Some(true)) {
        queue.pending.remove(&key);
        drop(queue);
        if !broker.retry_after_next_completion() {
            return Some(mesh_core_icon::resolve_icon_for_module(
                module_id, name, size,
            ));
        }
    }
    None
}

fn icon_raster_bytes(width: u32, height: u32) -> Option<usize> {
    if width == 0 || height == 0 || width > MAX_ICON_DIMENSION || height > MAX_ICON_DIMENSION {
        return None;
    }
    let bytes = checked_pixel_bytes(width, height, 4)?;
    (bytes <= MAX_ICON_RASTER_BYTES).then_some(bytes)
}

const IMAGE_DECODE_WORK_BYTES: usize = MAX_SOURCE_IMAGE_BYTES;

fn drain_image_decode_jobs() -> bool {
    let results = {
        let Some(queue) = IMAGE_DECODE_QUEUE.get() else {
            return false;
        };
        let Ok(mut queue) = queue.lock() else {
            return false;
        };
        let mut results = Vec::new();
        while let Ok(result) = queue.receiver.try_recv() {
            let waiters = queue.pending.remove(&result.key);
            results.push((result, waiters));
        }
        results
    };

    let mut completed = false;
    if let Some(queue) = IMAGE_DECODE_QUEUE.get()
        && let Ok(mut queue) = queue.lock()
    {
        for (result, waiters) in results {
            if result.cancelled
                || result.image.is_none()
                || resource_fingerprint(&result.key.path) != Some(result.key.freshness)
            {
                continue;
            }
            waiters.notify();
            queue.ready.insert(
                result.key,
                (result.image.expect("image checked above"), result._budget),
            );
            completed = true;
        }
    }
    if completed {
        IMAGE_DECODE_RESULTS_READY.store(true, Ordering::Release);
    }
    completed
}

fn take_ready_image(key: &ImageDecodeKey) -> Option<Arc<image::RgbaImage>> {
    let queue = IMAGE_DECODE_QUEUE.get()?;
    queue
        .lock()
        .ok()?
        .ready
        .remove(key)
        .and_then(|(image, _budget)| {
            (resource_fingerprint(&key.path) == Some(key.freshness)).then_some(image)
        })
}

fn schedule_image_decode_job(key: ImageDecodeKey) -> Option<bool> {
    let Ok(mut queue) = image_decode_queue().lock() else {
        return None;
    };
    if !queue.pending.insert(key.clone()) {
        return Some(false);
    }
    let result_sender = queue.result_sender.clone();
    let result_key = key.clone();
    let cancellation_key = key.clone();
    let cancellation_sender = result_sender.clone();
    let job_path = Arc::clone(&key.path);
    let Some(broker) = ResourceBroker::global() else {
        queue.pending.remove(&key);
        return None;
    };
    let scheduled = broker.submit(
        IMAGE_DECODE_WORK_BYTES,
        move |context: ResourceBrokerContext| {
            let image = if context.is_current() && source_image_is_bounded(&job_path) {
                image::open(job_path.as_ref())
                    .ok()
                    .map(|decoded| Arc::new(decoded.to_rgba8()))
            } else {
                None
            };
            let cancelled = !context.is_current();
            let reservation = context.into_reservation();
            let _ = result_sender.send(ImageDecodeJobResult {
                key: result_key,
                image: if cancelled { None } else { image },
                cancelled,
                _budget: reservation,
            });
        },
        move |reservation| {
            let _ = cancellation_sender.send(ImageDecodeJobResult {
                key: cancellation_key,
                image: None,
                cancelled: true,
                _budget: reservation,
            });
        },
    );
    match scheduled {
        Some(true) => Some(true),
        Some(false) => {
            queue.pending.remove(&key);
            Some(false)
        }
        None => {
            queue.pending.remove(&key);
            None
        }
    }
}

pub fn poll_image_decode_jobs() -> bool {
    drain_image_decode_jobs() || IMAGE_DECODE_RESULTS_READY.swap(false, Ordering::AcqRel)
}

pub fn image_decode_jobs_pending() -> bool {
    let pending = IMAGE_DECODE_QUEUE
        .get()
        .and_then(|queue| queue.lock().ok())
        .is_some_and(|queue| !queue.pending.is_empty());
    pending || IMAGE_DECODE_RESULTS_READY.load(Ordering::Acquire)
}

fn source_image_is_bounded(path: &Path) -> bool {
    let Ok((width, height)) = image::image_dimensions(path) else {
        return false;
    };
    width <= MAX_SOURCE_IMAGE_DIMENSION
        && height <= MAX_SOURCE_IMAGE_DIMENSION
        && checked_pixel_bytes(width, height, 4)
            .is_some_and(|bytes| bytes <= MAX_SOURCE_IMAGE_BYTES)
}

fn icon_dimensions(dest_w: i32, dest_h: i32) -> Option<(u32, u32)> {
    let width = u32::try_from(dest_w.max(1)).ok()?;
    let height = u32::try_from(dest_h.max(1)).ok()?;
    icon_raster_bytes(width, height).map(|_| (width, height))
}

fn drain_icon_raster_jobs() -> bool {
    let results = {
        let Some(queue) = ICON_RASTER_QUEUE.get() else {
            return false;
        };
        let Ok(mut queue) = queue.lock() else {
            return false;
        };
        let current_generation = mesh_core_resources::resource_revision();
        queue.ready.retain(|key, (_, linked_resources, _)| {
            key.resource_revision == current_generation
                && linked_resources_are_current(linked_resources)
        });
        let mut results = Vec::new();
        while let Ok(result) = queue.receiver.try_recv() {
            let waiters = queue.pending.remove(&result.key);
            results.push((result, waiters));
        }
        results
    };

    let mut cached_results = Vec::new();
    let mut ready_results = Vec::new();
    for (result, waiters) in results {
        if result.cancelled || !resource_generation_is_current(result.key.resource_revision) {
            continue;
        }
        profiling::record_icon_image_raster(result.elapsed);
        if result.cacheable {
            profiling::record_raster_cache_miss();
        } else {
            profiling::record_raster_cache_bypass();
        }
        if let Some(variant) = result.variant {
            waiters.notify();
            if result.cacheable {
                cached_results.push((result.key, variant, result._budget));
            } else {
                ready_results.push((result.key, variant, result.linked_resources, result._budget));
            }
        }
    }

    let cached_completed = !cached_results.is_empty();
    for (key, variant, budget) in cached_results {
        store_variant(key, variant);
        drop(budget);
    }
    let mut completed = cached_completed;
    if !ready_results.is_empty()
        && let Some(queue) = ICON_RASTER_QUEUE.get()
        && let Ok(mut queue) = queue.lock()
    {
        for (key, variant, linked_resources, budget) in ready_results {
            queue.ready.insert(key, (variant, linked_resources, budget));
        }
        completed = true;
    }
    if completed {
        ICON_RASTER_RESULTS_READY.store(true, Ordering::Release);
    }
    completed
}

fn take_ready_variant(key: &RasterCacheKey) -> Option<RasterVariant> {
    let queue = ICON_RASTER_QUEUE.get()?;
    queue
        .lock()
        .ok()?
        .ready
        .remove(key)
        .and_then(|(variant, linked_resources, _budget)| {
            linked_resources_are_current(&linked_resources).then_some(variant)
        })
}

pub fn poll_icon_raster_jobs() -> bool {
    drain_icon_raster_jobs() || ICON_RASTER_RESULTS_READY.swap(false, Ordering::AcqRel)
}

pub fn icon_raster_jobs_pending() -> bool {
    let pending = ICON_RASTER_QUEUE
        .get()
        .and_then(|queue| queue.lock().ok())
        .is_some_and(|queue| !queue.pending.is_empty());
    pending || ICON_RASTER_RESULTS_READY.load(Ordering::Acquire)
}

/// Where a raster job reads its source pixels.
#[cfg_attr(test, allow(dead_code))]
enum RasterSource {
    File { path: PathBuf, kind: IconFileKind },
    Blob(Arc<Blob>),
}

/// Queue one file- or blob-backed icon for off-thread decode and
/// rasterization. `None` means the worker could not be started;
/// `Some(false)` means an equivalent job is already in flight or the bounded
/// queue is full.
fn schedule_icon_raster_job(
    key: RasterCacheKey,
    source: RasterSource,
    width: u32,
    height: u32,
    tint: Color,
    multicolor: bool,
) -> Option<bool> {
    let Some(work_bytes) = icon_raster_bytes(width, height) else {
        return Some(false);
    };
    let Ok(mut queue) = icon_raster_queue().lock() else {
        return None;
    };
    if !queue.pending.insert(key.clone()) {
        return Some(false);
    }
    let result_sender = queue.result_sender.clone();
    let result_key = key.clone();
    let cancellation_key = key.clone();
    let cancellation_sender = result_sender.clone();
    let Some(broker) = ResourceBroker::global() else {
        queue.pending.remove(&key);
        return None;
    };
    let scheduled = broker.submit(
        work_bytes,
        move |context: ResourceBrokerContext| {
            let started = std::time::Instant::now();
            let (variant, cacheable, linked_resources) = if !context.is_current() {
                (None, false, Vec::new())
            } else {
                match source {
                    RasterSource::Blob(blob) => (
                        raster_blob_variant(&blob, width, height, tint, multicolor),
                        true,
                        Vec::new(),
                    ),
                    RasterSource::File {
                        path: job_path,
                        kind: IconFileKind::Bitmap,
                    } => (
                        raster_bitmap_variant(&job_path, width, height, tint, multicolor),
                        true,
                        Vec::new(),
                    ),
                    RasterSource::File {
                        path: job_path,
                        kind: IconFileKind::Svg,
                    } => match std::fs::read_to_string(&job_path) {
                        Ok(svg_data) => {
                            let linked_resources =
                                linked_resource_fingerprints(&job_path, &svg_data);
                            (
                                raster_svg_variant_from_data(
                                    &job_path, &svg_data, width, height, tint, multicolor,
                                ),
                                !svg_has_external_resource_reference(&svg_data),
                                linked_resources,
                            )
                        }
                        Err(_) => (None, false, Vec::new()),
                    },
                }
            };
            let cancelled =
                !context.is_current() || !linked_resources_are_current(&linked_resources);
            let reservation = context.into_reservation();
            let _ = result_sender.send(RasterJobResult {
                key: result_key,
                variant: if cancelled { None } else { variant },
                cacheable: if cancelled { false } else { cacheable },
                linked_resources,
                cancelled,
                elapsed: started.elapsed(),
                _budget: reservation,
            });
        },
        move |reservation| {
            let _ = cancellation_sender.send(RasterJobResult {
                key: cancellation_key,
                variant: None,
                cacheable: false,
                linked_resources: Vec::new(),
                cancelled: true,
                elapsed: std::time::Duration::ZERO,
                _budget: reservation,
            });
        },
    );
    match scheduled {
        Some(true) => Some(true),
        Some(false) => {
            queue.pending.remove(&key);
            Some(false)
        }
        None => {
            queue.pending.remove(&key);
            None
        }
    }
}

fn image_cache() -> &'static Mutex<ByteLruCache<Arc<Path>, CachedImage>> {
    IMAGE_CACHE.get_or_init(|| {
        Mutex::new(ByteLruCache::new(
            IMAGE_CACHE_CAPACITY,
            IMAGE_CACHE_MAX_BYTES,
        ))
    })
}

fn raster_cache() -> &'static Mutex<ByteLruCache<RasterCacheKey, RasterVariant>> {
    RASTER_CACHE.get_or_init(|| {
        Mutex::new(ByteLruCache::new(
            RASTER_CACHE_CAPACITY,
            RASTER_CACHE_MAX_BYTES,
        ))
    })
}

fn source_identity_cache() -> &'static Mutex<ByteLruCache<Arc<Path>, CachedSourceIdentity>> {
    SOURCE_IDENTITY_CACHE.get_or_init(|| {
        Mutex::new(ByteLruCache::new(
            SOURCE_IDENTITY_CACHE_CAPACITY,
            METADATA_CACHE_MAX_BYTES,
        ))
    })
}

#[cfg(test)]
fn svg_cacheability_cache() -> &'static Mutex<ByteLruCache<Arc<Path>, CachedSvgCacheability>> {
    SVG_CACHEABILITY_CACHE.get_or_init(|| {
        Mutex::new(ByteLruCache::new(
            SVG_CACHEABILITY_CACHE_CAPACITY,
            METADATA_CACHE_MAX_BYTES,
        ))
    })
}

fn get_or_load_sync(path: &Path) -> Option<Arc<image::RgbaImage>> {
    if !source_image_is_bounded(path) {
        return None;
    }
    let resource_revision = resource_revision();
    let Some(freshness) = file_freshness(path) else {
        return image::open(path)
            .ok()
            .map(|image| Arc::new(image.to_rgba8()));
    };
    if let Ok(mut guard) = image_cache().lock()
        && let Some(cached) = guard.get(path)
        && cached.resource_revision == resource_revision
        && cached.freshness == freshness
    {
        return Some(Arc::clone(&cached.image));
    }
    let img = Arc::new(image::open(path).ok()?.to_rgba8());
    if let Ok(mut guard) = image_cache().lock() {
        guard.insert(
            Arc::from(path),
            CachedImage {
                resource_revision,
                freshness,
                image: Arc::clone(&img),
            },
            img.as_raw().len(),
        );
    }
    Some(img)
}

#[cfg(test)]
fn get_or_load(path: &Path) -> Option<Arc<image::RgbaImage>> {
    get_or_load_sync(path)
}

#[cfg(not(test))]
fn get_or_load(path: &Path) -> Option<Arc<image::RgbaImage>> {
    let freshness = resource_fingerprint(path)?;
    let key = ImageDecodeKey {
        path: Arc::from(path),
        freshness,
    };
    if let Ok(mut guard) = image_cache().lock()
        && let Some(cached) = guard.get(path)
        && cached.freshness == key.freshness
    {
        return Some(Arc::clone(&cached.image));
    }
    if let Some(image) = take_ready_image(&key) {
        if let Ok(mut guard) = image_cache().lock() {
            guard.insert(
                Arc::clone(&key.path),
                CachedImage {
                    resource_revision: resource_revision(),
                    freshness: key.freshness,
                    image: Arc::clone(&image),
                },
                image.as_raw().len(),
            );
        }
        return Some(image);
    }
    let _ = schedule_image_decode_job(key);
    None
}

pub(crate) fn load_image_rgba(path: &Path) -> Option<Arc<image::RgbaImage>> {
    get_or_load(path)
}

fn encode_tint(color: Color) -> u32 {
    ((color.r as u32) << 24) | ((color.g as u32) << 16) | ((color.b as u32) << 8) | color.a as u32
}

fn file_freshness(path: &Path) -> Option<FileFreshness> {
    #[cfg(test)]
    FILE_FRESHNESS_STAT_PROBES.fetch_add(1, Ordering::Relaxed);

    resource_fingerprint(path)
}

fn source_identity(path: &Path, freshness: Option<FileFreshness>) -> Arc<Path> {
    let resource_revision = resource_revision();
    let cache = source_identity_cache();
    if let Ok(mut guard) = cache.lock()
        && let Some(cached) = guard.get(path)
        && cached.resource_revision == resource_revision
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
                resource_revision,
                freshness,
                identity: Arc::clone(&identity),
            },
            metadata_cache_weight(path).saturating_add(identity.as_os_str().len()),
        );
    }

    identity
}

#[cfg(test)]
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
        resource_revision: resource_revision(),
        source_kind: RasterSourceKind::File,
        source_identity: source_identity(path, Some(freshness)),
        width,
        height,
        tint: encode_tint(tint),
        multicolor,
        freshness: Some(freshness),
    }
}

#[cfg(test)]
fn svg_file_cacheability(path: &Path) -> Option<(bool, FileFreshness)> {
    let resource_revision = resource_revision();
    let freshness = file_freshness(path)?;
    let cache = svg_cacheability_cache();
    if let Ok(mut guard) = cache.lock()
        && let Some(cached) = guard.get(path)
        && cached.resource_revision == resource_revision
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
                resource_revision,
                freshness,
                cacheable,
            },
            metadata_cache_weight(path),
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

fn linked_resource_fingerprints(svg_path: &Path, svg_data: &str) -> Vec<LinkedResourceFingerprint> {
    let mut resources = Vec::new();
    for reference in svg_external_resource_references(svg_data) {
        let reference = reference.split('#').next().unwrap_or_default().trim();
        if reference.is_empty()
            || reference.starts_with("data:")
            || (!reference.starts_with("file:") && reference.contains("://"))
        {
            continue;
        }
        let reference = reference.strip_prefix("file:").unwrap_or(reference);
        let path = Path::new(reference);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            svg_path
                .parent()
                .map_or_else(|| path.to_path_buf(), |parent| parent.join(path))
        };
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        let path: Arc<Path> = path.into();
        if resources
            .iter()
            .any(|resource: &LinkedResourceFingerprint| resource.path.as_ref() == path.as_ref())
        {
            continue;
        }
        resources.push(LinkedResourceFingerprint {
            fingerprint: resource_fingerprint(&path),
            path,
        });
    }
    resources
}

fn linked_resources_are_current(resources: &[LinkedResourceFingerprint]) -> bool {
    resources
        .iter()
        .all(|resource| resource_fingerprint(&resource.path) == resource.fingerprint)
}

fn svg_external_resource_references(svg_data: &str) -> Vec<String> {
    let mut references = Vec::new();
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
            break;
        };
        let reference = value[..end].trim();
        if !reference.is_empty() && !reference.starts_with('#') && !reference.starts_with("data:") {
            references.push(reference.to_owned());
        }
        remaining = &value[end + quote.len_utf8()..];
    }

    let mut remaining = svg_data;
    while let Some(index) = remaining.find("url(") {
        let after = &remaining[index + "url(".len()..];
        let Some(end) = after.find(')') else {
            break;
        };
        let reference = after[..end]
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim();
        if !reference.is_empty() && !reference.starts_with('#') && !reference.starts_with("data:") {
            references.push(reference.to_owned());
        }
        remaining = &after[end + 1..];
    }
    references
}

#[cfg(test)]
fn missing_icon_key(width: u32, height: u32, tint: Color) -> RasterCacheKey {
    RasterCacheKey {
        resource_revision: resource_revision(),
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
    if icon_file_kind(path).is_none() {
        return CachedResourceOpacity::Unknown;
    }
    let Some(freshness) = file_freshness(path) else {
        return CachedResourceOpacity::Unknown;
    };
    let key = raster_file_key_with_freshness(path, width, height, tint, multicolor, freshness);
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
        let weight = variant.pixels.len();
        cache.insert(key, variant, weight);
    }
}

fn metadata_cache_weight(path: &Path) -> usize {
    std::mem::size_of::<usize>()
        .saturating_mul(4)
        .saturating_add(path.as_os_str().len())
}

const ICON_SKIA_CACHE_CAPACITY: usize = 256;
const ICON_SKIA_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;

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
    static ICON_SKIA_CACHE: RefCell<ByteLruCache<usize, CachedIconImage>> =
        RefCell::new(ByteLruCache::new(ICON_SKIA_CACHE_CAPACITY, ICON_SKIA_CACHE_MAX_BYTES));
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
            variant.pixels.len().saturating_mul(2),
        );
        Some(image)
    })
}

fn blit_variant_on_canvas(canvas: &Canvas, variant: &RasterVariant, dest_x: i32, dest_y: i32) {
    let dest = Rect::from_xywh(
        dest_x as f32,
        dest_y as f32,
        variant.width as f32,
        variant.height as f32,
    );
    blit_variant_into_rect(canvas, variant, dest, SamplingOptions::default());
}

fn blit_variant_into_rect(
    canvas: &Canvas,
    variant: &RasterVariant,
    dest: Rect,
    sampling: SamplingOptions,
) {
    let Some(image) = cached_skia_image_for_variant(variant) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_anti_alias(false);
    let (image, source) = crate::surface::image_pages::resolve(&image);
    crate::prepared_scene::retain_image(&image);
    canvas.draw_image_rect_with_sampling_options(
        &image,
        source
            .as_ref()
            .map(|rect| (rect, skia_safe::canvas::SrcRectConstraint::Strict)),
        dest,
        sampling,
        &paint,
    );
}

/// Device pixels per local unit under the canvas's current matrix, for
/// rasterizing bitmaps that a CSS `transform` magnifies. Rounded up to a
/// quarter step so an animated `scale()` reuses a few cached rasters instead
/// of rasterizing one per frame; minification keeps the 1:1 raster.
pub(crate) fn canvas_raster_scale(canvas: &Canvas) -> f32 {
    let matrix = canvas.local_to_device_as_3x3();
    let scale_x = matrix.scale_x().hypot(matrix.skew_y());
    let scale_y = matrix.skew_x().hypot(matrix.scale_y());
    let scale = scale_x.max(scale_y);
    if !scale.is_finite() || scale <= 1.0 + 1e-3 {
        return 1.0;
    }
    ((scale * 4.0 - 1e-3).ceil() / 4.0).min(MAX_TRANSFORM_RASTER_SCALE)
}

/// Sampling for a raster drawn at a size other than its own.
pub(crate) fn resampled_sampling() -> SamplingOptions {
    SamplingOptions::new(skia_safe::FilterMode::Linear, skia_safe::MipmapMode::None)
}

#[cfg(test)]
fn blit_variant(buffer: &mut PixelBuffer, variant: &RasterVariant, dest_x: i32, dest_y: i32) {
    PixelCanvasSession::new(buffer).with_canvas(|canvas| {
        blit_variant_on_canvas(canvas, variant, dest_x, dest_y);
    });
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
    icon_raster_bytes(width, height)?;
    let img = get_or_load_sync(path)?;
    scaled_rgba_variant(&img, width, height, tint, multicolor)
}

fn scaled_rgba_variant(
    img: &image::RgbaImage,
    width: u32,
    height: u32,
    tint: Color,
    multicolor: bool,
) -> Option<RasterVariant> {
    let pixel_bytes = icon_raster_bytes(width, height)?;
    // For reductions of 2x and more (wallpaper thumbnails), area averaging is
    // within ~1/255 per channel of Lanczos3 at about a seventh of the cost.
    let scaled = if img.width() >= width * 2 && img.height() >= height * 2 {
        image::imageops::thumbnail(img, width, height)
    } else {
        image::imageops::resize(img, width, height, FilterType::Lanczos3)
    };
    let mut pixels = Vec::with_capacity(pixel_bytes);
    for p in scaled.as_raw().chunks_exact(4) {
        if multicolor {
            pixels.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        } else {
            pixels.extend_from_slice(&[tint.b, tint.g, tint.r, p[3]]);
        }
    }
    Some(variant_from_bgra(width, height, pixels))
}

fn raster_blob_variant(
    blob: &Blob,
    width: u32,
    height: u32,
    tint: Color,
    multicolor: bool,
) -> Option<RasterVariant> {
    icon_raster_bytes(width, height)?;
    let image = decode_blob_rgba(blob)?;
    scaled_rgba_variant(&image, width, height, tint, multicolor)
}

/// Decode a blob to straight-alpha RGBA, bounded like file images.
fn decode_blob_rgba(blob: &Blob) -> Option<image::RgbaImage> {
    let bytes = blob.bytes();
    match blob.format() {
        BlobFormat::Bytes => None,
        BlobFormat::Encoded(format) => {
            let format = match format {
                EncodedImageFormat::Png => image::ImageFormat::Png,
                EncodedImageFormat::Jpeg => image::ImageFormat::Jpeg,
                EncodedImageFormat::Bmp => image::ImageFormat::Bmp,
            };
            let mut reader =
                image::io::Reader::with_format(std::io::Cursor::new(bytes.as_ref()), format);
            let mut limits = image::io::Limits::default();
            limits.max_image_width = Some(MAX_SOURCE_IMAGE_DIMENSION);
            limits.max_image_height = Some(MAX_SOURCE_IMAGE_DIMENSION);
            limits.max_alloc = Some(MAX_SOURCE_IMAGE_BYTES as u64);
            reader.limits(limits);
            reader.decode().ok().map(|decoded| decoded.to_rgba8())
        }
        BlobFormat::Raw {
            layout,
            width,
            height,
            stride,
        } => {
            let pixel = layout.bytes_per_pixel() as usize;
            let mut rgba = Vec::with_capacity(checked_pixel_bytes(width, height, 4)?);
            for row in 0..height as usize {
                let start = row * stride as usize;
                let row = bytes.get(start..start + width as usize * pixel)?;
                for source in row.chunks_exact(pixel) {
                    rgba.extend_from_slice(&match layout {
                        RawPixelLayout::Argb32 => [source[1], source[2], source[3], source[0]],
                        RawPixelLayout::Rgba8 => [source[0], source[1], source[2], source[3]],
                        RawPixelLayout::Rgb8 => [source[0], source[1], source[2], 255],
                    });
                }
            }
            image::RgbaImage::from_raw(width, height, rgba)
        }
    }
}

fn blob_raster_key(
    handle: &str,
    width: u32,
    height: u32,
    tint: Color,
    multicolor: bool,
) -> RasterCacheKey {
    RasterCacheKey {
        resource_revision: resource_revision(),
        source_kind: RasterSourceKind::Blob,
        source_identity: Arc::from(Path::new(handle)),
        width,
        height,
        tint: encode_tint(tint),
        multicolor,
        freshness: None,
    }
}

/// Resolve a `mesh-blob:` source. A rasterized variant outlives the blob in
/// the raster cache, so a consumer still showing a released handle keeps its
/// pixels until its state moves on.
fn resolve_blob_variant(
    handle: &str,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
    multicolor: bool,
) -> FileVariant {
    let Some((width, height)) = icon_dimensions(dest_w, dest_h) else {
        return FileVariant::Unavailable;
    };
    let key = blob_raster_key(handle, width, height, tint, multicolor);
    #[cfg(not(test))]
    if let Some(variant) = take_ready_variant(&key) {
        return FileVariant::Ready(variant);
    }
    if let Some(variant) = cached_variant(&key) {
        profiling::record_raster_cache_hit(variant.fully_opaque);
        return FileVariant::Ready(variant);
    }
    let Some(blob) = lookup_blob(handle) else {
        return FileVariant::Unavailable;
    };
    #[cfg(not(test))]
    {
        let scheduled = schedule_icon_raster_job(
            key.clone(),
            RasterSource::Blob(Arc::clone(&blob)),
            width,
            height,
            tint,
            multicolor,
        );
        let repaint_pending = match scheduled {
            Some(true) => true,
            Some(false) => {
                icon_raster_job_pending(&key)
                    || ResourceBroker::global()
                        .is_some_and(ResourceBroker::retry_after_next_completion)
            }
            None => false,
        };
        if repaint_pending {
            return FileVariant::Pending;
        }
    }
    let started = std::time::Instant::now();
    let variant = raster_blob_variant(&blob, width, height, tint, multicolor);
    profiling::record_icon_image_raster(started.elapsed());
    let Some(variant) = variant else {
        return FileVariant::Unavailable;
    };
    profiling::record_raster_cache_miss();
    store_variant(key, variant.clone());
    FileVariant::Ready(variant)
}

#[cfg(test)]
fn raster_svg_variant(
    path: &Path,
    width: u32,
    height: u32,
    tint: Color,
    multicolor: bool,
) -> Option<RasterVariant> {
    icon_raster_bytes(width, height)?;
    let svg_data = std::fs::read_to_string(path).ok()?;
    raster_svg_variant_from_data(path, &svg_data, width, height, tint, multicolor)
}

fn raster_svg_variant_from_data(
    path: &Path,
    svg_data: &str,
    width: u32,
    height: u32,
    tint: Color,
    multicolor: bool,
) -> Option<RasterVariant> {
    icon_raster_bytes(width, height)?;
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

#[cfg(test)]
fn raster_missing_icon_variant(width: u32, height: u32, tint: Color) -> Option<RasterVariant> {
    icon_raster_bytes(width, height)?;
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
    let mut pixels = Vec::with_capacity(checked_pixel_bytes(width, height, 4).unwrap_or_default());
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

#[cfg(test)]
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

#[cfg(test)]
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
    if let FileVariant::Ready(variant) =
        resolve_file_variant(path, dest_w, dest_h, tint, multicolor)
    {
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
    let raster_scale = canvas_raster_scale(canvas);
    let mut stretched = false;
    let raster_w = (dest_w as f32 * raster_scale).ceil() as i32;
    let raster_h = (dest_h as f32 * raster_scale).ceil() as i32;
    let variant = match resolve_file_variant(path, raster_w, raster_h, tint, multicolor) {
        FileVariant::Ready(variant) => variant,
        // A magnified raster is still being prepared: stretch the 1:1 raster
        // already on screen instead of blinking the icon out until it lands.
        FileVariant::Pending if raster_scale != 1.0 => {
            match resolve_file_variant(path, dest_w, dest_h, tint, multicolor) {
                FileVariant::Ready(variant) => {
                    stretched = true;
                    variant
                }
                FileVariant::Pending | FileVariant::Unavailable => return,
            }
        }
        FileVariant::Pending => return,
        FileVariant::Unavailable => {
            draw_missing_icon_fallback_on_canvas(canvas, dest_x, dest_y, dest_w, dest_h, tint);
            return;
        }
    };
    if stretched {
        let dest = Rect::from_xywh(
            dest_x as f32,
            dest_y as f32,
            variant.width as f32,
            variant.height as f32,
        );
        blit_variant_into_rect(canvas, &variant, dest, resampled_sampling());
    } else if raster_scale == 1.0 {
        blit_variant_on_canvas(canvas, &variant, dest_x, dest_y);
    } else {
        let dest = Rect::from_xywh(
            dest_x as f32,
            dest_y as f32,
            variant.width as f32 / raster_scale,
            variant.height as f32 / raster_scale,
        );
        blit_variant_into_rect(canvas, &variant, dest, resampled_sampling());
    }
}

enum FileVariant {
    Ready(RasterVariant),
    /// A worker is preparing the variant and repaints the requesting consumer.
    #[cfg_attr(test, allow(dead_code))]
    Pending,
    Unavailable,
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
) -> FileVariant {
    drain_icon_raster_jobs();
    if let Some(handle) = path.to_str().filter(|path| path.starts_with(BLOB_SCHEME)) {
        return resolve_blob_variant(handle, dest_w, dest_h, tint, multicolor);
    }
    #[cfg(test)]
    {
        resolve_file_variant_sync(path, dest_w, dest_h, tint, multicolor)
            .map_or(FileVariant::Unavailable, FileVariant::Ready)
    }
    #[cfg(not(test))]
    {
        resolve_file_variant_async(path, dest_w, dest_h, tint, multicolor)
    }
}

#[cfg(test)]
fn resolve_file_variant_sync(
    path: &Path,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
    multicolor: bool,
) -> Option<RasterVariant> {
    let kind = icon_file_kind(path)?;
    let (width, height) = icon_dimensions(dest_w, dest_h)?;
    let key = match kind {
        IconFileKind::Svg => {
            let (cacheable, freshness) = svg_file_cacheability(path)?;
            if cacheable {
                Some(raster_file_key_with_freshness(
                    path, width, height, tint, multicolor, freshness,
                ))
            } else {
                None
            }
        }
        IconFileKind::Bitmap => raster_file_key(path, width, height, tint, multicolor),
    };
    if let Some(key) = key.as_ref()
        && let Some(variant) = cached_variant(key)
    {
        profiling::record_raster_cache_hit(variant.fully_opaque);
        return Some(variant);
    }

    let variant = match kind {
        IconFileKind::Bitmap => {
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
        IconFileKind::Svg => {
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
    }?;

    if let Some(key) = key {
        store_variant(key, variant.clone());
    }
    Some(variant)
}

#[cfg(not(test))]
fn resolve_file_variant_async(
    path: &Path,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
    multicolor: bool,
) -> FileVariant {
    let (Some(kind), Some((width, height)), Some(freshness)) = (
        icon_file_kind(path),
        icon_dimensions(dest_w, dest_h),
        file_freshness(path),
    ) else {
        return FileVariant::Unavailable;
    };
    let key = raster_file_key_with_freshness(path, width, height, tint, multicolor, freshness);

    if let Some(variant) = take_ready_variant(&key) {
        return FileVariant::Ready(variant);
    }

    if let Some(variant) = cached_variant(&key) {
        profiling::record_raster_cache_hit(variant.fully_opaque);
        return FileVariant::Ready(variant);
    }

    let source = RasterSource::File {
        path: path.to_path_buf(),
        kind,
    };
    let repaint_pending =
        match schedule_icon_raster_job(key.clone(), source, width, height, tint, multicolor) {
            Some(true) => true,
            Some(false) => {
                icon_raster_job_pending(&key)
                    || ResourceBroker::global()
                        .is_some_and(ResourceBroker::retry_after_next_completion)
            }
            None => false,
        };
    // The consumer repaints when the worker publishes; painting nothing until
    // then keeps decode and resampling off the frame. Only when no worker can
    // ever wake it is the file rasterized here.
    if repaint_pending {
        return FileVariant::Pending;
    }
    let started = std::time::Instant::now();
    let (variant, cacheable) =
        raster_file_variant_on_render_thread(path, kind, width, height, tint, multicolor);
    profiling::record_icon_image_raster(started.elapsed());
    let Some(variant) = variant else {
        return FileVariant::Unavailable;
    };
    if cacheable {
        profiling::record_raster_cache_miss();
        store_variant(key, variant.clone());
    } else {
        profiling::record_raster_cache_bypass();
    }
    FileVariant::Ready(variant)
}

#[cfg(not(test))]
fn icon_raster_job_pending(key: &RasterCacheKey) -> bool {
    icon_raster_queue()
        .lock()
        .is_ok_and(|queue| queue.pending.contains(key))
}

#[cfg(not(test))]
fn raster_file_variant_on_render_thread(
    path: &Path,
    kind: IconFileKind,
    width: u32,
    height: u32,
    tint: Color,
    multicolor: bool,
) -> (Option<RasterVariant>, bool) {
    match kind {
        IconFileKind::Bitmap => (
            raster_bitmap_variant(path, width, height, tint, multicolor),
            true,
        ),
        IconFileKind::Svg => {
            let Ok(svg_data) = std::fs::read_to_string(path) else {
                return (None, false);
            };
            let cacheable = !svg_has_external_resource_reference(&svg_data);
            (
                raster_svg_variant_from_data(path, &svg_data, width, height, tint, multicolor),
                cacheable,
            )
        }
    }
}

/// Draw the built-in missing-icon fallback. Tests retain the embedded SVG
/// raster for pixel coverage; production paints a deterministic placeholder so
/// a missing resource never performs work on the frame thread.
#[cfg(test)]
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
    #[cfg(test)]
    if let Some(variant) = resolve_missing_icon_variant(dest_w, dest_h, tint) {
        blit_variant_on_canvas(canvas, &variant, dest_x, dest_y);
        return;
    }

    #[cfg(not(test))]
    draw_missing_icon_placeholder(canvas, dest_x, dest_y, dest_w, dest_h, tint);
}

#[cfg(test)]
fn resolve_missing_icon_variant(dest_w: i32, dest_h: i32, tint: Color) -> Option<RasterVariant> {
    let (width, height) = icon_dimensions(dest_w, dest_h)?;
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

#[cfg(not(test))]
fn draw_missing_icon_placeholder(
    canvas: &Canvas,
    dest_x: i32,
    dest_y: i32,
    dest_w: i32,
    dest_h: i32,
    tint: Color,
) {
    let width = dest_w.max(1) as f32;
    let height = dest_h.max(1) as f32;
    let rect = Rect::from_xywh(dest_x as f32, dest_y as f32, width, height);
    let mut paint = Paint::default();
    paint.set_anti_alias(false);
    paint.set_color(skia_safe::Color::from_argb(tint.a, tint.r, tint.g, tint.b));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width((width.min(height) / 8.0).max(1.0));
    canvas.draw_rect(rect, &paint);
    canvas.draw_line(
        (dest_x as f32, dest_y as f32),
        ((dest_x as f32) + width, (dest_y as f32) + height),
        &paint,
    );
    canvas.draw_line(
        ((dest_x as f32) + width, dest_y as f32),
        (dest_x as f32, (dest_y as f32) + height),
        &paint,
    );
}

#[cfg(test)]
#[allow(dead_code)] // Test-only pixel-buffer API retained for renderer fixtures.
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
#[cfg(test)]
#[allow(dead_code)] // Test-only pixel-buffer API retained for renderer fixtures.
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
    multicolor: bool,
) {
    session.with_canvas(|canvas| {
        draw_icon_from_path_with_options_on_canvas(
            canvas, path, dest_x, dest_y, dest_w, dest_h, tint, multicolor,
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
    axes: GlyphAxes,
) {
    #[cfg(test)]
    let resolution = resolve_icon_result(name, size);
    #[cfg(not(test))]
    let Some(resolution) = resolve_icon_for_render("", name, size) else {
        return;
    };
    draw_icon_resolution_with_axes_in_session(
        session, resolution, dest_x, dest_y, dest_w, dest_h, tint, axes,
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
    #[cfg(test)]
    let resolution = mesh_core_icon::resolve_icon_for_module(module_id, name, size);
    #[cfg(not(test))]
    let Some(resolution) = resolve_icon_for_render(module_id, name, size) else {
        return;
    };
    draw_icon_resolution_with_axes_in_session(
        session, resolution, dest_x, dest_y, dest_w, dest_h, tint, axes,
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
                    font_bytes,
                    font_fingerprint,
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
                        font_bytes.as_ref(),
                        font_fingerprint,
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
#[cfg(test)]
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
                    font_bytes: _,
                    font_fingerprint: _,
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
    use std::time::{Duration, Instant};
    #[cfg(unix)]
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use crate::resource_state_test_lock as icon_test_lock;

    /// How long a test waits for a worker thread under a loaded parallel run.
    const WORKER_WAIT: Duration = Duration::from_secs(5);

    fn tint() -> Color {
        Color {
            r: 16,
            g: 120,
            b: 220,
            a: 255,
        }
    }

    fn pixel(buffer: &PixelBuffer, x: u32, y: u32) -> Color {
        let offset = (y * buffer.stride() + x * 4) as usize;
        Color {
            b: buffer.data()[offset],
            g: buffer.data()[offset + 1],
            r: buffer.data()[offset + 2],
            a: buffer.data()[offset + 3],
        }
    }

    fn non_transparent_pixels(buffer: &PixelBuffer) -> Vec<(u32, u32, Color)> {
        let mut pixels = Vec::new();
        for y in 0..buffer.height() {
            for x in 0..buffer.width() {
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
        if let Some(cache) = SOURCE_IDENTITY_CACHE.get() {
            cache.lock().unwrap().clear();
        }
        if let Some(cache) = SVG_CACHEABILITY_CACHE.get() {
            cache.lock().unwrap().clear();
        }
        if let Some(queue) = ICON_RASTER_QUEUE.get() {
            queue.lock().unwrap().ready.clear();
        }
        if let Some(queue) = IMAGE_DECODE_QUEUE.get() {
            queue.lock().unwrap().ready.clear();
        }
        ICON_RASTER_RESULTS_READY.store(false, Ordering::Release);
        IMAGE_DECODE_RESULTS_READY.store(false, Ordering::Release);
        FILE_FRESHNESS_STAT_PROBES.store(0, Ordering::Relaxed);
        profiling::reset_raster_metrics();
    }

    fn file_freshness_stat_probes() -> usize {
        FILE_FRESHNESS_STAT_PROBES.load(Ordering::Relaxed)
    }

    fn write_test_svg(path: &Path) {
        fs::write(
            path,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect x="1" y="1" width="6" height="6" fill="black"/></svg>"#,
        )
        .unwrap();
    }

    #[test]
    fn icon_raster_worker_transfers_svg_variant_to_cache() {
        if crate::isolate_resource_state_test(
            "surface::icon::tests::icon_raster_worker_transfers_svg_variant_to_cache",
        ) {
            return;
        }
        let _guard = icon_test_lock();
        clear_icon_caches();
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("worker.svg");
        write_test_svg(&path);
        let key = raster_file_key(&path, 8, 8, tint(), false).unwrap();

        assert_eq!(
            schedule_icon_raster_job(
                key.clone(),
                RasterSource::File {
                    path: path.clone(),
                    kind: IconFileKind::Svg
                },
                8,
                8,
                tint(),
                false,
            ),
            Some(true)
        );

        let deadline = Instant::now() + WORKER_WAIT;
        while {
            drain_icon_raster_jobs();
            cached_variant(&key).is_none()
        } {
            assert!(
                Instant::now() < deadline,
                "worker did not publish the SVG variant"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(poll_icon_raster_jobs());
        assert!(!icon_raster_jobs_pending());
    }

    #[test]
    fn image_decode_worker_transfers_bounded_image_off_the_paint_path() {
        if crate::isolate_resource_state_test(
            "surface::icon::tests::image_decode_worker_transfers_bounded_image_off_the_paint_path",
        ) {
            return;
        }
        let _guard = icon_test_lock();
        clear_icon_caches();
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("decoded.png");
        ImageBuffer::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([255_u8, 0, 0, 255])
            } else {
                Rgba([0, 255, 0, 255])
            }
        })
        .save(&path)
        .unwrap();
        let key = ImageDecodeKey {
            path: Arc::from(path.as_path()),
            freshness: resource_fingerprint(&path).unwrap(),
        };

        let first = super::super::ResourcePaintConsumer::default();
        let second = super::super::ResourcePaintConsumer::default();
        let unrelated = super::super::ResourcePaintConsumer::default();
        {
            let _scope = first.enter();
            assert_eq!(schedule_image_decode_job(key.clone()), Some(true));
        }
        {
            let _scope = second.enter();
            assert_eq!(schedule_image_decode_job(key.clone()), Some(false));
        }
        let deadline = Instant::now() + WORKER_WAIT;
        let image = loop {
            drain_image_decode_jobs();
            if let Some(image) = take_ready_image(&key) {
                break image;
            }
            assert!(Instant::now() < deadline, "worker did not decode the image");
            std::thread::sleep(Duration::from_millis(1));
        };
        assert!(first.take_ready());
        assert!(second.take_ready());
        assert!(!unrelated.take_ready());
        assert_eq!(image.dimensions(), (2, 1));
        assert_eq!(image.get_pixel(0, 0), &Rgba([255, 0, 0, 255]));
        assert!(poll_image_decode_jobs());
        assert!(!image_decode_jobs_pending());
    }

    #[test]
    fn magnified_bitmap_icons_rasterize_at_their_device_scale() {
        if crate::isolate_resource_state_test(
            "surface::icon::tests::magnified_bitmap_icons_rasterize_at_their_device_scale",
        ) {
            return;
        }
        let _guard = icon_test_lock();
        clear_icon_caches();
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("checker.png");
        ImageBuffer::from_fn(32, 32, |x, y| {
            let value = if (x + y) % 2 == 0 { 255u8 } else { 0 };
            Rgba([value, value, value, 255])
        })
        .save(&path)
        .unwrap();

        let mut buffer = PixelBuffer::new(32, 32);
        PixelCanvasSession::new(&mut buffer).with_canvas(|canvas| {
            canvas.scale((4.0, 4.0));
            assert_eq!(canvas_raster_scale(canvas), 4.0);
            draw_icon_from_path_with_options_on_canvas(canvas, &path, 0, 0, 8, 8, tint(), true);
        });

        let row = &buffer.data()[..32 * 4];
        let first = row[0] as i32;
        let second = row[4] as i32;
        assert!(
            (first - second).abs() > 200,
            "a 4x-magnified 8px icon must keep its 32px source detail: {first} vs {second}"
        );
    }

    #[test]
    fn raster_scale_rounds_magnification_up_to_quarter_steps() {
        let mut buffer = PixelBuffer::new(4, 4);
        PixelCanvasSession::new(&mut buffer).with_canvas(|canvas| {
            assert_eq!(canvas_raster_scale(canvas), 1.0);
            canvas.scale((0.96, 0.96));
            assert_eq!(canvas_raster_scale(canvas), 1.0);
            canvas.reset_matrix();
            canvas.translate((5.0, -12.0));
            canvas.scale((1.22, 1.22));
            assert_eq!(canvas_raster_scale(canvas), 1.25);
            canvas.scale((2.0, 2.0));
            assert_eq!(canvas_raster_scale(canvas), 2.5);
        });
    }

    #[test]
    fn icon_raster_worker_returns_external_svg_without_caching() {
        if crate::isolate_resource_state_test(
            "surface::icon::tests::icon_raster_worker_returns_external_svg_without_caching",
        ) {
            return;
        }
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
        let key = raster_file_key(&svg_path, 8, 8, tint(), true).unwrap();

        assert_eq!(
            schedule_icon_raster_job(
                key.clone(),
                RasterSource::File {
                    path: svg_path.clone(),
                    kind: IconFileKind::Svg,
                },
                8,
                8,
                tint(),
                true,
            ),
            Some(true)
        );

        let deadline = Instant::now() + WORKER_WAIT;
        loop {
            drain_icon_raster_jobs();
            let ready = ICON_RASTER_QUEUE
                .get()
                .and_then(|queue| queue.lock().ok())
                .is_some_and(|queue| queue.ready.contains_key(&key));
            if ready {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not publish the external SVG variant"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        fs::remove_file(&image_path).unwrap();
        ImageBuffer::from_fn(2, 2, |_, _| Rgba([0u8, 0, 255, 255]))
            .save(&image_path)
            .unwrap();
        assert!(
            take_ready_variant(&key).is_none(),
            "a linked-file change must invalidate an in-flight SVG handoff"
        );
        assert!(cached_variant(&key).is_none());

        assert_eq!(
            schedule_icon_raster_job(
                key.clone(),
                RasterSource::File {
                    path: svg_path.clone(),
                    kind: IconFileKind::Svg,
                },
                8,
                8,
                tint(),
                true,
            ),
            Some(true)
        );
        let variant = loop {
            drain_icon_raster_jobs();
            if let Some(variant) = take_ready_variant(&key) {
                break variant;
            }
            assert!(
                Instant::now() < deadline + WORKER_WAIT,
                "worker did not republish the linked SVG after invalidation"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert_eq!(variant.pixels[0..4], [255, 0, 0, 255]);
        assert!(poll_icon_raster_jobs());
        assert!(!icon_raster_jobs_pending());
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

    #[test]
    fn raster_file_key_checks_freshness_each_time() {
        let _guard = icon_test_lock();
        clear_icon_caches();
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("logo.png");
        ImageBuffer::from_fn(1, 1, |_, _| Rgba([255u8, 0, 0, 255]))
            .save(&path)
            .unwrap();

        let first_key = raster_file_key(&path, 8, 8, tint(), true).unwrap();
        let second_key = raster_file_key(&path, 8, 8, tint(), true).unwrap();

        assert_eq!(first_key, second_key);
        assert_eq!(file_freshness_stat_probes(), 2);
    }

    #[test]
    fn icon_file_kind_is_case_insensitive() {
        assert_eq!(
            icon_file_kind(Path::new("symbol.SvG")),
            Some(IconFileKind::Svg)
        );
        assert_eq!(
            icon_file_kind(Path::new("photo.JPEG")),
            Some(IconFileKind::Bitmap)
        );
        assert_eq!(icon_file_kind(Path::new("notes.txt")), None);
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
    fn blob_sources_decode_raw_and_encoded_pixels_and_survive_release_in_the_cache() {
        use mesh_core_resources::blob::{BlobMeta, BlobOwner, MIME_ARGB32};
        let _guard = icon_test_lock();
        clear_icon_caches();
        let owner = BlobOwner::new();
        let format = BlobFormat::from_mime(
            MIME_ARGB32,
            BlobMeta {
                width: Some(2),
                height: Some(1),
                stride: None,
            },
        )
        .unwrap();
        // ARGB: opaque red, then opaque blue.
        let handle = owner
            .put(format, vec![255, 255, 0, 0, 255, 0, 0, 255])
            .unwrap();
        let path = Path::new(handle.as_ref());

        let mut buffer = PixelBuffer::new(4, 2);
        draw_icon_from_path_with_options(&mut buffer, path, 0, 0, 2, 1, tint(), true);
        assert_eq!((pixel(&buffer, 0, 0).r, pixel(&buffer, 0, 0).b), (255, 0));
        assert_eq!((pixel(&buffer, 1, 0).r, pixel(&buffer, 1, 0).b), (0, 255));

        drop(owner);
        let mut cached = PixelBuffer::new(4, 2);
        draw_icon_from_path_with_options(&mut cached, path, 0, 0, 2, 1, tint(), true);
        assert_eq!(pixel(&cached, 0, 0).r, 255, "rasterized variants outlive the blob");
        let mut resized = PixelBuffer::new(4, 4);
        draw_icon_from_path_with_options(&mut resized, path, 0, 0, 4, 2, tint(), true);
        assert!(
            non_transparent_pixels(&resized)
                .iter()
                .all(|(_, _, color)| *color != pixel(&buffer, 0, 0)),
            "a released blob has no new variants"
        );

        let owner = BlobOwner::new();
        let mut png = Vec::new();
        ImageBuffer::from_pixel(1, 1, Rgba([0u8, 200, 0, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let handle = owner
            .put(BlobFormat::Encoded(EncodedImageFormat::Png), png)
            .unwrap();
        let mut buffer = PixelBuffer::new(2, 2);
        draw_icon_from_path_with_options(
            &mut buffer,
            Path::new(handle.as_ref()),
            0,
            0,
            1,
            1,
            tint(),
            true,
        );
        assert_eq!(pixel(&buffer, 0, 0).g, 200);
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
        let mut registry = mesh_core_icon::IconRegistry::empty();
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

    #[test]
    fn icon_raster_dimensions_reject_overflow_and_oversized_assets() {
        assert_eq!(icon_dimensions(0, 0), Some((1, 1)));
        assert!(icon_dimensions((MAX_ICON_DIMENSION + 1) as i32, 16).is_none());
        assert!(icon_raster_bytes(u32::MAX, 1).is_none());
        assert!(icon_raster_bytes(MAX_ICON_DIMENSION, MAX_ICON_DIMENSION).is_some());

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.png");
        ImageBuffer::from_fn(1, 1, |_, _| Rgba([0_u8, 0, 0, 255]))
            .save(&source)
            .unwrap();
        assert!(source_image_is_bounded(&source));
    }
}
