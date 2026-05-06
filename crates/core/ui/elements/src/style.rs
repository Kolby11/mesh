/// Style resolution — converts style AST + theme tokens into computed styles.
use crate::tree::ElementState;
use mesh_core_component::style::{Selector, StyleRule, StyleValue};
use mesh_core_theme::{Theme, TokenValue};
use std::collections::HashMap;

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
    pub transform: Transform2D,
    pub transition: TransitionStyle,
    pub animation: AnimationStyle,
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dimension {
    Auto,
    Px(f32),
    Percent(f32),
    /// Shrink-wrap to intrinsic content size. Maps from `content`, `fit-content`,
    /// `max-content`, `min-content` in CSS. Resolved to `Px` before children are
    /// laid out, so children always see a concrete available size.
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
    pub padding: bool,
    pub margin: bool,
    pub transform: bool,
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
            padding: true,
            margin: true,
            transform: true,
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
}

/// Decomposed 2D affine transform.
///
/// Stored as separate translate / scale / rotation components rather than a
/// 3x2 matrix because that's the form interpolation needs — lerping a matrix
/// directly produces shear artifacts. The painter and hit-test pipelines
/// recompose at the point of use.
///
/// `rotation` is in radians.
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
    /// Custom cubic-bezier curve, parameters are `(x1, y1, x2, y2)`.
    /// Themeable via `transition-timing-function: token(motion.easing.<name>)`.
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
        self.resolve_value_with_variables(value, &HashMap::new())
    }

    fn resolve_value_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> String {
        match value {
            StyleValue::Literal(s) => resolve_embedded_tokens(s, self.theme),
            StyleValue::Token(name) => match self.theme.token(name) {
                Some(TokenValue::String(s)) => s.clone(),
                Some(TokenValue::Number(n)) => format!("{n}"),
                Some(TokenValue::Bool(b)) => format!("{b}"),
                None => {
                    tracing::warn!("unresolved theme token: {name}");
                    String::new()
                }
            },
            StyleValue::Var(name) => variables
                .get(name)
                .map(|value| self.resolve_value_with_variables(value, variables))
                .unwrap_or_default(),
        }
    }

    fn resolve_color_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> Color {
        let resolved = self.resolve_value_with_variables(value, variables);
        Color::from_hex(&resolved).unwrap_or(Color::TRANSPARENT)
    }

    fn resolve_number_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> f32 {
        parse_px(&self.resolve_value_with_variables(value, variables))
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

    pub fn resolve_time_ms(&self, value: &StyleValue) -> u32 {
        let resolved = self.resolve_value(value);
        parse_time_ms(&resolved)
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
        self.resolve_node_style_with_diagnostics(rules, tag, classes, id, context, state)
            .0
    }

    pub fn resolve_node_style_with_diagnostics(
        &self,
        rules: &[StyleRule],
        tag: &str,
        classes: &[String],
        id: Option<&str>,
        context: StyleContext,
        state: ElementState,
    ) -> (ComputedStyle, Vec<StyleDiagnostic>) {
        let mut style = ComputedStyle::default();
        let mut diagnostics = Vec::new();
        let mut variables = HashMap::new();

        // Apply default direction based on tag.
        if tag == "column" {
            style.direction = FlexDirection::Column;
        }

        // Apply matching rules in order.
        for rule in rules {
            if rule_matches(rule, tag, classes, id, context, state) {
                for decl in &rule.declarations {
                    if decl.property.starts_with("--") {
                        variables.insert(decl.property.clone(), decl.value.clone());
                        continue;
                    }
                    if !is_supported_css_property(&decl.property) {
                        diagnostics.push(StyleDiagnostic {
                            property: decl.property.clone(),
                            selector: Some(selector_to_diagnostic_string(&rule.selector)),
                            message: format!("unsupported CSS property '{}'", decl.property),
                        });
                        continue;
                    }
                    if let StyleValue::Var(name) = &decl.value
                        && !variables.contains_key(name)
                    {
                        diagnostics.push(StyleDiagnostic {
                            property: decl.property.clone(),
                            selector: Some(selector_to_diagnostic_string(&rule.selector)),
                            message: format!(
                                "unsupported CSS variable reference '{name}' for property '{}'",
                                decl.property
                            ),
                        });
                    }
                    apply_declaration(&mut style, &decl.property, &decl.value, self, &variables);
                }
            }
        }

        (style, diagnostics)
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
                            &HashMap::new(),
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
                "focus-visible" => state.focus_visible,
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
    variables: &HashMap<String, StyleValue>,
) {
    match property {
        "background" | "background-color" => {
            style.background_color = resolver.resolve_color_with_variables(value, variables)
        }
        "color" => style.color = resolver.resolve_color_with_variables(value, variables),
        "border" => apply_border_shorthand(
            style,
            &resolver.resolve_value_with_variables(value, variables),
        ),
        "border-color" => {
            style.border_color = parse_border_color_shorthand(
                &resolver.resolve_value_with_variables(value, variables),
            )
        }
        "font" => apply_font_shorthand(
            style,
            &resolver.resolve_value_with_variables(value, variables),
        ),
        "font-size" => style.font_size = resolver.resolve_number_with_variables(value, variables),
        "font-weight" => {
            style.font_weight = resolver.resolve_number_with_variables(value, variables) as u16
        }
        "font-family" => {
            style.font_family = resolver.resolve_value_with_variables(value, variables)
        }
        "font-style" => {
            style.font_style = match resolver
                .resolve_value_with_variables(value, variables)
                .as_str()
            {
                "italic" | "oblique" => FontStyle::Italic,
                _ => FontStyle::Normal,
            };
        }
        "letter-spacing" => {
            style.letter_spacing = resolver.resolve_number_with_variables(value, variables)
        }
        "text-overflow" => {
            style.text_overflow = match resolver
                .resolve_value_with_variables(value, variables)
                .as_str()
            {
                "ellipsis" => TextOverflow::Ellipsis,
                _ => TextOverflow::Clip,
            };
        }
        "line-height" => {
            style.line_height = resolver.resolve_number_with_variables(value, variables)
        }
        "padding" => {
            style.padding =
                parse_edges_shorthand(&resolver.resolve_value_with_variables(value, variables))
        }
        "padding-top" => {
            style.padding.top = resolver.resolve_number_with_variables(value, variables)
        }
        "padding-right" => {
            style.padding.right = resolver.resolve_number_with_variables(value, variables)
        }
        "padding-bottom" => {
            style.padding.bottom = resolver.resolve_number_with_variables(value, variables)
        }
        "padding-left" => {
            style.padding.left = resolver.resolve_number_with_variables(value, variables)
        }
        "padding-x" | "padding-inline" => {
            let v = resolver.resolve_number_with_variables(value, variables);
            style.padding.left = v;
            style.padding.right = v;
        }
        "padding-y" | "padding-block" => {
            let v = resolver.resolve_number_with_variables(value, variables);
            style.padding.top = v;
            style.padding.bottom = v;
        }
        "margin" => {
            style.margin =
                parse_edges_shorthand(&resolver.resolve_value_with_variables(value, variables))
        }
        "margin-top" => style.margin.top = resolver.resolve_number_with_variables(value, variables),
        "margin-right" => {
            style.margin.right = resolver.resolve_number_with_variables(value, variables)
        }
        "margin-bottom" => {
            style.margin.bottom = resolver.resolve_number_with_variables(value, variables)
        }
        "margin-left" => {
            style.margin.left = resolver.resolve_number_with_variables(value, variables)
        }
        "margin-x" | "margin-inline" => {
            let v = resolver.resolve_number_with_variables(value, variables);
            style.margin.left = v;
            style.margin.right = v;
        }
        "margin-y" | "margin-block" => {
            let v = resolver.resolve_number_with_variables(value, variables);
            style.margin.top = v;
            style.margin.bottom = v;
        }
        "gap" => style.gap = resolver.resolve_number_with_variables(value, variables),
        "column-gap" | "row-gap" | "gap-x" => {
            style.gap = resolver.resolve_number_with_variables(value, variables)
        }
        "border-radius" => {
            style.border_radius =
                parse_corners_shorthand(&resolver.resolve_value_with_variables(value, variables))
        }
        "border-top-left-radius" => {
            style.border_radius.top_left = resolver.resolve_number_with_variables(value, variables)
        }
        "border-top-right-radius" => {
            style.border_radius.top_right = resolver.resolve_number_with_variables(value, variables)
        }
        "border-bottom-right-radius" => {
            style.border_radius.bottom_right =
                resolver.resolve_number_with_variables(value, variables)
        }
        "border-bottom-left-radius" => {
            style.border_radius.bottom_left =
                resolver.resolve_number_with_variables(value, variables)
        }
        "border-width" => {
            style.border_width =
                parse_edges_shorthand(&resolver.resolve_value_with_variables(value, variables))
        }
        "border-top-width" => {
            style.border_width.top = resolver.resolve_number_with_variables(value, variables)
        }
        "border-right-width" => {
            style.border_width.right = resolver.resolve_number_with_variables(value, variables)
        }
        "border-bottom-width" => {
            style.border_width.bottom = resolver.resolve_number_with_variables(value, variables)
        }
        "border-left-width" => {
            style.border_width.left = resolver.resolve_number_with_variables(value, variables)
        }
        "opacity" => style.opacity = resolver.resolve_number_with_variables(value, variables),
        "transform" => {
            style.transform =
                parse_transform(&resolver.resolve_value_with_variables(value, variables))
        }
        "transition-duration" => {
            style.transition.duration_ms =
                parse_first_time_ms(&resolver.resolve_value_with_variables(value, variables))
        }
        "transition-delay" => {
            style.transition.delay_ms =
                parse_first_time_ms(&resolver.resolve_value_with_variables(value, variables))
        }
        "transition-timing-function" => {
            style.transition.easing = parse_easing_keyword(first_comma_item(
                &resolver.resolve_value_with_variables(value, variables),
            ))
        }
        "transition-property" => {
            style.transition.properties = parse_transition_properties(
                &resolver.resolve_value_with_variables(value, variables),
            )
        }
        "transition" => {
            let resolved = resolver.resolve_value_with_variables(value, variables);
            let parsed = parse_transition_shorthand(&resolved);
            style.transition.properties = parsed.0;
            style.transition.duration_ms = parsed.1;
            style.transition.delay_ms = parsed.2;
            style.transition.easing = parsed.3;
        }
        "animation-name" => {
            style.animation.name = parse_animation_name(first_comma_item(
                &resolver.resolve_value_with_variables(value, variables),
            ))
        }
        "animation-duration" => {
            style.animation.duration_ms =
                parse_first_time_ms(&resolver.resolve_value_with_variables(value, variables))
        }
        "animation-delay" => {
            style.animation.delay_ms =
                parse_first_time_ms(&resolver.resolve_value_with_variables(value, variables))
        }
        "animation-timing-function" => {
            style.animation.easing = parse_easing_keyword(first_comma_item(
                &resolver.resolve_value_with_variables(value, variables),
            ))
        }
        "animation-iteration-count" => {
            style.animation.iteration_count = parse_animation_iteration_count(first_comma_item(
                &resolver.resolve_value_with_variables(value, variables),
            ))
        }
        "animation-direction" => {
            style.animation.direction = parse_animation_direction(first_comma_item(
                &resolver.resolve_value_with_variables(value, variables),
            ))
        }
        "animation-fill-mode" => {
            style.animation.fill_mode = parse_animation_fill_mode(first_comma_item(
                &resolver.resolve_value_with_variables(value, variables),
            ))
        }
        "animation-play-state" => {
            style.animation.play_state = parse_animation_play_state(first_comma_item(
                &resolver.resolve_value_with_variables(value, variables),
            ))
        }
        "animation" => {
            style.animation =
                parse_animation_shorthand(&resolver.resolve_value_with_variables(value, variables))
        }
        "overflow" => {
            let (x, y) =
                parse_overflow_shorthand(&resolver.resolve_value_with_variables(value, variables));
            style.overflow_x = x;
            style.overflow_y = y;
        }
        "overflow-x" => {
            style.overflow_x =
                parse_overflow(&resolver.resolve_value_with_variables(value, variables))
        }
        "overflow-y" => {
            style.overflow_y =
                parse_overflow(&resolver.resolve_value_with_variables(value, variables))
        }
        "width" => {
            style.width = parse_dimension(&resolver.resolve_value_with_variables(value, variables))
        }
        "height" => {
            style.height = parse_dimension(&resolver.resolve_value_with_variables(value, variables))
        }
        "min-width" => {
            style.min_width = Some(resolver.resolve_number_with_variables(value, variables))
        }
        "max-width" => {
            style.max_width = Some(resolver.resolve_number_with_variables(value, variables))
        }
        "min-height" => {
            style.min_height = Some(resolver.resolve_number_with_variables(value, variables))
        }
        "max-height" => {
            style.max_height = Some(resolver.resolve_number_with_variables(value, variables))
        }
        "flex-grow" => style.flex_grow = resolver.resolve_number_with_variables(value, variables),
        "flex-shrink" => {
            style.flex_shrink = resolver.resolve_number_with_variables(value, variables)
        }
        "flex-basis" => {
            style.flex_basis =
                parse_dimension(&resolver.resolve_value_with_variables(value, variables))
        }
        "flex" => {
            let v = resolver.resolve_value_with_variables(value, variables);
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
            } else {
                apply_flex_shorthand(style, v);
            }
        }
        "flex-wrap" => {
            style.flex_wrap = match resolver
                .resolve_value_with_variables(value, variables)
                .as_str()
            {
                "wrap" => FlexWrap::Wrap,
                "wrap-reverse" => FlexWrap::WrapReverse,
                _ => FlexWrap::NoWrap,
            };
        }
        "align-self" => {
            style.align_self = match resolver
                .resolve_value_with_variables(value, variables)
                .as_str()
            {
                "auto" => AlignSelf::Auto,
                "start" | "flex-start" => AlignSelf::Start,
                "end" | "flex-end" => AlignSelf::End,
                "center" => AlignSelf::Center,
                "baseline" => AlignSelf::Baseline,
                _ => AlignSelf::Stretch,
            };
        }
        "align-content" => {
            style.align_content = match resolver
                .resolve_value_with_variables(value, variables)
                .as_str()
            {
                "start" | "flex-start" => AlignContent::Start,
                "end" | "flex-end" => AlignContent::End,
                "center" => AlignContent::Center,
                "space-between" => AlignContent::SpaceBetween,
                "space-around" => AlignContent::SpaceAround,
                _ => AlignContent::Stretch,
            };
        }
        "flex-direction" => {
            style.direction = match resolver
                .resolve_value_with_variables(value, variables)
                .as_str()
            {
                "column" | "column-reverse" => FlexDirection::Column,
                _ => FlexDirection::Row,
            };
        }
        "direction" => {
            match resolver
                .resolve_value_with_variables(value, variables)
                .as_str()
            {
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
            style.justify_content = match resolver
                .resolve_value_with_variables(value, variables)
                .as_str()
            {
                "center" => JustifyContent::Center,
                "end" | "flex-end" => JustifyContent::End,
                "space-between" => JustifyContent::SpaceBetween,
                "space-around" => JustifyContent::SpaceAround,
                _ => JustifyContent::Start,
            };
        }
        "align-items" => {
            style.align_items = match resolver
                .resolve_value_with_variables(value, variables)
                .as_str()
            {
                "center" => AlignItems::Center,
                "start" | "flex-start" => AlignItems::Start,
                "end" | "flex-end" => AlignItems::End,
                _ => AlignItems::Stretch,
            };
        }
        "text-align" => {
            style.text_align = match resolver
                .resolve_value_with_variables(value, variables)
                .as_str()
            {
                "center" => TextAlign::Center,
                "right" => TextAlign::Right,
                _ => TextAlign::Left,
            };
        }
        "display" => {
            style.display = match resolver
                .resolve_value_with_variables(value, variables)
                .as_str()
            {
                "none" => Display::None,
                _ => Display::Flex,
            };
        }
        "visibility" => {
            if matches!(
                resolver
                    .resolve_value_with_variables(value, variables)
                    .as_str(),
                "hidden" | "collapse"
            ) {
                style.opacity = 0.0;
            }
        }
        "position" => {
            style.position = match resolver
                .resolve_value_with_variables(value, variables)
                .as_str()
            {
                "relative" => Position::Relative,
                "absolute" => Position::Absolute,
                _ => Position::Static,
            };
        }
        "z-index" => {
            let v = resolver.resolve_value_with_variables(value, variables);
            style.z_index = v.trim().parse::<i32>().unwrap_or(0);
        }
        "top" => style.inset_top = Some(resolver.resolve_number_with_variables(value, variables)),
        "right" => {
            style.inset_right = Some(resolver.resolve_number_with_variables(value, variables))
        }
        "bottom" => {
            style.inset_bottom = Some(resolver.resolve_number_with_variables(value, variables))
        }
        "left" => style.inset_left = Some(resolver.resolve_number_with_variables(value, variables)),
        "inset" => {
            let edges =
                parse_edges_shorthand(&resolver.resolve_value_with_variables(value, variables));
            style.inset_top = Some(edges.top);
            style.inset_right = Some(edges.right);
            style.inset_bottom = Some(edges.bottom);
            style.inset_left = Some(edges.left);
        }
        _ if property.starts_with("--") => {}
        _ => {
            tracing::warn!("unsupported CSS property '{}'", property);
        }
    }
}

fn selector_to_diagnostic_string(selector: &Selector) -> String {
    match selector {
        Selector::Universal => "*".to_string(),
        Selector::Tag(tag) => tag.clone(),
        Selector::Class(class) => format!(".{class}"),
        Selector::Id(id) => format!("#{id}"),
        Selector::State(tag, state) => format!("{tag}:{state}"),
        Selector::Compound(parts) => parts
            .iter()
            .map(selector_to_diagnostic_string)
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn resolve_embedded_tokens(value: &str, theme: &Theme) -> String {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;

    while let Some(start) = rest.find("token(") {
        output.push_str(&rest[..start]);
        let token_start = start + "token(".len();
        let Some(end) = rest[token_start..].find(')') else {
            output.push_str(&rest[start..]);
            return output;
        };

        let name = rest[token_start..token_start + end].trim();
        match theme.token(name) {
            Some(TokenValue::String(s)) => output.push_str(s),
            Some(TokenValue::Number(n)) => output.push_str(&format!("{n}")),
            Some(TokenValue::Bool(b)) => output.push_str(&format!("{b}")),
            None => tracing::warn!("unresolved theme token: {name}"),
        }
        rest = &rest[token_start + end + 1..];
    }

    output.push_str(rest);
    output
}

fn shorthand_numbers(value: &str) -> Vec<f32> {
    value
        .split_whitespace()
        .take(4)
        .map(parse_px)
        .collect::<Vec<_>>()
}

fn parse_edges_shorthand(value: &str) -> Edges {
    let values = shorthand_numbers(value);
    match values.as_slice() {
        [all] => Edges::all(*all),
        [vertical, horizontal] => Edges {
            top: *vertical,
            right: *horizontal,
            bottom: *vertical,
            left: *horizontal,
        },
        [top, horizontal, bottom] => Edges {
            top: *top,
            right: *horizontal,
            bottom: *bottom,
            left: *horizontal,
        },
        [top, right, bottom, left] => Edges {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        },
        _ => Edges::zero(),
    }
}

/// Parse the CSS `transform` shorthand value. Functions are applied in
/// authoring order; for non-commuting compositions (rotate + translate) this
/// matches CSS semantics. Unknown functions are ignored.
///
/// Recognised forms:
///   - `none`
///   - `translate(<x>)`            — y defaults to 0
///   - `translate(<x> <y>)`        — space-separated
///   - `translate(<x>, <y>)`       — comma-separated
///   - `translateX(<x>)`, `translateY(<y>)`
///   - `scale(<s>)`                — uniform
///   - `scale(<sx> <sy>)`, `scale(<sx>, <sy>)`
///   - `scaleX(<s>)`, `scaleY(<s>)`
///   - `rotate(<angle>)`           — `deg`, `rad`, `turn`
fn parse_transform(value: &str) -> Transform2D {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "none" {
        return Transform2D::IDENTITY;
    }

    let mut transform = Transform2D::IDENTITY;
    let mut rest = trimmed;
    while !rest.is_empty() {
        rest = rest.trim_start();
        let Some(open) = rest.find('(') else {
            break;
        };
        let name = rest[..open].trim();
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find(')') else {
            break;
        };
        let args_str = &after_open[..close];
        let args: Vec<f32> = args_str
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(parse_transform_length)
            .collect();
        let angle_arg: Option<f32> = args_str
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(str::trim)
            .find(|s| !s.is_empty())
            .map(parse_transform_angle);

        match name {
            "translate" => {
                if let Some(&x) = args.first() {
                    transform.translate_x += x;
                }
                if let Some(&y) = args.get(1) {
                    transform.translate_y += y;
                }
            }
            "translateX" => {
                if let Some(&x) = args.first() {
                    transform.translate_x += x;
                }
            }
            "translateY" => {
                if let Some(&y) = args.first() {
                    transform.translate_y += y;
                }
            }
            "scale" => {
                if let Some(&sx) = args.first() {
                    transform.scale_x *= sx;
                    let sy = args.get(1).copied().unwrap_or(sx);
                    transform.scale_y *= sy;
                }
            }
            "scaleX" => {
                if let Some(&sx) = args.first() {
                    transform.scale_x *= sx;
                }
            }
            "scaleY" => {
                if let Some(&sy) = args.first() {
                    transform.scale_y *= sy;
                }
            }
            "rotate" => {
                if let Some(angle) = angle_arg {
                    transform.rotation += angle;
                }
            }
            _ => {}
        }

        rest = &after_open[close + 1..];
    }
    transform
}

fn parse_transform_length(token: &str) -> f32 {
    let token = token.trim();
    if let Some(rest) = token.strip_suffix("px") {
        rest.trim().parse::<f32>().unwrap_or(0.0)
    } else {
        token.parse::<f32>().unwrap_or(0.0)
    }
}

fn parse_transform_angle(token: &str) -> f32 {
    let token = token.trim();
    if let Some(rest) = token.strip_suffix("deg") {
        rest.trim().parse::<f32>().unwrap_or(0.0).to_radians()
    } else if let Some(rest) = token.strip_suffix("turn") {
        rest.trim().parse::<f32>().unwrap_or(0.0) * std::f32::consts::TAU
    } else if let Some(rest) = token.strip_suffix("rad") {
        rest.trim().parse::<f32>().unwrap_or(0.0)
    } else {
        token.parse::<f32>().unwrap_or(0.0).to_radians()
    }
}

fn parse_corners_shorthand(value: &str) -> Corners {
    let edges = parse_edges_shorthand(value);
    Corners {
        top_left: edges.top,
        top_right: edges.right,
        bottom_right: edges.bottom,
        bottom_left: edges.left,
    }
}

fn parse_border_color_shorthand(value: &str) -> Color {
    value
        .split_whitespace()
        .find_map(Color::from_hex)
        .or_else(|| Color::from_hex(value.trim()))
        .unwrap_or(Color::TRANSPARENT)
}

fn apply_border_shorthand(style: &mut ComputedStyle, value: &str) {
    if value.trim() == "none" {
        style.border_width = Edges::zero();
        style.border_color = Color::TRANSPARENT;
        return;
    }

    for token in value.split_whitespace() {
        if token.ends_with("px") || token.parse::<f32>().is_ok() {
            style.border_width = Edges::all(parse_px(token));
        } else if let Some(color) = Color::from_hex(token) {
            style.border_color = color;
        }
    }
}

fn apply_flex_shorthand(style: &mut ComputedStyle, value: &str) {
    let parts = value.split_whitespace().collect::<Vec<_>>();
    if parts.len() >= 3 {
        if let Ok(grow) = parts[0].parse::<f32>() {
            style.flex_grow = grow;
        }
        if let Ok(shrink) = parts[1].parse::<f32>() {
            style.flex_shrink = shrink;
        }
        style.flex_basis = parse_dimension(parts[2]);
    }
}

fn apply_font_shorthand(style: &mut ComputedStyle, value: &str) {
    let mut family_parts = Vec::new();
    let mut saw_size = false;

    for token in value.split_whitespace() {
        if token == "italic" || token == "oblique" {
            style.font_style = FontStyle::Italic;
        } else if token == "normal" {
            style.font_style = FontStyle::Normal;
        } else if let Ok(weight) = token.parse::<u16>() {
            style.font_weight = weight;
        } else if token.contains("px") {
            let mut size_parts = token.split('/');
            if let Some(size) = size_parts.next() {
                style.font_size = parse_px(size);
                saw_size = true;
            }
            if let Some(line_height) = size_parts.next() {
                style.line_height = parse_px(line_height);
            }
        } else if saw_size {
            family_parts.push(token.trim_matches('"').trim_matches('\''));
        }
    }

    if !family_parts.is_empty() {
        style.font_family = family_parts.join(" ");
    }
}

fn parse_overflow_shorthand(value: &str) -> (Overflow, Overflow) {
    let parts = value.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        [one] => {
            let overflow = parse_overflow(one);
            (overflow, overflow)
        }
        [x, y, ..] => (parse_overflow(x), parse_overflow(y)),
        [] => (Overflow::Visible, Overflow::Visible),
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

fn parse_transition_properties(value: &str) -> TransitionProperties {
    let mut properties = TransitionProperties::none();
    for property in value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match property {
            "all" => return TransitionProperties::all(),
            "border-radius" => properties.border_radius = true,
            "border-width" => properties.border_width = true,
            "opacity" => properties.opacity = true,
            "background-color" | "background" => properties.background_color = true,
            "border-color" => properties.border_color = true,
            "color" => properties.color = true,
            "width" => properties.width = true,
            "height" => properties.height = true,
            "padding" => properties.padding = true,
            "margin" => properties.margin = true,
            "transform" => properties.transform = true,
            _ => {}
        }
    }
    properties
}

fn first_comma_item(value: &str) -> &str {
    let mut depth: i32 = 0;
    let mut split_at = value.len();
    for (idx, ch) in value.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            ',' if depth == 0 => {
                split_at = idx;
                break;
            }
            _ => {}
        }
    }
    value[..split_at].trim()
}

fn parse_first_time_ms(value: &str) -> u32 {
    parse_time_ms(first_comma_item(value))
}

/// Split `value` on `delim` while ignoring any `delim` that appears inside
/// parentheses. Lets `transition: opacity 200ms cubic-bezier(0.2, 0, 0, 1)`
/// keep the bezier call together when the shorthand is tokenized.
fn split_paren_aware(value: &str, delim: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut buf = String::new();
    for ch in value.chars() {
        match ch {
            '(' => {
                depth += 1;
                buf.push(ch);
            }
            ')' => {
                depth = (depth - 1).max(0);
                buf.push(ch);
            }
            c if c == delim && depth == 0 => {
                out.push(std::mem::take(&mut buf));
            }
            _ => buf.push(ch),
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

fn parse_easing_keyword(value: &str) -> TransitionEasing {
    let trimmed = value.trim();
    match trimmed {
        "linear" => TransitionEasing::Linear,
        "ease" => TransitionEasing::Ease,
        "ease-in" => TransitionEasing::EaseIn,
        "ease-out" => TransitionEasing::EaseOut,
        "ease-in-out" => TransitionEasing::EaseInOut,
        _ => parse_cubic_bezier(trimmed).unwrap_or(TransitionEasing::EaseOut),
    }
}

fn parse_cubic_bezier(value: &str) -> Option<TransitionEasing> {
    let inner = value
        .strip_prefix("cubic-bezier(")
        .and_then(|rest| rest.strip_suffix(')'))?;
    let parts: Vec<f32> = inner
        .split(',')
        .map(|part| part.trim().parse::<f32>())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if parts.len() != 4 {
        return None;
    }
    Some(TransitionEasing::CubicBezier(
        parts[0].clamp(0.0, 1.0),
        parts[1],
        parts[2].clamp(0.0, 1.0),
        parts[3],
    ))
}

fn looks_like_time(token: &str) -> bool {
    if let Some(rest) = token.strip_suffix("ms") {
        rest.trim().parse::<f32>().is_ok()
    } else if let Some(rest) = token.strip_suffix('s') {
        rest.trim().parse::<f32>().is_ok()
    } else {
        // Bare numeric tokens (from `motion.duration.*` theme values that
        // resolve to plain numbers) are interpreted as milliseconds.
        token.trim().parse::<f32>().is_ok()
    }
}

fn parse_transition_shorthand(value: &str) -> (TransitionProperties, u32, u32, TransitionEasing) {
    let mut properties = TransitionProperties::none();
    let mut duration_ms = 0u32;
    let mut delay_ms = 0u32;
    let mut easing = TransitionEasing::EaseOut;

    for item in split_paren_aware(value, ',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let mut item_time_count = 0;
        for token in split_paren_aware(item, ' ').iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if looks_like_time(token) {
                let ms = parse_time_ms(token);
                if item_time_count == 0 && duration_ms == 0 {
                    duration_ms = ms;
                } else if item_time_count > 0 && delay_ms == 0 {
                    delay_ms = ms;
                }
                item_time_count += 1;
                continue;
            }
            match token {
                "all" => properties = TransitionProperties::all(),
                "border-radius" => properties.border_radius = true,
                "border-width" => properties.border_width = true,
                "opacity" => properties.opacity = true,
                "background-color" | "background" => properties.background_color = true,
                "border-color" => properties.border_color = true,
                "color" => properties.color = true,
                "width" => properties.width = true,
                "height" => properties.height = true,
                "padding" => properties.padding = true,
                "margin" => properties.margin = true,
                "transform" => properties.transform = true,
                "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out"
                    if easing == TransitionEasing::EaseOut =>
                {
                    easing = parse_easing_keyword(token)
                }
                _ if token.starts_with("cubic-bezier(") && easing == TransitionEasing::EaseOut => {
                    easing = parse_easing_keyword(token);
                }
                _ => {}
            }
        }
    }

    (properties, duration_ms, delay_ms, easing)
}

fn parse_animation_name(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value == "none" {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_animation_iteration_count(value: &str) -> AnimationIterationCount {
    let value = value.trim();
    if value == "infinite" {
        AnimationIterationCount::Infinite
    } else {
        AnimationIterationCount::Number(value.parse::<u32>().unwrap_or(1))
    }
}

fn parse_animation_direction(value: &str) -> AnimationDirection {
    match value.trim() {
        "reverse" => AnimationDirection::Reverse,
        "alternate" => AnimationDirection::Alternate,
        "alternate-reverse" => AnimationDirection::AlternateReverse,
        _ => AnimationDirection::Normal,
    }
}

fn parse_animation_fill_mode(value: &str) -> AnimationFillMode {
    match value.trim() {
        "forwards" => AnimationFillMode::Forwards,
        "backwards" => AnimationFillMode::Backwards,
        "both" => AnimationFillMode::Both,
        _ => AnimationFillMode::None,
    }
}

fn parse_animation_play_state(value: &str) -> AnimationPlayState {
    match value.trim() {
        "paused" => AnimationPlayState::Paused,
        _ => AnimationPlayState::Running,
    }
}

fn parse_animation_shorthand(value: &str) -> AnimationStyle {
    let mut animation = AnimationStyle::default();
    let mut time_count = 0;

    for token in first_comma_item(value).split_whitespace() {
        if looks_like_time(token) {
            let ms = parse_time_ms(token);
            if time_count == 0 {
                animation.duration_ms = ms;
            } else {
                animation.delay_ms = ms;
            }
            time_count += 1;
        } else if matches!(
            token,
            "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out"
        ) {
            animation.easing = parse_easing_keyword(token);
        } else if token == "infinite" || token.parse::<u32>().is_ok() {
            animation.iteration_count = parse_animation_iteration_count(token);
        } else if matches!(
            token,
            "normal" | "reverse" | "alternate" | "alternate-reverse"
        ) {
            animation.direction = parse_animation_direction(token);
        } else if matches!(token, "none" | "forwards" | "backwards" | "both") {
            animation.fill_mode = parse_animation_fill_mode(token);
        } else if matches!(token, "running" | "paused") {
            animation.play_state = parse_animation_play_state(token);
        } else {
            animation.name = parse_animation_name(token);
        }
    }

    animation
}

fn parse_time_ms(value: &str) -> u32 {
    let raw = value.trim();
    if let Some(ms) = raw.strip_suffix("ms") {
        return ms.trim().parse::<f32>().unwrap_or(0.0).max(0.0).round() as u32;
    }
    if let Some(seconds) = raw.strip_suffix('s') {
        return (seconds.trim().parse::<f32>().unwrap_or(0.0).max(0.0) * 1000.0).round() as u32;
    }
    // Bare numeric values are treated as milliseconds. This is what makes
    // numeric duration tokens (e.g. `motion.duration.short`: 150) usable
    // directly: `transition-duration: token(motion.duration.short)`
    // expands to `150` which, without a unit, is interpreted as 150ms.
    raw.parse::<f32>()
        .ok()
        .map(|v| v.max(0.0).round() as u32)
        .unwrap_or(0)
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
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let value = StyleValue::Token("color.primary".to_string());
        let resolved = resolver.resolve_value(&value);
        assert_eq!(resolved, "#6750A4");
    }

    #[test]
    fn supported_css_properties_cover_phase_8_contract() {
        for property in [
            "background",
            "background-color",
            "color",
            "border",
            "border-color",
            "border-width",
            "border-radius",
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
            "padding-inline",
            "padding-block",
            "margin",
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
        ] {
            assert!(is_supported_css_property(property), "{property}");
        }
        assert!(is_supported_css_property("--local-token"));
        assert!(!is_supported_css_property("grid-template-columns"));
        assert!(!is_supported_css_property("transform"));
    }

    #[test]
    fn style_diagnostics_unsupported_property_produces_style_diagnostic() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "grid-template-columns".to_string(),
                value: StyleValue::Literal("1fr 1fr".to_string()),
            }],
            container_query: None,
        }];

        let (_style, diagnostics) = resolver.resolve_node_style_with_diagnostics(
            &rules,
            "box",
            &["panel".to_string()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].property, "grid-template-columns");
        assert_eq!(diagnostics[0].selector.as_deref(), Some(".panel"));
        assert!(
            diagnostics[0]
                .message
                .contains("unsupported CSS property 'grid-template-columns'")
        );
    }

    #[test]
    fn resolve_node_style_from_rules() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);

        let rules = vec![StyleRule {
            selector: Selector::Tag("text".to_string()),
            declarations: vec![
                mesh_core_component::style::Declaration {
                    property: "font-size".to_string(),
                    value: StyleValue::Literal("20px".to_string()),
                },
                mesh_core_component::style::Declaration {
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
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "overflow-y".to_string(),
                value: StyleValue::Literal("auto".to_string()),
            }],
            container_query: Some(mesh_core_component::style::ContainerQuery {
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
        use mesh_core_component::style::{Declaration, Selector};

        let theme = mesh_core_theme::default_theme();
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
    fn focus_visible_requires_focus_visible_state() {
        use crate::tree::ElementState;
        use mesh_core_component::style::{Declaration, Selector};

        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::State("input".to_string(), "focus-visible".to_string()),
            declarations: vec![Declaration {
                property: "color".to_string(),
                value: StyleValue::Literal("#abcdef".to_string()),
            }],
            container_query: None,
        }];

        let focused_only = resolver.resolve_node_style(
            &rules,
            "input",
            &[],
            None,
            StyleContext::default(),
            ElementState {
                focused: true,
                ..Default::default()
            },
        );
        assert_ne!(
            focused_only.color,
            Color::from_hex("#abcdef").unwrap(),
            ":focus-visible should no longer alias plain focused state"
        );

        let focus_visible = resolver.resolve_node_style(
            &rules,
            "input",
            &[],
            None,
            StyleContext::default(),
            ElementState {
                focused: true,
                focus_visible: true,
                ..Default::default()
            },
        );
        assert_eq!(focus_visible.color, Color::from_hex("#abcdef").unwrap());
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
        use mesh_core_component::parser::parse_component;

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

        let theme = mesh_core_theme::default_theme();
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
        use mesh_core_component::parser::parse_component;

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

        let theme = mesh_core_theme::default_theme();
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

    #[test]
    fn padding_margin_four_value_shorthands_expand_to_edges() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("card".to_string()),
            declarations: vec![
                mesh_core_component::style::Declaration {
                    property: "padding".to_string(),
                    value: StyleValue::Literal("1px 2px 3px 4px".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "margin".to_string(),
                    value: StyleValue::Literal("5px 6px 7px 8px".to_string()),
                },
            ],
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &["card".to_string()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.padding.top, 1.0);
        assert_eq!(style.padding.right, 2.0);
        assert_eq!(style.padding.bottom, 3.0);
        assert_eq!(style.padding.left, 4.0);
        assert_eq!(style.margin.top, 5.0);
        assert_eq!(style.margin.right, 6.0);
        assert_eq!(style.margin.bottom, 7.0);
        assert_eq!(style.margin.left, 8.0);
    }

    #[test]
    fn border_shorthand_sets_width_and_color() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Tag("box".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "border".to_string(),
                value: StyleValue::Literal("2px solid #ffffff".to_string()),
            }],
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &[],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.border_width, Edges::all(2.0));
        assert_eq!(style.border_color, Color::WHITE);
    }

    #[test]
    fn overflow_two_value_shorthand_sets_axes() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Tag("box".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "overflow".to_string(),
                value: StyleValue::Literal("hidden auto".to_string()),
            }],
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &[],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.overflow_x, Overflow::Hidden);
        assert_eq!(style.overflow_y, Overflow::Auto);
    }

    #[test]
    fn flex_triple_shorthand_sets_grow_shrink_basis() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Tag("box".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "flex".to_string(),
                value: StyleValue::Literal("1 0 12px".to_string()),
            }],
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &[],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.flex_grow, 1.0);
        assert_eq!(style.flex_shrink, 0.0);
        assert!(matches!(style.flex_basis, Dimension::Px(px) if px == 12.0));
    }

    #[test]
    fn font_shorthand_sets_text_fields() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Tag("text".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "font".to_string(),
                value: StyleValue::Literal("italic 600 16px/1.4 Inter".to_string()),
            }],
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

        assert_eq!(style.font_style, FontStyle::Italic);
        assert_eq!(style.font_weight, 600);
        assert_eq!(style.font_size, 16.0);
        assert_eq!(style.line_height, 1.4);
        assert_eq!(style.font_family, "Inter");
    }

    #[test]
    fn css_variable_resolves_local_literal_value() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![
                mesh_core_component::style::Declaration {
                    property: "--surface".to_string(),
                    value: StyleValue::Literal("#ffffff".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "background".to_string(),
                    value: StyleValue::Var("--surface".to_string()),
                },
            ],
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &["panel".to_string()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.background_color, Color::WHITE);
    }

    #[test]
    fn css_variable_resolves_token_value_before_computed_style() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![
                mesh_core_component::style::Declaration {
                    property: "--surface".to_string(),
                    value: StyleValue::Token("color.primary".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "background".to_string(),
                    value: StyleValue::Var("--surface".to_string()),
                },
            ],
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &["panel".to_string()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.background_color, Color::from_hex("#6750A4").unwrap());
    }

    #[test]
    fn missing_css_variable_produces_style_diagnostic() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "background".to_string(),
                value: StyleValue::Var("--missing".to_string()),
            }],
            container_query: None,
        }];

        let (_style, diagnostics) = resolver.resolve_node_style_with_diagnostics(
            &rules,
            "box",
            &["panel".to_string()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("--missing"));
    }

    #[test]
    fn token_resolution_still_works_after_variable_support() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let value = StyleValue::Token("color.primary".to_string());
        assert_eq!(resolver.resolve_value(&value), "#6750A4");
    }

    #[test]
    fn transition_shorthand_parses_comma_separated_items() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "transition".to_string(),
                value: StyleValue::Literal(
                    "opacity 150ms ease-in 25ms, border-color 250ms ease-out".to_string(),
                ),
            }],
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &["panel".to_string()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.transition.duration_ms, 150);
        assert_eq!(style.transition.delay_ms, 25);
        assert_eq!(style.transition.easing, TransitionEasing::EaseIn);
        assert!(style.transition.properties.animates_opacity());
        assert!(style.transition.properties.animates_border_color());
    }

    #[test]
    fn transition_property_supports_phase_8_visual_properties() {
        let properties = parse_transition_properties(
            "all, opacity, background, background-color, color, border-color, border-radius",
        );

        assert!(properties.animates_opacity());
        assert!(properties.animates_background_color());
        assert!(properties.animates_border_color());
        assert!(properties.animates_color());
        assert!(properties.animates_border_radius());
    }

    #[test]
    fn animation_longhands_store_metadata_only() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![
                mesh_core_component::style::Declaration {
                    property: "animation-name".to_string(),
                    value: StyleValue::Literal("pulse".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-duration".to_string(),
                    value: StyleValue::Literal("320ms".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-delay".to_string(),
                    value: StyleValue::Literal("40ms".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-timing-function".to_string(),
                    value: StyleValue::Literal("ease-in-out".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-iteration-count".to_string(),
                    value: StyleValue::Literal("infinite".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-direction".to_string(),
                    value: StyleValue::Literal("alternate".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-fill-mode".to_string(),
                    value: StyleValue::Literal("both".to_string()),
                },
                mesh_core_component::style::Declaration {
                    property: "animation-play-state".to_string(),
                    value: StyleValue::Literal("paused".to_string()),
                },
            ],
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &["panel".to_string()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.animation.name.as_deref(), Some("pulse"));
        assert_eq!(style.animation.duration_ms, 320);
        assert_eq!(style.animation.delay_ms, 40);
        assert_eq!(style.animation.easing, TransitionEasing::EaseInOut);
        assert_eq!(
            style.animation.iteration_count,
            AnimationIterationCount::Infinite
        );
        assert_eq!(style.animation.direction, AnimationDirection::Alternate);
        assert_eq!(style.animation.fill_mode, AnimationFillMode::Both);
        assert_eq!(style.animation.play_state, AnimationPlayState::Paused);
    }

    #[test]
    fn animation_shorthand_stores_metadata_only() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations: vec![mesh_core_component::style::Declaration {
                property: "animation".to_string(),
                value: StyleValue::Literal(
                    "pulse 250ms ease-in-out 50ms 2 alternate both paused".to_string(),
                ),
            }],
            container_query: None,
        }];

        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &["panel".to_string()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.animation.name.as_deref(), Some("pulse"));
        assert_eq!(style.animation.duration_ms, 250);
        assert_eq!(style.animation.delay_ms, 50);
        assert_eq!(style.animation.easing, TransitionEasing::EaseInOut);
        assert_eq!(
            style.animation.iteration_count,
            AnimationIterationCount::Number(2)
        );
        assert_eq!(style.animation.direction, AnimationDirection::Alternate);
        assert_eq!(style.animation.fill_mode, AnimationFillMode::Both);
        assert_eq!(style.animation.play_state, AnimationPlayState::Paused);
    }

    #[test]
    fn shell_card_css_subset_resolves_for_layout() {
        use mesh_core_component::parser::parse_component;

        let source = r#"
<style>
.shell-card {
    --pad: token(spacing.md);
    padding: var(--pad);
    margin: 4px 8px;
    border: 1px solid token(color.outline);
    display: flex;
    flex-direction: column;
    gap: 6px;
    position: relative;
    overflow: hidden;
}
</style>
"#;
        let file = parse_component(source).unwrap();
        let rules = file.style.unwrap().rules;

        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let style = resolver.resolve_node_style(
            &rules,
            "box",
            &["shell-card".to_string()],
            None,
            StyleContext::default(),
            ElementState::default(),
        );

        assert_eq!(style.padding, Edges::all(16.0));
        assert_eq!(style.margin.top, 4.0);
        assert_eq!(style.margin.right, 8.0);
        assert_eq!(style.margin.bottom, 4.0);
        assert_eq!(style.margin.left, 8.0);
        assert_eq!(style.border_width, Edges::all(1.0));
        assert_eq!(style.border_color.a, 255);
        assert_eq!(style.direction, FlexDirection::Column);
        assert_eq!(style.gap, 6.0);
        assert_eq!(style.position, Position::Relative);
        assert_eq!(style.overflow_x, Overflow::Hidden);
        assert_eq!(style.overflow_y, Overflow::Hidden);
    }

    #[test]
    fn pseudo_state_rules_still_apply_after_variable_support() {
        pseudo_state_rules_apply_when_state_matches();
    }

    #[test]
    fn container_query_rules_still_apply_after_variable_support() {
        container_query_rules_apply_against_context();
    }
}
