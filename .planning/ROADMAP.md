# Roadmap: MESH v1.6 Localized Keybind Management

**Status:** Active milestone planning
**Phases:** 32-36
**Total Phases:** 5

## Overview

`v1.6` turns MESH's early surface shortcut support into a formal localized keybind management system for frontend modules. Modules should be able to declare semantic actions once, map those actions to handlers and target controls, provide locale-specific access-key defaults, allow stable user overrides, and expose resolved shortcut metadata to scripts, diagnostics, and accessibility.

The milestone deliberately stays module/surface-scoped. Compositor-global shortcuts through XDG Desktop Portal remain future work because they require portal sessions, user permission/configuration flows, and activation signal handling beyond the first stable module keybind contract.

## Phases

### Phase 32: Keybind Declaration Contract

**Goal:** Add a typed module keybind declaration model so frontend modules can declare stable semantic actions without relying on ad hoc runtime JSON parsing.
**Depends on:** Phase 31
**Requirements:** `KEYB-01`, `KEYB-02`, `KEYB-03`

Planned work:

- Define typed Rust structures for frontend keybind action declarations with stable action ids.
- Support handler, target control reference, scope, label or i18n key, trigger kind, and default trigger metadata.
- Parse declarations from existing module manifest/settings paths in a way that preserves current navigation-bar shortcut behavior.
- Validate malformed declarations early and make valid declarations available to shell runtime.

Success criteria:

1. A frontend module can declare at least one semantic keybind action with a stable id.
2. The parser exposes typed declarations to shell/component code without direct ad hoc JSON lookup during dispatch.
3. Existing `@mesh/navigation-bar` mute shortcut remains compatible through the new declaration contract.
4. Invalid declaration shapes produce diagnostics without blocking unrelated valid module data.

Plans:

- **32-01: Keybind declaration contract and compatibility bridge** *(Wave 1, ready)* - add normalized keybind declaration types, bridge current settings shortcuts through typed declarations, and prove navigation-bar compatibility.

Cross-cutting constraints:

- Preserve current navigation-bar `keyboard.shortcuts.mute` behavior and `KeyboardSettings.surface_shortcuts` override identity by action id.
- Do not implement locale fallback, duplicate detection, or XDG portal/global shortcut behavior in Phase 32.
- Shell-global shortcut and existing input precedence must remain unchanged.

### Phase 33: Locale-Aware Keybind Resolution

**Goal:** Resolve effective keybinds from module defaults, active locale, and user overrides with deterministic precedence.
**Depends on:** Phase 32
**Requirements:** `LOCL-01`, `LOCL-02`, `LOCL-03`

Planned work:

- Add a keybind resolver that merges generic module defaults, locale-specific defaults, and shell user overrides.
- Add support for localized access-key defaults such as English `Accept -> A` and Slovak `Prijat -> P`.
- Preserve stable override identity by module id and action id.
- Fall back to generic defaults when locale-specific entries are missing or invalid.

Success criteria:

1. Resolver tests prove user overrides win over locale defaults and locale defaults win over generic defaults.
2. A Slovak locale binding can resolve to a different access key than English for the same action id.
3. Missing locale data falls back to the generic declaration without disabling the action.
4. Override identity never depends on translated label text.

### Phase 34: Script Dispatch and Input Precedence

**Goal:** Dispatch resolved keybind actions into module scripts while preserving existing shell-global, text-input, focus traversal, and widget-control behavior.
**Depends on:** Phase 33
**Requirements:** `DISP-01`, `DISP-02`, `DISP-03`

Planned work:

- Replace the current surface shortcut runtime lookup with resolved keybind records.
- Dispatch script handlers with action id, trigger kind, key/modifier data, locale, target metadata, and resolved label.
- Preserve shell-global debug shortcuts as the highest-priority shortcut path.
- Preserve Tab/Escape traversal, Ctrl+C text selection copy, text input, and focused button/toggle/slider behavior.
- Prove keybind handlers can activate existing functions, buttons, popovers, and service commands.

Success criteria:

1. Keybind handler events include action id, trigger metadata, locale, target metadata, and resolved label.
2. Navigation-bar/audio actions can be activated through resolved keybind dispatch.
3. Existing shell-global debug shortcuts still win before module keybinds.
4. Text input and focused-control keyboard behavior are not captured by single-letter module access keys.

### Phase 35: Conflict Diagnostics and Override Safety

