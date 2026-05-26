# Requirements: MESH v1.16 Element Library

**Defined:** 2026-05-26
**Core Value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## v1 Requirements

### Element Contract And Infrastructure

- [x] **ELEMCORE-01**: MESH defines a native element taxonomy for layout, display, action, text input, choice/menu, container, collection, and shell-specific controls.
- [x] **ELEMCORE-02**: Parser, AST, template, and runtime metadata can represent every planned native element and common attributes without bespoke parser branches per shipped surface.
- [x] **ELEMCORE-03**: Shared control state covers disabled, read-only, required, focus, selected, checked, expanded, pressed, invalid, active, and value state.
- [x] **ELEMCORE-04**: Shared value/change event plumbing lets Luau handlers consume control changes without per-control state workarounds.
- [x] **ELEMCORE-05**: Unsupported or invalid element attributes produce non-fatal diagnostics with concrete author actions.
- [x] **ELEMCORE-06**: Author docs explain the MESH-native element model and how it is informed by HTML, Qt Widgets, and Flutter without promising one-for-one compatibility.

### Layout And Structure Elements

- [x] **ELEMLAYOUT-01**: Layout primitives include `box`, `row`, `column`, `grid`, `stack`, `spacer`, `divider`, and `scroll-area`.
- [x] **ELEMLAYOUT-02**: Layout primitives expose configurable alignment, spacing, sizing, overflow, and scroll behavior through markup attributes and style hooks.
- [x] **ELEMLAYOUT-03**: Existing layout behavior remains compatible for current shipped modules.
- [x] **ELEMLAYOUT-04**: Structure primitives include `section`, `header`, `footer`, `group`, and `form-row` semantics for accessibility and styling.
- [x] **ELEMLAYOUT-05**: Layout and structure diagnostics catch invalid child/attribute combinations.

### Display Elements

- [x] **ELEMDISPLAY-01**: Display primitives include `text`, `icon`, `image`, `badge`, `progress`, `meter`, `tooltip`, `avatar`, and `shortcut`.
- [x] **ELEMDISPLAY-02**: Display primitives expose accessible labels, roles, value metadata, and style hooks.
- [x] **ELEMDISPLAY-03**: Progress and meter controls support determinate/indeterminate and min/max/current values.
- [x] **ELEMDISPLAY-04**: Tooltip behavior is available to pointer and keyboard users.
- [x] **ELEMDISPLAY-05**: Display diagnostics catch missing assets, invalid value ranges, and missing accessible names where relevant.

### Action Controls

- [ ] **ELEMACTION-01**: Action controls include `button`, `icon-button`, `toggle-button`, `command-button`, and `link-button`.
- [ ] **ELEMACTION-02**: Action controls expose pressed, disabled, default, destructive, busy, and keybind-aware states.
- [ ] **ELEMACTION-03**: Action controls support pointer activation, keyboard activation, accessibility roles, and Luau handlers.

### Text And Numeric Input Controls

- [ ] **ELEMTEXT-01**: Text inputs include `input`, `textarea`, `search`, and `password` variants.
- [ ] **ELEMTEXT-02**: Numeric inputs include `number-input` and `stepper` with min/max/step behavior.
- [ ] **ELEMTEXT-03**: Text and numeric controls support value, placeholder, disabled, read-only, required, invalid, and input/change events.
- [ ] **ELEMTEXT-04**: Text selection, clipboard, focus, traversal, and accessibility behavior remain coherent with existing shell input rules.
- [ ] **ELEMTEXT-05**: Text and numeric input diagnostics catch invalid values, missing labels, and unsupported configurations.

### Choice Controls

- [ ] **ELEMCHOICE-01**: Choice controls include `select`, `option`, `checkbox`, `switch`, `radio`, `radio-group`, and `segmented-control`.
- [ ] **ELEMCHOICE-02**: `select` renders a compact selected value and opens a visible vertical dropdown/popup for options.
- [ ] **ELEMCHOICE-03**: Choice controls support value/checked state, disabled options, keyboard navigation, pointer selection, and change events.
- [ ] **ELEMCHOICE-04**: Choice controls expose accessibility roles and selected/checked/value metadata.
- [ ] **ELEMCHOICE-05**: Choice controls support style hooks for trigger, popup, option, checked, selected, disabled, focus, and active states.

### Menus And Command Controls

- [ ] **ELEMMENU-01**: Menu controls include `menu`, `menu-item`, `command-item`, `separator`, and `preference-row`.
- [ ] **ELEMMENU-02**: Menus support roving focus, activation, disabled items, icons, shortcuts, grouping, and dismissal behavior.
- [ ] **ELEMMENU-03**: Menu and command controls expose Luau activation/change events and accessibility metadata.
- [ ] **ELEMMENU-04**: Menu diagnostics catch invalid nesting, missing labels, and invalid command/value relationships.

### Containers

- [ ] **ELEMCONTAINER-01**: Containers include `panel`, `popover`, `dialog`, `sheet`, `tabs`, `tab`, `accordion`, and `details`.
- [ ] **ELEMCONTAINER-02**: Containers support open/closed state, focus trapping/restore where needed, escape/dismiss behavior, labels, and accessibility metadata.
- [ ] **ELEMCONTAINER-03**: Containers expose style hooks for surface, header, body, footer, backdrop, active tab, expanded item, and disabled states.
- [ ] **ELEMCONTAINER-04**: Container diagnostics catch invalid nesting and missing labels for modal/interactive containers.

### Collections

