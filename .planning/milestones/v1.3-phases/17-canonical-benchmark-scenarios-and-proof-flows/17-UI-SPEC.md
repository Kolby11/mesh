---
phase: 17
slug: canonical-benchmark-scenarios-and-proof-flows
status: approved
shadcn_initialized: false
preset: none
created: 2026-05-09
---

# Phase 17 - UI Design Contract

> Visual and interaction contract for frontend work in Phase 17.

---

## Design System

| Property | Value |
|----------|-------|
| Tool | none |
| Preset | not applicable |
| Component library | built-in `.mesh` components |
| Icon library | none for this phase unless existing `.mesh` icon support is already present in the inspector |
| Font | existing theme typography tokens only |

Phase 17 extends the existing `@mesh/debug-inspector` right-side panel. It must not introduce a separate visual system, new page-level layout, marketing/hero styling, or decorative backgrounds.

---

## Product Surface Contract

| Area | Contract |
|------|----------|
| Surface | `@mesh/debug-inspector` benchmark view inside the existing right-side debug inspector |
| Primary user | Developer inspecting live shell responsiveness |
| Density | Compact operational tool, not a dashboard page |
| Layout unit | Scenario row |
| Visual rhythm | Header/copy, then one vertical list of five fixed benchmark scenario rows |
| Interaction model | Explicit run action per scenario; no automatic benchmark runs from opening the view |
| Result model | Latest session result shown inline in each scenario row |
| Empty model | Rows remain visible with unavailable/ready/waiting copy |

The benchmark view replaces scaffold cards with rows. Each row must be stable in height class and content order so status/result changes do not reflow the whole panel unexpectedly.

---

## Benchmark Scenario Rows

Each scenario row must expose these text/state slots:

