# Phase 11: Keyboard Navigation and Shortcuts - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-06
**Phase:** 11-keyboard-navigation-and-shortcuts
**Areas discussed:** Traversal order, Focus visibility, Activation behavior, Shortcut scope

---

## Traversal order

### Q1. Base traversal model

| Option | Description | Selected |
|--------|-------------|----------|
| Visual order | Follow rendered layout users see | ✓ |
| Template order | Follow source / tree order only | |
| Mixed rule | Template order with later exceptions | |

**User's choice:** Visual order
**Notes:** Default Tab traversal should match what users see on screen, not raw template order.

### Q2. Same-row tie breaking

| Option | Description | Selected |
|--------|-------------|----------|
| Left-to-right, then next row | Predictable visual traversal | ✓ |
| Template order within the row | Preserve source order for peers | |
| Geometry only | Strict rendered coordinates | |

**User's choice:** Left-to-right, then next row
**Notes:** User preferred a simple visual tie-break instead of source-order exceptions.

### Q3. Skipped controls

| Option | Description | Selected |
|--------|-------------|----------|
| Skip disabled and hidden controls | Only interactive visible items are tabbable | ✓ |
| Skip hidden only | Disabled items can still receive focus | |
| Include everything | Traversal mirrors structure even when unusable | |

**User's choice:** Skip disabled and hidden controls
**Notes:** Normal Tab flow should only include meaningfully usable controls.

### Q4. End-of-surface behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Wrap to the first control | Loop within the surface | ✓ |
| Stop at the end | No wrapping | |
| Let the shell decide later | Do not lock wrap behavior yet | |

**User's choice:** Wrap to the first control
**Notes:** Surface-local keyboard traversal should remain reliable and self-contained.

### Q5. Author override model

| Option | Description | Selected |
|--------|-------------|----------|
| Override only | Keep visual order as the default and use `tabindex` only for exceptions | ✓ |
| Full author control | Let authors define the entire traversal model | |

**User's choice:** Override only
**Notes:** User explicitly wants default behavior first, with `tabindex`-style overrides only when needed.

### Q6. `tabindex="-1"` behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Yes | Focusable by script/pointer but skipped by normal Tab | ✓ |
| No | Override only reorders tabbable elements | |
| Separate attribute | Split ordering and tabbable state | |

**User's choice:** Yes
**Notes:** User wants web-like `tabindex` semantics here too.

---

## Focus visibility

### Q1. Core `:focus-visible` rule

| Option | Description | Selected |
|--------|-------------|----------|
| Keyboard only | Real modality-aware visible focus | ✓ |
| Always when focused | Keep `:focus-visible` equal to `:focus` | |
| Author decides per component | No engine-level heuristic | |

**User's choice:** Keyboard only
**Notes:** User asked to keep behavior close to web engines and explicitly wanted guidance from current web semantics.

### Q2. Script-focused continuation after keyboard use

| Option | Description | Selected |
|--------|-------------|----------|
| Yes | Keep `:focus-visible` across script-driven continuation | ✓ |
| No | Script focus never stays visible without a fresh Tab | |
| Case-by-case later | Do not lock it yet | |

**User's choice:** Yes
**Notes:** Chosen to stay close to web-engine heuristics.

### Q3. Pointer-focused text entry

| Option | Description | Selected |
|--------|-------------|----------|
| Yes | Text-entry controls still show visible focus | ✓ |
| No | Pointer focus never gets `:focus-visible` | |
| Inputs only | Narrower text-entry rule | |

**User's choice:** Yes
**Notes:** User accepted the common browser pattern where text entry still needs a visible insertion target.

### Q4. Author styling contract

| Option | Description | Selected |
|--------|-------------|----------|
| `:focus-visible` strong ring, `:focus` logical state | Distinguish visual and logical focus hooks | ✓ |
| Style `:focus` only | Simpler but more visually noisy | |
| Treat them the same | Keep both hooks but no distinction | |

**User's choice:** `:focus-visible` strong ring, `:focus` logical state
**Notes:** User wants web-like semantics instead of a shell-only shortcut.

### Q5. Pointer interaction after keyboard navigation

| Option | Description | Selected |
|--------|-------------|----------|
| Pointer action clears `:focus-visible` for that interaction | Closest to web behavior | ✓ |
| Keep keyboard modality sticky longer | Preserve ring across later pointer actions | |
| Let authors choose | No engine-level policy | |

**User's choice:** Pointer action clears `:focus-visible` for that interaction
**Notes:** Non-text pointer focus should generally not look keyboard-focused.

