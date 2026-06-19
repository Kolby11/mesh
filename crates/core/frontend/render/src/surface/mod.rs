mod buffer;
mod debug_overlay;
mod glyph;
pub(crate) mod icon;
mod painter;
mod profiling;
mod text;

use std::cell::RefCell;
use std::collections::HashSet;

use mesh_core_elements::NodeId;

use crate::display_list::{DisplayPaintCommand, SelectedDisplayListPaint};

pub use buffer::{PixelBuffer, PixelCanvasSession};
pub use debug_overlay::DebugOverlay;
pub use glyph::GlyphAxes;
pub use painter::{
    FrontendRenderEngine, PainterBackendSnapshot, PainterCapabilitySnapshot,
    PainterDiagnosticSnapshot, TooltipPaintColors,
};
pub use profiling::RasterMetrics;
pub use text::{SharedTextMeasurer, TextCacheMetrics, TextRenderer};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaintProfilingMetrics {
    pub text: TextCacheMetrics,
    pub traversal_micros: u64,
    pub icon_image_raster_micros: u64,
    pub raster_cache_hits: u64,
    pub raster_cache_misses: u64,
    pub raster_cache_bypasses: u64,
    pub raster_cache_opaque_hits: u64,
    pub raster_cache_translucent_hits: u64,
}

thread_local! {
    static FRONTEND_RENDERER: RefCell<FrontendRenderEngine> = RefCell::new(FrontendRenderEngine::new());
}

/// Set the colors used by the next tooltip paint on this thread's renderer.
/// The shell calls this from its paint path so tooltip surfaces reflect the
/// active theme tokens instead of hardcoded defaults.
pub fn set_tooltip_paint_colors(colors: TooltipPaintColors) {
    FRONTEND_RENDERER.with(|engine| {
        engine.borrow().set_tooltip_colors(colors);
    });
}

/// Set the opacity used by the next tooltip paint on this thread's renderer.
/// The shell calls this to animate tooltip fade-in.
pub fn set_tooltip_paint_opacity(opacity: f32) {
    FRONTEND_RENDERER.with(|engine| {
        engine.borrow().set_tooltip_opacity(opacity);
    });
}

/// When `centered` is true the next tooltip paint treats its X coordinate as
/// the horizontal center of the tooltip box rather than the left edge.
/// The shell sets this for element-anchored placements (bottom/top) so the
/// tooltip is centered under or over the hovered element.
pub fn set_tooltip_center_x(centered: bool) {
    FRONTEND_RENDERER.with(|engine| {
        engine.borrow().set_tooltip_center_x(centered);
    });
}

/// Set the starting scale factor for the `"expand"` tooltip animation.
/// 0.0 disables scale (pure fade/slide). Values like 0.85 start the box at
/// 85 % of its full size and grow it to 100 % over the fade-in duration.
pub fn set_tooltip_scale_from(scale_from: f32) {
    FRONTEND_RENDERER.with(|engine| {
        engine.borrow().set_tooltip_scale_from(scale_from);
    });
}

pub fn paint_frontend_tree(
    tree: &mesh_core_elements::WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    tooltip: Option<(&str, f32, f32)>,
) {
    paint_frontend_tree_at(tree, buffer, scale, 0.0, 0.0, tooltip);
}

pub fn paint_frontend_tree_at(
    tree: &mesh_core_elements::WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    tooltip: Option<(&str, f32, f32)>,
) {
    paint_frontend_tree_at_for_module(tree, buffer, scale, offset_x, offset_y, tooltip, None);
}

