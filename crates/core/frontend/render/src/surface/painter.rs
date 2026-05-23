mod backend;
mod geometry;
mod text;
mod tree;
mod widgets;

use std::sync::Mutex;

use super::PixelBuffer;
use super::icon;
use super::text::{TextCacheMetrics, TextRenderer, TextSelectionGeometry};
#[allow(unused_imports)]
pub(crate) use backend::{
    MAX_EFFECT_BLUR_RADIUS, PaintBackend, PainterBackendCapabilities, PainterBlendMode,
    PainterClip, PainterCommand, PainterDiagnostic, PainterDiagnosticSource, PainterFilter,
    PainterImage, PainterImageSource, PainterLayer, PainterLinearGradient, PainterPaint,
    PainterPaintStyle, PainterPath, PainterPathElement, PainterStroke, SkiaPaintBackend,
    UnsupportedPainterFeature,
};
use mesh_core_elements::style::{
    BackgroundPaint, Color, Display, Overflow, TextAlign, TextDirection, TextOverflow,
};
use mesh_core_elements::tree::WidgetNode;
use mesh_core_elements::{BoxShadow, VisualFilter};

pub(crate) use geometry::ClipRect;
use geometry::{
    clip_to_tuple, dim_color, intersect_clip, node_attr_f32, node_clips_children, opacity_color,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PainterBackendSnapshot {
    pub backend_id: &'static str,
    pub rollback_authority: &'static str,
    pub capabilities: Vec<PainterCapabilitySnapshot>,
    pub recent_diagnostics: Vec<PainterDiagnosticSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PainterCapabilitySnapshot {
    pub feature: &'static str,
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PainterDiagnosticSnapshot {
    pub backend_id: &'static str,
    pub feature: &'static str,
    pub message: String,
    pub node_id: Option<mesh_core_elements::NodeId>,
    pub property: Option<String>,
}

pub struct FrontendRenderEngine {
    paint_backend: Box<dyn PaintBackend>,
    painter_diagnostics: Mutex<Vec<PainterDiagnostic>>,
    text_renderer: TextRenderer,
}

impl FrontendRenderEngine {
    pub fn new() -> Self {
        Self {
            paint_backend: Box::<SkiaPaintBackend>::default(),
            painter_diagnostics: Mutex::new(Vec::new()),
            text_renderer: TextRenderer::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn with_paint_backend(paint_backend: Box<dyn PaintBackend>) -> Self {
        Self {
            paint_backend,
            painter_diagnostics: Mutex::new(Vec::new()),
            text_renderer: TextRenderer::new(),
        }
    }

    pub fn paint_backend_id(&self) -> &'static str {
        self.paint_backend.id()
    }

    pub fn paint_backend_snapshot(&self) -> PainterBackendSnapshot {
        PainterBackendSnapshot {
            backend_id: self.paint_backend.id(),
            rollback_authority: crate::renderer_library_rollback_authority(),
            capabilities: painter_capability_snapshots(self.paint_backend.capabilities()),
            recent_diagnostics: self.painter_diagnostic_snapshots(),
        }
    }

    pub fn painter_diagnostic_snapshots(&self) -> Vec<PainterDiagnosticSnapshot> {
        self.painter_diagnostics()
            .into_iter()
            .map(painter_diagnostic_snapshot)
            .collect()
    }

    #[allow(dead_code)]
    pub(crate) fn paint_backend_capabilities(&self) -> PainterBackendCapabilities {
        self.paint_backend.capabilities()
    }

    #[allow(dead_code)]
    pub(crate) fn painter_diagnostics(&self) -> Vec<PainterDiagnostic> {
        self.painter_diagnostics
            .lock()
            .map(|diagnostics| diagnostics.clone())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub(crate) fn clear_painter_diagnostics(&self) {
        if let Ok(mut diagnostics) = self.painter_diagnostics.lock() {
            diagnostics.clear();
        }
    }

    fn execute_painter_commands(&self, buffer: &mut PixelBuffer, commands: &[PainterCommand]) {
        let mut local = Vec::new();
        self.paint_backend
            .execute_commands(buffer, commands, &mut local);
        if !local.is_empty()
            && let Ok(mut diagnostics) = self.painter_diagnostics.lock()
        {
            diagnostics.extend(local);
        }
    }

    pub(super) fn fill_rect_clipped(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        color: Color,
        clip: ClipRect,
    ) {
        self.execute_painter_commands(
            buffer,
            &[PainterCommand::DrawRect {
                rect,
                paint: PainterPaint::fill(color),
                clip,
            }],
        );
    }

    pub(super) fn fill_rounded_rect_clipped(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        color: Color,
        clip: ClipRect,
    ) {
        self.execute_painter_commands(
            buffer,
            &[PainterCommand::DrawRoundedRect {
                rect,
                radius,
                paint: PainterPaint::fill(color),
                clip,
            }],
        );
    }

    pub(super) fn fill_rect_clipped_with_filter(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        color: Color,
        clip: ClipRect,
        filter: VisualFilter,
    ) {
        self.execute_painter_commands(
            buffer,
            &[PainterCommand::DrawRect {
                rect,
                paint: PainterPaint::fill(color).with_filter(filter),
                clip,
            }],
        );
    }

    pub(super) fn fill_rounded_rect_clipped_with_filter(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        color: Color,
        clip: ClipRect,
        filter: VisualFilter,
    ) {
        self.execute_painter_commands(
            buffer,
            &[PainterCommand::DrawRoundedRect {
                rect,
                radius,
                paint: PainterPaint::fill(color).with_filter(filter),
                clip,
            }],
        );
    }

    pub(super) fn stroke_rounded_rect_clipped(
        &self,
        buffer: &mut PixelBuffer,
        rect: ClipRect,
        radius: f32,
        stroke_width: i32,
        color: Color,
        clip: ClipRect,
    ) -> bool {
        let before = self.painter_diagnostics().len();
        self.execute_painter_commands(
            buffer,
            &[PainterCommand::DrawRoundedRect {
                rect,
                radius,
                paint: PainterPaint::stroke(color, stroke_width as f32),
                clip,
            }],
        );
        self.painter_diagnostics().len() == before
    }

    pub(super) fn draw_box_shadow(
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
        self.execute_painter_commands(
            buffer,
            &[PainterCommand::DrawShadow {
                rect,
                radius,
                shadow,
                clip,
            }],
        );
    }

    pub(super) fn apply_backdrop_filter(
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
        self.execute_painter_commands(
            buffer,
            &[PainterCommand::ApplyFilter {
                rect,
                radius,
                filter: PainterFilter::Backdrop(filter),
                clip,
            }],
        );
    }

    pub(super) fn draw_background_paint(
        &self,
        buffer: &mut PixelBuffer,
        paint: &BackgroundPaint,
        rect: ClipRect,
        radius: f32,
        clip: ClipRect,
    ) {
        let command = match paint {
            BackgroundPaint::None => return,
            BackgroundPaint::Image(source) => PainterCommand::DrawImage {
                image: PainterImage {
                    source: PainterImageSource::Path(source.path.clone()),
                },
                rect,
                paint: PainterPaint::fill(Color::WHITE),
                clip,
            },
            BackgroundPaint::LinearGradient(gradient) => PainterCommand::DrawLinearGradient {
                gradient: PainterLinearGradient {
                    from: gradient.from,
                    to: gradient.to,
                },
                rect,
                radius,
                clip,
            },
        };
        self.execute_painter_commands(buffer, &[command]);
    }

    pub fn reset_text_cache_metrics(&self) {
        self.text_renderer.reset_cache_metrics();
    }

    pub fn text_cache_metrics(&self) -> TextCacheMetrics {
        self.text_renderer.cache_metrics()
    }
}

fn painter_capability_snapshots(
    capabilities: PainterBackendCapabilities,
) -> Vec<PainterCapabilitySnapshot> {
    vec![
        PainterCapabilitySnapshot {
            feature: "clips",
            supported: capabilities.clips,
        },
        PainterCapabilitySnapshot {
            feature: "layers",
            supported: capabilities.layers,
        },
        PainterCapabilitySnapshot {
            feature: "rects",
            supported: capabilities.rects,
        },
        PainterCapabilitySnapshot {
            feature: "rounded_rects",
            supported: capabilities.rounded_rects,
        },
        PainterCapabilitySnapshot {
            feature: "paths",
            supported: capabilities.paths,
        },
        PainterCapabilitySnapshot {
            feature: "text",
            supported: capabilities.text,
        },
        PainterCapabilitySnapshot {
            feature: "images",
            supported: capabilities.images,
        },
        PainterCapabilitySnapshot {
            feature: "shadows",
            supported: capabilities.shadows,
        },
        PainterCapabilitySnapshot {
            feature: "filters",
            supported: capabilities.filters,
        },
        PainterCapabilitySnapshot {
            feature: "blend_modes",
            supported: capabilities.blend_modes,
        },
    ]
}

fn painter_diagnostic_snapshot(diagnostic: PainterDiagnostic) -> PainterDiagnosticSnapshot {
    PainterDiagnosticSnapshot {
        backend_id: diagnostic.backend_id,
        feature: unsupported_painter_feature_label(diagnostic.feature),
        message: diagnostic.message,
        node_id: diagnostic.source.as_ref().and_then(|source| source.node_id),
        property: diagnostic
            .source
            .as_ref()
            .and_then(|source| source.property.clone()),
    }
}

fn unsupported_painter_feature_label(feature: UnsupportedPainterFeature) -> &'static str {
    match feature {
        UnsupportedPainterFeature::ClipStack => "clip_stack",
        UnsupportedPainterFeature::LayerStack => "layer_stack",
        UnsupportedPainterFeature::Path => "path",
        UnsupportedPainterFeature::Text => "text",
        UnsupportedPainterFeature::Image => "image",
        UnsupportedPainterFeature::Gradient => "gradient",
        UnsupportedPainterFeature::Filter => "filter",
        UnsupportedPainterFeature::BlendMode => "blend_mode",
    }
}

impl Default for FrontendRenderEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