**Goal:** Make keybind authoring failures visible and non-fatal, especially duplicate bindings inside the same scope.
**Depends on:** Phase 33, Phase 34
**Requirements:** `DIAG-01`, `DIAG-02`, `DIAG-03`

Planned work:

- Detect duplicate keybinds in the same surface/scope after locale and override resolution.
- Diagnose malformed triggers, unknown keys/modifiers, missing handlers, missing target refs, and invalid locale bindings.
- Keep valid keybinds active when unrelated declarations fail validation.
- Add regression coverage for stable user override keys by module id and action id.

Success criteria:

1. Duplicate bindings in one scope emit visible diagnostics.
2. Malformed declarations are non-fatal and do not disable unrelated valid keybinds.
3. Missing handler and missing target reference cases are diagnosed clearly.
4. User override tests prove translated label changes do not affect override lookup.

### Phase 36: Accessibility Metadata and Shipped-Surface Proof

**Goal:** Expose resolved keybind metadata through accessibility annotations and prove localized module keybinds on shipped surfaces.
**Depends on:** Phase 34, Phase 35
**Requirements:** `A11Y-01`, `PROOF-01`

Planned work:

- Drive accessibility shortcut/access-key annotations from the same resolved records used by dispatch.
- Update `@mesh/navigation-bar` and `@mesh/audio-popover` to prove localized keybind declarations and script dispatch.
- Add English and Slovak proof data for at least one localized access-key action.
- Add proof for user override behavior and conflict diagnostics on a shipped surface.
- Document authoring guidance for module keybind declarations and localization hints.

Success criteria:

1. Accessibility metadata shows the effective resolved binding, including user overrides.
2. Navigation bar and audio popover pass real-surface regression tests for localized keybind behavior.
3. English and Slovak bindings are covered by automated tests.
4. Author documentation explains shortcuts, access keys, scopes, localization, and out-of-scope global shortcuts.

## Milestone Boundaries

### Included

- Typed frontend module keybind declarations
- Locale-aware access-key defaults and deterministic fallback
- User override precedence by module id and action id
- Script dispatch for resolved module actions
- Conflict and malformed-binding diagnostics
- Accessibility metadata for resolved bindings
- Shipped-surface proof on navigation bar/audio popover

### Excluded

- Compositor-global shortcuts through XDG Desktop Portal
- Full keybind settings UI
- Automatic translation or automatic access-key generation
- Replacement of focus traversal, text input, or built-in widget activation behavior
- Skia-backed rendering investigation

## Research Basis

This roadmap follows Microsoft, GNOME, and GTK guidance that shortcuts/accelerators, access keys/mnemonics, and focused-widget key bindings should remain distinct concepts. Localized action names should be able to localize access keys, but collision handling must be scoped and diagnostics must be visible. Wayland compositor-global shortcuts are intentionally deferred because XDG Desktop Portal GlobalShortcuts is a separate permissioned session API.

Primary research artifacts:

- `.planning/research/STACK.md`
- `.planning/research/FEATURES.md`
- `.planning/research/ARCHITECTURE.md`
- `.planning/research/PITFALLS.md`
- `.planning/research/SUMMARY.md`

Primary external sources:

- https://learn.microsoft.com/en-us/globalization/input/hotkeys-accelerators
- https://learn.microsoft.com/en-us/windows/apps/develop/input/access-keys
- https://learn.microsoft.com/en-us/windows/apps/develop/input/keyboard-accelerators
- https://developer.gnome.org/hig/guidelines/keyboard.html
- https://gnome.pages.gitlab.gnome.org/gtk/gtk4/input-handling.html
- https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html

## Archived Milestones

- `v1.5` CPU Rendering Performance Improvement - shipped 2026-05-13.
- `v1.4` Major Performance Fixes - shipped 2026-05-09.
- `v1.3` Performance Instrumentation and Responsiveness - shipped 2026-05-09.
- `v1.2` Rendering System Upgrade - shipped 2026-05-08.
- `v1.1` Backend Plugin MVP - shipped 2026-05-05.

## Backlog and Carryover

- Skia-backed rendering remains a future rendering investigation candidate after localized keybind management.
- Deferred validation/UAT cleanup from older milestones remains backlog work outside `v1.6`.
- The pending unified package/module manifest phase idea remains future planning work and is not part of keybind management.
- The slight audio popover transition delay from Phase 31 remains deferred polish: `.planning/todos/pending/2026-05-13-phase31-audio-popover-transition-delay.md`.

---
*Roadmap updated: 2026-05-13 after defining v1.6 requirements*
