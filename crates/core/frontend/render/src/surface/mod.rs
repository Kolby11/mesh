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
pub use debug_overlay::{DebugOverlay, DebugOverlayRestore, DebugPerfHudSnapshot};
pub use glyph::GlyphAxes;
pub use painter::{
    FrontendRenderEngine, PainterBackendSnapshot, PainterCapabilitySnapshot,
    PainterDiagnosticSnapshot, TooltipPaintColors,
};
pub use profiling::RasterMetrics;
pub use text::{SharedTextMeasurer, TextCacheMetrics, TextRenderer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PaintCommandClass {
    Primitive,
    Text,
    Icon,
    Control,
    Scrollbar,
}

impl PaintCommandClass {
    pub const ALL: [Self; 5] = [
        Self::Primitive,
        Self::Text,
        Self::Icon,
        Self::Control,
        Self::Scrollbar,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Primitive => "primitive",
            Self::Text => "text",
            Self::Icon => "icon",
            Self::Control => "control",
            Self::Scrollbar => "scrollbar",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaintCommandClassMetrics {
    pub command_count: u64,
    pub elapsed_micros: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaintCommandAttribution {
    classes: [PaintCommandClassMetrics; 5],
}

impl PaintCommandAttribution {
    pub fn get(&self, class: PaintCommandClass) -> PaintCommandClassMetrics {
        self.classes[class as usize]
    }

    pub(crate) fn record(
        &mut self,
        class: PaintCommandClass,
        command_count: u64,
        elapsed: std::time::Duration,
    ) {
        let metrics = &mut self.classes[class as usize];
        metrics.command_count = metrics.command_count.saturating_add(command_count);
        metrics.elapsed_micros = metrics
            .elapsed_micros
            .saturating_add(elapsed.as_micros().min(u128::from(u64::MAX)) as u64);
    }

    pub fn merge(&mut self, other: Self) {
        for class in PaintCommandClass::ALL {
            let current = &mut self.classes[class as usize];
            let next = other.classes[class as usize];
            current.command_count = current.command_count.saturating_add(next.command_count);
            current.elapsed_micros = current.elapsed_micros.saturating_add(next.elapsed_micros);
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PaintProfilingMetrics {
    pub text: TextCacheMetrics,
    pub command_attribution: PaintCommandAttribution,
    pub traversal_micros: u64,
    pub icon_image_raster_micros: u64,
    pub glyph_cache_hits: u64,
    pub glyph_cache_misses: u64,
    pub glyph_cache_entries: u64,
    pub glyph_cache_capacity: u64,
    pub font_bytes_cache_hits: u64,
    pub font_bytes_cache_misses: u64,
    pub font_bytes_cache_entries: u64,
    pub font_bytes_cache_capacity: u64,
    pub skia_glyph_cache_hits: u64,
    pub skia_glyph_cache_misses: u64,
    pub skia_glyph_cache_entries: u64,
    pub skia_glyph_cache_capacity: u64,
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

/// Set the tooltip box scale for the next tooltip paint (1.0 = resting
/// size). The shell samples the theme-CSS tooltip enter animation and pushes
/// the current frame's scale here; the box is drawn centered inside its final
/// rect so it expands outward from the middle.
pub fn set_tooltip_paint_scale(scale: f32) {
    FRONTEND_RENDERER.with(|engine| {
        engine.borrow().set_tooltip_scale(scale);
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
            command_attribution: PaintCommandAttribution::default(),
            traversal_micros,
            icon_image_raster_micros: raster.icon_image_raster_micros,
            glyph_cache_hits: raster.glyph_cache_hits,
            glyph_cache_misses: raster.glyph_cache_misses,
            glyph_cache_entries: raster.glyph_cache_entries,
            glyph_cache_capacity: raster.glyph_cache_capacity,
            font_bytes_cache_hits: raster.font_bytes_cache_hits,
            font_bytes_cache_misses: raster.font_bytes_cache_misses,
            font_bytes_cache_entries: raster.font_bytes_cache_entries,
            font_bytes_cache_capacity: raster.font_bytes_cache_capacity,
            skia_glyph_cache_hits: raster.skia_glyph_cache_hits,
            skia_glyph_cache_misses: raster.skia_glyph_cache_misses,
            skia_glyph_cache_entries: raster.skia_glyph_cache_entries,
            skia_glyph_cache_capacity: raster.skia_glyph_cache_capacity,
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
    paint_selected_display_list_for_module_with_profiling_metrics_and_attribution(
        commands,
        buffer,
        scale,
        clip,
        paint_nodes,
        tooltip,
        module_id,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn paint_selected_display_list_for_module_with_profiling_metrics_and_attribution(
    commands: &SelectedDisplayListPaint<'_>,
    buffer: &mut PixelBuffer,
    scale: f32,
    clip: Option<(u32, u32, u32, u32)>,
    paint_nodes: Option<&HashSet<NodeId>>,
    tooltip: Option<(&str, f32, f32)>,
    module_id: Option<&str>,
    collect_command_attribution: bool,
) -> PaintProfilingMetrics {
    FRONTEND_RENDERER.with(|engine| {
        let engine = engine.borrow();
        profiling::reset_raster_metrics();
        engine.reset_text_cache_metrics();
        let traversal_started = std::time::Instant::now();
        let command_attribution = if collect_command_attribution {
            engine.render_selected_display_list_for_module_with_attribution(
                commands,
                buffer,
                scale,
                clip,
                paint_nodes,
                module_id,
            )
        } else {
            engine.render_selected_display_list_for_module(
                commands,
                buffer,
                scale,
                clip,
                paint_nodes,
                module_id,
            );
            PaintCommandAttribution::default()
        };
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
            command_attribution,
            traversal_micros,
            icon_image_raster_micros: raster.icon_image_raster_micros,
            glyph_cache_hits: raster.glyph_cache_hits,
            glyph_cache_misses: raster.glyph_cache_misses,
            glyph_cache_entries: raster.glyph_cache_entries,
            glyph_cache_capacity: raster.glyph_cache_capacity,
            font_bytes_cache_hits: raster.font_bytes_cache_hits,
            font_bytes_cache_misses: raster.font_bytes_cache_misses,
            font_bytes_cache_entries: raster.font_bytes_cache_entries,
            font_bytes_cache_capacity: raster.font_bytes_cache_capacity,
            skia_glyph_cache_hits: raster.skia_glyph_cache_hits,
            skia_glyph_cache_misses: raster.skia_glyph_cache_misses,
            skia_glyph_cache_entries: raster.skia_glyph_cache_entries,
            skia_glyph_cache_capacity: raster.skia_glyph_cache_capacity,
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
