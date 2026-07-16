# UI XML/CSS Transition Sketch

This note sketches how to move `.mesh` frontend modules toward:

- a UI-focused XML vocabulary instead of HTML
- a bounded CSS subset for styling
- a deliberately bounded UI/CSS runtime profile

The intent is not "run the web platform inside MESH". The intent is closer to
Qt/QML and declarative UI toolkits:

1. parse author-friendly UI XML and CSS,
2. lower them into a typed MESH UI IR,
3. keep layout, reactivity, and rendering fast enough for shell surfaces.

## Current implementation status

This file started as a sketch. The current codebase has since implemented some
of the transition, while other parts are still only design direction.

Functional now:

- `.mesh` SFC parsing still happens in `crates/core/ui/component/src/parser.rs`.
- `SourceTag` exists in `crates/core/ui/component/src/template.rs` and preserves
  source-level tag intent.
- `UiTag` exists in `crates/core/frontend/compiler/src/tags.rs`.
- Source tags are lowered through `lower_source_tag()` before `WidgetNode`
  construction in `crates/core/frontend/compiler/src/render.rs`.
- Built-in template primitives are lowercase.
- PascalCase references are reserved for explicitly imported custom components.
- CSS is parsed with `lightningcss`.
- Supported selector shapes include tag, class, id, universal, compound, simple
  pseudo-state selectors, selector lists, and bounded container queries.
- Runtime state styling works for hover/focus/active/disabled/checked where the
  state is populated.
- `:focus-visible` is distinct from plain `:focus` and follows the shell's
  input-modality tracking.
- Transition parsing and interpolation now cover practical shell properties:
  color, background, border color/width/radius, opacity, sizing, spacing, and
  transform metadata.
- `transform: translate(...)` is painted and hit-tested; the transform data
  model also carries scale/rotate for future paint paths.
- Shell focus traversal is no longer pointer-only: tabbable controls participate
  in visual-order keyboard traversal and receive focused `keydown` / `keyup`
  events.
- Read-only selectable text exists as an explicit opt-in on `text` nodes via
  `selectable="true"`.
- Runtime layout/rendering still operates on a small primitive tag set.

Partially functional:

- Unsupported CSS is rejected by parser errors, but diagnostics are not yet a
  first-class compile product with source spans and migration help.
- `LoweredSelector`, `SimpleSelector`, and `StateSelector` exist as data-model
  direction, but `StyleResolver` still matches the source `Selector` form.
- The lowering boundary exists for tags, but there is not yet a complete
  `LoweredFrontend` IR with lowered templates, lowered styles, and diagnostics.
- `image`, `list`, `list-item`, `separator`, and `spacer` lower to existing
  primitives; they do not yet have distinct runtime/layout/render behavior.
- `switch` and `checkbox` lower to an input-like primitive, but checked-state
  interaction and native toggle painting are not complete.
- Input controls support text-like editing. `password-input` masks its display,
  and `number-input` filters typed characters lightly, but there is no cursor
  movement, selection, validation, or form semantics.
- `scale(...)` and `rotate(...)` parse and animate through style state, but the
  current painter still treats them as identity.

Not functional yet:

- Full HTML semantics are intentionally not supported.
- Descendant, child, sibling, attribute, structural, relational, and
  pseudo-element selectors are not supported.
- Browser layout models such as grid and browser formatting contexts are not
  supported.
- Full CSS cascade/custom property semantics are not supported.
- Keyframes, filter effects, full transform painting, and browser-style
  animation timelines are not supported.

## Current pipeline in this codebase

Today the path is already close to a compiler pipeline:

```text
.mesh SFC source
  -> crates/core/ui/component/src/parser.rs::parse_component
  -> ComponentFile { template, script, style, ... }
  -> crates/core/frontend/compiler/src/compile.rs::compile_frontend_module
  -> crates/core/frontend/compiler/src/render.rs::build_widget_tree_from_component
  -> WidgetNode tree / CompiledFrontendModule
  -> mesh-core-elements StyleResolver / LayoutEngine
  -> mesh-core-shell component runtime and retained dirty tracking
  -> mesh-core-render software painter
  -> mesh-core-presentation dev-window or layer-shell backend
```

The main transition pressure points are:

- `crates/core/ui/component/src/template.rs`
  Current template AST stores raw tag strings and generic attributes.
- `crates/core/ui/component/src/style.rs`
  Current style AST is intentionally small: simple selectors, declarations, container queries.
- `crates/core/frontend/compiler/src/compile.rs`
  Compiles frontend modules and resolves local component imports.
- `crates/core/frontend/compiler/src/render.rs`
  Builds widget trees from compiled component source.
- `crates/core/frontend/compiler/src/tags.rs`
  Lowers source-level `SourceTag` values to runtime `UiTag` primitives.
