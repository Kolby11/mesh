# Phase 52: Style Profile And Lowering Compatibility - Pattern Map

**Mapped:** 2026-05-22
**Files analyzed:** 8
**Analogs found:** 8 / 8

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `docs/rendering/style-profile.md` | documentation | transform | `docs/css-coverage.md` | role-match |
| `crates/core/ui/elements/src/style/types.rs` | model/config | transform | `crates/core/ui/elements/src/style/types.rs` | exact |
| `crates/core/ui/elements/src/style/resolve.rs` | service/resolver | request-response | `crates/core/ui/elements/src/style/resolve.rs` | exact |
| `crates/core/ui/elements/src/style.rs` | test | request-response | `crates/core/ui/elements/src/style.rs` | exact |
| `crates/core/ui/component/src/parser/styles.rs` | parser/utility | transform | `crates/core/ui/component/src/parser/styles.rs` | exact |
| `crates/core/ui/component/src/parser.rs` | test | transform | `crates/core/ui/component/src/parser.rs` | exact |
| `modules/frontend/navigation-bar/src/main.mesh` | fixture | declarative style input | `modules/frontend/navigation-bar/src/main.mesh` | exact |
| `modules/frontend/audio-popover/src/main.mesh` | fixture | declarative style input | `modules/frontend/audio-popover/src/main.mesh` | exact |

## Pattern Assignments

### `docs/rendering/style-profile.md` (documentation, transform)

**Analog:** `docs/css-coverage.md`

**Document boundary pattern** (lines 1-5):
```markdown
# CSS Coverage in MESH

MESH supports practical shell CSS, not full browser CSS. The style parser accepts a focused subset, `mesh-core-elements` resolves tokens and local variables into `ComputedStyle`, layout consumes layout fields, and the renderer consumes visual fields.

Unsupported properties produce style diagnostics. Unsupported at-rules are rejected by the component parser instead of being silently ignored.
```

**Support matrix pattern** (lines 19-33):
```markdown
## Supported Properties

| Area | Properties |
|---|---|
| Box model and sizing | `width`, `height`, `min-width`, `max-width`, `min-height`, `max-height`, `padding`, `padding-*`, `padding-inline`, `padding-block`, `padding-x`, `padding-y`, `margin`, `margin-*`, `margin-inline`, `margin-block`, `margin-x`, `margin-y` |
| Borders and radius | `border`, `border-color`, `border-width`, `border-*-width`, `border-radius`, `border-*-radius` |
| Visuals | `background`, `background-color`, `color`, `opacity`, `visibility` |
| Typography | `font`, `font-family`, `font-size`, `font-weight`, `font-style`, `line-height`, `letter-spacing`, `text-align`, `text-overflow`, `direction` |
| Flex layout | `display`, `flex`, `flex-direction`, `flex-wrap`, `flex-grow`, `flex-shrink`, `flex-basis`, `justify-content`, `align-items`, `align-self`, `align-content`, `gap`, `row-gap`, `column-gap`, `gap-x` |
```

**Out-of-scope pattern** (lines 124-133):
```markdown
## Explicitly Out Of Scope

MESH does not implement CSS Grid, floats, multicolumn layout, full media queries, arbitrary at-rules, browser box model modes, filters, `box-shadow`, gradients/images as CSS backgrounds, generated content, or full text layout controls such as `white-space` and `word-break`.

## Engine Boundary

Parser and lowering live in `mesh-core-component`. Computed style and value resolution live in `mesh-core-elements`. Layout and paint consumption live in `mesh-core-elements` and `mesh-core-render` respectively.
```

**Apply:** keep the direct, table-driven author contract style, but update it for Phase 52 statuses: implemented, diagnostic-only, deferred, and out-of-scope. If the planner chooses to modify `docs/css-coverage.md` instead of creating `docs/rendering/style-profile.md`, use the same analog.

---

### `crates/core/ui/elements/src/style/types.rs` (model/config, transform)

**Analog:** `crates/core/ui/elements/src/style/types.rs`

