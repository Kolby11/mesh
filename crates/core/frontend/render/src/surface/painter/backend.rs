use super::*;
use crate::surface::icon;
use mesh_core_elements::{BoxShadow, VisualFilter};
use skia_safe::{
    BlurStyle, Canvas, Color4f, Data, ImageInfo, MaskFilter, PaintStyle, Path as SkiaPath,
    PathBuilder, Point, RRect, Rect, TileMode, canvas::SaveLayerRec, gradient as skia_gradient,
    image_filters, images,
};

pub(crate) const MAX_EFFECT_BLUR_RADIUS: f32 = 96.0;
use mesh_core_elements::lru::LruCache;
use skia_safe::SamplingOptions;
use std::cell::RefCell;
use std::sync::Arc;

const SKIA_IMAGE_CACHE_CAPACITY: usize = 128;
const GRADIENT_SHADER_CACHE_CAPACITY: usize = 64;
type GradientShaderCacheKey = (u32, u32, i32, i32, i32, i32);

// Cached Skia image keyed by the raw pointer of the underlying
// `Arc<RgbaImage>` allocation. Holds a strong `Arc` reference in the value so
// the heap allocation cannot be freed and reallocated at the same address
// while the cache entry is live (which would silently return the wrong image).
struct CachedSkiaImage {
    _keep_alive: Arc<image::RgbaImage>,
    image: skia_safe::Image,
}

thread_local! {
    static SKIA_IMAGE_CACHE: RefCell<LruCache<usize, CachedSkiaImage>> =
        RefCell::new(LruCache::new(SKIA_IMAGE_CACHE_CAPACITY));

    // Cache for linear-gradient shaders keyed by (from_rgba, to_rgba, x, y, w, h).
    // A panel with a static background gradient reuses the same shader every frame.
    static GRADIENT_SHADER_CACHE: RefCell<LruCache<GradientShaderCacheKey, skia_safe::Shader>> =
        RefCell::new(LruCache::new(GRADIENT_SHADER_CACHE_CAPACITY));
}

#[allow(dead_code)]
pub(crate) trait PaintBackend: Send + Sync {
    fn id(&self) -> &'static str;

    fn capabilities(&self) -> PainterBackendCapabilities;

    fn execute_commands(
        &self,
        buffer: &mut PixelBuffer,
        commands: &[PainterCommand],
        diagnostics: &mut Vec<PainterDiagnostic>,
    );

    /// Execute commands within an open canvas session so multiple
    /// invocations within a single paint pass can share one
    /// `surfaces::wrap_pixels`. Default implementation falls back to
    /// `execute_commands` against the raw buffer (one wrap per call).
    fn execute_commands_in_session(
        &self,
        session: &mut PixelCanvasSession<'_>,
        commands: &[PainterCommand],
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) {
        session.with_buffer(|buffer| {
            self.execute_commands(buffer, commands, diagnostics);
        });
    }

    fn fill_rect(&self, buffer: &mut PixelBuffer, rect: ClipRect, color: Color, clip: ClipRect) {
        let mut diagnostics = Vec::new();
        self.execute_commands(
            buffer,
            &[PainterCommand::DrawRect {
                rect,
                paint: PainterPaint::fill(color),
                clip,
            }],
            &mut diagnostics,
        );
    }

    fn fill_rect_with_filter(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        color: Color,
        clip: ClipRect,
        filter: VisualFilter,
    ) {
        let mut diagnostics = Vec::new();
        self.execute_commands(
            buffer,
            &[PainterCommand::DrawRect {
                rect,
                paint: PainterPaint::fill(color).with_filter(filter),
                clip,
            }],
            &mut diagnostics,
        );
    }

    fn fill_rounded_rect(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        color: Color,
        clip: ClipRect,
    ) {
        let mut diagnostics = Vec::new();
        self.execute_commands(
            buffer,
            &[PainterCommand::DrawRoundedRect {
                rect,
                radius,
                paint: PainterPaint::fill(color),
                clip,
            }],
            &mut diagnostics,
        );
    }

    fn fill_rounded_rect_with_filter(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        color: Color,
        clip: ClipRect,
        filter: VisualFilter,
    ) {
        let mut diagnostics = Vec::new();
        self.execute_commands(
            buffer,
            &[PainterCommand::DrawRoundedRect {
                rect,
                radius,
                paint: PainterPaint::fill(color).with_filter(filter),
                clip,
            }],
            &mut diagnostics,
        );
    }

    fn stroke_rounded_rect(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        stroke_width: i32,
        color: Color,
        clip: ClipRect,
    ) -> bool {
        let mut diagnostics = Vec::new();
        self.execute_commands(
            buffer,
            &[PainterCommand::DrawRoundedRect {
                rect,
                radius,
                paint: PainterPaint::stroke(color, stroke_width as f32),
                clip,
            }],
            &mut diagnostics,
        );
        diagnostics.is_empty()
    }

