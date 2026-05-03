---
phase: 03
slug: frontend-reactivity-and-events
status: approved
shadcn_initialized: false
preset: none
created: 2026-05-02
---

# Phase 03 — UI Design Contract

> Interaction contract for the frontend reactivity/event proof. This phase changes behavior inside the existing MESH shell UI; it does not introduce a new web app or visual system.

---

## Design System

| Property | Value |
|----------|-------|
| Tool | none |
| Preset | not applicable |
| Component library | MESH `.mesh` primitives |
| Icon library | existing MESH icon names |
| Font | existing theme typography tokens |

---

## Spacing Scale

Declared values (must use existing tokens):

| Token | Value | Usage |
|-------|-------|-------|
| xs | `token(spacing.xs)` | Icon gaps, compact slider/control padding |
| sm | `token(spacing.sm)` | Button and inline-control padding |
| md | `token(spacing.md)` | Compact grouped-control gaps where needed |
| lg | `token(spacing.lg)` | Existing navigation shell padding only |

Exceptions: keep existing `2px`, `30px`, `40px`, and `44px` values already present in navigation-bar controls unless replacing them is required for the inline slider to fit.

---

## Typography

| Role | Size | Weight | Line Height |
|------|------|--------|-------------|
| Body | existing token | existing token | existing token |
| Label | existing token | existing token | existing token |
| Heading | not introduced | not introduced | not introduced |
| Display | not introduced | not introduced | not introduced |

No new visible explanatory text is introduced beyond tooltip/title/ARIA strings for the inline volume slider.

---

## Color

| Role | Value | Usage |
|------|-------|-------|
| Dominant | `token(color.surface)` | Existing navigation-bar shell |
| Secondary | `token(color.surface-container)` / `token(color.surface-container-high)` | Existing compact controls and hover states |
| Accent | existing theme slider track/thumb styling | Inline slider value affordance only |
| Destructive | not used | No destructive controls in this phase |

Accent reserved for: slider track/thumb or focused control affordance only.

---

## Copywriting Contract

| Element | Copy |
|---------|------|
| Primary CTA | none |
| Empty state heading | none |
| Empty state body | none |
| Error state | Diagnostics overlay entry for handler failures: include component id, handler name, and script error message |
| Destructive confirmation | none |
| Slider title | `Volume {percent}%` or `Audio service unavailable` |
| Slider aria-label | `Volume` |

---

## Interaction Contract

| Interaction | Contract |
|-------------|----------|
| Volume slider drag | `on_change(value)` fires continuously on drag with a `number` argument from `0.0` to `1.0` for normalized sliders or the slider's configured numeric range when the implementation keeps `min/max` semantics |
| Volume slider release | `on_release(value)` fires once on pointer release if declared |
| Focusable controls | `on_focus` fires when a control becomes focused through pointer or keyboard routing |
| Existing volume button click | `on_click(event)` keeps the current pointer and `current_target.position` payload behavior |
| Handler failure | Log with `tracing::warn!`, add a visible diagnostics entry, keep last successfully rendered frame |

---

## Registry Safety

| Registry | Blocks Used | Safety Gate |
|----------|-------------|-------------|
| shadcn official | none | not required |
| third-party | none | not allowed |

---

## Checker Sign-Off

- [x] Dimension 1 Copywriting: PASS
- [x] Dimension 2 Visuals: PASS
- [x] Dimension 3 Color: PASS
- [x] Dimension 4 Typography: PASS
- [x] Dimension 5 Spacing: PASS
- [x] Dimension 6 Registry Safety: PASS

**Approval:** approved 2026-05-02
