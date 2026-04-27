/// Style resolution — converts style AST + theme tokens into computed styles.
use crate::tree::ElementState;
use mesh_component::style::{Selector, StyleRule, StyleValue};
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
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,

    // Text
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub color: Color,
    pub text_align: TextAlign,
    pub line_height: f32,

    // Text extended
    pub font_style: FontStyle,
    pub letter_spacing: f32,
    pub text_overflow: TextOverflow,
    pub text_direction: TextDirection,

    // Layout
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

    // Positioning
    pub position: Position,
    pub z_index: i32,
    pub inset_top: Option<f32>,
    pub inset_right: Option<f32>,
    pub inset_bottom: Option<f32>,
    pub inset_left: Option<f32>,
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
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Dimension {
    Auto,
    Px(f32),
    Percent(f32),
    /// Shrink-wrap to intrinsic content size. Maps from `content`, `fit-content`,
    /// `max-content`, `min-content` in CSS. Resolved to `Px` before children are
    /// laid out, so children always see a concrete available size.
    Content,
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

#[derive(Debug, Clone, Copy)]
pub struct Corners {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
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

/// Base text direction for a node and its subtree.
///
/// Affects default text alignment and flex main-axis start/end semantics.
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
    /// Normal flow — the default.
    #[default]
    Static,
    /// In flow but offset by top/left/right/bottom.
    Relative,
    /// Out of flow; positioned against the nearest containing block.
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
            StyleValue::Token(name) => match self.theme.token(name) {
                Some(TokenValue::String(s)) => s.clone(),
                Some(TokenValue::Number(n)) => format!("{n}"),
                Some(TokenValue::Bool(b)) => format!("{b}"),
                None => {
                    tracing::warn!("unresolved theme token: {name}");
                    String::new()
                }
            },
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
        context: StyleContext,
        state: ElementState,
    ) -> ComputedStyle {
        let mut style = ComputedStyle::default();

        // Apply default direction based on tag.
        if tag == "column" {
            style.direction = FlexDirection::Column;
        }

        // Apply matching rules in order.
        for rule in rules {
            if rule_matches(rule, tag, classes, id, context, state) {
                for decl in &rule.declarations {
                    apply_declaration(&mut style, &decl.property, &decl.value, self);
                }
            }
        }

        style
    }

    /// Re-resolve computed styles for every node in a subtree using each node's
    /// current `ElementState`. Call this after `InputState::process` changes
    /// hover/focus/active flags to apply pseudo-class rules without rebuilding
    /// the entire component tree.
    pub fn restyle_subtree(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
    ) {
        // Only overlay pseudo-state rules — the base style was already computed by
        // build_element_node (which applied non-state rules, tag defaults, and inherited
        // styles). Recomputing from scratch here would wipe all of that.
        if node.state != ElementState::default() {
            let classes: Vec<String> = node
                .attributes
                .get("class")
                .map(|s| s.split_whitespace().map(str::to_owned).collect())
                .unwrap_or_default();
            let id = node.attributes.get("id").map(|s| s.as_str());
            let state = node.state;

            tracing::debug!(
                "[hover] restyle: tag={} classes={:?} state={state:?}",
                node.tag,
                classes
            );
            for rule in rules {
                if selector_involves_state(&rule.selector)
                    && rule_matches(rule, &node.tag, &classes, id, context, state)
                {
                    tracing::debug!(
                        "[hover] restyle: applying rule selector={:?}",
                        rule.selector
                    );
                    for decl in &rule.declarations {
                        apply_declaration(
                            &mut node.computed_style,
                            &decl.property,
                            &decl.value,
                            self,
                        );
                    }
                }
            }
        }

        let children = std::mem::take(&mut node.children);
        let mut restyled = children;
        for child in &mut restyled {
            self.restyle_subtree(child, rules, context);
        }
        node.children = restyled;
    }
}

