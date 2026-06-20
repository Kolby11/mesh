use super::parse::*;
use super::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue};
use mesh_core_theme::{Theme, TokenValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

fn empty_variables() -> &'static HashMap<String, StyleValue> {
    static EMPTY: OnceLock<HashMap<String, StyleValue>> = OnceLock::new();
    EMPTY.get_or_init(HashMap::new)
}

// Reusable scratch HashMap for CSS custom-property variable resolution.
// Cleared at the start of each resolve call to avoid per-node allocations
// while retaining allocated capacity across calls on the same thread.
thread_local! {
    static VARIABLE_SCRATCH: RefCell<HashMap<String, StyleValue>> =
        RefCell::new(HashMap::new());
    static CANDIDATE_RULE_SCRATCH: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// The five CSS-inherited fields from a parent node. Used instead of cloning
/// the full `ComputedStyle` (~60 fields) when passing parent context into
/// recursive restyle calls.
struct ParentInheritedStyle {
    color: Color,
    font_family: Arc<str>,
    font_size: f32,
    font_weight: u16,
    line_height: f32,
}

impl From<&ComputedStyle> for ParentInheritedStyle {
    fn from(s: &ComputedStyle) -> Self {
        Self {
            color: s.color,
            font_family: s.font_family.clone(),
            font_size: s.font_size,
            font_weight: s.font_weight,
            line_height: s.line_height,
        }
    }
}

/// Resolves style values against a theme's design tokens.
pub struct StyleResolver<'a> {
    theme: &'a Theme,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleNodeAttrs<'a> {
    tag: &'a str,
    classes: ClassList<'a>,
    id: Option<&'a str>,
    key: Option<&'a str>,
    module_id: Option<&'a str>,
    state: ElementState,
    state_mask: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum ClassList<'a> {
    #[default]
    Empty,
    Borrowed(&'a [String]),
    Owned(Vec<String>),
}

impl<'a> ClassList<'a> {
    fn from_class_slice(classes: &'a [String]) -> Self {
        if classes.is_empty() {
            return Self::Empty;
        }
        if classes
            .iter()
            .any(|class| class.is_empty() || class.chars().any(char::is_whitespace))
        {
            Self::Owned(split_class_values(classes.iter().map(String::as_str)))
        } else {
            Self::Borrowed(classes)
        }
    }

    fn iter(&self) -> impl Iterator<Item = &str> {
        match self {
            Self::Empty => ClassListIter::Empty,
            Self::Borrowed(classes) => ClassListIter::Borrowed(classes.iter()),
            Self::Owned(classes) => ClassListIter::Owned(classes.iter()),
        }
    }

    fn has_class(&self, class: &str) -> bool {
        self.iter().any(|candidate| candidate == class)
    }
}

enum ClassListIter<'a> {
    Empty,
    Borrowed(std::slice::Iter<'a, String>),
    Owned(std::slice::Iter<'a, String>),
}

impl<'a> Iterator for ClassListIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Empty => None,
            Self::Borrowed(iter) => iter.next().map(String::as_str),
            Self::Owned(iter) => iter.next().map(String::as_str),
        }
    }
}

fn split_class_values<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    values
        .flat_map(str::split_whitespace)
        .filter(|class| !class.is_empty())
        .map(str::to_owned)
        .collect()
}

impl<'a> StyleNodeAttrs<'a> {
    pub fn new(
        tag: &'a str,
        classes: &'a [String],
        id: Option<&'a str>,
        state: ElementState,
    ) -> Self {
        Self {
            tag,
            classes: ClassList::from_class_slice(classes),
            id,
            key: None,
            module_id: None,
            state,
            state_mask: active_state_mask(state),
        }
    }

    pub fn from_node(node: &'a mut crate::tree::WidgetNode) -> Self {
        node.refresh_class_tokens_cache();
        let classes = ClassList::from_class_slice(node.class_tokens());
        Self {
            tag: node.tag.as_str(),
            classes,
            id: node.attributes.get("id").map(|value| value.as_str()),
            key: node.attributes.get("_mesh_key").map(|value| value.as_str()),
            module_id: node
                .attributes
                .get("_mesh_module_id")
                .map(|value| value.as_str()),
            state: node.state,
            state_mask: active_state_mask(node.state),
        }
    }

    fn has_class(&self, class: &str) -> bool {
        self.classes.has_class(class)
    }

    fn id(&self) -> Option<&str> {
        self.id
    }

    fn module_id(&self) -> Option<&str> {
        self.module_id
    }
}

