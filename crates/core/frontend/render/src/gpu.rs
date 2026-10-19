//! Skia-GL (Ganesh) execution over an EGL context.
//!
//! A [`GpuContext`] is not `Send`: EGL binds a context to the thread that made
//! it current, so the context must be created, used and dropped on the
//! renderer-owning thread. Presentation is not owned here: offscreen targets
//! are read back, and [`DmabufTarget`]s export their storage as dmabuf planes
//! that the presentation layer wraps in `wl_buffer`s and commits.

pub mod scene;

use std::ffi::c_void;
use std::fmt;
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::fs::MetadataExt;

use khronos_egl as egl;
use skia_safe::gpu::{self, DirectContext, SurfaceOrigin};
use skia_safe::{AlphaType, BlendMode, ColorType, IRect, ImageInfo, Paint, Surface};

use crate::{DamageRect, PixelBuffer, PixelCanvasSession};

/// `EGL_MESA_platform_surfaceless`: a display with no window system, used for
/// offscreen rendering and tests.
const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;
/// `EGL_EXT_platform_device`: a display on one enumerated GPU.
const PLATFORM_DEVICE_EXT: egl::Enum = 0x313F;
const DRM_DEVICE_FILE_EXT: egl::Int = 0x3233;
const DRM_RENDER_NODE_FILE_EXT: egl::Int = 0x3377;
const GL_TEXTURE_2D_IMAGE_TARGET: egl::Enum = 0x30B1;
const GL_TEXTURE_LEVEL: egl::Attrib = 0x30BC;

const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_RGBA8: u32 = 0x8058;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_NEAREST: i32 = 0x2600;

/// `DRM_FORMAT_ABGR8888`: R, G, B, A bytes in memory, the layout of a GL
/// `RGBA8` texture on little-endian hosts.
pub const DRM_FORMAT_ABGR8888: u32 = u32::from_le_bytes(*b"AB24");

type Egl = egl::DynamicInstance<egl::EGL1_5>;

#[derive(Debug)]
pub enum GpuError {
    LoadEgl(String),
    Egl(&'static str, egl::Error),
    NoConfig,
    MissingExtension(&'static str),
    SkiaInterface,
    SkiaContext,
    NoDevice(u64),
    SurfaceAllocation { width: u32, height: u32 },
    DmabufExport,
    Readback,
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoadEgl(error) => write!(f, "cannot load libEGL: {error}"),
            Self::Egl(call, error) => write!(f, "{call} failed: {error}"),
            Self::NoConfig => f.write_str("no EGL config supports OpenGL ES 3"),
            Self::MissingExtension(name) => write!(f, "EGL extension {name} is unavailable"),
            Self::SkiaInterface => f.write_str("Skia could not assemble a GL interface"),
            Self::SkiaContext => f.write_str("Skia could not create a GL context"),
            Self::SurfaceAllocation { width, height } => {
                write!(f, "cannot allocate a {width}x{height} GPU render target")
            }
            Self::NoDevice(device) => write!(
                f,
                "no EGL device matches DRM device {}:{}",
                device >> 8 & 0xfff,
                device & 0xff
            ),
            Self::DmabufExport => f.write_str("EGL could not export the texture as a dmabuf"),
            Self::Readback => f.write_str("GPU readback failed"),
        }
    }
}

impl std::error::Error for GpuError {}

/// An EGL display and OpenGL ES 3 context with a Skia `DirectContext` on top.
pub struct GpuContext {
    skia: DirectContext,
    egl: Egl,
    display: egl::Display,
    context: egl::Context,
    renderer: String,
    textures: Option<TextureFns>,
    export: Option<DmabufExportFns>,
    fence: Option<NativeFenceFns>,
}

type GenTextures = unsafe extern "system" fn(i32, *mut u32);
type DeleteTextures = unsafe extern "system" fn(i32, *const u32);
type BindTexture = unsafe extern "system" fn(u32, u32);
type TexStorage2D = unsafe extern "system" fn(u32, i32, u32, i32, i32);
type TexParameteri = unsafe extern "system" fn(u32, u32, i32);
type ExportQuery =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *mut i32, *mut i32, *mut u64) -> u32;
type Export =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *mut i32, *mut i32, *mut i32) -> u32;

