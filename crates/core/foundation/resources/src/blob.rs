//! Content-addressed immutable blobs that service providers publish beside
//! their state.
//!
//! A provider registers bytes once through its [`BlobOwner`] and puts the
//! returned `mesh-blob:sha256-…` handle in state; consumers pass the handle
//! wherever an image source is accepted and the renderer reads the bytes
//! here. Bytes never travel through state records or consumer scripts.
//!
//! Lifetime is owned by published state. [`BlobOwner::publish`] pins exactly
//! the handles the provider's latest state references. A blob that state does
//! not reference survives one publication after it was put, so a callback can
//! register bytes before the snapshot that uses them is taken, and is released
//! at the next one. Dropping the owner, which happens when the provider stops
//! or restarts, releases everything it held. Handles are rejected at
//! publication unless their owner registered them, so state can never refer
//! to bytes that have not arrived (bulk-channel ordering, design note §5.3).

use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// The URI scheme of every blob handle. A state string with this scheme is a
/// blob reference, whether or not it is well formed.
pub const BLOB_SCHEME: &str = "mesh-blob:";
pub const BLOB_HANDLE_PREFIX: &str = "mesh-blob:sha256-";
const BLOB_DIGEST_HEX_LEN: usize = 64;

/// The largest single blob a provider may register.
pub const MAX_BLOB_BYTES: usize = 8 * 1024 * 1024;
/// The bytes one provider may hold at once.
pub const MAX_OWNER_BLOB_BYTES: usize = 32 * 1024 * 1024;
/// The largest width or height a raw pixel blob may declare.
pub const MAX_RAW_BLOB_DIMENSION: u32 = 4096;

pub const MIME_PNG: &str = "image/png";
pub const MIME_JPEG: &str = "image/jpeg";
pub const MIME_BMP: &str = "image/bmp";
/// 32-bit pixels in A, R, G, B byte order with straight alpha: the
/// StatusNotifierItem `IconPixmap` layout.
pub const MIME_ARGB32: &str = "image/x-mesh-argb32";
/// 8-bit R, G, B, A pixels with straight alpha: freedesktop notification
/// `image-data` with an alpha channel.
pub const MIME_RGBA8: &str = "image/x-mesh-rgba8";
/// 8-bit R, G, B pixels: freedesktop notification `image-data` without alpha.
pub const MIME_RGB8: &str = "image/x-mesh-rgb8";
/// Uninterpreted bytes. Hosts register large binary arguments this way so a
/// provider can give them an image format without reading them.
pub const MIME_BYTES: &str = "application/octet-stream";

/// Whether `value` has the syntax of a blob handle. It says nothing about
/// whether the blob is registered.
pub fn is_blob_handle(value: &str) -> bool {
    value.strip_prefix(BLOB_HANDLE_PREFIX).is_some_and(|digest| {
        digest.len() == BLOB_DIGEST_HEX_LEN
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncodedImageFormat {
    Png,
    Jpeg,
    Bmp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RawPixelLayout {
    Argb32,
    Rgba8,
    Rgb8,
}

impl RawPixelLayout {
    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::Argb32 | Self::Rgba8 => 4,
            Self::Rgb8 => 3,
        }
    }
}

/// How a blob's bytes are interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlobFormat {
    /// Not displayable until [`BlobOwner::retype`] gives it an image format.
    Bytes,
    Encoded(EncodedImageFormat),
    Raw {
        layout: RawPixelLayout,
        width: u32,
        height: u32,
        /// Bytes from the start of one row to the next.
        stride: u32,
    },
}

/// Dimensions accompanying a raw pixel MIME type. `stride` defaults to a
/// tightly packed row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BlobMeta {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub stride: Option<u32>,
}

