mod buffer;
mod debug_overlay;
mod glyph;
mod icon;
mod painter;
mod profiling;
mod text;

use std::cell::RefCell;
use std::collections::HashSet;

use mesh_core_elements::NodeId;

use crate::display_list::DisplayPaintCommand;

pub use buffer::PixelBuffer;
pub use debug_overlay::DebugOverlay;
pub use glyph::GlyphAxes;
pub use painter::FrontendRenderEngine;
pub use profiling::RasterMetrics;
pub use text::{SharedTextMeasurer, TextCacheMetrics, TextRenderer};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaintProfilingMetrics {
    pub text: TextCacheMetrics,
    pub traversal_micros: u64,
    pub icon_image_raster_micros: u64,
}

thread_local! {
    static FRONTEND_RENDERER: RefCell<FrontendRenderEngine> = RefCell::new(FrontendRenderEngine::new());
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
            engine.render_tooltip(tooltip_text, x + offset_x, y + offset_y, buffer, scale);
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
            engine.render_tooltip(tooltip_text, x + offset_x, y + offset_y, buffer, scale);
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
        if let Some((tooltip_text, x, y)) = tooltip {
            engine.render_tooltip(tooltip_text, x, y, buffer, scale);
        }
        PaintProfilingMetrics {
            text: engine.text_cache_metrics(),
            traversal_micros: traversal_started
                .elapsed()
                .as_micros()
                .min(u128::from(u64::MAX)) as u64,
            icon_image_raster_micros: profiling::raster_metrics().icon_image_raster_micros,
        }
    })
}