---

## Activation behavior

### Q1. Button activation keys

| Option | Description | Selected |
|--------|-------------|----------|
| Enter and Space both activate | Familiar keyboard button behavior | ✓ |
| Enter only | Narrower contract | |
| Enter activates, Space only shows pressed state | More nuanced browser-like detail | |

**User's choice:** Enter and Space both activate
**Notes:** User preferred the broader familiar default for keyboard-first shell UI.

### Q2. Button activation timing

| Option | Description | Selected |
|--------|-------------|----------|
| On key release | Leaves room for pressed-state feedback | ✓ |
| On key press | Snappier but less web-like | |
| Enter on press, Space on release | Browser nuance immediately | |

**User's choice:** On key release
**Notes:** Chosen as the simpler, more web-like default.

### Q3. Toggle control keys

| Option | Description | Selected |
|--------|-------------|----------|
| Space toggles; Enter can also toggle | Permissive keyboard toggle behavior | ✓ |
| Space only | Narrower common-web behavior | |
| Enter only | Simpler but less familiar | |

**User's choice:** Space toggles; Enter can also toggle
**Notes:** User accepted a slightly more permissive shell default for toggles.

### Q4. Slider keys

| Option | Description | Selected |
|--------|-------------|----------|
| Arrow keys only | Step adjustment only | ✓ |
| Arrow keys plus Home/End | Add min/max jumps | |
| Arrow keys plus Home/End and PageUp/PageDown | Richest first-pass keyboard contract | |

**User's choice:** Arrow keys only
**Notes:** User also clarified these are default behaviors only.

### Q5. Configurability

| Option | Description | Selected |
|--------|-------------|----------|
| Fixed defaults | Hardcoded engine behavior | |
| Shell-settings remappable defaults | Defaults owned by shell settings and remapping | ✓ |

**User's choice:** Shell-settings remappable defaults
**Notes:** User explicitly said all activation defaults should be configurable through shell settings rather than hardcoded.

---

## Shortcut scope

### Q1. Default `onkeydown` / `onkeyup` scope

| Option | Description | Selected |
|--------|-------------|----------|
| Focused element only | Closest to normal web behavior | ✓ |
| Surface-wide by default | Any visible surface can react | |
| Bubble from focused element to surface | Add propagation path by default | |

**User's choice:** Focused element only
**Notes:** Safe local default chosen for ordinary key handlers.

### Q2. Broader shortcut layer

| Option | Description | Selected |
|--------|-------------|----------|
| Focused handlers only in Phase 11 | No broader shortcut contract yet | |
| Add explicit surface-level shortcuts now | Separate surface workflow shortcuts | ✓ |
| Add surface-level and global plugin shortcuts now | Largest first-pass scope | |

**User's choice:** Add explicit surface-level shortcuts now
**Notes:** User gave a concrete example: volume widget should be able to define `m` for mute.

### Q3. Shortcut configuration source

| Option | Description | Selected |
|--------|-------------|----------|
| Module defaults only | Frontend module settings define bindings | |
| Module defaults with core shell settings overrides | Defaults plus shell-owned remapping | ✓ |

**User's choice:** Module defaults with core shell settings overrides
**Notes:** User wants these defaults configurable from frontend module settings and overridable through the core shell settings.

### Q4. Conflict precedence

| Option | Description | Selected |
|--------|-------------|----------|
| Shell global wins | Core shell remains authoritative | ✓ |
| Surface shortcut wins when focused | Local control takes precedence | |
| Configurable precedence | Per-installation policy | |

**User's choice:** Shell global wins
**Notes:** User accepted shell-global precedence as the safer default.

### Q5. Surface shortcut activation window

| Option | Description | Selected |
|--------|-------------|----------|
| Only when that surface has keyboard focus | Avoid background shortcut capture | ✓ |
| Any time the surface is visible | More convenient but riskier | |
| Configurable per shortcut | Extra policy surface | |

**User's choice:** Only when that surface has keyboard focus
**Notes:** Surface-level shortcuts should not fire from unfocused background surfaces.

---

## the agent's Discretion

- Exact traversal sorting implementation details remain open as long as the locked visual-order and `tabindex` semantics hold.
- Exact modality state representation and config schema remain open as long as the locked behavior and remapping rules hold.

## Deferred Ideas

- Plugin-global shortcuts beyond the focused surface were discussed implicitly as a possible larger scope, but deferred out of Phase 11.
- Richer slider keyboard bindings (`Home`, `End`, `PageUp`, `PageDown`) were deferred beyond the locked Phase 11 defaults.
