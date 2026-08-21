use super::StyleResolver;
use super::attrs::StyleNodeAttrs;
use super::index::*;
use super::matching::*;
use crate::style::parse::*;
use crate::style::*;
use mesh_core_component::style::{Declaration, Selector, StyleValue};
use mesh_core_theme::TokenValue;
use std::collections::HashMap;

macro_rules! css_property_table {
    (
        fn $apply:ident(
            $style:ident: &mut ComputedStyle,
            $property:ident: &str,
            $value:ident: &StyleValue,
            $resolver:ident: &StyleResolver,
            $variables:ident: &HashMap<String, StyleValue>,
        ) { $($arms:tt)* }
    ) => {
        css_property_table! {
            @parse
            [$apply, $style, $property, $value, $resolver, $variables]
            []
            []
            $($arms)*
        }
    };
    (
        @parse $signature:tt
        [$($names:expr,)*]
        [$($parsed:tt)*]
        $first:literal $(| $alias:literal)* => $body:block $(,)?
        $($rest:tt)*
    ) => {
        css_property_table! {
            @parse $signature
            [$($names,)* $first, $($alias,)*]
            [$($parsed)* $first $(| $alias)* => $body,]
            $($rest)*
        }
    };
    (
        @parse $signature:tt
        [$($names:expr,)*]
        [$($parsed:tt)*]
        $first:literal $(| $alias:literal)* => $body:expr,
        $($rest:tt)*
    ) => {
        css_property_table! {
            @parse $signature
            [$($names,)* $first, $($alias,)*]
            [$($parsed)* $first $(| $alias)* => $body,]
            $($rest)*
        }
    };
    (
        @parse
        [$apply:ident, $style:ident, $property:ident, $value:ident, $resolver:ident, $variables:ident]
        [$($names:expr,)*]
        [$($parsed:tt)*]
    ) => {
        pub(super) const LOWERED_CSS_PROPERTIES: &[&str] = &[$($names,)*];

        pub(super) fn $apply(
            $style: &mut ComputedStyle,
            $property: &str,
            $value: &StyleValue,
            $resolver: &StyleResolver,
            $variables: &HashMap<String, StyleValue>,
        ) {
            match $property {
                $($parsed)*
                _ => tracing::warn!("unsupported CSS property '{}'", $property),
            }
        }
    };
}

pub(crate) fn lowered_css_properties() -> &'static [&'static str] {
    LOWERED_CSS_PROPERTIES
}

impl<'a> StyleResolver<'a> {
    pub fn apply_declarations_with_diagnostics(
        &self,
        style: &mut ComputedStyle,
        declarations: &[mesh_core_component::style::Declaration],
        selector: Option<&str>,
    ) -> Vec<StyleDiagnostic> {
        let mut diagnostics = Vec::new();
        let mut variables = HashMap::new();

        for decl in declarations {
            self.apply_declaration_with_diagnostics(
                style,
                decl,
                selector.map(str::to_string),
                &mut diagnostics,
                &mut variables,
            );
        }

        diagnostics
    }

    pub(super) fn apply_theme_component_defaults(
        &self,
        style: &mut ComputedStyle,
        tag: &str,
        module_id: Option<&str>,
        mut diagnostics: Option<&mut Vec<StyleDiagnostic>>,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        if let Some(defaults) = self.theme.component_defaults("base") {
            self.apply_theme_defaults_map(style, "base", defaults, &mut diagnostics, variables);
        }
        if let Some(defaults) = self.theme.component_defaults(tag) {
            self.apply_theme_defaults_map(style, tag, defaults, &mut diagnostics, variables);
        }
        if let Some(module_id) = module_id {
            self.seed_module_theme_variables(module_id, variables);
            if let Some(defaults) = self.theme.module_component_defaults(module_id, "base") {
                self.apply_theme_defaults_map(style, "base", defaults, &mut diagnostics, variables);
            }
            if let Some(defaults) = self.theme.module_component_defaults(module_id, tag) {
                self.apply_theme_defaults_map(style, tag, defaults, &mut diagnostics, variables);
            }
        }
    }

