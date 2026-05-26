# Roadmap: MESH

## Milestones

- 🚧 **v1.16 Element Library** — Phases 86-91 planned
- ✅ **v1.15 Persistent Storage System** — Phases 81-85 shipped 2026-05-26 ([archive](milestones/v1.15-ROADMAP.md))
- ✅ **v1.14 Unified Luau Scripting Runtime** — Phases 74-80 shipped 2026-05-26 ([archive](milestones/v1.14-ROADMAP.md))
- ✅ **v1.13 Manifest I18n Contract** — Phases 70-73 shipped 2026-05-24 ([archive](milestones/v1.13-ROADMAP.md))
- ✅ **v1.12 Module Object Contract** — Phases 65-69 shipped 2026-05-23 ([archive](milestones/v1.12-ROADMAP.md))
- ✅ **v1.11 Surface Keybind Completion** — Phases 60-64 shipped 2026-05-23 ([archive](milestones/v1.11-ROADMAP.md))

## Intent

Build a broad native MESH element library so module authors can create
meaningful shell surfaces without reimplementing common controls in every
frontend module.

The scope is informed by HTML semantic/form controls, Qt Widgets' desktop
control set, and Flutter's composable widget categories, but the contract is
MESH-native: deterministic retained rendering, shell CSS profile styling,
Luau value/change events, keyboard/focus/accessibility behavior, and non-fatal
diagnostics.

## Phase Summary

| # | Phase | Goal | Requirements | Success Criteria |
|---|-------|------|--------------|------------------|
| 86 | Element Contract And Infrastructure | 3/3 | Complete    | 2026-05-26 |
| 87 | Layout And Display Elements | Implement layout, structure, and display primitives needed to compose shell surfaces. | ELEMLAYOUT-01, ELEMLAYOUT-02, ELEMLAYOUT-03, ELEMLAYOUT-04, ELEMLAYOUT-05, ELEMDISPLAY-01, ELEMDISPLAY-02, ELEMDISPLAY-03, ELEMDISPLAY-04, ELEMDISPLAY-05 | 10 |
| 88 | Action And Text Input Controls | Implement configurable action controls and text/numeric input controls with keyboard, value, and accessibility behavior. | ELEMACTION-01, ELEMACTION-02, ELEMACTION-03, ELEMTEXT-01, ELEMTEXT-02, ELEMTEXT-03, ELEMTEXT-04, ELEMTEXT-05 | 8 |
| 89 | Choice Controls And Menus | Implement select, option, checkbox, switch, radio, segmented, menu, menu item, command item, and preference row controls. | ELEMCHOICE-01, ELEMCHOICE-02, ELEMCHOICE-03, ELEMCHOICE-04, ELEMCHOICE-05, ELEMMENU-01, ELEMMENU-02, ELEMMENU-03, ELEMMENU-04 | 9 |
| 90 | Containers And Collections | Implement higher-level containers and collection views for real shell surfaces. | ELEMCONTAINER-01, ELEMCONTAINER-02, ELEMCONTAINER-03, ELEMCONTAINER-04, ELEMCOLLECT-01, ELEMCOLLECT-02, ELEMCOLLECT-03, ELEMCOLLECT-04 | 8 |
| 91 | Shell Proof, Docs, And Hardening | Prove the broad library on shipped surfaces, document authoring, and harden accessibility, diagnostics, and styling behavior. | ELEMPROOF-01, ELEMPROOF-02, ELEMPROOF-03, ELEMPROOF-04, ELEMPROOF-05, ELEMPROOF-06 | 6 |

## Execution Rules

- Build MESH-native controls, not browser-compatible HTML form semantics.
- Keep controls configurable through markup attributes, style classes, Luau value/change events, and accessibility metadata.
- Use shared control state for disabled, read-only, required, focus, selected, checked, expanded, pressed, invalid, and active states.
- Preserve retained rendering, hit testing, keyboard focus, accessibility annotations, and diagnostics for every new control.
- Prefer common shell workflows over exhaustive web/app framework parity.
- Every element must have parser coverage, runtime behavior coverage, styling hooks, and docs before the milestone closes.
- Shipped proof must migrate real navigation/debug/audio UI away from bespoke control workarounds where practical.