impl BlobFormat {
    pub fn from_mime(mime: &str, meta: BlobMeta) -> Result<Self, BlobError> {
        let layout = match mime {
            MIME_BYTES => return Ok(Self::Bytes),
            MIME_PNG => return Ok(Self::Encoded(EncodedImageFormat::Png)),
            MIME_JPEG => return Ok(Self::Encoded(EncodedImageFormat::Jpeg)),
            MIME_BMP => return Ok(Self::Encoded(EncodedImageFormat::Bmp)),
            MIME_ARGB32 => RawPixelLayout::Argb32,
            MIME_RGBA8 => RawPixelLayout::Rgba8,
            MIME_RGB8 => RawPixelLayout::Rgb8,
            other => return Err(BlobError::UnsupportedMime(other.to_string())),
        };
        let (Some(width), Some(height)) = (meta.width, meta.height) else {
            return Err(BlobError::MissingDimensions(mime.to_string()));
        };
        if width == 0
            || height == 0
            || width > MAX_RAW_BLOB_DIMENSION
            || height > MAX_RAW_BLOB_DIMENSION
        {
            return Err(BlobError::InvalidDimensions { width, height });
        }
        let row = width * layout.bytes_per_pixel();
        let stride = meta.stride.unwrap_or(row);
        if stride < row {
            return Err(BlobError::InvalidStride { stride, row });
        }
        Ok(Self::Raw {
            layout,
            width,
            height,
            stride,
        })
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Bytes => MIME_BYTES,
            Self::Encoded(EncodedImageFormat::Png) => MIME_PNG,
            Self::Encoded(EncodedImageFormat::Jpeg) => MIME_JPEG,
            Self::Encoded(EncodedImageFormat::Bmp) => MIME_BMP,
            Self::Raw { layout, .. } => match layout {
                RawPixelLayout::Argb32 => MIME_ARGB32,
                RawPixelLayout::Rgba8 => MIME_RGBA8,
                RawPixelLayout::Rgb8 => MIME_RGB8,
            },
        }
    }

    /// The fewest bytes a blob in this format can hold; the last row needs no
    /// stride padding.
    fn minimum_len(self) -> usize {
        match self {
            Self::Bytes | Self::Encoded(_) => 1,
            Self::Raw {
                layout,
                width,
                height,
                stride,
            } => {
                (height as usize - 1) * stride as usize
                    + width as usize * layout.bytes_per_pixel() as usize
            }
        }
    }
}

/// One registered blob. Its bytes never change.
#[derive(Debug)]
pub struct Blob {
    handle: Arc<str>,
    format: BlobFormat,
    bytes: Arc<[u8]>,
}

impl Blob {
    pub fn handle(&self) -> &Arc<str> {
        &self.handle
    }

    pub fn format(&self) -> BlobFormat {
        self.format
    }

