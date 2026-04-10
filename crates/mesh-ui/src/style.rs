/// Style resolution — converts style AST + theme tokens into computed styles.
use mesh_component::style::{StyleRule, StyleValue, Selector};
use mesh_theme::{Theme, TokenValue};

/// Fully resolved style for a widget node.
#[derive(Debug, Clone)]
pub struct ComputedStyle {
    // Box model
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
    pub padding: Edges,
    pub margin: Edges,
    pub border_width: Edges,

    // Visual
    pub background_color: Color,
    pub border_color: Color,
    pub border_radius: Corners,
    pub opacity: f32,

    // Text
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub color: Color,
    pub text_align: TextAlign,
    pub line_height: f32,

    // Layout
    pub display: Display,
    pub direction: FlexDirection,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub gap: f32,
    pub flex_grow: f32,
    pub flex_shrink: f32,
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
            font_family: "Inter".to_string(),
            font_size: 14.0,
            font_weight: 400,
            color: Color::WHITE,
            text_align: TextAlign::Left,
            line_height: 1.4,
            display: Display::Flex,
            direction: FlexDirection::Row,
            justify_content: JustifyContent::Start,
            align_items: AlignItems::Stretch,
            gap: 0.0,
            flex_grow: 0.0,
            flex_shrink: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Dimension {
    Auto,
    Px(f32),
    Percent(f32),
}

#[derive(Debug, Clone, Copy)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub fn zero() -> Self {
        Self { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 }
    }

    pub fn all(value: f32) -> Self {
        Self { top: value, right: value, bottom: value, left: value }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Corners {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl Corners {
    pub fn zero() -> Self {
        Self { top_left: 0.0, top_right: 0.0, bottom_right: 0.0, bottom_left: 0.0 }
    }

    pub fn all(value: f32) -> Self {
        Self { top_left: value, top_right: value, bottom_right: value, bottom_left: value }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self { r: 0, g: 0, b: 0, a: 0 };
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255, a: 255 };
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };

    /// Parse a hex color string: `#RGB`, `#RRGGBB`, or `#RRGGBBAA`.
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display { Flex, None }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection { Row, Column }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent { Start, End, Center, SpaceBetween, SpaceAround }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems { Start, End, Center, Stretch }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign { Left, Center, Right }

/// Resolves style values against a theme's design tokens.
pub struct StyleResolver<'a> {
    theme: &'a Theme,
}

impl<'a> StyleResolver<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self { theme }
    }

    /// Resolve a `StyleValue` to a concrete string.
    pub fn resolve_value(&self, value: &StyleValue) -> String {
        match value {
            StyleValue::Literal(s) => s.clone(),
            StyleValue::Token(name) => {
                match self.theme.token(name) {
                    Some(TokenValue::String(s)) => s.clone(),
                    Some(TokenValue::Number(n)) => format!("{n}"),
                    Some(TokenValue::Bool(b)) => format!("{b}"),
                    None => {
                        tracing::warn!("unresolved theme token: {name}");
                        String::new()
                    }
                }
            }
            StyleValue::Var(name) => {
                tracing::debug!("var({name}) not yet resolved");
                String::new()
            }
        }
    }

    /// Resolve a color value (hex string or theme token).
    pub fn resolve_color(&self, value: &StyleValue) -> Color {
        let resolved = self.resolve_value(value);
        Color::from_hex(&resolved).unwrap_or(Color::TRANSPARENT)
    }

    /// Resolve a numeric value (px or theme token).
    pub fn resolve_number(&self, value: &StyleValue) -> f32 {
        let resolved = self.resolve_value(value);
        parse_px(&resolved)
    }

    /// Apply a set of style rules to produce a `ComputedStyle` for a node.
    pub fn resolve_node_style(
        &self,
        rules: &[StyleRule],
        tag: &str,
        classes: &[String],
        id: Option<&str>,
    ) -> ComputedStyle {
        let mut style = ComputedStyle::default();

        // Apply default direction based on tag.
        if tag == "column" {
            style.direction = FlexDirection::Column;
        }

        // Apply matching rules in order.
        for rule in rules {
            if selector_matches(&rule.selector, tag, classes, id) {
                for decl in &rule.declarations {
                    apply_declaration(&mut style, &decl.property, &decl.value, self);
                }
            }
        }

        style
    }
}

fn selector_matches(selector: &Selector, tag: &str, classes: &[String], id: Option<&str>) -> bool {
    match selector {
        Selector::Universal => true,
        Selector::Tag(t) => t == tag,
        Selector::Class(c) => classes.iter().any(|cls| cls == c),
        Selector::Id(i) => id == Some(i.as_str()),
        Selector::State(t, _state) => t == tag, // State matching is simplified for now.
        Selector::Compound(parts) => parts.iter().all(|s| selector_matches(s, tag, classes, id)),
    }
}

