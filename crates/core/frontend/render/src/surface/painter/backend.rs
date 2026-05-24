use super::*;
use crate::surface::icon;
use crate::surface::painter::geometry::rounded_rect_coverage;
use mesh_core_elements::{BoxShadow, VisualFilter};
use skia_safe::{
    BlurStyle, Color4f, Data, ImageInfo, MaskFilter, PaintStyle, Path as SkiaPath, PathBuilder,
    Point, RRect, Rect, TileMode, canvas::SaveLayerRec, gradient as skia_gradient, image_filters,
    images,
};

pub(crate) const MAX_EFFECT_BLUR_RADIUS: f32 = 96.0;
use skia_safe::SamplingOptions;

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
    DrawText {
        text: String,
        x: f32,
        y: f32,
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
#[allow(dead_code)]
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

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub(crate) enum PainterFilter {
    None,
    Blur(VisualFilter),
    Backdrop(VisualFilter),
}

impl Default for PainterFilter {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
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
    Text,
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
        let mut clip_stack: Vec<PainterClip> = Vec::new();
        let mut layer_stack: Vec<PainterLayer> = Vec::new();
        for command in commands {
            match command {
                PainterCommand::PushClip(clip) => {
                    clip_stack.push(*clip);
                }
                PainterCommand::PopClip => {
                    clip_stack.pop();
                }
                PainterCommand::PushLayer(layer) => {
                    if layer.blend_mode != PainterBlendMode::SrcOver {
                        diagnostics.push(PainterDiagnostic {
                            backend_id: self.id(),
                            feature: UnsupportedPainterFeature::BlendMode,
                            message: "non-SrcOver layer blend modes are not supported".into(),
                            source: None,
                        });
                    }
                    if let PainterFilter::Blur(filter) = layer.filter
                        && self.diagnose_excessive_blur(filter, diagnostics)
                    {
                        continue;
                    }
                    layer_stack.push(*layer);
                }
                PainterCommand::PopLayer => {
                    layer_stack.pop();
                }
                PainterCommand::DrawRect { rect, paint, clip } => {
                    let paint = layer_paint(*paint, &layer_stack);
                    self.diagnose_unsupported_paint(paint, diagnostics);
                    self.draw_rect_command(
                        buffer,
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
                    let paint = layer_paint(*paint, &layer_stack);
                    self.diagnose_unsupported_paint(paint, diagnostics);
                    self.draw_rounded_rect_command(
                        buffer,
                        *rect,
                        *radius,
                        paint,
                        effective_clip(*clip, &clip_stack),
                    );
                }
                PainterCommand::DrawPath { path, paint, clip } => {
                    let paint = layer_paint(*paint, &layer_stack);
                    self.diagnose_unsupported_paint(paint, diagnostics);
                    self.draw_path_command(buffer, path, paint, effective_clip(*clip, &clip_stack));
                }
                PainterCommand::DrawText { .. } => diagnostics.push(PainterDiagnostic {
                    backend_id: self.id(),
                    feature: UnsupportedPainterFeature::Text,
                    message:
                        "text commands are part of the contract but still handled by TextRenderer"
                            .into(),
                    source: None,
                }),
                PainterCommand::DrawImage {
                    image,
                    rect,
                    paint,
                    clip,
                } => {
                    let paint = layer_paint(*paint, &layer_stack);
                    self.draw_image_command(
                        buffer,
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
                        buffer,
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
                        buffer,
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
                            buffer,
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
}

impl SkiaPaintBackend {
    fn diagnose_unsupported_paint(
        &self,
        paint: PainterPaint,
        diagnostics: &mut Vec<PainterDiagnostic>,
    ) {
        if paint.blend_mode != PainterBlendMode::SrcOver {
            diagnostics.push(PainterDiagnostic {
                backend_id: self.id(),
                feature: UnsupportedPainterFeature::BlendMode,
                message: "non-SrcOver blend modes are deferred to blend-mode migration".into(),
                source: None,
            });
        }
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

    fn fill_rect_impl(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        color: Color,
        clip: ClipRect,
    ) {
        let clipped = intersect_clip(rect, clip);
        if clipped.width <= 0 || clipped.height <= 0 {
            return;
        }
        buffer.with_skia_canvas(|canvas| {
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
        });
    }

    fn fill_rounded_rect_impl(
        &self,
        buffer: &mut PixelBuffer,
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
            self.fill_rect_impl(buffer, rect, color, clip);
            return;
        }

        if buffer.fill_rounded_rect_clipped(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            radius,
            color,
            (clipped.x, clipped.y, clipped.width, clipped.height),
        ) {
            return;
        }

        for py in clipped.y..clipped.y + clipped.height {
            for px in clipped.x..clipped.x + clipped.width {
                let coverage =
                    rounded_rect_coverage(rect, radius, px as f32 + 0.5, py as f32 + 0.5);
                if coverage <= 0.0 {
                    continue;
                }
                buffer.blend_pixel_f32(px as u32, py as u32, color, coverage);
            }
        }
    }

    fn stroke_rounded_rect_impl(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        stroke_width: i32,
        color: Color,
        clip: ClipRect,
    ) -> bool {
        if stroke_width <= 0 {
            return false;
        }

        let clipped = intersect_clip(rect, clip);
        if clipped.width <= 0 || clipped.height <= 0 {
            return true;
        }

        let half_w = (rect.width.max(0) as f32) * 0.5;
        let half_h = (rect.height.max(0) as f32) * 0.5;
        let radius = radius.max(0.0).min(half_w).min(half_h);
        if radius < 0.5 {
            self.stroke_rect_impl(buffer, rect, stroke_width, color, clip);
            return true;
        }

        buffer.stroke_rounded_rect_clipped(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            radius,
            stroke_width as f32,
            color,
            (clipped.x, clipped.y, clipped.width, clipped.height),
        )
    }

    fn draw_box_shadow_impl(
        &self,
        buffer: &mut PixelBuffer,
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
            buffer.clear_rect(
                clipped.x.max(0) as u32,
                clipped.y.max(0) as u32,
                clipped.width as u32,
                clipped.height as u32,
                shadow.color,
            );
            return;
        }

        let skia_clip = (clipped.x, clipped.y, clipped.width, clipped.height);
        buffer.with_skia_canvas(|canvas| {
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
        });
    }

    fn apply_backdrop_filter_impl(
        &self,
        buffer: &mut PixelBuffer,
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
        buffer.with_skia_canvas(|canvas| {
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
        });
    }

    fn draw_rect_command(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        paint: PainterPaint,
        clip: ClipRect,
    ) {
        match paint.style {
            PainterPaintStyle::Fill => {
                self.fill_shape(buffer, rect, 0.0, paint.color, clip, paint.filter)
            }
            PainterPaintStyle::Stroke(stroke) => {
                self.stroke_rect_impl(buffer, rect, stroke.width.round() as i32, paint.color, clip);
            }
        }
    }

    fn draw_rounded_rect_command(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        paint: PainterPaint,
        clip: ClipRect,
    ) {
        match paint.style {
            PainterPaintStyle::Fill => {
                self.fill_shape(buffer, rect, radius, paint.color, clip, paint.filter)
            }
            PainterPaintStyle::Stroke(stroke) => {
                self.stroke_rounded_rect_impl(
                    buffer,
                    rect,
                    radius,
                    stroke.width.round() as i32,
                    paint.color,
                    clip,
                );
            }
        }
    }

    fn draw_path_command(
        &self,
        buffer: &mut PixelBuffer,
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
        buffer.with_skia_canvas(|canvas| {
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
        });
    }

    fn draw_linear_gradient_command(
        &self,
        buffer: &mut PixelBuffer,
        gradient: PainterLinearGradient,
        rect: ClipRect,
        radius: f32,
        clip: ClipRect,
    ) {
        let clipped = intersect_clip(rect, clip);
        if clipped.width <= 0 || clipped.height <= 0 {
            return;
        }
        buffer.with_skia_canvas(|canvas| {
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
            let Some(shader) = skia_gradient::shaders::linear_gradient(
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
            let mut paint = skia_safe::Paint::default();
            paint.set_anti_alias(true);
            paint.set_shader(shader);
            if radius > 0.5 {
                canvas.draw_rrect(RRect::new_rect_xy(rect, radius, radius), &paint);
            } else {
                canvas.draw_rect(rect, &paint);
            }
            canvas.restore_to_count(save_count);
        });
    }

    fn draw_image_command(
        &self,
        buffer: &mut PixelBuffer,
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
        let info = ImageInfo::new(
            (rgba.width() as i32, rgba.height() as i32),
            skia_safe::ColorType::RGBA8888,
            skia_safe::AlphaType::Unpremul,
            None,
        );
        let data = Data::new_copy(rgba.as_raw());
        let Some(skia_image) = images::raster_from_data(&info, data, (rgba.width() * 4) as usize)
        else {
            diagnostics.push(PainterDiagnostic {
                backend_id: self.id(),
                feature: UnsupportedPainterFeature::Image,
                message: format!("could not decode image source '{path}'"),
                source: None,
            });
            return;
        };
        buffer.with_skia_canvas(|canvas| {
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
        });
    }

    fn stroke_rect_impl(
        &self,
        buffer: &mut PixelBuffer,
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
        buffer.with_skia_canvas(|canvas| {
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
        });
    }

    fn fill_shape(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        color: Color,
        clip: ClipRect,
        filter: VisualFilter,
    ) {
        if filter.is_none() {
            if radius > 0.5 {
                self.fill_rounded_rect_impl(buffer, rect, radius, color, clip);
            } else {
                self.fill_rect_impl(buffer, rect, color, clip);
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
        buffer.with_skia_canvas(|canvas| {
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
        });
    }
}

fn effective_clip(clip: ClipRect, clip_stack: &[PainterClip]) -> ClipRect {
    clip_stack
        .iter()
        .fold(clip, |clip, pushed| intersect_clip(clip, pushed.rect))
}

fn layer_paint(mut paint: PainterPaint, layer_stack: &[PainterLayer]) -> PainterPaint {
    for layer in layer_stack {
        let alpha = ((paint.color.a as f32) * layer.opacity.clamp(0.0, 1.0))
            .round()
            .clamp(0.0, 255.0) as u8;
        paint.color.a = alpha;
        if let PainterFilter::Blur(filter) = layer.filter
            && paint.filter.is_none()
        {
            paint.filter = filter;
        }
    }
    paint
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
