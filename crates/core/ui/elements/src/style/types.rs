use std::sync::Arc;

use mesh_core_theme::TokenValue;

/// Author-facing style diagnostic emitted while resolving supported shell CSS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleDiagnostic {
    pub property: String,
    pub selector: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleProfileStatus {
    Implemented,
    DiagnosticOnly,
    Deferred,
    OutOfScope,
}

impl StyleProfileStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Implemented => "implemented",
            Self::DiagnosticOnly => "diagnostic-only",
            Self::Deferred => "deferred",
            Self::OutOfScope => "out-of-scope",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleProfileProperty {
    pub property: &'static str,
    pub category: &'static str,
    pub status: StyleProfileStatus,
}

const SUPPORTED_CSS_PROPERTIES: &[&str] = &[
    "background",
    "background-color",
    "background-image",
    "color",
    "border",
    "border-color",
    "border-width",
    "border-top-width",
    "border-right-width",
    "border-bottom-width",
    "border-left-width",
    "border-radius",
    "border-top-left-radius",
    "border-top-right-radius",
    "border-bottom-right-radius",
    "border-bottom-left-radius",
    "display",
    "visibility",
    "opacity",
    "overflow",
    "overflow-x",
    "overflow-y",
    "width",
    "height",
    "min-width",
    "max-width",
    "min-height",
    "max-height",
    "padding",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "padding-x",
    "padding-y",
    "padding-inline",
    "padding-block",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "margin-x",
    "margin-y",
    "margin-inline",
    "margin-block",
    "font",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "line-height",
    "letter-spacing",
    "text-align",
    "text-overflow",
    "white-space",
    "direction",
    "flex",
    "flex-direction",
    "flex-wrap",
    "flex-grow",
    "flex-shrink",
    "flex-basis",
    "justify-content",
    "align-items",
    "align-self",
    "align-content",
    "gap",
    "row-gap",
    "column-gap",
    "gap-x",
    "position",
    "z-index",
    "inset",
    "top",
    "right",
    "bottom",
    "left",
    "transition",
    "transition-property",
    "transition-duration",
    "transition-delay",
    "transition-timing-function",
    "animation",
    "animation-name",
    "animation-duration",
    "animation-delay",
    "animation-timing-function",
    "animation-iteration-count",
    "animation-direction",
    "animation-fill-mode",
    "animation-play-state",
    "transform",
    "transform-origin",
    "box-shadow",
    "filter",
    "backdrop-filter",
    "tooltip-anchor",
    "tooltip-offset",
];

const STYLE_PROFILE_PROPERTIES: &[StyleProfileProperty] = &[
    StyleProfileProperty {
        property: "background",
        category: "color",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "background-color",
        category: "color",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "color",
        category: "color",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-color",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-width",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-top-width",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-right-width",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-bottom-width",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-left-width",
        category: "border",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-radius",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-top-left-radius",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-top-right-radius",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-bottom-right-radius",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-bottom-left-radius",
        category: "radius",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "display",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "visibility",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "opacity",
        category: "opacity",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "overflow",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "overflow-x",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "overflow-y",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "width",
        category: "size",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "height",
        category: "size",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "min-width",
        category: "size",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "max-width",
        category: "size",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "min-height",
        category: "size",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "max-height",
        category: "size",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-top",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-right",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-bottom",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-left",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-x",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-y",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-inline",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "padding-block",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-top",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-right",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-bottom",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-left",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-x",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-y",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-inline",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "margin-block",
        category: "spacing",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "font",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "font-family",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "font-size",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "font-weight",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "font-style",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "line-height",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "letter-spacing",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "text-align",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "text-overflow",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "direction",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "flex",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "flex-direction",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "flex-wrap",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "flex-grow",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "flex-shrink",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "flex-basis",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "justify-content",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "align-items",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "align-self",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "align-content",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "gap",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "row-gap",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "column-gap",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "gap-x",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "position",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "z-index",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "inset",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "top",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "right",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "bottom",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "left",
        category: "layout",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transition",
        category: "transition",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transition-property",
        category: "transition",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transition-duration",
        category: "transition",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transition-delay",
        category: "transition",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transition-timing-function",
        category: "transition",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-name",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-duration",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-delay",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-timing-function",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-iteration-count",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-direction",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-fill-mode",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "animation-play-state",
        category: "animation",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transform",
        category: "transform",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "transform-origin",
        category: "transform",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "box-shadow",
        category: "shadow",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "filter",
        category: "filter",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "backdrop-filter",
        category: "filter",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "border-style",
        category: "border",
        status: StyleProfileStatus::DiagnosticOnly,
    },
    StyleProfileProperty {
        property: "background-image",
        category: "image",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "linear-gradient",
        category: "gradient",
        status: StyleProfileStatus::Deferred,
    },
    StyleProfileProperty {
        property: "grid-template-columns",
        category: "layout",
        status: StyleProfileStatus::OutOfScope,
    },
    StyleProfileProperty {
        property: "float",
        category: "layout",
        status: StyleProfileStatus::OutOfScope,
    },
    StyleProfileProperty {
        property: "white-space",
        category: "font",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "container-type",
        category: "layout",
        status: StyleProfileStatus::OutOfScope,
    },
    StyleProfileProperty {
        property: "text-wrap",
        category: "font",
        status: StyleProfileStatus::OutOfScope,
    },
    StyleProfileProperty {
        property: "tooltip-anchor",
        category: "tooltip",
        status: StyleProfileStatus::Implemented,
    },
    StyleProfileProperty {
        property: "tooltip-offset",
        category: "tooltip",
        status: StyleProfileStatus::Implemented,
    },
];