    pub(super) fn seed_module_theme_variables(
        &self,
        module_id: &str,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        let Some(module) = self.theme.modules().get(module_id) else {
            return;
        };
        let mut cache = self.module_variable_cache.borrow_mut();
        let entries = cache.entry(module_id.to_owned()).or_insert_with(|| {
            module
                .tokens
                .iter()
                .map(|(name, value)| {
                    (
                        format!("--{}", name.replace('.', "-")),
                        StyleValue::Literal(match value {
                            TokenValue::String(value) => value.clone(),
                            TokenValue::Number(value) => format!("{value}"),
                            TokenValue::Bool(value) => format!("{value}"),
                        }),
                    )
                })
                .collect()
        });
        for (key, value) in entries {
            variables
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
    }

    pub(super) fn apply_theme_style_rules(
        &self,
        style: &mut ComputedStyle,
        attrs: &StyleNodeAttrs,
        diagnostics: &mut Option<&mut Vec<StyleDiagnostic>>,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        let mut apply = |rules: &[mesh_core_theme::ThemeStyleRule], scope: &str| {
            for (index, rule) in rules.iter().enumerate() {
                if !selector_matches_attrs(&rule.selector, attrs) {
                    continue;
                }
                let declarations =
                    indexed_theme_defaults(self.theme.revision(), &rule.declarations);
                let selector = format!("@theme:{scope}:rule:{index}");
                for declaration in declarations.iter() {
                    let diagnostic_sink = diagnostics
                        .as_mut()
                        .map(|diagnostics| (selector.as_str(), &mut **diagnostics));
                    self.apply_indexed_declaration(style, declaration, diagnostic_sink, variables);
                }
            }
        };

        apply(self.theme.style_rules(), "global");
        if let Some(module_id) = attrs.module_id
            && let Some(module) = self.theme.modules().get(module_id)
        {
            apply(&module.rules, module_id);
        }
    }

    pub(super) fn apply_theme_defaults_map(
        &self,
        style: &mut ComputedStyle,
        component_name: &str,
        defaults: &mesh_core_theme::ComponentDefaults,
        diagnostics: &mut Option<&mut Vec<StyleDiagnostic>>,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        let selector = diagnostics
            .as_ref()
            .map(|_| format!("@theme:{component_name}"));
        let declarations = indexed_theme_defaults(self.theme.revision(), defaults);
        for declaration in declarations.iter() {
            let diagnostic_sink = diagnostics.as_mut().and_then(|diagnostics| {
                selector
                    .as_deref()
                    .map(|selector| (selector, &mut **diagnostics))
            });
            self.apply_indexed_declaration(style, &declaration, diagnostic_sink, variables);
        }
    }

    #[cfg(test)]
    pub(super) fn apply_declaration_no_diagnostics(
        &self,
        style: &mut ComputedStyle,
        decl: &Declaration,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        self.apply_property_value_no_diagnostics(style, &decl.property, &decl.value, variables);
    }

    pub(super) fn apply_indexed_declaration(
        &self,
        style: &mut ComputedStyle,
        decl: &IndexedDeclaration,
        mut diagnostics: Option<(&str, &mut Vec<StyleDiagnostic>)>,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        match &decl.property {
            IndexedProperty::Custom(property) => {
                variables.insert(property.clone(), decl.value.clone());
            }
            IndexedProperty::StaticDiagnostic { property, message } => {
                push_indexed_static_style_diagnostic(&mut diagnostics, property, message);
            }
            IndexedProperty::Lowered {
                name,
                strict_animation,
                background_image,
            } => {
                if let Some(literal) = decl.literal
                    && apply_typed_literal(style, name, literal)
                {
                    return;
                }
                if let StyleValue::Var(variable_name) = &decl.value
                    && !*strict_animation
                    && !variables.contains_key(variable_name)
                    && self.cached_theme_token_value(variable_name).is_missing()
                {
                    push_indexed_style_diagnostic(
                        &mut diagnostics,
                        name.clone(),
                        format!(
                            "unsupported CSS variable reference '{variable_name}' for property '{name}'"
                        ),
                    );
                }
                if *strict_animation
                    && let Err(token_name) =
                        self.validate_animation_value_with_variables(&decl.value, variables)
                {
                    push_indexed_style_diagnostic(
                        &mut diagnostics,
                        name.clone(),
                        format!("unresolved animation token reference '{token_name}'"),
                    );
                    return;
                }
                if *background_image {
                    let resolved = self.resolve_value_with_variables(&decl.value, variables);
                    if !is_supported_background_image(&resolved) {
                        push_indexed_style_diagnostic(
                            &mut diagnostics,
                            name.clone(),
                            format!("unsupported background-image '{resolved}'"),
                        );
                        return;
                    }
                }
                apply_declaration(style, name, &decl.value, self, variables);
            }
        }
    }

    #[cfg(test)]
    pub(super) fn apply_property_value_no_diagnostics(
        &self,
        style: &mut ComputedStyle,
        property: &str,
        value: &StyleValue,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        if property.starts_with("--") {
            variables.insert(property.to_string(), value.clone());
            return;
        }
        if let Some(status) = style_profile_status(property)
            && !matches!(status, StyleProfileStatus::Implemented)
        {
            return;
        }
        if !is_supported_css_property(property) {
            return;
        }
        if contains_deprecated_token_reference(value) {
            return;
        }
        if is_strict_animation_property(property)
            && self
                .validate_animation_value_with_variables(value, variables)
                .is_err()
        {
            return;
        }
        if property == "background-image" {
            let resolved = self.resolve_value_with_variables(value, variables);
            if !is_supported_background_image(&resolved) {
                return;
            }
        }
        apply_declaration(style, property, value, self, variables);
    }

    pub(super) fn apply_declaration_with_diagnostics(
        &self,
        style: &mut ComputedStyle,
        decl: &Declaration,
        selector: Option<String>,
        diagnostics: &mut Vec<StyleDiagnostic>,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        if decl.property.starts_with("--") {
            variables.insert(decl.property.clone(), decl.value.clone());
            return;
        }
        if let Some(status) = style_profile_status(&decl.property) {
            match status {
                StyleProfileStatus::Implemented => {}
                StyleProfileStatus::DiagnosticOnly => {
                    diagnostics.push(StyleDiagnostic {
                        property: decl.property.clone(),
                        selector,
                        message: format!(
                            "diagnostic-only CSS property '{}' is accepted by the parser but not lowered into ComputedStyle",
                            decl.property
                        ),
                    });
                    return;
                }
                StyleProfileStatus::Deferred => {
                    diagnostics.push(StyleDiagnostic {
                        property: decl.property.clone(),
                        selector,
                        message: format!(
                            "deferred CSS property '{}' is accepted by the parser but not lowered in the current painter profile",
                            decl.property
                        ),
                    });
                    return;
                }
                StyleProfileStatus::OutOfScope => {
                    diagnostics.push(StyleDiagnostic {
                        property: decl.property.clone(),
                        selector,
                        message: format!(
                            "unsupported CSS property '{}' is out-of-scope for the MESH shell CSS profile",
                            decl.property
                        ),
                    });
                    return;
                }
            }
        }
        if !is_supported_css_property(&decl.property) {
            diagnostics.push(StyleDiagnostic {
                property: decl.property.clone(),
                selector,
                message: format!("unsupported CSS property '{}'", decl.property),
            });
            return;
        }
        if contains_deprecated_token_reference(&decl.value) {
            diagnostics.push(StyleDiagnostic {
                property: decl.property.clone(),
                selector: selector.clone(),
                message: "deprecated token() references are not supported; use var(--...)"
                    .to_string(),
            });
            return;
        }
        if let StyleValue::Var(name) = &decl.value
            && !is_strict_animation_property(&decl.property)
            && !variables.contains_key(name)
            && self.cached_theme_token_value(name).is_missing()
        {
            diagnostics.push(StyleDiagnostic {
                property: decl.property.clone(),
                selector: selector.clone(),
                message: format!(
                    "unsupported CSS variable reference '{name}' for property '{}'",
                    decl.property
                ),
            });
        }
        if is_strict_animation_property(&decl.property)
            && let Err(token_name) =
                self.validate_animation_value_with_variables(&decl.value, variables)
        {
            diagnostics.push(StyleDiagnostic {
                property: decl.property.clone(),
                selector,
                message: format!("unresolved animation token reference '{token_name}'"),
            });
            return;
        }
        if decl.property == "background-image" {
            let resolved = self.resolve_value_with_variables(&decl.value, variables);
            if !is_supported_background_image(&resolved) {
                diagnostics.push(StyleDiagnostic {
                    property: decl.property.clone(),
                    selector,
                    message: format!("unsupported background-image '{resolved}'"),
                });
                return;
            }
        }
        apply_declaration(style, &decl.property, &decl.value, self, variables);
    }
}

pub(super) fn push_indexed_style_diagnostic(
    diagnostics: &mut Option<(&str, &mut Vec<StyleDiagnostic>)>,
    property: String,
    message: String,
) {
    if let Some((selector, diagnostics)) = diagnostics.as_mut() {
        diagnostics.push(StyleDiagnostic {
            property,
            selector: Some((*selector).to_string()),
            message,
        });
    }
}

pub(super) fn push_indexed_static_style_diagnostic(
    diagnostics: &mut Option<(&str, &mut Vec<StyleDiagnostic>)>,
    property: &str,
    message: &str,
) {
    if let Some((selector, diagnostics)) = diagnostics.as_mut() {
        diagnostics.push(StyleDiagnostic {
            property: property.to_owned(),
            selector: Some((*selector).to_string()),
            message: message.to_owned(),
        });
    }
}

css_property_table! {
fn apply_declaration(
    style: &mut ComputedStyle,
    property: &str,
    value: &StyleValue,
    resolver: &StyleResolver,
    variables: &HashMap<String, StyleValue>,
) {
        "background" | "background-color" => {
            style.background_color = resolver.resolve_color_with_variables(value, variables)
        }
        "color" => style.color = resolver.resolve_color_with_variables(value, variables),
        "border" => resolver.with_resolved_str(value, variables, |resolved| {
            apply_border_shorthand(style, resolved)
        }),
        "border-color" => {
            style.border_color = resolver.with_resolved_str(value, variables, |resolved| {
                parse_border_color_shorthand(resolved)
            })
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
            style.font_family = resolver
                .resolve_value_with_variables(value, variables)
                .into()
        }
        "font-style" => {
            style.font_style = resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "italic" | "oblique" => FontStyle::Italic,
                _ => FontStyle::Normal,
            });
        }
        "letter-spacing" => {
            style.letter_spacing = resolver.resolve_number_with_variables(value, variables)
        }
        "text-overflow" => {
            style.text_overflow = resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "ellipsis" => TextOverflow::Ellipsis,
                _ => TextOverflow::Clip,
            });
        }
        "white-space" => {
            style.white_space = resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "nowrap" => WhiteSpace::Nowrap,
                _ => WhiteSpace::Normal,
            });
        }
        "line-height" => {
            style.line_height = resolver.resolve_number_with_variables(value, variables)
        }
        "padding" => {
            style.padding = resolver
                .with_resolved_str(value, variables, |resolved| parse_edges_shorthand(resolved))
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
            style.margin = resolver
                .with_resolved_str(value, variables, |resolved| parse_edges_shorthand(resolved))
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
            style.border_radius = resolver
                .with_resolved_str(value, variables, |resolved| parse_corners_shorthand(resolved))
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
            style.border_width = resolver
                .with_resolved_str(value, variables, |resolved| parse_edges_shorthand(resolved))
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
                resolver.with_resolved_str(value, variables, |resolved| parse_transform(resolved))
        }
        "box-shadow" => {
            style.box_shadow =
                resolver.with_resolved_str(value, variables, |resolved| parse_box_shadow(resolved))
        }
        "background-image" => {
            style.background_paint = resolver.with_resolved_str(value, variables, |resolved| {
                parse_background_image(resolved)
            });
        }
        "filter" => {
            style.filter =
                resolver.with_resolved_str(value, variables, |resolved| parse_filter(resolved))
        }
        "backdrop-filter" => {
            style.backdrop_filter =
                resolver.with_resolved_str(value, variables, |resolved| parse_filter(resolved))
        }
        "transition-duration" => {
            first_transition_mut(&mut style.transitions).duration_ms =
                resolver.with_resolved_str(value, variables, |resolved| parse_first_time_ms(resolved))
        }
        "transition-delay" => {
            first_transition_mut(&mut style.transitions).delay_ms =
                resolver.with_resolved_str(value, variables, |resolved| parse_first_time_ms(resolved))
        }
        "transition-timing-function" => {
            first_transition_mut(&mut style.transitions).easing =
                resolver.with_resolved_str(value, variables, |resolved| {
                    parse_easing_keyword(first_comma_item(resolved))
                })
        }
        "transition-property" => {
            first_transition_mut(&mut style.transitions).properties =
                resolver.with_resolved_str(value, variables, |resolved| {
                    parse_transition_properties(resolved)
                })
        }
        "transition" => {
            let resolved = resolver.resolve_value_with_variables(value, variables);
            style.transitions = parse_transition_shorthand(&resolved);
        }
        "animation-name" => {
            first_animation_mut(&mut style.animations).name =
                resolver.with_resolved_str(value, variables, |resolved| {
                    parse_animation_name(first_comma_item(resolved))
                })
        }
        "animation-duration" => {
            first_animation_mut(&mut style.animations).duration_ms =
                resolver.with_resolved_str(value, variables, |resolved| parse_first_time_ms(resolved))
        }
        "animation-delay" => {
            first_animation_mut(&mut style.animations).delay_ms =
                resolver.with_resolved_str(value, variables, |resolved| parse_first_time_ms(resolved))
        }
        "animation-timing-function" => {
            first_animation_mut(&mut style.animations).easing =
                resolver.with_resolved_str(value, variables, |resolved| {
                    parse_easing_keyword(first_comma_item(resolved))
                })
        }
        "animation-iteration-count" => {
            first_animation_mut(&mut style.animations).iteration_count =
                resolver.with_resolved_str(value, variables, |resolved| {
                    parse_animation_iteration_count(first_comma_item(resolved))
                })
        }
        "animation-direction" => {
            first_animation_mut(&mut style.animations).direction =
                resolver.with_resolved_str(value, variables, |resolved| {
                    parse_animation_direction(first_comma_item(resolved))
                })
        }
        "animation-fill-mode" => {
            first_animation_mut(&mut style.animations).fill_mode =
                resolver.with_resolved_str(value, variables, |resolved| {
                    parse_animation_fill_mode(first_comma_item(resolved))
                })
        }
        "animation-play-state" => {
            first_animation_mut(&mut style.animations).play_state =
                resolver.with_resolved_str(value, variables, |resolved| {
                    parse_animation_play_state(first_comma_item(resolved))
                })
        }
        "animation" => {
            style.animations =
                parse_animation_shorthand(&resolver.resolve_value_with_variables(value, variables))
        }
        "transform-origin" => {
            style.transform_origin = resolver
                .with_resolved_str(value, variables, |resolved| parse_transform_origin(resolved))
        }
        "overflow" => {
            let (x, y) = resolver.with_resolved_str(value, variables, |resolved| {
                parse_overflow_shorthand(resolved)
            });
            style.overflow_x = x;
            style.overflow_y = y;
        }
        "overflow-x" => {
            style.overflow_x =
                resolver.with_resolved_str(value, variables, |resolved| parse_overflow(resolved))
        }
        "overflow-y" => {
            style.overflow_y =
                resolver.with_resolved_str(value, variables, |resolved| parse_overflow(resolved))
        }
        "width" => {
            style.width =
                resolver.with_resolved_str(value, variables, |resolved| parse_dimension(resolved))
        }
        "height" => {
            style.height =
                resolver.with_resolved_str(value, variables, |resolved| parse_dimension(resolved))
        }
        "min-width" => {
            style.min_width = resolver
                .with_resolved_str(value, variables, |resolved| parse_size_constraint(resolved))
        }
        "max-width" => {
            style.max_width = resolver
                .with_resolved_str(value, variables, |resolved| parse_size_constraint(resolved))
        }
        "min-height" => {
            style.min_height = resolver
                .with_resolved_str(value, variables, |resolved| parse_size_constraint(resolved))
        }
        "max-height" => {
            style.max_height = resolver
                .with_resolved_str(value, variables, |resolved| parse_size_constraint(resolved))
        }
        "flex-grow" => style.flex_grow = resolver.resolve_number_with_variables(value, variables),
        "flex-shrink" => {
            style.flex_shrink = resolver.resolve_number_with_variables(value, variables)
        }
        "flex-basis" => {
            style.flex_basis =
                resolver.with_resolved_str(value, variables, |resolved| parse_dimension(resolved))
        }
        "flex" => {
            resolver.with_resolved_str(value, variables, |resolved| {
                let v = resolved.trim();
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
            });
        }
        "flex-wrap" => {
            style.flex_wrap = resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "wrap" => FlexWrap::Wrap,
                "wrap-reverse" => FlexWrap::WrapReverse,
                _ => FlexWrap::NoWrap,
            });
        }
        "align-self" => {
            style.align_self = resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "auto" => AlignSelf::Auto,
                "start" | "flex-start" => AlignSelf::Start,
                "end" | "flex-end" => AlignSelf::End,
                "center" => AlignSelf::Center,
                "baseline" => AlignSelf::Baseline,
                _ => AlignSelf::Stretch,
            });
        }
        "align-content" => {
            style.align_content = resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "start" | "flex-start" => AlignContent::Start,
                "end" | "flex-end" => AlignContent::End,
                "center" => AlignContent::Center,
                "space-between" => AlignContent::SpaceBetween,
                "space-around" => AlignContent::SpaceAround,
                _ => AlignContent::Stretch,
            });
        }
        "flex-direction" => {
            style.direction = resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "column" | "column-reverse" => FlexDirection::Column,
                _ => FlexDirection::Row,
            });
        }
        "direction" => {
            resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "rtl" => style.text_direction = TextDirection::Rtl,
                "ltr" => style.text_direction = TextDirection::Ltr,
                other => tracing::warn!(
                    "direction: {other} is not valid; use flex-direction for layout direction"
                ),
            });
        }
        "justify-content" => {
            style.justify_content = resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "center" => JustifyContent::Center,
                "end" | "flex-end" => JustifyContent::End,
                "space-between" => JustifyContent::SpaceBetween,
                "space-around" => JustifyContent::SpaceAround,
                _ => JustifyContent::Start,
            });
        }
        "align-items" => {
            style.align_items = resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "center" => AlignItems::Center,
                "start" | "flex-start" => AlignItems::Start,
                "end" | "flex-end" => AlignItems::End,
                _ => AlignItems::Stretch,
            });
        }
        "text-align" => {
            style.text_align = resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "center" => TextAlign::Center,
                "right" => TextAlign::Right,
                _ => TextAlign::Left,
            });
        }
        "display" => {
            style.display = resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "none" => Display::None,
                _ => Display::Flex,
            });
        }
        "visibility" => {
            style.visibility = resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "hidden" => Visibility::Hidden,
                "collapse" => Visibility::Collapse,
                _ => Visibility::Visible,
            });
        }
        "position" => {
            style.position = resolver.with_resolved_str(value, variables, |resolved| match resolved {
                "relative" => Position::Relative,
                "absolute" => Position::Absolute,
                "fixed" => Position::Fixed,
                _ => Position::Static,
            });
        }
        "z-index" => {
            let v = resolver.resolve_value_with_variables(value, variables);
            style.z_index = v.trim().parse::<i32>().unwrap_or(0);
        }
        "mix-blend-mode" => {
            style.mix_blend_mode = resolver.with_resolved_str(value, variables, |resolved| match resolved.trim() {
                "multiply" => BlendMode::Multiply,
                "screen" => BlendMode::Screen,
                _ => BlendMode::Normal,
            });
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
            let edges = resolver
                .with_resolved_str(value, variables, |resolved| parse_edges_shorthand(resolved));
            style.inset_top = Some(edges.top);
            style.inset_right = Some(edges.right);
            style.inset_bottom = Some(edges.bottom);
            style.inset_left = Some(edges.left);
        }
        "--icon-fill" => {
            style.icon_fill = Some(resolver.resolve_number_with_variables(value, variables))
        }
        "--icon-weight" => {
            style.icon_weight = Some(resolver.resolve_number_with_variables(value, variables))
        }
        "--icon-grade" => {
            style.icon_grade = Some(resolver.resolve_number_with_variables(value, variables))
        }
        "--icon-optical-size" => {
            style.icon_optical_size = Some(resolver.resolve_number_with_variables(value, variables))
        }
        "tooltip-anchor" => {
            let resolved = resolver.resolve_value_with_variables_mode(value, variables, false);
            if let Some(anchor) = TooltipAnchor::from_css(&resolved) {
                style.tooltip_anchor = anchor;
            }
        }
        "tooltip-offset" => {
            let resolved = resolver.resolve_value_with_variables_mode(value, variables, false);
            let parts: Vec<&str> = resolved.split_whitespace().collect();
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (
                    super::parse::trim_px_suffix(parts[0]).parse::<f32>(),
                    super::parse::trim_px_suffix(parts[1]).parse::<f32>(),
                ) {
                    style.tooltip_offset = Some((x, y));
                }
            }
        }
    }
}