/// Bucketed view of style rules for candidate filtering.
///
/// The index owns its keys, so it can be cached across restyle passes — the
/// caller provides the rules slice it was built from for each lookup and the
/// index validates identity through `is_for()`.
#[derive(Debug, Clone)]
pub struct StyleRuleIndex {
    rules_ptr: usize,
    rules_len: usize,
    tag: HashMap<String, Vec<usize>>,
    class: HashMap<String, Vec<usize>>,
    id: HashMap<String, Vec<usize>>,
    state: Vec<(u32, Vec<usize>)>,
    /// Reverse index: maps individual state bits (e.g., STATE_HOVERED=1)
    /// to the rule indices that depend on that specific state.
    /// Separates per-bit dependencies from the combined bitmask entries
    /// in `state` used for forward candidate-rule lookup.
    state_to_rules: HashMap<u32, Vec<usize>>,
    fallback: Vec<usize>,
}

impl StyleRuleIndex {
    pub fn new(rules: &[StyleRule]) -> Self {
        let mut index = Self {
            rules_ptr: rules.as_ptr() as usize,
            rules_len: rules.len(),
            tag: HashMap::new(),
            class: HashMap::new(),
            id: HashMap::new(),
            state: Vec::new(),
            state_to_rules: HashMap::new(),
            fallback: Vec::new(),
        };
        for (idx, rule) in rules.iter().enumerate() {
            index.index_selector(idx, &rule.selector);
        }
        index
    }

    /// Returns true when this index was built from the given rules slice
    /// (same memory + length). Use to decide whether to reuse or rebuild.
    pub fn is_for(&self, rules: &[StyleRule]) -> bool {
        self.rules_ptr == rules.as_ptr() as usize && self.rules_len == rules.len()
    }

