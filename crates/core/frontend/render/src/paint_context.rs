use crate::{BackdropBlurPolicy, BackdropFrame, BlurQuality, TooltipPaintStyle};

/// Value inputs for one frame. Resource preparation and the recorded scene
/// consume this snapshot; later settings changes cannot alter committed paint.
#[derive(Clone, Debug)]
pub struct PaintContext {
    pub blur_quality: BlurQuality,
    pub backdrop_policy: BackdropBlurPolicy,
    pub backdrop: BackdropFrame,
    pub tooltip_style: TooltipPaintStyle,
    pub tooltip_opacity: f32,
    pub tooltip_font_family: String,
    pub tooltip_center_x: bool,
    pub tooltip_scale: f32,
}

impl Default for PaintContext {
    fn default() -> Self {
        Self {
            blur_quality: BlurQuality::default(),
            backdrop_policy: BackdropBlurPolicy::CompositorRegion,
            backdrop: BackdropFrame::default(),
            tooltip_style: TooltipPaintStyle::DEFAULT_DARK,
            tooltip_opacity: 1.0,
            tooltip_font_family: "Inter".into(),
            tooltip_center_x: false,
            tooltip_scale: 1.0,
        }
    }
}

impl crate::FrontendRenderEngine {
    pub fn with_context(context: &PaintContext) -> Self {
        let engine = Self::new();
        engine.apply_context(context);
        engine
    }

    pub(crate) fn apply_context(&self, context: &PaintContext) {
        self.set_blur_quality(context.blur_quality);
        self.set_backdrop_blur_policy(context.backdrop_policy);
        self.set_backdrop_frame(context.backdrop);
        self.set_tooltip_style(context.tooltip_style);
        self.set_tooltip_opacity(context.tooltip_opacity);
        self.set_tooltip_font_family(context.tooltip_font_family.clone());
        self.set_tooltip_center_x(context.tooltip_center_x);
        self.set_tooltip_scale(context.tooltip_scale);
    }
}
