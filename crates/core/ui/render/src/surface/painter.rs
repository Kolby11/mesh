mod geometry;
mod text;
mod tree;
mod widgets;

use super::PixelBuffer;
use super::icon;
use super::text::{TextRenderer, TextSelectionGeometry};
use mesh_core_elements::style::{Color, Display, Overflow, TextAlign, TextDirection, TextOverflow};
use mesh_core_elements::tree::WidgetNode;

pub(crate) use geometry::{ClipRect, fill_rect_clipped, fill_rounded_rect_clipped};
use geometry::{clip_to_tuple, dim_color, intersect_clip, node_attr_f32, node_clips_children};

pub struct FrontendRenderEngine {
    text_renderer: TextRenderer,
}

impl FrontendRenderEngine {
    pub fn new() -> Self {
        Self {
            text_renderer: TextRenderer::new(),
        }
    }
}

impl Default for FrontendRenderEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
