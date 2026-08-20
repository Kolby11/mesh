# Section 8 — UI element core: improvement audit

**Audited:** 2026-08-19  
**Scope:** `mesh-core-elements`: the `WidgetNode` tree and identity model,
element contracts and attributes, input/event state, CSS-like style matching
and resolution, Taffy and retained layout, text measurement, popover metadata,
and accessibility projection.

This is an audit record, not a second task tracker. Open work belongs in
[`docs/BACKLOG.md`](../../../../docs/BACKLOG.md). No production code was
changed for this review.

## Logical instruction/process tree

The current section-8 process can be read as this ordered pipeline. Each stage
has a distinct input/output contract, but several of those contracts are
currently implicit rather than validated.

```text
.mesh source / runtime input
  │
  ├─ compiler lowering
  │    ├─ source tag → runtime tag
  │    ├─ attributes, bindings, handlers, composition values
  │    ├─ initial computed style
  │    ├─ local accessibility metadata
  │    └─ recursively constructed WidgetNode tree
  │
  ├─ element contract/type lookup
  │    ├─ ElementContractDef: attrs, events, states, style hooks
  │    └─ ElementTypeDef: kind, fields, runtime behavior
  │
  ├─ shell/runtime finalization
  │    ├─ stable runtime IDs and mesh keys
  │    ├─ focus, hover, active, checked, scroll and window state
  │    ├─ hidden/popover runtime constraints
  │    └─ retained-tree change classification
  │
  ├─ interaction input
  │    └─ RawInputEvent → InputState::process or EventDispatcher::dispatch
  │         ├─ hit testing, focus, pointer press/capture, keyboard target
  │         └─ UiEvent records and ElementState changes
  │
  ├─ style invalidation/resolution
  │    ├─ state bit index and selector matching
  │    ├─ theme/module/default/inline cascade
  │    ├─ inherited values and custom-property variables
  │    └─ ComputedStyle per node
  │
  ├─ layout lowering and retained computation
  │    ├─ WidgetNode → Taffy tree + NodeId/TaffyId map
  │    ├─ dirty synchronization and text measurement
  │    ├─ intrinsic measurement cache
  │    └─ geometry write-back / retained validity
  │
  ├─ surface promotion
  │    └─ popover anchor, gravity, offset, grab, and constraints
  │
  └─ semantic projection
       ├─ role, accessible name/description, states, focus metadata
       ├─ visibility and hidden filtering
       ├─ parent/child and cross-surface relationships
       └─ AccessibilityTree / AccessKit snapshot

downstream consumers: renderer/display list, shell interaction, Wayland
child-surface promotion, automation, and accessibility clients
```

### Required invariants

1. Every live node has a unique identity for the lifetime of its tree. A
   cloned or moved authored subtree must either preserve explicitly defined
   identity semantics or receive fresh runtime identities before maps are built.
2. The source tag, runtime tag, contract, type definition, attributes, events,
   pseudo-states, and accessibility role use one consistent vocabulary.
3. A frame observes one coherent interaction/style/layout/semantic snapshot;
   state changes invalidate every dependent consumer before it reads the data.
4. A failed layout or measurement pass leaves a clearly invalid state or a
   last-known-good snapshot, never zero/stale geometry advertised as valid.
5. Accessibility is derived from the same live state used for routing and
   honors hidden/display and ARIA semantics.

## Verification

- `nix develop -c cargo check -p mesh-core-elements` passed.
- `git diff --check` passed.
- `nix develop -c cargo test -p mesh-core-elements --lib` ran 270 tests:
  205 passed and 57 were ignored; 8 failed in the existing style/theme
  baseline fixtures. Those failures resolve the active `tokyo-night` theme
  while assertions expect `mesh-default-dark` values; no production code was
  changed by this audit.
- The requested Luna xhigh flow mapper, logical/order reviewer, direct code
  reviewer, and additional cross-boundary seam reviewer were launched. Their
  returned reports were used below; no worker edited files.

## Confirmed findings

### 1. P1 — accessibility metadata and publication do not implement the full contract

The compiler accessibility builder reads only a subset of metadata
(`crates/core/frontend/compiler/src/render/elements.rs:436-518`). It does not
apply `role`/`aria-role`, `aria-hidden`, `aria-description`, or ARIA aliases
such as `aria-disabled`, `aria-checked`, `aria-expanded`, and `aria-pressed`.
Those attributes are accepted/interned
(`crates/core/ui/elements/src/attributes.rs:722-728`) but are not normalized
into the semantic projection. `AccessibilityTree::from_widget_tree` includes
nodes without hidden/display filtering (`accessibility.rs:101-123`). The
AccessKit adapter is also feature-gated and its tree publication is not wired
from the shell’s normal render path, despite the shipped specification status
(`crates/core/frontend/render/src/accesskit_adapter.rs:1-115`,
`docs/spec/09-accessibility.md:16-26`).

