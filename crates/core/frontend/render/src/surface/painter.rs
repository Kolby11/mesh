mod backend;
mod geometry;
mod text;
mod tree;
mod widgets;

use std::cell::{Cell, RefCell};
use std::sync::Mutex;

use super::icon;
use super::text::{SharedTextMeasurer, TextCacheMetrics, TextRenderer, TextSelectionGeometry};
use super::{PixelBuffer, PixelCanvasSession};
#[allow(unused_imports)]
pub(crate) use backend::{
    MAX_EFFECT_BLUR_RADIUS, PaintBackend, PainterBackendCapabilities, PainterBlendMode,
    PainterClip, PainterCommand, PainterDiagnostic, PainterDiagnosticSource, PainterFilter,
    PainterImage, PainterImageSource, PainterLayer, PainterLinearGradient, PainterPaint,
    PainterPaintStyle, PainterPath, PainterPathElement, PainterStroke, SkiaPaintBackend,
    UnsupportedPainterFeature,
};
use mesh_core_elements::style::{
    BackgroundPaint, Color, Display, Overflow, Position, TextAlign, TextDirection, TextOverflow,
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

#[derive(Debug, Clone, Copy)]
pub struct TooltipPaintColors {
    pub background: Color,
    pub border: Color,
    pub foreground: Color,
}

impl TooltipPaintColors {
    pub const DEFAULT_DARK: Self = Self {
        background: Color {
            r: 0x32,
            g: 0x30,
            b: 0x2f,
            a: 0xff,
        },
        border: Color {
            r: 0x50,
            g: 0x49,
            b: 0x45,
            a: 0xff,
        },
        foreground: Color {
            r: 0xeb,
            g: 0xdb,
            b: 0xb2,
            a: 0xff,
        },
    };
}

/// Where a tooltip should appear relative to the hovered element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TooltipAnchor {
    /// Centered below the element (default)
    #[default]
    BottomCenter,
    /// Centered above the element
    TopCenter,
    /// Centered to the left of the element
    LeftCenter,
    /// Centered to the right of the element
    RightCenter,
    /// Offset from cursor (legacy behavior: cursor + (14, 18))
    CursorBottomRight,
    /// Follows the cursor as it moves within the element
    CursorFollow,
}

/// Pre-computed positioning, sizing, and animation state for a tooltip frame.
#[derive(Debug, Clone)]
pub struct TooltipRenderState {
    pub text: String,
    pub anchor: TooltipAnchor,
    /// Bounding box of the hovered element in surface coordinates: (left, top, right, bottom).
    pub element_bounds: Option<(f32, f32, f32, f32)>,
    /// Cursor position in surface coordinates.
    pub cursor_x: f32,
    pub cursor_y: f32,
    /// Final pixel position of the tooltip top-left corner after positioning and flipping.
    pub paint_x: i32,
    pub paint_y: i32,
    /// Measured width and height of the tooltip box in pixels.
    pub box_w: i32,
    pub box_h: i32,
    /// Opacity for fade-in animation (0.0 = invisible, 1.0 = fully opaque).
    pub opacity: f32,
}

pub struct FrontendRenderEngine {
    paint_backend: Box<dyn PaintBackend>,
    painter_diagnostics: Mutex<Vec<PainterDiagnostic>>,
    text_renderer: SharedTextMeasurer,
    tooltip_colors: Cell<TooltipPaintColors>,
    tooltip_opacity: Cell<f32>,
    /// When true, `paint_x` passed to `render_tooltip` is the horizontal
    /// center of the tooltip box rather than its left edge.
    tooltip_center_x: Cell<bool>,
    /// Starting scale factor for the `"expand"` animation (0.0 = no scale).
    /// The box is rendered at `scale_from + (1.0 - scale_from) * opacity` of
    /// its full size, anchored at the element-closest edge.
    tooltip_scale_from: Cell<f32>,
    render_scratch: RefCell<RenderScratch>,
    /// Full-surface clip set at the start of each render pass. Used to give
    /// `position: fixed` children the viewport clip rather than their parent's.
    viewport_clip: Cell<ClipRect>,
}

#[derive(Default)]
struct RenderScratch {
    batched_commands: Vec<PainterCommand>,
    node_commands: Vec<PainterCommand>,
}

const MAX_RETAINED_BATCH_COMMANDS: usize = 4096;
const MAX_RETAINED_NODE_COMMANDS: usize = 16;
const DEFAULT_NODE_COMMANDS: usize = 5;

impl RenderScratch {
    fn prepare(&mut self, batch_capacity: usize) {
        self.batched_commands.clear();
        self.node_commands.clear();
        if self.batched_commands.capacity() > MAX_RETAINED_BATCH_COMMANDS
            && batch_capacity <= MAX_RETAINED_BATCH_COMMANDS
        {
            self.batched_commands = Vec::with_capacity(batch_capacity);
        }
        if self.batched_commands.capacity() < batch_capacity {
            self.batched_commands
                .reserve(batch_capacity - self.batched_commands.capacity());
        }
        if self.node_commands.capacity() > MAX_RETAINED_NODE_COMMANDS {
            self.node_commands = Vec::with_capacity(DEFAULT_NODE_COMMANDS);
        }
        if self.node_commands.capacity() < DEFAULT_NODE_COMMANDS {
            self.node_commands
                .reserve(DEFAULT_NODE_COMMANDS - self.node_commands.capacity());
        }
    }
}

