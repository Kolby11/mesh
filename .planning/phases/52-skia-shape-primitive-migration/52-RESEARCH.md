# Phase 52: Style Profile And Lowering Compatibility - Research

**Researched:** 2026-05-22  
**Domain:** MESH `.mesh` style parser, token resolver, computed style profile, painter lowering compatibility  
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
## Implementation Decisions

### Style Profile Scope
- The supported profile is MESH shell CSS, not browser CSS. Keep XML/.mesh tags
  and existing MESH element vocabulary authoritative; do not add arbitrary HTML
  or DOM compatibility.
- Classify style properties as implemented, diagnostic-only, deferred, or
  out-of-scope. This classification should be visible in docs/tests, not only in
  code comments.
- Preserve existing parser/resolver support for color, size, spacing, border,
  radius, opacity, transform, shadow, filter, layout, font, animation, and
  transition properties that already compile.
- Treat unsupported web-like properties as diagnostics. Silent acceptance of
  properties MESH cannot lower/render is not acceptable.

### Token Compatibility
- Theme tokens remain resolved through the existing `mesh-core-theme` and
  `StyleResolver` path. Do not introduce a parallel token system for painter
  work.
- CSS custom properties that already work as local variables remain supported;
  painter profile documentation should distinguish them from theme tokens.
- Token resolution failures should stay actionable and testable, especially for
  animation and painter-relevant visual properties.
- Shipped navigation/audio module styles are compatibility fixtures for this
  phase.

### Lowering Boundary
- Style data passed toward render objects, display lists, and painter commands
  must remain backend-neutral. No `skia_safe` types belong in `mesh-core-elements`
  style structs, retained display-list data, or render-object data.
- Phase 52 may add profile metadata, documentation, diagnostics, and tests, but
  broad command lowering and helper bypass removal belong to Phase 53.
- Existing `ComputedStyle`, `StyleDiagnostic`, `supported_css_properties`, and
  `StyleResolver` are the preferred integration points.
- Parser/resolver changes should be conservative and compile-safe; avoid new
  parser architecture unless current structures cannot express the profile.

### Autonomous Planning Defaults
- Prefer focused plans that write a support matrix/documentation first, then add
  resolver diagnostics/tests, then prove shipped style compatibility.
- Verification should include targeted `mesh-core-elements` style tests and
  shell/frontend fixtures for shipped navigation/audio styles where existing
  test harnesses make that practical.
- If a property is already parsed but not yet rendered, mark it diagnostic-only
  or deferred according to current behavior rather than pretending painter
  support exists.
- Leave animation behavior implementation to Phase 56 while documenting the
  currently accepted animation property surface.

### the agent's Discretion
The planner may choose exact file names for the style profile document and may
decide whether the property matrix lives in docs, render docs, or
`mesh-core-elements` tests, provided `.planning/REQUIREMENTS.md` traceability and
author-facing compatibility are preserved.

### Deferred Ideas (OUT OF SCOPE)
## Deferred Ideas

- Element/control command coverage belongs to Phase 53.
- Skia primitive execution belongs to Phase 54.
- Shadows, blur, images, gradients, and layer effects belong to Phase 55.
- Animation invalidation and transition paint integration belong to Phase 56.
- Damage/visual-bounds correctness belongs to Phase 57.
- Backend observability/rollback belongs to Phase 58.
- Full browser CSS, arbitrary HTML parsing, DOM APIs, and browser layout modes
  remain out of scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| STYLE-01 | Maintainer has a documented bounded style profile for MESH's XML/.mesh, CSS-like syntax, and theme tokens, covering supported visual properties and explicitly excluding arbitrary browser CSS. | The current author-facing property list is `SUPPORTED_CSS_PROPERTIES`; `.mesh` style parsing is delegated to Lightning CSS then lowered to MESH-owned `StyleRule`/`Declaration` data; unsupported at-rules and container-query forms already error in parser lowering. [VERIFIED: crates/core/ui/elements/src/style/types.rs:11] [VERIFIED: crates/core/ui/component/src/parser/styles.rs:23] [VERIFIED: crates/core/ui/component/src/parser/styles.rs:40] |
| STYLE-02 | Existing token references and shipped module styles continue to resolve through the current theme/token pipeline while painter-relevant values lower into backend-neutral render data. | `StyleResolver` resolves `StyleValue::Token`, embedded `token(...)`, and local `var(...)` references before applying declarations; `ComputedStyle` and `DisplayPaintStyle` carry plain MESH structs and primitives, not Skia types. [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:169] [VERIFIED: crates/core/ui/elements/src/style/types.rs:121] [VERIFIED: crates/core/frontend/render/src/display_list.rs:197] |
| STYLE-03 | Unsupported or ambiguous web-style properties produce diagnostics instead of being silently accepted with missing visual behavior. | Unsupported declaration names already flow through `StyleDiagnostic`; missing CSS variables and missing strict animation tokens also produce diagnostics, but accepted-yet-unlowered properties still need profile classification/tests. [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:505] [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:525] [VERIFIED: crates/core/ui/elements/src/style/types.rs:102] |
</phase_requirements>