    pub fn bytes(&self) -> &Arc<[u8]> {
        &self.bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BlobError {
    #[error("unsupported blob MIME type '{0}'")]
    UnsupportedMime(String),
    #[error("blob MIME type '{0}' requires width and height")]
    MissingDimensions(String),
    #[error("blob dimensions {width}x{height} are outside 1..={MAX_RAW_BLOB_DIMENSION}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("blob stride {stride} is shorter than its {row}-byte row")]
    InvalidStride { stride: u32, row: u32 },
    #[error("blob holds {actual} bytes but its format needs at least {expected}")]
    TooShort { expected: usize, actual: usize },
    #[error("blob of {0} bytes exceeds the {MAX_BLOB_BYTES}-byte limit")]
    TooLarge(usize),
    #[error("provider blob memory would exceed {MAX_OWNER_BLOB_BYTES} bytes")]
    QuotaExceeded,
    #[error("state references blob '{0}', which this provider has not registered")]
    Unregistered(String),
}

#[derive(Debug)]
struct Entry {
    blob: Arc<Blob>,
    holders: usize,
}

#[derive(Debug, Default)]
struct OwnerHolds {
    /// Each held handle with the publication count when it was last put.
    held: HashMap<Arc<str>, u64>,
    pinned: HashSet<Arc<str>>,
    publications: u64,
    bytes: usize,
}

#[derive(Debug, Default)]
struct Registry {
    blobs: HashMap<Arc<str>, Entry>,
    owners: HashMap<u64, OwnerHolds>,
}

impl Registry {
    fn release(&mut self, handle: &str) {
        if let Some(entry) = self.blobs.get_mut(handle) {
            entry.holders -= 1;
            if entry.holders == 0 {
                self.blobs.remove(handle);
            }
        }
    }
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(Mutex::default)
}

fn lock_registry() -> std::sync::MutexGuard<'static, Registry> {
    registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The registered blob for `handle`, if any provider still holds it.
pub fn lookup_blob(handle: &str) -> Option<Arc<Blob>> {
    lock_registry()
        .blobs
        .get(handle)
        .map(|entry| Arc::clone(&entry.blob))
}

/// One provider instance's claim on registered blobs. Dropping it releases
/// every blob the provider held.
#[derive(Debug)]
pub struct BlobOwner {
    id: u64,
}

impl Default for BlobOwner {
    fn default() -> Self {
        Self::new()
    }
}

impl BlobOwner {
    pub fn new() -> Self {
        static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_OWNER.fetch_add(1, Ordering::Relaxed);
        lock_registry().owners.insert(id, OwnerHolds::default());
        Self { id }
    }

    /// Register `bytes` in `format` and return the handle state should carry.
    /// Registering content this owner already holds returns the same handle
    /// and restarts its one-publication grace.
    pub fn put(&self, format: BlobFormat, bytes: Vec<u8>) -> Result<Arc<str>, BlobError> {
        self.insert(format, bytes.into())
    }

    /// Register the bytes of a blob this owner holds under another format,
    /// without copying them.
    pub fn retype(&self, handle: &str, format: BlobFormat) -> Result<Arc<str>, BlobError> {
        let bytes = {
            let registry = lock_registry();
            let held = registry
                .owners
                .get(&self.id)
                .is_some_and(|owner| owner.held.contains_key(handle));
            match registry.blobs.get(handle) {
                Some(entry) if held => Arc::clone(&entry.blob.bytes),
                _ => return Err(BlobError::Unregistered(handle.to_string())),
            }
        };
        self.insert(format, bytes)
    }

    fn insert(&self, format: BlobFormat, bytes: Arc<[u8]>) -> Result<Arc<str>, BlobError> {
        if bytes.len() > MAX_BLOB_BYTES {
            return Err(BlobError::TooLarge(bytes.len()));
        }
        let expected = format.minimum_len();
        if bytes.len() < expected {
            return Err(BlobError::TooShort {
                expected,
                actual: bytes.len(),
            });
        }
        let handle: Arc<str> = blob_handle(format, &bytes).into();
        let len = bytes.len();
        let mut registry = lock_registry();
        let registry = &mut *registry;
        let owner = registry.owners.entry(self.id).or_default();
        let publications = owner.publications;
        if let Some(put_at) = owner.held.get_mut(&handle) {
            *put_at = publications;
            return Ok(handle);
        }
        if owner.bytes + len > MAX_OWNER_BLOB_BYTES {
            return Err(BlobError::QuotaExceeded);
        }
        owner.bytes += len;
        owner.held.insert(Arc::clone(&handle), publications);
        registry
            .blobs
            .entry(Arc::clone(&handle))
            .or_insert_with(|| Entry {
                blob: Arc::new(Blob {
                    handle: Arc::clone(&handle),
                    format,
                    bytes,
                }),
                holders: 0,
            })
            .holders += 1;
        Ok(handle)
    }

    /// Record the handles the provider's latest state references. Fails,
    /// changing nothing, when one was not registered by this owner. Blobs the
    /// state does not reference and that were put before the previous
    /// publication are released.
    pub fn publish<'a>(
        &self,
        referenced: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), BlobError> {
        let mut registry = lock_registry();
        let registry = &mut *registry;
        let owner = registry.owners.entry(self.id).or_default();
        let mut pinned = HashSet::new();
        for handle in referenced {
            let Some((held, _)) = owner.held.get_key_value(handle) else {
                return Err(BlobError::Unregistered(handle.to_string()));
            };
            pinned.insert(Arc::clone(held));
        }
        let previous = owner.publications;
        owner.publications += 1;
        let mut released = Vec::new();
        owner.held.retain(|handle, put_at| {
            let keep = pinned.contains(handle) || *put_at >= previous;
            if !keep {
                released.push(Arc::clone(handle));
            }
            keep
        });
        owner.pinned = pinned;
        owner.bytes -= released
            .iter()
            .filter_map(|handle| registry.blobs.get(handle))
            .map(|entry| entry.blob.bytes.len())
            .sum::<usize>();
        for handle in released {
            registry.release(&handle);
        }
        Ok(())
    }

    /// Bytes this owner currently holds.
    pub fn held_bytes(&self) -> usize {
        lock_registry()
            .owners
            .get(&self.id)
            .map_or(0, |owner| owner.bytes)
    }
}

impl Drop for BlobOwner {
    fn drop(&mut self) {
        let mut registry = lock_registry();
        if let Some(owner) = registry.owners.remove(&self.id) {
            for handle in owner.held.keys() {
                registry.release(handle);
            }
        }
    }
}

fn blob_handle(format: BlobFormat, bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format.mime().as_bytes());
    hasher.update([0]);
    if let BlobFormat::Raw {
        width,
        height,
        stride,
        ..
    } = format
    {
        for value in [width, height, stride] {
            hasher.update(value.to_le_bytes());
        }
    }
    hasher.update(bytes);
    let mut handle = String::with_capacity(BLOB_HANDLE_PREFIX.len() + BLOB_DIGEST_HEX_LEN);
    handle.push_str(BLOB_HANDLE_PREFIX);
    for byte in hasher.finalize() {
        use std::fmt::Write;
        let _ = write!(handle, "{byte:02x}");
    }
    handle
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argb(width: u32, height: u32, fill: u8) -> (BlobFormat, Vec<u8>) {
        let format = BlobFormat::from_mime(
            MIME_ARGB32,
            BlobMeta {
                width: Some(width),
                height: Some(height),
                stride: None,
            },
        )
        .unwrap();
        (format, vec![fill; (width * height * 4) as usize])
    }