impl FrontendRenderEngine {
    pub fn new() -> Self {
        Self {
            paint_backend: Box::<SkiaPaintBackend>::default(),
            painter_diagnostics: Mutex::new(Vec::new()),
            text_renderer: SharedTextMeasurer,
            tooltip_colors: Cell::new(TooltipPaintColors::DEFAULT_DARK),
            tooltip_opacity: Cell::new(1.0),
            tooltip_center_x: Cell::new(false),
            tooltip_scale_from: Cell::new(0.0),
            render_scratch: RefCell::new(RenderScratch::default()),
            viewport_clip: Cell::new(ClipRect { x: 0, y: 0, width: 0, height: 0 }),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn with_paint_backend(paint_backend: Box<dyn PaintBackend>) -> Self {
        Self {
            paint_backend,
            painter_diagnostics: Mutex::new(Vec::new()),
            text_renderer: SharedTextMeasurer,
            tooltip_colors: Cell::new(TooltipPaintColors::DEFAULT_DARK),
            tooltip_opacity: Cell::new(1.0),
            tooltip_center_x: Cell::new(false),
            tooltip_scale_from: Cell::new(0.0),
            render_scratch: RefCell::new(RenderScratch::default()),
            viewport_clip: Cell::new(ClipRect { x: 0, y: 0, width: 0, height: 0 }),
        }
    }

    pub fn set_tooltip_colors(&self, colors: TooltipPaintColors) {
        self.tooltip_colors.set(colors);
    }

    pub fn set_tooltip_opacity(&self, opacity: f32) {
        self.tooltip_opacity.set(opacity.clamp(0.0, 1.0));
    }

    pub fn set_tooltip_center_x(&self, centered: bool) {
        self.tooltip_center_x.set(centered);
    }

    pub fn set_tooltip_scale_from(&self, scale_from: f32) {
        self.tooltip_scale_from.set(scale_from.clamp(0.0, 1.0));
    }

    pub(super) fn tooltip_colors(&self) -> TooltipPaintColors {
        self.tooltip_colors.get()
    }

    pub(super) fn tooltip_opacity(&self) -> f32 {
        self.tooltip_opacity.get()
    }

    pub(super) fn tooltip_center_x(&self) -> bool {
        self.tooltip_center_x.get()
    }

    pub(super) fn tooltip_scale_from(&self) -> f32 {
        self.tooltip_scale_from.get()
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
        if commands.is_empty() {
            return;
        }
        let mut local = Vec::new();
        self.paint_backend
            .execute_commands(buffer, commands, &mut local);
        if !local.is_empty()
            && let Ok(mut diagnostics) = self.painter_diagnostics.lock()
        {
            diagnostics.extend(local);
        }
    }

    pub(super) fn execute_painter_commands_in_session(
        &self,
        session: &mut PixelCanvasSession<'_>,
        commands: &[PainterCommand],
    ) {
        if commands.is_empty() {
            return;
        }
        let mut local = Vec::new();
        self.paint_backend
            .execute_commands_in_session(session, commands, &mut local);
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

    pub(super) fn fill_rect_clipped_in_session(
        &self,
        session: &mut PixelCanvasSession<'_>,
        rect: ClipRect,
        color: Color,
        clip: ClipRect,
    ) {
        self.execute_painter_commands_in_session(
            session,
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

    pub(super) fn fill_rounded_rect_clipped_in_session(
        &self,
        session: &mut PixelCanvasSession<'_>,
        rect: ClipRect,
        radius: f32,
        color: Color,
        clip: ClipRect,
    ) {
        self.execute_painter_commands_in_session(
            session,
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
        _buffer: &mut PixelBuffer,
        _rect: ClipRect,
        _radius: f32,
        filter: VisualFilter,
        _clip: ClipRect,
    ) {
        // CPU software blur removed per BLUR-03.
        // Compositor blur is handled by org_kde_kwin_blur protocol
        // (see crates/core/presentation). The backdrop_filter data
        // continues to flow through the display list for region
        // computation even though we don't render it on the CPU.
        if filter.is_none() {
            return;
        }
        // No-op: blur is offloaded to compositor or rendered flat.
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

    pub(super) fn stroke_rounded_rect_clipped_in_session(
        &self,
        session: &mut PixelCanvasSession<'_>,
        rect: ClipRect,
        radius: f32,
        stroke_width: i32,
        color: Color,
        clip: ClipRect,
    ) -> bool {
        let before = self.painter_diagnostics().len();
        self.execute_painter_commands_in_session(
            session,
            &[PainterCommand::DrawRoundedRect {
                rect,
                radius,
                paint: PainterPaint::stroke(color, stroke_width as f32),
                clip,
            }],
        );
        self.painter_diagnostics().len() == before
    }

    pub(super) fn draw_box_shadow_in_session(
        &self,
        session: &mut PixelCanvasSession<'_>,
        rect: ClipRect,
        radius: f32,
        shadow: BoxShadow,
        clip: ClipRect,
    ) {
        if shadow.is_none() || shadow.inset {
            return;
        }
        self.execute_painter_commands_in_session(
            session,
            &[PainterCommand::DrawShadow {
                rect,
                radius,
                shadow,
                clip,
            }],
        );
    }

    pub(super) fn apply_backdrop_filter_in_session(
        &self,
        _session: &mut PixelCanvasSession<'_>,
        _rect: ClipRect,
        _radius: f32,
        filter: VisualFilter,
        _clip: ClipRect,
    ) {
        // CPU software blur removed per BLUR-03.
        // See apply_backdrop_filter comment above.
        if filter.is_none() {
            return;
        }
        // No-op: blur is offloaded to compositor or rendered flat.
    }

    pub(super) fn draw_background_paint_in_session(
        &self,
        session: &mut PixelCanvasSession<'_>,
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
        self.execute_painter_commands_in_session(session, &[command]);
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