**Improve it:** add one post-child semantic-normalization phase that resolves
roles, names, descriptions, hidden state, ARIA aliases, focus, relationships,
and visibility. Publish that snapshot explicitly after layout, and add tests
for direct core consumers and the AccessKit handoff.

### 2. P1 — visible descendant text is not reliably used as an accessible name

`build_element_node` computes accessibility before children are lowered
(`crates/core/frontend/compiler/src/render/mod.rs:814-832`). The adapter
fallback uses explicit label/ARIA label or the node’s own content
(`crates/core/frontend/render/src/accesskit_adapter.rs:127-133`), not visible
descendant text. A button containing a text child can therefore have no name
unless the author duplicates the text into an explicit label.

**Improve it:** normalize semantics after child construction and implement the
documented precedence, including visible descendant text. Test nested text,
icon-only controls, hidden children, and locale changes.

### 3. P1 — declared pseudo-states are indexed but several never match

`state_name_bit` recognizes `readonly`, `required`, `selected`, `expanded`,
`pressed`, `invalid`, and `value` (`style/resolve/state.rs:3-22,70-101`), but
`selector_matches_attrs` only evaluates hover/focus/active/disabled/checked,
focus-visible, and window states (`style/resolve/matching.rs:40-61`). Runtime
annotation also initializes fewer states than the vocabulary advertises
(`crates/core/shell/src/shell/component/runtime_tree/annotate.rs:199-305`).
Rules such as `input:required`, `tab:selected`, or `button:pressed` can be
indexed yet never apply.

**Improve it:** generate one typed pseudo-state table for indexing, mutation,
matching, shell invalidation, diagnostics, and tests. Implement or explicitly
reject each state for each applicable element, then test the full matrix.

### 4. P1 — compiler inheritance matching ignores pseudo-state truth

The compiler-side inherited-style matcher does not use the actual state when
deciding whether a selector contributes to the inherited mask
(`crates/core/frontend/compiler/src/style.rs:312-333`, used from
`render/mod.rs:792-822`). A rule such as `button:hover { color: red }` can mark
the property as explicitly set even when the node is not hovered, preventing a
child from inheriting its parent’s color.

**Improve it:** pass real `ElementState` into that calculation or reuse the
elements resolver’s matcher, and test state-true/state-false inheritance.

### 5. P1 — incremental Taffy failure leaves retained state valid

When incremental `compute_layout_with_measure` fails, the code logs and zeros
the subtree (`layout/mod.rs:400-501`) but does not clear retained validity. A
same-size, non-dirty frame can then take the valid fast path
(`layout/mod.rs:449-457`) and reuse failed geometry indefinitely. The fresh
retained path clears validity on failure (`layout/retained.rs:20-59`), so the
two paths disagree.

**Improve it:** make layout transactional, preserve last-known-good geometry,
mark the state invalid on failure, and retry next frame. Add an injected
failure/recovery test without changing available size.

### 6. P1 — event routes disagree, and pointer activation lacks capture semantics

`InputState::process` targets keyboard events at its tracked focus and mutates
state (`events.rs:202-221`), while `EventDispatcher::dispatch` sends keyboard
events to the root and does not perform the same state transitions
(`events.rs:312-380`). In `InputState`, release dispatches to the current
hit-test node rather than the press origin (`events.rs:163-196`), while
`PointerUp` is mapped to `click` (`events.rs:51-85`). Pressing A and releasing
over B can therefore activate B or produce a different result from the shell
interaction path, which already validates the press target.

**Improve it:** make one stateful dispatcher canonical; add pointer capture,
pressed-target identity, explicit raw-up versus activation semantics, focus
eligibility, and changed-node/invalidation output. Add parity tests for both
public APIs, drag-off/drag-back, removed origins, keyboard focus, and disabled
targets.

### 7. P1 — element contract, runtime type, compiler event, and source-tag tables drift

`element_type_for_tag` silently falls back to the first type definition
(`crates/core/ui/elements/src/element/mod.rs:270-275`). The type registry and
contract registry do not cover exactly the same vocabulary
(`element/contracts.rs:491-518,557-735,857-906`); several source/runtime tags
such as `slider`, `scroll`, `scroll-view`, and `label` follow different paths.
Missing contracts cause validation to return no diagnostic
(`element/validate.rs:3-20,225-227`), while the common contract macro assigns
too many attributes/events to every element (`element/contracts.rs:472-489`).
The compiler also accepts a wider event family than the contract validator
declares (`frontend/compiler/src/render/elements.rs:403` versus
`element/validate.rs:225+`). `UiTag::Image::as_str()` returning `"icon"`
(`frontend/compiler/src/tags.rs:27-47`) is a concrete tag-lowering mismatch.

**Improve it:** generate runtime types, source/runtime tag mapping, contracts,
events, attributes, style hooks, and accessibility defaults from one canonical
schema. Unknown tags should return diagnostics, never `Box`; separate source
and runtime tag fields; and add an exhaustive registry/event matrix including
image lowering.

### 8. P1 — CSS custom-property cycles have no reliable guard

