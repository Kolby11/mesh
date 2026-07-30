use mesh_core_component::style::{Selector, StyleRule};
use mesh_core_elements::style::FlexDirection;
use mesh_core_elements::{ComputedStyle, Dimension, StyleContext};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct InheritedStyleMask {
    color: bool,
    font_family: bool,
    font_size: bool,
    font_weight: bool,
    line_height: bool,
}

impl InheritedStyleMask {
    /// True when every bit this mask sets is already set in `accumulated`, so
    /// matching its rule cannot change the result.
    fn adds_nothing_to(self, accumulated: Self) -> bool {
        (!self.color || accumulated.color)
            && (!self.font_family || accumulated.font_family)
            && (!self.font_size || accumulated.font_size)
            && (!self.font_weight || accumulated.font_weight)
            && (!self.line_height || accumulated.line_height)
    }
}

#[derive(Clone, Copy)]
struct InheritedStyleRuleCandidate {
    index: usize,
    mask: InheritedStyleMask,
}

/// Candidates grouped by a selector key that is *necessary* for a match.
///
/// Every selector shape MESH supports here is either a single simple selector
/// or a compound whose parts must all match, so any one part's key is a
/// necessary condition. Nodes therefore only need to visit the buckets for
/// their own tag, classes, and id, plus the unkeyed bucket (universal and
/// state-on-`*` selectors). Bucketing prunes; it never decides a match on its
/// own — `selector_matches` still runs on every visited candidate.
#[derive(Default)]
struct InheritedStyleCandidateBuckets {
    unkeyed: Vec<InheritedStyleRuleCandidate>,
    by_tag: HashMap<String, Vec<InheritedStyleRuleCandidate>>,
    by_class: HashMap<String, Vec<InheritedStyleRuleCandidate>>,
    by_id: HashMap<String, Vec<InheritedStyleRuleCandidate>>,
}

