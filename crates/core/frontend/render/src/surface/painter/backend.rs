use super::*;
use crate::surface::icon;
use mesh_core_elements::{BoxShadow, VisualFilter};
use skia_safe::{
    BlurStyle, Canvas, Color4f, Data, ImageInfo, MaskFilter, PaintStyle, Path as SkiaPath,
    PathBuilder, Point, RRect, Rect, TileMode, canvas::SaveLayerRec, gradient as skia_gradient,
    image_filters, images,
};
use smallvec::SmallVec;

use mesh_core_elements::lru::LruCache;
use skia_safe::SamplingOptions;
use std::cell::RefCell;
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) const MAX_EFFECT_BLUR_RADIUS: f32 = 96.0;

/// The one lever that measurably changes blur cost is how many times the
/// kernel runs. Pre-resampling the layer is *not* one: Skia's raster blur
/// already downsamples for wide kernels, so an explicit resample chain only
/// adds passes (~2x slower; see `.planning/log/performance-log.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlurQuality {
    /// Each pass runs at `sigma / sqrt(passes)`, so more passes buy a smoother
    /// falloff, not a wider blur. Clamped to `1..=MAX_BLUR_PASSES`.
    pub passes: u8,
    /// Larger radii are dropped with a diagnostic rather than rasterized.
    pub max_radius: f32,
}

/// Beyond three passes the visual difference stops being measurable.
pub const MAX_BLUR_PASSES: u8 = 3;

impl Default for BlurQuality {
    fn default() -> Self {
        Self {
            passes: 1,
            max_radius: MAX_EFFECT_BLUR_RADIUS,
        }
    }
}

impl BlurQuality {
    pub(crate) fn resolved_passes(self) -> u8 {
        self.passes.clamp(1, MAX_BLUR_PASSES)
    }
}

const SKIA_IMAGE_CACHE_CAPACITY: usize = 128;
const GRADIENT_SHADER_CACHE_CAPACITY: usize = 64;
type GradientShaderCacheKey = (u32, u32, i32, i32);
#[cfg(test)]
static GRADIENT_SHADER_CREATIONS: AtomicUsize = AtomicUsize::new(0);

// Keyed by the `Arc<RgbaImage>` allocation's address, so the value holds a
// strong reference — otherwise a freed allocation could be reused at the same
// address and silently return the wrong image.
struct CachedSkiaImage {
    _keep_alive: Arc<image::RgbaImage>,
    image: skia_safe::Image,
}

thread_local! {
    static SKIA_IMAGE_CACHE: RefCell<LruCache<usize, CachedSkiaImage>> =
        RefCell::new(LruCache::new(SKIA_IMAGE_CACHE_CAPACITY));

    // Keyed by (from_rgba, to_rgba, w, h) rather than position, so a moving
    // same-sized gradient reuses its shader instead of churning the cache.
    static GRADIENT_SHADER_CACHE: RefCell<LruCache<GradientShaderCacheKey, skia_safe::Shader>> =
        RefCell::new(LruCache::new(GRADIENT_SHADER_CACHE_CAPACITY));
}

#[cfg(test)]
pub(super) fn reset_gradient_shader_cache_for_tests() {
    GRADIENT_SHADER_CACHE.with(|cache| cache.borrow_mut().clear());
    GRADIENT_SHADER_CREATIONS.store(0, Ordering::Relaxed);
}

