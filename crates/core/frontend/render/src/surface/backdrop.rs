//! The shell backdrop source behind `backdrop-filter: refract()`.
//!
//! A Wayland client never sees what the compositor shows behind its surfaces,
//! so displacement ("liquid glass") cannot read the real backdrop. It samples
//! one image MESH holds instead — the wallpaper the active profile paints —
//! cover-fit to the output and placed at the surface's position on it. That
//! source remains available to background-layer wallpaper surfaces; other layers
//! use compositor blur of the real backdrop and only the rim is painted here
//! (spec 04 §11).

use mesh_core_elements::VisualFilter;
use mesh_core_elements::style::Corners;
use skia_safe::{
    AlphaType, Canvas, ClipOp, ColorType, Data, FilterMode, ISize, ImageInfo, Matrix, MipmapMode,
    Paint, RRect, Rect, RuntimeEffect, SamplingOptions, Shader, TileMode, image_filters, images,
    runtime_effect::ChildPtr, surfaces,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

/// Where the surface being painted sits on its output, in logical pixels.
/// The shell sets it before each surface paints; refraction samples the
/// backdrop at `origin + local position`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BackdropFrame {
    pub origin: (f32, f32),
    pub output: (f32, f32),
    /// The caller explicitly permits sampling the configured image. The shell
    /// enables this only for background-layer wallpaper surfaces; desktop
    /// widgets use compositor blur because a foreign wallpaper may lie beneath.
    pub source_beneath: bool,
}

/// Cover-fit, pre-blurred copies are bounded by bytes rather than count: a
/// glass theme uses around ten blur radii, and evicting one re-blurs the whole
/// output on the paint path. A 2880x1800 unblurred copy is about 20 MiB;
/// blurred copies are downsampled and much smaller.
const PREPARED_CACHE_BYTES: usize = 96 << 20;

/// Largest downsampling of a blurred copy. A Gaussian of sigma `s` leaves
/// nothing above roughly `1 / s` cycles per pixel, so a copy downsampled by
/// `f <= s / 2` and bilinearly upsampled is visually identical to the
/// full-resolution blur while costing `f * f` times fewer pixels.
const MAX_PREPARED_DOWNSAMPLE: i32 = 8;

fn prepared_downsample(blur: f32) -> i32 {
    let sigma = blur.max(0.0) / 2.0;
    let mut factor = 1;
    while factor < MAX_PREPARED_DOWNSAMPLE && (factor * 2) as f32 <= sigma / 2.0 {
        factor *= 2;
    }
    factor
}

