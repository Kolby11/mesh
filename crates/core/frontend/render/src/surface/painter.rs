mod geometry;
mod text;
mod tree;
mod widgets;

use super::PixelBuffer;
use super::icon;
use super::text::{TextCacheMetrics, TextRenderer, TextSelectionGeometry};
use mesh_core_elements::style::{Color, Display, Overflow, TextAlign, TextDirection, TextOverflow};
use mesh_core_elements::tree::WidgetNode;

pub(crate) use geometry::{
    ClipRect, fill_rect_clipped, fill_rounded_rect_clipped, stroke_rounded_rect_clipped,
};
use geometry::{
    clip_to_tuple, dim_color, intersect_clip, node_attr_f32, node_clips_children, opacity_color,
};

pub struct FrontendRenderEngine {
    text_renderer: TextRenderer,
}

impl FrontendRenderEngine {
    pub fn new() -> Self {
        Self {
            text_renderer: TextRenderer::new(),
        }
    }

    pub fn reset_text_cache_metrics(&self) {
        self.text_renderer.reset_cache_metrics();
    }

    pub fn text_cache_metrics(&self) -> TextCacheMetrics {
        self.text_renderer.cache_metrics()
    }
}

impl Default for FrontendRenderEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
