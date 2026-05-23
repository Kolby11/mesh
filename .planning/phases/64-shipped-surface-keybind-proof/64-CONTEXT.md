# Phase 64: Shipped Surface Keybind Proof - Context

**Gathered:** 2026-05-23
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 64 locks shipped-surface proof for the completed focused-surface keybind system on navigation and audio surfaces. It may add a minimal shipped audio keybind declaration where proof requires it, but it does not add compositor-global shortcuts, a keybind settings UI, or generated access keys.

</domain>

<decisions>
## Implementation Decisions

### Shipped Proof
- Keep navigation-bar `mute` shortcut proof as the manifest-owned shortcut baseline.
- Add/verify an audio-popover focused-surface access key for its mute action so the real audio surface proves keybind dispatch without disrupting slider, button, focus, or text input behavior.

### Regression Scope
- Reuse existing Phase 60-63 focused tests for shell-global precedence, text input ownership, selection copy, locale/override fallback, diagnostics, and debug metadata.
- Final verification should run focused keybind, navigation, audio-surface, and debug suites.
</decisions>

<code_context>
## Relevant Code

- `modules/frontend/navigation-bar/module.json` and `src/components/volume-button.mesh` already declare and subscribe the shipped navigation mute keybind.
- `modules/frontend/audio-popover/src/main.mesh` has real volume/mute controls and service commands but no manifest keybind declaration yet.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` already contains navigation and audio keyboard interaction tests.
</code_context>

<specifics>
## Phase Requirements

- KPROOF-01: navigation-bar dispatch and shell-global precedence proof.
- KPROOF-02: audio-popover keybind/access-key proof without slider/button/focus/text regressions.
- KPROOF-03: locale and override proof for exact locale, parent locale, generic default, user override, and no-binding cases.
- KPROOF-04: final focused verification suites for keyboard, keybind, navigation, and audio behavior.
</specifics>

<deferred>
Compositor-global shortcuts, keybind settings UI, and generated access keys remain backlog items.
</deferred>