fn selector_matches(
    selector: &Selector,
    tag: &str,
    classes: &[String],
    id: Option<&str>,
    state: ElementState,
) -> bool {
    match selector {
        Selector::Universal => true,
        Selector::Tag(t) => t == tag,
        Selector::Class(c) => classes.iter().any(|cls| cls == c),
        Selector::Id(i) => id == Some(i.as_str()),
        Selector::State(t, pseudo) => {
            let tag_matches = t == "*" || t == tag;
            let state_matches = match pseudo.as_str() {
                "hover" | "hovered" => state.hovered,
                "focus" | "focused" => state.focused,
                "active" => state.active,
                "disabled" => state.disabled,
                "checked" => state.checked,
                "focus-visible" => state.focused,
                _ => false,
            };
            tag_matches && state_matches
        }
        Selector::Compound(parts) => parts
            .iter()
            .all(|s| selector_matches(s, tag, classes, id, state)),
    }
}

fn selector_involves_state(selector: &Selector) -> bool {
    match selector {
        Selector::State(_, _) => true,
        Selector::Compound(parts) => parts.iter().any(|s| matches!(s, Selector::State(_, _))),
        _ => false,
    }
}

fn rule_matches(
    rule: &StyleRule,
    tag: &str,
    classes: &[String],
    id: Option<&str>,
    context: StyleContext,
    state: ElementState,
) -> bool {
    selector_matches(&rule.selector, tag, classes, id, state)
        && rule
            .container_query
            .is_none_or(|query| query.matches(context.container_width, context.container_height))
}