    #[test]
    fn handles_are_content_addressed_and_well_formed() {
        let owner = BlobOwner::new();
        let (format, bytes) = argb(2, 2, 7);
        let first = owner.put(format, bytes.clone()).unwrap();
        let second = owner.put(format, bytes).unwrap();
        assert_eq!(first, second);
        assert!(is_blob_handle(&first));
        assert!(!is_blob_handle("mesh-blob:sha256-xyz"));
        assert!(!is_blob_handle(&first.to_uppercase()));
        let (other_format, other_bytes) = argb(1, 4, 7);
        assert_ne!(owner.put(other_format, other_bytes).unwrap(), first);
    }

    #[test]
    fn raw_formats_validate_dimensions_stride_and_length() {
        let meta = |width, height, stride| BlobMeta {
            width: Some(width),
            height: Some(height),
            stride,
        };
        assert!(matches!(
            BlobFormat::from_mime(MIME_RGB8, BlobMeta::default()),
            Err(BlobError::MissingDimensions(_))
        ));
        assert!(matches!(
            BlobFormat::from_mime(MIME_RGBA8, meta(0, 4, None)),
            Err(BlobError::InvalidDimensions { .. })
        ));
        assert!(matches!(
            BlobFormat::from_mime(MIME_RGB8, meta(4, 4, Some(8))),
            Err(BlobError::InvalidStride { stride: 8, row: 12 })
        ));
        assert!(matches!(
            BlobFormat::from_mime("image/gif", BlobMeta::default()),
            Err(BlobError::UnsupportedMime(_))
        ));
        // A padded stride needs no padding after the last row.
        let format = BlobFormat::from_mime(MIME_RGB8, meta(2, 2, Some(8))).unwrap();
        let owner = BlobOwner::new();
        assert!(owner.put(format, vec![0; 8 + 6]).is_ok());
        assert_eq!(
            owner.put(format, vec![0; 13]),
            Err(BlobError::TooShort {
                expected: 14,
                actual: 13
            })
        );
    }