- [ ] **ELEMCOLLECT-01**: Collection views include `list`, `list-item`, `table`, `row`, `cell`, `tree`, and `empty-state`.
- [ ] **ELEMCOLLECT-02**: Collections support selection, active row, disabled rows, headers, keyboard navigation, scroll integration, and accessibility metadata.
- [ ] **ELEMCOLLECT-03**: Collections expose value/change/activation events suitable for Luau state and service-driven payloads.
- [ ] **ELEMCOLLECT-04**: Collection diagnostics catch invalid row/cell/tree relationships and missing accessibility metadata.

### Proof, Docs, And Hardening

- [ ] **ELEMPROOF-01**: Shipped navigation, audio popover, quick settings/debug surfaces use native library elements for at least one workflow per control family.
- [ ] **ELEMPROOF-02**: Regression tests cover parser support, rendering, input, keyboard navigation, accessibility, diagnostics, and Luau events for every new element family.
- [ ] **ELEMPROOF-03**: Author docs include an element reference with attributes, events, states, accessibility notes, and examples.
- [ ] **ELEMPROOF-04**: A shipped demo/gallery or debug surface can render the element library for visual inspection.
- [ ] **ELEMPROOF-05**: Unsupported browser/Qt/Flutter parity expectations are documented as out-of-scope or future work.
- [ ] **ELEMPROOF-06**: Milestone audit verifies no native element family lacks proof coverage.

## Future Requirements

### Advanced Element Work

- **ELEMADV-01**: Virtualized lists, large data tables, drag/reorder behavior, and editable tree/table cells remain future work unless promoted during v1.16 execution.
- **ELEMADV-02**: Full browser-compatible form submission and validation semantics remain out of scope.
- **ELEMADV-03**: Multi-select and complex nested menus remain future work unless promoted by shipped shell needs.

### Package Distribution

- **LUAPKG-01**: Remote package resolution and third-party dependency fetching remain future work.
- **LUAPKG-02**: Language-server import completion remains future work after the runtime contract is stable.

## Out of Scope

| Feature | Reason |
|---------|--------|
| One-for-one HTML/Qt/Flutter compatibility | MESH needs shell-native retained rendering and deterministic behavior, not broad framework emulation. |
| Browser form submission semantics | Shell controls emit Luau events and state changes; they do not submit documents. |
| Native platform widget embedding | MESH draws and owns its controls for consistent theming, diagnostics, and retained rendering. |
| Full rich text editor | Text input is in scope; document editing is a separate product surface. |
| Virtualized mega-tables | Useful later, but broad element coverage comes first. |
| Drag-and-drop builder interactions | Higher complexity and not required for the first broad shell element library. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| ELEMCORE-01 | Phase 86 | Complete |
| ELEMCORE-02 | Phase 86 | Complete |
| ELEMCORE-03 | Phase 86 | Complete |
| ELEMCORE-04 | Phase 86 | Complete |
| ELEMCORE-05 | Phase 86 | Complete |
| ELEMCORE-06 | Phase 86 | Complete |
| ELEMLAYOUT-01 | Phase 87 | Complete |
| ELEMLAYOUT-02 | Phase 87 | Complete |
| ELEMLAYOUT-03 | Phase 87 | Complete |
| ELEMLAYOUT-04 | Phase 87 | Complete |
| ELEMLAYOUT-05 | Phase 87 | Complete |
| ELEMDISPLAY-01 | Phase 87 | Complete |
| ELEMDISPLAY-02 | Phase 87 | Complete |
| ELEMDISPLAY-03 | Phase 87 | Complete |
| ELEMDISPLAY-04 | Phase 87 | Complete |
| ELEMDISPLAY-05 | Phase 87 | Complete |
| ELEMACTION-01 | Phase 88 | Pending |
| ELEMACTION-02 | Phase 88 | Pending |
| ELEMACTION-03 | Phase 88 | Pending |
| ELEMTEXT-01 | Phase 88 | Pending |
| ELEMTEXT-02 | Phase 88 | Pending |
| ELEMTEXT-03 | Phase 88 | Pending |
| ELEMTEXT-04 | Phase 88 | Pending |
| ELEMTEXT-05 | Phase 88 | Pending |
| ELEMCHOICE-01 | Phase 89 | Pending |
| ELEMCHOICE-02 | Phase 89 | Pending |
| ELEMCHOICE-03 | Phase 89 | Pending |
| ELEMCHOICE-04 | Phase 89 | Pending |
| ELEMCHOICE-05 | Phase 89 | Pending |
| ELEMMENU-01 | Phase 89 | Pending |
| ELEMMENU-02 | Phase 89 | Pending |
| ELEMMENU-03 | Phase 89 | Pending |
| ELEMMENU-04 | Phase 89 | Pending |
| ELEMCONTAINER-01 | Phase 90 | Pending |
| ELEMCONTAINER-02 | Phase 90 | Pending |
| ELEMCONTAINER-03 | Phase 90 | Pending |
| ELEMCONTAINER-04 | Phase 90 | Pending |
| ELEMCOLLECT-01 | Phase 90 | Pending |
| ELEMCOLLECT-02 | Phase 90 | Pending |
| ELEMCOLLECT-03 | Phase 90 | Pending |
| ELEMCOLLECT-04 | Phase 90 | Pending |
| ELEMPROOF-01 | Phase 91 | Pending |
| ELEMPROOF-02 | Phase 91 | Pending |
| ELEMPROOF-03 | Phase 91 | Pending |
| ELEMPROOF-04 | Phase 91 | Pending |
| ELEMPROOF-05 | Phase 91 | Pending |
| ELEMPROOF-06 | Phase 91 | Pending |

**Coverage:**

- v1 requirements: 47 total
- Mapped to phases: 47
- Unmapped: 0

---
*Requirements defined: 2026-05-26*