fn apply_declaration(
    style: &mut ComputedStyle,
    property: &str,
    value: &StyleValue,
    resolver: &StyleResolver,
) {
    match property {
        "background" | "background-color" => style.background_color = resolver.resolve_color(value),
        "color" => style.color = resolver.resolve_color(value),
        "border-color" => style.border_color = resolver.resolve_color(value),
        "font-size" => style.font_size = resolver.resolve_number(value),
        "font-weight" => style.font_weight = resolver.resolve_number(value) as u16,
        "font-family" => style.font_family = resolver.resolve_value(value),
        "font-style" => {
            style.font_style = match resolver.resolve_value(value).as_str() {
                "italic" | "oblique" => FontStyle::Italic,
                _ => FontStyle::Normal,
            };
        }
        "letter-spacing" => style.letter_spacing = resolver.resolve_number(value),
        "text-overflow" => {
            style.text_overflow = match resolver.resolve_value(value).as_str() {
                "ellipsis" => TextOverflow::Ellipsis,
                _ => TextOverflow::Clip,
            };
        }
        "line-height" => style.line_height = resolver.resolve_number(value),
        "padding" => style.padding = Edges::all(resolver.resolve_number(value)),
        "padding-top" => style.padding.top = resolver.resolve_number(value),
        "padding-right" => style.padding.right = resolver.resolve_number(value),
        "padding-bottom" => style.padding.bottom = resolver.resolve_number(value),
        "padding-left" => style.padding.left = resolver.resolve_number(value),
        "padding-x" | "padding-inline" => {
            let v = resolver.resolve_number(value);
            style.padding.left = v;
            style.padding.right = v;
        }
        "padding-y" | "padding-block" => {
            let v = resolver.resolve_number(value);
            style.padding.top = v;
            style.padding.bottom = v;
        }
        "margin" => style.margin = Edges::all(resolver.resolve_number(value)),
        "margin-top" => style.margin.top = resolver.resolve_number(value),
        "margin-right" => style.margin.right = resolver.resolve_number(value),
        "margin-bottom" => style.margin.bottom = resolver.resolve_number(value),
        "margin-left" => style.margin.left = resolver.resolve_number(value),
        "margin-x" | "margin-inline" => {
            let v = resolver.resolve_number(value);
            style.margin.left = v;
            style.margin.right = v;
        }
        "margin-y" | "margin-block" => {
            let v = resolver.resolve_number(value);
            style.margin.top = v;
            style.margin.bottom = v;
        }
        "gap" => style.gap = resolver.resolve_number(value),
        "column-gap" | "gap-x" => style.gap = resolver.resolve_number(value),
        "border-radius" => style.border_radius = Corners::all(resolver.resolve_number(value)),
        "border-top-left-radius" => style.border_radius.top_left = resolver.resolve_number(value),
        "border-top-right-radius" => style.border_radius.top_right = resolver.resolve_number(value),
        "border-bottom-right-radius" => {
            style.border_radius.bottom_right = resolver.resolve_number(value)
        }
        "border-bottom-left-radius" => {
            style.border_radius.bottom_left = resolver.resolve_number(value)
        }
        "border-width" => style.border_width = Edges::all(resolver.resolve_number(value)),
        "border-top-width" => style.border_width.top = resolver.resolve_number(value),
        "border-right-width" => style.border_width.right = resolver.resolve_number(value),
        "border-bottom-width" => style.border_width.bottom = resolver.resolve_number(value),
        "border-left-width" => style.border_width.left = resolver.resolve_number(value),
        "opacity" => style.opacity = resolver.resolve_number(value),
        "overflow" => {
            let overflow = parse_overflow(&resolver.resolve_value(value));
            style.overflow_x = overflow;
            style.overflow_y = overflow;
        }
        "overflow-x" => style.overflow_x = parse_overflow(&resolver.resolve_value(value)),
        "overflow-y" => style.overflow_y = parse_overflow(&resolver.resolve_value(value)),
        "width" => style.width = parse_dimension(&resolver.resolve_value(value)),
        "height" => style.height = parse_dimension(&resolver.resolve_value(value)),
        "min-width" => style.min_width = Some(resolver.resolve_number(value)),
        "max-width" => style.max_width = Some(resolver.resolve_number(value)),
        "min-height" => style.min_height = Some(resolver.resolve_number(value)),
        "max-height" => style.max_height = Some(resolver.resolve_number(value)),
        "flex-grow" => style.flex_grow = resolver.resolve_number(value),
        "flex-shrink" => style.flex_shrink = resolver.resolve_number(value),
        "flex-basis" => style.flex_basis = parse_dimension(&resolver.resolve_value(value)),
        "flex" => {
            let v = resolver.resolve_value(value);
            let v = v.trim();
            if v == "none" {
                style.flex_grow = 0.0;
                style.flex_shrink = 0.0;
                style.flex_basis = Dimension::Auto;
            } else if v == "auto" {
                style.flex_grow = 1.0;
                style.flex_shrink = 1.0;
                style.flex_basis = Dimension::Auto;
            } else if let Ok(n) = v.parse::<f32>() {
                style.flex_grow = n;
                style.flex_shrink = 1.0;
                style.flex_basis = Dimension::Px(0.0);
            }
        }
        "flex-wrap" => {
            style.flex_wrap = match resolver.resolve_value(value).as_str() {
                "wrap" => FlexWrap::Wrap,
                "wrap-reverse" => FlexWrap::WrapReverse,
                _ => FlexWrap::NoWrap,
            };
        }
        "align-self" => {
            style.align_self = match resolver.resolve_value(value).as_str() {
                "auto" => AlignSelf::Auto,
                "start" | "flex-start" => AlignSelf::Start,
                "end" | "flex-end" => AlignSelf::End,
                "center" => AlignSelf::Center,
                "baseline" => AlignSelf::Baseline,
                _ => AlignSelf::Stretch,
            };
        }
        "align-content" => {
            style.align_content = match resolver.resolve_value(value).as_str() {
                "start" | "flex-start" => AlignContent::Start,
                "end" | "flex-end" => AlignContent::End,
                "center" => AlignContent::Center,
                "space-between" => AlignContent::SpaceBetween,
                "space-around" => AlignContent::SpaceAround,
                _ => AlignContent::Stretch,
            };
        }
        "flex-direction" => {
            style.direction = match resolver.resolve_value(value).as_str() {
                "column" | "column-reverse" => FlexDirection::Column,
                _ => FlexDirection::Row,
            };
        }
        "direction" => {
            match resolver.resolve_value(value).as_str() {
                "rtl" => style.text_direction = TextDirection::Rtl,
                "ltr" => style.text_direction = TextDirection::Ltr,
                // Legacy alias kept for .mesh files that used direction: column/row
                // before flex-direction was introduced. Warn and ignore.
                other => tracing::warn!(
                    "direction: {other} is not valid; use flex-direction for layout direction"
                ),
            }
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
        "position" => {
            style.position = match resolver.resolve_value(value).as_str() {
                "relative" => Position::Relative,
                "absolute" => Position::Absolute,
                _ => Position::Static,
            };
        }
        "z-index" => {
            let v = resolver.resolve_value(value);
            style.z_index = v.trim().parse::<i32>().unwrap_or(0);
        }
        "top" => style.inset_top = Some(resolver.resolve_number(value)),
        "right" => style.inset_right = Some(resolver.resolve_number(value)),
        "bottom" => style.inset_bottom = Some(resolver.resolve_number(value)),
        "left" => style.inset_left = Some(resolver.resolve_number(value)),
        "inset" => {
            let v = resolver.resolve_number(value);
            style.inset_top = Some(v);
            style.inset_right = Some(v);
            style.inset_bottom = Some(v);
            style.inset_left = Some(v);
        }
        _ => {
            tracing::debug!("unknown style property: {property}");
        }
    }
}

fn parse_overflow(value: &str) -> Overflow {
    match value.trim() {
        "hidden" => Overflow::Hidden,
        "auto" => Overflow::Auto,
        "scroll" => Overflow::Scroll,
        _ => Overflow::Visible,
    }
}

fn parse_px(s: &str) -> f32 {
    let s = s.trim().trim_end_matches("px");
    s.parse().unwrap_or(0.0)
}

fn parse_dimension(s: &str) -> Dimension {
    let s = s.trim();
    match s {
        "auto" => Dimension::Auto,
        "content" | "fit-content" | "max-content" | "min-content" => Dimension::Content,
        _ if s.ends_with('%') => Dimension::Percent(s.trim_end_matches('%').parse().unwrap_or(0.0)),
        _ => Dimension::Px(parse_px(s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_colors() {
        assert_eq!(
            Color::from_hex("#fff"),
            Some(Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255
            })
        );
        assert_eq!(
            Color::from_hex("#6750A4"),
            Some(Color {
                r: 103,
                g: 80,
                b: 164,
                a: 255
            })
        );
        assert_eq!(
            Color::from_hex("#00000080"),
            Some(Color {
                r: 0,
                g: 0,
                b: 0,
                a: 128
            })
        );
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
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "text",
            &[],
            None,
            StyleContext::default(),
            ElementState::default(),
        );
        assert_eq!(style.font_size, 20.0);
        assert_eq!(
            style.color,
            Color {
                r: 103,
                g: 80,
                b: 164,
                a: 255
            }
        );
    }

    #[test]
    fn container_query_rules_apply_against_context() {
        let theme = mesh_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![mesh_component::style::Declaration {
                property: "overflow-y".to_string(),
                value: StyleValue::Literal("auto".to_string()),
            }],
            container_query: Some(mesh_component::style::ContainerQuery {
                min_width: Some(480.0),
                ..Default::default()
            }),
        }];

        let narrow = resolver.resolve_node_style(
            &rules,
            "column",
            &["panel".into()],
            None,
            StyleContext {
                container_width: 320.0,
                container_height: 240.0,
            },
            ElementState::default(),
        );
        assert_eq!(narrow.overflow_y, Overflow::Visible);

        let wide = resolver.resolve_node_style(
            &rules,
            "column",
            &["panel".into()],
            None,
            StyleContext {
                container_width: 640.0,
                container_height: 240.0,
            },
            ElementState::default(),
        );
        assert_eq!(wide.overflow_y, Overflow::Auto);
    }

    #[test]
    fn pseudo_state_rules_apply_when_state_matches() {
        use crate::tree::ElementState;
        use mesh_component::style::{Declaration, Selector};

        let theme = mesh_theme::default_theme();
        let resolver = StyleResolver::new(&theme);

        let rules = vec![
            StyleRule {
                selector: Selector::Tag("button".to_string()),
                declarations: vec![Declaration {
                    property: "background-color".to_string(),
                    value: StyleValue::Literal("#333333".to_string()),
                }],
                container_query: None,
            },
            StyleRule {
                selector: Selector::State("button".to_string(), "hover".to_string()),
                declarations: vec![Declaration {
                    property: "background-color".to_string(),
                    value: StyleValue::Literal("#ffffff".to_string()),
                }],
                container_query: None,
            },
        ];

        let idle = resolver.resolve_node_style(
            &rules,
            "button",
            &[],
            None,
            StyleContext::default(),
            ElementState::default(),
        );
        assert_eq!(idle.background_color, Color::from_hex("#333333").unwrap());

        let hovered = resolver.resolve_node_style(
            &rules,
            "button",
            &[],
            None,
            StyleContext::default(),
            ElementState {
                hovered: true,
                ..Default::default()
            },
        );
        assert_eq!(
            hovered.background_color,
            Color::from_hex("#ffffff").unwrap()
        );
    }

    #[test]
    fn input_state_sets_hover_flags_on_nodes() {
        use crate::events::{InputState, RawInputEvent, UiEvent};
        use crate::layout::LayoutEngine;
        use crate::style::Dimension;
        use crate::tree::WidgetNode;

        let mut root = WidgetNode::new("root");
        root.computed_style.width = Dimension::Px(200.0);
        root.computed_style.height = Dimension::Px(100.0);

        let mut btn = WidgetNode::new("button");
        btn.computed_style.width = Dimension::Px(100.0);
        btn.computed_style.height = Dimension::Px(50.0);
        let btn_id = btn.id;
        root.children = vec![btn];
        LayoutEngine::compute(&mut root, 200.0, 100.0);

        let mut input = InputState::new();

        // Move pointer over the button.
        let events = input.process(
            &mut root,
            &RawInputEvent::PointerMotion { x: 50.0, y: 25.0 },
        );
        assert!(root.children[0].state.hovered, "button should be hovered");
        assert!(!root.state.hovered, "root should not be hovered");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, UiEvent::PointerEnter { node_id } if *node_id == btn_id))
        );

        // Move pointer off the button onto the root.
        let events = input.process(
            &mut root,
            &RawInputEvent::PointerMotion { x: 150.0, y: 75.0 },
        );
        assert!(
            !root.children[0].state.hovered,
            "button hover should be cleared"
        );
        assert!(root.state.hovered, "root should now be hovered");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, UiEvent::PointerLeave { node_id } if *node_id == btn_id))
        );
    }

    #[test]
    fn padding_inline_and_block_tokens_resolve_to_computed_edges() {
        use mesh_component::parser::parse_component;

        let source = r#"
<style>
.panel {
    padding-inline: token(spacing.lg);
    padding-block: token(spacing.sm);
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let rules = file.style.unwrap().rules;

        let theme = mesh_theme::default_theme();
        let resolver = StyleResolver::new(&theme);

        let style = resolver.resolve_node_style(
            &rules,
            "div",
            &["panel".to_owned()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        // spacing.lg = 24, spacing.sm = 8
        assert_eq!(style.padding.left, 24.0, "padding-inline left");
        assert_eq!(style.padding.right, 24.0, "padding-inline right");
        assert_eq!(style.padding.top, 8.0, "padding-block top");
        assert_eq!(style.padding.bottom, 8.0, "padding-block bottom");
    }

    #[test]
    fn padding_shorthand_and_overrides_resolve_correctly() {
        use mesh_component::parser::parse_component;

        let source = r#"
<style>
.card {
    padding: 16px;
    padding-top: 4px;
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let rules = file.style.unwrap().rules;

        let theme = mesh_theme::default_theme();
        let resolver = StyleResolver::new(&theme);

        let style = resolver.resolve_node_style(
            &rules,
            "div",
            &["card".to_owned()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.padding.top, 4.0, "padding-top override");
        assert_eq!(style.padding.right, 16.0, "shorthand right");
        assert_eq!(style.padding.bottom, 16.0, "shorthand bottom");
        assert_eq!(style.padding.left, 16.0, "shorthand left");
    }
}