/// Raw GL entry points for textures Skia does not own.
#[derive(Clone, Copy)]
struct TextureFns {
    gen_textures: GenTextures,
    delete_textures: DeleteTextures,
    bind_texture: BindTexture,
    tex_storage_2d: TexStorage2D,
    tex_parameteri: TexParameteri,
}

impl TextureFns {
    fn load(egl: &Egl) -> Option<Self> {
        let load = |name| egl.get_proc_address(name).map(|f| f as *const c_void);
        // SAFETY: each pointer is the named GLES 3 entry point, transmuted to
        // its specified signature.
        unsafe {
            Some(Self {
                gen_textures: std::mem::transmute::<*const c_void, GenTextures>(load(
                    "glGenTextures",
                )?),
                delete_textures: std::mem::transmute::<*const c_void, DeleteTextures>(load(
                    "glDeleteTextures",
                )?),
                bind_texture: std::mem::transmute::<*const c_void, BindTexture>(load(
                    "glBindTexture",
                )?),
                tex_storage_2d: std::mem::transmute::<*const c_void, TexStorage2D>(load(
                    "glTexStorage2D",
                )?),
                tex_parameteri: std::mem::transmute::<*const c_void, TexParameteri>(load(
                    "glTexParameteri",
                )?),
            })
        }
    }
}

type CreateSync = unsafe extern "system" fn(*mut c_void, u32, *const i32) -> *mut c_void;
type DestroySync = unsafe extern "system" fn(*mut c_void, *mut c_void) -> u32;
type DupNativeFence = unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32;
type GlFlush = unsafe extern "system" fn();

const SYNC_NATIVE_FENCE_ANDROID: u32 = 0x3144;
const SYNC_NATIVE_FENCE_FD_ANDROID: i32 = 0x3145;
const NO_NATIVE_FENCE_FD_ANDROID: i32 = -1;

/// `EGL_ANDROID_native_fence_sync` entry points: a sync file that signals
/// when the GPU finishes the work submitted before it.
#[derive(Clone, Copy)]
struct NativeFenceFns {
    create: CreateSync,
    destroy: DestroySync,
    dup: DupNativeFence,
    flush: GlFlush,
}

impl NativeFenceFns {
    fn load(egl: &Egl, display_extensions: &str) -> Option<Self> {
        if !has_extension(display_extensions, "EGL_ANDROID_native_fence_sync") {
            return None;
        }
        let load = |name| egl.get_proc_address(name);
        // SAFETY: each pointer is the named entry point, transmuted to its
        // specified signature.
        unsafe {
            Some(Self {
                create: std::mem::transmute::<extern "system" fn(), CreateSync>(load(
                    "eglCreateSyncKHR",
                )?),
                destroy: std::mem::transmute::<extern "system" fn(), DestroySync>(load(
                    "eglDestroySyncKHR",
                )?),
                dup: std::mem::transmute::<extern "system" fn(), DupNativeFence>(load(
                    "eglDupNativeFenceFDANDROID",
                )?),
                flush: std::mem::transmute::<extern "system" fn(), GlFlush>(load("glFlush")?),
            })
        }
    }
}

/// `EGL_MESA_image_dma_buf_export` entry points.
#[derive(Clone, Copy)]
struct DmabufExportFns {
    query: ExportQuery,
    export: Export,
}

impl DmabufExportFns {
    fn load(egl: &Egl, display_extensions: &str) -> Option<Self> {
        if !has_extension(display_extensions, "EGL_MESA_image_dma_buf_export")
            || !has_extension(display_extensions, "EGL_KHR_gl_texture_2D_image")
        {
            return None;
        }
        let query = egl.get_proc_address("eglExportDMABUFImageQueryMESA")?;
        let export = egl.get_proc_address("eglExportDMABUFImageMESA")?;
        // SAFETY: both pointers are the extension's entry points, transmuted
        // to their specified signatures.
        unsafe {
            Some(Self {
                query: std::mem::transmute::<extern "system" fn(), ExportQuery>(query),
                export: std::mem::transmute::<extern "system" fn(), Export>(export),
            })
        }
    }
}