- `crates/core/ui/elements/src/style.rs`
  Style resolution is fast because selector matching is shallow and property support is bounded.
- `crates/core/ui/elements/src/layout.rs`
  Layout is a flexbox-like subset, not browser layout.
- `crates/core/frontend/render/src/surface/painter.rs`
  Rendering is by UI primitive tag, not by browser semantics.
- `crates/core/presentation/src/lib.rs`
  Selects the dev-window or layer-shell backend and commits rendered
  `PixelBuffer`s.

## What should change conceptually

The key shift is:

```text
UI XML/CSS source syntax
  -> parsed XML/CSS AST
  -> validated MESH UI/CSS profile
  -> lowered UI IR
  -> runtime WidgetNode / layout / render tree
```

That means we should stop treating HTML as the authoring model too.
The authoring model should become a MESH-native UI vocabulary.

Runtime should continue to operate on a smaller set of MESH primitives.

## Recommended target architecture

Use three layers instead of one:

### 1. Source AST

Keep a source-faithful representation of what the module author wrote.

- UI tags stay intact: `panel`, `row`, `column`, `text`, `icon`, `button`
- CSS is parsed with a real parser, including syntax we may later reject
- source spans are preserved for diagnostics

Suggested additions:

- `template.rs`
  Add a `UiElementNode` model or extend `ElementNode` with semantic tag metadata.
- `style.rs`
  Split "parsed CSS" from "supported CSS".

### 2. Validated MESH profile

Add a compile-time validation/lowering phase that accepts only the CSS and UI
semantics that MESH can afford.

This phase should:

- map author tags into a smaller semantic set
- reject unsupported selectors/properties with actionable diagnostics
- precompute selector specificity and rule applicability metadata
- flag costly features before runtime

Suggested new concepts:

- `SourceTag`
  Source-level UI tag enum or interned identifier
- `UiTag`
  Runtime primitive tag enum
- `MeshCssProfile`
  The supported selector/property/value surface
- `CompileDiagnostic`
  Warning/error with source span and migration help

### 3. UI IR / runtime tree

Keep runtime nodes small and predictable.

- `WidgetNode` should still carry MESH primitives, resolved attributes, computed style, event handlers, and accessibility info
- layout remains MESH-owned
- rendering remains MESH-owned

This preserves performance and avoids turning shell rendering into browser emulation.

## UI XML transition

### What to keep

- SFC outer blocks in `parse_component`
- quick template parsing model
- control-flow preprocessing for `{#if}` / `{#for}`
- component references for PascalCase tags

### What to change

The current system parses many HTML tags but normalizes them very late in
`crates/core/frontend/compiler/src/lib.rs`. That makes the runtime path easy, but it
mixes author syntax with runtime semantics.

Move tag lowering into an explicit phase:

```text
template source
  -> parsed UI XML template AST
  -> semantic UI tag classification
  -> UiTag lowering
  -> WidgetNode construction
```

Suggested source vocabulary:

- `panel`
- `row`
- `column`
- `text`
- `label`
- `button`
- `input`
- `text-input`
- `password-input`
- `search-input`
- `number-input`
- `email-input`
- `url-input`
- `slider`
- `switch`
- `checkbox`
- `icon`
- `image`
- `scroll-view`
- `list`
- `list-item`
- `slot`
- `spacer`
- `separator`
- `surface`
- `widget`

Built-in tags are not the only tags the language should allow.
Frontend modules should also be able to introduce custom tags by exporting a
component tag from their manifest.

Suggested lowered `UiTag` set:

- `Container`
- `Text`
- `Button`
- `InputText`
- `InputRange`
- `Toggle`
- `Icon`
- `Image`
- `ScrollArea`
- `List`
- `ListItem`
- `Separator`
- `Spacer`
- `SurfacePortal`
- `Slot`

Suggested source-to-UI mapping:

- `panel`, `row`, `column` -> `Container`
- `text`, `label` -> `Text`
- `button` -> `Button`
- `input type="text"` -> `InputText`
- `text-input` or `input type="text"` -> `InputText`
- `password-input` or `input type="password"` -> `InputText` with masked display
- `search-input` or `input type="search"` -> `InputText`
- `number-input` or `input type="number"` -> `InputText` with numeric editing rules
- `email-input` or `input type="email"` -> `InputText`
- `url-input` or `input type="url"` -> `InputText`
- `slider` or `input type="range"` -> `InputRange`
- `switch`, `checkbox` -> `Toggle`
- `icon` -> `Icon`
- `image` -> `Image`
- `scroll-view` -> `ScrollArea`
- `list` -> `List`
- `list-item` -> `ListItem`
- `spacer` -> `Spacer`
- `separator` -> `Separator`

