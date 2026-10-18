use super::index::references_style_function;
use crate::style::parse::*;
use crate::style::*;
use mesh_core_component::style::StyleValue;

macro_rules! literal_properties {
    ($raw:ident, $style:ident, $value:ident; $($variant:ident($ty:ident): $($name:literal)|+ => $parse:expr => $apply:expr;)*) => {
        #[derive(Debug, Clone, Copy)]
        pub(super) enum LiteralProperty { $($variant,)* }

        #[derive(Debug, Clone, Copy)]
        pub(super) enum TypedLiteralOperation { $($variant($ty),)* }

        impl LiteralProperty {
            pub(super) fn from_name(name: &str) -> Option<Self> {
                match name { $($($name)|+ => Some(Self::$variant),)* _ => None }
            }

            pub(super) fn lower_number(self, number: f64) -> Option<TypedLiteralOperation> {
                match self { $(Self::$variant => literal_properties!(@number $variant($ty), number, self),)* }
            }

            pub(super) fn lower(self, $raw: &str) -> Option<TypedLiteralOperation> {
                if references_style_function($raw) { return None; }
                if $raw.trim() == "inherit" && matches!(self,
                    Self::Color | Self::FontSize | Self::FontWeight | Self::LineHeight) {
                    return None;
                }
                match self { $(Self::$variant => ($parse).map(TypedLiteralOperation::$variant),)* }
            }
        }

        pub(super) fn apply_typed_literal($style: &mut ComputedStyle, operation: TypedLiteralOperation) {
            match operation { $(TypedLiteralOperation::$variant($value) => { $apply; })* }
        }
    };
    (@number $variant:ident(f32), $number:ident, $property:ident) => {
        Some(TypedLiteralOperation::$variant($number as f32))
    };
    (@number $variant:ident(u16), $number:ident, $property:ident) => {
        Some(TypedLiteralOperation::$variant(($number as f32) as u16))
    };
    (@number $variant:ident($ty:ident), $number:ident, $property:ident) => {
        $property.lower(&$number.to_string())
    };

}