impl GpuContext {
    /// Create a context on a surfaceless display. Fails when libEGL, the
    /// surfaceless platform or a GLES3 driver is unavailable; callers fall back
    /// to raster painting.
    pub fn headless() -> Result<Self, GpuError> {
        // SAFETY: loading libEGL runs no code beyond the library's initializers,
        // and the resolved symbols are checked against EGL 1.5.
        let egl = unsafe { Egl::load_required() }
            .map_err(|error| GpuError::LoadEgl(error.to_string()))?;
        let client_extensions = egl
            .query_string(None, egl::EXTENSIONS)
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !has_extension(&client_extensions, "EGL_MESA_platform_surfaceless") {
            return Err(GpuError::MissingExtension("EGL_MESA_platform_surfaceless"));
        }
        // SAFETY: the surfaceless platform takes no native display.
        let display = unsafe {
            egl.get_platform_display(
                PLATFORM_SURFACELESS_MESA,
                egl::DEFAULT_DISPLAY,
                &[egl::ATTRIB_NONE],
            )
        }
        .map_err(|error| GpuError::Egl("eglGetPlatformDisplay", error))?;
        Self::from_display(egl, display)
    }

    /// Create a context on the GPU whose DRM primary or render node has the
    /// device number `device` (`st_rdev`), such as the main device a Wayland
    /// compositor reports in its dmabuf feedback. Buffers rendered here are
    /// importable by that compositor without a cross-device copy.
    pub fn on_drm_device(device: u64) -> Result<Self, GpuError> {
        // SAFETY: as in `headless`.
        let egl = unsafe { Egl::load_required() }
            .map_err(|error| GpuError::LoadEgl(error.to_string()))?;
        let client_extensions = egl
            .query_string(None, egl::EXTENSIONS)
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        for name in ["EGL_EXT_device_enumeration", "EGL_EXT_platform_device"] {
            if !has_extension(&client_extensions, name) {
                return Err(GpuError::MissingExtension(name));
            }
        }
        let egl_device = find_drm_device(&egl, device).ok_or(GpuError::NoDevice(device))?;
        // SAFETY: the device handle came from `eglQueryDevicesEXT`.
        let display = unsafe {
            egl.get_platform_display(PLATFORM_DEVICE_EXT, egl_device, &[egl::ATTRIB_NONE])
        }
        .map_err(|error| GpuError::Egl("eglGetPlatformDisplay", error))?;
        Self::from_display(egl, display)
    }

    // EGL displays are process-wide: every context on the same platform shares
    // one handle, and `eglTerminate` would invalidate it for all of them. The
    // display therefore stays initialized for the life of the process.
    fn from_display(egl: Egl, display: egl::Display) -> Result<Self, GpuError> {
        egl.initialize(display)
            .map_err(|error| GpuError::Egl("eglInitialize", error))?;
        let (context, skia, renderer) = Self::create_on(&egl, display)?;
        let display_extensions = egl
            .query_string(Some(display), egl::EXTENSIONS)
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        let textures = TextureFns::load(&egl);
        let export = DmabufExportFns::load(&egl, &display_extensions);
        let fence = NativeFenceFns::load(&egl, &display_extensions);
        Ok(Self {
            skia,
            egl,
            display,
            context,
            renderer,
            textures,
            export,
            fence,
        })
    }

    fn create_on(
        egl: &Egl,
        display: egl::Display,
    ) -> Result<(egl::Context, DirectContext, String), GpuError> {
        let display_extensions = egl
            .query_string(Some(display), egl::EXTENSIONS)
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !has_extension(&display_extensions, "EGL_KHR_surfaceless_context") {
            return Err(GpuError::MissingExtension("EGL_KHR_surfaceless_context"));
        }
        egl.bind_api(egl::OPENGL_ES_API)
            .map_err(|error| GpuError::Egl("eglBindAPI", error))?;
        let config = egl
            .choose_first_config(
                display,
                // A zero surface mask admits pbuffer-only configs; the default
                // requires window support, which surfaceless displays lack.
                &[
                    egl::RENDERABLE_TYPE,
                    egl::OPENGL_ES3_BIT,
                    egl::SURFACE_TYPE,
                    0,
                    egl::NONE,
                ],
            )
            .map_err(|error| GpuError::Egl("eglChooseConfig", error))?
            .ok_or(GpuError::NoConfig)?;
        let context = egl
            .create_context(
                display,
                config,
                None,
                &[egl::CONTEXT_MAJOR_VERSION, 3, egl::NONE],
            )
            .map_err(|error| GpuError::Egl("eglCreateContext", error))?;
        let skia = egl
            .make_current(display, None, None, Some(context))
            .map_err(|error| GpuError::Egl("eglMakeCurrent", error))
            .and_then(|()| {
                let interface = gpu::gl::Interface::new_load_with(|name| {
                    egl.get_proc_address(name)
                        .map_or(std::ptr::null(), |function| function as *const c_void)
                })
                .ok_or(GpuError::SkiaInterface)?;
                let renderer = gl_renderer_name(egl);
                let skia =
                    gpu::direct_contexts::make_gl(interface, None).ok_or(GpuError::SkiaContext)?;
                Ok((skia, renderer))
            });
        match skia {
            Ok((skia, renderer)) => Ok((context, skia, renderer)),
            Err(error) => {
                let _ = egl.make_current(display, None, None, None);
                let _ = egl.destroy_context(display, context);
                Err(error)
            }
        }
    }

