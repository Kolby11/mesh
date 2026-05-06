use super::parse::*;
use super::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Selector, StyleRule, StyleValue};
use mesh_core_theme::{Theme, TokenValue};
use std::collections::HashMap;

/// Resolves style values against a theme's design tokens.
pub struct StyleResolver<'a> {
    theme: &'a Theme,
}

impl<'a> StyleResolver<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self { theme }
    }

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

    pub fn resolve_color(&self, value: &StyleValue) -> Color {
        let resolved = self.resolve_value(value);
        Color::from_hex(&resolved).unwrap_or(Color::TRANSPARENT)
    }

    pub fn resolve_number(&self, value: &StyleValue) -> f32 {
        let resolved = self.resolve_value(value);
        parse_px(&resolved)
    }

    pub fn resolve_time_ms(&self, value: &StyleValue) -> u32 {
        let resolved = self.resolve_value(value);
        parse_time_ms(&resolved)
    }

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

        if tag == "column" {
            style.direction = FlexDirection::Column;
        }

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

    pub fn restyle_subtree(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
    ) {
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
