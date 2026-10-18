#![allow(clippy::too_many_arguments)]

mod device_damage;
pub mod display_list;
mod fractional_scale;
pub mod library_adapters;
mod paint_input;
pub mod scene_thread;

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
pub use device_damage::{DeviceDamage, MAX_DEVICE_DAMAGE_RECTS};
pub use display_list::{
    BackdropBlurPolicy, DamageRect, DisplayBatchBarrierCounts, DisplayListClip, DisplayListMetrics,
    DisplayListRepaintPolicy, DisplayPaintCommand, DisplayPaintCommandKind, FramePaintDamage,
    FramePaintEffects, FramePaintInputs, FramePaintPlan, FramePaintReplay, FramePaintReplaySpan,
    FramePaintTopology, FramePaintTransform, RetainedDisplayList, SelectedDisplayListPaint,
};
pub use fractional_scale::{DeviceRect, FractionalScale};
pub use library_adapters::{
    CURRENT_RENDERER_AUTHORITY, RendererLibraryStatus, renderer_library_rollback_authority,
    renderer_library_statuses,
};
pub use paint_input::paint_variables_fingerprint;
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
    SharedTextMeasurer, TextCacheMetrics, TextRenderer, TooltipPaintColors, backdrop_blur_policy,
    blur_quality, glyph_raster_jobs_pending, icon_raster_jobs_pending,
    icon_resolution_jobs_pending, image_decode_jobs_pending, paint_backend_supports_backdrop_blur,
    paint_display_list_for_module_with_profiling_metrics, paint_frontend_tree,
    paint_frontend_tree_at, paint_frontend_tree_at_for_module,
    paint_frontend_tree_at_for_module_with_text_metrics,
    paint_frontend_tree_at_for_module_with_text_metrics_clipped,
    paint_frontend_tree_at_for_module_with_text_metrics_clipped_filtered,
    paint_selected_display_list_for_module_with_profiling_metrics,
    paint_selected_display_list_for_module_with_profiling_metrics_and_attribution,
    paint_selected_display_list_regions_for_module_with_profiling_metrics_and_attribution,
    poll_glyph_raster_jobs, poll_icon_raster_jobs, poll_icon_resolution_jobs,
    poll_image_decode_jobs, resource_completion_waker_installed, set_backdrop_blur_policy,
    BackdropFrame, has_backdrop_image, set_backdrop_frame, set_backdrop_image,
    set_blur_quality, set_font_aliases, set_font_database, set_resource_completion_waker,
    set_tooltip_center_x, set_tooltip_paint_colors, set_tooltip_paint_font_family,
    set_tooltip_paint_opacity, set_tooltip_paint_scale,
};

/// Serializes tests that touch process-wide resource state: the global
/// resource revision and the icon/image worker queues and caches. A revision
/// bump from a parallel test makes an in-flight worker result stale, so the
/// worker test sees nothing published.
#[cfg(test)]
pub(crate) fn resource_state_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
