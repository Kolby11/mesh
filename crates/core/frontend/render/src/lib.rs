#![allow(clippy::too_many_arguments)]

pub mod display_list;
pub mod library_adapters;

#[cfg(feature = "renderer-parley")]
mod parley_adapter;

#[cfg(feature = "renderer-anyrender")]
mod anyrender_adapter;

#[cfg(feature = "renderer-accesskit")]
mod accesskit_adapter;

pub mod proof;
pub mod render_object;
pub mod surface;

#[cfg(feature = "renderer-accesskit")]
pub use accesskit_adapter::build_accesskit_runtime_update;
pub use display_list::{
    DamageRect, DisplayBatchBarrierCounts, DisplayListClip, DisplayListMetrics,
    DisplayListRepaintPolicy, DisplayPaintCommand, DisplayPaintCommandKind, RetainedDisplayList,
    SelectedDisplayListPaint,
};
pub use library_adapters::{
    CURRENT_RENDERER_AUTHORITY, RendererLibraryStatus, renderer_library_rollback_authority,
    renderer_library_statuses,
};
pub use proof::{
    FocusedAccessKitUpdate, FocusedAccessibilityEvidence, FocusedDirtyEvidence,
    FocusedProofDiagnostic, FocusedProofNode, FocusedProofSnapshot, build_accesskit_update,
    build_focused_proof_snapshot,
};
pub use render_object::{RenderObjectDirtySummary, RenderObjectFingerprint, RenderObjectTree};
pub use surface::{
    BlurQuality, DebugOverlay, DebugOverlayRestore, DebugPerfHudSnapshot, FrontendRenderEngine,
    GlyphAxes, MAX_BLUR_PASSES, PaintCommandAttribution, PaintCommandClass,
    PaintCommandClassMetrics, PaintProfilingMetrics, PainterBackendSnapshot,
    PainterCapabilitySnapshot, PainterDiagnosticSnapshot, PixelBuffer, RasterMetrics,
    SharedTextMeasurer, TextCacheMetrics, TextRenderer, TooltipPaintColors, blur_quality,
    glyph_raster_jobs_pending, icon_raster_jobs_pending, icon_resolution_jobs_pending,
    paint_display_list_for_module_with_profiling_metrics, paint_frontend_tree,
    paint_frontend_tree_at, paint_frontend_tree_at_for_module,
    paint_frontend_tree_at_for_module_with_text_metrics,
    paint_frontend_tree_at_for_module_with_text_metrics_clipped,
    paint_frontend_tree_at_for_module_with_text_metrics_clipped_filtered,
    paint_selected_display_list_for_module_with_profiling_metrics,
    paint_selected_display_list_for_module_with_profiling_metrics_and_attribution,
    paint_selected_display_list_regions_for_module_with_profiling_metrics_and_attribution,
    poll_glyph_raster_jobs, poll_icon_raster_jobs, poll_icon_resolution_jobs, set_blur_quality,
    set_font_aliases, set_font_database, set_tooltip_center_x, set_tooltip_paint_colors,
    set_tooltip_paint_opacity, set_tooltip_paint_scale,
};