    #[test]
    fn publication_pins_referenced_blobs_and_releases_the_rest_after_one_grace() {
        let owner = BlobOwner::new();
        let (format, bytes) = argb(3, 3, 1);
        let kept = owner.put(format, bytes).unwrap();
        let (format, bytes) = argb(3, 3, 2);
        let dropped = owner.put(format, bytes).unwrap();

        owner.publish([kept.as_ref()]).unwrap();
        assert!(lookup_blob(&dropped).is_some(), "one publication of grace");
        owner.publish([kept.as_ref()]).unwrap();
        assert!(lookup_blob(&dropped).is_none());
        assert!(lookup_blob(&kept).is_some());
        assert_eq!(owner.held_bytes(), 36);

        owner.publish([]).unwrap();
        assert!(lookup_blob(&kept).is_none());
        assert_eq!(owner.held_bytes(), 0);
    }

    #[test]
    fn state_cannot_reference_another_owners_or_unknown_blobs() {
        let producer = BlobOwner::new();
        let other = BlobOwner::new();
        let (format, bytes) = argb(2, 1, 9);
        let handle = producer.put(format, bytes).unwrap();
        assert_eq!(
            other.publish([handle.as_ref()]),
            Err(BlobError::Unregistered(handle.to_string()))
        );
        let unknown = format!("{BLOB_HANDLE_PREFIX}{}", "0".repeat(64));
        assert!(producer.publish([unknown.as_str()]).is_err());
        // A failed publication changes nothing.
        producer.publish([handle.as_ref()]).unwrap();
        assert!(lookup_blob(&handle).is_some());
    }

    #[test]
    fn retyping_shares_held_bytes_under_a_new_handle() {
        let owner = BlobOwner::new();
        let raw = owner.put(BlobFormat::Bytes, vec![1, 2, 3, 4]).unwrap();
        let (format, _) = argb(1, 1, 0);
        let image = owner.retype(&raw, format).unwrap();
        assert_ne!(raw, image);
        let (raw_blob, image_blob) = (lookup_blob(&raw).unwrap(), lookup_blob(&image).unwrap());
        assert!(Arc::ptr_eq(raw_blob.bytes(), image_blob.bytes()));
        assert_eq!(image_blob.format(), format);
        assert!(BlobOwner::new().retype(&raw, format).is_err());
        let (too_big, _) = argb(2, 1, 0);
        assert!(matches!(owner.retype(&raw, too_big), Err(BlobError::TooShort { .. })));
    }

    #[test]
    fn shared_content_lives_until_its_last_owner_drops() {
        let first = BlobOwner::new();
        let second = BlobOwner::new();
        let (format, bytes) = argb(4, 1, 5);
        let handle = first.put(format, bytes.clone()).unwrap();
        second.put(format, bytes).unwrap();
        drop(first);
        assert!(lookup_blob(&handle).is_some());
        drop(second);
        assert!(lookup_blob(&handle).is_none());
    }

    #[test]
    fn owners_are_bounded_by_blob_and_total_size() {
        let owner = BlobOwner::new();
        let encoded = BlobFormat::Encoded(EncodedImageFormat::Png);
        assert_eq!(
            owner.put(encoded, vec![0; MAX_BLOB_BYTES + 1]),
            Err(BlobError::TooLarge(MAX_BLOB_BYTES + 1))
        );
        for fill in 0..(MAX_OWNER_BLOB_BYTES / MAX_BLOB_BYTES) {
            owner.put(encoded, vec![fill as u8; MAX_BLOB_BYTES]).unwrap();
        }
        assert_eq!(
            owner.put(encoded, vec![0xff; 16]),
            Err(BlobError::QuotaExceeded)
        );
    }
}
