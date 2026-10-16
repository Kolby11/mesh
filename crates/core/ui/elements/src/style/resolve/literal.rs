use super::index::references_style_function;
use crate::style::parse::*;
use crate::style::*;
use mesh_core_component::style::StyleValue;

macro_rules! literal_properties {
    ($raw:ident; $($variant:ident($ty:ident): $($name:literal)|+ => $parse:expr => $field:ident $(.$part:ident)?;)*) => {
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

        pub(super) fn apply_typed_literal(style: &mut ComputedStyle, operation: TypedLiteralOperation) {
            match operation { $(TypedLiteralOperation::$variant(value) => style.$field $(.$part)? = value,)* }
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

literal_properties! { raw;
    Background(Color): "background" | "background-color" => Color::from_css(raw) => background_color;
    Color(Color): "color" => Color::from_css(raw) => color;
    FontSize(f32): "font-size" => Some(parse_px(raw)) => font_size;
    FontWeight(u16): "font-weight" => Some(parse_px(raw) as u16) => font_weight;
    LetterSpacing(f32): "letter-spacing" => Some(parse_px(raw)) => letter_spacing;
    LineHeight(f32): "line-height" => Some(parse_px(raw)) => line_height;
    Gap(f32): "gap" | "column-gap" | "row-gap" | "gap-x" => Some(parse_px(raw)) => gap;
    Opacity(f32): "opacity" => Some(parse_px(raw)) => opacity;
    PaddingTop(f32): "padding-top" => Some(parse_px(raw)) => padding.top;
    PaddingRight(f32): "padding-right" => Some(parse_px(raw)) => padding.right;
    PaddingBottom(f32): "padding-bottom" => Some(parse_px(raw)) => padding.bottom;
    PaddingLeft(f32): "padding-left" => Some(parse_px(raw)) => padding.left;
    MarginTop(f32): "margin-top" => Some(parse_px(raw)) => margin.top;
    MarginRight(f32): "margin-right" => Some(parse_px(raw)) => margin.right;
    MarginBottom(f32): "margin-bottom" => Some(parse_px(raw)) => margin.bottom;
    MarginLeft(f32): "margin-left" => Some(parse_px(raw)) => margin.left;
    BorderTopWidth(f32): "border-top-width" => Some(parse_px(raw)) => border_width.top;
    BorderRightWidth(f32): "border-right-width" => Some(parse_px(raw)) => border_width.right;
    BorderBottomWidth(f32): "border-bottom-width" => Some(parse_px(raw)) => border_width.bottom;
    BorderLeftWidth(f32): "border-left-width" => Some(parse_px(raw)) => border_width.left;
    Padding(Edges): "padding" => Some(parse_edges_shorthand(raw)) => padding;
    Margin(Edges): "margin" => Some(parse_edges_shorthand(raw)) => margin;
    BorderWidth(Edges): "border-width" => Some(parse_edges_shorthand(raw)) => border_width;
    BorderRadius(Corners): "border-radius" => Some(parse_corners_shorthand(raw)) => border_radius;
    Width(Dimension): "width" => Some(parse_dimension(raw)) => width;
    Height(Dimension): "height" => Some(parse_dimension(raw)) => height;
    FlexBasis(Dimension): "flex-basis" => Some(parse_dimension(raw)) => flex_basis;
    MinWidth(Dimension): "min-width" => Some(parse_size_constraint(raw)) => min_width;
    MaxWidth(Dimension): "max-width" => Some(parse_size_constraint(raw)) => max_width;
    MinHeight(Dimension): "min-height" => Some(parse_size_constraint(raw)) => min_height;
    MaxHeight(Dimension): "max-height" => Some(parse_size_constraint(raw)) => max_height;
}

#[derive(Debug, Clone, Copy)]
pub(super) enum CachedLiteral {
    Static(TypedLiteralOperation),
    Theme {
        revision: u64,
        operation: TypedLiteralOperation,
    },
}

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

    pub(super) fn get(&self) -> Option<CachedLiteral> {
        match self {
            Self::Static(operation) => Some(CachedLiteral::Static(*operation)),
            Self::Dynamic(cache) => {
                cache
                    .lock()
                    .unwrap()
                    .map(|(revision, operation)| CachedLiteral::Theme {
                        revision,
                        operation,
                    })
            }
        }
    }

    pub(super) fn cache_theme(&self, revision: u64, operation: TypedLiteralOperation) {
        if let Self::Dynamic(cache) = self {
            *cache.lock().unwrap() = Some((revision, operation));
        }
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