/// Paint a frontend tree, telling the icon resolver which module owns the
/// tree. Lets the painter consult per-module icon bindings (preferred pack,
/// declared mappings, user overrides) before falling back to shell-wide
/// defaults. Pass `None` for `module_id` to use the legacy shell-wide
/// resolution path.
pub fn paint_frontend_tree_at_for_module(
    tree: &mesh_core_elements::WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    tooltip: Option<(&str, f32, f32)>,
    module_id: Option<&str>,
) {
    let _ = paint_frontend_tree_at_for_module_with_text_metrics(
        tree, buffer, scale, offset_x, offset_y, tooltip, module_id,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn paint_frontend_tree_at_for_module_with_text_metrics(
    tree: &mesh_core_elements::WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    tooltip: Option<(&str, f32, f32)>,
    module_id: Option<&str>,
) -> TextCacheMetrics {
    FRONTEND_RENDERER.with(|engine| {
        let engine = engine.borrow();
        profiling::reset_raster_metrics();
        engine.reset_text_cache_metrics();
        engine.render_tree_at_for_module(tree, buffer, scale, offset_x, offset_y, module_id);
        if let Some((tooltip_text, x, y)) = tooltip {
            engine.render_tooltip(tooltip_text, x + offset_x, y + offset_y, buffer, scale);
        }
        engine.text_cache_metrics()
    })
}

#[allow(clippy::too_many_arguments)]
pub fn paint_frontend_tree_at_for_module_with_text_metrics_clipped(
    tree: &mesh_core_elements::WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    clip: (u32, u32, u32, u32),
    tooltip: Option<(&str, f32, f32)>,
    module_id: Option<&str>,
) -> TextCacheMetrics {
    FRONTEND_RENDERER.with(|engine| {
        let engine = engine.borrow();
        profiling::reset_raster_metrics();
        engine.reset_text_cache_metrics();
        engine.render_tree_at_for_module_clipped(
            tree, buffer, scale, offset_x, offset_y, clip, module_id,
        );
        if let Some((tooltip_text, x, y)) = tooltip {
            engine.render_tooltip_clipped(
                tooltip_text,
                x + offset_x,
                y + offset_y,
                buffer,
                scale,
                Some(clip),
            );
        }
        engine.text_cache_metrics()
    })
}

#[allow(clippy::too_many_arguments)]
pub fn paint_frontend_tree_at_for_module_with_text_metrics_clipped_filtered(
    tree: &mesh_core_elements::WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
    clip: (u32, u32, u32, u32),
    paint_nodes: &HashSet<NodeId>,
    tooltip: Option<(&str, f32, f32)>,
    module_id: Option<&str>,
) -> TextCacheMetrics {
    FRONTEND_RENDERER.with(|engine| {
        let engine = engine.borrow();
        profiling::reset_raster_metrics();
        engine.reset_text_cache_metrics();
        engine.render_tree_at_for_module_clipped_filtered(
            tree,
            buffer,
            scale,
            offset_x,
            offset_y,
            clip,
            paint_nodes,
            module_id,
        );
        if let Some((tooltip_text, x, y)) = tooltip {
            engine.render_tooltip_clipped(
                tooltip_text,
                x + offset_x,
                y + offset_y,
                buffer,
                scale,
                Some(clip),
            );
        }
        engine.text_cache_metrics()
    })
}

pub fn paint_display_list_for_module_with_profiling_metrics(
    commands: &[DisplayPaintCommand],
    buffer: &mut PixelBuffer,
    scale: f32,
    clip: Option<(u32, u32, u32, u32)>,
    paint_nodes: Option<&HashSet<NodeId>>,
    tooltip: Option<(&str, f32, f32)>,
    module_id: Option<&str>,
) -> PaintProfilingMetrics {
    FRONTEND_RENDERER.with(|engine| {
        let engine = engine.borrow();
        profiling::reset_raster_metrics();
        engine.reset_text_cache_metrics();
        let traversal_started = std::time::Instant::now();
        engine.render_display_list_for_module(
            commands,
            buffer,
            scale,
            clip,
            paint_nodes,
            module_id,
        );
        let traversal_micros = traversal_started
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX)) as u64;
        if let Some((tooltip_text, x, y)) = tooltip {
            engine.render_tooltip_clipped(tooltip_text, x, y, buffer, scale, clip);
        }
        let raster = profiling::raster_metrics();
        PaintProfilingMetrics {
            text: engine.text_cache_metrics(),
            traversal_micros,
            icon_image_raster_micros: raster.icon_image_raster_micros,
            raster_cache_hits: raster.raster_cache_hits,
            raster_cache_misses: raster.raster_cache_misses,
            raster_cache_bypasses: raster.raster_cache_bypasses,
            raster_cache_opaque_hits: raster.raster_cache_opaque_hits,
            raster_cache_translucent_hits: raster.raster_cache_translucent_hits,
        }
    })
}

pub fn paint_selected_display_list_for_module_with_profiling_metrics(
    commands: &SelectedDisplayListPaint<'_>,
    buffer: &mut PixelBuffer,
    scale: f32,
    clip: Option<(u32, u32, u32, u32)>,
    paint_nodes: Option<&HashSet<NodeId>>,
    tooltip: Option<(&str, f32, f32)>,
    module_id: Option<&str>,
) -> PaintProfilingMetrics {
    FRONTEND_RENDERER.with(|engine| {
        let engine = engine.borrow();
        profiling::reset_raster_metrics();
        engine.reset_text_cache_metrics();
        let traversal_started = std::time::Instant::now();
        engine.render_selected_display_list_for_module(
            commands,
            buffer,
            scale,
            clip,
            paint_nodes,
            module_id,
        );
        let traversal_micros = traversal_started
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX)) as u64;
        if let Some((tooltip_text, x, y)) = tooltip {
            engine.render_tooltip_clipped(tooltip_text, x, y, buffer, scale, clip);
        }
        let raster = profiling::raster_metrics();
        PaintProfilingMetrics {
            text: engine.text_cache_metrics(),
            traversal_micros,
            icon_image_raster_micros: raster.icon_image_raster_micros,
            raster_cache_hits: raster.raster_cache_hits,
            raster_cache_misses: raster.raster_cache_misses,
            raster_cache_bypasses: raster.raster_cache_bypasses,
            raster_cache_opaque_hits: raster.raster_cache_opaque_hits,
            raster_cache_translucent_hits: raster.raster_cache_translucent_hits,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_overlay_does_not_dominate_paint_traversal_metric() {
        let mut without_tooltip_buffer = PixelBuffer::new(1024, 160);
        let without_tooltip = paint_display_list_for_module_with_profiling_metrics(
            &[],
            &mut without_tooltip_buffer,
            1.0,
            None,
            None,
            None,
            None,
        );

        let tooltip = "phase26 retained traversal proof ".repeat(256);
        let mut with_tooltip_buffer = PixelBuffer::new(1024, 160);
        let with_tooltip = paint_display_list_for_module_with_profiling_metrics(
            &[],
            &mut with_tooltip_buffer,
            1.0,
            None,
            None,
            Some((&tooltip, 12.0, 18.0)),
            None,
        );

        assert!(
            with_tooltip.text.shaping_micros > 0,
            "tooltip render should exercise text shaping on a cache miss"
        );

        let baseline = without_tooltip.traversal_micros.max(1);
        let allowed_growth = baseline.saturating_mul(10).saturating_add(100);
        assert!(
            with_tooltip.traversal_micros <= allowed_growth,
            "tooltip overlay work should not be counted inside paint_traversal: baseline={}us with_tooltip={}us",
            without_tooltip.traversal_micros,
            with_tooltip.traversal_micros
        );
    }
}