struct BackdropSource {
    path: PathBuf,
    image: skia_safe::Image,
    prepared: Mutex<Vec<(PreparedKey, skia_safe::Image)>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreparedKey {
    width: i32,
    height: i32,
    blur_tenths: u32,
}

static SOURCE: LazyLock<Mutex<Option<Arc<BackdropSource>>>> = LazyLock::new(|| Mutex::new(None));

/// Load (or clear) the backdrop image. Reloading the same path is a no-op, so
/// the shell can call this on every settings change.
pub fn set_backdrop_image(path: Option<&Path>) -> Result<(), String> {
    let mut source = SOURCE.lock().map_err(|_| "backdrop source lock poisoned")?;
    let Some(path) = path else {
        *source = None;
        return Ok(());
    };
    if source.as_ref().is_some_and(|current| current.path == path) {
        return Ok(());
    }
    // Every prepared copy minifies the source with mipmaps; build them once
    // instead of on each copy.
    let image = decode(path)?;
    let image = image.with_default_mipmaps().unwrap_or(image);
    *source = Some(Arc::new(BackdropSource {
        path: path.to_path_buf(),
        image,
        prepared: Mutex::new(Vec::new()),
    }));
    Ok(())
}

pub fn has_backdrop_image() -> bool {
    SOURCE
        .lock()
        .map(|source| source.is_some())
        .unwrap_or(false)
}

fn decode(path: &Path) -> Result<skia_safe::Image, String> {
    let rgba = image::open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .to_rgba8();
    let (width, height) = rgba.dimensions();
    let info = ImageInfo::new(
        (width as i32, height as i32),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        None,
    );
    let row_bytes = width as usize * 4;
    images::raster_from_data(&info, Data::new_copy(rgba.as_raw()), row_bytes)
        .ok_or_else(|| format!("{}: could not upload decoded image", path.display()))
}

impl BackdropSource {
    /// The source cover-fit to a `width` x `height` device output and blurred
    /// by `blur` device pixels, built once per key. A blurred copy may be
    /// smaller than the output; callers scale it up to `width` x `height`.
    fn prepared(&self, width: i32, height: i32, blur: f32) -> Option<skia_safe::Image> {
        let key = PreparedKey {
            width,
            height,
            blur_tenths: (blur.max(0.0) * 10.0).round() as u32,
        };
        let mut cache = self.prepared.lock().ok()?;
        if let Some(index) = cache.iter().position(|(cached, _)| *cached == key) {
            let entry = cache.remove(index);
            let image = entry.1.clone();
            cache.push(entry);
            return Some(image);
        }
        let factor = prepared_downsample(blur);
        let (copy_w, copy_h) = (
            (width + factor - 1) / factor,
            (height + factor - 1) / factor,
        );
        let mut surface = surfaces::raster_n32_premul(ISize::new(copy_w, copy_h))?;
        let canvas = surface.canvas();
        // Cover-fit in output coordinates, then map by the exact copy/output
        // ratio that `draw_refraction` inverts; rounding the copy size up must
        // not shift the image.
        canvas.scale((copy_w as f32 / width as f32, copy_h as f32 / height as f32));
        let (iw, ih) = (self.image.width() as f32, self.image.height() as f32);
        let scale = (width as f32 / iw).max(height as f32 / ih);
        let (dw, dh) = (iw * scale, ih * scale);
        let dst = Rect::from_xywh(
            (width as f32 - dw) / 2.0,
            (height as f32 - dh) / 2.0,
            dw,
            dh,
        );
        let mut paint = Paint::default();
        if blur > 0.0 {
            // Image-filter sigma is local; the canvas scale maps it into the copy.
            let sigma = blur / 2.0;
            paint.set_image_filter(image_filters::blur(
                (sigma, sigma),
                TileMode::Clamp,
                None,
                None,
            ));
        }
        canvas.draw_image_rect_with_sampling_options(
            &self.image,
            None,
            dst,
            SamplingOptions::new(FilterMode::Linear, MipmapMode::Linear),
            &paint,
        );
        let image = surface.image_snapshot();
        let bytes = |image: &skia_safe::Image| image.width() as usize * image.height() as usize * 4;
        let mut total =
            cache.iter().map(|(_, cached)| bytes(cached)).sum::<usize>() + bytes(&image);
        while total > PREPARED_CACHE_BYTES && !cache.is_empty() {
            total -= bytes(&cache.remove(0).1);
        }
        cache.push((key, image.clone()));
        Some(image)
    }
}

/// A rounded-rect lens. Inside an `edge`-wide rim the sample point moves along
/// the outward surface normal by up to `depth`, with a profile that is flat in
/// the middle and steepens toward the rim, so content just outside the glass
/// appears bent into its edge. `dispersion` pulls red and blue apart along the
/// same normal; a thin highlight lights the rim from the top-left.
const LIQUID_GLASS_SKSL: &str = r#"
uniform shader backdrop;
uniform float4 bounds;
uniform float4 radii;
uniform float depth;
uniform float edge;
uniform float dispersion;
uniform float rim;

float sd(float2 p, float2 center, float2 half_size, float r) {
    float2 q = abs(p - center) - half_size + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}

half4 main(float2 p) {
    float2 half_size = bounds.zw * 0.5;
    float2 center = bounds.xy + half_size;
    float2 d = p - center;
    float r = d.x < 0.0 ? (d.y < 0.0 ? radii.x : radii.w) : (d.y < 0.0 ? radii.y : radii.z);
    r = min(r, min(half_size.x, half_size.y));
    float dist = sd(p, center, half_size, r);
    float2 grad = float2(
        sd(p + float2(1.0, 0.0), center, half_size, r) - sd(p - float2(1.0, 0.0), center, half_size, r),
        sd(p + float2(0.0, 1.0), center, half_size, r) - sd(p - float2(0.0, 1.0), center, half_size, r));
    float glen = length(grad);
    float2 n = glen > 0.0001 ? grad / glen : float2(0.0);

    float t = edge > 0.0 ? clamp(1.0 + dist / edge, 0.0, 1.0) : 0.0;
    float k = t * t * t;
    float2 offset = n * depth * k;

    half4 color;
    if (dispersion > 0.0) {
        float2 split = n * dispersion * k;
        color = half4(
            backdrop.eval(p + offset + split).r,
            backdrop.eval(p + offset).g,
            backdrop.eval(p + offset - split).b,
            1.0);
    } else {
        color = backdrop.eval(p + offset);
    }

    if (rim > 0.0) {
        float band = clamp(1.0 + dist / rim, 0.0, 1.0);
        float light = clamp(dot(n, normalize(float2(-1.0, -1.4))), 0.0, 1.0);
        color.rgb += half3(band * band * (0.18 + 0.32 * light));
    }
    return color;
}
"#;

/// The lens's rim highlight alone, as premultiplied white, for glass whose
/// backdrop the compositor blurs. Source-over white of alpha `a` lightens the
/// blurred backdrop by about the `+a` the lens adds to its sample. The shader
/// computes its own edge coverage: Skia's anti-aliasing of a shaded rounded
/// rect depends on whether the clip cuts the arc, which would make a partial
/// repaint differ from a full one.
const GLASS_RIM_SKSL: &str = r#"
uniform float4 bounds;
uniform float4 radii;
uniform float rim;

float sd(float2 p, float2 center, float2 half_size, float r) {
    float2 q = abs(p - center) - half_size + r;
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - r;
}

half4 main(float2 p) {
    float2 half_size = bounds.zw * 0.5;
    float2 center = bounds.xy + half_size;
    float2 d = p - center;
    float r = d.x < 0.0 ? (d.y < 0.0 ? radii.x : radii.w) : (d.y < 0.0 ? radii.y : radii.z);
    r = min(r, min(half_size.x, half_size.y));
    float dist = sd(p, center, half_size, r);
    float2 grad = float2(
        sd(p + float2(1.0, 0.0), center, half_size, r) - sd(p - float2(1.0, 0.0), center, half_size, r),
        sd(p + float2(0.0, 1.0), center, half_size, r) - sd(p - float2(0.0, 1.0), center, half_size, r));
    float glen = length(grad);
    float2 n = glen > 0.0001 ? grad / glen : float2(0.0);
    float band = clamp(1.0 + dist / rim, 0.0, 1.0);
    float light = clamp(dot(n, normalize(float2(-1.0, -1.4))), 0.0, 1.0);
    float coverage = clamp(0.5 - dist, 0.0, 1.0);
    half a = half(min(band * band * (0.18 + 0.32 * light), 1.0) * coverage);
    return half4(a, a, a, a);
}
"#;

thread_local! {
    // `RuntimeEffect` is not `Send`; painting happens on the renderer thread,
    // which compiles the shader once on first use.
    static EFFECT: Option<RuntimeEffect> = RuntimeEffect::make_for_shader(LIQUID_GLASS_SKSL, None)
        .map_err(|error| tracing::error!("liquid glass shader failed to compile: {error}"))
        .ok();
    static RIM_EFFECT: Option<RuntimeEffect> = RuntimeEffect::make_for_shader(GLASS_RIM_SKSL, None)
        .map_err(|error| tracing::error!("glass rim shader failed to compile: {error}"))
        .ok();
}

/// Paint the refracted backdrop into the device-space rounded rect. `rect`,
/// `radii` and `filter` are device-scaled; `frame` is device-scaled too.
/// Returns false when there is no source to sample.
pub(crate) fn draw_refraction(
    canvas: &Canvas,
    rect: Rect,
    radii: Corners,
    filter: VisualFilter,
    frame: BackdropFrame,
) -> bool {
    let Some(source) = SOURCE.lock().ok().and_then(|source| source.clone()) else {
        return false;
    };
    let (output_w, output_h) = (frame.output.0.round() as i32, frame.output.1.round() as i32);
    if output_w <= 0 || output_h <= 0 || rect.is_empty() {
        return false;
    }
    let Some(prepared) = source.prepared(output_w, output_h, filter.blur_radius) else {
        return false;
    };
    crate::prepared_scene::retain_image(&prepared);
    // Prepared-image coordinates -> output coordinates -> surface-local device
    // coordinates; a downsampled copy scales back up to the output.
    let mut local = Matrix::translate((-frame.origin.0, -frame.origin.1));
    local.pre_scale(
        (
            output_w as f32 / prepared.width() as f32,
            output_h as f32 / prepared.height() as f32,
        ),
        None,
    );
    let Some(backdrop) = prepared.to_shader(
        (TileMode::Clamp, TileMode::Clamp),
        SamplingOptions::new(FilterMode::Linear, MipmapMode::None),
        &local,
    ) else {
        return false;
    };
    draw_backdrop(canvas, rect, radii, filter, backdrop)
}

/// Paint only the rim highlight `refract()` gives the glass edge, over
/// whatever is already beneath. `rect`, `radii` and `filter` are
/// device-scaled. Returns false when the element has no rim.
pub(crate) fn draw_rim(canvas: &Canvas, rect: Rect, radii: Corners, filter: VisualFilter) -> bool {
    let rim = rim_width(filter);
    if rim <= 0.0 || rect.is_empty() {
        return false;
    }
    let Some(paint) = rim_paint(rect, radii, rim) else {
        return false;
    };
    // The shader is exactly zero where `dist <= -rim`. The raster backend
    // interprets it per pixel, so a large panel would otherwise spend most of
    // a partial repaint shading interior pixels that it leaves unchanged.
    let save_count = canvas.save();
    for interior in band_interior(rect, radii, rim) {
        if !interior.is_empty() {
            canvas.clip_rect(interior, ClipOp::Difference, false);
        }
    }
    canvas.draw_rect(rect.with_outset((1.0, 1.0)), &paint);
    canvas.restore_to_count(save_count);
    true
}

fn rim_paint(rect: Rect, radii: Corners, rim: f32) -> Option<Paint> {
    let uniforms: [f32; 9] = [
        rect.left,
        rect.top,
        rect.width(),
        rect.height(),
        radii.top_left,
        radii.top_right,
        radii.bottom_right,
        radii.bottom_left,
        rim,
    ];
    let bytes: Vec<u8> = uniforms
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect();
    let shader = RIM_EFFECT.with(|effect| {
        effect
            .as_ref()?
            .make_shader(Data::new_copy(&bytes), &[], None)
    })?;
    let mut paint = Paint::default();
    paint.set_anti_alias(false);
    paint.set_shader(shader);
    Some(paint)
}

/// Two rects whose union holds only points at least `band` inside the rounded
/// rect, plus a margin: one clear of the top and bottom corner arcs,
/// one clear of the left and right ones. Outside the corner squares the
/// signed distance is the distance to the nearest straight edge.
fn band_interior(rect: Rect, radii: Corners, band: f32) -> [Rect; 2] {
    const MARGIN: f32 = 2.0;
    // The shader clamps each radius to half the shorter side.
    let max_radius = rect.width().min(rect.height()) / 2.0;
    let inset = |radius: f32| radius.min(max_radius).max(band) + MARGIN;
    let top = inset(radii.top_left.max(radii.top_right));
    let bottom = inset(radii.bottom_left.max(radii.bottom_right));
    let left = inset(radii.top_left.max(radii.bottom_left));
    let right = inset(radii.top_right.max(radii.bottom_right));
    let edge = band + MARGIN;
    [
        Rect::new(
            rect.left + edge,
            rect.top + top,
            rect.right - edge,
            rect.bottom - bottom,
        ),
        Rect::new(
            rect.left + left,
            rect.top + edge,
            rect.right - right,
            rect.bottom - edge,
        ),
    ]
}

fn rim_width(filter: VisualFilter) -> f32 {
    if filter.refract_edge > 0.0 {
        (filter.refract_edge * 0.12).clamp(1.0, 3.0)
    } else {
        0.0
    }
}

fn draw_backdrop(
    canvas: &Canvas,
    rect: Rect,
    radii: Corners,
    filter: VisualFilter,
    backdrop: Shader,
) -> bool {
    // With no rim or dispersion the lens evaluates to the input shader,
    // including its alpha. Wallpaper must not pay for an SDF per output pixel.
    let shader = if filter.refract_edge <= 0.0 && filter.dispersion <= 0.0 {
        backdrop
    } else {
        let Some(shader) = lens_shader(rect, radii, filter, backdrop) else {
            return false;
        };
        shader
    };
    draw_shader(canvas, rect, radii, shader);
    true
}

fn lens_shader(
    rect: Rect,
    radii: Corners,
    filter: VisualFilter,
    backdrop: Shader,
) -> Option<Shader> {
    EFFECT.with(|effect| make_lens_shader(effect.as_ref()?, rect, radii, filter, backdrop))
}

fn make_lens_shader(
    effect: &RuntimeEffect,
    rect: Rect,
    radii: Corners,
    filter: VisualFilter,
    backdrop: Shader,
) -> Option<Shader> {
    let rim = rim_width(filter);
    let uniforms: [f32; 12] = [
        rect.left,
        rect.top,
        rect.width(),
        rect.height(),
        radii.top_left,
        radii.top_right,
        radii.bottom_right,
        radii.bottom_left,
        filter.refract_depth,
        filter.refract_edge,
        filter.dispersion,
        rim,
    ];
    let bytes: Vec<u8> = uniforms
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect();
    effect.make_shader(Data::new_copy(&bytes), &[ChildPtr::Shader(backdrop)], None)
}

fn draw_shader(canvas: &Canvas, rect: Rect, radii: Corners, shader: Shader) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_shader(shader);
    let rrect = RRect::new_rect_radii(
        rect,
        &[
            (radii.top_left, radii.top_left).into(),
            (radii.top_right, radii.top_right).into(),
            (radii.bottom_right, radii.bottom_right).into(),
            (radii.bottom_left, radii.bottom_left).into(),
        ],
    );
    canvas.draw_rrect(rrect, &paint);
}

#[cfg(test)]
mod tests;
