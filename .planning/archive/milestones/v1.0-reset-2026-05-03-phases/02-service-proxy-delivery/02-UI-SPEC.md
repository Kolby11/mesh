---
phase: 02
slug: service-proxy-delivery
status: approved
shadcn_initialized: false
preset: none
created: 2026-05-02
---

# Phase 02 — UI Design Contract

> Visual and interaction contract for frontend phases. Generated inline from Phase 02 context and existing bundled surface patterns, then verified against the UI-phase dimensions.

---

## Design System

| Property | Value |
|----------|-------|
| Tool | none |
| Preset | not applicable |
| Component library | none |
| Icon library | MESH `icon` element using named icons from service state / XDG lookup |
| Font | theme-driven sans stack via `token(typography.*)`; no custom one-off font families in this phase |

### Surface Direction

Phase 02 should preserve and sharpen the existing shell look rather than invent a new visual language:

- **Panel:** compact, low-noise, status-first utility bar
- **Quick settings:** structured utility drawer with dense controls, clear hierarchy, and no decorative flourishes
- **Overall mood:** pragmatic system UI, not marketing UI

### Layout Contract

- Panel remains a single horizontal band with left / center / right zones.
- Quick settings remains a two-column utility drawer:
  - left = compact nav rail / chips
  - right = active service detail area
- The drawer should feel intentionally modular: header, quick chips, nav rail, active section.
- Service content blocks should read like operator modules, not cards in a social/app dashboard.

### Interaction Contract

- Service proxies are not interaction affordances. Buttons, sliders, and toggles remain on template elements only.
- State reads should update UI through rerendered field reads, not through callback-driven "sync" choreography.
- Advanced provider-only affordances may appear in bundled surfaces, but the core user path must still be visually coherent on the shared base contract.
- Commands that mutate backend state should feel immediate and utility-oriented:
  - volume up/down
  - mute toggle
  - wifi enable/disable
- Service lookup failure states must be visible in copy, never silently blank.

### Accessibility Contract

- Every interactive control must keep explicit `title` and `aria-label` text.
- Icon-only affordances must expose a textual label or tooltip.
- Empty, unavailable, and degraded states must render readable text, not only icon changes.
- Status text should remain understandable without color alone.

---

## Spacing Scale

Declared values (must be multiples of 4):

| Token | Value | Usage |
|-------|-------|-------|
| xs | 4px | Icon-to-label gaps, micro spacing inside compact controls |
| sm | 8px | Default control gaps, inline padding, stacked compact items |
| md | 16px | Standard internal section spacing, row/column gaps |
| lg | 24px | Drawer padding, section separation |
| xl | 32px | Large drawer splits and major content breathing room |
| 2xl | 48px | Reserved for larger surface transitions or future expanded sections |
| 3xl | 64px | Not used directly in this phase; reserved for page-scale surfaces later |

Exceptions: none

### Density Rules

- **Panel** uses `sm` as the dominant spacing unit, with `xs` only for icon/value coupling.
- **Quick settings** uses `md` between logical groups and `sm` within control clusters.
- Avoid mixing more than three spacing tiers in a single module.
- Keep nav rail and chips visually compact; detail panes may breathe more than navigation.

---

## Typography

| Role | Size | Weight | Line Height |
|------|------|--------|-------------|
| Body | 13px | 500 | 1.35 |
| Label | 11px | 700 | 1.2 |
| Heading | 18px | 700 | 1.2 |
| Display | 24px | 800 | 1.1 |

### Typographic Rules

- Utility labels such as section headers use uppercase label styling sparingly.
- Live values like volume percentage should carry stronger weight than descriptive copy.
- Panel values should stay compact and scan-friendly; avoid oversized typography in the top bar.
- Secondary descriptors such as provider/backend names should use subdued body or label roles, never compete with primary state values.

---

## Color

| Role | Value | Usage |
|------|-------|-------|
| Dominant (60%) | `token(color.surface)` | Primary shell backgrounds and the quick-settings root surface |
| Secondary (30%) | `token(color.surface-container)` / `token(color.surface-container-high)` | Nav buttons, control wells, section containers, chips at rest |
| Accent (10%) | `token(color.primary)` plus `token(color.primary-container)` | Active selection, slider emphasis, hover/selected utility emphasis only |
| Destructive | `token(color.error)` / `token(color.error-container)` | Unsupported, failed, or explicitly destructive actions only |

Accent reserved for: active nav state, focused or hovered primary controls, slider emphasis, and the currently selected quick-settings destination. Never use accent for all buttons at rest.

### Color Rules

- Keep the shell neutral-first; the UI should read as infrastructure, not brand theater.
- Do not introduce a new bespoke palette in this phase; stay on theme tokens already used by bundled surfaces.
- Unsupported or unavailable service states should prefer neutral or error container treatments over bright accent styling.
- Hover states may intensify from secondary container to primary container, but rest states should remain subdued.

---

## Copywriting Contract

| Element | Copy |
|---------|------|
| Primary CTA | `Close quick settings` |
| Empty state heading | `Select a section` |
| Empty state body | `Select a section from the left to view live controls and service status.` |
| Error state | `Service unavailable. Check provider status or configuration, then try again.` |
| Destructive confirmation | `Disconnect network`: `Disconnect this network and keep the interface available for reconnection.` |

### Copy Rules

- Use system-oriented, literal wording. Prefer `Wi-Fi`, `Bluetooth`, `Audio`, `Unavailable`, `Connected`, `Disconnected`.
- Do not use playful or promotional language.
- Fallback copy should explain what is missing and what the user can still do.
- Provider/backend names may appear as secondary status copy when useful, not as headline content.

---

## Registry Safety

| Registry | Blocks Used | Safety Gate |
|----------|-------------|-------------|
| none | none | not required |

---

## Phase-Specific UI Contracts

### Panel Contract

- The panel is a **read-first summary surface**.
- It should show:
  - network summary
  - volume icon + value
  - battery text
  - clock
- The only direct interactive control that must remain in-scope here is opening quick settings from the volume indicator button.
- Panel service state should prefer concise derived labels:
  - short text
  - single icon
  - no verbose service diagnostics inline

### Quick Settings Contract

- Quick settings is the **command-first utility surface** for this phase.
- It should support:
  - service section selection from the nav rail
  - primary read state for audio and network
  - primary commands for audio and Wi-Fi
- Detail sections should use stacked utility modules with obvious section titles and live values.
- The drawer should degrade gracefully:
  - if service lookup fails, keep the frame and explanatory copy
  - do not collapse the whole surface into blank space

### Service-State Presentation Rules

- Read state should be reflected directly from proxy fields wherever possible.
- Derived UI state is allowed for:
  - icon name
  - value formatting
  - tooltips
  - provider labels
- Those derivations should stay local to the component and be recomputed from current proxy state on rerender.
- Do not preserve the old callback-driven "sync" mental model in UI examples or docs.

### Dominant Provider Rules

- Bundled surfaces may use richer dominant-provider extras for advanced behavior.
- The base experience must still feel intentional:
  - core reads visible
  - primary commands present
  - graceful fallback copy when richer extras are absent
- Provider-specific richness should enhance detail panes, not be required for the shell to appear fundamentally usable.

---

## Checker Sign-Off

- [x] Dimension 1 Copywriting: PASS
- [x] Dimension 2 Visuals: PASS
- [x] Dimension 3 Color: PASS
- [x] Dimension 4 Typography: PASS
- [x] Dimension 5 Spacing: PASS
- [x] Dimension 6 Registry Safety: PASS

**Approval:** approved 2026-05-02