    /// The driver's `GL_RENDERER` string, for diagnostics and profiling.
    pub fn renderer(&self) -> &str {
        &self.renderer
    }

    /// Allocate a premultiplied BGRA render target, matching `PixelBuffer`'s
    /// byte layout so readback is a straight copy.
    pub fn offscreen_target(&mut self, width: u32, height: u32) -> Result<GpuTarget, GpuError> {
        let info = target_info(width, height);
        gpu::surfaces::render_target(
            &mut self.skia,
            gpu::Budgeted::Yes,
            &info,
            None,
            SurfaceOrigin::TopLeft,
            None,
            false,
            None,
        )
        .map(|surface| GpuTarget {
            surface,
            width,
            height,
        })
        .ok_or(GpuError::SurfaceAllocation { width, height })
    }

    /// Submit recorded work to the driver without waiting for completion.
    pub fn flush(&mut self) {
        self.skia.flush_and_submit();
    }

    /// Submit recorded work and wait until the GPU has executed it, so a
    /// buffer handed to another process holds the finished frame.
    pub fn finish(&mut self) {
        self.skia.flush_submit_and_sync_cpu();
    }

    /// Submit recorded work and make `target`'s dmabuf carry a write fence
    /// for it, so an importer that honors implicit sync waits for the GPU
    /// instead of this thread. Falls back to [`Self::finish`] when the driver
    /// or kernel cannot attach the fence.
    pub fn submit_for(&mut self, target: &DmabufTarget) {
        self.flush();
        if !self.attach_write_fence(target) {
            self.finish();
        }
    }

    fn attach_write_fence(&self, target: &DmabufTarget) -> bool {
        let Some(fence) = self.fence else {
            return false;
        };
        let display = self.display.as_ptr();
        let attributes = [
            SYNC_NATIVE_FENCE_FD_ANDROID,
            NO_NATIVE_FENCE_FD_ANDROID,
            egl::NONE,
        ];
        // SAFETY: the display is live and the attribute list is terminated.
        let sync =
            unsafe { (fence.create)(display, SYNC_NATIVE_FENCE_ANDROID, attributes.as_ptr()) };
        if sync.is_null() {
            return false;
        }
        // The fence fd exists once the sync command has been flushed.
        // SAFETY: a context is current on this thread.
        unsafe { (fence.flush)() };
        // SAFETY: `sync` is a live native-fence sync of this display.
        let fd = unsafe { (fence.dup)(display, sync) };
        // SAFETY: as above; the duplicated fd outlives the sync object.
        unsafe { (fence.destroy)(display, sync) };
        if fd < 0 {
            return false;
        }
        // SAFETY: `dup` transferred a new descriptor to us.
        let sync_file = unsafe { OwnedFd::from_raw_fd(fd) };
        target
            .planes
            .planes
            .first()
            .is_some_and(|plane| import_write_fence(&plane.fd, &sync_file))
    }

    /// Whether [`Self::dmabuf_target`] can export render targets.
    pub fn supports_dmabuf_export(&self) -> bool {
        self.textures.is_some() && self.export.is_some()
    }