#[cfg(test)]
pub(super) fn gradient_shader_creations_for_tests() -> usize {
    GRADIENT_SHADER_CREATIONS.load(Ordering::Relaxed)
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

    /// Lets several invocations in one paint pass share a single
    /// `surfaces::wrap_pixels`. Defaults to one wrap per call.
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

    /// The layer stack outlives this call: a filtered subtree opens a layer in
    /// one command buffer and closes it in a later one, so open layers cannot
    /// live in a local of the execute loop the way clips do.
    fn execute_commands_in_session_with_layers(
        &self,
        session: &mut PixelCanvasSession<'_>,
        commands: &[PainterCommand],
        _layers: &mut PainterLayerStack,
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) {
        self.execute_commands_in_session(session, commands, diagnostics);
    }

    /// Restore the canvas to its state before the outermost unbalanced push.
    fn close_open_layers(
        &self,
        _session: &mut PixelCanvasSession<'_>,
        layers: &mut PainterLayerStack,
    ) {
        layers.clear();
    }

    /// For the immediate painter, which rasterizes a filtered subtree into its
    /// own buffer instead of opening a canvas layer.
    fn composite_blurred_buffer(
        &self,
        _buffer: &mut PixelBuffer,
        _source: &PixelBuffer,
        _at: (i32, i32),
        _filter: VisualFilter,
        _quality: BlurQuality,
        _clip: ClipRect,
    ) {
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
    pub blur_quality: BlurQuality,
}

impl PainterLayer {
    /// An unfiltered layer used for opacity/blend isolation.
    pub(crate) fn isolated(bounds: ClipRect, opacity: f32, blend_mode: PainterBlendMode) -> Self {
        Self {
            bounds,
            opacity,
            blend_mode,
            filter: PainterFilter::None,
            blur_quality: BlurQuality::default(),
        }
    }

    /// A layer that blurs everything drawn into it before compositing.
    pub(crate) fn blurred(
        bounds: ClipRect,
        filter: VisualFilter,
        blur_quality: BlurQuality,
    ) -> Self {
        Self {
            bounds,
            opacity: 1.0,
            blend_mode: PainterBlendMode::SrcOver,
            filter: PainterFilter::Blur(filter),
            blur_quality,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ActivePainterLayer {
    /// `None` when the backend declined the push; still recorded so the
    /// matching pop stays balanced.
    save_count: Option<usize>,
}

/// Layers opened by `PushLayer` and not yet closed. Owned by the render engine
/// for a whole paint pass, since a filtered subtree spans many command buffers.
#[derive(Debug, Default)]
pub(crate) struct PainterLayerStack {
    layers: SmallVec<[ActivePainterLayer; 4]>,
}

impl PainterLayerStack {
    pub(crate) fn depth(&self) -> usize {
        self.layers.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    fn push(&mut self, layer: ActivePainterLayer) {
        self.layers.push(layer);
    }

    fn pop(&mut self) -> Option<ActivePainterLayer> {
        self.layers.pop()
    }

    /// Restoring to this count closes every open layer at once.
    fn outermost_save_count(&self) -> Option<usize> {
        self.layers.iter().find_map(|layer| layer.save_count)
    }

    fn clear(&mut self) {
        self.layers.clear();
    }
}

/// Each nesting level is an offscreen allocation plus a blur pass over it, so
/// a runaway nest is a frame-time cliff rather than a visual improvement.
pub(crate) const MAX_BLUR_LAYER_DEPTH: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PainterPaint {
    pub color: Color,
    pub style: PainterPaintStyle,
    pub blend_mode: PainterBlendMode,
}

impl PainterPaint {
    pub(crate) fn fill(color: Color) -> Self {
        Self {
            color,
            style: PainterPaintStyle::Fill,
            blend_mode: PainterBlendMode::SrcOver,
        }
    }

    pub(crate) fn stroke(color: Color, width: f32) -> Self {
        Self {
            color,
            style: PainterPaintStyle::Stroke(PainterStroke { width }),
            blend_mode: PainterBlendMode::SrcOver,
        }
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
        let mut layers = PainterLayerStack::default();
        let _ = buffer.with_skia_canvas(|canvas| {
            self.execute_commands_on_canvas(canvas, commands, &mut layers, diagnostics);
            close_open_layers_on_canvas(canvas, &mut layers);
        });
    }

    fn execute_commands_in_session(
        &self,
        session: &mut PixelCanvasSession<'_>,
        commands: &[PainterCommand],
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) {
        let mut layers = PainterLayerStack::default();
        let _ = session.with_canvas(|canvas| {
            self.execute_commands_on_canvas(canvas, commands, &mut layers, diagnostics);
            close_open_layers_on_canvas(canvas, &mut layers);
        });
    }

    fn execute_commands_in_session_with_layers(
        &self,
        session: &mut PixelCanvasSession<'_>,
        commands: &[PainterCommand],
        layers: &mut PainterLayerStack,
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) {
        let _ = session.with_canvas(|canvas| {
            self.execute_commands_on_canvas(canvas, commands, layers, diagnostics);
        });
    }

    fn close_open_layers(
        &self,
        session: &mut PixelCanvasSession<'_>,
        layers: &mut PainterLayerStack,
    ) {
        if layers.is_empty() {
            return;
        }
        let _ = session.with_canvas(|canvas| {
            close_open_layers_on_canvas(canvas, layers);
        });
    }

    fn composite_blurred_buffer(
        &self,
        buffer: &mut PixelBuffer,
        source: &PixelBuffer,
        at: (i32, i32),
        filter: VisualFilter,
        quality: BlurQuality,
        clip: ClipRect,
    ) {
        if source.width() == 0 || source.height() == 0 || clip.width <= 0 || clip.height <= 0 {
            return;
        }
        let info = ImageInfo::new(
            (source.width() as i32, source.height() as i32),
            skia_safe::ColorType::BGRA8888,
            skia_safe::AlphaType::Premul,
            None,
        );
        let Some(image) = images::raster_from_data(
            &info,
            Data::new_copy(source.data()),
            source.stride() as usize,
        ) else {
            return;
        };
        let mut paint = skia_safe::Paint::default();
        if let Some(image_filter) = blur_image_filter(filter.blur_radius, quality) {
            paint.set_image_filter(image_filter);
        }
        let _ = buffer.with_skia_canvas(|canvas| {
            let save_count = canvas.save();
            canvas.clip_rect(
                Rect::from_xywh(
                    clip.x as f32,
                    clip.y as f32,
                    clip.width as f32,
                    clip.height as f32,
                ),
                None,
                false,
            );
            canvas.draw_image_with_sampling_options(
                &image,
                (at.0 as f32, at.1 as f32),
                SamplingOptions::from(skia_safe::FilterMode::Linear),
                Some(&paint),
            );
            canvas.restore_to_count(save_count);
        });
    }
}

fn close_open_layers_on_canvas(canvas: &Canvas, layers: &mut PainterLayerStack) {
    if let Some(save_count) = layers.outermost_save_count() {
        canvas.restore_to_count(save_count);
    }
    layers.clear();
}

impl SkiaPaintBackend {
    fn execute_commands_on_canvas(
        &self,
        canvas: &Canvas,
        commands: &[PainterCommand],
        layer_stack: &mut PainterLayerStack,
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) {
        let mut clip_stack: SmallVec<[ClipRect; 8]> = SmallVec::new();
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
                    let blur_radius = match layer.filter {
                        PainterFilter::Blur(filter) => filter.blur_radius,
                        _ => 0.0,
                    };
                    let over_budget = matches!(layer.filter, PainterFilter::Blur(filter)
                    if self.diagnose_blur_over_budget(
                        filter,
                        layer.blur_quality.max_radius,
                        diagnostics,
                    ));
                    let too_deep = blur_radius > 0.0 && layer_stack.depth() >= MAX_BLUR_LAYER_DEPTH;
                    if too_deep {
                        diagnostics.push(PainterDiagnostic {
                            backend_id: self.id(),
                            feature: UnsupportedPainterFeature::Filter,
                            message: format!(
                                "blur layers nested deeper than {MAX_BLUR_LAYER_DEPTH}; \
                                 painting subtree unblurred"
                            ),
                            source: None,
                        });
                    }
                    let layer = if over_budget || too_deep {
                        PainterLayer {
                            filter: PainterFilter::None,
                            ..*layer
                        }
                    } else {
                        *layer
                    };
                    layer_stack.push(self.push_layer_command(
                        canvas,
                        layer,
                        clip_stack.last().copied(),
                    ));
                }
                PainterCommand::PopLayer => {
                    if let Some(layer) = layer_stack.pop()
                        && let Some(save_count) = layer.save_count
                    {
                        canvas.restore_to_count(save_count);
                    }
                }
                PainterCommand::DrawRect { rect, paint, clip } => {
                    let paint = *paint;
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
                    // PushLayer/PopLayer around the whole subtree is the only
                    // lowering that blurs descendants, not just the node.
                    PainterFilter::Blur(filter) => {
                        diagnostics.push(PainterDiagnostic {
                            backend_id: self.id(),
                            feature: UnsupportedPainterFeature::Filter,
                            message: format!(
                                "blur filter of radius {} must be lowered into a layer scope",
                                filter.blur_radius
                            ),
                            source: None,
                        });
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
    ) -> ActivePainterLayer {
        let bounds = current_clip
            .map(|clip| intersect_clip(layer.bounds, clip))
            .unwrap_or(layer.bounds);
        if bounds.width <= 0 || bounds.height <= 0 {
            return ActivePainterLayer { save_count: None };
        }

        let mut paint = skia_safe::Paint::default();
        paint.set_alpha_f(layer.opacity.clamp(0.0, 1.0));
        paint.set_blend_mode(blend_mode_to_skia(layer.blend_mode));

        if let PainterFilter::Blur(filter) = layer.filter
            && let Some(image_filter) = blur_image_filter(filter.blur_radius, layer.blur_quality)
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
        ActivePainterLayer {
            save_count: Some(save_count),
        }
    }

    fn diagnose_excessive_blur(
        &self,
        filter: VisualFilter,
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) -> bool {
        self.diagnose_blur_over_budget(filter, MAX_EFFECT_BLUR_RADIUS, diagnostics)
    }

    /// Reports and rejects a blur wider than the caller's budget. Filtered
    /// layers carry the user's configured cap; shadows and backdrops use the
    /// built-in one.
    fn diagnose_blur_over_budget(
        &self,
        filter: VisualFilter,
        max_radius: f32,
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) -> bool {
        if filter.blur_radius <= max_radius {
            return false;
        }
        diagnostics.push(PainterDiagnostic {
            backend_id: self.id(),
            feature: UnsupportedPainterFeature::Filter,
            message: format!(
                "excessive blur radius {} exceeds max {max_radius}",
                filter.blur_radius
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
        // The backdrop save_layer snapshots and redraws the full canvas clip
        // (its bounds rec is an allocation hint, not a clip), so the effective
        // clip must be applied to the canvas or a damage-clipped replay would
        // re-blur and rewrite pixels outside the cleared damage region.
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
                PainterPaintStyle::Fill => this.fill_shape(canvas, rect, 0.0, paint.color, clip),
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
                PainterPaintStyle::Fill => this.fill_shape(canvas, rect, radius, paint.color, clip),
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
        let grad_cache_key = (from_rgba, to_rgba, rect.width, rect.height);
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
        canvas.translate((rect.x as f32, rect.y as f32));
        let rect = Rect::from_xywh(0.0, 0.0, rect.width as f32, rect.height as f32);
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
                (Point::new(0.0, 0.0), Point::new(0.0, rect.height())),
                &shader_gradient,
                None,
            ) else {
                canvas.restore_to_count(save_count);
                return;
            };
            #[cfg(test)]
            GRADIENT_SHADER_CREATIONS.fetch_add(1, Ordering::Relaxed);
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
    ) {
        if radius > 0.5 {
            self.fill_rounded_rect_impl(canvas, rect, radius, color, clip);
        } else {
            self.fill_rect_impl(canvas, rect, color, clip);
        }
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

/// Builds the image filter that realizes `radius` under `quality`.
///
/// `passes` blurs of `sigma / sqrt(passes)` compose to one blur of `sigma`,
/// which is what keeps a quality change from also changing how blurred the
/// result looks.
fn blur_image_filter(radius: f32, quality: BlurQuality) -> Option<skia_safe::ImageFilter> {
    if radius <= 0.0 {
        return None;
    }
    let passes = quality.resolved_passes();
    let sigma = blur_radius_to_sigma(radius) / f32::from(passes).sqrt();
    let mut filter = None;
    for _ in 0..passes {
        filter = image_filters::blur((sigma, sigma), Some(TileMode::Decal), filter, None);
        filter.as_ref()?;
    }
    filter
}

fn skia_paint(color: Color, anti_alias: bool) -> skia_safe::Paint {
    let mut paint = skia_safe::Paint::default();
    paint.set_anti_alias(anti_alias);
    paint.set_color(crate::surface::buffer::skia_color(color));
    paint.set_blend_mode(skia_safe::BlendMode::SrcOver);
    paint
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn painter_clip_and_layer_stacks_stay_inline_for_common_depths() {
        let mut clip_stack: SmallVec<[ClipRect; 8]> = SmallVec::new();
        let mut layer_stack: SmallVec<[ActivePainterLayer; 4]> = SmallVec::new();

        for index in 0..8 {
            clip_stack.push(ClipRect {
                x: index,
                y: index,
                width: 100,
                height: 50,
            });
        }
        for save_count in 0..4 {
            layer_stack.push(ActivePainterLayer {
                save_count: Some(save_count),
            });
        }

        assert!(!clip_stack.spilled());
        assert!(!layer_stack.spilled());
    }

    // cargo test -p mesh-core-render --release -- painter_stack_smallvec_beats_per_batch_vec_allocation --ignored --nocapture
    #[test]
    #[ignore = "release-only painter stack allocation microbenchmark"]
    fn painter_stack_smallvec_beats_per_batch_vec_allocation() {
        let iterations = 8_000_000;
        let clip = ClipRect {
            x: 0,
            y: 0,
            width: 120,
            height: 64,
        };

        let old_started = std::time::Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let mut clip_stack: Vec<ClipRect> = Vec::with_capacity(8);
            let mut layer_stack: Vec<ActivePainterLayer> = Vec::with_capacity(4);
            for _ in 0..4 {
                clip_stack.push(clip);
            }
            for save_count in 0..2 {
                layer_stack.push(ActivePainterLayer {
                    save_count: Some(save_count),
                });
            }
            old_total += clip_stack.len() + layer_stack.len();
            layer_stack.pop();
            clip_stack.pop();
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let mut clip_stack: SmallVec<[ClipRect; 8]> = SmallVec::new();
            let mut layer_stack: SmallVec<[ActivePainterLayer; 4]> = SmallVec::new();
            for _ in 0..4 {
                clip_stack.push(clip);
            }
            for save_count in 0..2 {
                layer_stack.push(ActivePainterLayer {
                    save_count: Some(save_count),
                });
            }
            new_total += clip_stack.len() + layer_stack.len();
            layer_stack.pop();
            clip_stack.pop();
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "painter stacks: per-batch Vec {old_time:?}; SmallVec {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
    }
}
