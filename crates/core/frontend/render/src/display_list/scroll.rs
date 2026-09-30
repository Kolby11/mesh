use mesh_core_elements::style::{BlendMode, Position};
use mesh_core_elements::{AffineTransform, WidgetNode};

/// Content-space retention currently admits translation-only subtrees without
/// fixed descendants or compositing/readback dependencies. Nested scrollers
/// retain their existing geometry inside the outer content coordinate space.
/// Everything else keeps the established cumulative-affine rebuild path.
pub(super) fn can_retain_scroll_content(node: &WidgetNode, world: AffineTransform) -> bool {
    let scroll = node.resolved_scroll_metrics();
    (scroll.max_x > f32::EPSILON || scroll.max_y > f32::EPSILON)
        && (node.computed_style.overflow_x.clips_contents()
            || node.computed_style.overflow_y.clips_contents())
        && is_translation(world)
        && plain_content(node)
}

fn is_translation(transform: AffineTransform) -> bool {
    transform.m11 == 1.0 && transform.m22 == 1.0 && transform.m12 == 0.0 && transform.m21 == 0.0
}

fn plain_content(node: &WidgetNode) -> bool {
    let style = &node.computed_style;
    style.position != Position::Fixed
        && style.transform.scale_x == 1.0
        && style.transform.scale_y == 1.0
        && style.transform.rotation == 0.0
        && style.opacity == 1.0
        && style.mix_blend_mode == BlendMode::Normal
        && style.filter.blur_radius == 0.0
        && style.backdrop_filter.blur_radius == 0.0
        && node.children.iter().all(plain_content)
}

// A signed logical clip used only while retaining content. The viewport is
// applied at replay, so it must not prune commands during construction.
pub(super) const CONTENT_CLIP: super::DisplayListClip = super::DisplayListClip {
    x: -536_870_912,
    y: -536_870_912,
    width: 1_073_741_824,
    height: 1_073_741_824,
};