    /// Allocate a render target whose storage is exported as dmabuf planes.
    /// The driver picks the modifier; callers check it against what the
    /// consumer accepts before sharing the planes.
    pub fn dmabuf_target(&mut self, width: u32, height: u32) -> Result<DmabufTarget, GpuError> {
        let (Some(fns), Some(export)) = (self.textures, self.export) else {
            return Err(GpuError::MissingExtension("EGL_MESA_image_dma_buf_export"));
        };
        let allocation = GpuError::SurfaceAllocation { width, height };
        let (gl_width, gl_height) = (
            i32::try_from(width).map_err(|_| GpuError::SurfaceAllocation { width, height })?,
            i32::try_from(height).map_err(|_| GpuError::SurfaceAllocation { width, height })?,
        );
        let mut texture = 0;
        // SAFETY: this context is current on this thread; the calls follow
        // the GLES 3 texture-storage contract.
        unsafe {
            (fns.gen_textures)(1, &mut texture);
            (fns.bind_texture)(GL_TEXTURE_2D, texture);
            (fns.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST);
            (fns.tex_parameteri)(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST);
            (fns.tex_storage_2d)(GL_TEXTURE_2D, 1, GL_RGBA8, gl_width, gl_height);
            (fns.bind_texture)(GL_TEXTURE_2D, 0);
        }
        // Skia caches GL bindings; tell it they changed behind its back.
        self.skia.reset(None);
        let texture = OwnedTexture {
            id: texture,
            delete: fns.delete_textures,
        };
        let planes = self.export_texture(&texture, export)?;
        let info = gpu::gl::TextureInfo {
            target: GL_TEXTURE_2D,
            id: texture.id,
            format: GL_RGBA8,
            protected: gpu::Protected::No,
        };
        // SAFETY: `info` names a live, complete RGBA8 texture that outlives
        // the wrapping surface (`DmabufTarget` drops the surface first).
        let backend = unsafe {
            gpu::backend_textures::make_gl(
                (gl_width, gl_height),
                gpu::Mipmapped::No,
                info,
                "mesh-dmabuf",
            )
        };
        let surface = gpu::surfaces::wrap_backend_texture(
            &mut self.skia,
            &backend,
            SurfaceOrigin::TopLeft,
            None,
            ColorType::RGBA8888,
            None,
            None,
        )
        .ok_or(allocation)?;
        Ok(DmabufTarget {
            surface,
            _texture: texture,
            planes,
            width,
            height,
        })
    }

    fn export_texture(
        &self,
        texture: &OwnedTexture,
        export: DmabufExportFns,
    ) -> Result<DmabufPlanes, GpuError> {
        // SAFETY: the client buffer is a texture name of this context, as
        // `EGL_GL_TEXTURE_2D` requires.
        let image = self
            .egl
            .create_image(
                self.display,
                self.context,
                GL_TEXTURE_2D_IMAGE_TARGET,
                unsafe { egl::ClientBuffer::from_ptr(texture.id as usize as *mut c_void) },
                &[GL_TEXTURE_LEVEL, 0, egl::ATTRIB_NONE],
            )
            .map_err(|error| GpuError::Egl("eglCreateImage", error))?;
        let planes = self.export_image(&image, export);
        // The exported descriptors keep the storage alive; the image is only
        // the export handle.
        let _ = self.egl.destroy_image(self.display, image);
        planes
    }

    fn export_image(
        &self,
        image: &egl::Image,
        export: DmabufExportFns,
    ) -> Result<DmabufPlanes, GpuError> {
        let (mut fourcc, mut plane_count, mut modifier) = (0i32, 0i32, 0u64);
        // SAFETY: the display and image are live; the out-pointers are valid.
        let queried = unsafe {
            (export.query)(
                self.display.as_ptr(),
                image.as_ptr(),
                &mut fourcc,
                &mut plane_count,
                &mut modifier,
            )
        };
        if queried == 0 || !(1..=4).contains(&plane_count) {
            return Err(GpuError::DmabufExport);
        }
        let (mut fds, mut strides, mut offsets) = ([-1i32; 4], [0i32; 4], [0i32; 4]);
        // SAFETY: each array holds the four entries the extension may write.
        let exported = unsafe {
            (export.export)(
                self.display.as_ptr(),
                image.as_ptr(),
                fds.as_mut_ptr(),
                strides.as_mut_ptr(),
                offsets.as_mut_ptr(),
            )
        };
        let count = plane_count as usize;
        // Take ownership first so every returned descriptor is closed.
        let owned: Vec<Option<OwnedFd>> = fds[..count]
            .iter()
            // SAFETY: a non-negative entry is a new descriptor the driver
            // transferred to us.
            .map(|&fd| (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) }))
            .collect();
        if exported == 0 {
            return Err(GpuError::DmabufExport);
        }
        let mut planes = Vec::with_capacity(count);
        let mut last = None;
        for (index, fd) in owned.into_iter().enumerate() {
            // Planes of one buffer may share a descriptor, reported once.
            let fd = match fd {
                Some(fd) => fd,
                None => last
                    .as_ref()
                    .and_then(|fd: &OwnedFd| fd.try_clone().ok())
                    .ok_or(GpuError::DmabufExport)?,
            };
            last = fd.try_clone().ok();
            planes.push(DmabufPlane {
                fd,
                offset: offsets[index] as u32,
                stride: strides[index] as u32,
            });
        }
        Ok(DmabufPlanes {
            fourcc: fourcc as u32,
            modifier,
            planes,
        })
    }
}

