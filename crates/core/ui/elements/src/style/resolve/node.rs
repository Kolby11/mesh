use super::StyleResolver;
use super::attrs::*;
use super::cache::*;
use super::index::*;
use super::matching::*;
use crate::style::*;
use crate::tree::ElementState;
use mesh_core_component::style::StyleRule;
use std::collections::HashMap;
use std::rc::Rc;

impl<'a> StyleResolver<'a> {
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

    #[allow(clippy::too_many_arguments)]
    pub fn resolve_node_style_for_module_indexed_with_inline_style(
        &self,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        tag: &str,
        classes: &[String],
        id: Option<&str>,
        inline_style: Option<&str>,
        context: StyleContext,
        state: ElementState,
        module_id: Option<&str>,
    ) -> ComputedStyle {
        debug_assert!(index.is_for(rules));
        let mut attrs = StyleNodeAttrs::new(tag, classes, id, state);
        attrs.module_id = module_id;
        attrs.inline_style = inline_style;
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

    /// Resolve a live widget node through an existing rule index while reusing
    /// the node's cached class tokens.
    ///
    /// Runtime diagnostic passes run immediately after the ordinary restyle,
    /// which has already refreshed this cache. Taking the node directly avoids
    /// rebuilding an owned `Vec<String>` from its `class` attribute for every
    /// node in the tree.
    pub fn resolve_node_style_with_diagnostics_for_node_indexed(
        &self,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        node: &mut crate::tree::WidgetNode,
        context: StyleContext,
    ) -> (ComputedStyle, Vec<StyleDiagnostic>) {
        debug_assert!(index.is_for(rules));
        let attrs = StyleNodeAttrs::from_node(node);
        self.resolve_node_style_with_attrs_indexed(rules, index, &attrs, context)
    }

    pub(super) fn resolve_node_style_with_attrs(
        &self,
        rules: &[StyleRule],
        attrs: &StyleNodeAttrs,
        context: StyleContext,
    ) -> (ComputedStyle, Vec<StyleDiagnostic>) {
        let index = StyleRuleIndex::new(rules);
        self.resolve_node_style_with_attrs_indexed(rules, &index, attrs, context)
    }

    pub(super) fn resolve_node_style_with_attrs_no_diagnostics(
        &self,
        rules: &[StyleRule],
        attrs: &StyleNodeAttrs,
        context: StyleContext,
    ) -> ComputedStyle {
        let index = StyleRuleIndex::new(rules);
        self.resolve_node_style_with_attrs_indexed_no_diagnostics(rules, &index, attrs, context)
    }

    pub(super) fn resolve_node_style_with_attrs_indexed_no_diagnostics(
        &self,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        attrs: &StyleNodeAttrs,
        context: StyleContext,
    ) -> ComputedStyle {
        self.resolve_node_style_with_attrs_indexed_inner(rules, index, attrs, context, None, None)
    }

    pub(super) fn cached_theme_component_defaults_no_diagnostics(
        &self,
        tag: &str,
        module_id: Option<&str>,
    ) -> Rc<ThemeComponentDefaults> {
        if let Some(recent) = self.recent_theme_defaults(tag, module_id) {
            return recent;
        }

        if let Some(cached) = shared_theme_defaults(
            self.theme.revision(),
            self.props_fingerprint,
            &self.props,
            tag,
            module_id,
        ) {
            self.remember_theme_defaults(tag, module_id, &cached);
            return cached;
        }

        let mut style = ComputedStyle::default();
        let mut variables = HashMap::new();
        self.apply_theme_component_defaults(&mut style, tag, module_id, None, &mut variables);
        let cached = Rc::new(ThemeComponentDefaults { style, variables });
        remember_shared_theme_defaults(
            self.theme.revision(),
            self.props_fingerprint,
            &self.props,
            tag,
            module_id,
            &cached,
        );
        self.remember_theme_defaults(tag, module_id, &cached);
        cached
    }

    /// Look for `(module_id, tag)` in the comparison-keyed front cache.
    ///
    /// Most-recently-used order, so the run of sibling nodes sharing a tag
    /// answers on the first compare.
    pub(super) fn recent_theme_defaults(
        &self,
        tag: &str,
        module_id: Option<&str>,
    ) -> Option<Rc<ThemeComponentDefaults>> {
        let mut recent = self.theme_default_recent.borrow_mut();
        let position = recent
            .iter()
            .position(|entry| entry.tag == tag && entry.module_id.as_deref() == module_id)?;
        if position > 0 {
            recent[..=position].rotate_right(1);
        }
        Some(Rc::clone(&recent[0].defaults))
    }

    pub(super) fn remember_theme_defaults(
        &self,
        tag: &str,
        module_id: Option<&str>,
        defaults: &Rc<ThemeComponentDefaults>,
    ) {
        let mut recent = self.theme_default_recent.borrow_mut();
        if recent.len() == THEME_DEFAULT_RECENT_CAPACITY {
            recent.pop();
        }
        recent.insert(
            0,
            RecentThemeDefaults {
                module_id: module_id.map(str::to_owned),
                tag: tag.to_owned(),
                defaults: Rc::clone(defaults),
            },
        );
    }

    pub(super) fn resolve_node_style_with_attrs_indexed(
        &self,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        attrs: &StyleNodeAttrs,
        context: StyleContext,
    ) -> (ComputedStyle, Vec<StyleDiagnostic>) {
        let mut diagnostics = Vec::new();
        let style = self.resolve_node_style_with_attrs_indexed_inner(
            rules,
            index,
            attrs,
            context,
            Some(&mut diagnostics),
            None,
        );
        (style, diagnostics)
    }

    pub(super) fn resolve_node_style_with_attrs_indexed_inner(
        &self,
        rules: &[StyleRule],
        index: &StyleRuleIndex,
        attrs: &StyleNodeAttrs,
        context: StyleContext,
        mut diagnostics: Option<&mut Vec<StyleDiagnostic>>,
        mut attribution: Option<&mut StyleRuleAttribution>,
    ) -> ComputedStyle {
        // `shared` holds the defaults by reference when they came from the
        // no-diagnostics cache; the diagnostics path still builds its own copy
        // because it also carries per-resolution diagnostics.
        let mut shared = None;
        let (mut style, default_variables) = if let Some(diagnostics) = diagnostics.as_mut() {
            let (style, variables, default_diagnostics) =
                self.cached_theme_component_defaults_with_diagnostics(attrs.tag, attrs.module_id());
            diagnostics.extend(default_diagnostics);
            (style, Some(variables))
        } else {
            let defaults =
                self.cached_theme_component_defaults_no_diagnostics(attrs.tag, attrs.module_id());
            let style = defaults.style.clone();
            shared = Some(defaults);
            (style, None)
        };

        VARIABLE_SCRATCH.with(|scratch| {
            let mut scratch_variables = scratch.borrow_mut();
            scratch_variables.clear();
            // Themes usually declare no default custom properties at all, so
            // the common case seeds nothing and every lookup falls through to
            // the node's own declarations.
            match (&shared, default_variables) {
                (Some(defaults), _) if !defaults.variables.is_empty() => {
                    scratch_variables.extend(
                        defaults
                            .variables
                            .iter()
                            .map(|(name, value)| (name.clone(), value.clone())),
                    );
                }
                (_, Some(variables)) => scratch_variables.extend(variables),
                _ => {}
            }

            if let Some(attribution) = attribution.as_deref_mut() {
                index.for_each_candidate_rule_index(attrs, |rule_idx| {
                    let Some(rule) = rules.get(rule_idx) else {
                        return;
                    };
                    let started = std::time::Instant::now();
                    if rule_matches_attrs(rule, attrs, context) {
                        let selector = index.selector_diagnostic(rule_idx);
                        for decl in index.no_diagnostics_declarations(rule_idx) {
                            let diagnostic_sink = diagnostics
                                .as_mut()
                                .map(|diagnostics| (selector, &mut **diagnostics));
                            self.apply_indexed_declaration(
                                &mut style,
                                decl,
                                diagnostic_sink,
                                &mut scratch_variables,
                            );
                        }
                        attribution.record(rule_idx, started.elapsed());
                    }
                });
            } else {
                // Keep the production path clock-free and free of per-rule
                // profiling branches. The single option branch is per node.
                index.for_each_candidate_rule_index(attrs, |rule_idx| {
                    let Some(rule) = rules.get(rule_idx) else {
                        return;
                    };
                    if rule_matches_attrs(rule, attrs, context) {
                        let selector = index.selector_diagnostic(rule_idx);
                        for decl in index.no_diagnostics_declarations(rule_idx) {
                            let diagnostic_sink = diagnostics
                                .as_mut()
                                .map(|diagnostics| (selector, &mut **diagnostics));
                            self.apply_indexed_declaration(
                                &mut style,
                                decl,
                                diagnostic_sink,
                                &mut scratch_variables,
                            );
                        }
                    }
                });
            }

            if let Some(inline_style) = attrs.inline_style {
                match cached_inline_style(inline_style) {
                    CachedInlineStyle::Declarations(declarations) => {
                        for declaration in declarations.iter() {
                            let diagnostic_sink = diagnostics
                                .as_mut()
                                .map(|diagnostics| ("@inline", &mut **diagnostics));
                            self.apply_indexed_declaration(
                                &mut style,
                                declaration,
                                diagnostic_sink,
                                &mut scratch_variables,
                            );
                        }
                    }
                    CachedInlineStyle::Error(error) => {
                        if let Some(diagnostics) = diagnostics.as_mut() {
                            diagnostics.push(StyleDiagnostic {
                                property: "style".into(),
                                selector: Some("@inline".into()),
                                message: error.to_string(),
                            });
                        }
                    }
                }
            }
        });

        style
    }

    pub(super) fn cached_theme_component_defaults_with_diagnostics(
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
        let mut diagnostics = Vec::new();
        let mut default_variables = HashMap::new();
        self.apply_theme_component_defaults(
            &mut style,
            tag,
            module_id,
            Some(&mut diagnostics),
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
}
