use mesh_core_component::style::{Selector, StyleRule};
use mesh_core_elements::style::FlexDirection;
use mesh_core_elements::{ComputedStyle, Dimension, StyleContext};
use std::cell::RefCell;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct InheritedStyleMask {
    color: bool,
    font_family: bool,
    font_size: bool,
    font_weight: bool,
    line_height: bool,
}

#[derive(Clone, Copy)]
struct InheritedStyleRuleCandidate {
    index: usize,
    mask: InheritedStyleMask,
}

#[derive(Default)]
struct InheritedStyleRuleIndex {
    rules_ptr: usize,
    rules_len: usize,
    non_container: Vec<InheritedStyleRuleCandidate>,
    container: Vec<InheritedStyleRuleCandidate>,
}

impl InheritedStyleRuleIndex {
    fn is_for(&self, rules: &[StyleRule]) -> bool {
        self.rules_ptr == rules.as_ptr() as usize && self.rules_len == rules.len()
    }

    fn rebuild(&mut self, rules: &[StyleRule]) {
        self.rules_ptr = rules.as_ptr() as usize;
        self.rules_len = rules.len();
        self.non_container.clear();
        self.container.clear();
        self.non_container.reserve(rules.len().min(16));
        self.container.reserve(rules.len().min(8));

        for (index, rule) in rules.iter().enumerate() {
            let mask = inherited_declaration_mask(rule);
            if mask == InheritedStyleMask::default() {
                continue;
            }
            let candidate = InheritedStyleRuleCandidate { index, mask };
            if rule.container_query.is_some() {
                self.container.push(candidate);
            } else {
                self.non_container.push(candidate);
            }
        }
    }
}

thread_local! {
    static INHERITED_STYLE_RULE_INDEX: RefCell<InheritedStyleRuleIndex> =
        RefCell::new(InheritedStyleRuleIndex::default());
}

pub(crate) fn inherit_text_style(
    style: &mut ComputedStyle,
    parent_style: &ComputedStyle,
    explicit: InheritedStyleMask,
) {
    if !explicit.color {
        style.color = parent_style.color;
    }
    if !explicit.font_family {
        style.font_family = parent_style.font_family.clone();
    }
    if !explicit.font_size {
        style.font_size = parent_style.font_size;
    }
    if !explicit.font_weight {
        style.font_weight = parent_style.font_weight;
    }
    if !explicit.line_height {
        style.line_height = parent_style.line_height;
    }
}

pub(crate) fn inherited_style_mask(
    rules: &[StyleRule],
    tag: &str,
    classes: &[String],
    id: Option<&str>,
    context: StyleContext,
) -> InheritedStyleMask {
    INHERITED_STYLE_RULE_INDEX.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.is_for(rules) {
            cache.rebuild(rules);
        }

        let mut mask = InheritedStyleMask::default();
        for candidate in &cache.non_container {
            let rule = &rules[candidate.index];
            if selector_matches(&rule.selector, tag, classes, id) {
                mask |= candidate.mask;
            }
        }
        for candidate in &cache.container {
            let rule = &rules[candidate.index];
            if selector_matches(&rule.selector, tag, classes, id)
                && rule.container_query.is_none_or(|query| {
                    query.matches(context.container_width, context.container_height)
                })
            {
                mask |= candidate.mask;
            }
        }
        mask
    })
}

fn inherited_declaration_mask(rule: &StyleRule) -> InheritedStyleMask {
    let mut mask = InheritedStyleMask::default();
    for decl in &rule.declarations {
        match decl.property.as_str() {
            "color" => mask.color = true,
            "font-family" => mask.font_family = true,
            "font-size" => mask.font_size = true,
            "font-weight" => mask.font_weight = true,
            "line-height" => mask.line_height = true,
            _ => {}
        }
    }
    mask
}

fn selector_matches(selector: &Selector, tag: &str, classes: &[String], id: Option<&str>) -> bool {
    match selector {
        Selector::Universal => true,
        Selector::Tag(tag_name) => tag_name == tag,
        Selector::Class(class_name) => classes.iter().any(|class| class == class_name),
        Selector::Id(id_name) => id == Some(id_name.as_str()),
        Selector::State(tag_name, _state) => tag_name == "*" || tag_name == tag,
        Selector::Compound(parts) => parts
            .iter()
            .all(|part| selector_matches(part, tag, classes, id)),
    }
}

impl std::ops::BitOrAssign for InheritedStyleMask {
    fn bitor_assign(&mut self, rhs: Self) {
        self.color |= rhs.color;
        self.font_family |= rhs.font_family;
        self.font_size |= rhs.font_size;
        self.font_weight |= rhs.font_weight;
        self.line_height |= rhs.line_height;
    }
}

pub(crate) fn child_style_context(
    style: &ComputedStyle,
    parent_context: StyleContext,
) -> StyleContext {
    let width = (resolve_dimension_for_context(style.width, parent_context.container_width)
        - style.margin.horizontal())
    .max(0.0);
    let height = (resolve_dimension_for_context(style.height, parent_context.container_height)
        - style.margin.vertical())
    .max(0.0);

    StyleContext {
        container_width: (width - style.padding.horizontal()).max(0.0),
        container_height: (height - style.padding.vertical()).max(0.0),
    }
}

fn resolve_dimension_for_context(dimension: Dimension, available: f32) -> f32 {
    match dimension {
        Dimension::Px(px) => px,
        Dimension::Percent(percent) => available * percent / 100.0,
        Dimension::Auto | Dimension::Content | Dimension::Fit => available.max(0.0),
    }
}

pub(crate) fn surface_style(_surface_id: &str, width: u32, height: u32) -> ComputedStyle {
    let mut style = ComputedStyle::default();
    style.direction = FlexDirection::Column;
    style.width = mesh_core_elements::Dimension::Px(width as f32);
    style.height = mesh_core_elements::Dimension::Px(height as f32);
    style
}

/// Style for the synthetic `<column>` wrapper `{#for}`/`{#if}` blocks are
/// compiled into. This wrapper is invisible authoring structure, not a real
/// layout container an author styled. Its layout is compiler structure and is
/// intentionally independent from author-facing `column` theme defaults.
pub(crate) fn synthetic_wrapper_style() -> ComputedStyle {
    let mut style = ComputedStyle::default();
    style.direction = FlexDirection::Column;
    style
}

pub(crate) fn embedded_root_style() -> ComputedStyle {
    let mut style = ComputedStyle::default();
    style.direction = FlexDirection::Column;
    style
}

pub(crate) fn slot_style(tag: &str) -> ComputedStyle {
    let mut style = ComputedStyle::default();
    style.direction = if tag == "column" {
        FlexDirection::Column
    } else {
        FlexDirection::Row
    };
    style
}