impl Drop for GpuContext {
    fn drop(&mut self) {
        // Abandoning first stops Skia from issuing GL calls once the EGL
        // context is gone; destroying the context frees its GL objects.
        self.skia.abandon();
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_context(self.display, self.context);
    }
}

/// A GPU render target allocated from one [`GpuContext`]. Dropping it after
/// the context is safe: the abandoned Skia context makes the release a no-op.
pub struct GpuTarget {
    surface: Surface,
    width: u32,
    height: u32,
}

impl GpuTarget {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn canvas(&mut self) -> &skia_safe::Canvas {
        self.surface.canvas()
    }

    /// A paint session on this target. The painter, damage clears and scroll
    /// shifts run through it exactly as they do on a [`PixelBuffer`].
    pub fn session(&mut self) -> PixelCanvasSession<'_> {
        PixelCanvasSession::on_surface(&mut self.surface)
    }

    /// Copy the target into a CPU buffer. Synchronous and stalls the GPU
    /// pipeline; for tests, parity checks and debug snapshots, never per frame.
    pub fn read_back(&mut self, context: &mut GpuContext) -> Result<PixelBuffer, GpuError> {
        context.flush();
        let mut buffer =
            PixelBuffer::try_new(self.width, self.height).ok_or(GpuError::SurfaceAllocation {
                width: self.width,
                height: self.height,
            })?;
        let info = target_info(self.width, self.height);
        let stride = buffer.stride() as usize;
        if self
            .surface
            .read_pixels(&info, buffer.data_mut(), stride, (0, 0))
        {
            Ok(buffer)
        } else {
            Err(GpuError::Readback)
        }
    }
}

/// A GL texture name deleted on drop. Deleting after the context is gone is a
/// no-op, as for every other GL object.
struct OwnedTexture {
    id: u32,
    delete: DeleteTextures,
}

impl Drop for OwnedTexture {
    fn drop(&mut self) {
        // SAFETY: `id` was generated by this thread's context.
        unsafe { (self.delete)(1, &self.id) };
    }
}

/// One plane of an exported dmabuf.
#[derive(Debug)]
pub struct DmabufPlane {
    pub fd: OwnedFd,
    pub offset: u32,
    pub stride: u32,
}

/// The exported layout of a [`DmabufTarget`].
#[derive(Debug)]
pub struct DmabufPlanes {
    /// DRM fourcc of the buffer.
    pub fourcc: u32,
    /// DRM format modifier shared by every plane.
    pub modifier: u64,
    pub planes: Vec<DmabufPlane>,
}

/// An RGBA render target whose storage is shared with a dmabuf. Pixels drawn
/// here are visible to the dmabuf's importer once [`GpuContext::finish`]
/// returns.
pub struct DmabufTarget {
    // Dropped before the texture it wraps.
    surface: Surface,
    _texture: OwnedTexture,
    planes: DmabufPlanes,
    width: u32,
    height: u32,
}

impl DmabufTarget {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn planes(&self) -> &DmabufPlanes {
        &self.planes
    }

    pub fn canvas(&mut self) -> &skia_safe::Canvas {
        self.surface.canvas()
    }

    /// Replace `rects` (device pixels) with the same pixels of `source`.
    /// Targets of different sizes copy their overlap.
    pub fn copy_from(&mut self, source: &mut GpuTarget, rects: &[DamageRect]) {
        if rects.is_empty() {
            return;
        }
        let image = source.surface.image_snapshot();
        let canvas = self.surface.canvas();
        let mut paint = Paint::default();
        paint.set_blend_mode(BlendMode::Src);
        for rect in rects {
            let clip = IRect::from_xywh(
                rect.x.min(i32::MAX as u32) as i32,
                rect.y.min(i32::MAX as u32) as i32,
                rect.width.min(i32::MAX as u32) as i32,
                rect.height.min(i32::MAX as u32) as i32,
            );
            canvas.save();
            canvas.clip_irect(clip, skia_safe::ClipOp::Intersect);
            canvas.draw_image(&image, (0.0, 0.0), Some(&paint));
            canvas.restore();
        }
    }
}