**Imports and diagnostic shape** (lines 1-9):
```rust
use mesh_core_theme::TokenValue;

/// Author-facing style diagnostic emitted while resolving supported shell CSS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleDiagnostic {
    pub property: String,
    pub selector: Option<String>,
    pub message: String,
}
```

**Supported property source-of-truth pattern** (lines 11-18, 102-118):
```rust
const SUPPORTED_CSS_PROPERTIES: &[&str] = &[
    "background",
    "background-color",
    "color",
    "border",
    "border-color",
    "border-width",
    "transform",
    "transform-origin",
    "box-shadow",
    "filter",
    "backdrop-filter",
];

pub fn supported_css_properties() -> &'static [&'static str] {
    SUPPORTED_CSS_PROPERTIES
}

pub fn is_supported_css_property(property: &str) -> bool {
    property.starts_with("--") || SUPPORTED_CSS_PROPERTIES.contains(&property)
}
```

**Backend-neutral computed style pattern** (lines 121-183):
```rust
/// Fully resolved style for a widget node.
#[derive(Debug, Clone)]
pub struct ComputedStyle {
    pub border_width: Edges,
    pub background_color: Color,
    pub border_color: Color,
    pub border_radius: Corners,
    pub opacity: f32,
    pub transform: Transform2D,
    pub box_shadow: BoxShadow,
    pub filter: VisualFilter,
    pub backdrop_filter: VisualFilter,
    pub transition: TransitionStyle,
    pub animation: AnimationStyle,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
}
```

**Apply:** add any style profile metadata next to `SUPPORTED_CSS_PROPERTIES` and keep the public helper pattern. Do not add backend-specific types; `ComputedStyle` remains plain MESH data.

---

### `crates/core/ui/elements/src/style/resolve.rs` (service/resolver, request-response)

**Analog:** `crates/core/ui/elements/src/style/resolve.rs`

**Imports pattern** (lines 1-6):
```rust
use super::parse::*;
use super::*;
use crate::tree::ElementState;
use mesh_core_component::style::{Declaration, Selector, StyleRule, StyleValue};
use mesh_core_theme::{Theme, TokenValue};
use std::collections::{HashMap, HashSet};
```

**Public diagnostics API pattern** (lines 290-317):
```rust
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
```

**Token resolution pattern** (lines 169-190):
```rust
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
```

**Error/diagnostics pattern** (lines 505-550):
```rust
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
    if !is_supported_css_property(&decl.property) {
        diagnostics.push(StyleDiagnostic {
            property: decl.property.clone(),
            selector,
            message: format!("unsupported CSS property '{}'", decl.property),
        });
        return;
    }
    if is_strict_animation_property(&decl.property) {
        if let Err(token_name) =
            self.validate_animation_value_with_variables(&decl.value, variables)
        {
            diagnostics.push(StyleDiagnostic {
                property: decl.property.clone(),
                selector,
                message: format!("unresolved animation token reference '{token_name}'"),
            });
            return;
        }
    }
    apply_declaration(style, &decl.property, &decl.value, self, variables);
}
```

**Core lowering pattern** (lines 676-828):
```rust
match property {
    "background" | "background-color" => {
        style.background_color = resolver.resolve_color_with_variables(value, variables)
    }
    "border" => apply_border_shorthand(
        style,
        &resolver.resolve_value_with_variables(value, variables),
    ),
    "opacity" => style.opacity = resolver.resolve_number_with_variables(value, variables),
    "transform" => {
        style.transform =
            parse_transform(&resolver.resolve_value_with_variables(value, variables))
    }
    "box-shadow" => {
        style.box_shadow =
            parse_box_shadow(&resolver.resolve_value_with_variables(value, variables))
    }
    "filter" => {
        style.filter = parse_filter(&resolver.resolve_value_with_variables(value, variables))
    }
    "backdrop-filter" => {
        style.backdrop_filter =
            parse_filter(&resolver.resolve_value_with_variables(value, variables))
    }
```

**Apply:** add profile diagnostics in `apply_declaration_with_diagnostics`, before `apply_declaration`, so author-facing unsupported/diagnostic-only cases remain non-fatal and testable.

---

### `crates/core/ui/elements/src/style.rs` (test, request-response)