Important: the tag should not directly imply browser layout behavior.
For example:

- `row` does mean primary horizontal layout intent, but not full web flexbox semantics
- `column` does mean primary vertical layout intent
- `panel` means a generic styled container, not a browser block element
- `list` does not imply browser bullets unless we add them explicitly

This gives us semantic authoring without inheriting browser baggage.

### Input control status and target

Inputs should stay shell-oriented rather than growing into browser forms.

Functional now:

- `input type="text"` for basic text editing
- `text-input` as a semantic text input tag
- `password-input` as a semantic password input tag with masked display
- `search-input` as a semantic search input tag
- `number-input` as a semantic number input tag with light character filtering
- `email-input` and `url-input` as semantic text-like tags
- `slider` as the current range control

Still needed:

- cursor movement and selection for text-like inputs
- submit/change/input events with consistent payloads
- validation states for email/url/number without browser form semantics
- `checkbox` and `switch` checked-state storage, interaction, and painting
- optional step/min/max enforcement for `number-input`
- a future date/time story only if shell UI needs it

### Custom exported tags

The UI XML vocabulary should be open for composition.

If a module defines a reusable component, it should be able to export a custom
tag through canonical `module.json` contribution metadata, and dependent
modules should be able to use that tag directly in template markup. Legacy
`exports.component.tag` examples are migration-era wording; new docs should
prefer `mesh.contributes` in `module.json`.

Example:

```json
{
  "mesh": {
    "kind": "frontend",
    "contributes": {
      "layout": [
        {
          "id": "battery-widget",
          "entrypoint": "src/main.mesh",
          "label": "Battery Widget"
        }
      ]
    }
  }
}
```

Then another frontend module that declares the dependency can write:

```xml
<template>
  <panel>
    <BatteryWidget percent="{percent}" />
  </panel>
</template>
```

That behavior already fits the current composition path in this repo:

- manifest helper: `crates/core/extension/module/src/manifest.rs`
- compile-time reference collection: `crates/core/frontend/compiler/src/lib.rs`
- dependency/tag resolution: `crates/core/shell/src/shell/component.rs`

So the planned UI XML system should preserve this and document it as a
first-class feature, not an incidental implementation detail.

### Example target syntax

```xml
<template>
  <panel class="panel-root">
    <row gap="8" align="center">
      <icon name="battery" />
      <text>Battery {percent}%</text>
    </row>
  </panel>
</template>
```

This is a better fit for the current engine than:

```xml
<template>
  <box class="panel-root">
    <text class="icon">battery</text>
    <text>Battery {percent}%</text>
  </box>
</template>
```

## CSS transition

### The right strategy

Parse more CSS than we execute.

That means:

- use `lightningcss` as the source parser
- lower into a MESH CSS subset
- reject unsupported or high-cost features at compile time

This is better than inventing a CSS parser, and better than pretending full CSS works.

### Recommended supported subset

Keep the subset aligned with `mesh-core-elements` and shell performance needs.

Selectors to support:

- type/tag selectors
- class selectors
- id selectors
- compound selectors like `Button.primary`
- state selectors backed by real runtime state: `:hover`, `:focus`, `:active`, `:disabled`, `:checked`
- modality-aware visible focus through `:focus-visible`
- selector lists
- container queries

Selectors to reject for now:

- descendant combinators
- child combinators
- sibling combinators
- attribute selectors
- pseudo-elements
- structural selectors like `:first-child`, `:nth-child`
- relational selectors like `:has()`

Properties to keep leaning on:

- sizing
- spacing
- border radius / border color / border width
- background color
- text styling
- flex layout subset
- overflow
- opacity
- transitions for the practical visual properties above
- bounded transforms, with translate painted today and scale/rotate staged in
  the style pipeline

Properties to reject until there is a compelling shell use-case:

- grid
- arbitrary positioning beyond the current bounded model
- filters
- shadows, if they require expensive blur paths
- animations / keyframes
- custom properties with browser-like cascading semantics

### Why this subset is a good boundary

It avoids the most expensive classes of work:

- ancestor/sibling selector matching
- subtree-sensitive selectors
- browser formatting contexts
- animation timelines and compositor-like invalidation logic
- full cascade edge cases

## Concrete codebase plan

### Phase 0: make the architecture explicit

Add explicit types for "source syntax" vs "runtime primitive".

Suggested changes:

- `crates/core/ui/component/src/template.rs`
  Add `SourceTag` and preserve the literal source tag.
- `crates/core/ui/component/src/style.rs`
  Add a richer parsed selector/value model, separate from the supported runtime subset.
- `crates/core/frontend/compiler/src/lib.rs`
  Replace ad-hoc `normalize_tag()` with a named lowering step.

### Phase 1: add a compile/lower stage

