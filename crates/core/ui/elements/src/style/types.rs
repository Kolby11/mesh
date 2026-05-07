use mesh_core_theme::TokenValue;

/// Author-facing style diagnostic emitted while resolving supported shell CSS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleDiagnostic {
    pub property: String,
    pub selector: Option<String>,
    pub message: String,
}

const SUPPORTED_CSS_PROPERTIES: &[&str] = &[
    "background",
    "background-color",
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
];

pub fn supported_css_properties() -> &'static [&'static str] {
    SUPPORTED_CSS_PROPERTIES
}

pub fn is_supported_css_property(property: &str) -> bool {
    property.starts_with("--") || SUPPORTED_CSS_PROPERTIES.contains(&property)
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
    pub border_color: Color,
    pub border_radius: Corners,
    pub opacity: f32,
    pub transform: Transform2D,
    pub transition: TransitionStyle,
    pub animation: AnimationStyle,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub color: Color,
    pub text_align: TextAlign,
    pub line_height: f32,
    pub font_style: FontStyle,
    pub letter_spacing: f32,
    pub text_overflow: TextOverflow,
    pub text_direction: TextDirection,
    pub display: Display,
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
            border_color: Color::TRANSPARENT,
            border_radius: Corners::zero(),
            opacity: 1.0,
            transform: Transform2D::IDENTITY,
            transition: TransitionStyle::default(),
            animation: AnimationStyle::default(),
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            font_family: "Inter".to_string(),
            font_size: 14.0,
            font_weight: 400,
            color: Color::WHITE,
            text_align: TextAlign::Left,
            line_height: 1.4,
            font_style: FontStyle::Normal,
            letter_spacing: 0.0,
            text_overflow: TextOverflow::Clip,
            text_direction: TextDirection::Ltr,
            display: Display::Flex,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    pub font_size: bool,
    pub letter_spacing: bool,
    pub line_height: bool,
    pub gap: bool,
    pub inset_top: bool,
    pub inset_right: bool,
    pub inset_bottom: bool,
    pub inset_left: bool,
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    pub translate_x: f32,
    pub translate_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TransitionEasing {
    Linear,
    Ease,
    EaseIn,
    #[default]
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TransitionStyle {
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub easing: TransitionEasing,
    pub properties: TransitionProperties,
}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationIterationCount {
    Number(u32),
    Infinite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationDirection {
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationFillMode {
    None,
    Forwards,
    Backwards,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    Flex,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Start,
    End,
    Center,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    Normal,
    Italic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDirection {
    #[default]
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOverflow {
    Clip,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignSelf {
    Auto,
    Start,
    End,
    Center,
    Stretch,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignContent {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    Stretch,
}