**Analog:** `crates/core/ui/elements/src/style.rs`

**Imports pattern** (lines 8-14):
```rust
#[cfg(test)]
mod tests {
    use super::parse::parse_transition_properties;
    use super::*;
    use crate::tree::ElementState;
    use mesh_core_component::style::{Selector, StyleRule, StyleValue};
```

**Supported list test pattern** (lines 56-137):
```rust
#[test]
fn supported_css_properties_cover_phase_8_contract() {
    for property in [
        "background",
        "background-color",
        "color",
        "border",
        "border-color",
        "border-width",
        "transition",
        "animation",
    ] {
        assert!(is_supported_css_property(property), "{property}");
    }
    assert!(is_supported_css_property("--local-token"));
    assert!(!is_supported_css_property("grid-template-columns"));
    assert!(is_supported_css_property("transform"));
}
```

**Unsupported diagnostic test pattern** (lines 166-195):
```rust
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
}
```

**Token and variable fixture pattern** (lines 724-755, 1180-1224):
```rust
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

assert_eq!(style.padding.left, 24.0, "padding-inline left");
```

**Apply:** add `style_profile_*`, `style_diagnostics_*`, and `shipped_navigation_style_*` tests here. Prefer inline `.mesh` fragments for matrix behavior and real fixture paths for shipped compatibility.

---

### `crates/core/ui/component/src/parser/styles.rs` (parser/utility, transform)

**Analog:** `crates/core/ui/component/src/parser/styles.rs`

**Imports pattern** (lines 1-19):
```rust
use crate::style::{
    ContainerQuery, Declaration, KeyframeRule, KeyframeStop, Selector, StyleBlock, StyleRule,
    StyleValue, is_transition_safe_keyframe_property,
};
use cssparser::{Parser, ParserInput, ToCss as CssParserToCss, Token};
use lightningcss::{
    rules::{
        CssRule as LightningCssRule,
        keyframes::{KeyframeSelector, KeyframesName},
        style::StyleRule as LightningStyleRule,
    },
    stylesheet::{ParserOptions as CssParserOptions, PrinterOptions, StyleSheet},
};
```

**Parser boundary pattern** (lines 23-38):
```rust
pub(super) fn parse_style(source: &str) -> Result<StyleBlock, ParseError> {
    let stylesheet = StyleSheet::parse(
        source,
        CssParserOptions {
            filename: "<style>".into(),
            error_recovery: false,
            ..CssParserOptions::default()
        },
    )
    .map_err(map_lightning_error)?;

    let mut rules = Vec::new();
    let mut keyframes = Vec::new();
    lower_css_rules(&stylesheet.rules.0, None, &mut rules, &mut keyframes)?;
    Ok(StyleBlock { rules, keyframes })
}
```

**Keyframe validation pattern** (lines 147-166):
```rust
fn validate_keyframe_declaration(
    rule_name: &str,
    declaration: &Declaration,
) -> Result<(), ParseError> {
    if contains_keyframe_value_reference(&declaration.value) {
        return Err(ParseError::InvalidStyle {
            message: format!(
                "keyframes '{rule_name}' cannot use token() or var() references in stop values"
            ),
            line: 0,
        });
    }
    if !is_transition_safe_keyframe_property(&declaration.property) {
        return Err(ParseError::InvalidStyle {
            message: format!("unsupported keyframe property '{}'", declaration.property),
            line: 0,
        });
    }
    Ok(())
}
```

**At-rule rejection pattern** (lines 58-67):
```rust
LightningCssRule::Keyframes(keyframes_rule) => {
    keyframes.push(lower_keyframes_rule(keyframes_rule)?);
}
LightningCssRule::Ignored => {}
other => {
    return Err(ParseError::InvalidStyle {
        message: format!("unsupported at-rule '{}'", css_rule_name(other)),
        line: 0,
    });
}
```

**Apply:** keep syntax errors in the parser and semantic/profile warnings in `StyleResolver`. Only update parser behavior if the Phase 52 matrix changes keyframe acceptance.

---

### `crates/core/ui/component/src/parser.rs` (test, transform)

