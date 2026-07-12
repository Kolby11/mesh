use super::parse::*;
use super::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue, prop_variable_key};
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
    /// Per-instance resolved component-prop values, keyed by `prop_variable_key`
    /// (`--mesh-prop-<name>`). Consulted as a read-only fallback after the
    /// per-node custom-variable scratch. Empty without a `<props>` block.
    props: HashMap<String, StyleValue>,
    module_variable_cache: RefCell<HashMap<String, Vec<(String, StyleValue)>>>,
    theme_default_cache: RefCell<HashMap<String, (ComputedStyle, HashMap<String, StyleValue>)>>,
    module_theme_default_cache:
        RefCell<HashMap<String, HashMap<String, (ComputedStyle, HashMap<String, StyleValue>)>>>,
    theme_default_diagnostic_cache: RefCell<HashMap<String, ThemeDefaultDiagnosticPrototype>>,
    module_theme_default_diagnostic_cache:
        RefCell<HashMap<String, HashMap<String, ThemeDefaultDiagnosticPrototype>>>,
    theme_reference_cache: RefCell<HashMap<String, Arc<str>>>,
    theme_value_cache: RefCell<HashMap<String, CachedThemeTokenValue>>,
}

type ThemeDefaultDiagnosticPrototype = (
    ComputedStyle,
    HashMap<String, StyleValue>,
    Vec<StyleDiagnostic>,
);

#[derive(Debug, Clone)]
enum CachedThemeTokenValue {
    Missing,
    String(Arc<str>),
    Number(f64),
    Bool(bool),
}

impl CachedThemeTokenValue {
    fn from_token(value: Option<&TokenValue>) -> Self {
        match value {
            Some(TokenValue::String(value)) => Self::String(Arc::from(value.as_str())),
            Some(TokenValue::Number(value)) => Self::Number(*value),
            Some(TokenValue::Bool(value)) => Self::Bool(*value),
            None => Self::Missing,
        }
    }

    fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
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
            key: node.mesh_key(),
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
    no_diagnostics_declarations: Vec<Vec<IndexedDeclaration>>,
    selector_diagnostics: Vec<String>,
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
            no_diagnostics_declarations: rules
                .iter()
                .map(|rule| {
                    rule.declarations
                        .iter()
                        .map(IndexedDeclaration::from_declaration)
                        .collect()
                })
                .collect(),
            selector_diagnostics: rules
                .iter()
                .map(|rule| selector_to_diagnostic_string(&rule.selector))
                .collect(),
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
        })
    }

    fn for_each_candidate_rule_index(&self, attrs: &StyleNodeAttrs, mut visit: impl FnMut(usize)) {
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
                visit(idx);
            }
        })
    }

    fn no_diagnostics_declarations(&self, rule_idx: usize) -> &[IndexedDeclaration] {
        self.no_diagnostics_declarations
            .get(rule_idx)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn selector_diagnostic(&self, rule_idx: usize) -> &str {
        self.selector_diagnostics
            .get(rule_idx)
            .map(String::as_str)
            .unwrap_or("")
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

#[derive(Debug, Clone)]
struct IndexedDeclaration {
    property: IndexedProperty,
    value: StyleValue,
}

#[derive(Debug, Clone)]
enum IndexedProperty {
    Custom(String),
    Lowered {
        name: String,
        strict_animation: bool,
        background_image: bool,
    },
    DiagnosticOnly(String),
    Deferred(String),
    OutOfScope(String),
    Unsupported(String),
    DeprecatedToken(String),
}

impl IndexedDeclaration {
    fn from_declaration(decl: &Declaration) -> Self {
        Self {
            property: IndexedProperty::from_property(&decl.property, &decl.value),
            value: decl.value.clone(),
        }
    }
}

impl IndexedProperty {
    fn from_property(property: &str, value: &StyleValue) -> Self {
        if property.starts_with("--") {
            return Self::Custom(property.to_owned());
        }
        if let Some(status) = style_profile_status(property) {
            match status {
                StyleProfileStatus::Implemented => {}
                StyleProfileStatus::DiagnosticOnly => {
                    return Self::DiagnosticOnly(property.to_owned());
                }
                StyleProfileStatus::Deferred => {
                    return Self::Deferred(property.to_owned());
                }
                StyleProfileStatus::OutOfScope => {
                    return Self::OutOfScope(property.to_owned());
                }
            }
        }
        if !is_supported_css_property(property) {
            return Self::Unsupported(property.to_owned());
        }
        if contains_deprecated_token_reference(value) {
            return Self::DeprecatedToken(property.to_owned());
        }
        Self::Lowered {
            name: property.to_owned(),
            strict_animation: is_strict_animation_property(property),
            background_image: property == "background-image",
        }
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
        Self {
            theme,
            props: HashMap::new(),
            module_variable_cache: RefCell::new(HashMap::new()),
            theme_default_cache: RefCell::new(HashMap::new()),
            module_theme_default_cache: RefCell::new(HashMap::new()),
            theme_default_diagnostic_cache: RefCell::new(HashMap::new()),
            module_theme_default_diagnostic_cache: RefCell::new(HashMap::new()),
            theme_reference_cache: RefCell::new(HashMap::new()),
            theme_value_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Attach per-instance component-prop values. `props` is keyed by
    /// `prop_variable_key(name)` and holds the resolved value for each prop.
    pub fn with_props(mut self, props: HashMap<String, StyleValue>) -> Self {
        self.props = props;
        self
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
            StyleValue::Literal(s) => self
                .resolve_embedded_references_cached(s, variables, strict_animation_tokens)
                .unwrap_or_default(),
            StyleValue::Var(name) => variables
                .get(name)
                .map(|value| {
                    self.resolve_value_with_variables_mode(
                        value,
                        variables,
                        strict_animation_tokens,
                    )
                })
                .unwrap_or_else(|| {
                    self.resolve_theme_reference(name, strict_animation_tokens)
                        .unwrap_or_default()
                }),
            StyleValue::Prop(name) => self
                .lookup_variable(variables, &prop_variable_key(name))
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

    fn with_resolved_str<R>(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        read: impl FnOnce(&str) -> R,
    ) -> R {
        if let Some(resolved) = self.resolve_simple_str_with_variables(value, variables, 0) {
            return read(resolved);
        }
        let resolved = self.resolve_value_with_variables(value, variables);
        read(&resolved)
    }

    fn resolve_simple_str_with_variables<'b>(
        &'b self,
        value: &'b StyleValue,
        variables: &'b HashMap<String, StyleValue>,
        depth: u8,
    ) -> Option<&'b str> {
        if depth > 16 {
            return None;
        }
        match value {
            StyleValue::Literal(value) => {
                if value.contains("var(") || value.contains("prop(") {
                    None
                } else {
                    Some(value.as_str())
                }
            }
            StyleValue::Var(name) => {
                if let Some(value) = self.lookup_variable(variables, name) {
                    return self.resolve_simple_str_with_variables(
                        value,
                        variables,
                        depth.saturating_add(1),
                    );
                }
                let token_name = self.cached_theme_token_name(name);
                match self.theme.token(&token_name) {
                    Some(TokenValue::String(value)) => Some(value.as_str()),
                    Some(TokenValue::Number(_)) | Some(TokenValue::Bool(_)) | None => None,
                }
            }
            StyleValue::Prop(name) => self
                .lookup_variable(variables, &prop_variable_key(name))
                .and_then(|value| {
                    self.resolve_simple_str_with_variables(
                        value,
                        variables,
                        depth.saturating_add(1),
                    )
                }),
        }
    }

    fn resolve_theme_reference(
        &self,
        name: &str,
        strict_animation_tokens: bool,
    ) -> Result<String, String> {
        match self.cached_theme_token_value(name) {
            CachedThemeTokenValue::String(s) => Ok(s.to_string()),
            CachedThemeTokenValue::Number(n) => Ok(format!("{n}")),
            CachedThemeTokenValue::Bool(b) => Ok(format!("{b}")),
            CachedThemeTokenValue::Missing => {
                let token_name = self.cached_theme_token_name(name);
                if strict_animation_tokens && token_name.starts_with("animation.") {
                    return Err(token_name.to_string());
                }
                tracing::warn!("unresolved theme token: {token_name}");
                Ok(String::new())
            }
        }
    }

    fn resolve_embedded_references_cached(
        &self,
        value: &str,
        variables: &HashMap<String, StyleValue>,
        strict_animation_tokens: bool,
    ) -> Result<String, String> {
        let mut output = String::with_capacity(value.len());
        let mut rest = value;

        loop {
            let var_pos = rest.find("var(");
            let prop_pos = rest.find("prop(");
            let Some((start, is_prop)) = (match (var_pos, prop_pos) {
                (Some(v), Some(p)) if p < v => Some((p, true)),
                (Some(v), _) => Some((v, false)),
                (None, Some(p)) => Some((p, true)),
                (None, None) => None,
            }) else {
                break;
            };

            output.push_str(&rest[..start]);
            let prefix_len = if is_prop { "prop(".len() } else { "var(".len() };
            let reference_start = start + prefix_len;
            let Some(end) = rest[reference_start..].find(')') else {
                output.push_str(&rest[start..]);
                return Ok(output);
            };

            let name = rest[reference_start..reference_start + end].trim();
            if is_prop {
                let prop_key = prop_variable_key(name);
                if let Some(value) = self.lookup_variable(variables, &prop_key) {
                    let resolved = self.style_value_to_string_cached(
                        value,
                        variables,
                        strict_animation_tokens,
                    )?;
                    output.push_str(&self.resolve_embedded_references_cached(
                        &resolved,
                        variables,
                        strict_animation_tokens,
                    )?);
                }
            } else if let Some(value) = self.lookup_variable(variables, name) {
                let resolved =
                    self.style_value_to_string_cached(value, variables, strict_animation_tokens)?;
                output.push_str(&self.resolve_embedded_references_cached(
                    &resolved,
                    variables,
                    strict_animation_tokens,
                )?);
            } else {
                match self.cached_theme_token_value(name) {
                    CachedThemeTokenValue::String(s) => output.push_str(&s),
                    CachedThemeTokenValue::Number(n) => output.push_str(&format!("{n}")),
                    CachedThemeTokenValue::Bool(b) => output.push_str(&format!("{b}")),
                    CachedThemeTokenValue::Missing => {
                        let token_name = self.cached_theme_token_name(name);
                        if strict_animation_tokens && token_name.starts_with("animation.") {
                            return Err(token_name.to_string());
                        }
                        tracing::warn!("unresolved theme token: {token_name}");
                    }
                }
            }
            rest = &rest[reference_start + end + 1..];
        }

        output.push_str(rest);
        Ok(output)
    }

    fn style_value_to_string_cached(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        strict_animation_tokens: bool,
    ) -> Result<String, String> {
        match value {
            StyleValue::Literal(value) => {
                self.resolve_embedded_references_cached(value, variables, strict_animation_tokens)
            }
            StyleValue::Prop(name) => {
                if let Some(value) = self.lookup_variable(variables, &prop_variable_key(name)) {
                    return self.style_value_to_string_cached(
                        value,
                        variables,
                        strict_animation_tokens,
                    );
                }
                Ok(String::new())
            }
            StyleValue::Var(name) => {
                if let Some(value) = self.lookup_variable(variables, name) {
                    return self.style_value_to_string_cached(
                        value,
                        variables,
                        strict_animation_tokens,
                    );
                }
                self.resolve_theme_reference(name, strict_animation_tokens)
            }
        }
    }

    fn find_unresolved_animation_token_cached(&self, value: &str) -> Option<String> {
        let mut rest = value;

        loop {
            let var_start = rest.find("var(");
            let Some(start) = var_start else {
                break;
            };

            let reference_start = start + "var(".len();
            let end = rest[reference_start..].find(')')?;
            let reference = rest[reference_start..reference_start + end].trim();
            let token_name = self.cached_theme_token_name(reference);
            let token_value = self.cached_theme_token_value(reference);
            if token_name.starts_with("animation.") && token_value.is_missing() {
                return Some(token_name.to_string());
            }
            rest = &rest[reference_start + end + 1..];
        }

        None
    }

    fn validate_animation_value_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> Result<(), String> {
        match value {
            StyleValue::Literal(value) => {
                if let Some(name) = self.find_unresolved_animation_token_cached(value) {
                    return Err(name);
                }
                Ok(())
            }
            StyleValue::Var(name) => variables
                .get(name)
                .map(|value| self.validate_animation_value_with_variables(value, variables))
                .unwrap_or_else(|| {
                    let token_name = self.cached_theme_token_name(name);
                    if token_name.starts_with("animation.")
                        && self.cached_theme_token_value(name).is_missing()
                    {
                        Err(token_name.to_string())
                    } else {
                        Ok(())
                    }
                }),
            StyleValue::Prop(name) => self
                .lookup_variable(variables, &prop_variable_key(name))
                .map(|value| self.validate_animation_value_with_variables(value, variables))
                .unwrap_or(Ok(())),
        }
    }

    fn resolve_color_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> Color {
        self.resolve_color_with_variables_inner(value, variables, 0)
            .unwrap_or_else(|| {
                let resolved = self.resolve_value_with_variables(value, variables);
                Color::from_hex(&resolved).unwrap_or(Color::TRANSPARENT)
            })
    }

    fn resolve_color_with_variables_inner(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        depth: u8,
    ) -> Option<Color> {
        if depth > 16 {
            return None;
        }
        match value {
            StyleValue::Literal(value) => {
                if value.contains("var(") || value.contains("prop(") {
                    None
                } else {
                    Some(Color::from_hex(value).unwrap_or(Color::TRANSPARENT))
                }
            }
            StyleValue::Var(name) => {
                if let Some(value) = self.lookup_variable(variables, name) {
                    return self.resolve_color_with_variables_inner(
                        value,
                        variables,
                        depth.saturating_add(1),
                    );
                }
                match self.cached_theme_token_value(name) {
                    CachedThemeTokenValue::String(value) => {
                        Some(Color::from_hex(&value).unwrap_or(Color::TRANSPARENT))
                    }
                    CachedThemeTokenValue::Number(_) | CachedThemeTokenValue::Bool(_) => {
                        Some(Color::TRANSPARENT)
                    }
                    CachedThemeTokenValue::Missing => None,
                }
            }
            StyleValue::Prop(name) => self
                .lookup_variable(variables, &prop_variable_key(name))
                .and_then(|value| {
                    self.resolve_color_with_variables_inner(
                        value,
                        variables,
                        depth.saturating_add(1),
                    )
                }),
        }
    }

    fn resolve_number_with_variables(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
    ) -> f32 {
        self.resolve_number_with_variables_inner(value, variables, 0)
            .unwrap_or_else(|| parse_px(&self.resolve_value_with_variables(value, variables)))
    }

    fn resolve_number_with_variables_inner(
        &self,
        value: &StyleValue,
        variables: &HashMap<String, StyleValue>,
        depth: u8,
    ) -> Option<f32> {
        if depth > 16 {
            return None;
        }
        match value {
            StyleValue::Literal(value) => {
                if value.contains("var(") || value.contains("prop(") {
                    None
                } else {
                    Some(parse_px(value))
                }
            }
            StyleValue::Var(name) => {
                if let Some(value) = self.lookup_variable(variables, name) {
                    return self.resolve_number_with_variables_inner(
                        value,
                        variables,
                        depth.saturating_add(1),
                    );
                }
                match self.cached_theme_token_value(name) {
                    CachedThemeTokenValue::Number(value) => Some(value as f32),
                    CachedThemeTokenValue::String(value) => Some(parse_px(&value)),
                    CachedThemeTokenValue::Bool(_) => Some(0.0),
                    CachedThemeTokenValue::Missing => None,
                }
            }
            StyleValue::Prop(name) => self
                .lookup_variable(variables, &prop_variable_key(name))
                .and_then(|value| {
                    self.resolve_number_with_variables_inner(
                        value,
                        variables,
                        depth.saturating_add(1),
                    )
                }),
        }
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

    #[allow(clippy::too_many_arguments)]
    pub fn resolve_node_style_for_module_indexed(
        &self,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        tag: &str,
        classes: &[String],
        id: Option<&str>,
        context: StyleContext,
        state: ElementState,
        module_id: Option<&str>,
    ) -> ComputedStyle {
        debug_assert!(index.is_for(rules));
        let mut attrs = StyleNodeAttrs::new(tag, classes, id, state);
        attrs.module_id = module_id;
        self.resolve_node_style_with_attrs_indexed_no_diagnostics(rules, index, &attrs, context)
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

    #[allow(clippy::too_many_arguments)]
    pub fn resolve_node_style_with_diagnostics_for_module_indexed(
        &self,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        tag: &str,
        classes: &[String],
        id: Option<&str>,
        context: StyleContext,
        state: ElementState,
        module_id: Option<&str>,
    ) -> (ComputedStyle, Vec<StyleDiagnostic>) {
        debug_assert!(index.is_for(rules));
        let mut attrs = StyleNodeAttrs::new(tag, classes, id, state);
        attrs.module_id = module_id;
        self.resolve_node_style_with_attrs_indexed(rules, index, &attrs, context)
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
        let (mut style, default_variables) =
            self.cached_theme_component_defaults_no_diagnostics(attrs.tag, attrs.module_id());

        VARIABLE_SCRATCH.with(|scratch| {
            let mut variables = scratch.borrow_mut();
            variables.clear();
            variables.extend(default_variables);

            index.for_each_candidate_rule_index(attrs, |rule_idx| {
                let Some(rule) = rules.get(rule_idx) else {
                    return;
                };
                if rule_matches_attrs(rule, attrs, context) {
                    for decl in index.no_diagnostics_declarations(rule_idx) {
                        self.apply_indexed_declaration_no_diagnostics(
                            &mut style,
                            decl,
                            &mut variables,
                        );
                    }
                }
            });
        });

        style
    }

    fn cached_theme_component_defaults_no_diagnostics(
        &self,
        tag: &str,
        module_id: Option<&str>,
    ) -> (ComputedStyle, HashMap<String, StyleValue>) {
        let cached = if let Some(module_id) = module_id {
            self.module_theme_default_cache
                .borrow()
                .get(module_id)
                .and_then(|tags| tags.get(tag))
                .cloned()
        } else {
            self.theme_default_cache.borrow().get(tag).cloned()
        };
        if let Some(cached) = cached {
            return cached;
        }

        let mut style = ComputedStyle::default();
        if tag == "column" {
            style.direction = FlexDirection::Column;
        }
        let mut default_variables = HashMap::new();
        self.apply_theme_component_defaults_no_diagnostics(
            &mut style,
            tag,
            module_id,
            &mut default_variables,
        );
        let cached = (style, default_variables);
        if let Some(module_id) = module_id {
            self.module_theme_default_cache
                .borrow_mut()
                .entry(module_id.to_owned())
                .or_default()
                .insert(tag.to_owned(), cached.clone());
        } else {
            self.theme_default_cache
                .borrow_mut()
                .insert(tag.to_owned(), cached.clone());
        }
        cached
    }

    fn lookup_variable<'b>(
        &'b self,
        variables: &'b HashMap<String, StyleValue>,
        name: &str,
    ) -> Option<&'b StyleValue> {
        variables.get(name).or_else(|| self.props.get(name))
    }

    fn cached_theme_token_name(&self, reference: &str) -> Arc<str> {
        if let Some(name) = self.theme_reference_cache.borrow().get(reference) {
            return Arc::clone(name);
        }
        let name = Arc::<str>::from(theme_reference_to_token_name(reference));
        self.theme_reference_cache
            .borrow_mut()
            .insert(reference.to_owned(), Arc::clone(&name));
        name
    }

    fn cached_theme_token_value(&self, reference: &str) -> CachedThemeTokenValue {
        let reference = reference.trim();
        if let Some(value) = self.theme_value_cache.borrow().get(reference) {
            return value.clone();
        }
        let token_name = self.cached_theme_token_name(reference);
        let value = CachedThemeTokenValue::from_token(self.theme.token(&token_name));
        self.theme_value_cache
            .borrow_mut()
            .insert(reference.to_owned(), value.clone());
        value
    }

    fn resolve_node_style_with_attrs_indexed(
        &self,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        attrs: &StyleNodeAttrs,
        context: StyleContext,
    ) -> (ComputedStyle, Vec<StyleDiagnostic>) {
        let (mut style, mut variables, mut diagnostics) =
            self.cached_theme_component_defaults_with_diagnostics(attrs.tag, attrs.module_id());

        index.for_each_candidate_rule_index(attrs, |rule_idx| {
            let Some(rule) = rules.get(rule_idx) else {
                return;
            };
            if rule_matches_attrs(rule, attrs, context) {
                let selector = index.selector_diagnostic(rule_idx);
                for decl in index.no_diagnostics_declarations(rule_idx) {
                    self.apply_indexed_declaration_with_diagnostics(
                        &mut style,
                        decl,
                        selector,
                        &mut diagnostics,
                        &mut variables,
                    );
                }
            }
        });

        (style, diagnostics)
    }

    fn cached_theme_component_defaults_with_diagnostics(
        &self,
        tag: &str,
        module_id: Option<&str>,
    ) -> ThemeDefaultDiagnosticPrototype {
        let cached = if let Some(module_id) = module_id {
            self.module_theme_default_diagnostic_cache
                .borrow()
                .get(module_id)
                .and_then(|tags| tags.get(tag))
                .cloned()
        } else {
            self.theme_default_diagnostic_cache
                .borrow()
                .get(tag)
                .cloned()
        };
        if let Some(cached) = cached {
            return cached;
        }

        let mut style = ComputedStyle::default();
        if tag == "column" {
            style.direction = FlexDirection::Column;
        }
        let mut diagnostics = Vec::new();
        let mut default_variables = HashMap::new();
        self.apply_theme_component_defaults(
            &mut style,
            tag,
            module_id,
            &mut diagnostics,
            &mut default_variables,
        );
        let cached = (style, default_variables, diagnostics);
        if let Some(module_id) = module_id {
            self.module_theme_default_diagnostic_cache
                .borrow_mut()
                .entry(module_id.to_owned())
                .or_default()
                .insert(tag.to_owned(), cached.clone());
        } else {
            self.theme_default_diagnostic_cache
                .borrow_mut()
                .insert(tag.to_owned(), cached.clone());
        }
        cached
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

    /// Re-resolves the computed style of `node` and its descendants, reusing a
    /// caller-provided index. The index must have been built from the same
    /// `rules` slice; this is verified with `is_for()` and the index is rebuilt
    /// in place if not.
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

    /// Re-resolves the computed style of every child of `node` (but not `node`
    /// itself), reusing a caller-provided index.
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

    pub fn restyle_subtree_for_ids(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        target_ids: &std::collections::HashSet<crate::tree::NodeId>,
    ) {
        let index = StyleRuleIndex::new(rules);
        self.restyle_subtree_for_ids_with_index(node, rules, &index, context, target_ids);
    }

    pub fn restyle_subtree_for_ids_cached(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        context: StyleContext,
        index: &mut Option<StyleRuleIndex>,
        target_ids: &std::collections::HashSet<crate::tree::NodeId>,
    ) {
        let idx = ensure_index(rules, index);
        self.restyle_subtree_for_ids_with_index(node, rules, idx, context, target_ids);
    }

    fn restyle_subtree_for_ids_with_index(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        target_ids: &std::collections::HashSet<crate::tree::NodeId>,
    ) {
        self.restyle_subtree_for_ids_with_index_and_inheritance(
            node, rules, index, context, target_ids, None,
        );
    }

    fn restyle_subtree_for_ids_with_index_and_inheritance(
        &self,
        node: &mut crate::tree::WidgetNode,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        context: StyleContext,
        target_ids: &std::collections::HashSet<crate::tree::NodeId>,
        parent_style: Option<&ParentInheritedStyle>,
    ) {
        let is_target = target_ids.contains(&node.id);
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
                self.restyle_subtree_for_ids_with_index_and_inheritance(
                    child,
                    rules,
                    index,
                    context,
                    target_ids,
                    Some(&child_parent),
                );
            }
        } else {
            // This node is not a target and is not in an affected subtree.
            // Don't restyle it, but keep recursing — target nodes may be
            // deeper in the tree.
            for child in &mut node.children {
                self.restyle_subtree_for_ids_with_index_and_inheritance(
                    child, rules, index, context, target_ids, None,
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
            self.seed_module_theme_variables(module_id, variables);
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
            self.seed_module_theme_variables(module_id, variables);
            if let Some(defaults) = self.theme.module_component_defaults(module_id, "base") {
                self.apply_theme_defaults_map_no_diagnostics(style, defaults, variables);
            }
            if let Some(defaults) = self.theme.module_component_defaults(module_id, tag) {
                self.apply_theme_defaults_map_no_diagnostics(style, defaults, variables);
            }
        }
    }

    fn seed_module_theme_variables(
        &self,
        module_id: &str,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        let Some(module) = self.theme.modules.get(module_id) else {
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
            let value = classify_theme_style_value(value);
            self.apply_property_value_no_diagnostics(style, property, &value, variables);
        }
    }

    #[cfg(test)]
    fn apply_declaration_no_diagnostics(
        &self,
        style: &mut ComputedStyle,
        decl: &Declaration,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        self.apply_property_value_no_diagnostics(style, &decl.property, &decl.value, variables);
    }

    fn apply_indexed_declaration_no_diagnostics(
        &self,
        style: &mut ComputedStyle,
        decl: &IndexedDeclaration,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        match &decl.property {
            IndexedProperty::Custom(property) => {
                variables.insert(property.clone(), decl.value.clone());
            }
            IndexedProperty::Lowered {
                name,
                strict_animation,
                background_image,
            } => {
                if *strict_animation
                    && self
                        .validate_animation_value_with_variables(&decl.value, variables)
                        .is_err()
                {
                    return;
                }
                if *background_image {
                    let resolved = self.resolve_value_with_variables(&decl.value, variables);
                    if !is_supported_background_image(&resolved) {
                        return;
                    }
                }
                apply_declaration(style, name, &decl.value, self, variables);
            }
            IndexedProperty::DiagnosticOnly(_)
            | IndexedProperty::Deferred(_)
            | IndexedProperty::OutOfScope(_)
            | IndexedProperty::Unsupported(_)
            | IndexedProperty::DeprecatedToken(_) => {}
        }
    }

    fn apply_indexed_declaration_with_diagnostics(
        &self,
        style: &mut ComputedStyle,
        decl: &IndexedDeclaration,
        selector: &str,
        diagnostics: &mut Vec<StyleDiagnostic>,
        variables: &mut HashMap<String, StyleValue>,
    ) {
        match &decl.property {
            IndexedProperty::Custom(property) => {
                variables.insert(property.clone(), decl.value.clone());
            }
            IndexedProperty::DiagnosticOnly(property) => {
                diagnostics.push(StyleDiagnostic {
                    property: property.clone(),
                    selector: Some(selector.to_owned()),
                    message: format!(
                        "diagnostic-only CSS property '{property}' is accepted by the parser but not lowered into ComputedStyle"
                    ),
                });
            }
            IndexedProperty::Deferred(property) => {
                diagnostics.push(StyleDiagnostic {
                    property: property.clone(),
                    selector: Some(selector.to_owned()),
                    message: format!(
                        "deferred CSS property '{property}' is accepted by the parser but not lowered in the current painter profile"
                    ),
                });
            }
            IndexedProperty::OutOfScope(property) => {
                diagnostics.push(StyleDiagnostic {
                    property: property.clone(),
                    selector: Some(selector.to_owned()),
                    message: format!(
                        "unsupported CSS property '{property}' is out-of-scope for the MESH shell CSS profile"
                    ),
                });
            }
            IndexedProperty::Unsupported(property) => {
                diagnostics.push(StyleDiagnostic {
                    property: property.clone(),
                    selector: Some(selector.to_owned()),
                    message: format!("unsupported CSS property '{property}'"),
                });
            }
            IndexedProperty::DeprecatedToken(property) => {
                diagnostics.push(StyleDiagnostic {
                    property: property.clone(),
                    selector: Some(selector.to_owned()),
                    message: "deprecated token() references are not supported; use var(--...)"
                        .to_string(),
                });
            }
            IndexedProperty::Lowered {
                name,
                strict_animation,
                background_image,
            } => {
                if let StyleValue::Var(variable_name) = &decl.value
                    && !*strict_animation
                    && !variables.contains_key(variable_name)
                    && self.cached_theme_token_value(variable_name).is_missing()
                {
                    diagnostics.push(StyleDiagnostic {
                        property: name.clone(),
                        selector: Some(selector.to_owned()),
                        message: format!(
                            "unsupported CSS variable reference '{variable_name}' for property '{name}'"
                        ),
                    });
                }
                if *strict_animation
                    && let Err(token_name) =
                        self.validate_animation_value_with_variables(&decl.value, variables)
                {
                    diagnostics.push(StyleDiagnostic {
                        property: name.clone(),
                        selector: Some(selector.to_owned()),
                        message: format!("unresolved animation token reference '{token_name}'"),
                    });
                    return;
                }
                if *background_image {
                    let resolved = self.resolve_value_with_variables(&decl.value, variables);
                    if !is_supported_background_image(&resolved) {
                        diagnostics.push(StyleDiagnostic {
                            property: name.clone(),
                            selector: Some(selector.to_owned()),
                            message: format!("unsupported background-image '{resolved}'"),
                        });
                        return;
                    }
                }
                apply_declaration(style, name, &decl.value, self, variables);
            }
        }
    }

    fn apply_property_value_no_diagnostics(
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

fn contains_deprecated_token_reference(value: &StyleValue) -> bool {
    match value {
        StyleValue::Literal(value) => value.contains("token("),
        StyleValue::Var(_) | StyleValue::Prop(_) => false,
    }
}

fn classify_theme_style_value(value: &str) -> StyleValue {
    let value = value.trim();
    if value.starts_with("var(") && value.ends_with(')') {
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
        const LOWERED_CSS_PROPERTIES: &[&str] = &[$($names,)*];

        fn $apply(
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

pub(super) fn lowered_css_properties() -> &'static [&'static str] {
    LOWERED_CSS_PROPERTIES
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
            first_transition_mut(&mut style.transitions).easing = parse_easing_keyword(
                first_comma_item(&resolver.resolve_value_with_variables(value, variables)),
            )
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
            first_animation_mut(&mut style.animations).name = parse_animation_name(
                first_comma_item(&resolver.resolve_value_with_variables(value, variables)),
            )
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
                    parts[0].trim_end_matches("px").parse::<f32>(),
                    parts[1].trim_end_matches("px").parse::<f32>(),
                ) {
                    style.tooltip_offset = Some((x, y));
                }
            }
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

fn theme_reference_to_token_name(name: &str) -> String {
    let name = name.trim();
    let Some(variable) = name.strip_prefix("--") else {
        return name.to_string();
    };
    css_custom_property_to_token_name(variable)
}

fn css_custom_property_to_token_name(variable: &str) -> String {
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

fn canonicalize_prefixed(value: &str, prefixes: &[&str]) -> String {
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

fn canonicalize_suffixed(value: &str, suffixes: &[&str]) -> String {
    for suffix in suffixes {
        if let Some(rest) = value.strip_suffix(&format!("-{suffix}")) {
            return format!("{rest}.{suffix}");
        }
    }
    value.to_string()
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
    fn indexed_declarations_match_uncached_no_diagnostics_application() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let declarations = vec![
            Declaration {
                property: "--accent".to_string(),
                value: StyleValue::Literal("#112233".to_string()),
            },
            Declaration {
                property: "background-color".to_string(),
                value: StyleValue::Var("--accent".to_string()),
            },
            Declaration {
                property: "padding".to_string(),
                value: StyleValue::Literal("2px 4px".to_string()),
            },
            Declaration {
                property: "unknown-property".to_string(),
                value: StyleValue::Literal("ignored".to_string()),
            },
            Declaration {
                property: "transition-duration".to_string(),
                value: StyleValue::Var("--animation-missing-duration".to_string()),
            },
        ];
        let rules = vec![StyleRule {
            selector: Selector::Tag("button".to_string()),
            declarations: declarations.clone(),
            container_query: None,
        }];
        let index = StyleRuleIndex::new(&rules);

        let mut uncached = ComputedStyle::default();
        let mut uncached_variables = HashMap::new();
        for declaration in &declarations {
            resolver.apply_declaration_no_diagnostics(
                &mut uncached,
                declaration,
                &mut uncached_variables,
            );
        }

        let mut indexed = ComputedStyle::default();
        let mut indexed_variables = HashMap::new();
        for declaration in index.no_diagnostics_declarations(0) {
            resolver.apply_indexed_declaration_no_diagnostics(
                &mut indexed,
                declaration,
                &mut indexed_variables,
            );
        }

        assert_eq!(indexed.background_color, uncached.background_color);
        assert_eq!(indexed.padding, uncached.padding);
        assert_eq!(indexed.transitions, uncached.transitions);
        assert_eq!(
            resolver.resolve_value_with_variables(
                &StyleValue::Var("--accent".to_string()),
                &indexed_variables,
            ),
            resolver.resolve_value_with_variables(
                &StyleValue::Var("--accent".to_string()),
                &uncached_variables,
            )
        );
    }

    #[test]
    fn indexed_diagnostics_match_uncached_diagnostics() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let rules = vec![
            StyleRule {
                selector: Selector::Class("panel".to_string()),
                declarations: vec![Declaration {
                    property: "grid-template-columns".to_string(),
                    value: StyleValue::Literal("1fr 1fr".to_string()),
                }],
                container_query: None,
            },
            StyleRule {
                selector: Selector::Tag("box".to_string()),
                declarations: vec![
                    Declaration {
                        property: "color".to_string(),
                        value: StyleValue::Literal("#112233".to_string()),
                    },
                    Declaration {
                        property: "unknown-property".to_string(),
                        value: StyleValue::Literal("ignored".to_string()),
                    },
                    Declaration {
                        property: "background-color".to_string(),
                        value: StyleValue::Var("--missing-color".to_string()),
                    },
                ],
                container_query: None,
            },
        ];
        let index = StyleRuleIndex::new(&rules);
        let classes = vec!["panel".to_string()];

        let (_uncached_style, uncached) = resolver.resolve_node_style_with_diagnostics_for_module(
            &rules,
            "box",
            &classes,
            None,
            StyleContext::default(),
            ElementState::default(),
            Some("@test/module"),
        );
        let (_indexed_style, indexed) = resolver
            .resolve_node_style_with_diagnostics_for_module_indexed(
                &rules,
                &index,
                "box",
                &classes,
                None,
                StyleContext::default(),
                ElementState::default(),
                Some("@test/module"),
            );

        assert_eq!(indexed, uncached);
    }

    #[test]
    fn cached_diagnostic_theme_defaults_match_replayed_defaults() {
        let mut theme = mesh_core_theme::default_theme();
        theme.defaults.components.insert(
            "benchmark-card".into(),
            [
                ("background-color".into(), "#112233".into()),
                ("color".into(), "#ffffff".into()),
                ("font-size".into(), "13px".into()),
                ("grid-template-columns".into(), "1fr 1fr".into()),
                ("--local-accent".into(), "#445566".into()),
            ]
            .into_iter()
            .collect(),
        );
        let resolver = StyleResolver::new(&theme);

        let mut replayed_style = ComputedStyle::default();
        let mut replayed_diagnostics = Vec::new();
        let mut replayed_variables = HashMap::new();
        resolver.apply_theme_component_defaults(
            &mut replayed_style,
            "benchmark-card",
            None,
            &mut replayed_diagnostics,
            &mut replayed_variables,
        );

        let (cached_style, cached_variables, cached_diagnostics) =
            resolver.cached_theme_component_defaults_with_diagnostics("benchmark-card", None);

        assert_eq!(
            cached_style.background_color,
            replayed_style.background_color
        );
        assert_eq!(cached_style.color, replayed_style.color);
        assert!((cached_style.font_size - replayed_style.font_size).abs() < f32::EPSILON);
        assert_eq!(cached_variables.len(), replayed_variables.len());
        assert!(matches!(
            cached_variables.get("--local-accent"),
            Some(StyleValue::Literal(value)) if value == "#445566"
        ));
        assert_eq!(cached_diagnostics, replayed_diagnostics);
    }

    // cargo test -p mesh-core-elements --release -- indexed_diagnostic_declarations_skip_static_reclassification --ignored --nocapture
    #[test]
    #[ignore = "release-only indexed diagnostic declaration microbenchmark"]
    fn indexed_diagnostic_declarations_skip_static_reclassification() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let declarations = vec![
            Declaration {
                property: "--accent".to_string(),
                value: StyleValue::Literal("#112233".to_string()),
            },
            Declaration {
                property: "background-color".to_string(),
                value: StyleValue::Var("--accent".to_string()),
            },
            Declaration {
                property: "grid-template-columns".to_string(),
                value: StyleValue::Literal("1fr 1fr".to_string()),
            },
            Declaration {
                property: "unknown-property".to_string(),
                value: StyleValue::Literal("ignored".to_string()),
            },
            Declaration {
                property: "transition-duration".to_string(),
                value: StyleValue::Var("--animation-missing-duration".to_string()),
            },
        ];
        let rules = vec![StyleRule {
            selector: Selector::Class("panel".to_string()),
            declarations,
            container_query: None,
        }];
        let index = StyleRuleIndex::new(&rules);
        let classes = vec!["panel".to_string()];
        let attrs = StyleNodeAttrs::new("box", &classes, None, ElementState::default());
        let iterations = 200_000;

        let old_started = std::time::Instant::now();
        let mut old_count = 0usize;
        for _ in 0..iterations {
            let mut style = ComputedStyle::default();
            let mut diagnostics = Vec::new();
            let mut variables = HashMap::new();
            index.for_each_candidate_rule(&rules, &attrs, |rule| {
                if rule_matches_attrs(rule, &attrs, StyleContext::default()) {
                    for decl in &rule.declarations {
                        resolver.apply_declaration_with_diagnostics(
                            std::hint::black_box(&mut style),
                            decl,
                            Some(selector_to_diagnostic_string(&rule.selector)),
                            &mut diagnostics,
                            &mut variables,
                        );
                    }
                }
            });
            old_count = old_count.wrapping_add(std::hint::black_box(diagnostics.len()));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_count = 0usize;
        for _ in 0..iterations {
            let mut style = ComputedStyle::default();
            let mut diagnostics = Vec::new();
            let mut variables = HashMap::new();
            index.for_each_candidate_rule_index(&attrs, |rule_idx| {
                let rule = &rules[rule_idx];
                if rule_matches_attrs(rule, &attrs, StyleContext::default()) {
                    let selector = selector_to_diagnostic_string(&rule.selector);
                    for decl in index.no_diagnostics_declarations(rule_idx) {
                        resolver.apply_indexed_declaration_with_diagnostics(
                            std::hint::black_box(&mut style),
                            decl,
                            &selector,
                            &mut diagnostics,
                            &mut variables,
                        );
                    }
                }
            });
            new_count = new_count.wrapping_add(std::hint::black_box(diagnostics.len()));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "diagnostic declaration apply: static reclassification {old_time:?}; indexed metadata {new_time:?}; ratio {:.1}x; counts={old_count}/{new_count}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_count, new_count);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-elements --release -- indexed_selector_diagnostics_skip_per_node_formatting --ignored --nocapture
    #[test]
    #[ignore = "release-only indexed selector diagnostic string microbenchmark"]
    fn indexed_selector_diagnostics_skip_per_node_formatting() {
        let selector = Selector::Compound(vec![
            Selector::Tag("button".to_string()),
            Selector::Class("primary".to_string()),
            Selector::State("button".to_string(), "hover".to_string()),
        ]);
        let rules = vec![StyleRule {
            selector,
            declarations: vec![Declaration {
                property: "grid-template-columns".to_string(),
                value: StyleValue::Literal("1fr 1fr".to_string()),
            }],
            container_query: None,
        }];
        let index = StyleRuleIndex::new(&rules);
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            old_total = old_total.wrapping_add(
                selector_to_diagnostic_string(std::hint::black_box(&rules[0].selector)).len(),
            );
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            new_total =
                new_total.wrapping_add(index.selector_diagnostic(std::hint::black_box(0)).len());
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "selector diagnostics: per-node formatting {old_time:?}; indexed string {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-elements --release -- indexed_diagnostics_beat_per_node_index_rebuild --ignored --nocapture
    #[test]
    #[ignore = "release-only indexed diagnostics microbenchmark"]
    fn indexed_diagnostics_beat_per_node_index_rebuild() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let mut rules = Vec::new();
        for index in 0..80 {
            rules.push(StyleRule {
                selector: Selector::Class(format!("panel-{index}")),
                declarations: vec![
                    Declaration {
                        property: "color".to_string(),
                        value: StyleValue::Literal("#112233".to_string()),
                    },
                    Declaration {
                        property: "grid-template-columns".to_string(),
                        value: StyleValue::Literal("1fr 1fr".to_string()),
                    },
                ],
                container_query: None,
            });
        }
        let index = StyleRuleIndex::new(&rules);
        let classes = vec!["panel-79".to_string()];
        let iterations = 20_000usize;

        let old_started = std::time::Instant::now();
        let mut old_count = 0usize;
        for _ in 0..iterations {
            let (_style, diagnostics) = resolver.resolve_node_style_with_diagnostics_for_module(
                std::hint::black_box(&rules),
                "box",
                std::hint::black_box(&classes),
                None,
                StyleContext::default(),
                ElementState::default(),
                Some("@test/module"),
            );
            old_count = old_count.wrapping_add(diagnostics.len());
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_count = 0usize;
        for _ in 0..iterations {
            let (_style, diagnostics) = resolver
                .resolve_node_style_with_diagnostics_for_module_indexed(
                    std::hint::black_box(&rules),
                    std::hint::black_box(&index),
                    "box",
                    std::hint::black_box(&classes),
                    None,
                    StyleContext::default(),
                    ElementState::default(),
                    Some("@test/module"),
                );
            new_count = new_count.wrapping_add(diagnostics.len());
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "style diagnostics: per-node index rebuild {old_time:?}; cached index {new_time:?}; ratio {:.1}x; counts={old_count}/{new_count}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_count, new_count);
        assert!(new_time < old_time);
    }

    #[test]
    fn numeric_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme
            .tokens
            .insert("spacing.large".into(), TokenValue::Number(18.0));
        theme
            .tokens
            .insert("opacity.enabled".into(), TokenValue::Bool(true));
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert("--local-size".into(), StyleValue::Literal("14px".into()));
        variables.insert(
            prop_variable_key("gap"),
            StyleValue::Var("--spacing-large".into()),
        );

        for value in [
            StyleValue::Literal("12px".into()),
            StyleValue::Var("--local-size".into()),
            StyleValue::Var("--spacing-large".into()),
            StyleValue::Var("--opacity-enabled".into()),
            StyleValue::Prop("gap".into()),
        ] {
            let string_resolved =
                parse_px(&resolver.resolve_value_with_variables(&value, &variables));
            let numeric_resolved = resolver.resolve_number_with_variables(&value, &variables);
            assert_eq!(numeric_resolved, string_resolved);
        }
    }

    #[test]
    fn color_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme
            .tokens
            .insert("color.primary".into(), TokenValue::String("#112233".into()));
        theme
            .tokens
            .insert("spacing.large".into(), TokenValue::Number(18.0));
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert(
            "--local-color".into(),
            StyleValue::Literal("#445566".into()),
        );
        variables.insert(
            prop_variable_key("accent"),
            StyleValue::Var("--color-primary".into()),
        );

        for value in [
            StyleValue::Literal("#abcdef".into()),
            StyleValue::Literal("not-a-color".into()),
            StyleValue::Var("--local-color".into()),
            StyleValue::Var("--color-primary".into()),
            StyleValue::Var("--spacing-large".into()),
            StyleValue::Prop("accent".into()),
        ] {
            let resolved = resolver.resolve_value_with_variables(&value, &variables);
            let string_resolved = Color::from_hex(&resolved).unwrap_or(Color::TRANSPARENT);
            let color_resolved = resolver.resolve_color_with_variables(&value, &variables);
            assert_eq!(color_resolved, string_resolved);
        }
    }

    #[test]
    fn keyword_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme
            .tokens
            .insert("display.hidden".into(), TokenValue::String("none".into()));
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert("--local-display".into(), StyleValue::Literal("none".into()));
        variables.insert(
            prop_variable_key("display"),
            StyleValue::Var("--display-hidden".into()),
        );

        for value in [
            StyleValue::Literal("none".into()),
            StyleValue::Literal("flex".into()),
            StyleValue::Var("--local-display".into()),
            StyleValue::Var("--display-hidden".into()),
            StyleValue::Prop("display".into()),
        ] {
            let string_resolved = match resolver
                .resolve_value_with_variables(&value, &variables)
                .as_str()
            {
                "none" => Display::None,
                _ => Display::Flex,
            };
            let borrowed_resolved =
                resolver.with_resolved_str(&value, &variables, |resolved| match resolved {
                    "none" => Display::None,
                    _ => Display::Flex,
                });
            assert_eq!(borrowed_resolved, string_resolved);
        }
    }

    #[test]
    fn dimension_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme
            .tokens
            .insert("size.panel".into(), TokenValue::String("320px".into()));
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert("--local-width".into(), StyleValue::Literal("75%".into()));
        variables.insert(
            prop_variable_key("width"),
            StyleValue::Var("--size-panel".into()),
        );

        for value in [
            StyleValue::Literal("auto".into()),
            StyleValue::Literal("240px".into()),
            StyleValue::Literal("50%".into()),
            StyleValue::Var("--local-width".into()),
            StyleValue::Var("--size-panel".into()),
            StyleValue::Prop("width".into()),
        ] {
            let string_resolved =
                parse_dimension(&resolver.resolve_value_with_variables(&value, &variables));
            let borrowed_resolved = resolver
                .with_resolved_str(&value, &variables, |resolved| parse_dimension(resolved));
            assert_eq!(borrowed_resolved, string_resolved);
        }
    }

    #[test]
    fn overflow_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "overflow.panel".into(),
            TokenValue::String("hidden auto".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert(
            "--local-overflow".into(),
            StyleValue::Literal("scroll".into()),
        );
        variables.insert(
            prop_variable_key("overflow"),
            StyleValue::Var("--overflow-panel".into()),
        );

        for value in [
            StyleValue::Literal("hidden".into()),
            StyleValue::Literal("hidden auto".into()),
            StyleValue::Var("--local-overflow".into()),
            StyleValue::Var("--overflow-panel".into()),
            StyleValue::Prop("overflow".into()),
        ] {
            let string_resolved = parse_overflow_shorthand(
                &resolver.resolve_value_with_variables(&value, &variables),
            );
            let borrowed_resolved = resolver.with_resolved_str(&value, &variables, |resolved| {
                parse_overflow_shorthand(resolved)
            });
            assert_eq!(borrowed_resolved, string_resolved);
        }
    }

    #[test]
    fn time_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "animation.duration.fast".into(),
            TokenValue::String("120ms".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert(
            "--local-duration".into(),
            StyleValue::Literal("0.2s".into()),
        );
        variables.insert(
            prop_variable_key("duration"),
            StyleValue::Var("--animation-duration-fast".into()),
        );

        for value in [
            StyleValue::Literal("120ms".into()),
            StyleValue::Literal("0.2s".into()),
            StyleValue::Literal("300".into()),
            StyleValue::Var("--local-duration".into()),
            StyleValue::Var("--animation-duration-fast".into()),
            StyleValue::Prop("duration".into()),
        ] {
            let string_resolved =
                parse_first_time_ms(&resolver.resolve_value_with_variables(&value, &variables));
            let borrowed_resolved = resolver
                .with_resolved_str(&value, &variables, |resolved| parse_first_time_ms(resolved));
            assert_eq!(borrowed_resolved, string_resolved);
        }
    }

    #[test]
    fn transition_property_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "transition.properties.common".into(),
            TokenValue::String("opacity, transform, width".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert(
            "--local-properties".into(),
            StyleValue::Literal("all".into()),
        );
        variables.insert(
            prop_variable_key("properties"),
            StyleValue::Var("--transition-properties-common".into()),
        );

        for value in [
            StyleValue::Literal("opacity".into()),
            StyleValue::Literal("opacity, transform, width".into()),
            StyleValue::Var("--local-properties".into()),
            StyleValue::Var("--transition-properties-common".into()),
            StyleValue::Prop("properties".into()),
        ] {
            let string_resolved = parse_transition_properties(
                &resolver.resolve_value_with_variables(&value, &variables),
            );
            let borrowed_resolved = resolver.with_resolved_str(&value, &variables, |resolved| {
                parse_transition_properties(resolved)
            });
            assert_eq!(borrowed_resolved, string_resolved);
        }
    }

    #[test]
    fn filter_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "filter.blur.medium".into(),
            TokenValue::String("blur(12px)".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert("--local-filter".into(), StyleValue::Literal("none".into()));
        variables.insert(
            prop_variable_key("filter"),
            StyleValue::Var("--filter-blur-medium".into()),
        );

        for value in [
            StyleValue::Literal("none".into()),
            StyleValue::Literal("blur(4px)".into()),
            StyleValue::Var("--local-filter".into()),
            StyleValue::Var("--filter-blur-medium".into()),
            StyleValue::Prop("filter".into()),
        ] {
            let string_resolved =
                parse_filter(&resolver.resolve_value_with_variables(&value, &variables));
            let borrowed_resolved =
                resolver.with_resolved_str(&value, &variables, |resolved| parse_filter(resolved));
            assert_eq!(borrowed_resolved, string_resolved);
        }
    }

    #[test]
    fn background_image_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "background.gradient.accent".into(),
            TokenValue::String("linear-gradient(#112233, #445566)".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert(
            "--local-background".into(),
            StyleValue::Literal("none".into()),
        );
        variables.insert(
            prop_variable_key("background"),
            StyleValue::Var("--background-gradient-accent".into()),
        );

        for value in [
            StyleValue::Literal("none".into()),
            StyleValue::Literal("url(assets/panel.png)".into()),
            StyleValue::Literal("linear-gradient(#112233, #445566)".into()),
            StyleValue::Var("--local-background".into()),
            StyleValue::Var("--background-gradient-accent".into()),
            StyleValue::Prop("background".into()),
        ] {
            let string_resolved =
                parse_background_image(&resolver.resolve_value_with_variables(&value, &variables));
            let borrowed_resolved = resolver.with_resolved_str(&value, &variables, |resolved| {
                parse_background_image(resolved)
            });
            assert_eq!(borrowed_resolved, string_resolved);
        }
    }

    #[test]
    fn edge_shorthand_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "spacing.inset.panel".into(),
            TokenValue::String("4px 8px 12px 16px".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert(
            "--local-inset".into(),
            StyleValue::Literal("2px 6px".into()),
        );
        variables.insert(
            prop_variable_key("inset"),
            StyleValue::Var("--spacing-inset-panel".into()),
        );

        for value in [
            StyleValue::Literal("4px".into()),
            StyleValue::Literal("4px 8px".into()),
            StyleValue::Literal("4px 8px 12px 16px".into()),
            StyleValue::Var("--local-inset".into()),
            StyleValue::Var("--spacing-inset-panel".into()),
            StyleValue::Prop("inset".into()),
        ] {
            let string_resolved =
                parse_edges_shorthand(&resolver.resolve_value_with_variables(&value, &variables));
            let borrowed_resolved = resolver.with_resolved_str(&value, &variables, |resolved| {
                parse_edges_shorthand(resolved)
            });
            assert_eq!(borrowed_resolved, string_resolved);
        }
    }

    #[test]
    fn corner_shorthand_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "radius.panel".into(),
            TokenValue::String("4px 8px 12px 16px".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert(
            "--local-radius".into(),
            StyleValue::Literal("2px 6px".into()),
        );
        variables.insert(
            prop_variable_key("radius"),
            StyleValue::Var("--radius-panel".into()),
        );

        for value in [
            StyleValue::Literal("4px".into()),
            StyleValue::Literal("4px 8px".into()),
            StyleValue::Literal("4px 8px 12px 16px".into()),
            StyleValue::Var("--local-radius".into()),
            StyleValue::Var("--radius-panel".into()),
            StyleValue::Prop("radius".into()),
        ] {
            let string_resolved =
                parse_corners_shorthand(&resolver.resolve_value_with_variables(&value, &variables));
            let borrowed_resolved = resolver.with_resolved_str(&value, &variables, |resolved| {
                parse_corners_shorthand(resolved)
            });
            assert_eq!(borrowed_resolved, string_resolved);
        }
    }

    #[test]
    fn border_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "border.panel".into(),
            TokenValue::String("2px solid #112233".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert("--local-border".into(), StyleValue::Literal("none".into()));
        variables.insert(
            prop_variable_key("border"),
            StyleValue::Var("--border-panel".into()),
        );

        for value in [
            StyleValue::Literal("none".into()),
            StyleValue::Literal("1px solid #445566".into()),
            StyleValue::Var("--local-border".into()),
            StyleValue::Var("--border-panel".into()),
            StyleValue::Prop("border".into()),
        ] {
            let mut string_style = ComputedStyle::default();
            apply_border_shorthand(
                &mut string_style,
                &resolver.resolve_value_with_variables(&value, &variables),
            );
            let mut borrowed_style = ComputedStyle::default();
            resolver.with_resolved_str(&value, &variables, |resolved| {
                apply_border_shorthand(&mut borrowed_style, resolved);
            });
            assert_eq!(borrowed_style.border_width, string_style.border_width);
            assert_eq!(borrowed_style.border_color, string_style.border_color);
        }

        let color = StyleValue::Var("--border-panel".into());
        let string_color = parse_border_color_shorthand(
            &resolver.resolve_value_with_variables(&color, &variables),
        );
        let borrowed_color = resolver.with_resolved_str(&color, &variables, |resolved| {
            parse_border_color_shorthand(resolved)
        });
        assert_eq!(borrowed_color, string_color);
    }

    #[test]
    fn transform_origin_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "transform.origin.panel".into(),
            TokenValue::String("25% 75%".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert(
            "--local-origin".into(),
            StyleValue::Literal("left top".into()),
        );
        variables.insert(
            prop_variable_key("origin"),
            StyleValue::Var("--transform-origin-panel".into()),
        );

        for value in [
            StyleValue::Literal("center".into()),
            StyleValue::Literal("10px 20px".into()),
            StyleValue::Var("--local-origin".into()),
            StyleValue::Var("--transform-origin-panel".into()),
            StyleValue::Prop("origin".into()),
        ] {
            let string_resolved =
                parse_transform_origin(&resolver.resolve_value_with_variables(&value, &variables));
            let borrowed_resolved = resolver.with_resolved_str(&value, &variables, |resolved| {
                parse_transform_origin(resolved)
            });
            assert_eq!(borrowed_resolved, string_resolved);
        }
    }

    #[test]
    fn transform_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "transform.panel".into(),
            TokenValue::String("translate(12px, 8px) scale(1.2) rotate(15deg)".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert(
            "--local-transform".into(),
            StyleValue::Literal("translateX(4px)".into()),
        );
        variables.insert(
            prop_variable_key("transform"),
            StyleValue::Var("--transform-panel".into()),
        );

        for value in [
            StyleValue::Literal("none".into()),
            StyleValue::Literal("translate(10px, 20px) rotate(0.25turn)".into()),
            StyleValue::Var("--local-transform".into()),
            StyleValue::Var("--transform-panel".into()),
            StyleValue::Prop("transform".into()),
        ] {
            let string_resolved =
                parse_transform(&resolver.resolve_value_with_variables(&value, &variables));
            let borrowed_resolved = resolver
                .with_resolved_str(&value, &variables, |resolved| parse_transform(resolved));
            assert_eq!(borrowed_resolved, string_resolved);
        }
    }

    #[test]
    fn box_shadow_resolution_matches_string_resolution_for_simple_references() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "shadow.panel".into(),
            TokenValue::String("2px 4px 8px 1px #112233".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert("--local-shadow".into(), StyleValue::Literal("none".into()));
        variables.insert(
            prop_variable_key("shadow"),
            StyleValue::Var("--shadow-panel".into()),
        );

        for value in [
            StyleValue::Literal("none".into()),
            StyleValue::Literal("1px 2px 3px #445566".into()),
            StyleValue::Var("--local-shadow".into()),
            StyleValue::Var("--shadow-panel".into()),
            StyleValue::Prop("shadow".into()),
        ] {
            let string_resolved =
                parse_box_shadow(&resolver.resolve_value_with_variables(&value, &variables));
            let borrowed_resolved = resolver
                .with_resolved_str(&value, &variables, |resolved| parse_box_shadow(resolved));
            assert_eq!(borrowed_resolved, string_resolved);
        }
    }

    #[test]
    fn flex_resolution_matches_string_resolution_for_simple_references() {
        fn apply_flex_value(style: &mut ComputedStyle, value: &str) {
            let value = value.trim();
            if value == "none" {
                style.flex_grow = 0.0;
                style.flex_shrink = 0.0;
                style.flex_basis = Dimension::Auto;
            } else if value == "auto" {
                style.flex_grow = 1.0;
                style.flex_shrink = 1.0;
                style.flex_basis = Dimension::Auto;
            } else if let Ok(n) = value.parse::<f32>() {
                style.flex_grow = n;
                style.flex_shrink = 1.0;
                style.flex_basis = Dimension::Px(0.0);
            } else {
                apply_flex_shorthand(style, value);
            }
        }

        let mut theme = mesh_core_theme::default_theme();
        theme
            .tokens
            .insert("flex.panel".into(), TokenValue::String("2 1 240px".into()));
        let resolver = StyleResolver::new(&theme);
        let mut variables = HashMap::new();
        variables.insert("--local-flex".into(), StyleValue::Literal("auto".into()));
        variables.insert(
            prop_variable_key("flex"),
            StyleValue::Var("--flex-panel".into()),
        );

        for value in [
            StyleValue::Literal("none".into()),
            StyleValue::Literal("1".into()),
            StyleValue::Literal("2 1 240px".into()),
            StyleValue::Var("--local-flex".into()),
            StyleValue::Var("--flex-panel".into()),
            StyleValue::Prop("flex".into()),
        ] {
            let mut string_style = ComputedStyle::default();
            apply_flex_value(
                &mut string_style,
                &resolver.resolve_value_with_variables(&value, &variables),
            );
            let mut borrowed_style = ComputedStyle::default();
            resolver.with_resolved_str(&value, &variables, |resolved| {
                apply_flex_value(&mut borrowed_style, resolved);
            });
            assert_eq!(borrowed_style.flex_grow, string_style.flex_grow);
            assert_eq!(borrowed_style.flex_shrink, string_style.flex_shrink);
            assert_eq!(borrowed_style.flex_basis, string_style.flex_basis);
        }
    }

    // cargo test -p mesh-core-elements --release -- numeric_theme_token_resolution_beats_string_roundtrip --ignored --nocapture
    #[test]
    #[ignore = "release-only numeric token resolution microbenchmark"]
    fn numeric_theme_token_resolution_beats_string_roundtrip() {
        let mut theme = mesh_core_theme::default_theme();
        theme
            .tokens
            .insert("spacing.large".into(), TokenValue::Number(18.0));
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--spacing-large".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0.0f32;
        for _ in 0..iterations {
            old_accumulator += parse_px(
                &resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables),
            );
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0.0f32;
        for _ in 0..iterations {
            new_accumulator +=
                resolver.resolve_number_with_variables(std::hint::black_box(&value), &variables);
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "numeric token resolution: string roundtrip {old_time:?}; typed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-elements --release -- time_theme_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only time token resolution microbenchmark"]
    fn time_theme_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "animation.duration.fast".into(),
            TokenValue::String("120ms".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--animation-duration-fast".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0u32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            old_accumulator =
                old_accumulator.wrapping_add(std::hint::black_box(parse_first_time_ms(&resolved)));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0u32;
        for _ in 0..iterations {
            let parsed =
                resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                    parse_first_time_ms(resolved)
                });
            new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(parsed));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "time token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-elements --release -- transition_property_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only transition property token resolution microbenchmark"]
    fn transition_property_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "transition.properties.common".into(),
            TokenValue::String("opacity, transform, width".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--transition-properties-common".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0u32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(
                transition_property_score(parse_transition_properties(&resolved)),
            ));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0u32;
        for _ in 0..iterations {
            let properties =
                resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                    parse_transition_properties(resolved)
                });
            new_accumulator = new_accumulator
                .wrapping_add(std::hint::black_box(transition_property_score(properties)));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "transition property token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    fn transition_property_score(properties: TransitionProperties) -> u32 {
        u32::from(properties.all)
            + u32::from(properties.opacity)
            + u32::from(properties.transform)
            + u32::from(properties.width)
            + u32::from(properties.height)
            + u32::from(properties.background_color)
            + u32::from(properties.color)
    }

    // cargo test -p mesh-core-elements --release -- filter_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only filter token resolution microbenchmark"]
    fn filter_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "filter.blur.medium".into(),
            TokenValue::String("blur(12px)".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--filter-blur-medium".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0.0f32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            old_accumulator += std::hint::black_box(parse_filter(&resolved).blur_radius);
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0.0f32;
        for _ in 0..iterations {
            let filter =
                resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                    parse_filter(resolved)
                });
            new_accumulator += std::hint::black_box(filter.blur_radius);
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "filter token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-elements --release -- background_image_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only background-image token resolution microbenchmark"]
    fn background_image_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "background.gradient.accent".into(),
            TokenValue::String("linear-gradient(#112233, #445566)".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--background-gradient-accent".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0u32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(
                background_paint_score(&parse_background_image(&resolved)),
            ));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0u32;
        for _ in 0..iterations {
            let paint =
                resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                    parse_background_image(resolved)
                });
            new_accumulator =
                new_accumulator.wrapping_add(std::hint::black_box(background_paint_score(&paint)));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "background-image token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    fn background_paint_score(paint: &BackgroundPaint) -> u32 {
        match paint {
            BackgroundPaint::None => 1,
            BackgroundPaint::Image(source) => 2_u32.saturating_add(source.path.len() as u32),
            BackgroundPaint::LinearGradient(gradient) => 3_u32
                .saturating_add(u32::from(gradient.from.r))
                .saturating_add(u32::from(gradient.to.r)),
        }
    }

    // cargo test -p mesh-core-elements --release -- edge_shorthand_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only edge shorthand token resolution microbenchmark"]
    fn edge_shorthand_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "spacing.inset.panel".into(),
            TokenValue::String("4px 8px 12px 16px".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--spacing-inset-panel".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0.0f32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            old_accumulator += std::hint::black_box(edge_score(parse_edges_shorthand(&resolved)));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0.0f32;
        for _ in 0..iterations {
            let edges =
                resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                    parse_edges_shorthand(resolved)
                });
            new_accumulator += std::hint::black_box(edge_score(edges));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "edge shorthand token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    fn edge_score(edges: Edges) -> f32 {
        edges.top + edges.right + edges.bottom + edges.left
    }

    // cargo test -p mesh-core-elements --release -- corner_shorthand_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only corner shorthand token resolution microbenchmark"]
    fn corner_shorthand_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "radius.panel".into(),
            TokenValue::String("4px 8px 12px 16px".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--radius-panel".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0.0f32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            old_accumulator +=
                std::hint::black_box(corner_score(parse_corners_shorthand(&resolved)));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0.0f32;
        for _ in 0..iterations {
            let corners =
                resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                    parse_corners_shorthand(resolved)
                });
            new_accumulator += std::hint::black_box(corner_score(corners));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "corner shorthand token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    fn corner_score(corners: Corners) -> f32 {
        corners.top_left + corners.top_right + corners.bottom_right + corners.bottom_left
    }

    // cargo test -p mesh-core-elements --release -- border_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only border token resolution microbenchmark"]
    fn border_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "border.panel".into(),
            TokenValue::String("2px solid #112233".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--border-panel".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0.0f32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            let mut style = ComputedStyle::default();
            apply_border_shorthand(&mut style, &resolved);
            old_accumulator += std::hint::black_box(border_score(&style));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0.0f32;
        for _ in 0..iterations {
            let mut style = ComputedStyle::default();
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                apply_border_shorthand(&mut style, resolved);
            });
            new_accumulator += std::hint::black_box(border_score(&style));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "border token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    fn border_score(style: &ComputedStyle) -> f32 {
        edge_score(style.border_width) + f32::from(style.border_color.r)
    }

    // cargo test -p mesh-core-elements --release -- transform_origin_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only transform-origin token resolution microbenchmark"]
    fn transform_origin_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "transform.origin.panel".into(),
            TokenValue::String("25% 75%".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--transform-origin-panel".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0.0f32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            old_accumulator +=
                std::hint::black_box(transform_origin_score(parse_transform_origin(&resolved)));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0.0f32;
        for _ in 0..iterations {
            let origin =
                resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                    parse_transform_origin(resolved)
                });
            new_accumulator += std::hint::black_box(transform_origin_score(origin));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "transform-origin token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    fn transform_origin_score(origin: TransformOrigin) -> f32 {
        fn axis_score(value: TransformOriginValue) -> f32 {
            match value {
                TransformOriginValue::Percent(value) | TransformOriginValue::Px(value) => value,
            }
        }
        axis_score(origin.x) + axis_score(origin.y)
    }

    // cargo test -p mesh-core-elements --release -- transform_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only transform token resolution microbenchmark"]
    fn transform_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "transform.panel".into(),
            TokenValue::String("translate(12px, 8px) scale(1.2) rotate(15deg)".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--transform-panel".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0.0f32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            old_accumulator += std::hint::black_box(transform_score(parse_transform(&resolved)));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0.0f32;
        for _ in 0..iterations {
            let transform =
                resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                    parse_transform(resolved)
                });
            new_accumulator += std::hint::black_box(transform_score(transform));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "transform token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    fn transform_score(transform: Transform2D) -> f32 {
        transform.translate_x
            + transform.translate_y
            + transform.scale_x
            + transform.scale_y
            + transform.rotation
    }

    // cargo test -p mesh-core-elements --release -- box_shadow_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only box-shadow token resolution microbenchmark"]
    fn box_shadow_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "shadow.panel".into(),
            TokenValue::String("2px 4px 8px 1px #112233".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--shadow-panel".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0.0f32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            old_accumulator += std::hint::black_box(box_shadow_score(parse_box_shadow(&resolved)));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0.0f32;
        for _ in 0..iterations {
            let shadow =
                resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                    parse_box_shadow(resolved)
                });
            new_accumulator += std::hint::black_box(box_shadow_score(shadow));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "box-shadow token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    fn box_shadow_score(shadow: BoxShadow) -> f32 {
        shadow.offset_x
            + shadow.offset_y
            + shadow.blur_radius
            + shadow.spread_radius
            + f32::from(shadow.color.r)
            + f32::from(shadow.inset)
    }

    // cargo test -p mesh-core-elements --release -- flex_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only flex token resolution microbenchmark"]
    fn flex_token_resolution_beats_string_clone() {
        fn apply_flex_value(style: &mut ComputedStyle, value: &str) {
            let value = value.trim();
            if value == "none" {
                style.flex_grow = 0.0;
                style.flex_shrink = 0.0;
                style.flex_basis = Dimension::Auto;
            } else if value == "auto" {
                style.flex_grow = 1.0;
                style.flex_shrink = 1.0;
                style.flex_basis = Dimension::Auto;
            } else if let Ok(n) = value.parse::<f32>() {
                style.flex_grow = n;
                style.flex_shrink = 1.0;
                style.flex_basis = Dimension::Px(0.0);
            } else {
                apply_flex_shorthand(style, value);
            }
        }

        let mut theme = mesh_core_theme::default_theme();
        theme
            .tokens
            .insert("flex.panel".into(), TokenValue::String("2 1 240px".into()));
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--flex-panel".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0.0f32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            let mut style = ComputedStyle::default();
            apply_flex_value(&mut style, &resolved);
            old_accumulator += std::hint::black_box(flex_score(&style));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0.0f32;
        for _ in 0..iterations {
            let mut style = ComputedStyle::default();
            resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                apply_flex_value(&mut style, resolved);
            });
            new_accumulator += std::hint::black_box(flex_score(&style));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "flex token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    fn flex_score(style: &ComputedStyle) -> f32 {
        let basis = match style.flex_basis {
            Dimension::Px(value) | Dimension::Percent(value) => value,
            Dimension::Auto => 1.0,
            Dimension::Content | Dimension::Fit => 2.0,
        };
        style.flex_grow + style.flex_shrink + basis
    }

    // cargo test -p mesh-core-elements --release -- overflow_theme_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only overflow token resolution microbenchmark"]
    fn overflow_theme_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme.tokens.insert(
            "overflow.panel".into(),
            TokenValue::String("hidden auto".into()),
        );
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--overflow-panel".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0u32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            let (x, y) = parse_overflow_shorthand(&resolved);
            old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(
                overflow_score(x).saturating_add(overflow_score(y)),
            ));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0u32;
        for _ in 0..iterations {
            let (x, y) =
                resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                    parse_overflow_shorthand(resolved)
                });
            new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(
                overflow_score(x).saturating_add(overflow_score(y)),
            ));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "overflow token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    fn overflow_score(value: Overflow) -> u32 {
        match value {
            Overflow::Visible => 1,
            Overflow::Hidden => 2,
            Overflow::Auto => 3,
            Overflow::Scroll => 4,
        }
    }

    // cargo test -p mesh-core-elements --release -- dimension_theme_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only dimension token resolution microbenchmark"]
    fn dimension_theme_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme
            .tokens
            .insert("size.panel".into(), TokenValue::String("320px".into()));
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--size-panel".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0.0f32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            if let Dimension::Px(px) = parse_dimension(&resolved) {
                old_accumulator += std::hint::black_box(px);
            }
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0.0f32;
        for _ in 0..iterations {
            let dimension =
                resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                    parse_dimension(resolved)
                });
            if let Dimension::Px(px) = dimension {
                new_accumulator += std::hint::black_box(px);
            }
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "dimension token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-elements --release -- keyword_theme_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only keyword token resolution microbenchmark"]
    fn keyword_theme_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme
            .tokens
            .insert("display.hidden".into(), TokenValue::String("none".into()));
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--display-hidden".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0u32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            let display = match resolved.as_str() {
                "none" => Display::None,
                _ => Display::Flex,
            };
            old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(match display {
                Display::None => 1,
                Display::Flex => 2,
            }));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0u32;
        for _ in 0..iterations {
            let display =
                resolver.with_resolved_str(std::hint::black_box(&value), &variables, |resolved| {
                    match resolved {
                        "none" => Display::None,
                        _ => Display::Flex,
                    }
                });
            new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(match display {
                Display::None => 1,
                Display::Flex => 2,
            }));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "keyword token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-elements --release -- color_theme_token_resolution_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only color token resolution microbenchmark"]
    fn color_theme_token_resolution_beats_string_clone() {
        let mut theme = mesh_core_theme::default_theme();
        theme
            .tokens
            .insert("color.primary".into(), TokenValue::String("#112233".into()));
        let resolver = StyleResolver::new(&theme);
        let variables = HashMap::new();
        let value = StyleValue::Var("--color-primary".into());
        let iterations = 500_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0u32;
        for _ in 0..iterations {
            let resolved =
                resolver.resolve_value_with_variables(std::hint::black_box(&value), &variables);
            let color = Color::from_hex(&resolved).unwrap_or(Color::TRANSPARENT);
            old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(color.r as u32));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0u32;
        for _ in 0..iterations {
            let color =
                resolver.resolve_color_with_variables(std::hint::black_box(&value), &variables);
            new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(color.r as u32));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "color token resolution: string clone {old_time:?}; borrowed fast path {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-elements --release -- theme_default_direct_apply_beats_declaration_allocation --ignored --nocapture
    #[test]
    #[ignore = "release-only theme default application microbenchmark"]
    fn theme_default_direct_apply_beats_declaration_allocation() {
        fn old_apply_theme_defaults_map_no_diagnostics(
            resolver: &StyleResolver<'_>,
            style: &mut ComputedStyle,
            defaults: &mesh_core_theme::ComponentDefaults,
            variables: &mut HashMap<String, StyleValue>,
        ) {
            for (property, value) in defaults {
                let declaration = Declaration {
                    property: property.clone(),
                    value: classify_theme_style_value(value),
                };
                resolver.apply_declaration_no_diagnostics(style, &declaration, variables);
            }
        }

        let mut defaults = mesh_core_theme::ComponentDefaults::new();
        defaults.insert("background-color".into(), "#112233".into());
        defaults.insert("color".into(), "#ffffff".into());
        defaults.insert("font-size".into(), "13px".into());
        defaults.insert("padding".into(), "4px 8px".into());
        defaults.insert("border-radius".into(), "6px".into());
        defaults.insert("gap".into(), "5px".into());
        defaults.insert("opacity".into(), "0.875".into());
        defaults.insert("--local-accent".into(), "#445566".into());

        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let iterations = 200_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0u32;
        for _ in 0..iterations {
            let mut style = ComputedStyle::default();
            let mut variables = HashMap::new();
            old_apply_theme_defaults_map_no_diagnostics(
                &resolver,
                std::hint::black_box(&mut style),
                &defaults,
                &mut variables,
            );
            old_accumulator =
                old_accumulator.wrapping_add(std::hint::black_box(style.background_color.r as u32));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0u32;
        for _ in 0..iterations {
            let mut style = ComputedStyle::default();
            let mut variables = HashMap::new();
            resolver.apply_theme_defaults_map_no_diagnostics(
                std::hint::black_box(&mut style),
                &defaults,
                &mut variables,
            );
            new_accumulator =
                new_accumulator.wrapping_add(std::hint::black_box(style.background_color.r as u32));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "theme defaults apply: declaration allocation {old_time:?}; direct property apply {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-elements --release -- cached_theme_default_prototype_beats_reapplying_string_map --ignored --nocapture
    #[test]
    #[ignore = "release-only theme default prototype microbenchmark"]
    fn cached_theme_default_prototype_beats_reapplying_string_map() {
        let mut theme = mesh_core_theme::default_theme();
        theme.defaults.components.insert(
            "benchmark-card".into(),
            [
                ("background-color".into(), "#112233".into()),
                ("color".into(), "#ffffff".into()),
                ("font-size".into(), "13px".into()),
                ("padding".into(), "4px 8px".into()),
                ("border-radius".into(), "6px".into()),
                ("gap".into(), "5px".into()),
                ("opacity".into(), "0.875".into()),
                ("--local-accent".into(), "#445566".into()),
            ]
            .into_iter()
            .collect(),
        );
        let resolver = StyleResolver::new(&theme);
        let attrs = StyleNodeAttrs {
            tag: "benchmark-card",
            ..StyleNodeAttrs::default()
        };
        let rules = Vec::new();
        let index = StyleRuleIndex::new(&rules);
        let context = StyleContext::default();
        let iterations = 200_000;

        let uncached_started = std::time::Instant::now();
        let mut uncached_accumulator = 0u32;
        for _ in 0..iterations {
            let mut style = ComputedStyle::default();
            let mut variables = HashMap::new();
            resolver.apply_theme_component_defaults_no_diagnostics(
                std::hint::black_box(&mut style),
                "benchmark-card",
                None,
                &mut variables,
            );
            uncached_accumulator = uncached_accumulator
                .wrapping_add(std::hint::black_box(style.background_color.r as u32));
        }
        let uncached_time = uncached_started.elapsed();

        // Populate the prototype outside the timed cache-hit loop.
        let _ = resolver
            .resolve_node_style_with_attrs_indexed_no_diagnostics(&rules, &index, &attrs, context);
        let cached_started = std::time::Instant::now();
        let mut cached_accumulator = 0u32;
        for _ in 0..iterations {
            let style = resolver.resolve_node_style_with_attrs_indexed_no_diagnostics(
                std::hint::black_box(&rules),
                std::hint::black_box(&index),
                std::hint::black_box(&attrs),
                context,
            );
            cached_accumulator = cached_accumulator
                .wrapping_add(std::hint::black_box(style.background_color.r as u32));
        }
        let cached_time = cached_started.elapsed();

        eprintln!(
            "theme default prototype: reapply strings {uncached_time:?}; cached {cached_time:?}; ratio {:.1}x; accumulators={uncached_accumulator}/{cached_accumulator}",
            uncached_time.as_secs_f64() / cached_time.as_secs_f64()
        );
        assert_eq!(uncached_accumulator, cached_accumulator);
        assert!(cached_time < uncached_time);
    }

    // cargo test -p mesh-core-elements --release -- cached_diagnostic_theme_default_prototype_beats_reapplying_string_map --ignored --nocapture
    #[test]
    #[ignore = "release-only diagnostic theme default prototype microbenchmark"]
    fn cached_diagnostic_theme_default_prototype_beats_reapplying_string_map() {
        fn old_resolve_diagnostic_theme_defaults(
            resolver: &StyleResolver<'_>,
            tag: &str,
        ) -> (ComputedStyle, Vec<StyleDiagnostic>) {
            let mut style = ComputedStyle::default();
            let mut diagnostics = Vec::new();
            let mut variables = HashMap::new();
            resolver.apply_theme_component_defaults(
                &mut style,
                tag,
                None,
                &mut diagnostics,
                &mut variables,
            );
            (style, diagnostics)
        }

        let mut theme = mesh_core_theme::default_theme();
        theme.defaults.components.insert(
            "benchmark-card".into(),
            [
                ("background-color".into(), "#112233".into()),
                ("color".into(), "#ffffff".into()),
                ("font-size".into(), "13px".into()),
                ("padding".into(), "4px 8px".into()),
                ("border-radius".into(), "6px".into()),
                ("gap".into(), "5px".into()),
                ("opacity".into(), "0.875".into()),
                ("grid-template-columns".into(), "1fr 1fr".into()),
                ("--local-accent".into(), "#445566".into()),
            ]
            .into_iter()
            .collect(),
        );
        let resolver = StyleResolver::new(&theme);
        let attrs = StyleNodeAttrs {
            tag: "benchmark-card",
            ..StyleNodeAttrs::default()
        };
        let rules = Vec::new();
        let index = StyleRuleIndex::new(&rules);
        let context = StyleContext::default();
        let iterations = 200_000;

        let uncached_started = std::time::Instant::now();
        let mut uncached_accumulator = 0u32;
        for _ in 0..iterations {
            let (style, diagnostics) =
                old_resolve_diagnostic_theme_defaults(&resolver, "benchmark-card");
            uncached_accumulator = uncached_accumulator.wrapping_add(std::hint::black_box(
                style.background_color.r as u32 + diagnostics.len() as u32,
            ));
        }
        let uncached_time = uncached_started.elapsed();

        let _ = resolver.resolve_node_style_with_attrs_indexed(&rules, &index, &attrs, context);
        let cached_started = std::time::Instant::now();
        let mut cached_accumulator = 0u32;
        for _ in 0..iterations {
            let (style, diagnostics) = resolver.resolve_node_style_with_attrs_indexed(
                std::hint::black_box(&rules),
                std::hint::black_box(&index),
                std::hint::black_box(&attrs),
                context,
            );
            cached_accumulator = cached_accumulator.wrapping_add(std::hint::black_box(
                style.background_color.r as u32 + diagnostics.len() as u32,
            ));
        }
        let cached_time = cached_started.elapsed();

        eprintln!(
            "diagnostic theme default prototype: reapply strings {uncached_time:?}; cached {cached_time:?}; ratio {:.1}x; accumulators={uncached_accumulator}/{cached_accumulator}",
            uncached_time.as_secs_f64() / cached_time.as_secs_f64()
        );
        assert_eq!(uncached_accumulator, cached_accumulator);
        assert!(cached_time < uncached_time);
    }

    // cargo test -p mesh-core-elements --release -- layered_prop_lookup_beats_per_node_prop_cloning --ignored --nocapture
    #[test]
    #[ignore = "release-only layered prop lookup microbenchmark"]
    fn layered_prop_lookup_beats_per_node_prop_cloning() {
        let theme = mesh_core_theme::default_theme();
        let props = (0..32)
            .map(|index| {
                (
                    prop_variable_key(&format!("prop_{index}")),
                    StyleValue::Literal(format!("{index}px")),
                )
            })
            .collect::<HashMap<_, _>>();
        let resolver = StyleResolver::new(&theme).with_props(props.clone());
        let attrs = StyleNodeAttrs {
            tag: "box",
            ..StyleNodeAttrs::default()
        };
        let rules = Vec::new();
        let index = StyleRuleIndex::new(&rules);
        let context = StyleContext::default();
        let _ = resolver
            .resolve_node_style_with_attrs_indexed_no_diagnostics(&rules, &index, &attrs, context);
        let iterations = 200_000usize;

        let cloned_started = std::time::Instant::now();
        let mut cloned_total = 0usize;
        for _ in 0..iterations {
            let mut variables = HashMap::new();
            for (key, value) in &props {
                variables.insert(key.clone(), value.clone());
            }
            cloned_total = cloned_total.wrapping_add(std::hint::black_box(variables.len()));
            let style = resolver.resolve_node_style_with_attrs_indexed_no_diagnostics(
                &rules, &index, &attrs, context,
            );
            cloned_total = cloned_total.wrapping_add(std::hint::black_box(style.opacity as usize));
        }
        let cloned_time = cloned_started.elapsed();

        let layered_started = std::time::Instant::now();
        let mut layered_total = 0usize;
        for _ in 0..iterations {
            let style = resolver.resolve_node_style_with_attrs_indexed_no_diagnostics(
                std::hint::black_box(&rules),
                std::hint::black_box(&index),
                std::hint::black_box(&attrs),
                context,
            );
            layered_total =
                layered_total.wrapping_add(std::hint::black_box(style.opacity as usize));
        }
        let layered_time = layered_started.elapsed();

        eprintln!(
            "per-node prop seed: cloned {cloned_time:?}; layered {layered_time:?}; ratio {:.1}x; totals={cloned_total}/{layered_total}",
            cloned_time.as_secs_f64() / layered_time.as_secs_f64()
        );
        assert!(cloned_total > layered_total);
        assert!(layered_time < cloned_time);
    }

    // cargo test -p mesh-core-elements --release -- cached_theme_reference_beats_recanonicalizing --ignored --nocapture
    #[test]
    #[ignore = "release-only theme reference canonicalization microbenchmark"]
    fn cached_theme_reference_beats_recanonicalizing() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let reference = "--color-primary";
        let iterations = 1_000_000usize;

        let canonicalized_started = std::time::Instant::now();
        let mut canonicalized_total = 0usize;
        for _ in 0..iterations {
            let name = theme_reference_to_token_name(std::hint::black_box(reference));
            canonicalized_total = canonicalized_total.wrapping_add(name.len());
        }
        let canonicalized_time = canonicalized_started.elapsed();

        let _ = resolver.cached_theme_token_name(reference);
        let cached_started = std::time::Instant::now();
        let mut cached_total = 0usize;
        for _ in 0..iterations {
            let name = resolver.cached_theme_token_name(std::hint::black_box(reference));
            cached_total = cached_total.wrapping_add(name.len());
        }
        let cached_time = cached_started.elapsed();

        eprintln!(
            "theme reference mapping: canonicalized {canonicalized_time:?}; cached {cached_time:?}; ratio {:.1}x; totals={canonicalized_total}/{cached_total}",
            canonicalized_time.as_secs_f64() / cached_time.as_secs_f64()
        );
        assert_eq!(canonicalized_total, cached_total);
        assert!(cached_time < canonicalized_time);
    }

    #[test]
    fn cached_theme_token_value_matches_theme_lookup() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);

        match resolver.cached_theme_token_value("--color-primary") {
            CachedThemeTokenValue::String(value) => assert_eq!(
                Some(value.as_ref()),
                theme.token("color.primary").and_then(|value| match value {
                    TokenValue::String(value) => Some(value.as_str()),
                    TokenValue::Number(_) | TokenValue::Bool(_) => None,
                })
            ),
            CachedThemeTokenValue::Number(_)
            | CachedThemeTokenValue::Bool(_)
            | CachedThemeTokenValue::Missing => panic!("expected string color token"),
        }
        assert!(
            resolver
                .cached_theme_token_value("--definitely-missing")
                .is_missing()
        );
    }

    // cargo test -p mesh-core-elements --release -- cached_theme_token_value_beats_cached_name_theme_lookup --ignored --nocapture
    #[test]
    #[ignore = "release-only theme token value lookup microbenchmark"]
    fn cached_theme_token_value_beats_cached_name_theme_lookup() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let reference = "--color-primary";
        let iterations = 1_000_000usize;

        let _ = resolver.cached_theme_token_name(reference);
        let old_started = std::time::Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let name = resolver.cached_theme_token_name(std::hint::black_box(reference));
            if let Some(TokenValue::String(value)) = theme.token(&name) {
                old_total = old_total.wrapping_add(std::hint::black_box(value.len()));
            }
        }
        let old_time = old_started.elapsed();

        let _ = resolver.cached_theme_token_value(reference);
        let new_started = std::time::Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            if let CachedThemeTokenValue::String(value) =
                resolver.cached_theme_token_value(std::hint::black_box(reference))
            {
                new_total = new_total.wrapping_add(std::hint::black_box(value.len()));
            }
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "theme token value lookup: cached-name+theme {old_time:?}; cached-value {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-elements --release -- cached_embedded_theme_references_beat_recanonicalizing --ignored --nocapture
    #[test]
    #[ignore = "release-only embedded theme reference microbenchmark"]
    fn cached_embedded_theme_references_beat_recanonicalizing() {
        fn old_resolve_embedded_references(value: &str, theme: &Theme) -> String {
            let mut output = String::new();
            let mut rest = value;
            loop {
                let Some(start) = rest.find("var(") else {
                    break;
                };
                output.push_str(&rest[..start]);
                let reference_start = start + "var(".len();
                let Some(end) = rest[reference_start..].find(')') else {
                    output.push_str(&rest[start..]);
                    return output;
                };
                let name = theme_reference_to_token_name(
                    rest[reference_start..reference_start + end].trim(),
                );
                if let Some(TokenValue::String(value)) = theme.token(&name) {
                    output.push_str(value);
                }
                rest = &rest[reference_start + end + 1..];
            }
            output.push_str(rest);
            output
        }

        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let value =
            "linear-gradient(var(--color-primary), var(--color-secondary), var(--color-primary))";
        let variables = HashMap::new();
        let iterations = 300_000usize;

        let old_started = std::time::Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let resolved = old_resolve_embedded_references(
                std::hint::black_box(value),
                std::hint::black_box(&theme),
            );
            old_total = old_total.wrapping_add(std::hint::black_box(resolved.len()));
        }
        let old_time = old_started.elapsed();

        let _ = resolver.resolve_embedded_references_cached(value, &variables, false);
        let new_started = std::time::Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let resolved = resolver
                .resolve_embedded_references_cached(
                    std::hint::black_box(value),
                    std::hint::black_box(&variables),
                    false,
                )
                .expect("embedded references should resolve");
            new_total = new_total.wrapping_add(std::hint::black_box(resolved.len()));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "embedded theme references: recanonicalized {old_time:?}; cached {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-elements --release -- indexed_declaration_application_beats_uncached_validation --ignored --nocapture
    #[test]
    #[ignore = "release-only indexed declaration application microbenchmark"]
    fn indexed_declaration_application_beats_uncached_validation() {
        let theme = mesh_core_theme::default_theme();
        let resolver = StyleResolver::new(&theme);
        let declarations = vec![
            Declaration {
                property: "--accent".to_string(),
                value: StyleValue::Literal("#112233".to_string()),
            },
            Declaration {
                property: "background-color".to_string(),
                value: StyleValue::Var("--accent".to_string()),
            },
            Declaration {
                property: "color".to_string(),
                value: StyleValue::Literal("#ffffff".to_string()),
            },
            Declaration {
                property: "font-size".to_string(),
                value: StyleValue::Literal("13px".to_string()),
            },
            Declaration {
                property: "padding".to_string(),
                value: StyleValue::Literal("4px 8px".to_string()),
            },
            Declaration {
                property: "border-radius".to_string(),
                value: StyleValue::Literal("6px".to_string()),
            },
            Declaration {
                property: "gap".to_string(),
                value: StyleValue::Literal("5px".to_string()),
            },
            Declaration {
                property: "opacity".to_string(),
                value: StyleValue::Literal("0.875".to_string()),
            },
            Declaration {
                property: "unknown-property".to_string(),
                value: StyleValue::Literal("ignored".to_string()),
            },
        ];
        let rules = vec![StyleRule {
            selector: Selector::Tag("button".to_string()),
            declarations: declarations.clone(),
            container_query: None,
        }];
        let index = StyleRuleIndex::new(&rules);
        let indexed_declarations = index.no_diagnostics_declarations(0);
        let iterations = 200_000;

        let old_started = std::time::Instant::now();
        let mut old_accumulator = 0u32;
        for _ in 0..iterations {
            let mut style = ComputedStyle::default();
            let mut variables = HashMap::new();
            for declaration in &declarations {
                resolver.apply_declaration_no_diagnostics(
                    std::hint::black_box(&mut style),
                    declaration,
                    &mut variables,
                );
            }
            old_accumulator =
                old_accumulator.wrapping_add(std::hint::black_box(style.background_color.r as u32));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_accumulator = 0u32;
        for _ in 0..iterations {
            let mut style = ComputedStyle::default();
            let mut variables = HashMap::new();
            for declaration in indexed_declarations {
                resolver.apply_indexed_declaration_no_diagnostics(
                    std::hint::black_box(&mut style),
                    declaration,
                    &mut variables,
                );
            }
            new_accumulator =
                new_accumulator.wrapping_add(std::hint::black_box(style.background_color.r as u32));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "declaration apply: uncached validation {old_time:?}; indexed metadata {new_time:?}; ratio {:.1}x; accumulators={old_accumulator}/{new_accumulator}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_accumulator, new_accumulator);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-elements --release -- cached_module_theme_variables_beat_reformatting --ignored --nocapture
    #[test]
    #[ignore = "release-only module theme variable cache microbenchmark"]
    fn cached_module_theme_variables_beat_reformatting() {
        let mut theme = mesh_core_theme::default_theme();
        let module = theme.modules.entry("benchmark".into()).or_default();
        for index in 0..32 {
            module.tokens.insert(
                format!("palette.group{index}.accent"),
                TokenValue::String(format!("#{index:06x}")),
            );
        }
        let resolver = StyleResolver::new(&theme);
        let mut warm = HashMap::new();
        resolver.seed_module_theme_variables("benchmark", &mut warm);
        let iterations = 100_000;

        let old_started = std::time::Instant::now();
        let mut old_total = 0_usize;
        let mut old_variables = HashMap::new();
        for _ in 0..iterations {
            old_variables.clear();
            for (name, value) in &theme.modules["benchmark"].tokens {
                old_variables.insert(
                    format!("--{}", name.replace('.', "-")),
                    StyleValue::Literal(match value {
                        TokenValue::String(value) => value.clone(),
                        TokenValue::Number(value) => format!("{value}"),
                        TokenValue::Bool(value) => format!("{value}"),
                    }),
                );
            }
            old_total = old_total.saturating_add(std::hint::black_box(old_variables.len()));
        }
        let old_time = old_started.elapsed();

        let new_started = std::time::Instant::now();
        let mut new_total = 0_usize;
        let mut new_variables = HashMap::new();
        for _ in 0..iterations {
            new_variables.clear();
            resolver.seed_module_theme_variables("benchmark", &mut new_variables);
            new_total = new_total.saturating_add(std::hint::black_box(new_variables.len()));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "module theme variables: reformat {old_time:?}; cached {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
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