## Phases

- [x] Phase 86: Element Contract And Infrastructure (completed 2026-05-26)
- [ ] Phase 87: Layout And Display Elements
- [ ] Phase 88: Action And Text Input Controls
- [ ] Phase 89: Choice Controls And Menus
- [ ] Phase 90: Containers And Collections
- [ ] Phase 91: Shell Proof, Docs, And Hardening

### Phase 86: Element Contract And Infrastructure

**Goal:** Define the native element taxonomy, parser/runtime metadata, shared control state, events, diagnostics, and author contract.

**Requirements:** ELEMCORE-01, ELEMCORE-02, ELEMCORE-03, ELEMCORE-04, ELEMCORE-05, ELEMCORE-06

**Status:** Complete

**Success criteria:**

1. MESH has a documented element taxonomy covering layout, display, action, text input, choice/menu, container, collection, and shell-specific controls.
2. Parser/AST/runtime metadata can represent each planned native element and its common attributes.
3. Shared control state covers disabled, read-only, required, focus, selected, checked, expanded, pressed, invalid, active, and value state.
4. Shared value/change event plumbing supports Luau handlers without bespoke per-control state code.
5. Unknown or unsupported element attributes produce non-fatal diagnostics with author actions.
6. Author docs explain the MESH-native element model and its relationship to HTML, Qt, and Flutter.

### Phase 87: Layout And Display Elements

**Goal:** Implement layout, structure, and display primitives needed to compose shell surfaces.

**Requirements:** ELEMLAYOUT-01, ELEMLAYOUT-02, ELEMLAYOUT-03, ELEMLAYOUT-04, ELEMLAYOUT-05, ELEMDISPLAY-01, ELEMDISPLAY-02, ELEMDISPLAY-03, ELEMDISPLAY-04, ELEMDISPLAY-05

**Status:** Not started

**Success criteria:**

1. Layout primitives include `box`, `row`, `column`, `grid`, `stack`, `spacer`, `divider`, and `scroll-area`.
2. Layout primitives expose configurable alignment, spacing, sizing, overflow, and scroll behavior.
3. Existing layout behavior remains compatible for current shipped modules.
4. Structure primitives include `section`, `header`, `footer`, `group`, and `form-row` semantics for accessibility and styling.
5. Layout and structure diagnostics catch invalid child/attribute combinations.
6. Display primitives include `text`, `icon`, `image`, `badge`, `progress`, `meter`, `tooltip`, `avatar`, and `shortcut`.
7. Display primitives expose accessible labels, roles, value metadata, and style hooks.
8. Progress and meter controls expose determinate/indeterminate and min/max/current values.
9. Tooltip behavior remains keyboard and pointer accessible.
10. Focused tests prove layout/display rendering, style hooks, accessibility annotations, and diagnostics.

### Phase 88: Action And Text Input Controls

**Goal:** Implement configurable action controls and text/numeric input controls with keyboard, value, and accessibility behavior.

**Requirements:** ELEMACTION-01, ELEMACTION-02, ELEMACTION-03, ELEMTEXT-01, ELEMTEXT-02, ELEMTEXT-03, ELEMTEXT-04, ELEMTEXT-05

**Status:** Not started

**Success criteria:**

1. Action controls include `button`, `icon-button`, `toggle-button`, `command-button`, and `link-button`.
2. Action controls expose pressed, disabled, default, destructive, busy, and keybind-aware states.
3. Action controls support pointer activation, keyboard activation, accessibility roles, and Luau handlers.
4. Text inputs include `input`, `textarea`, `search`, and `password` variants.
5. Numeric inputs include `number-input` and `stepper` with min/max/step behavior.
6. Text/numeric controls support value, placeholder, disabled, read-only, required, invalid, and change/input events.
7. Text selection, clipboard, focus, traversal, and accessibility behavior remain coherent with existing shell input rules.
8. Focused tests prove input editing, value events, validation diagnostics, keyboard traversal, and accessibility metadata.