## Summary

Phase 52 should standardize the current MESH style profile around the existing parser/resolver boundary, not around browser CSS. The reliable source of truth is the current lowering chain: `.mesh` style text is parsed by `mesh-core-component`, lowered into MESH `StyleRule`/`Declaration`/`KeyframeRule` structs, resolved by `mesh-core-elements::StyleResolver`, then carried into backend-neutral `ComputedStyle` and display-list paint style data. [VERIFIED: crates/core/ui/component/src/parser/styles.rs:23] [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:152] [VERIFIED: crates/core/ui/elements/src/style/types.rs:121] [VERIFIED: crates/core/frontend/render/src/display_list.rs:197]

The biggest planning risk is confusing “Lightning CSS can parse this syntax” or “`SUPPORTED_CSS_PROPERTIES` accepts this property” with “MESH lowers and renders this property.” `transform-origin` is listed as supported but has no `apply_declaration` branch, and shipped navigation styles contain `container-type`, `text-wrap`, `border-style`, descendant selectors, `inherit`, and `transparent` values that should be classified as compatibility diagnostics, profile aliases, deferred behavior, or explicit no-ops. [VERIFIED: crates/core/ui/elements/src/style/types.rs:102] [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:676] [VERIFIED: modules/frontend/navigation-bar/src/main.mesh:121] [VERIFIED: modules/frontend/navigation-bar/src/main.mesh:155] [VERIFIED: modules/frontend/navigation-bar/src/components/volume-button.mesh:324]

**Primary recommendation:** implement a versioned MESH painter style support matrix first, then drive code/tests from that matrix by extending `supported_css_properties`, `StyleDiagnostic`, and shipped `.mesh` fixture assertions without adding a new parser architecture. [VERIFIED: .planning/phases/52-skia-shape-primitive-migration/52-CONTEXT.md]

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| `.mesh` CSS-like syntax parsing | Frontend compiler/component parser | API / Backend: none | `mesh-core-component` owns style parsing and lowers Lightning CSS rules into MESH `StyleBlock` data before runtime resolution. [VERIFIED: crates/core/ui/component/src/parser/styles.rs:23] |
| Theme token and CSS variable resolution | Frontend style resolver | Theme storage | `StyleResolver` owns token, embedded-token, and local CSS variable resolution against `mesh-core-theme`. [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:169] [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:1129] |
| Painter-relevant computed style | Frontend style/layout/render data | Painter backend consumes copied intent | `ComputedStyle` stores backend-neutral values, and `DisplayPaintStyle` copies painter-relevant fields into display-list data. [VERIFIED: crates/core/ui/elements/src/style/types.rs:121] [VERIFIED: crates/core/frontend/render/src/display_list.rs:197] |
| Unsupported property diagnostics | Frontend style resolver | Component parser for syntax errors | Unsupported supported-list misses are currently `StyleDiagnostic`; parser-level unsupported at-rules and keyframe/container forms are `ParseError`. [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:517] [VERIFIED: crates/core/ui/component/src/parser/styles.rs:62] |
| Skia execution | Painter backend only | Display list supplies backend-neutral commands/data | `skia_safe` imports are present in painter backend files, while `mesh-core-elements` style data and display-list style data use MESH structs. [VERIFIED: crates/core/frontend/render/src/surface/painter/backend.rs:3] [VERIFIED: crates/core/frontend/render/src/display_list.rs:197] |

## Project Constraints (from AGENTS.md)

No `AGENTS.md` file was found in `/home/kolby/projects/mesh` or its parents during this research session. [VERIFIED: `find .. -name AGENTS.md -print` returned no paths]

No `.codex/skills/` or `.agents/skills/` project skill directory was available; `.codex` is a file and `.agents` contains no skill subdirectories. [VERIFIED: `ls -la .codex .agents`]

## Standard Stack

### Core

