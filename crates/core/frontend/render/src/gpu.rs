//! Skia-GL (Ganesh) execution over an EGL context.
//!
//! A [`GpuContext`] is not `Send`: EGL binds a context to the thread that made
//! it current, so the context must be created, used and dropped on the
//! renderer-owning thread (`scene_thread`). Presentation is not owned here;
//! offscreen targets are read back or, later, exported as dmabuf buffers that
//! the presentation layer commits.

use std::ffi::c_void;
use std::fmt;

use khronos_egl as egl;
use skia_safe::gpu::{self, DirectContext, SurfaceOrigin};
use skia_safe::{AlphaType, ColorType, ImageInfo, Surface};

use crate::{PixelBuffer, PixelCanvasSession};

/// `EGL_MESA_platform_surfaceless`: a display with no window system, used for
/// offscreen rendering and tests.
const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;

type Egl = egl::DynamicInstance<egl::EGL1_5>;

#[derive(Debug)]
pub enum GpuError {
    LoadEgl(String),
    Egl(&'static str, egl::Error),
    NoConfig,
    MissingExtension(&'static str),
    SkiaInterface,
    SkiaContext,
    SurfaceAllocation { width: u32, height: u32 },
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

    // EGL displays are process-wide: every context on the same platform shares
    // one handle, and `eglTerminate` would invalidate it for all of them. The
    // display therefore stays initialized for the life of the process.
    fn from_display(egl: Egl, display: egl::Display) -> Result<Self, GpuError> {
        egl.initialize(display)
            .map_err(|error| GpuError::Egl("eglInitialize", error))?;
        let (context, skia, renderer) = Self::create_on(&egl, display)?;
        Ok(Self {
            skia,
            egl,
            display,
            context,
            renderer,
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