Introduce a frontend compiler module in `mesh-core-frontend` that produces:

- lowered template IR
- lowered style IR
- diagnostics

Suggested API shape:

```rust
pub struct LoweredFrontend {
    pub template: LoweredTemplate,
    pub styles: LoweredStyleSheet,
    pub diagnostics: Vec<FrontendDiagnostic>,
}
```

This phase should be called from `compile_frontend_module()`, not during every frame.

### Phase 2: precompile selectors

Do not keep matching arbitrary source selectors at runtime.

Instead, lower supported selectors into a cheap matcher shape such as:

- optional tag key
- optional id key
- small class set
- optional runtime state mask
- optional container query bounds

For example:

```text
button.primary:hover
  -> { tag=button, classes=[primary], state=hover }
```

This keeps `mesh-core-elements::StyleResolver` fast and predictable.

### Phase 3: move tag lowering earlier

Today `build_element_node()` and `normalize_tag()` still decide runtime form while building `WidgetNode`.

Change that to:

- lower source element -> semantic UI element first
- build `WidgetNode` from lowered UI element

That will make rendering/layout code simpler and make diagnostics more accurate.

### Phase 4: make diagnostics first-class

When a module writes unsupported CSS, fail clearly.

Examples:

- "Descendant selectors are parsed but not supported in MESH surfaces"
- "`display: grid` is not supported; use flex containers"
- "`box-shadow` is intentionally unavailable in the shell CSS profile"

This should live near `compile_frontend_module()` so module authors get feedback at load/compile time.

### Phase 5: migrate shipped modules

Update core frontend modules so they author in the new source model while staying inside the supported profile.

That means:

- replace HTML-like tags with MESH UI tags
- avoid selectors that require tree walking
- prefer classes over structural selectors
- move layout intent into the tag set where it is stable and cheap

## Suggested source tag taxonomy

If you want the syntax to feel more like Qt than the web, the tags should encode
UI intent directly.

Recommended first-pass tag families:

- Layout: `panel`, `row`, `column`, `stack`, `scroll-view`, `spacer`, `separator`
- Content: `text`, `label`, `icon`, `image`
- Controls: `button`, `icon-button`, `input`, `text-input`, `password-input`,
  `search-input`, `number-input`, `email-input`, `url-input`, `slider`, `switch`,
  `checkbox`
- Structure: `list`, `list-item`, `slot`
- Composition: `surface`, `widget`, module-exported custom tags like `BatteryWidget`

That set is small enough to optimize well and expressive enough for shell UI.

## Suggested data model changes

These are the highest-value type changes.

### In `mesh-core-component`

Add source fidelity:

```rust
pub struct ElementNode {
    pub source_tag: String,
    pub tag_kind: SourceTag,
    pub attributes: Vec<Attribute>,
    pub children: Vec<TemplateNode>,
}
```

Add supported/lowered selector types:

```rust
pub enum ParsedSelector { ... }
pub enum LoweredSelector {
    Simple(SimpleSelector),
    State(SimpleSelector, StateSelector),
}
```

### In `mesh-core-frontend`

Add a lowering boundary:

```rust
pub enum UiTag {
    Container,
    Text,
    Button,
    InputText,
    InputRange,
    Toggle,
    Icon,
    Image,
    ScrollArea,
    List,
    ListItem,
    Separator,
    Spacer,
}
```

Then build `WidgetNode` from `UiTag` rather than from raw source tags.

### In `mesh-core-elements`

Prefer runtime enums over strings where possible.

Today `WidgetNode.tag` is a `String`. Long-term it should become an enum if we want faster dispatch and fewer ad-hoc string checks in:

- `layout.rs`
- `events.rs`
- `render/painter.rs`

## Performance rules for the CSS subset

If we want HTML/CSS authoring without browser costs, the profile should follow a few hard rules:

1. No selector that requires walking ancestors or siblings during normal matching.
2. No selector that requires inspecting arbitrary descendants.
3. No layout model besides the MESH-owned flex/subtree model unless we deliberately add one.
4. No paint features that require expensive blur/filter passes by default.
5. No dynamic cascade features that force broad invalidation.

That profile still gives module authors familiar syntax while keeping the shell responsive.

## Short version

The transition should not be:

```text
.mesh -> webby markup -> full CSS runtime
```

It should be:

```text
.mesh SFC
  -> UI XML/CSS source parser
  -> MESH UI profile validator/lowerer
  -> typed UI IR
  -> WidgetNode
  -> layout tree
  -> render tree
```

In this repo, the best place to introduce that boundary is between:

- `crates/core/ui/component` as source parser
- `crates/core/frontend/compiler` as compiler/lowerer
- `crates/core/frontend/render` as painter

That keeps `mesh-core-elements` and `mesh-core-shell` small, predictable, and fast.