| Slot | Required content |
|------|------------------|
| Scenario title | One of: `Hover`, `Surface open/close`, `Pointer-driven update`, `Keyboard traversal`, `Backend-driven update` |
| Scenario id | One of: `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, `backend_update` |
| Target | Surface/module identity, for example `@mesh/navigation-bar`, `@mesh/audio-popover`, or `mesh.audio -> @mesh/pipewire-audio` |
| Status | `Profiling off`, `Ready`, `Waiting for samples`, `Complete`, `Unavailable`, or `Skipped` |
| Action | Concrete button label; see Copywriting Contract |
| Primary metric | Most relevant timing summary for the scenario |
| Secondary metric | Supporting surface/backend/redraw summary |
| Hint | One sentence explaining what to do when unavailable or empty |

Scenario-specific metric contract:

| Scenario | Primary metric | Secondary metric |
|----------|----------------|------------------|
| Hover | `input_handling` or hover/restyle timing | `style_restyle` plus visible render timing |
| Surface open/close | `total_surface_render` | redraw count |
| Pointer-driven update | input-to-visible-response summary | layout/paint or total render |
| Keyboard traversal | focus/input/render summary | target surface id |
| Backend-driven update | backend stage timing | resulting frontend surface render cost |

Rows must not hide scenario definitions when profiling is off. Disable or explain the run action instead.

---

## Spacing Scale

Declared values must continue using existing theme spacing tokens.

| Token | Value | Usage |
|-------|-------|-------|
| xs | `token(spacing.xs)` | Inline gaps, badge padding, row microcopy gap |
| sm | `token(spacing.sm)` | Button padding, compact row gaps, tab spacing |
| md | `token(spacing.md)` | Row padding, section gaps, inspector body rhythm |
| lg | `token(spacing.lg)` if available | Only for larger internal breathing room; avoid in the 320px inspector unless needed |

Exceptions: existing `2px` gap in header copy may remain. Do not add new arbitrary spacing values unless the component already lacks a suitable token and the value is documented in code comments.

---

## Typography

Use existing theme typography tokens. Do not scale font size with viewport width.

| Role | Size | Weight | Line Height |
|------|------|--------|-------------|
| Body | `token(typography.size.xs)` | 400 | existing renderer default |
| Label | `token(typography.size.xs)` | 700 for scenario id/status labels | existing renderer default |
| Heading | `token(typography.size.sm)` | 700 | existing renderer default |
| Panel title | `token(typography.size.md)` | 700 | existing renderer default |

Benchmark rows must use compact headings. Do not introduce display-size text inside the inspector.

---

## Color

Use theme semantic tokens only. Do not hard-code hex colors in Phase 17 `.mesh` files.

| Role | Value | Usage |
|------|-------|-------|
| Dominant (60%) | `token(color.surface)` | Inspector root |
| Secondary (30%) | `token(color.surface-container)` and `token(color.surface-container-low)` | Scenario rows, state/result groups |
| Accent (10%) | `token(color.primary)` | Active tab indicator and primary run action only |
| Status neutral | `token(color.on-surface-variant)` | Waiting/empty/unavailable copy |
| Destructive | not used | Phase 17 has no destructive action |

Accent reserved for: active benchmark/run action, active tab indicator, and any single selected/live state marker. Do not color every metric or every interactive element with the accent.

---

## Component Contracts

### Benchmark View

File: `modules/frontend/debug-inspector/src/components/benchmark-view.mesh`

Required changes:

- Replace the scaffold-only explanatory cards with a row-based benchmark list.
- Keep exactly five visible scenario rows.
- Accept row data from `modules/frontend/debug-inspector/src/main.mesh` through explicit props.
- Preserve stable empty/unavailable rendering.
- Use `.benchmark-row`, `.benchmark-title`, `.benchmark-meta`, `.benchmark-status`, `.benchmark-action`, `.benchmark-primary-metric`, and `.benchmark-secondary-metric` or equivalent clearly named classes.

### Debug Inspector Parent

File: `modules/frontend/debug-inspector/src/main.mesh`

Required changes:

- Read benchmark scenario/result data from the `mesh.debug` service payload.
- Derive safe fallback values when `debug_service.benchmarks` is nil, malformed, or empty.
- Wire one explicit handler per run action or one generic handler with scenario id payload.
- Publish only debug-scoped shell events such as `shell.run-debug-benchmark` if new actions are needed.
- Preserve existing overview, surfaces, backend services, close, and profiling toggle behavior.

### Existing Views

Files:

- `modules/frontend/debug-inspector/src/components/overview-view.mesh`
- `modules/frontend/debug-inspector/src/components/surfaces-view.mesh`
- `modules/frontend/debug-inspector/src/components/backend-services-view.mesh`
- `modules/frontend/debug-inspector/src/components/view-tabs.mesh`

Phase 17 should not redesign these views. Touch them only if the benchmark tab needs a small shared style or if tests require a shared helper state.

---

## Interaction States

| State | Required behavior |
|-------|-------------------|
| Profiling off | Rows visible; action copy says `Start profiling first` or run buttons are disabled/unavailable through existing supported `.mesh` affordances |
| Profiling live, no samples | Rows visible; status `Waiting for samples`; hint tells user to run or interact with the scenario |
| Ready | Row shows `Run scenario` action and target surface/module |
| Running | Row status `Running`; keep previous/latest metrics visible if available |
| Complete | Row status `Complete`; primary and secondary metric slots populated |
| Unavailable | Row status `Unavailable`; action not presented as runnable; hint names missing target or data |
| Skipped | Row status `Skipped`; hint explains why the shell did not run it |

No state may remove the row entirely. Hidden rows would make the fixed benchmark suite look incomplete.

---

## Copywriting Contract

| Element | Copy |
|---------|------|
| View title | `Benchmark / Interaction` |
| View body | `Run fixed shell interactions and compare their live profiling summaries.` |
| Primary CTA | `Run scenario` |
| Profiling-off action | `Start profiling first` |
| Empty state heading | `No benchmark results yet` |
| Empty state body | `Run a scenario while profiling is live to populate latest-session metrics.` |
| Unavailable state | `Unavailable: required surface or provider data is not present.` |
| Running state | `Running` |
| Complete state | `Complete` |
| Destructive confirmation | not applicable |

Scenario titles must remain exactly:

- `Hover`
- `Surface open/close`
- `Pointer-driven update`
- `Keyboard traversal`
- `Backend-driven update`

Do not include explanatory onboarding paragraphs inside each row. Keep row copy to one short hint plus metrics.

---

## Accessibility and Keyboard Contract

- Benchmark actions must be buttons, not clickable text.
- Button labels must include the scenario name in `aria-label` or visible text when the visible button label is generic.
- The benchmark tab must remain reachable through the existing `ViewTabs` component.
- Keyboard traversal scenario work must not break the inspector's existing focus and tab behavior.
- Text must fit inside a 320px-wide inspector panel. Long target ids may wrap but must not overlap metrics or actions.

---

## Responsive and Layout Constraints

- Primary target width is the existing right-side inspector panel, currently configured with width `320`.
- Scenario rows must work at 320px width.
- Use vertical stacking inside rows; do not use side-by-side metric layouts that require wide desktop space.
- Keep action buttons at a stable minimum height of at least 32px.
- Do not create nested cards. A scenario row may be a single bordered row/card, but metric groups inside it should be unframed text groups.

---

## Registry Safety

| Registry | Blocks Used | Safety Gate |
|----------|-------------|-------------|
| shadcn official | none | not required |
| third-party | none | not allowed for this phase |

Phase 17 must not add a web UI registry, React component library, or external icon dependency. The target UI is `.mesh`.

---

## Checker Sign-Off

- [x] Dimension 1 Copywriting: PASS
- [x] Dimension 2 Visuals: PASS
- [x] Dimension 3 Color: PASS
- [x] Dimension 4 Typography: PASS
- [x] Dimension 5 Spacing: PASS
- [x] Dimension 6 Registry Safety: PASS

**Approval:** approved 2026-05-09