pub fn supported_css_properties() -> &'static [&'static str] {
    SUPPORTED_CSS_PROPERTIES
}

pub fn is_supported_css_property(property: &str) -> bool {
    property.starts_with("--") || SUPPORTED_CSS_PROPERTIES.contains(&property)
}

pub fn style_profile_properties() -> &'static [StyleProfileProperty] {
    STYLE_PROFILE_PROPERTIES
}

pub fn style_profile_status(property: &str) -> Option<StyleProfileStatus> {
    if property.starts_with("--") {
        return Some(StyleProfileStatus::Implemented);
    }

    STYLE_PROFILE_PROPERTIES
        .iter()
        .find(|entry| entry.property == property)
        .map(|entry| entry.status)
}

pub fn is_transition_safe_keyframe_property(property: &str) -> bool {
    mesh_core_component::style::is_transition_safe_keyframe_property(property)
}

/// Where a tooltip should appear relative to the hovered element.
///
/// Set via the `tooltip-anchor` CSS property. Elements use this to declare
/// their preferred tooltip placement. The shell applies screen-edge avoidance
/// automatically — if the preferred placement would overflow, it flips to the
/// opposite side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TooltipAnchor {
    /// Use the shell's global default positioning strategy.
    #[default]
    Auto,
    /// Centered below the element (preferred), flipping above on overflow.
    Bottom,
    /// Centered above the element (preferred), flipping below on overflow.
    Top,
    /// Left of the element (preferred), flipping right on overflow.
    Left,
    /// Right of the element (preferred), flipping left on overflow.
    Right,
    /// Place the tooltip near the cursor.
    Cursor,
}

impl TooltipAnchor {
    pub fn from_css(value: &str) -> Option<Self> {
        match value.trim() {
            "auto" => Some(Self::Auto),
            "bottom" => Some(Self::Bottom),
            "top" => Some(Self::Top),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "cursor" => Some(Self::Cursor),
            _ => None,
        }
    }
}