/// The bucket a candidate is filed under.
enum SelectorKey<'a> {
    Tag(&'a str),
    Class(&'a str),
    Id(&'a str),
    Unkeyed,
}

impl InheritedStyleCandidateBuckets {
    fn clear(&mut self) {
        self.unkeyed.clear();
        // Keep the allocated bucket vectors; rule sets churn between rebuilds
        // but their selector shapes mostly repeat.
        self.by_tag.clear();
        self.by_class.clear();
        self.by_id.clear();
    }

    fn is_empty(&self) -> bool {
        self.unkeyed.is_empty()
            && self.by_tag.is_empty()
            && self.by_class.is_empty()
            && self.by_id.is_empty()
    }

    fn push(&mut self, selector: &Selector, candidate: InheritedStyleRuleCandidate) {
        match selector_key(selector) {
            SelectorKey::Tag(tag) => self.by_tag.entry(tag.to_string()).or_default(),
            SelectorKey::Class(class) => self.by_class.entry(class.to_string()).or_default(),
            SelectorKey::Id(id) => self.by_id.entry(id.to_string()).or_default(),
            SelectorKey::Unkeyed => &mut self.unkeyed,
        }
        .push(candidate);
    }

    /// Visit every candidate that could match this node, in no particular
    /// order: the mask is OR-accumulated, so bucket order does not matter.
    fn visit_matching<F: FnMut(&[InheritedStyleRuleCandidate])>(
        &self,
        tag: &str,
        classes: &[String],
        id: Option<&str>,
        mut visit: F,
    ) {
        if !self.unkeyed.is_empty() {
            visit(&self.unkeyed);
        }
        if let Some(candidates) = self.by_tag.get(tag) {
            visit(candidates);
        }
        if !self.by_class.is_empty() {
            for class in classes {
                if let Some(candidates) = self.by_class.get(class.as_str()) {
                    visit(candidates);
                }
            }
        }
        if let Some(id) = id {
            if let Some(candidates) = self.by_id.get(id) {
                visit(candidates);
            }
        }
    }
}

/// Pick a selector part whose match is required for the whole selector to
/// match. Compound selectors require all parts, so the first keyable part is a
/// sound choice; anything else stays unkeyed and is visited by every node.
fn selector_key(selector: &Selector) -> SelectorKey<'_> {
    match selector {
        Selector::Tag(tag) => SelectorKey::Tag(tag.as_str()),
        Selector::Class(class) => SelectorKey::Class(class.as_str()),
        Selector::Id(id) => SelectorKey::Id(id.as_str()),
        Selector::State(tag, _) if tag != "*" => SelectorKey::Tag(tag.as_str()),
        Selector::Compound(parts) => parts
            .iter()
            .map(selector_key)
            .find(|key| !matches!(key, SelectorKey::Unkeyed))
            .unwrap_or(SelectorKey::Unkeyed),
        Selector::State(_, _) | Selector::Universal => SelectorKey::Unkeyed,
    }
}

#[derive(Default)]
struct InheritedStyleRuleIndex {
    rules_ptr: usize,
    rules_len: usize,
    non_container: InheritedStyleCandidateBuckets,
    container: InheritedStyleCandidateBuckets,
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

        for (index, rule) in rules.iter().enumerate() {
            let mask = inherited_declaration_mask(rule);
            if mask == InheritedStyleMask::default() {
                continue;
            }
            let candidate = InheritedStyleRuleCandidate { index, mask };
            if rule.container_query.is_some() {
                self.container.push(&rule.selector, candidate);
            } else {
                self.non_container.push(&rule.selector, candidate);
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
        cache
            .non_container
            .visit_matching(tag, classes, id, |candidates| {
                for candidate in candidates {
                    if candidate.mask.adds_nothing_to(mask) {
                        continue;
                    }
                    let rule = &rules[candidate.index];
                    if selector_matches(&rule.selector, tag, classes, id) {
                        mask |= candidate.mask;
                    }
                }
            });
        if !cache.container.is_empty() {
            cache
                .container
                .visit_matching(tag, classes, id, |candidates| {
                    for candidate in candidates {
                        if candidate.mask.adds_nothing_to(mask) {
                            continue;
                        }
                        let rule = &rules[candidate.index];
                        if selector_matches(&rule.selector, tag, classes, id)
                            && rule.container_query.is_none_or(|query| {
                                query.matches(context.container_width, context.container_height)
                            })
                        {
                            mask |= candidate.mask;
                        }
                    }
                });
        }
        mask
    })
}

/// The pre-bucketing implementation: one cached flat candidate list per rule
/// set, scanned in full for every node. Retained as the parity reference and
/// benchmark baseline for [`inherited_style_mask`].
#[cfg(test)]
#[derive(Default)]
struct FlatInheritedStyleRuleIndex {
    rules_ptr: usize,
    rules_len: usize,
    non_container: Vec<InheritedStyleRuleCandidate>,
    container: Vec<InheritedStyleRuleCandidate>,
}

#[cfg(test)]
thread_local! {
    static FLAT_INHERITED_STYLE_RULE_INDEX: RefCell<FlatInheritedStyleRuleIndex> =
        RefCell::new(FlatInheritedStyleRuleIndex::default());
}

#[cfg(test)]
pub(crate) fn inherited_style_mask_scan(
    rules: &[StyleRule],
    tag: &str,
    classes: &[String],
    id: Option<&str>,
    context: StyleContext,
) -> InheritedStyleMask {
    FLAT_INHERITED_STYLE_RULE_INDEX.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.rules_ptr != rules.as_ptr() as usize || cache.rules_len != rules.len() {
            cache.rules_ptr = rules.as_ptr() as usize;
            cache.rules_len = rules.len();
            cache.non_container.clear();
            cache.container.clear();
            for (index, rule) in rules.iter().enumerate() {
                let mask = inherited_declaration_mask(rule);
                if mask == InheritedStyleMask::default() {
                    continue;
                }
                let candidate = InheritedStyleRuleCandidate { index, mask };
                if rule.container_query.is_some() {
                    cache.container.push(candidate);
                } else {
                    cache.non_container.push(candidate);
                }
            }
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

#[cfg(test)]
mod inherited_mask_tests {
    use super::*;
    use mesh_core_component::style::{ContainerQuery, Declaration, StyleValue};

    fn rule(selector: Selector, property: &str) -> StyleRule {
        StyleRule {
            selector,
            declarations: vec![Declaration {
                property: property.to_string(),
                value: StyleValue::Literal("inherit-me".to_string()),
            }],
            container_query: None,
        }
    }

    fn container_rule(selector: Selector, property: &str) -> StyleRule {
        let mut rule = rule(selector, property);
        rule.container_query = Some(ContainerQuery {
            min_width: Some(100.0),
            ..ContainerQuery::default()
        });
        rule
    }

    /// A rule set shaped like a real component stylesheet: theme-wide
    /// universal rules, many tag and class rules, a few id and compound and
    /// state rules, a couple of container rules, and a long tail of rules with
    /// no inheritable declarations at all.
    fn representative_rules() -> Vec<StyleRule> {
        let inheritable = ["color", "font-size", "font-weight", "font-family"];
        let mut rules = vec![
            rule(Selector::Universal, "font-family"),
            rule(
                Selector::State("*".to_string(), "hover".to_string()),
                "color",
            ),
        ];
        for (index, tag) in ["row", "column", "box", "button", "text", "icon", "input"]
            .into_iter()
            .enumerate()
        {
            rules.push(rule(
                Selector::Tag(tag.to_string()),
                inheritable[index % inheritable.len()],
            ));
            rules.push(rule(
                Selector::State(tag.to_string(), "hover".to_string()),
                "color",
            ));
        }
        for index in 0..24 {
            rules.push(rule(
                Selector::Class(format!("class-{index}")),
                inheritable[index % inheritable.len()],
            ));
        }
        for index in 0..6 {
            rules.push(rule(Selector::Id(format!("id-{index}")), "color"));
        }
        for index in 0..6 {
            rules.push(rule(
                Selector::Compound(vec![
                    Selector::Tag("button".to_string()),
                    Selector::Class(format!("variant-{index}")),
                ]),
                "font-weight",
            ));
        }
        rules.push(container_rule(
            Selector::Tag("row".to_string()),
            "font-size",
        ));
        rules.push(container_rule(
            Selector::Class("responsive".to_string()),
            "color",
        ));
        // Rules with no inheritable declarations: they must be skipped by both
        // paths and never reach a bucket.
        for index in 0..40 {
            rules.push(rule(
                Selector::Class(format!("layout-{index}")),
                "padding-left",
            ));
        }
        rules
    }

    fn cases() -> Vec<(&'static str, Vec<String>, Option<String>)> {
        vec![
            ("column", vec![], None),
            ("row", vec!["class-3".to_string()], None),
            (
                "button",
                vec!["variant-2".to_string(), "class-11".to_string()],
                None,
            ),
            (
                "text",
                vec!["layout-7".to_string()],
                Some("id-4".to_string()),
            ),
            ("box", vec!["responsive".to_string()], None),
            (
                "icon",
                vec!["unmatched".to_string()],
                Some("absent".to_string()),
            ),
            ("custom-element", vec![], None),
        ]
    }

    #[test]
    fn bucketed_inherited_mask_matches_full_scan() {
        let rules = representative_rules();
        for context in [
            StyleContext {
                container_width: 320.0,
                container_height: 200.0,
            },
            StyleContext {
                container_width: 60.0,
                container_height: 40.0,
            },
        ] {
            for (tag, classes, id) in cases() {
                let bucketed = inherited_style_mask(&rules, tag, &classes, id.as_deref(), context);
                let scanned =
                    inherited_style_mask_scan(&rules, tag, &classes, id.as_deref(), context);
                assert!(
                    bucketed == scanned,
                    "mask mismatch for <{tag}> classes={classes:?} id={id:?}"
                );
            }
        }
    }

    #[test]
    fn bucketed_inherited_mask_rebuilds_for_a_new_rule_set() {
        let first = vec![rule(Selector::Tag("row".to_string()), "color")];
        let second = vec![rule(Selector::Tag("row".to_string()), "font-size")];
        let context = StyleContext::default();
        let first_mask = inherited_style_mask(&first, "row", &[], None, context);
        assert!(first_mask.color && !first_mask.font_size);
        let second_mask = inherited_style_mask(&second, "row", &[], None, context);
        assert!(second_mask.font_size && !second_mask.color);
    }

    // cargo test -p mesh-core-frontend --release -- inherited_mask_buckets_beat_full_candidate_scan --ignored --nocapture
    #[test]
    #[ignore = "release-only inherited-style-mask index microbenchmark"]
    fn inherited_mask_buckets_beat_full_candidate_scan() {
        use std::time::Instant;

        let rules = representative_rules();
        let context = StyleContext {
            container_width: 320.0,
            container_height: 200.0,
        };
        let nodes = cases();
        let passes = 200_000usize;

        // Warm both thread-local indexes so neither pays its build cost.
        let _ = inherited_style_mask_scan(&rules, "row", &[], None, context);
        let _ = inherited_style_mask(&rules, "row", &[], None, context);

        let scan_started = Instant::now();
        let mut scan_checksum = 0usize;
        for _ in 0..passes {
            for (tag, classes, id) in &nodes {
                let mask = inherited_style_mask_scan(
                    std::hint::black_box(&rules),
                    tag,
                    classes,
                    id.as_deref(),
                    context,
                );
                scan_checksum += mask.color as usize + mask.font_size as usize;
            }
        }
        let scan_time = scan_started.elapsed();

        let bucket_started = Instant::now();
        let mut bucket_checksum = 0usize;
        for _ in 0..passes {
            for (tag, classes, id) in &nodes {
                let mask = inherited_style_mask(
                    std::hint::black_box(&rules),
                    tag,
                    classes,
                    id.as_deref(),
                    context,
                );
                bucket_checksum += mask.color as usize + mask.font_size as usize;
            }
        }
        let bucket_time = bucket_started.elapsed();

        eprintln!(
            "inherited style mask over {} lookups ({} rules): full scan {scan_time:?}, bucketed {bucket_time:?}, ratio {:.2}x",
            passes * nodes.len(),
            rules.len(),
            scan_time.as_secs_f64() / bucket_time.as_secs_f64()
        );
        println!(
            "MESH_PERF metric=inherited_style_mask_bucket_speedup value={:.6}",
            scan_time.as_secs_f64() / bucket_time.as_secs_f64()
        );
        assert_eq!(scan_checksum, bucket_checksum);
        assert!(bucket_time < scan_time);
    }
}