    pub fn candidate_rules<'a>(
        &self,
        rules: &'a [StyleRule],
        attrs: &StyleNodeAttrs,
    ) -> Vec<&'a StyleRule> {
        let mut candidates = Vec::with_capacity(self.fallback.len().saturating_add(8));
        self.for_each_candidate_rule(rules, attrs, |rule| candidates.push(rule));
        candidates
    }

    pub fn for_each_candidate_rule<'a>(
        &self,
        rules: &'a [StyleRule],
        attrs: &StyleNodeAttrs,
        mut visit: impl FnMut(&'a StyleRule),
    ) {
        CANDIDATE_RULE_SCRATCH.with(|scratch| {
            let mut ids = scratch.borrow_mut();
            ids.clear();
            ids.extend_from_slice(&self.fallback);
            if let Some(tag) = self.tag.get(attrs.tag) {
                ids.extend_from_slice(tag);
            }
            for class in attrs.classes.iter() {
                if let Some(class_ids) = self.class.get(class) {
                    ids.extend_from_slice(class_ids);
                }
            }
            if let Some(id) = attrs.id()
                && let Some(id_ids) = self.id.get(id)
            {
                ids.extend_from_slice(id_ids);
            }
            for (state_bit, state_ids) in &self.state {
                if attrs.state_mask & *state_bit != 0 {
                    ids.extend_from_slice(state_ids);
                }
            }
            ids.sort_unstable();
            ids.dedup();
            for &idx in ids.iter() {
                if let Some(rule) = rules.get(idx) {
                    visit(rule);
                }
            }
        });
    }

    fn index_selector(&mut self, idx: usize, selector: &Selector) {
        // Index state bits from compound selector parts so that
        // compound rules like `button:hover` also populate state_to_rules.
        if let Selector::Compound(parts) = selector {
            for part in parts {
                if let Selector::State(_, state) = part {
                    self.index_state_selector(idx, state);
                }
            }
        }
        match selector_index_key(selector) {
            Some(SelectorIndexKey::Tag(tag)) => {
                self.tag.entry(tag.to_string()).or_default().push(idx)
            }
            Some(SelectorIndexKey::Class(class)) => {
                self.class.entry(class.to_string()).or_default().push(idx)
            }
            Some(SelectorIndexKey::Id(id)) => self.id.entry(id.to_string()).or_default().push(idx),
            Some(SelectorIndexKey::State(state)) => self.index_state_selector(idx, state),
            None => self.fallback.push(idx),
        }
    }

    fn index_state_selector(&mut self, idx: usize, state: &str) {
        let Some(state_bit) = state_name_bit(state) else {
            return;
        };
        if let Some((_, ids)) = self
            .state
            .iter_mut()
            .find(|(existing_bit, _)| *existing_bit == state_bit)
        {
            ids.push(idx);
        } else {
            self.state.push((state_bit, vec![idx]));
        }
        // Populate reverse index: map the individual state bit to this rule.
        self.state_to_rules.entry(state_bit).or_default().push(idx);
    }

    /// Returns the indices of all rules that depend on the given state bit.
    ///
    /// This is an O(1) reverse lookup — no iteration over rules needed.
    /// Returns an empty slice if no rules reference this state bit.
    pub fn rules_for_state_bit(&self, bit: u32) -> &[usize] {
        self.state_to_rules
            .get(&bit)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

enum SelectorIndexKey<'a> {
    Tag(&'a str),
    Class(&'a str),
    Id(&'a str),
    State(&'a str),
}

fn ensure_index<'cache>(
    rules: &[StyleRule],
    cache: &'cache mut Option<StyleRuleIndex>,
) -> &'cache StyleRuleIndex {
    let needs_rebuild = !cache.as_ref().is_some_and(|index| index.is_for(rules));
    if needs_rebuild {
        *cache = Some(StyleRuleIndex::new(rules));
    }
    cache.as_ref().expect("index populated above")
}

impl<'a> StyleResolver<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self { theme }
    }

    pub fn resolve_value(&self, value: &StyleValue) -> String {
        self.resolve_value_with_variables(value, empty_variables())
    }

    fn resolve_value_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> String {
        self.resolve_value_with_variables_mode(value, variables, false)
    }

    fn resolve_value_with_variables_mode(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        strict_animation_tokens: bool,
    ) -> String {
        match value {
            StyleValue::Literal(s) => {
                resolve_embedded_tokens(s, self.theme, strict_animation_tokens).unwrap_or_default()
            }
            StyleValue::Token(name) => match self.theme.token(name) {
                Some(TokenValue::String(s)) => s.clone(),
                Some(TokenValue::Number(n)) => format!("{n}"),
                Some(TokenValue::Bool(b)) => format!("{b}"),
                None => {
                    if strict_animation_tokens && name.starts_with("animation.") {
                        return String::new();
                    }
                    tracing::warn!("unresolved theme token: {name}");
                    String::new()
                }
            },
            StyleValue::Var(name) => variables
                .get(name)
                .map(|value| {
                    self.resolve_value_with_variables_mode(
                        value,
                        variables,
                        strict_animation_tokens,
                    )
                })
                .unwrap_or_default(),
        }
    }

    fn validate_animation_value_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> Result<(), String> {
        match value {
            StyleValue::Literal(value) => {
                if let Some(name) = find_unresolved_animation_token(value, self.theme) {
                    return Err(name);
                }
                Ok(())
            }
            StyleValue::Token(name) => {
                if name.starts_with("animation.") && self.theme.token(name).is_none() {
                    return Err(name.clone());
                }
                Ok(())
            }
            StyleValue::Var(name) => variables
                .get(name)
                .map(|value| self.validate_animation_value_with_variables(value, variables))
                .unwrap_or(Ok(())),
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
        let attrs = StyleNodeAttrs::new(tag, classes, id, state);
        self.resolve_node_style_with_attrs_no_diagnostics(rules, &attrs, context)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn resolve_node_style_for_module(
        &self,
        rules: &[StyleRule],
        tag: &str,
        classes: &[String],
        id: Option<&str>,
        context: StyleContext,
        state: ElementState,
        module_id: Option<&str>,
    ) -> ComputedStyle {
        let mut attrs = StyleNodeAttrs::new(tag, classes, id, state);
        attrs.module_id = module_id;
        self.resolve_node_style_with_attrs_no_diagnostics(rules, &attrs, context)
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
        self.resolve_node_style_with_diagnostics_for_module(
            rules, tag, classes, id, context, state, None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn resolve_node_style_with_diagnostics_for_module(
        &self,
        rules: &[StyleRule],
        tag: &str,
        classes: &[String],
        id: Option<&str>,
        context: StyleContext,
        state: ElementState,
        module_id: Option<&str>,
    ) -> (ComputedStyle, Vec<StyleDiagnostic>) {
        let mut attrs = StyleNodeAttrs::new(tag, classes, id, state);
        attrs.module_id = module_id;
        self.resolve_node_style_with_attrs(rules, &attrs, context)
    }

    fn resolve_node_style_with_attrs(
        &self,
        rules: &[StyleRule],
        attrs: &StyleNodeAttrs,
        context: StyleContext,
    ) -> (ComputedStyle, Vec<StyleDiagnostic>) {
        let index = StyleRuleIndex::new(rules);
        self.resolve_node_style_with_attrs_indexed(rules, &index, attrs, context)
    }

    fn resolve_node_style_with_attrs_no_diagnostics(
        &self,
        rules: &[StyleRule],
        attrs: &StyleNodeAttrs,
        context: StyleContext,
    ) -> ComputedStyle {
        let index = StyleRuleIndex::new(rules);
        self.resolve_node_style_with_attrs_indexed_no_diagnostics(rules, &index, attrs, context)
    }

    fn resolve_node_style_with_attrs_indexed_no_diagnostics(
        &self,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        attrs: &StyleNodeAttrs,
        context: StyleContext,
    ) -> ComputedStyle {
        let mut style = ComputedStyle::default();

        if attrs.tag == "column" {
            style.direction = FlexDirection::Column;
        }

        VARIABLE_SCRATCH.with(|scratch| {
            let mut variables = scratch.borrow_mut();
            variables.clear();

            self.apply_theme_component_defaults_no_diagnostics(
                &mut style,
                attrs.tag,
                attrs.module_id(),
                &mut variables,
            );

            index.for_each_candidate_rule(rules, attrs, |rule| {
                if rule_matches_attrs(rule, attrs, context) {
                    for decl in &rule.declarations {
                        self.apply_declaration_no_diagnostics(&mut style, decl, &mut variables);
                    }
                }
            });
        });

        style
    }

    fn resolve_node_style_with_attrs_indexed(
        &self,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        attrs: &StyleNodeAttrs,
        context: StyleContext,
    ) -> (ComputedStyle, Vec<StyleDiagnostic>) {
        let mut style = ComputedStyle::default();
        let mut diagnostics = Vec::new();
        let mut variables = HashMap::new();

        if attrs.tag == "column" {
            style.direction = FlexDirection::Column;
        }

        self.apply_theme_component_defaults(
            &mut style,
            attrs.tag,
            attrs.module_id(),
            &mut diagnostics,
            &mut variables,
        );

        index.for_each_candidate_rule(rules, attrs, |rule| {
            if rule_matches_attrs(rule, attrs, context) {
                for decl in &rule.declarations {
                    self.apply_declaration_with_diagnostics(
                        &mut style,
                        decl,
                        Some(selector_to_diagnostic_string(&rule.selector)),
                        &mut diagnostics,
                        &mut variables,
                    );
                }
            }
        });

        (style, diagnostics)
    }

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

    pub fn restyle_subtree(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
    ) {
        let index = StyleRuleIndex::new(rules);
        self.restyle_subtree_with_index(node, rules, &index, context, None);
    }

    /// Like `restyle_subtree` but reuses a caller-provided index. The index
    /// must have been built from the same `rules` slice; this is verified
    /// with `is_for()` and the index is rebuilt in place if not.
    pub fn restyle_subtree_cached(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        index_cache: &mut Option<StyleRuleIndex>,
    ) {
        let index = ensure_index(rules, index_cache);
        self.restyle_subtree_with_index(node, rules, index, context, None);
    }

    pub fn restyle_subtree_children(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
    ) {
        let index = StyleRuleIndex::new(rules);
        let parent = ParentInheritedStyle::from(&node.computed_style);
        for child in &mut node.children {
            self.restyle_subtree_with_index(child, rules, &index, context, Some(&parent));
        }
    }

    /// Like `restyle_subtree_children` but reuses a caller-provided index.
    pub fn restyle_subtree_children_cached(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        index_cache: &mut Option<StyleRuleIndex>,
    ) {
        let index = ensure_index(rules, index_cache);
        let parent = ParentInheritedStyle::from(&node.computed_style);
        for child in &mut node.children {
            self.restyle_subtree_with_index(child, rules, index, context, Some(&parent));
        }
    }

    fn restyle_subtree_with_index(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        parent_style: Option<&ParentInheritedStyle>,
    ) {
        let attrs = StyleNodeAttrs::from_node(node);
        node.computed_style = self
            .resolve_node_style_with_attrs_indexed_no_diagnostics(rules, index, &attrs, context);
        if let Some(parent) = parent_style {
            inherit_retained_text_style(&mut node.computed_style, parent);
        }

        let parent = ParentInheritedStyle::from(&node.computed_style);
        for child in &mut node.children {
            self.restyle_subtree_with_index(child, rules, index, context, Some(&parent));
        }
    }

    pub fn restyle_subtree_for_keys(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        target_keys: &std::collections::HashSet<String>,
    ) {
        let index = StyleRuleIndex::new(rules);
        self.restyle_subtree_for_keys_with_index(node, rules, &index, context, target_keys);
    }

    pub fn restyle_subtree_for_keys_cached(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        index: &mut Option<StyleRuleIndex>,
        target_keys: &std::collections::HashSet<String>,
    ) {
        let idx = ensure_index(rules, index);
        self.restyle_subtree_for_keys_with_index(node, rules, idx, context, target_keys);
    }

    fn restyle_subtree_for_keys_with_index(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        target_keys: &std::collections::HashSet<String>,
    ) {
        self.restyle_subtree_for_keys_with_index_and_inheritance(
            node,
            rules,
            index,
            context,
            target_keys,
            None,
        );
    }

    fn restyle_subtree_for_keys_with_index_and_inheritance(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        target_keys: &std::collections::HashSet<String>,
        parent_style: Option<&ParentInheritedStyle>,
    ) {
        let is_target = node
            .attributes
            .get("_mesh_key")
            .is_some_and(|key| target_keys.contains(key));
        // A node should have its style recomputed if it is a direct target, or
        // if it is a descendant of a restyled node (parent_style.is_some()),
        // in which case it must inherit updated values from its restyled parent.
        let should_restyle = is_target || parent_style.is_some();

        if should_restyle {
            // Recompute this node's style.
            // For target nodes: apply new pseudo-class rules.
            // For descendants of targets: inherit updated values from the
            // restyled ancestor.
            let attrs = StyleNodeAttrs::from_node(node);
            node.computed_style = self.resolve_node_style_with_attrs_indexed_no_diagnostics(
                rules, index, &attrs, context,
            );
            if let Some(parent) = parent_style {
                inherit_retained_text_style(&mut node.computed_style, parent);
            }

            // Pass this node's style down so children inherit from it.
            let child_parent = ParentInheritedStyle::from(&node.computed_style);
            for child in &mut node.children {
                self.restyle_subtree_for_keys_with_index_and_inheritance(
                    child,
                    rules,
                    index,
                    context,
                    target_keys,
                    Some(&child_parent),
                );
            }
        } else {
            // This node is not a target and is not in an affected subtree.
            // Don't restyle it, but keep recursing — target nodes may be
            // deeper in the tree.
            for child in &mut node.children {
                self.restyle_subtree_for_keys_with_index_and_inheritance(
                    child,
                    rules,
                    index,
                    context,
                    target_keys,
                    None,
                );
            }
        }
    }

    fn apply_theme_component_defaults(
        &self,
        style: &mut ComputedStyle,
        tag: &str,
        module_id: Option<&str>,
        diagnostics: &mut Vec<StyleDiagnostic>,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        if let Some(defaults) = self.theme.component_defaults("base") {
            self.apply_theme_defaults_map(style, "base", defaults, diagnostics, variables);
        }
        if let Some(defaults) = self.theme.component_defaults(tag) {
            self.apply_theme_defaults_map(style, tag, defaults, diagnostics, variables);
        }
        if let Some(module_id) = module_id {
            if let Some(defaults) = self.theme.module_component_defaults(module_id, "base") {
                self.apply_theme_defaults_map(style, "base", defaults, diagnostics, variables);
            }
            if let Some(defaults) = self.theme.module_component_defaults(module_id, tag) {
                self.apply_theme_defaults_map(style, tag, defaults, diagnostics, variables);
            }
        }
    }

    fn apply_theme_component_defaults_no_diagnostics(
        &self,
        style: &mut ComputedStyle,
        tag: &str,
        module_id: Option<&str>,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        if let Some(defaults) = self.theme.component_defaults("base") {
            self.apply_theme_defaults_map_no_diagnostics(style, defaults, variables);
        }
        if let Some(defaults) = self.theme.component_defaults(tag) {
            self.apply_theme_defaults_map_no_diagnostics(style, defaults, variables);
        }
        if let Some(module_id) = module_id {
            if let Some(defaults) = self.theme.module_component_defaults(module_id, "base") {
                self.apply_theme_defaults_map_no_diagnostics(style, defaults, variables);
            }
            if let Some(defaults) = self.theme.module_component_defaults(module_id, tag) {
                self.apply_theme_defaults_map_no_diagnostics(style, defaults, variables);
            }
        }
    }

    fn apply_theme_defaults_map(
        &self,
        style: &mut ComputedStyle,
        component_name: &str,
        defaults: &mesh_core_theme::ComponentDefaults,
        diagnostics: &mut Vec<StyleDiagnostic>,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        for (property, value) in defaults {
            let declaration = Declaration {
                property: property.clone(),
                value: classify_theme_style_value(value),
            };
            self.apply_declaration_with_diagnostics(
                style,
                &declaration,
                Some(format!("@theme:{component_name}")),
                diagnostics,
                variables,
            );
        }
    }

    fn apply_theme_defaults_map_no_diagnostics(
        &self,
        style: &mut ComputedStyle,
        defaults: &mesh_core_theme::ComponentDefaults,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        for (property, value) in defaults {
            let declaration = Declaration {
                property: property.clone(),
                value: classify_theme_style_value(value),
            };
            self.apply_declaration_no_diagnostics(style, &declaration, variables);
        }
    }

    fn apply_declaration_no_diagnostics(
        &self,
        style: &mut ComputedStyle,
        decl: &Declaration,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        if decl.property.starts_with("--") {
            variables.insert(decl.property.clone(), decl.value.clone());
            return;
        }
        if let Some(status) = style_profile_status(&decl.property)
            && !matches!(status, StyleProfileStatus::Implemented)
        {
            return;
        }
        if !is_supported_css_property(&decl.property) {
            return;
        }
        if is_strict_animation_property(&decl.property)
            && self
                .validate_animation_value_with_variables(&decl.value, variables)
                .is_err()
        {
            return;
        }
        if decl.property == "background-image" {
            let resolved = self.resolve_value_with_variables(&decl.value, variables);
            if !is_supported_background_image(&resolved) {
                return;
            }
        }
        apply_declaration(style, &decl.property, &decl.value, self, variables);
    }

    fn apply_declaration_with_diagnostics(
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
        if let StyleValue::Var(name) = &decl.value
            && !variables.contains_key(name)
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

fn is_strict_animation_property(property: &str) -> bool {
    matches!(
        property,
        "transition"
            | "transition-duration"
            | "transition-delay"
            | "transition-timing-function"
            | "animation"
            | "animation-duration"
            | "animation-delay"
            | "animation-timing-function"
    )
}

fn classify_theme_style_value(value: &str) -> StyleValue {
    let value = value.trim();
    if value.starts_with("token(") && value.ends_with(')') {
        StyleValue::Token(value[6..value.len() - 1].trim().to_string())
    } else if value.starts_with("var(") && value.ends_with(')') {
        StyleValue::Var(value[4..value.len() - 1].trim().to_string())
    } else {
        StyleValue::Literal(value.to_string())
    }
}

fn selector_matches_attrs(selector: &Selector, attrs: &StyleNodeAttrs) -> bool {
    match selector {
        Selector::Universal => true,
        Selector::Tag(t) => t == attrs.tag,
        Selector::Class(c) => attrs.has_class(c),
        Selector::Id(i) => attrs.id() == Some(i.as_str()),
        Selector::State(t, pseudo) => {
            let tag_matches = t == "*" || t == attrs.tag;
            let state_matches = match pseudo.as_str() {
                "hover" | "hovered" => attrs.state.hovered,
                "focus" | "focused" => attrs.state.focused,
                "active" => attrs.state.active,
                "disabled" => attrs.state.disabled,
                "checked" => attrs.state.checked,
                "focus-visible" => attrs.state.focus_visible,
                _ => false,
            };
            tag_matches && state_matches
        }
        Selector::Compound(parts) => parts.iter().all(|s| selector_matches_attrs(s, attrs)),
    }
}

fn rule_matches_attrs(rule: &StyleRule, attrs: &StyleNodeAttrs, context: StyleContext) -> bool {
    selector_matches_attrs(&rule.selector, attrs)
        && rule
            .container_query
            .is_none_or(|query| query.matches(context.container_width, context.container_height))
}

fn inherit_retained_text_style(style: &mut ComputedStyle, parent: &ParentInheritedStyle) {
    let defaults = ComputedStyle::default();
    if style.color.a == 0 {
        style.color = parent.color;
    }
    if style.font_family == defaults.font_family {
        style.font_family = parent.font_family.clone();
    }
    if (style.font_size - defaults.font_size).abs() < f32::EPSILON {
        style.font_size = parent.font_size;
    }
    if style.font_weight == defaults.font_weight {
        style.font_weight = parent.font_weight;
    }
    if (style.line_height - defaults.line_height).abs() < f32::EPSILON {
        style.line_height = parent.line_height;
    }
}

fn selector_index_key(selector: &Selector) -> Option<SelectorIndexKey<'_>> {
    match selector {
        Selector::Tag(tag) => Some(SelectorIndexKey::Tag(tag)),
        Selector::Class(class) => Some(SelectorIndexKey::Class(class)),
        Selector::Id(id) => Some(SelectorIndexKey::Id(id)),
        Selector::State(_, state) => Some(SelectorIndexKey::State(state)),
        Selector::Compound(parts) => {
            let mut best = None;
            for part in parts {
                match part {
                    Selector::Id(id) => return Some(SelectorIndexKey::Id(id)),
                    Selector::Class(class) => best = Some(SelectorIndexKey::Class(class)),
                    Selector::Tag(tag) if best.is_none() => best = Some(SelectorIndexKey::Tag(tag)),
                    Selector::State(_, state) if best.is_none() => {
                        best = Some(SelectorIndexKey::State(state));
                    }
                    Selector::Universal => {}
                    Selector::Tag(_) | Selector::State(_, _) => {}
                    Selector::Compound(_) => return None,
                }
            }
            best
        }
        Selector::Universal => None,
    }
}

const STATE_HOVERED: u32 = 1 << 0;
const STATE_FOCUSED: u32 = 1 << 1;
const STATE_ACTIVE: u32 = 1 << 2;
const STATE_DISABLED: u32 = 1 << 3;
const STATE_READ_ONLY: u32 = 1 << 4;
const STATE_REQUIRED: u32 = 1 << 5;
const STATE_SELECTED: u32 = 1 << 6;
const STATE_CHECKED: u32 = 1 << 7;
const STATE_EXPANDED: u32 = 1 << 8;
const STATE_PRESSED: u32 = 1 << 9;
const STATE_INVALID: u32 = 1 << 10;
const STATE_VALUE: u32 = 1 << 11;
const STATE_FOCUS_VISIBLE: u32 = 1 << 12;

fn active_state_mask(state: ElementState) -> u32 {
    let mut mask = 0;
    if state.hovered {
        mask |= STATE_HOVERED;
    }
    if state.focused {
        mask |= STATE_FOCUSED;
    }
    if state.active {
        mask |= STATE_ACTIVE;
    }
    if state.disabled {
        mask |= STATE_DISABLED;
    }
    if state.read_only {
        mask |= STATE_READ_ONLY;
    }
    if state.required {
        mask |= STATE_REQUIRED;
    }
    if state.selected {
        mask |= STATE_SELECTED;
    }
    if state.checked {
        mask |= STATE_CHECKED;
    }
    if state.expanded {
        mask |= STATE_EXPANDED;
    }
    if state.pressed {
        mask |= STATE_PRESSED;
    }
    if state.invalid {
        mask |= STATE_INVALID;
    }
    if state.value {
        mask |= STATE_VALUE;
    }
    if state.focus_visible {
        mask |= STATE_FOCUS_VISIBLE;
    }
    mask
}

fn state_name_bit(state: &str) -> Option<u32> {
    match state {
        "hover" | "hovered" => Some(STATE_HOVERED),
        "focus" | "focused" => Some(STATE_FOCUSED),
        "active" => Some(STATE_ACTIVE),
        "disabled" => Some(STATE_DISABLED),
        "readonly" => Some(STATE_READ_ONLY),
        "required" => Some(STATE_REQUIRED),
        "selected" => Some(STATE_SELECTED),
        "checked" => Some(STATE_CHECKED),
        "expanded" => Some(STATE_EXPANDED),
        "pressed" => Some(STATE_PRESSED),
        "invalid" => Some(STATE_INVALID),
        "value" => Some(STATE_VALUE),
        "focus-visible" => Some(STATE_FOCUS_VISIBLE),
        _ => None,
    }
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
            style.font_family = resolver
                .resolve_value_with_variables(value, variables)
                .into()
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
        "box-shadow" => {
            style.box_shadow =
                parse_box_shadow(&resolver.resolve_value_with_variables(value, variables))
        }
        "background-image" => {
            let resolved = resolver.resolve_value_with_variables(value, variables);
            style.background_paint = parse_background_image(&resolved);
        }
        "filter" => {
            style.filter = parse_filter(&resolver.resolve_value_with_variables(value, variables))
        }
        "backdrop-filter" => {
            style.backdrop_filter =
                parse_filter(&resolver.resolve_value_with_variables(value, variables))
        }
        "transition-duration" => {
            first_transition_mut(&mut style.transitions).duration_ms =
                parse_first_time_ms(&resolver.resolve_value_with_variables(value, variables))
        }
        "transition-delay" => {
            first_transition_mut(&mut style.transitions).delay_ms =
                parse_first_time_ms(&resolver.resolve_value_with_variables(value, variables))
        }
        "transition-timing-function" => {
            first_transition_mut(&mut style.transitions).easing = parse_easing_keyword(
                first_comma_item(&resolver.resolve_value_with_variables(value, variables)),
            )
        }
        "transition-property" => {
            first_transition_mut(&mut style.transitions).properties =
                parse_transition_properties(
                    &resolver.resolve_value_with_variables(value, variables),
                )
        }
        "transition" => {
            let resolved = resolver.resolve_value_with_variables(value, variables);
            style.transitions = parse_transition_shorthand(&resolved);
        }
        "animation-name" => {
            first_animation_mut(&mut style.animations).name = parse_animation_name(
                first_comma_item(&resolver.resolve_value_with_variables(value, variables)),
            )
        }
        "animation-duration" => {
            first_animation_mut(&mut style.animations).duration_ms =
                parse_first_time_ms(&resolver.resolve_value_with_variables(value, variables))
        }
        "animation-delay" => {
            first_animation_mut(&mut style.animations).delay_ms =
                parse_first_time_ms(&resolver.resolve_value_with_variables(value, variables))
        }
        "animation-timing-function" => {
            first_animation_mut(&mut style.animations).easing = parse_easing_keyword(
                first_comma_item(&resolver.resolve_value_with_variables(value, variables)),
            )
        }
        "animation-iteration-count" => {
            first_animation_mut(&mut style.animations).iteration_count =
                parse_animation_iteration_count(first_comma_item(
                    &resolver.resolve_value_with_variables(value, variables),
                ))
        }
        "animation-direction" => {
            first_animation_mut(&mut style.animations).direction = parse_animation_direction(
                first_comma_item(&resolver.resolve_value_with_variables(value, variables)),
            )
        }
        "animation-fill-mode" => {
            first_animation_mut(&mut style.animations).fill_mode = parse_animation_fill_mode(
                first_comma_item(&resolver.resolve_value_with_variables(value, variables)),
            )
        }
        "animation-play-state" => {
            first_animation_mut(&mut style.animations).play_state = parse_animation_play_state(
                first_comma_item(&resolver.resolve_value_with_variables(value, variables)),
            )
        }
        "animation" => {
            style.animations =
                parse_animation_shorthand(&resolver.resolve_value_with_variables(value, variables))
        }
        "transform-origin" => {
            style.transform_origin =
                parse_transform_origin(&resolver.resolve_value_with_variables(value, variables))
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
            style.visibility = match resolver
                .resolve_value_with_variables(value, variables)
                .as_str()
            {
                "hidden" => Visibility::Hidden,
                "collapse" => Visibility::Collapse,
                _ => Visibility::Visible,
            };
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
                    parts[0].trim_end_matches("px").parse::<f32>(),
                    parts[1].trim_end_matches("px").parse::<f32>(),
                ) {
                    style.tooltip_offset = Some((x, y));
                }
            }
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

fn resolve_embedded_tokens(
    value: &str,
    theme: &Theme,
    strict_animation_tokens: bool,
) -> Result<String, String> {
    let mut output = String::with_capacity(value.len());
    let mut rest = value;

    while let Some(start) = rest.find("token(") {
        output.push_str(&rest[..start]);
        let token_start = start + "token(".len();
        let Some(end) = rest[token_start..].find(')') else {
            output.push_str(&rest[start..]);
            return Ok(output);
        };

        let name = rest[token_start..token_start + end].trim();
        match theme.token(name) {
            Some(TokenValue::String(s)) => output.push_str(s),
            Some(TokenValue::Number(n)) => output.push_str(&format!("{n}")),
            Some(TokenValue::Bool(b)) => output.push_str(&format!("{b}")),
            None => {
                if strict_animation_tokens && name.starts_with("animation.") {
                    return Err(name.to_string());
                }
                tracing::warn!("unresolved theme token: {name}");
            }
        }
        rest = &rest[token_start + end + 1..];
    }

    output.push_str(rest);
    Ok(output)
}

fn find_unresolved_animation_token(value: &str, theme: &Theme) -> Option<String> {
    let mut rest = value;

    while let Some(start) = rest.find("token(") {
        let token_start = start + "token(".len();
        let end = rest[token_start..].find(')')?;
        let name = rest[token_start..token_start + end].trim();
        if name.starts_with("animation.") && theme.token(name).is_none() {
            return Some(name.to_string());
        }
        rest = &rest[token_start + end + 1..];
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue};

    fn rule_with_state(state_selector: &str) -> StyleRule {
        StyleRule {
            selector: Selector::State("".to_string(), state_selector.to_string()),
            declarations: vec![Declaration {
                property: "color".to_string(),
                value: StyleValue::Literal("red".to_string()),
            }],
            container_query: None,
        }
    }

    fn rule_with_compound_state(tag: &str, state: &str) -> StyleRule {
        StyleRule {
            selector: Selector::Compound(vec![
                Selector::Tag(tag.to_string()),
                Selector::State(tag.to_string(), state.to_string()),
            ]),
            declarations: vec![Declaration {
                property: "color".to_string(),
                value: StyleValue::Literal("red".to_string()),
            }],
            container_query: None,
        }
    }

    #[test]
    fn state_to_rules_empty_for_unused_bit() {
        let index = StyleRuleIndex::new(&[]);
        let rules = index.rules_for_state_bit(STATE_HOVERED);
        assert!(rules.is_empty());
    }

    #[test]
    fn state_to_rules_returns_hover_rule_for_hover_bit() {
        let rules = vec![rule_with_state("hover")];
        let index = StyleRuleIndex::new(&rules);
        let result = index.rules_for_state_bit(STATE_HOVERED);
        assert_eq!(result, &[0]);
    }

    #[test]
    fn state_to_rules_distinguishes_different_state_bits() {
        let rules = vec![rule_with_state("hover"), rule_with_state("focus")];
        let index = StyleRuleIndex::new(&rules);

        assert_eq!(index.rules_for_state_bit(STATE_HOVERED), &[0]);
        assert_eq!(index.rules_for_state_bit(STATE_FOCUSED), &[1]);
        assert!(index.rules_for_state_bit(STATE_ACTIVE).is_empty());
    }

    #[test]
    fn state_to_rules_handles_compound_selector_with_state() {
        let rules = vec![rule_with_compound_state("button", "hover")];
        let index = StyleRuleIndex::new(&rules);

        assert_eq!(index.rules_for_state_bit(STATE_HOVERED), &[0]);
    }

    #[test]
    fn state_to_rules_multiple_rules_for_same_bit() {
        let rules = vec![rule_with_state("hover"), rule_with_state("hover")];
        let index = StyleRuleIndex::new(&rules);

        let result = index.rules_for_state_bit(STATE_HOVERED);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&0));
        assert!(result.contains(&1));
    }
}