| Library / Crate | Version | Purpose | Why Standard |
|-----------------|---------|---------|--------------|
| `mesh-core-component` | workspace `0.1.0` | Parses `.mesh` files and lowers CSS-like style blocks to MESH style structs. | It is the existing author-syntax boundary and already rejects unsupported at-rules/container query forms. [VERIFIED: crates/core/ui/component/Cargo.toml:1] [VERIFIED: crates/core/ui/component/src/parser/styles.rs:40] |
| `mesh-core-elements` | workspace `0.1.0` | Resolves style declarations, diagnostics, tokens, CSS variables, and computed style values. | It contains `StyleDiagnostic`, `SUPPORTED_CSS_PROPERTIES`, `ComputedStyle`, and `StyleResolver`, the integration points locked by context. [VERIFIED: crates/core/ui/elements/Cargo.toml:16] [VERIFIED: crates/core/ui/elements/src/style/types.rs:3] |
| `mesh-core-theme` | workspace `0.1.0` | Stores and loads token-based themes used by `StyleResolver`. | The phase explicitly preserves existing theme-token resolution through this crate. [VERIFIED: crates/core/foundation/theme/Cargo.toml:73] [VERIFIED: .planning/phases/52-skia-shape-primitive-migration/52-CONTEXT.md] |
| `mesh-core-render` | workspace `0.1.0` | Carries computed style into retained display-list and painter-facing data. | `DisplayPaintStyle` already carries opacity, shadow, filter, backdrop filter, text style, and icon axis fields without Skia types. [VERIFIED: crates/core/frontend/render/Cargo.toml:32] [VERIFIED: crates/core/frontend/render/src/display_list.rs:197] |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `lightningcss` | `1.0.0-alpha.71` | Parses CSS style sheets before MESH lowers accepted rule/property/value forms. | Keep using it for CSS syntax/tokenization; do not treat its full browser parser capability as MESH support. [VERIFIED: crates/core/ui/component/Cargo.toml:14] [CITED: https://docs.rs/lightningcss/latest/lightningcss/] |
| `cssparser` | direct `0.35.0`; transitive `0.33.0` | Tokenizes selectors during MESH selector lowering. | Keep using it for low-level selector tokens in `parse_selector`. [VERIFIED: crates/core/ui/component/Cargo.toml:13] [VERIFIED: Cargo.lock:368] [CITED: https://docs.rs/cssparser/latest/cssparser/] |
| `serde` / `serde_json` | `1.0.228` / `1.0.149` | Theme JSON loading and test fixture handling. | Use for shipped theme/profile fixture assertions. [VERIFIED: Cargo.lock:2436] [VERIFIED: Cargo.lock:2476] |
| `skia-safe` | `0.97.0` | Painter backend implementation only. | Do not introduce into style, computed style, render-object, or display-list retained data. [VERIFIED: crates/core/frontend/render/Cargo.toml:63] [VERIFIED: crates/core/frontend/render/src/surface/painter/backend.rs:3] |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Existing `StyleResolver` diagnostics | New style-profile validator pass | A separate pass risks duplicate truth and drift; resolver already sees selector context, variables, tokens, and computed output. [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:505] |
| Existing Lightning CSS parse path | Hand-written full CSS parser | The project already uses Lightning CSS for stylesheet parsing; hand-rolling would duplicate tokenization and at-rule parsing complexity. [VERIFIED: crates/core/ui/component/src/parser/styles.rs:23] [CITED: https://docs.rs/lightningcss/latest/lightningcss/] |
| Existing theme token system | New painter token registry | Context locks `mesh-core-theme` + `StyleResolver` as the token path; a second token path would contradict STYLE-02. [VERIFIED: .planning/phases/52-skia-shape-primitive-migration/52-CONTEXT.md] |

**Installation:** no new packages are recommended for Phase 52. [VERIFIED: Cargo.toml/Cargo.lock inspection]

**Version verification:** Rust versions were verified from `Cargo.toml`, `Cargo.lock`, and `cargo metadata`, not npm. [VERIFIED: crates/core/ui/component/Cargo.toml:13] [VERIFIED: Cargo.lock:1242]

## Architecture Patterns

### System Architecture Diagram

```text
.mesh source
  |
  v
mesh-core-component parse_style()
  |  - Lightning CSS parses stylesheet syntax
  |  - MESH lowers only accepted style rules, @container, @keyframes
  v
StyleBlock { rules, keyframes }
  |
  v
StyleResolver + Theme
  |  - resolve token(...) and StyleValue::Token
  |  - resolve local var(--x)
  |  - emit StyleDiagnostic for unsupported profile/property/token issues
  v
ComputedStyle (backend-neutral)
  |
  v
layout/render synchronization
  |
  v
DisplayPaintStyle / retained display-list data (backend-neutral)
  |
  v
Painter commands / Skia backend execution
```

### Recommended Project Structure

```text
docs/rendering/
└── style-profile.md            # Author-facing MESH style profile and support matrix

crates/core/ui/elements/src/style/
├── types.rs                    # supported property list/profile metadata
├── resolve.rs                  # diagnostics and computed lowering
└── parse.rs                    # value parsers for supported profile

crates/core/ui/elements/src/style.rs
└── tests                       # property matrix and shipped fixture resolver tests

crates/core/ui/component/src/parser/
└── styles.rs                   # parser-level syntax/profile errors
```

### Pattern 1: Support Matrix As Executable Contract

**What:** define each property once with classification: `implemented`, `diagnostic-only`, `deferred`, or `out-of-scope`; assert the matrix matches `supported_css_properties()` and resolver diagnostics. [VERIFIED: crates/core/ui/elements/src/style/types.rs:109]

**When to use:** use for all author-visible property decisions in Phase 52, especially values parsed by Lightning CSS but not lowered into `ComputedStyle`. [VERIFIED: crates/core/ui/component/src/parser/styles.rs:204]

**Example:**

```rust
// Source: existing style support API.
assert!(is_supported_css_property("background-color"));
assert!(!is_supported_css_property("grid-template-columns"));
```

[VERIFIED: crates/core/ui/elements/src/style.rs:57]

### Pattern 2: Resolver Diagnostics Own Semantic Style Warnings

**What:** syntax errors stay in `mesh-core-component`; profile/semantic warnings stay in `StyleResolver` as `StyleDiagnostic`. [VERIFIED: crates/core/ui/component/src/parser/styles.rs:269] [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:517]

**When to use:** use resolver diagnostics for unsupported properties, missing CSS variables, missing animation tokens, accepted-but-unlowered declarations, and ambiguous browser-like declarations that should not abort parsing. [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:525] [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:537]

**Example:**

```rust
let (_style, diagnostics) = resolver.resolve_node_style_with_diagnostics(
    &rules,
    "box",
    &["panel".to_string()],
    None,
    StyleContext::default(),
    ElementState::default(),
);
```

[VERIFIED: crates/core/ui/elements/src/style.rs:179]

### Pattern 3: Backend-Neutral Lowering Boundary

**What:** painter-relevant values lower to plain MESH structs (`Color`, `Edges`, `Corners`, `Transform2D`, `BoxShadow`, `VisualFilter`) and primitive values before backend execution. [VERIFIED: crates/core/ui/elements/src/style/types.rs:123]

**When to use:** use this boundary when adding profile tests for style-to-display-list compatibility; never add `skia_safe` to `mesh-core-elements` or retained display-list style data. [VERIFIED: crates/core/frontend/render/src/display_list.rs:197] [VERIFIED: crates/core/frontend/render/src/surface/painter/backend.rs:3]

**Example:**

```rust
pub struct DisplayPaintStyle {
    pub background_color: Color,
    pub border_color: Color,
    pub border_width: Edges,
    pub opacity: f32,
    pub box_shadow: BoxShadow,
    pub filter: VisualFilter,
}
```

[VERIFIED: crates/core/frontend/render/src/display_list.rs:197]

### Anti-Patterns to Avoid

- **Treating Lightning CSS parse success as MESH support:** Lightning CSS parses browser CSS broadly, but MESH must lower only the bounded shell profile. [CITED: https://docs.rs/lightningcss/latest/lightningcss/] [VERIFIED: .planning/REQUIREMENTS.md]
- **Leaving accepted properties unclassified:** `transform-origin` is in `SUPPORTED_CSS_PROPERTIES` but has no observed `apply_declaration` branch, so it must be documented as diagnostic-only/deferred or implemented deliberately. [VERIFIED: crates/core/ui/elements/src/style/types.rs:102] [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:676]
- **Adding Skia types above the painter backend:** existing `skia_safe` imports are in render surface/painter implementation files, not style structs. [VERIFIED: crates/core/frontend/render/src/surface/painter/backend.rs:3] [VERIFIED: crates/core/ui/elements/src/style/types.rs:121]
- **Breaking shipped `.mesh` compatibility while tightening diagnostics:** navigation fixtures include current unsupported browser-like declarations, so tests should assert expected diagnostics and continued token/style resolution rather than rejecting the whole file. [VERIFIED: modules/frontend/navigation-bar/src/main.mesh:121] [VERIFIED: modules/frontend/navigation-bar/src/components/volume-button.mesh:324]

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| CSS syntax parsing | A custom stylesheet parser | `lightningcss::StyleSheet::parse` plus MESH lowering | The current parser already uses Lightning CSS with `error_recovery: false` and explicit MESH lowering/error mapping. [VERIFIED: crates/core/ui/component/src/parser/styles.rs:23] [CITED: https://docs.rs/lightningcss/latest/lightningcss/stylesheet/struct.ParserOptions.html] |
| Selector tokenization | Ad hoc string splitting | `cssparser::Parser` in `parse_selector` | `cssparser` is already used to parse selector tokens and report unsupported selector tokens. [VERIFIED: crates/core/ui/component/src/parser/styles.rs:463] [CITED: https://docs.rs/cssparser/latest/cssparser/] |
| Token system | A painter-specific token resolver | `mesh-core-theme` + `StyleResolver` | Existing resolver handles `StyleValue::Token`, embedded `token(...)`, numbers, strings, booleans, and local vars. [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:169] |
| Painter backend values | Skia-specific retained style structs | MESH `ComputedStyle` and `DisplayPaintStyle` | Backend-neutral data is required by PAINT-02/STYLE-02 and already exists. [VERIFIED: .planning/REQUIREMENTS.md] [VERIFIED: crates/core/frontend/render/src/display_list.rs:197] |

**Key insight:** Phase 52 should make compatibility observable; it should not broaden the styling language. [VERIFIED: .planning/phases/52-skia-shape-primitive-migration/52-CONTEXT.md]

## Common Pitfalls

### Pitfall 1: Supported List Drift

**What goes wrong:** a property appears in `SUPPORTED_CSS_PROPERTIES`, so no unsupported-property diagnostic fires, but no computed style field changes. [VERIFIED: crates/core/ui/elements/src/style/types.rs:11] [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:676]

**Why it happens:** support is currently split between a string allowlist and a separate `apply_declaration` match. [VERIFIED: crates/core/ui/elements/src/style/types.rs:11] [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:676]

**How to avoid:** test every matrix `implemented` property by resolving a declaration and asserting a `ComputedStyle` or display-list field changed. [VERIFIED: crates/core/ui/elements/src/style.rs:294]

**Warning signs:** properties like `transform-origin`, `border-style`, and `text-wrap` appear in fixtures or support tables without corresponding computed fields. [VERIFIED: crates/core/ui/elements/src/style/types.rs:102] [VERIFIED: modules/frontend/navigation-bar/src/components/volume-button.mesh:324]

### Pitfall 2: Shipped Fixture Diagnostics Treated As Failures

**What goes wrong:** tightening unsupported property diagnostics can make navigation fixtures noisy or fail if tests expect zero diagnostics. [VERIFIED: modules/frontend/navigation-bar/src/main.mesh:121] [VERIFIED: modules/frontend/navigation-bar/src/main.mesh:155]

**Why it happens:** shipped styles currently include browser-like compatibility declarations (`container-type`, `text-wrap`, `border-style`) that are not in the current supported list. [VERIFIED: crates/core/ui/elements/src/style/types.rs:11] [VERIFIED: modules/frontend/navigation-bar/src/components/volume-button.mesh:324]

**How to avoid:** fixture tests should assert exact expected diagnostics plus resolved critical fields/tokens, not “no diagnostics” globally. [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:517]

**Warning signs:** adding `container-type` or `text-wrap` to `SUPPORTED_CSS_PROPERTIES` only to silence warnings without lowering behavior. [VERIFIED: crates/core/ui/elements/src/style/types.rs:11]

### Pitfall 3: Keyframe Support Mismatch

**What goes wrong:** component parser tests currently expect `filter` and `box-shadow` to be rejected in keyframes, while shared transition-safe property logic now accepts them. [VERIFIED: crates/core/ui/component/src/style.rs:158] [VERIFIED: `cargo test -p mesh-core-component parser -- --nocapture` failed 2 parser tests]

**Why it happens:** `mesh-core-component::style::is_transition_safe_keyframe_property` includes `box-shadow`, `filter`, and `backdrop-filter`, but parser tests still assert rejection of `filter`. [VERIFIED: crates/core/ui/component/src/style.rs:200] [VERIFIED: `cargo test -p mesh-core-component parser -- --nocapture`]

**How to avoid:** make the Phase 52 matrix choose one policy and update parser tests accordingly; given context says preserve existing support for shadow/filter/animation properties that already compile, the likely plan is to classify keyframe filter/shadow as accepted metadata/deferred render behavior rather than parser-invalid. [VERIFIED: .planning/phases/52-skia-shape-primitive-migration/52-CONTEXT.md]

**Warning signs:** `cargo test -p mesh-core-component parser` fails before implementation work starts. [VERIFIED: command output]

### Pitfall 4: Render Test Environment Linker Gap

**What goes wrong:** `mesh-core-render` painter tests fail at link time, unrelated to Rust code behavior. [VERIFIED: `cargo test -p mesh-core-render painter -- --nocapture` failed]

**Why it happens:** current environment cannot find `-lfreetype` and `-lfontconfig` while linking `skia-safe` render tests. [VERIFIED: command output]

**How to avoid:** Phase 52 validation should use `mesh-core-elements` and `mesh-core-component` tests as the fast gate, and reserve render tests for an environment with fontconfig/freetype available. [VERIFIED: command output]

**Warning signs:** linker output includes `rust-lld: error: unable to find library -lfreetype` and `-lfontconfig`. [VERIFIED: command output]

## Code Examples

### Unsupported Property Diagnostic

```rust
if !is_supported_css_property(&decl.property) {
    diagnostics.push(StyleDiagnostic {
        property: decl.property.clone(),
        selector,
        message: format!("unsupported CSS property '{}'", decl.property),
    });
    return;
}
```

[VERIFIED: crates/core/ui/elements/src/style/resolve.rs:517]

### Strict Animation Token Diagnostic

```rust
if is_strict_animation_property(&decl.property) {
    if let Err(token_name) = self.validate_animation_value_with_variables(&decl.value, variables) {
        diagnostics.push(StyleDiagnostic {
            property: decl.property.clone(),
            selector,
            message: format!("unresolved animation token reference '{token_name}'"),
        });
        return;
    }
}
```

[VERIFIED: crates/core/ui/elements/src/style/resolve.rs:537]

### Parser-Level Unsupported At-Rule Error

```rust
other => {
    return Err(ParseError::InvalidStyle {
        message: format!("unsupported at-rule '{}'", css_rule_name(other)),
        line: 0,
    });
}
```

[VERIFIED: crates/core/ui/component/src/parser/styles.rs:62]

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Browser-like CSS assumptions | Bounded MESH shell CSS profile | v1.10 roadmap/context | The plan must document and test MESH support, not browser compatibility. [VERIFIED: .planning/ROADMAP.md] |
| Direct style-to-backend thinking | Backend-neutral computed/display-list style, Skia only below painter backend | Phase 51/v1.10 decisions | Phase 52 must keep style/render retained data Skia-free. [VERIFIED: .planning/STATE.md] [VERIFIED: crates/core/frontend/render/src/surface/painter/backend.rs:3] |
| Parser-only style validation | Parser syntax validation plus resolver diagnostics | Current code | Unsupported property and token issues can be non-fatal diagnostics while parse errors remain for unsupported syntax forms. [VERIFIED: crates/core/ui/component/src/parser/styles.rs:269] [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:517] |

**Deprecated/outdated:**
- Treating `filter` keyframes as parser-invalid is outdated relative to the current shared transition-safe helper, because the helper accepts `filter` and `backdrop-filter`. [VERIFIED: crates/core/ui/component/src/style.rs:200] [VERIFIED: `cargo test -p mesh-core-component parser -- --nocapture`]
- Treating `skia_safe` as acceptable in style/render retained data contradicts current v1.10 decisions and observed module boundaries. [VERIFIED: .planning/STATE.md] [VERIFIED: crates/core/frontend/render/src/surface/painter/backend.rs:3]

## Assumptions Log

All claims in this research were verified against project files, commands, or cited official docs. No `[ASSUMED]` claims are present.

## Open Questions (RESOLVED)

1. **RESOLVED: Where should the author-facing style profile document live?**
   - What we know: context allows docs, render docs, or `mesh-core-elements` tests if traceability is preserved. [VERIFIED: .planning/phases/52-skia-shape-primitive-migration/52-CONTEXT.md]
   - What's unclear: the repo does not currently show a dedicated `docs/rendering/style-profile.md` file. [VERIFIED: `rg --files docs .planning crates | rg style-profile` found no path]
   - Recommendation: create `docs/rendering/style-profile.md` if a docs tree exists; otherwise create a compact markdown profile under `crates/core/frontend/render/README.md` or a new `crates/core/ui/elements/STYLE_PROFILE.md` and reference it from tests. [VERIFIED: crates/core/frontend/render/README.md exists]
   - Resolution: use the existing `docs/css-coverage.md` as the author-facing
     style profile, because it already documents MESH's CSS coverage and Plan
     52-01 rewrites it into the bounded painter style profile.

2. **RESOLVED: Should `container-type` be diagnostic-only or a supported no-op?**
   - What we know: shipped navigation uses `container-type: inline-size`, and parser already supports `@container` width/height conditions independently. [VERIFIED: modules/frontend/navigation-bar/src/main.mesh:121] [VERIFIED: crates/core/ui/component/src/parser/styles.rs:225]
   - What's unclear: there is no observed computed style field for `container-type`. [VERIFIED: crates/core/ui/elements/src/style/types.rs:121]
   - Recommendation: classify `container-type` as compatibility/diagnostic-only unless the planner adds explicit container-establishment semantics. [VERIFIED: current code inventory]
   - Resolution: classify `container-type` as diagnostic-only for Phase 52. Do
     not add no-op supported semantics or a computed style field in this phase.

3. **RESOLVED: Should descendant selectors be in-scope for diagnostics?**
   - What we know: `parse_selector` ignores whitespace and produces compound selector parts, so `.nav-button:hover .nav-button-glyph` is not modeled as a descendant relationship. [VERIFIED: crates/core/ui/component/src/parser/styles.rs:497] [VERIFIED: modules/frontend/navigation-bar/src/components/volume-button.mesh:369]
   - What's unclear: Phase 52 scope mentions property inventory more than selector-profile inventory. [VERIFIED: .planning/phases/52-skia-shape-primitive-migration/52-CONTEXT.md]
   - Recommendation: document descendant selectors as out-of-scope browser CSS and add a diagnostic only if it can be done without breaking existing fixture parsing. [VERIFIED: current parser behavior]
   - Resolution: document descendant selectors as out-of-scope browser CSS in
     the style profile. Add parser diagnostics only if implementation can do so
     without breaking existing fixture parsing; otherwise keep diagnostic work
     focused on properties and values.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|-------------|-----------|---------|----------|
| `cargo` | Rust tests and metadata | yes | `cargo 1.95.0 (f2d3ce0bd 2026-03-21)` | none needed [VERIFIED: command output] |
| `rustc` | Rust compilation | yes | `rustc 1.95.0 (59807616e 2026-04-14)` | none needed [VERIFIED: command output] |
| `jq` | metadata inspection | yes | `jq-1.8.1` | manual Cargo.toml/Cargo.lock inspection [VERIFIED: command output] |
| `fontconfig` / `freetype` link libraries | `mesh-core-render` tests via `skia-safe` | no for current linker path | unavailable to linker | use non-render tests for Phase 52 fast gate; run render tests in Nix graphics env [VERIFIED: command output] |

**Missing dependencies with no fallback:**
- None for Phase 52 style/parser/resolver tests. [VERIFIED: `cargo test -p mesh-core-elements style -- --nocapture` passed]

**Missing dependencies with fallback:**
- `fontconfig`/`freetype` link libraries block `mesh-core-render` painter tests in this shell; use targeted style/component tests for this phase and schedule render test proof in an environment that links Skia. [VERIFIED: command output]

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in `cargo test` with workspace crates. [VERIFIED: Cargo.toml/Cargo.lock] |
| Config file | root `Cargo.toml`; no separate nextest config found. [VERIFIED: `rg --files` test config scan] |
| Quick run command | `cargo test -p mesh-core-elements style -- --nocapture` |
| Component parser command | `cargo test -p mesh-core-component parser -- --nocapture` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|--------------|
| STYLE-01 | Style profile matrix matches supported/diagnostic/deferred/out-of-scope properties. | unit/docs consistency | `cargo test -p mesh-core-elements style_profile -- --nocapture` | no, Wave 0 |
| STYLE-02 | Tokens and shipped navigation/audio styles resolve through `StyleResolver` into backend-neutral `ComputedStyle`. | unit/fixture | `cargo test -p mesh-core-elements shipped_navigation_style -- --nocapture` | no, Wave 0 |
| STYLE-03 | Unsupported and ambiguous web-like properties emit `StyleDiagnostic`. | unit | `cargo test -p mesh-core-elements style_diagnostics -- --nocapture` | partial; unsupported property and variable diagnostics exist [VERIFIED: crates/core/ui/elements/src/style.rs:166] |

### Sampling Rate

- **Per task commit:** `cargo test -p mesh-core-elements style -- --nocapture` [VERIFIED: passed 35 tests]
- **Per wave merge:** `cargo test -p mesh-core-elements style -- --nocapture && cargo test -p mesh-core-component parser -- --nocapture` after resolving current parser expectation failures. [VERIFIED: command output]
- **Phase gate:** style/profile tests green; component parser expectations aligned; shipped `.mesh` fixture test asserts exact diagnostics and resolved token fields. [VERIFIED: current code inventory]

### Wave 0 Gaps

- [ ] Add a style profile matrix test file or colocated `style.rs` test for `supported_css_properties()` vs matrix classification. [VERIFIED: current tests cover support list but not classification]
- [ ] Add shipped navigation fixture parse/resolve tests covering `modules/frontend/navigation-bar/src/main.mesh` and key child components. [VERIFIED: shipped fixture paths exist]
- [ ] Fix or intentionally update stale `mesh-core-component` parser tests expecting `filter` keyframes to fail. [VERIFIED: `cargo test -p mesh-core-component parser -- --nocapture`]
- [ ] Document render test linker requirement for `fontconfig`/`freetype` before using `mesh-core-render` as a Phase 52 gate. [VERIFIED: `cargo test -p mesh-core-render painter -- --nocapture`]

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|------------------|
| V2 Authentication | no | This phase changes style parsing/resolution only; no authentication surface is in scope. [VERIFIED: .planning/REQUIREMENTS.md] |
| V3 Session Management | no | This phase changes style parsing/resolution only; no session state is in scope. [VERIFIED: .planning/REQUIREMENTS.md] |
| V4 Access Control | no | This phase changes style parsing/resolution only; no authorization checks are in scope. [VERIFIED: .planning/REQUIREMENTS.md] |
| V5 Input Validation | yes | Use Lightning CSS parser errors for syntax, MESH parser lowering errors for unsupported at-rules/container/keyframe forms, and `StyleDiagnostic` for non-fatal unsupported style profile issues. [VERIFIED: crates/core/ui/component/src/parser/styles.rs:23] [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:517] |
| V6 Cryptography | no | No cryptographic operations are in scope. [VERIFIED: .planning/ROADMAP.md] |

### Known Threat Patterns for Style Parsing

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Unbounded browser-CSS feature acceptance leading to invisible behavior gaps | Tampering / Repudiation | Explicit support matrix plus diagnostics for unsupported/ambiguous properties. [VERIFIED: .planning/REQUIREMENTS.md] |
| Invalid CSS silently ignored | Tampering | `error_recovery: false` for Lightning CSS parse and explicit `ParseError` mapping. [VERIFIED: crates/core/ui/component/src/parser/styles.rs:23] [CITED: https://docs.rs/lightningcss/latest/lightningcss/stylesheet/struct.ParserOptions.html] |
| Token resolution failure hiding visual behavior | Repudiation | Strict diagnostics for animation token failures and tests for token resolution. [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:204] |
| Backend-specific type leakage into retained style data | Information Disclosure / Tampering | Keep `skia_safe` imports confined to painter backend and use backend-neutral structs in style/display-list data. [VERIFIED: crates/core/frontend/render/src/surface/painter/backend.rs:3] [VERIFIED: crates/core/frontend/render/src/display_list.rs:197] |

## Sources

### Primary (HIGH confidence)

- `.planning/phases/52-skia-shape-primitive-migration/52-CONTEXT.md` - user decisions, locked scope, deferred work. [VERIFIED]
- `.planning/REQUIREMENTS.md` - STYLE-01, STYLE-02, STYLE-03 and v1.10 boundaries. [VERIFIED]
- `.planning/ROADMAP.md` - Phase 52 goal, autonomous task seed, success criteria. [VERIFIED]
- `crates/core/ui/elements/src/style/types.rs` - `StyleDiagnostic`, `SUPPORTED_CSS_PROPERTIES`, `ComputedStyle`, transition properties. [VERIFIED]
- `crates/core/ui/elements/src/style/resolve.rs` - token resolution, variables, diagnostics, declaration lowering. [VERIFIED]
- `crates/core/ui/component/src/parser/styles.rs` - Lightning CSS parse path, at-rule/container/keyframe/selector lowering. [VERIFIED]
- `crates/core/frontend/render/src/display_list.rs` - backend-neutral retained display-list paint style data. [VERIFIED]
- `modules/frontend/navigation-bar/src/main.mesh` and `modules/frontend/navigation-bar/src/components/volume-button.mesh` - shipped compatibility fixture styles. [VERIFIED]

### Secondary (MEDIUM confidence)

- Lightning CSS docs.rs, `lightningcss 1.0.0-alpha.71` - parser purpose and `ParserOptions::error_recovery`. [CITED: https://docs.rs/lightningcss/latest/lightningcss/] [CITED: https://docs.rs/lightningcss/latest/lightningcss/stylesheet/struct.ParserOptions.html]
- cssparser docs.rs - parser/token model. [CITED: https://docs.rs/cssparser/latest/cssparser/]
- Skia official docs - confirms Skia is a 2D graphics library, supporting boundary context only. [CITED: https://skia.org/docs/]

### Tertiary (LOW confidence)

- None.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - verified from Cargo manifests, Cargo.lock, and code paths. [VERIFIED: crates/core/ui/component/Cargo.toml:13] [VERIFIED: Cargo.lock:1242]
- Architecture: HIGH - verified from parser/resolver/display-list code and v1.10 planning decisions. [VERIFIED: crates/core/ui/component/src/parser/styles.rs:23] [VERIFIED: crates/core/ui/elements/src/style/resolve.rs:152]
- Pitfalls: HIGH - verified from current tests, command output, and shipped fixtures. [VERIFIED: command output] [VERIFIED: modules/frontend/navigation-bar/src/main.mesh:121]

**Research date:** 2026-05-22  
**Valid until:** 2026-06-21 for codebase-specific findings; re-check external crate versions before dependency changes. [VERIFIED: current date]