Custom properties are stored in `style/resolve/declaration.rs:202-205`, while
the main computed-value resolution paths recurse through `value.rs:35-57,
137-188,210-239` without the depth/cycle protection present in a simpler string
resolver. A cycle such as `--a: var(--b); --b: var(--a); color: var(--a)` can
therefore recurse indefinitely or fail non-diagnostically.

**Improve it:** track a dependency stack/visited set per resolution, emit a
structured cycle diagnostic, and apply the specified invalid-value fallback.
Test direct, indirect, and fallback cycles.

### 9. P2 — retained style inheritance confuses explicit defaults with inherited values

The retained resolver infers declaration explicitness from resolved values
(`style/resolve/matching.rs:76-96`). Explicit `transparent`, default font size,
font weight, or line height can therefore be mistaken for “unset” and replaced
by a parent value. The initial compiler path already has an explicit
declaration mask (`frontend/compiler/src/style.rs:172-192`), but retained
restyle does not preserve it.

**Improve it:** carry explicit/inherited property masks through resolution and
targeted restyle instead of comparing values with defaults. Add transparent,
default-size, and parent-different regression fixtures.

### 10. P2 — node focus and accessibility focus are separate mutable facts

Input handling updates `node.state.focused` but not
`node.accessibility.focused` (`events.rs:163-173,254-267`). The shell later
copies state into accessibility metadata
(`shell/component/runtime_tree/annotate.rs:255-260`), so direct core snapshots
can expose stale focus.

**Improve it:** derive semantic focus from canonical live state during snapshot
generation and keep shell annotation only as a compatibility adapter.

### 11. P2 — text measurement and cache keys omit shaping-affecting inputs

`TextMeasureData`, `TextMeasureKey`, and `TextMeasurer` omit letter spacing,
font style, text direction, language/shaping features, and resource/measurer
generation (`layout/lowering.rs:3-26`, `layout/mod.rs:28-39,115-135`,
`style/types.rs:663-674`). `nowrap` is used by measurement but not represented
in every cache identity. Font reload or a same-key style change can therefore
reuse stale intrinsic geometry; some parsed properties are not rendered or
measured end to end.

**Improve it:** pass a structured complete text-metrics context, include a
font/catalog/measurer revision in cache keys, invalidate on resource changes,
and add RTL/italic/letter-spacing/nowrap/reload tests.

### 12. P2 — popover errors and cross-surface semantics are under-specified

`popover.rs:96-232` silently falls back or ignores unknown anchor, gravity,
grab, offset, and constraint values. Placement contains no trigger/promoted
surface relationship, while accessibility metadata only carries ordinary
parent/child IDs (`popover.rs:84`, `accessibility.rs:85-93`). This prevents
reliable diagnostics and continuity of semantics across an `xdg_popup`
promotion.

**Improve it:** return typed placement diagnostics, reject invalid tokens,
store trigger/surface identity and relationship metadata, and test promotion,
dismissal, focus return, and accessibility continuity.

## Broader feature and architecture opportunities

The strongest improvement beyond the current flow is a typed frame
transaction/snapshot shared by interaction, style, layout, and accessibility:

```text
InputTransaction
  → StateDelta { focus, capture, pseudo-state, changed NodeIds }
  → StyleInvalidation { affected subtrees, revision }
  → LayoutTransaction { retained candidate, measure generation }
  → SemanticSnapshot { normalized ARIA, visibility, relationships }
  → immutable FrameSnapshot for renderer, shell, and AccessKit
```

Pair it with separate authored/source identity, stable runtime identity, and
retained backend handles; generated element schemas; phase stamps such as
`TreeBuilt → StateAnnotated → Styled → LaidOut → SemanticsReady`; semantic
diff events; and property-based tests for duplicate IDs, state matrices,
reordering, hidden transitions, non-finite numeric values, and cache
invalidation.

## Recommended implementation order

1. Establish tree identity and reject unknown/duplicate element definitions.
2. Unify source/runtime tags, contracts, event names, and pseudo-state tables.
3. Consolidate event routing around focus/capture and explicit invalidation.
4. Make retained layout transactional and complete text measurement generations.
5. Normalize ARIA, visibility, names, relationships, and focus after children.
6. Add CSS cycle diagnostics, popover diagnostics/relationships, and revision
   identities.
7. Introduce the immutable frame snapshot so these are one coherent contract.

## Regression matrix

| Area | Required regression coverage |
| --- | --- |
| Identity | cloned/duplicated subtrees, layout maps, event lookup, semantic IDs |
| Contracts | every tag’s kind/fields/attrs/events/hooks; unknown-tag diagnostics |
| States | every declared pseudo-state through index, match, mutation, restyle |
| Input | capture, press/release origin, disabled nodes, focus, keyboard, API parity |
| Layout | retained failure/retry, last-known-good geometry, text revisions |
| Accessibility | role/name/description/state precedence, descendant text, hidden/display, focus, publication |
| Styles | state inheritance, explicit defaults, cycle diagnostics, stylesheet replacement |
| Popovers | invalid metadata, constraints, trigger/surface relationship, focus return |