/// Fully resolved style for a widget node.
#[derive(Debug, Clone)]
pub struct ComputedStyle {
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
    pub padding: Edges,
    pub margin: Edges,
    pub border_width: Edges,
    pub background_color: Color,
    pub background_paint: BackgroundPaint,
    pub border_color: Color,
    pub border_radius: Corners,
    pub opacity: f32,
    pub transform: Transform2D,
    pub transform_origin: TransformOrigin,
    pub box_shadow: BoxShadow,
    pub filter: VisualFilter,
    pub backdrop_filter: VisualFilter,
    pub transitions: Vec<TransitionStyle>,
    pub animations: Vec<AnimationStyle>,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub font_family: Arc<str>,
    pub font_size: f32,
    pub font_weight: u16,
    pub color: Color,
    pub text_align: TextAlign,
    pub line_height: f32,
    pub font_style: FontStyle,
    pub letter_spacing: f32,
    pub text_overflow: TextOverflow,
    pub white_space: WhiteSpace,
    pub text_direction: TextDirection,
    pub display: Display,
    pub visibility: Visibility,
    pub direction: FlexDirection,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub align_content: AlignContent,
    pub gap: f32,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Dimension,
    pub flex_wrap: FlexWrap,
    pub align_self: AlignSelf,
    pub position: Position,
    pub z_index: i32,
    pub inset_top: Option<f32>,
    pub inset_right: Option<f32>,
    pub inset_bottom: Option<f32>,
    pub inset_left: Option<f32>,
    /// Variable-font axis values for icon font packs (Material Symbols et
    /// al.). Sourced from CSS custom properties `--icon-fill`,
    /// `--icon-weight`, `--icon-grade`, `--icon-optical-size`. `None`
    /// means "use the font's default for this axis"; `Some` overrides it.
    /// Silently ignored when the resolved icon is a file (SVG/PNG) or
    /// when the font pack doesn't expose the axis.
    pub icon_fill: Option<f32>,
    pub icon_weight: Option<f32>,
    pub icon_grade: Option<f32>,
    pub icon_optical_size: Option<f32>,
    /// Per-element tooltip placement override. Set via the `tooltip-anchor`
    /// CSS property. `Auto` defers to the shell's global default.
    pub tooltip_anchor: TooltipAnchor,
    /// Per-element tooltip offset override in CSS pixels (horizontal, vertical).
    /// Set via the `tooltip-offset` CSS property. `None` uses the shell default.
    pub tooltip_offset: Option<(f32, f32)>,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            padding: Edges::zero(),
            margin: Edges::zero(),
            border_width: Edges::zero(),
            background_color: Color::TRANSPARENT,
            background_paint: BackgroundPaint::None,
            border_color: Color::TRANSPARENT,
            border_radius: Corners::zero(),
            opacity: 1.0,
            transform: Transform2D::IDENTITY,
            transform_origin: TransformOrigin::default(),
            box_shadow: BoxShadow::NONE,
            filter: VisualFilter::NONE,
            backdrop_filter: VisualFilter::NONE,
            transitions: vec![TransitionStyle::default()],
            animations: vec![AnimationStyle::default()],
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            font_family: Arc::from("Inter"),
            font_size: 14.0,
            font_weight: 400,
            color: Color::WHITE,
            text_align: TextAlign::Left,
            line_height: 1.4,
            font_style: FontStyle::Normal,
            letter_spacing: 0.0,
            text_overflow: TextOverflow::Clip,
            white_space: WhiteSpace::Normal,
            text_direction: TextDirection::Ltr,
            display: Display::Flex,
            visibility: Visibility::Visible,
            direction: FlexDirection::Row,
            justify_content: JustifyContent::Start,
            align_items: AlignItems::Stretch,
            align_content: AlignContent::Stretch,
            gap: 0.0,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::Auto,
            flex_wrap: FlexWrap::NoWrap,
            align_self: AlignSelf::Auto,
            position: Position::Static,
            z_index: 0,
            inset_top: None,
            inset_right: None,
            inset_bottom: None,
            inset_left: None,
            icon_fill: None,
            icon_weight: None,
            icon_grade: None,
            icon_optical_size: None,
            tooltip_anchor: TooltipAnchor::Auto,
            tooltip_offset: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dimension {
    Auto,
    Px(f32),
    Percent(f32),
    Content,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub fn zero() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }

    pub fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corners {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl Corners {
    pub fn zero() -> Self {
        Self {
            top_left: 0.0,
            top_right: 0.0,
            bottom_right: 0.0,
            bottom_left: 0.0,
        }
    }

    pub fn all(value: f32) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Overflow {
    Visible,
    Hidden,
    Auto,
    Scroll,
}

impl Overflow {
    pub fn clips_contents(self) -> bool {
        !matches!(self, Self::Visible)
    }

    pub fn shows_scrollbar_when_overflowing(self) -> bool {
        matches!(self, Self::Auto | Self::Scroll)
    }

    pub fn always_shows_scrollbar(self) -> bool {
        matches!(self, Self::Scroll)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StyleContext {
    pub container_width: f32,
    pub container_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct TransitionProperties {
    pub all: bool,
    pub border_radius: bool,
    pub border_width: bool,
    pub opacity: bool,
    pub background_color: bool,
    pub border_color: bool,
    pub color: bool,
    pub width: bool,
    pub height: bool,
    pub min_width: bool,
    pub max_width: bool,
    pub min_height: bool,
    pub max_height: bool,
    pub padding: bool,
    pub margin: bool,
    pub transform: bool,
    pub box_shadow: bool,
    pub filter: bool,
    pub backdrop_filter: bool,
    pub font_size: bool,
    pub letter_spacing: bool,
    pub line_height: bool,
    pub gap: bool,
    pub inset_top: bool,
    pub inset_right: bool,
    pub inset_bottom: bool,
    pub inset_left: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationPropertyBucket {
    None,
    PaintOnly,
    LayerEffect,
    LayoutAffecting,
}

impl TransitionProperties {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn all() -> Self {
        Self {
            all: true,
            border_radius: true,
            border_width: true,
            opacity: true,
            background_color: true,
            border_color: true,
            color: true,
            width: true,
            height: true,
            min_width: true,
            max_width: true,
            min_height: true,
            max_height: true,
            padding: true,
            margin: true,
            transform: true,
            box_shadow: true,
            filter: true,
            backdrop_filter: true,
            font_size: true,
            letter_spacing: true,
            line_height: true,
            gap: true,
            inset_top: true,
            inset_right: true,
            inset_bottom: true,
            inset_left: true,
        }
    }

    pub fn animates_border_radius(self) -> bool {
        self.all || self.border_radius
    }

    pub fn animates_border_width(self) -> bool {
        self.all || self.border_width
    }

    pub fn animates_opacity(self) -> bool {
        self.all || self.opacity
    }

    pub fn animates_background_color(self) -> bool {
        self.all || self.background_color
    }

    pub fn animates_border_color(self) -> bool {
        self.all || self.border_color
    }

    pub fn animates_color(self) -> bool {
        self.all || self.color
    }

    pub fn animates_width(self) -> bool {
        self.all || self.width
    }

    pub fn animates_height(self) -> bool {
        self.all || self.height
    }

    pub fn animates_padding(self) -> bool {
        self.all || self.padding
    }

    pub fn animates_margin(self) -> bool {
        self.all || self.margin
    }

    pub fn animates_transform(self) -> bool {
        self.all || self.transform
    }

    pub fn animates_box_shadow(self) -> bool {
        self.all || self.box_shadow
    }

    pub fn animates_filter(self) -> bool {
        self.all || self.filter
    }

    pub fn animates_backdrop_filter(self) -> bool {
        self.all || self.backdrop_filter
    }

    pub fn animates_min_width(self) -> bool {
        self.all || self.min_width
    }

    pub fn animates_max_width(self) -> bool {
        self.all || self.max_width
    }

    pub fn animates_min_height(self) -> bool {
        self.all || self.min_height
    }

    pub fn animates_max_height(self) -> bool {
        self.all || self.max_height
    }

    pub fn animates_font_size(self) -> bool {
        self.all || self.font_size
    }

    pub fn animates_letter_spacing(self) -> bool {
        self.all || self.letter_spacing
    }

    pub fn animates_line_height(self) -> bool {
        self.all || self.line_height
    }

    pub fn animates_gap(self) -> bool {
        self.all || self.gap
    }

    pub fn animates_inset_top(self) -> bool {
        self.all || self.inset_top
    }

    pub fn animates_inset_right(self) -> bool {
        self.all || self.inset_right
    }

    pub fn animates_inset_bottom(self) -> bool {
        self.all || self.inset_bottom
    }

    pub fn animates_inset_left(self) -> bool {
        self.all || self.inset_left
    }

    pub fn affects_layout(self) -> bool {
        self.all
            || self.border_width
            || self.width
            || self.height
            || self.min_width
            || self.max_width
            || self.min_height
            || self.max_height
            || self.padding
            || self.margin
            || self.font_size
            || self.letter_spacing
            || self.line_height
            || self.gap
            || self.inset_top
            || self.inset_right
            || self.inset_bottom
            || self.inset_left
    }

    pub fn animation_bucket(self) -> AnimationPropertyBucket {
        if self.all
            || self.width
            || self.height
            || self.min_width
            || self.max_width
            || self.min_height
            || self.max_height
            || self.padding
            || self.margin
            || self.font_size
            || self.letter_spacing
            || self.line_height
            || self.gap
            || self.inset_top
            || self.inset_right
            || self.inset_bottom
            || self.inset_left
        {
            return AnimationPropertyBucket::LayoutAffecting;
        }

        if self.box_shadow || self.filter || self.backdrop_filter {
            return AnimationPropertyBucket::LayerEffect;
        }

        if self.border_radius
            || self.border_width
            || self.opacity
            || self.background_color
            || self.border_color
            || self.color
            || self.transform
        {
            return AnimationPropertyBucket::PaintOnly;
        }

        AnimationPropertyBucket::None
    }

    pub fn has_paint_only_animation(self) -> bool {
        self.animation_bucket() == AnimationPropertyBucket::PaintOnly
    }

    pub fn has_layer_effect_animation(self) -> bool {
        self.animation_bucket() == AnimationPropertyBucket::LayerEffect
    }

    pub fn has_layout_affecting_animation(self) -> bool {
        self.animation_bucket() == AnimationPropertyBucket::LayoutAffecting
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    pub translate_x: f32,
    pub translate_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub spread_radius: f32,
    pub color: Color,
    pub inset: bool,
}

impl BoxShadow {
    pub const NONE: Self = Self {
        offset_x: 0.0,
        offset_y: 0.0,
        blur_radius: 0.0,
        spread_radius: 0.0,
        color: Color::TRANSPARENT,
        inset: false,
    };

    pub fn is_none(self) -> bool {
        self.color.a == 0
            || (self.offset_x == 0.0
                && self.offset_y == 0.0
                && self.blur_radius == 0.0
                && self.spread_radius == 0.0)
    }
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self::NONE
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum BackgroundPaint {
    #[default]
    None,
    Image(StyleImageSource),
    LinearGradient(StyleLinearGradient),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleImageSource {
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StyleLinearGradient {
    pub from: Color,
    pub to: Color,
}

impl Default for StyleLinearGradient {
    fn default() -> Self {
        Self {
            from: Color::TRANSPARENT,
            to: Color::TRANSPARENT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualFilter {
    pub blur_radius: f32,
}

impl VisualFilter {
    pub const NONE: Self = Self { blur_radius: 0.0 };

    pub fn is_none(self) -> bool {
        self.blur_radius <= 0.0
    }
}

impl Default for VisualFilter {
    fn default() -> Self {
        Self::NONE
    }
}

impl Transform2D {
    pub const IDENTITY: Self = Self {
        translate_x: 0.0,
        translate_y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation: 0.0,
    };

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Where the jumps land in a CSS `steps()` timing function. The legacy `start`
/// / `end` keywords map onto `JumpStart` / `JumpEnd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StepPosition {
    /// Jump at the start of each interval (`jump-start` / `start`).
    JumpStart,
    /// Jump at the end of each interval (`jump-end` / `end`). CSS default.
    #[default]
    JumpEnd,
    /// No jump at either end — `n` stops including both 0 and 1 (`jump-none`).
    JumpNone,
    /// Jump at both ends — neither 0 nor 1 is held (`jump-both`).
    JumpBoth,
}

/// A single axis value for `transform-origin`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformOriginValue {
    Percent(f32),
    Px(f32),
}

impl TransformOriginValue {
    pub fn resolve(&self, size: f32) -> f32 {
        match self {
            Self::Percent(p) => size * p / 100.0,
            Self::Px(v) => *v,
        }
    }
}

/// The CSS `transform-origin` property. Default is 50% 50% (center).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformOrigin {
    pub x: TransformOriginValue,
    pub y: TransformOriginValue,
}

impl Default for TransformOrigin {
    fn default() -> Self {
        Self {
            x: TransformOriginValue::Percent(50.0),
            y: TransformOriginValue::Percent(50.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TransitionEasing {
    Linear,
    Ease,
    EaseIn,
    #[default]
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
    /// `steps(n, <position>)` — a discrete step function with `n` intervals.
    Steps(u32, StepPosition),
}

impl std::hash::Hash for TransitionEasing {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::CubicBezier(a, b, c, d) => {
                a.to_bits().hash(state);
                b.to_bits().hash(state);
                c.to_bits().hash(state);
                d.to_bits().hash(state);
            }
            Self::Steps(count, position) => {
                count.hash(state);
                position.hash(state);
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Hash)]
pub struct TransitionStyle {
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub easing: TransitionEasing,
    pub properties: TransitionProperties,
}

#[derive(Debug, Clone, PartialEq, Hash)]
pub struct AnimationStyle {
    pub name: Option<String>,
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub easing: TransitionEasing,
    pub iteration_count: AnimationIterationCount,
    pub direction: AnimationDirection,
    pub fill_mode: AnimationFillMode,
    pub play_state: AnimationPlayState,
}

impl Default for AnimationStyle {
    fn default() -> Self {
        Self {
            name: None,
            duration_ms: 0,
            delay_ms: 0,
            easing: TransitionEasing::EaseOut,
            iteration_count: AnimationIterationCount::Number(1),
            direction: AnimationDirection::Normal,
            fill_mode: AnimationFillMode::None,
            play_state: AnimationPlayState::Running,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationIterationCount {
    Number(u32),
    Infinite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationDirection {
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationFillMode {
    None,
    Forwards,
    Backwards,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationPlayState {
    Running,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };

    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#')?;
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(Self { r, g, b, a: 255 })
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Self { r, g, b, a: 255 })
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Self { r, g, b, a })
            }
            _ => None,
        }
    }

    pub fn from_token(value: &TokenValue) -> Option<Self> {
        match value {
            TokenValue::String(s) => Self::from_hex(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Display {
    Flex,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    Collapse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JustifyContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignItems {
    Start,
    End,
    Center,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Normal,
    Italic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum TextDirection {
    #[default]
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WhiteSpace {
    #[default]
    Normal,
    Nowrap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignSelf {
    Auto,
    Start,
    End,
    Center,
    Stretch,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlignContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    Stretch,
}