/// Attach `sync_file` to `dmabuf` as an implicit write fence
/// (`DMA_BUF_IOCTL_IMPORT_SYNC_FILE`, Linux 6.0+).
fn import_write_fence(dmabuf: &OwnedFd, sync_file: &OwnedFd) -> bool {
    use std::os::fd::AsRawFd;
    #[repr(C)]
    struct ImportSyncFile {
        flags: u32,
        fd: i32,
    }
    const DMA_BUF_SYNC_WRITE: u32 = 2;
    // _IOW('b', 3, struct dma_buf_import_sync_file)
    const DMA_BUF_IOCTL_IMPORT_SYNC_FILE: libc::c_ulong = 0x4008_6203;
    let request = ImportSyncFile {
        flags: DMA_BUF_SYNC_WRITE,
        fd: sync_file.as_raw_fd(),
    };
    // SAFETY: both descriptors are open; the argument matches the ioctl's
    // structure and outlives the call.
    unsafe {
        libc::ioctl(
            dmabuf.as_raw_fd(),
            DMA_BUF_IOCTL_IMPORT_SYNC_FILE as _,
            &request as *const ImportSyncFile,
        ) == 0
    }
}

fn find_drm_device(egl: &Egl, device: u64) -> Option<*mut c_void> {
    type QueryDevices = unsafe extern "system" fn(i32, *mut *mut c_void, *mut i32) -> u32;
    type QueryDeviceString = unsafe extern "system" fn(*mut c_void, i32) -> *const std::ffi::c_char;
    // SAFETY: both are `EGL_EXT_device_enumeration`/`_query` entry points,
    // whose presence the caller checked, with their specified signatures.
    let (query_devices, query_string) = unsafe {
        (
            std::mem::transmute::<extern "system" fn(), QueryDevices>(
                egl.get_proc_address("eglQueryDevicesEXT")?,
            ),
            std::mem::transmute::<extern "system" fn(), QueryDeviceString>(
                egl.get_proc_address("eglQueryDeviceStringEXT")?,
            ),
        )
    };
    let mut devices = [std::ptr::null_mut(); 16];
    let mut count = 0;
    // SAFETY: the array has room for the 16 entries requested.
    if unsafe { query_devices(devices.len() as i32, devices.as_mut_ptr(), &mut count) } == 0 {
        return None;
    }
    devices[..count.clamp(0, 16) as usize]
        .iter()
        .copied()
        .find(|&candidate| {
            [DRM_RENDER_NODE_FILE_EXT, DRM_DEVICE_FILE_EXT]
                .into_iter()
                .any(|name| {
                    // SAFETY: the device came from `eglQueryDevicesEXT`; a
                    // non-null result is a static NUL-terminated string.
                    let path = unsafe { query_string(candidate, name) };
                    if path.is_null() {
                        return false;
                    }
                    let path = unsafe { std::ffi::CStr::from_ptr(path) };
                    path.to_str()
                        .ok()
                        .and_then(|path| std::fs::metadata(path).ok())
                        .is_some_and(|metadata| metadata.rdev() == device)
                })
        })
}

fn target_info(width: u32, height: u32) -> ImageInfo {
    ImageInfo::new(
        (width as i32, height as i32),
        ColorType::BGRA8888,
        AlphaType::Premul,
        None,
    )
}

fn has_extension(extensions: &str, name: &str) -> bool {
    extensions
        .split_ascii_whitespace()
        .any(|entry| entry == name)
}

fn gl_renderer_name(egl: &Egl) -> String {
    const GL_RENDERER: u32 = 0x1F01;
    let Some(get_string) = egl.get_proc_address("glGetString") else {
        return String::new();
    };
    // SAFETY: `glGetString` has this signature in every GL/GLES version, and a
    // context is current on this thread.
    let get_string: extern "system" fn(u32) -> *const std::ffi::c_char =
        unsafe { std::mem::transmute(get_string) };
    let name = get_string(GL_RENDERER);
    if name.is_null() {
        return String::new();
    }
    // SAFETY: GL returns a static NUL-terminated string.
    unsafe { std::ffi::CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests;