**Analog:** `crates/core/ui/component/src/parser.rs`

**Imports pattern** (lines 130-137):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ComponentImportTarget, ScriptLang,
        style::{ContainerQuery, Selector, StyleValue, is_transition_safe_keyframe_property},
        template::{AttributeValue, TemplateNode},
    };
```

**Parser style token test pattern** (lines 230-249):
```rust
#[test]
fn parse_style_tokens_and_literals() {
    let source = r#"
<style>
box {
    gap: 8px;
    padding: token(spacing.md);
    background: var(--bg);
}
</style>
"#;
    let file = parse_component(source).unwrap();
    let style = file.style.unwrap();
    let decls = &style.rules[0].declarations;
    assert!(matches!(&decls[0].value, StyleValue::Literal(v) if v == "8px"));
    assert!(matches!(&decls[1].value, StyleValue::Token(v) if v == "spacing.md"));
    assert!(matches!(&decls[2].value, StyleValue::Var(v) if v == "--bg"));
    assert!(style.keyframes.is_empty());
}
```

**Current keyframe expectation pattern** (lines 308-330):
```rust
#[test]
fn keyframe_property_helper_accepts_transition_safe_properties() {
    for property in [
        "opacity",
        "transform",
        "border-radius",
        "padding",
        "font-size",
        "inset",
    ] {
        assert!(is_transition_safe_keyframe_property(property), "{property}");
    }
}

#[test]
fn keyframe_property_helper_rejects_unsupported_properties() {
    for property in ["filter", "box-shadow", "grid-template-columns", "display"] {
        assert!(!is_transition_safe_keyframe_property(property), "{property}");
    }
}
```

**Keyframe parse/error test pattern** (lines 333-402):
```rust
#[test]
fn parse_percentage_keyframes() {
    let source = r#"
<style>
@keyframes pulse {
    0% { opacity: 0; }
    50% { opacity: 0.5; }
    100% { opacity: 1; }
}
</style>
"#;
    let file = parse_component(source).unwrap();
    let style = file.style.unwrap();
    assert_eq!(style.keyframes.len(), 1);
    assert_eq!(style.keyframes[0].name, "pulse");
}
```

**Apply:** update stale rejection tests if the matrix classifies `filter`, `backdrop-filter`, or `box-shadow` keyframes as accepted metadata/deferred rendering. Keep test names behavior-focused.

---

### Shipped style fixtures (fixture, declarative style input)

**Analogs:** `modules/frontend/navigation-bar/src/main.mesh`, `modules/frontend/navigation-bar/src/components/volume-button.mesh`, `modules/frontend/audio-popover/src/main.mesh`

**Navigation compatibility declarations** (main.mesh lines 118-130):
```css
.nav-shell {
    width: 100%;
    height: 100%;
    container-type: inline-size;
    justify-content: start;
    align-items: center;
    gap: token(spacing.lg);
    padding-inline: token(spacing.lg);
    background: token(color.surface);
    color: token(color.on-surface);
    transition: background-color token(animation.duration.short) token(animation.curves.bezier.standard),
                color token(animation.duration.short) token(animation.curves.bezier.standard),
                gap token(animation.duration.short) token(animation.curves.bezier.standard);
}
```

**Navigation diagnostic fixture declarations** (volume-button.mesh lines 61-80, 106-120):
```css
.nav-button {
    width: 40px;
    height: 40px;
    flex-shrink: 0;
    justify-content: center;
    align-items: center;
    padding: token(spacing.xs);
    border-radius: token(radius.md);
    transform: translateY(0px) scale(1);
    background: token(color.surface-container);
    color: token(color.on-surface);
    border-width: 2px;
    border-style: solid;
    border-color: transparent;
    text-wrap: none;
}

.nav-button-glyph {
    color: inherit;
    --icon-fill: 0;
    --icon-weight: 400;
}
```

**Audio shipped token fixture** (audio-popover main.mesh lines 184-204, 258-270):
```css
.audio-popover-shell {
    width: 100%;
    height: 100%;
    padding: 0 token(spacing.sm) token(spacing.sm) token(spacing.sm);
    background: transparent;
    opacity: 1;
    transition: opacity token(animation.duration.short) token(animation.curves.bezier.standard);
}

