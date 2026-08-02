use super::attrs::*;
use super::cache::*;
use super::declaration::*;
use super::matching::*;
use super::state::*;
use crate::style::parse::*;
use crate::style::*;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue};
use std::collections::HashMap;
use std::sync::Arc;

/// Bucketed view of style rules for candidate filtering.
///
/// The index owns its keys, so it can be cached across restyle passes — the
/// caller provides the rules slice it was built from for each lookup and the
/// index validates identity through `is_for()`.
#[derive(Debug, Clone)]
pub struct StyleRuleIndex {
    pub(super) rules_ptr: usize,
    pub(super) rules_len: usize,
    pub(super) tag: HashMap<String, Vec<usize>>,
    pub(super) class: HashMap<String, Vec<usize>>,
    pub(super) id: HashMap<String, Vec<usize>>,
    pub(super) state: Vec<(u32, Vec<usize>)>,
    /// Reverse index: maps individual state bits (e.g., STATE_HOVERED=1)
    /// to the rule indices that depend on that specific state.
    /// Separates per-bit dependencies from the combined bitmask entries
    /// in `state` used for forward candidate-rule lookup.
    pub(super) state_to_rules: HashMap<u32, Vec<usize>>,
    pub(super) fallback: Vec<usize>,
    pub(super) no_diagnostics_declarations: Vec<Vec<IndexedDeclaration>>,
    pub(super) selector_diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleRuleAttribution {
    pub(super) entries: Vec<StyleRuleAttributionEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleRuleAttributionEntry {
    pub selector: String,
    pub match_count: u64,
    pub elapsed_micros: u64,
}

impl StyleRuleAttribution {
    pub fn new(rules: &[StyleRule]) -> Self {
        Self {
            entries: rules
                .iter()
                .map(|rule| StyleRuleAttributionEntry {
                    selector: selector_to_diagnostic_string(&rule.selector),
                    ..StyleRuleAttributionEntry::default()
                })
                .collect(),
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = &StyleRuleAttributionEntry> {
        self.entries.iter().filter(|entry| entry.match_count > 0)
    }

    pub(super) fn record(&mut self, rule_idx: usize, elapsed: std::time::Duration) {
        let Some(entry) = self.entries.get_mut(rule_idx) else {
            return;
        };
        entry.match_count = entry.match_count.saturating_add(1);
        entry.elapsed_micros = entry
            .elapsed_micros
            .saturating_add(elapsed.as_micros().min(u128::from(u64::MAX)) as u64);
    }
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
        self.for_each_candidate_rule_index(attrs, |idx| {
            if let Some(rule) = rules.get(idx) {
                visit(rule);
            }
        });
    }

    pub(super) fn for_each_candidate_bucket<'a>(
        &'a self,
        attrs: &StyleNodeAttrs,
        mut visit: impl FnMut(&'a [usize]),
    ) {
        if !self.fallback.is_empty() {
            visit(&self.fallback);
        }
        if let Some(tag) = self.tag.get(attrs.tag) {
            visit(tag);
        }
        for class in attrs.classes.iter() {
            if let Some(class_ids) = self.class.get(class) {
                visit(class_ids);
            }
        }
        if let Some(id) = attrs.id()
            && let Some(id_ids) = self.id.get(id)
        {
            visit(id_ids);
        }
        for (state_bit, state_ids) in &self.state {
            if attrs.state_mask & *state_bit != 0 {
                visit(state_ids);
            }
        }
    }

    pub(super) fn for_each_candidate_rule_index(
        &self,
        attrs: &StyleNodeAttrs,
        mut visit: impl FnMut(usize),
    ) {
        // Keep the common one-bucket case allocation- and scratch-free. Only
        // borrow the thread-local merge buffer after a second bucket appears;
        // multi-bucket candidates still use the source-order-independent
        // sort/dedup path below.
        let mut first_bucket = None;
        let mut multiple_buckets = false;
        self.for_each_candidate_bucket(attrs, |bucket| {
            if let Some(first) = first_bucket {
                if !multiple_buckets {
                    CANDIDATE_RULE_SCRATCH.with(|scratch| {
                        let mut ids = scratch.borrow_mut();
                        ids.clear();
                        ids.extend_from_slice(first);
                        ids.extend_from_slice(bucket);
                    });
                    multiple_buckets = true;
                } else {
                    CANDIDATE_RULE_SCRATCH.with(|scratch| {
                        scratch.borrow_mut().extend_from_slice(bucket);
                    });
                }
            } else {
                first_bucket = Some(bucket);
            }
        });

        if !multiple_buckets {
            if let Some(bucket) = first_bucket {
                for &idx in bucket {
                    visit(idx);
                }
            }
            return;
        }

        CANDIDATE_RULE_SCRATCH.with(|scratch| {
            let mut ids = scratch.borrow_mut();
            ids.sort_unstable();
            ids.dedup();
            for &idx in ids.iter() {
                visit(idx);
            }
        });
    }

    pub(super) fn no_diagnostics_declarations(&self, rule_idx: usize) -> &[IndexedDeclaration] {
        self.no_diagnostics_declarations
            .get(rule_idx)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(super) fn selector_diagnostic(&self, rule_idx: usize) -> &str {
        self.selector_diagnostics
            .get(rule_idx)
            .map(String::as_str)
            .unwrap_or("")
    }

    pub(super) fn index_selector(&mut self, idx: usize, selector: &Selector) {
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

    pub(super) fn index_state_selector(&mut self, idx: usize, state: &str) {
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
pub(super) struct IndexedDeclaration {
    pub(super) property: IndexedProperty,
    pub(super) value: StyleValue,
    pub(super) literal: Option<TypedLiteralValue>,
}

/// A static declaration value parsed once when a stylesheet or theme default
/// is indexed. Values containing `var()` or `prop()` stay on the dynamic
/// resolver path; these variants are therefore safe to copy directly into a
/// node's computed style.
#[derive(Debug, Clone, Copy)]
pub(super) enum TypedLiteralValue {
    Color(Color),
    Number(f32),
    Edges(Edges),
    Corners(Corners),
    Dimension(Dimension),
}

#[derive(Debug, Clone)]
pub(super) enum IndexedProperty {
    Custom(String),
    Lowered {
        name: String,
        strict_animation: bool,
        background_image: bool,
    },
    StaticDiagnostic {
        property: String,
        message: String,
    },
}

#[derive(Clone)]
pub(super) enum CachedInlineStyle {
    Declarations(Arc<[IndexedDeclaration]>),
    Error(Arc<str>),
}

pub(super) struct CachedThemeDefaultDeclarations {
    /// The cache key includes the map address because immutable theme defaults
    /// keep it stable for their lifetime. Retaining entries across more theme
    /// churn makes allocator address reuse possible, though, so preserve the
    /// source map to reject a stale pointer-key hit before returning it.
    pub(super) source: mesh_core_theme::ComponentDefaults,
    pub(super) declarations: Arc<[IndexedDeclaration]>,
}

pub(super) fn cached_inline_style(source: &str) -> CachedInlineStyle {
    INLINE_STYLE_CACHE.with(|cache| {
        if let Some(cached) = cache.borrow_mut().get(source).cloned() {
            return cached;
        }

        let parsed = match mesh_core_component::parse_inline_style(source) {
            Ok(declarations) => CachedInlineStyle::Declarations(
                declarations
                    .iter()
                    .map(IndexedDeclaration::from_declaration)
                    .collect::<Vec<_>>()
                    .into(),
            ),
            Err(error) => CachedInlineStyle::Error(Arc::from(error.to_string())),
        };
        let mut cache = cache.borrow_mut();
        cache.insert(Arc::from(source), parsed.clone());
        parsed
    })
}

/// True when a literal style value still mentions a `var(` or `prop(`
/// reference and therefore cannot be consumed directly.
///
/// `str::contains(&str)` builds a two-way substring searcher per call, and this
/// question is asked twice for every literal declaration on every node. One
/// byte scan answers both halves with the same result.
pub(super) fn references_style_function(value: &str) -> bool {
    let bytes = value.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'(' {
            continue;
        }
        if index >= 3 && &bytes[index - 3..index] == b"var" {
            return true;
        }
        if index >= 4 && &bytes[index - 4..index] == b"prop" {
            return true;
        }
    }
    false
}

impl IndexedDeclaration {
    pub(super) fn from_declaration(decl: &Declaration) -> Self {
        Self {
            property: IndexedProperty::from_property(&decl.property, &decl.value),
            value: decl.value.clone(),
            literal: typed_literal_value(&decl.property, &decl.value),
        }
    }
}

pub(super) fn typed_literal_value(property: &str, value: &StyleValue) -> Option<TypedLiteralValue> {
    let StyleValue::Literal(value) = value else {
        return None;
    };
    if references_style_function(value) {
        return None;
    }

    match property {
        "background" | "background-color" | "color" => {
            Color::from_css(value).map(TypedLiteralValue::Color)
        }
        "font-size"
        | "font-weight"
        | "letter-spacing"
        | "line-height"
        | "gap"
        | "column-gap"
        | "row-gap"
        | "gap-x"
        | "opacity"
        | "padding-top"
        | "padding-right"
        | "padding-bottom"
        | "padding-left"
        | "margin-top"
        | "margin-right"
        | "margin-bottom"
        | "margin-left"
        | "border-top-width"
        | "border-right-width"
        | "border-bottom-width"
        | "border-left-width" => Some(TypedLiteralValue::Number(parse_px(value))),
        "padding" | "margin" | "border-width" => {
            Some(TypedLiteralValue::Edges(parse_edges_shorthand(value)))
        }
        "border-radius" => Some(TypedLiteralValue::Corners(parse_corners_shorthand(value))),
        "width" | "height" | "flex-basis" => {
            Some(TypedLiteralValue::Dimension(parse_dimension(value)))
        }
        "min-width" | "max-width" | "min-height" | "max-height" => {
            Some(TypedLiteralValue::Dimension(parse_size_constraint(value)))
        }
        _ => None,
    }
}

pub(super) fn apply_typed_literal(
    style: &mut ComputedStyle,
    property: &str,
    value: TypedLiteralValue,
) -> bool {
    match (property, value) {
        ("background" | "background-color", TypedLiteralValue::Color(value)) => {
            style.background_color = value
        }
        ("color", TypedLiteralValue::Color(value)) => style.color = value,
        ("font-size", TypedLiteralValue::Number(value)) => style.font_size = value,
        ("font-weight", TypedLiteralValue::Number(value)) => style.font_weight = value as u16,
        ("letter-spacing", TypedLiteralValue::Number(value)) => style.letter_spacing = value,
        ("line-height", TypedLiteralValue::Number(value)) => style.line_height = value,
        ("gap" | "column-gap" | "row-gap" | "gap-x", TypedLiteralValue::Number(value)) => {
            style.gap = value
        }
        ("opacity", TypedLiteralValue::Number(value)) => style.opacity = value,
        ("padding-top", TypedLiteralValue::Number(value)) => style.padding.top = value,
        ("padding-right", TypedLiteralValue::Number(value)) => style.padding.right = value,
        ("padding-bottom", TypedLiteralValue::Number(value)) => style.padding.bottom = value,
        ("padding-left", TypedLiteralValue::Number(value)) => style.padding.left = value,
        ("margin-top", TypedLiteralValue::Number(value)) => style.margin.top = value,
        ("margin-right", TypedLiteralValue::Number(value)) => style.margin.right = value,
        ("margin-bottom", TypedLiteralValue::Number(value)) => style.margin.bottom = value,
        ("margin-left", TypedLiteralValue::Number(value)) => style.margin.left = value,
        ("border-top-width", TypedLiteralValue::Number(value)) => style.border_width.top = value,
        ("border-right-width", TypedLiteralValue::Number(value)) => {
            style.border_width.right = value
        }
        ("border-bottom-width", TypedLiteralValue::Number(value)) => {
            style.border_width.bottom = value
        }
        ("border-left-width", TypedLiteralValue::Number(value)) => style.border_width.left = value,
        ("padding", TypedLiteralValue::Edges(value)) => style.padding = value,
        ("margin", TypedLiteralValue::Edges(value)) => style.margin = value,
        ("border-width", TypedLiteralValue::Edges(value)) => style.border_width = value,
        ("border-radius", TypedLiteralValue::Corners(value)) => style.border_radius = value,
        ("width", TypedLiteralValue::Dimension(value)) => style.width = value,
        ("height", TypedLiteralValue::Dimension(value)) => style.height = value,
        ("flex-basis", TypedLiteralValue::Dimension(value)) => style.flex_basis = value,
        ("min-width", TypedLiteralValue::Dimension(value)) => style.min_width = value,
        ("max-width", TypedLiteralValue::Dimension(value)) => style.max_width = value,
        ("min-height", TypedLiteralValue::Dimension(value)) => style.min_height = value,
        ("max-height", TypedLiteralValue::Dimension(value)) => style.max_height = value,
        _ => return false,
    }
    true
}

/// Lower theme defaults once per immutable theme revision. Theme defaults are
/// represented as string maps at the theme boundary, but style resolution
/// needs their indexed property and classified value forms. Rebuilding those
/// temporary declarations for each resolver cold path needlessly allocates.
///
/// The theme revision changes before mutable theme data is exposed, and a
/// component-default map has a stable address for the lifetime of that theme,
/// so the pair safely identifies this lowered representation.
pub(super) fn indexed_theme_defaults(
    revision: u64,
    defaults: &mesh_core_theme::ComponentDefaults,
) -> Arc<[IndexedDeclaration]> {
    let key = (revision, std::ptr::from_ref(defaults).cast::<()>() as usize);
    THEME_DEFAULT_DECLARATION_CACHE.with(|cache| {
        if let Some(cached) = cache.borrow_mut().get(&key)
            && cached.source == *defaults
        {
            return Arc::clone(&cached.declarations);
        }

        let declarations: Arc<[IndexedDeclaration]> = defaults
            .iter()
            .map(|(property, value)| {
                let value = classify_theme_style_value(value);
                IndexedDeclaration {
                    property: IndexedProperty::from_property(property, &value),
                    literal: typed_literal_value(property, &value),
                    value,
                }
            })
            .collect::<Vec<_>>()
            .into();

        let mut cache = cache.borrow_mut();
        cache.insert(
            key,
            CachedThemeDefaultDeclarations {
                source: defaults.clone(),
                declarations: Arc::clone(&declarations),
            },
        );
        declarations
    })
}

impl IndexedProperty {
    pub(super) fn from_property(property: &str, value: &StyleValue) -> Self {
        if property.starts_with("--") {
            return Self::Custom(property.to_owned());
        }
        if let Some(status) = style_profile_status(property) {
            match status {
                StyleProfileStatus::Implemented => {}
                StyleProfileStatus::DiagnosticOnly => {
                    return Self::StaticDiagnostic {
                        property: property.to_owned(),
                        message: format!(
                            "diagnostic-only CSS property '{property}' is accepted by the parser but not lowered into ComputedStyle"
                        ),
                    };
                }
                StyleProfileStatus::Deferred => {
                    return Self::StaticDiagnostic {
                        property: property.to_owned(),
                        message: format!(
                            "deferred CSS property '{property}' is accepted by the parser but not lowered in the current painter profile"
                        ),
                    };
                }
                StyleProfileStatus::OutOfScope => {
                    return Self::StaticDiagnostic {
                        property: property.to_owned(),
                        message: format!(
                            "unsupported CSS property '{property}' is out-of-scope for the MESH shell CSS profile"
                        ),
                    };
                }
            }
        }
        if !is_supported_css_property(property) {
            return Self::StaticDiagnostic {
                property: property.to_owned(),
                message: format!("unsupported CSS property '{property}'"),
            };
        }
        if contains_deprecated_token_reference(value) {
            return Self::StaticDiagnostic {
                property: property.to_owned(),
                message: "deprecated token() references are not supported; use var(--...)"
                    .to_owned(),
            };
        }
        Self::Lowered {
            name: property.to_owned(),
            strict_animation: is_strict_animation_property(property),
            background_image: property == "background-image",
        }
    }
}

pub(super) enum SelectorIndexKey<'a> {
    Tag(&'a str),
    Class(&'a str),
    Id(&'a str),
    State(&'a str),
}

pub(super) fn ensure_index<'cache>(
    rules: &[StyleRule],
    cache: &'cache mut Option<StyleRuleIndex>,
) -> &'cache StyleRuleIndex {
    let needs_rebuild = !cache.as_ref().is_some_and(|index| index.is_for(rules));
    if needs_rebuild {
        *cache = Some(StyleRuleIndex::new(rules));
    }
    cache.as_ref().expect("index populated above")
}