### Phase 89: Choice Controls And Menus

**Goal:** Implement select, option, checkbox, switch, radio, segmented, menu, menu item, command item, and preference row controls.

**Requirements:** ELEMCHOICE-01, ELEMCHOICE-02, ELEMCHOICE-03, ELEMCHOICE-04, ELEMCHOICE-05, ELEMMENU-01, ELEMMENU-02, ELEMMENU-03, ELEMMENU-04

**Status:** Not started

**Success criteria:**

1. Choice controls include `select`, `option`, `checkbox`, `switch`, `radio`, `radio-group`, and `segmented-control`.
2. `select` renders a compact selected value and opens a visible vertical dropdown/popup for options.
3. Choice controls support value/checked state, disabled options, keyboard navigation, pointer selection, and change events.
4. Choice controls expose accessibility roles and selected/checked/value metadata.
5. Choice controls support style hooks for trigger, popup, option, checked, selected, disabled, focus, and active states.
6. Menu controls include `menu`, `menu-item`, `command-item`, `separator`, and `preference-row`.
7. Menus support roving focus, activation, disabled items, icons, shortcuts, nested grouping where supported, and dismissal behavior.
8. Menu and choice diagnostics catch invalid option/value/group relationships.
9. Shipped navigation language selection uses native `select`/`option` instead of the custom horizontal menu.

### Phase 90: Containers And Collections

**Goal:** Implement higher-level containers and collection views for real shell surfaces.

**Requirements:** ELEMCONTAINER-01, ELEMCONTAINER-02, ELEMCONTAINER-03, ELEMCONTAINER-04, ELEMCOLLECT-01, ELEMCOLLECT-02, ELEMCOLLECT-03, ELEMCOLLECT-04

**Status:** Not started

**Success criteria:**

1. Containers include `panel`, `popover`, `dialog`, `sheet`, `tabs`, `tab`, `accordion`, and `details`.
2. Containers support open/closed state, focus trapping/restore where needed, escape/dismiss behavior, labels, and accessibility metadata.
3. Containers expose styling hooks for surface, header, body, footer, backdrop, active tab, expanded item, and disabled states.
4. Container diagnostics catch invalid nesting and missing labels for modal/interactive containers.
5. Collection views include `list`, `list-item`, `table`, `row`, `cell`, `tree`, and `empty-state`.
6. Collections support selection, active row, disabled rows, headers, keyboard navigation, scroll integration, and accessibility metadata.
7. Collections expose value/change/activation events suitable for Luau state and service-driven payloads.
8. Focused tests prove collection selection, keyboard traversal, accessibility, and retained rendering behavior.

### Phase 91: Shell Proof, Docs, And Hardening

**Goal:** Prove the broad element library on shipped surfaces, document authoring, and harden accessibility, diagnostics, and styling behavior.

**Requirements:** ELEMPROOF-01, ELEMPROOF-02, ELEMPROOF-03, ELEMPROOF-04, ELEMPROOF-05, ELEMPROOF-06

**Status:** Not started

**Success criteria:**

1. Shipped navigation, audio popover, quick settings/debug surfaces use native library elements for at least one workflow per control family.
2. Regression tests cover parser support, rendering, input, keyboard navigation, accessibility, diagnostics, and Luau events for every new element family.
3. Author docs include an element reference with attributes, events, states, accessibility notes, and examples.
4. A shipped demo/gallery or debug surface can render the element library for visual inspection.
5. Unsupported browser/Qt/Flutter parity expectations are documented as out-of-scope or future work.
6. Milestone audit verifies no native element family lacks proof coverage.

## Backlog

### Future: Package Distribution

Remote package fetching, third-party dependency resolution, and LSP import
completion remain future work after the runtime import contract is stable.