    fn draw_box_shadow(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        shadow: BoxShadow,
        clip: ClipRect,
    ) {
        let mut diagnostics = Vec::new();
        self.execute_commands(
            buffer,
            &[PainterCommand::DrawShadow {
                rect,
                radius,
                shadow,
                clip,
            }],
            &mut diagnostics,
        );
    }

    fn apply_backdrop_filter(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        filter: VisualFilter,
        clip: ClipRect,
    ) {
        let mut diagnostics = Vec::new();
        self.execute_commands(
            buffer,
            &[PainterCommand::ApplyFilter {
                rect,
                radius,
                filter: PainterFilter::Backdrop(filter),
                clip,
            }],
            &mut diagnostics,
        );
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub(crate) enum PainterCommand {
    PushClip(PainterClip),
    PopClip,
    PushLayer(PainterLayer),
    PopLayer,
    DrawRect {
        rect: ClipRect,
        paint: PainterPaint,
        clip: ClipRect,
    },
    DrawRoundedRect {
        rect: ClipRect,
        radius: f32,
        paint: PainterPaint,
        clip: ClipRect,
    },
    DrawPath {
        path: PainterPath,
        paint: PainterPaint,
        clip: ClipRect,
    },
    DrawImage {
        image: PainterImage,
        rect: ClipRect,
        paint: PainterPaint,
        clip: ClipRect,
    },
    DrawLinearGradient {
        gradient: PainterLinearGradient,
        rect: ClipRect,
        radius: f32,
        clip: ClipRect,
    },
    DrawShadow {
        rect: ClipRect,
        radius: f32,
        shadow: BoxShadow,
        clip: ClipRect,
    },
    ApplyFilter {
        rect: ClipRect,
        radius: f32,
        filter: PainterFilter,
        clip: ClipRect,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PainterClip {
    pub rect: ClipRect,
    pub radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PainterLayer {
    pub bounds: ClipRect,
    pub opacity: f32,
    pub blend_mode: PainterBlendMode,
    pub filter: PainterFilter,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ActivePainterLayer {
    save_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PainterPaint {
    pub color: Color,
    pub style: PainterPaintStyle,
    pub blend_mode: PainterBlendMode,
    pub filter: VisualFilter,
}

impl PainterPaint {
    pub(crate) fn fill(color: Color) -> Self {
        Self {
            color,
            style: PainterPaintStyle::Fill,
            blend_mode: PainterBlendMode::SrcOver,
            filter: VisualFilter::NONE,
        }
    }

    pub(crate) fn stroke(color: Color, width: f32) -> Self {
        Self {
            color,
            style: PainterPaintStyle::Stroke(PainterStroke { width }),
            blend_mode: PainterBlendMode::SrcOver,
            filter: VisualFilter::NONE,
        }
    }

    pub(crate) fn with_filter(mut self, filter: VisualFilter) -> Self {
        self.filter = filter;
        self
    }

    pub(crate) fn with_blend_mode(mut self, blend_mode: PainterBlendMode) -> Self {
        self.blend_mode = blend_mode;
        self
    }
}

impl PainterBlendMode {
    /// Maps the element-model `mix-blend-mode` to the painter's blend mode.
    pub(crate) fn from_style(blend: mesh_core_elements::BlendMode) -> Self {
        match blend {
            mesh_core_elements::BlendMode::Normal => PainterBlendMode::SrcOver,
            mesh_core_elements::BlendMode::Multiply => PainterBlendMode::Multiply,
            mesh_core_elements::BlendMode::Screen => PainterBlendMode::Screen,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PainterPaintStyle {
    Fill,
    Stroke(PainterStroke),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PainterStroke {
    pub width: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PainterPath {
    pub elements: Vec<PainterPathElement>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub(crate) enum PainterPathElement {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo(f32, f32, f32, f32),
    CubicTo(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PainterImage {
    pub source: PainterImageSource,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PainterImageSource {
    Path(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PainterLinearGradient {
    pub from: Color,
    pub to: Color,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[allow(dead_code)]
pub(crate) enum PainterFilter {
    #[default]
    None,
    Blur(VisualFilter),
    Backdrop(VisualFilter),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PainterBlendMode {
    SrcOver,
    Multiply,
    Screen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PainterBackendCapabilities {
    pub backend_id: &'static str,
    pub clips: bool,
    pub layers: bool,
    pub rects: bool,
    pub rounded_rects: bool,
    pub paths: bool,
    pub text: bool,
    pub images: bool,
    pub shadows: bool,
    pub filters: bool,
    pub blend_modes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum UnsupportedPainterFeature {
    ClipStack,
    LayerStack,
    Path,
    Image,
    Gradient,
    Filter,
    BlendMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PainterDiagnosticSource {
    pub node_id: Option<mesh_core_elements::NodeId>,
    pub property: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PainterDiagnostic {
    pub backend_id: &'static str,
    pub feature: UnsupportedPainterFeature,
    pub message: String,
    pub source: Option<PainterDiagnosticSource>,
}

#[derive(Debug, Default)]
pub(crate) struct SkiaPaintBackend;

impl PaintBackend for SkiaPaintBackend {
    fn id(&self) -> &'static str {
        "skia"
    }

    fn capabilities(&self) -> PainterBackendCapabilities {
        PainterBackendCapabilities {
            backend_id: self.id(),
            clips: true,
            layers: true,
            rects: true,
            rounded_rects: true,
            paths: true,
            text: false,
            images: true,
            shadows: true,
            filters: true,
            blend_modes: true,
        }
    }

    fn execute_commands(
        &self,
        buffer: &mut PixelBuffer,
        commands: &[PainterCommand],
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) {
        let _ = buffer.with_skia_canvas(|canvas| {
            self.execute_commands_on_canvas(canvas, commands, diagnostics);
        });
    }

    fn execute_commands_in_session(
        &self,
        session: &mut PixelCanvasSession<'_>,
        commands: &[PainterCommand],
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) {
        let _ = session.with_canvas(|canvas| {
            self.execute_commands_on_canvas(canvas, commands, diagnostics);
        });
    }
}

impl SkiaPaintBackend {
    fn execute_commands_on_canvas(
        &self,
        canvas: &Canvas,
        commands: &[PainterCommand],
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) {
        let mut clip_stack: Vec<ClipRect> = Vec::with_capacity(commands.len().min(32));
        let mut layer_stack: Vec<ActivePainterLayer> = Vec::with_capacity(commands.len().min(8));
        for command in commands {
            match command {
                PainterCommand::PushClip(clip) => {
                    let effective = clip_stack
                        .last()
                        .copied()
                        .map(|current| intersect_clip(current, clip.rect))
                        .unwrap_or(clip.rect);
                    clip_stack.push(effective);
                }
                PainterCommand::PopClip => {
                    clip_stack.pop();
                }
                PainterCommand::PushLayer(layer) => {
                    if let PainterFilter::Blur(filter) = layer.filter
                        && self.diagnose_excessive_blur(filter, diagnostics)
                    {
                        continue;
                    }
                    if let Some(active) =
                        self.push_layer_command(canvas, *layer, clip_stack.last().copied())
                    {
                        layer_stack.push(active);
                    }
                }
                PainterCommand::PopLayer => {
                    if let Some(layer) = layer_stack.pop() {
                        canvas.restore_to_count(layer.save_count);
                    }
                }
                PainterCommand::DrawRect { rect, paint, clip } => {
                    let paint = *paint;
                    self.diagnose_unsupported_paint(paint, diagnostics);
                    self.draw_rect_command(
                        canvas,
                        *rect,
                        paint,
                        effective_clip(*clip, &clip_stack),
                    );
                }
                PainterCommand::DrawRoundedRect {
                    rect,
                    radius,
                    paint,
                    clip,
                } => {
                    let paint = *paint;
                    self.diagnose_unsupported_paint(paint, diagnostics);
                    self.draw_rounded_rect_command(
                        canvas,
                        *rect,
                        *radius,
                        paint,
                        effective_clip(*clip, &clip_stack),
                    );
                }
                PainterCommand::DrawPath { path, paint, clip } => {
                    let paint = *paint;
                    self.diagnose_unsupported_paint(paint, diagnostics);
                    self.draw_path_command(canvas, path, paint, effective_clip(*clip, &clip_stack));
                }
                PainterCommand::DrawImage {
                    image,
                    rect,
                    paint,
                    clip,
                } => {
                    let paint = *paint;
                    self.draw_image_command(
                        canvas,
                        image,
                        *rect,
                        paint,
                        effective_clip(*clip, &clip_stack),
                        diagnostics,
                    );
                }
                PainterCommand::DrawLinearGradient {
                    gradient,
                    rect,
                    radius,
                    clip,
                } => {
                    self.draw_linear_gradient_command(
                        canvas,
                        *gradient,
                        *rect,
                        *radius,
                        effective_clip(*clip, &clip_stack),
                    );
                }
                PainterCommand::DrawShadow {
                    rect,
                    radius,
                    shadow,
                    clip,
                } => {
                    if self.diagnose_excessive_blur(
                        VisualFilter {
                            blur_radius: shadow.blur_radius,
                        },
                        diagnostics,
                    ) {
                        continue;
                    }
                    self.draw_box_shadow_impl(
                        canvas,
                        *rect,
                        *radius,
                        *shadow,
                        effective_clip(*clip, &clip_stack),
                    );
                }
                PainterCommand::ApplyFilter {
                    rect,
                    radius,
                    filter,
                    clip,
                } => match filter {
                    PainterFilter::None => {}
                    PainterFilter::Blur(filter) => {
                        if !self.diagnose_excessive_blur(*filter, diagnostics) {
                            diagnostics.push(PainterDiagnostic {
                                backend_id: self.id(),
                                feature: UnsupportedPainterFeature::Filter,
                                message:
                                    "standalone blur filter commands are deferred to layer migration"
                                        .into(),
                                source: None,
                            });
                        }
                    }
                    PainterFilter::Backdrop(filter) => {
                        if self.diagnose_excessive_blur(*filter, diagnostics) {
                            continue;
                        }
                        self.apply_backdrop_filter_impl(
                            canvas,
                            *rect,
                            *radius,
                            *filter,
                            effective_clip(*clip, &clip_stack),
                        );
                    }
                },
            }
        }
    }

    fn push_layer_command(
        &self,
        canvas: &Canvas,
        layer: PainterLayer,
        current_clip: Option<ClipRect>,
    ) -> Option<ActivePainterLayer> {
        let bounds = current_clip
            .map(|clip| intersect_clip(layer.bounds, clip))
            .unwrap_or(layer.bounds);
        if bounds.width <= 0 || bounds.height <= 0 {
            return None;
        }

        let mut paint = skia_safe::Paint::default();
        paint.set_alpha_f(layer.opacity.clamp(0.0, 1.0));
        paint.set_blend_mode(blend_mode_to_skia(layer.blend_mode));

        if let PainterFilter::Blur(filter) = layer.filter
            && filter.blur_radius > 0.0
            && let Some(image_filter) = image_filters::blur(
                (
                    blur_radius_to_sigma(filter.blur_radius),
                    blur_radius_to_sigma(filter.blur_radius),
                ),
                Some(TileMode::Decal),
                None,
                None,
            )
        {
            paint.set_image_filter(image_filter);
        }

        let bounds = Rect::from_xywh(
            bounds.x as f32,
            bounds.y as f32,
            bounds.width as f32,
            bounds.height as f32,
        );
        let save_count = canvas.save_layer(&SaveLayerRec::default().bounds(&bounds).paint(&paint));
        Some(ActivePainterLayer { save_count })
    }

    fn diagnose_unsupported_paint(
        &self,
        paint: PainterPaint,
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) {
        self.diagnose_excessive_blur(paint.filter, diagnostics);
    }

    fn diagnose_excessive_blur(
        &self,
        filter: VisualFilter,
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) -> bool {
        if filter.blur_radius <= MAX_EFFECT_BLUR_RADIUS {
            return false;
        }
        diagnostics.push(PainterDiagnostic {
            backend_id: self.id(),
            feature: UnsupportedPainterFeature::Filter,
            message: format!(
                "excessive blur radius {} exceeds max {}",
                filter.blur_radius, MAX_EFFECT_BLUR_RADIUS
            ),
            source: None,
        });
        true
    }

    fn fill_rect_impl(&self, canvas: &Canvas, rect: ClipRect, color: Color, clip: ClipRect) {
        let clipped = intersect_clip(rect, clip);
        if clipped.width <= 0 || clipped.height <= 0 {
            return;
        }
        let save_count = canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(
                clipped.x as f32,
                clipped.y as f32,
                clipped.width as f32,
                clipped.height as f32,
            ),
            None,
            false,
        );
        let rect = Rect::from_xywh(
            rect.x as f32,
            rect.y as f32,
            rect.width as f32,
            rect.height as f32,
        );
        let mut paint = skia_paint(color, false);
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rect(rect, &paint);
        canvas.restore_to_count(save_count);
    }

    fn fill_rounded_rect_impl(
        &self,
        canvas: &Canvas,
        rect: ClipRect,
        radius: f32,
        color: Color,
        clip: ClipRect,
    ) {
        let clipped = intersect_clip(rect, clip);
        if clipped.width <= 0 || clipped.height <= 0 {
            return;
        }

        let half_w = (rect.width.max(0) as f32) * 0.5;
        let half_h = (rect.height.max(0) as f32) * 0.5;
        let radius = radius.max(0.0).min(half_w).min(half_h);

        if radius < 0.5 {
            self.fill_rect_impl(canvas, rect, color, clip);
            return;
        }

        let save_count = canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(
                clipped.x as f32,
                clipped.y as f32,
                clipped.width as f32,
                clipped.height as f32,
            ),
            None,
            false,
        );
        let rect = Rect::from_xywh(
            rect.x as f32,
            rect.y as f32,
            rect.width as f32,
            rect.height as f32,
        );
        let mut paint = skia_paint(color, true);
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(RRect::new_rect_xy(rect, radius, radius), &paint);
        canvas.restore_to_count(save_count);
    }

    fn stroke_rounded_rect_impl(
        &self,
        canvas: &Canvas,
        rect: ClipRect,
        radius: f32,
        stroke_width: i32,
        color: Color,
        clip: ClipRect,
    ) {
        if stroke_width <= 0 {
            return;
        }

        let clipped = intersect_clip(rect, clip);
        if clipped.width <= 0 || clipped.height <= 0 {
            return;
        }

        let half_w = (rect.width.max(0) as f32) * 0.5;
        let half_h = (rect.height.max(0) as f32) * 0.5;
        let radius = radius.max(0.0).min(half_w).min(half_h);
        if radius < 0.5 {
            self.stroke_rect_impl(canvas, rect, stroke_width, color, clip);
            return;
        }

        let stroke_width = stroke_width as f32;
        let save_count = canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(
                clipped.x as f32,
                clipped.y as f32,
                clipped.width as f32,
                clipped.height as f32,
            ),
            None,
            false,
        );
        let inset = stroke_width * 0.5;
        let stroke_w = (rect.width as f32 - stroke_width).max(0.0);
        let stroke_h = (rect.height as f32 - stroke_width).max(0.0);
        if stroke_w > 0.0 && stroke_h > 0.0 {
            let rect = Rect::from_xywh(
                rect.x as f32 + inset,
                rect.y as f32 + inset,
                stroke_w,
                stroke_h,
            );
            let radius = (radius - inset)
                .max(0.0)
                .min(stroke_w * 0.5)
                .min(stroke_h * 0.5);
            let mut paint = skia_paint(color, true);
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(stroke_width);
            canvas.draw_rrect(RRect::new_rect_xy(rect, radius, radius), &paint);
        }
        canvas.restore_to_count(save_count);
    }

    fn draw_box_shadow_impl(
        &self,
        canvas: &Canvas,
        rect: ClipRect,
        radius: f32,
        shadow: BoxShadow,
        clip: ClipRect,
    ) {
        if shadow.is_none() || shadow.inset {
            return;
        }

        let shadow_rect = ClipRect {
            x: (rect.x as f32 + shadow.offset_x - shadow.spread_radius).round() as i32,
            y: (rect.y as f32 + shadow.offset_y - shadow.spread_radius).round() as i32,
            width: (rect.width as f32 + shadow.spread_radius * 2.0)
                .round()
                .max(0.0) as i32,
            height: (rect.height as f32 + shadow.spread_radius * 2.0)
                .round()
                .max(0.0) as i32,
        };
        let blur_pad = (shadow.blur_radius * 3.0).ceil() as i32;
        let shadow_bounds = ClipRect {
            x: shadow_rect.x - blur_pad,
            y: shadow_rect.y - blur_pad,
            width: shadow_rect.width + blur_pad * 2,
            height: shadow_rect.height + blur_pad * 2,
        };
        let clipped = intersect_clip(shadow_bounds, clip);
        if clipped.width <= 0
            || clipped.height <= 0
            || shadow_rect.width <= 0
            || shadow_rect.height <= 0
        {
            return;
        }

        if shadow.blur_radius <= 0.0 && radius <= 0.5 {
            let save_count = canvas.save();
            canvas.clip_rect(
                Rect::from_xywh(
                    clipped.x as f32,
                    clipped.y as f32,
                    clipped.width as f32,
                    clipped.height as f32,
                ),
                None,
                false,
            );
            let rect = Rect::from_xywh(
                shadow_rect.x as f32,
                shadow_rect.y as f32,
                shadow_rect.width as f32,
                shadow_rect.height as f32,
            );
            let mut paint = skia_paint(shadow.color, false);
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rect(rect, &paint);
            canvas.restore_to_count(save_count);
            return;
        }

        let skia_clip = (clipped.x, clipped.y, clipped.width, clipped.height);
        let save_count = canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(
                skia_clip.0 as f32,
                skia_clip.1 as f32,
                skia_clip.2 as f32,
                skia_clip.3 as f32,
            ),
            None,
            false,
        );
        let rect = Rect::from_xywh(
            shadow_rect.x as f32,
            shadow_rect.y as f32,
            shadow_rect.width as f32,
            shadow_rect.height as f32,
        );
        let mut paint = skia_paint(shadow.color, true);
        paint.set_style(PaintStyle::Fill);
        if shadow.blur_radius > 0.0 {
            paint.set_mask_filter(MaskFilter::blur(
                BlurStyle::Normal,
                blur_radius_to_sigma(shadow.blur_radius),
                Some(false),
            ));
        }
        let radius = (radius + shadow.spread_radius).max(0.0);
        if radius > 0.5 {
            canvas.draw_rrect(RRect::new_rect_xy(rect, radius, radius), &paint);
        } else {
            canvas.draw_rect(rect, &paint);
        }
        canvas.restore_to_count(save_count);
    }

    fn apply_backdrop_filter_impl(
        &self,
        canvas: &Canvas,
        rect: ClipRect,
        radius: f32,
        filter: VisualFilter,
        clip: ClipRect,
    ) {
        if filter.is_none() {
            return;
        }
        let blur_pad = (filter.blur_radius * 3.0).ceil() as i32;
        let paint_bounds = ClipRect {
            x: rect.x - blur_pad,
            y: rect.y - blur_pad,
            width: rect.width + blur_pad * 2,
            height: rect.height + blur_pad * 2,
        };
        let clipped = intersect_clip(paint_bounds, clip);
        if clipped.width <= 0 || clipped.height <= 0 {
            return;
        }
        let Some(backdrop) = image_filters::blur(
            (
                blur_radius_to_sigma(filter.blur_radius),
                blur_radius_to_sigma(filter.blur_radius),
            ),
            Some(TileMode::Decal),
            None,
            None,
        ) else {
            return;
        };
        let save_count = canvas.save();
        let rect = Rect::from_xywh(
            rect.x as f32,
            rect.y as f32,
            rect.width as f32,
            rect.height as f32,
        );
        if radius > 0.5 {
            canvas.clip_rrect(RRect::new_rect_xy(rect, radius, radius), None, true);
        } else {
            canvas.clip_rect(rect, None, false);
        }
        let layer_bounds = Rect::from_xywh(
            clipped.x as f32,
            clipped.y as f32,
            clipped.width as f32,
            clipped.height as f32,
        );
        let rec = SaveLayerRec::default()
            .bounds(&layer_bounds)
            .backdrop(&backdrop)
            .backdrop_tile_mode(TileMode::Decal);
        let layer_count = canvas.save_layer(&rec);
        canvas.restore_to_count(layer_count);
        canvas.restore_to_count(save_count);
    }

    /// Runs `draw` directly for the default `SrcOver` mode, or inside an
    /// isolated `save_layer` whose compositing blend mode is `blend` otherwise.
    /// A `save_layer` draws `draw` into an offscreen and composites the result
    /// onto the backdrop with the requested blend mode (the correct semantics
    /// for `mix-blend-mode`), so callers don't have to thread the mode through
    /// every fill/stroke primitive.
    fn draw_with_blend<F: FnOnce(&Self, &Canvas)>(
        &self,
        canvas: &Canvas,
        blend: PainterBlendMode,
        bounds: ClipRect,
        clip: ClipRect,
        draw: F,
    ) {
        if blend == PainterBlendMode::SrcOver {
            draw(self, canvas);
            return;
        }
        let region = intersect_clip(bounds, clip);
        if region.width <= 0 || region.height <= 0 {
            return;
        }
        let mut paint = skia_safe::Paint::default();
        paint.set_blend_mode(blend_mode_to_skia(blend));
        let rect = Rect::from_xywh(
            region.x as f32,
            region.y as f32,
            region.width as f32,
            region.height as f32,
        );
        let save_count = canvas.save_layer(&SaveLayerRec::default().bounds(&rect).paint(&paint));
        draw(self, canvas);
        canvas.restore_to_count(save_count);
    }

    fn draw_rect_command(
        &self,
        canvas: &Canvas,
        rect: ClipRect,
        paint: PainterPaint,
        clip: ClipRect,
    ) {
        self.draw_with_blend(
            canvas,
            paint.blend_mode,
            rect,
            clip,
            |this, canvas| match paint.style {
                PainterPaintStyle::Fill => {
                    this.fill_shape(canvas, rect, 0.0, paint.color, clip, paint.filter)
                }
                PainterPaintStyle::Stroke(stroke) => {
                    this.stroke_rect_impl(
                        canvas,
                        rect,
                        stroke.width.round() as i32,
                        paint.color,
                        clip,
                    );
                }
            },
        );
    }

    fn draw_rounded_rect_command(
        &self,
        canvas: &Canvas,
        rect: ClipRect,
        radius: f32,
        paint: PainterPaint,
        clip: ClipRect,
    ) {
        self.draw_with_blend(
            canvas,
            paint.blend_mode,
            rect,
            clip,
            |this, canvas| match paint.style {
                PainterPaintStyle::Fill => {
                    this.fill_shape(canvas, rect, radius, paint.color, clip, paint.filter)
                }
                PainterPaintStyle::Stroke(stroke) => {
                    this.stroke_rounded_rect_impl(
                        canvas,
                        rect,
                        radius,
                        stroke.width.round() as i32,
                        paint.color,
                        clip,
                    );
                }
            },
        );
    }

    fn draw_path_command(
        &self,
        canvas: &Canvas,
        path: &PainterPath,
        paint: PainterPaint,
        clip: ClipRect,
    ) {
        let Some(path) = skia_path(path) else {
            return;
        };
        if clip.width <= 0 || clip.height <= 0 {
            return;
        }
        let save_count = canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(
                clip.x as f32,
                clip.y as f32,
                clip.width as f32,
                clip.height as f32,
            ),
            None,
            true,
        );
        let mut skia_paint = skia_paint(paint.color, true);
        skia_paint.set_blend_mode(blend_mode_to_skia(paint.blend_mode));
        match paint.style {
            PainterPaintStyle::Fill => {
                skia_paint.set_style(PaintStyle::Fill);
            }
            PainterPaintStyle::Stroke(stroke) => {
                skia_paint.set_style(PaintStyle::Stroke);
                skia_paint.set_stroke_width(stroke.width.max(0.0));
            }
        }
        canvas.draw_path(&path, &skia_paint);
        canvas.restore_to_count(save_count);
    }

    fn draw_linear_gradient_command(
        &self,
        canvas: &Canvas,
        gradient: PainterLinearGradient,
        rect: ClipRect,
        radius: f32,
        clip: ClipRect,
    ) {
        let clipped = intersect_clip(rect, clip);
        if clipped.width <= 0 || clipped.height <= 0 {
            return;
        }
        let from_rgba = u32::from_be_bytes([
            gradient.from.r,
            gradient.from.g,
            gradient.from.b,
            gradient.from.a,
        ]);
        let to_rgba =
            u32::from_be_bytes([gradient.to.r, gradient.to.g, gradient.to.b, gradient.to.a]);
        let grad_cache_key = (from_rgba, to_rgba, rect.x, rect.y, rect.width, rect.height);
        let save_count = canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(
                clipped.x as f32,
                clipped.y as f32,
                clipped.width as f32,
                clipped.height as f32,
            ),
            None,
            false,
        );
        let rect = Rect::from_xywh(
            rect.x as f32,
            rect.y as f32,
            rect.width as f32,
            rect.height as f32,
        );
        let cache_key = grad_cache_key;
        let cached_shader = GRADIENT_SHADER_CACHE.with(|c| c.borrow_mut().get(&cache_key).cloned());
        let shader = if let Some(s) = cached_shader {
            s
        } else {
            let colors = [
                Color4f::from(crate::surface::buffer::skia_color(gradient.from)),
                Color4f::from(crate::surface::buffer::skia_color(gradient.to)),
            ];
            let gradient_colors =
                skia_gradient::Colors::new_evenly_spaced(colors.as_slice(), TileMode::Clamp, None);
            let shader_gradient = skia_gradient::Gradient::new(
                gradient_colors,
                skia_gradient::Interpolation::default(),
            );
            let Some(new_shader) = skia_gradient::shaders::linear_gradient(
                (
                    Point::new(rect.left(), rect.top()),
                    Point::new(rect.left(), rect.bottom()),
                ),
                &shader_gradient,
                None,
            ) else {
                canvas.restore_to_count(save_count);
                return;
            };
            GRADIENT_SHADER_CACHE.with(|c| c.borrow_mut().insert(cache_key, new_shader.clone()));
            new_shader
        };
        let mut paint = skia_safe::Paint::default();
        paint.set_anti_alias(true);
        paint.set_shader(shader);
        if radius > 0.5 {
            canvas.draw_rrect(RRect::new_rect_xy(rect, radius, radius), &paint);
        } else {
            canvas.draw_rect(rect, &paint);
        }
        canvas.restore_to_count(save_count);
    }

    fn draw_image_command(
        &self,
        canvas: &Canvas,
        image: &PainterImage,
        rect: ClipRect,
        paint: PainterPaint,
        clip: ClipRect,
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) {
        let clipped = intersect_clip(rect, clip);
        if clipped.width <= 0 || clipped.height <= 0 {
            return;
        }
        let PainterImageSource::Path(path) = &image.source;
        let Some(rgba) = icon::load_image_rgba(std::path::Path::new(path)) else {
            diagnostics.push(PainterDiagnostic {
                backend_id: self.id(),
                feature: UnsupportedPainterFeature::Image,
                message: format!("missing image asset '{path}'"),
                source: None,
            });
            return;
        };
        // Use the Arc pointer as cache key: same allocation == same pixel data,
        // so we can skip `Data::new_copy` and re-use the cached Skia image.
        // The cache value holds a strong Arc reference so the heap allocation
        // cannot be freed and re-used at the same address while cached.
        let arc_ptr = Arc::as_ptr(&rgba) as usize;
        let skia_image = SKIA_IMAGE_CACHE.with(|cache| {
            let mut map = cache.borrow_mut();
            if let Some(entry) = map.get(&arc_ptr) {
                return Some(entry.image.clone());
            }
            let info = ImageInfo::new(
                (rgba.width() as i32, rgba.height() as i32),
                skia_safe::ColorType::RGBA8888,
                skia_safe::AlphaType::Unpremul,
                None,
            );
            let data = Data::new_copy(rgba.as_raw());
            let img = images::raster_from_data(&info, data, (rgba.width() * 4) as usize)?;
            map.insert(
                arc_ptr,
                CachedSkiaImage {
                    _keep_alive: Arc::clone(&rgba),
                    image: img.clone(),
                },
            );
            Some(img)
        });
        let Some(skia_image) = skia_image else {
            diagnostics.push(PainterDiagnostic {
                backend_id: self.id(),
                feature: UnsupportedPainterFeature::Image,
                message: format!("could not decode image source '{path}'"),
                source: None,
            });
            return;
        };
        let save_count = canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(
                clipped.x as f32,
                clipped.y as f32,
                clipped.width as f32,
                clipped.height as f32,
            ),
            None,
            false,
        );
        let mut skia_paint = skia_paint(paint.color, true);
        skia_paint.set_alpha(paint.color.a);
        let dst = Rect::from_xywh(
            rect.x as f32,
            rect.y as f32,
            rect.width as f32,
            rect.height as f32,
        );
        canvas.draw_image_rect_with_sampling_options(
            skia_image,
            None,
            dst,
            SamplingOptions::default(),
            &skia_paint,
        );
        canvas.restore_to_count(save_count);
    }

    fn stroke_rect_impl(
        &self,
        canvas: &Canvas,
        rect: ClipRect,
        stroke_width: i32,
        color: Color,
        clip: ClipRect,
    ) {
        if stroke_width <= 0 {
            return;
        }
        let clipped = intersect_clip(rect, clip);
        if clipped.width <= 0 || clipped.height <= 0 {
            return;
        }
        let stroke_width = stroke_width.min(rect.width.max(0)).min(rect.height.max(0)) as f32;
        let inset = stroke_width * 0.5;
        let save_count = canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(
                clipped.x as f32,
                clipped.y as f32,
                clipped.width as f32,
                clipped.height as f32,
            ),
            None,
            false,
        );
        let rect = Rect::from_xywh(
            rect.x as f32 + inset,
            rect.y as f32 + inset,
            (rect.width as f32 - stroke_width).max(0.0),
            (rect.height as f32 - stroke_width).max(0.0),
        );
        let mut paint = skia_paint(color, false);
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(stroke_width);
        canvas.draw_rect(rect, &paint);
        canvas.restore_to_count(save_count);
    }

    fn fill_shape(
        &self,
        canvas: &Canvas,
        rect: ClipRect,
        radius: f32,
        color: Color,
        clip: ClipRect,
        filter: VisualFilter,
    ) {
        if filter.is_none() {
            if radius > 0.5 {
                self.fill_rounded_rect_impl(canvas, rect, radius, color, clip);
            } else {
                self.fill_rect_impl(canvas, rect, color, clip);
            }
            return;
        }

        let blur_pad = (filter.blur_radius * 3.0).ceil() as i32;
        let paint_bounds = ClipRect {
            x: rect.x - blur_pad,
            y: rect.y - blur_pad,
            width: rect.width + blur_pad * 2,
            height: rect.height + blur_pad * 2,
        };
        let clipped = intersect_clip(paint_bounds, clip);
        if clipped.width <= 0 || clipped.height <= 0 {
            return;
        }
        let save_count = canvas.save();
        canvas.clip_rect(
            Rect::from_xywh(
                clipped.x as f32,
                clipped.y as f32,
                clipped.width as f32,
                clipped.height as f32,
            ),
            None,
            false,
        );
        let mut paint = skia_paint(color, true);
        paint.set_style(PaintStyle::Fill);
        paint.set_mask_filter(MaskFilter::blur(
            BlurStyle::Normal,
            blur_radius_to_sigma(filter.blur_radius),
            Some(false),
        ));
        let rect = Rect::from_xywh(
            rect.x as f32,
            rect.y as f32,
            rect.width as f32,
            rect.height as f32,
        );
        if radius > 0.5 {
            canvas.draw_rrect(RRect::new_rect_xy(rect, radius, radius), &paint);
        } else {
            canvas.draw_rect(rect, &paint);
        }
        canvas.restore_to_count(save_count);
    }
}

fn effective_clip(clip: ClipRect, clip_stack: &[ClipRect]) -> ClipRect {
    clip_stack
        .last()
        .copied()
        .map(|current| intersect_clip(clip, current))
        .unwrap_or(clip)
}

fn skia_path(path: &PainterPath) -> Option<SkiaPath> {
    let mut builder = PathBuilder::new();
    for element in &path.elements {
        match *element {
            PainterPathElement::MoveTo(x, y) => {
                builder.move_to((x, y));
            }
            PainterPathElement::LineTo(x, y) => {
                builder.line_to((x, y));
            }
            PainterPathElement::QuadTo(x1, y1, x2, y2) => {
                builder.quad_to((x1, y1), (x2, y2));
            }
            PainterPathElement::CubicTo(x1, y1, x2, y2, x3, y3) => {
                builder.cubic_to((x1, y1), (x2, y2), (x3, y3));
            }
            PainterPathElement::Close => {
                builder.close();
            }
        }
    }
    (!path.elements.is_empty()).then(|| builder.detach())
}

fn blend_mode_to_skia(blend: PainterBlendMode) -> skia_safe::BlendMode {
    match blend {
        PainterBlendMode::SrcOver => skia_safe::BlendMode::SrcOver,
        PainterBlendMode::Multiply => skia_safe::BlendMode::Multiply,
        PainterBlendMode::Screen => skia_safe::BlendMode::Screen,
    }
}

fn blur_radius_to_sigma(radius: f32) -> f32 {
    (radius.max(0.0) * 0.57735 + 0.5).max(0.01)
}

fn skia_paint(color: Color, anti_alias: bool) -> skia_safe::Paint {
    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(anti_alias);
    paint.set_color(crate::surface::buffer::skia_color(color));
    paint.set_blend_mode(skia_safe::BlendMode::SrcOver);
    paint
}