pub(super) fn selector_to_diagnostic_string(selector: &Selector) -> String {
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

pub(super) fn theme_reference_to_token_name(name: &str) -> String {
    let name = name.trim();
    let Some(variable) = name.strip_prefix("--") else {
        return name.to_string();
    };
    css_custom_property_to_token_name(variable)
}

pub(super) fn css_custom_property_to_token_name(variable: &str) -> String {
    let Some((group, rest)) = variable.split_once('-') else {
        return variable.to_string();
    };

    let rest = match group {
        "animation" => canonicalize_prefixed(
            rest,
            &["curves-bezier", "default", "duration", "opacity", "scale"],
        ),
        "border" => canonicalize_prefixed(rest, &["style", "width"]),
        "shadow" => canonicalize_prefixed(rest, &["colored", "umbra"]),
        "shape" => canonicalize_prefixed(rest, &["corner"]),
        "spacing" => canonicalize_prefixed(rest, &["inset"]),
        "state" => canonicalize_suffixed(rest, &["opacity"]),
        "icon" => canonicalize_prefixed(rest, &["size"]),
        "typography" => canonicalize_prefixed(
            rest,
            &[
                "family",
                "line-height",
                "scale-body-large",
                "scale-body-medium",
                "scale-body-small",
                "scale-display-large",
                "scale-display-medium",
                "scale-display-small",
                "scale-headline-large",
                "scale-headline-medium",
                "scale-headline-small",
                "scale-label-large",
                "scale-label-medium",
                "scale-label-small",
                "scale-title-large",
                "scale-title-medium",
                "scale-title-small",
                "size",
                "tracking",
                "weight",
            ],
        ),
        "color" | "elevation" | "radius" => rest.to_string(),
        _ => rest.replace('-', "."),
    };

    format!("{group}.{rest}")
}

pub(super) fn canonicalize_prefixed(value: &str, prefixes: &[&str]) -> String {
    let mut prefixes = prefixes.to_vec();
    prefixes.sort_by_key(|prefix| std::cmp::Reverse(prefix.len()));
    for prefix in prefixes {
        if value == prefix {
            return prefix.to_string();
        }
        if let Some(rest) = value.strip_prefix(&format!("{prefix}-")) {
            return format!("{}.{}", prefix.replace('-', "."), rest);
        }
    }
    value.to_string()
}

pub(super) fn canonicalize_suffixed(value: &str, suffixes: &[&str]) -> String {
    for suffix in suffixes {
        if let Some(rest) = value.strip_suffix(&format!("-{suffix}")) {
            return format!("{rest}.{suffix}");
        }
    }
    value.to_string()
}