.audio-action {
    height: 36px;
    padding: 0 token(spacing.md);
    justify-content: center;
    align-items: center;
    border-radius: token(radius.md);
    background: token(color.secondary-container);
    color: token(color.on-secondary-container);
    transform: scale(1);
}
```

**Apply:** shipped fixture tests should assert expected diagnostics for `container-type`, `text-wrap`, `border-style`, descendant selectors/inheritance as classified by the profile, while proving token-backed fields still resolve.

## Shared Patterns

### Backend-Neutral Render Data

**Source:** `crates/core/frontend/render/src/display_list.rs` lines 197-222
**Apply to:** `style/types.rs`, `style/resolve.rs`, style profile docs/tests

```rust
#[derive(Debug, Clone)]
pub struct DisplayPaintStyle {
    pub background_color: Color,
    pub border_color: Color,
    pub border_width: Edges,
    pub border_radius: f32,
    pub color: Color,
    pub padding: Edges,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub font_family: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub text_align: TextAlign,
    pub text_overflow: TextOverflow,
    pub text_direction: TextDirection,
    pub opacity: f32,
    pub box_shadow: BoxShadow,
    pub filter: VisualFilter,
    pub backdrop_filter: VisualFilter,
}
```

### Author Contract Boundary

**Source:** `docs/frontend/renderer-contract.md` lines 3-9, 37-43
**Apply to:** style profile documentation and diagnostics wording

```markdown
## Current Author Contract

- `.mesh template/script/style syntax remains the public authoring surface.`
- `Service proxies, theme tokens, locale helpers, capabilities, module dependencies, and explicit component imports remain the integration model.`

## Not Promised

- `.mesh is not HTML/CSS in a browser engine.`
- `Arbitrary DOM/web platform behavior is not promised.`
```

### Ownership Boundary

**Source:** `docs/renderer-ownership.md` lines 11-20, 46-50
**Apply to:** docs and code review checks for no `skia_safe` in style/display-list metadata

```markdown
| Component source parsing | authoritative | `crates/core/ui/component/src/lib.rs` | Parses author-facing `.mesh` single-file components before renderer migration touches runtime output. |
| Retained display-list ownership | authoritative | `crates/core/frontend/render/src/display_list.rs` | Owns paint command identity, selection payloads, damage data, repaint policy, and batching evidence. |
| Skia paint backend | authoritative | `crates/core/frontend/render/src/surface/painter/backend.rs` | Owns the low-level painter/raster work below MESH paint commands: antialiasing, paths, rounded rects, strokes, shadows, blur/image filters, blend modes, clipping, layers/saveLayer, and related Skia canvas behavior. |

Vello compatibility is a contract-shaping constraint, not Phase 51 production
scope. The painter API should stay backend-neutral while allowing Skia to use
Skia-specific primitives internally.
```

### Transform Parser Pattern

**Source:** `crates/core/ui/elements/src/style/parse.rs` lines 29-107
**Apply to:** profile classification for transform lowering support

```rust
pub(super) fn parse_transform(value: &str) -> Transform2D {
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
        match name {
            "translate" => { /* lowers translation values */ }
            "scale" => { /* lowers scale values */ }
            "rotate" => { /* lowers rotation values */ }
            _ => {}
        }
    }
    transform
}
```

## No Analog Found

All planned files have close analogs in the existing codebase. There is no existing dedicated `docs/rendering/style-profile.md`; use `docs/css-coverage.md` as the role-match analog if creating that file.

## Metadata

**Analog search scope:** `docs/`, `crates/core/ui/elements/src/style*`, `crates/core/ui/component/src/*`, `crates/core/frontend/render/src/display_list.rs`, `modules/frontend/navigation-bar/`, `modules/frontend/audio-popover/`
**Files scanned:** 65
**Project instructions:** no `AGENTS.md` found under `/home/kolby/projects/mesh` parents during this run.
**Project skills:** no project `.codex/skills/` or `.agents/skills/` `SKILL.md` files found.
**Pattern extraction date:** 2026-05-22