fn apply_declaration(style: &mut ComputedStyle, property: &str, value: &StyleValue, resolver: &StyleResolver) {
    match property {
        "background" | "background-color" => style.background_color = resolver.resolve_color(value),
        "color" => style.color = resolver.resolve_color(value),
        "border-color" => style.border_color = resolver.resolve_color(value),
        "font-size" => style.font_size = resolver.resolve_number(value),
        "font-weight" => style.font_weight = resolver.resolve_number(value) as u16,
        "font-family" => style.font_family = resolver.resolve_value(value),
        "line-height" => style.line_height = resolver.resolve_number(value),
        "padding" => style.padding = Edges::all(resolver.resolve_number(value)),
        "margin" => style.margin = Edges::all(resolver.resolve_number(value)),
        "gap" => style.gap = resolver.resolve_number(value),
        "border-radius" => style.border_radius = Corners::all(resolver.resolve_number(value)),
        "border-width" => style.border_width = Edges::all(resolver.resolve_number(value)),
        "opacity" => style.opacity = resolver.resolve_number(value),
        "width" => style.width = parse_dimension(&resolver.resolve_value(value)),
        "height" => style.height = parse_dimension(&resolver.resolve_value(value)),
        "flex-grow" => style.flex_grow = resolver.resolve_number(value),
        "flex-shrink" => style.flex_shrink = resolver.resolve_number(value),
        "direction" | "flex-direction" => {
            style.direction = match resolver.resolve_value(value).as_str() {
                "column" => FlexDirection::Column,
                _ => FlexDirection::Row,
            };
        }
        "justify-content" => {
            style.justify_content = match resolver.resolve_value(value).as_str() {
                "center" => JustifyContent::Center,
                "end" | "flex-end" => JustifyContent::End,
                "space-between" => JustifyContent::SpaceBetween,
                "space-around" => JustifyContent::SpaceAround,
                _ => JustifyContent::Start,
            };
        }
        "align-items" => {
            style.align_items = match resolver.resolve_value(value).as_str() {
                "center" => AlignItems::Center,
                "start" | "flex-start" => AlignItems::Start,
                "end" | "flex-end" => AlignItems::End,
                _ => AlignItems::Stretch,
            };
        }
        "text-align" => {
            style.text_align = match resolver.resolve_value(value).as_str() {
                "center" => TextAlign::Center,
                "right" => TextAlign::Right,
                _ => TextAlign::Left,
            };
        }
        "display" => {
            style.display = match resolver.resolve_value(value).as_str() {
                "none" => Display::None,
                _ => Display::Flex,
            };
        }
        _ => {
            tracing::debug!("unknown style property: {property}");
        }
    }
}

fn parse_px(s: &str) -> f32 {
    let s = s.trim().trim_end_matches("px");
    s.parse().unwrap_or(0.0)
}

fn parse_dimension(s: &str) -> Dimension {
    let s = s.trim();
    if s == "auto" {
        Dimension::Auto
    } else if let Some(pct) = s.strip_suffix('%') {
        Dimension::Percent(pct.parse().unwrap_or(0.0))
    } else {
        Dimension::Px(parse_px(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_colors() {
        assert_eq!(Color::from_hex("#fff"), Some(Color { r: 255, g: 255, b: 255, a: 255 }));
        assert_eq!(Color::from_hex("#6750A4"), Some(Color { r: 103, g: 80, b: 164, a: 255 }));
        assert_eq!(Color::from_hex("#00000080"), Some(Color { r: 0, g: 0, b: 0, a: 128 }));
        assert_eq!(Color::from_hex("invalid"), None);
    }

    #[test]
    fn resolve_theme_token() {
        let theme = mesh_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let value = StyleValue::Token("color.primary".to_string());
        let resolved = resolver.resolve_value(&value);
        assert_eq!(resolved, "#6750A4");
    }

    #[test]
    fn resolve_node_style_from_rules() {
        let theme = mesh_theme::default_theme();
        let resolver = StyleResolver::new(&theme);

        let rules = vec![StyleRule {
            selector: Selector::Tag("text".to_string()),
            declarations: vec![
                mesh_component::style::Declaration {
                    property: "font-size".to_string(),
                    value: StyleValue::Literal("20px".to_string()),
                },
                mesh_component::style::Declaration {
                    property: "color".to_string(),
                    value: StyleValue::Token("color.primary".to_string()),
                },
            ],
        }];

        let style = resolver.resolve_node_style(&rules, "text", &[], None);
        assert_eq!(style.font_size, 20.0);
        assert_eq!(style.color, Color { r: 103, g: 80, b: 164, a: 255 });
    }
}