literal_properties! { raw, style, value;
    Background(Color): "background" | "background-color" => Color::from_css(raw) => style.background_color = value;
    Color(Color): "color" => Color::from_css(raw) => style.color = value;
    FontSize(f32): "font-size" => Some(parse_px(raw)) => style.font_size = value;
    FontWeight(u16): "font-weight" => Some(parse_px(raw) as u16) => style.font_weight = value;
    LetterSpacing(f32): "letter-spacing" => Some(parse_px(raw)) => style.letter_spacing = value;
    LineHeight(f32): "line-height" => Some(parse_px(raw)) => style.line_height = value;
    Gap(f32): "gap" | "column-gap" | "row-gap" | "gap-x" => Some(parse_px(raw)) => style.gap = value;
    Opacity(f32): "opacity" => Some(parse_px(raw)) => style.opacity = value;
    PaddingTop(f32): "padding-top" => Some(parse_px(raw)) => style.padding.top = value;
    PaddingRight(f32): "padding-right" => Some(parse_px(raw)) => style.padding.right = value;
    PaddingBottom(f32): "padding-bottom" => Some(parse_px(raw)) => style.padding.bottom = value;
    PaddingLeft(f32): "padding-left" => Some(parse_px(raw)) => style.padding.left = value;
    MarginTop(f32): "margin-top" => Some(parse_px(raw)) => style.margin.top = value;
    MarginRight(f32): "margin-right" => Some(parse_px(raw)) => style.margin.right = value;
    MarginBottom(f32): "margin-bottom" => Some(parse_px(raw)) => style.margin.bottom = value;
    MarginLeft(f32): "margin-left" => Some(parse_px(raw)) => style.margin.left = value;
    BorderTopWidth(f32): "border-top-width" => Some(parse_px(raw)) => style.border_width.top = value;
    BorderRightWidth(f32): "border-right-width" => Some(parse_px(raw)) => style.border_width.right = value;
    BorderBottomWidth(f32): "border-bottom-width" => Some(parse_px(raw)) => style.border_width.bottom = value;
    BorderLeftWidth(f32): "border-left-width" => Some(parse_px(raw)) => style.border_width.left = value;
    Padding(Edges): "padding" => Some(parse_edges_shorthand(raw)) => style.padding = value;
    Margin(Edges): "margin" => Some(parse_edges_shorthand(raw)) => style.margin = value;
    BorderWidth(Edges): "border-width" => Some(parse_edges_shorthand(raw)) => style.border_width = value;
    BorderRadius(Corners): "border-radius" => Some(parse_corners_shorthand(raw)) => style.border_radius = value;
    Width(Dimension): "width" => Some(parse_dimension(raw)) => style.width = value;
    Height(Dimension): "height" => Some(parse_dimension(raw)) => style.height = value;
    FlexBasis(Dimension): "flex-basis" => Some(parse_dimension(raw)) => style.flex_basis = value;
    MinWidth(Dimension): "min-width" => Some(parse_size_constraint(raw)) => style.min_width = value;
    MaxWidth(Dimension): "max-width" => Some(parse_size_constraint(raw)) => style.max_width = value;
    MinHeight(Dimension): "min-height" => Some(parse_size_constraint(raw)) => style.min_height = value;
    MaxHeight(Dimension): "max-height" => Some(parse_size_constraint(raw)) => style.max_height = value;
    FlexGrow(f32): "flex-grow" => Some(parse_px(raw)) => style.flex_grow = value;
    FlexShrink(f32): "flex-shrink" => Some(parse_px(raw)) => style.flex_shrink = value;
    PaddingInline(f32): "padding-x" | "padding-inline" => Some(parse_px(raw))
        => { style.padding.left = value; style.padding.right = value };
    PaddingBlock(f32): "padding-y" | "padding-block" => Some(parse_px(raw))
        => { style.padding.top = value; style.padding.bottom = value };
    BorderColor(Color): "border-color" => Some(parse_border_color_shorthand(raw))
        => style.border_color = value;
    Overflow(OverflowPair): "overflow" => Some(OverflowPair(parse_overflow_shorthand(raw)))
        => { style.overflow_x = value.0 .0; style.overflow_y = value.0 .1 };
    OverflowX(Overflow): "overflow-x" => Some(parse_overflow(raw)) => style.overflow_x = value;
    OverflowY(Overflow): "overflow-y" => Some(parse_overflow(raw)) => style.overflow_y = value;
    FlexWrapValue(FlexWrap): "flex-wrap" => Some(match raw {
        "wrap" => FlexWrap::Wrap,
        "wrap-reverse" => FlexWrap::WrapReverse,
        _ => FlexWrap::NoWrap,
    }) => style.flex_wrap = value;
    AlignSelfValue(AlignSelf): "align-self" => Some(match raw {
        "auto" => AlignSelf::Auto,
        "start" | "flex-start" => AlignSelf::Start,
        "end" | "flex-end" => AlignSelf::End,
        "center" => AlignSelf::Center,
        "baseline" => AlignSelf::Baseline,
        _ => AlignSelf::Stretch,
    }) => style.align_self = value;
    FlexDirectionValue(FlexDirection): "flex-direction" => Some(match raw {
        "column" | "column-reverse" => FlexDirection::Column,
        _ => FlexDirection::Row,
    }) => style.direction = value;
    JustifyContentValue(JustifyContent): "justify-content" => Some(match raw {
        "center" => JustifyContent::Center,
        "end" | "flex-end" => JustifyContent::End,
        "space-between" => JustifyContent::SpaceBetween,
        "space-around" => JustifyContent::SpaceAround,
        _ => JustifyContent::Start,
    }) => style.justify_content = value;
    AlignItemsValue(AlignItems): "align-items" => Some(match raw {
        "center" => AlignItems::Center,
        "start" | "flex-start" => AlignItems::Start,
        "end" | "flex-end" => AlignItems::End,
        _ => AlignItems::Stretch,
    }) => style.align_items = value;
    TextAlignValue(TextAlign): "text-align" => Some(match raw {
        "center" => TextAlign::Center,
        "right" => TextAlign::Right,
        _ => TextAlign::Left,
    }) => style.text_align = value;
    DisplayValue(Display): "display" => Some(match raw {
        "none" => Display::None,
        _ => Display::Flex,
    }) => style.display = value;
}

/// The `overflow` shorthand's two axes as one copyable operation value.
#[derive(Debug, Clone, Copy)]
pub(super) struct OverflowPair((Overflow, Overflow));

#[derive(Debug)]
pub(super) enum LiteralCache {
    Static(TypedLiteralOperation),
    Dynamic(std::sync::Mutex<Option<(u64, TypedLiteralOperation)>>),
}

impl LiteralCache {
    pub(super) fn new(operation: Option<TypedLiteralOperation>) -> Self {
        match operation {
            Some(operation) => Self::Static(operation),
            None => Self::Dynamic(std::sync::Mutex::new(None)),
        }
    }

    /// The theme-token operation cached for a theme revision, if any.
    pub(super) fn theme_operation(&self) -> Option<(u64, TypedLiteralOperation)> {
        match self {
            Self::Static(_) => None,
            Self::Dynamic(cache) => *cache.lock().unwrap(),
        }
    }

    pub(super) fn cache_theme(&self, revision: u64, operation: TypedLiteralOperation) {
        if let Self::Dynamic(cache) = self {
            *cache.lock().unwrap() = Some((revision, operation));
        }
    }

    #[cfg(test)]
    pub(super) fn is_static(&self) -> bool {
        matches!(self, Self::Static(_))
    }

    #[cfg(test)]
    pub(super) fn clear_theme(&self) {
        if let Self::Dynamic(cache) = self {
            *cache.lock().unwrap() = None;
        }
    }
}

impl Clone for LiteralCache {
    fn clone(&self) -> Self {
        match self {
            Self::Static(operation) => Self::Static(*operation),
            Self::Dynamic(cache) => Self::Dynamic(std::sync::Mutex::new(*cache.lock().unwrap())),
        }
    }
}

pub(super) fn typed_literal_value(
    property: &str,
    value: &StyleValue,
) -> Option<TypedLiteralOperation> {
    let StyleValue::Literal(value) = value else {
        return None;
    };
    LiteralProperty::from_name(property)?.lower(value)
}
