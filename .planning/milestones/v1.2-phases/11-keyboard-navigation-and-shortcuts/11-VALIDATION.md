---
phase: 11
slug: keyboard-navigation-and-shortcuts
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-06
---

# Phase 11 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-shell keyboard_` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-config keyboard_ && nix develop -c cargo test -p mesh-core-elements focus_visible && nix develop -c cargo test -p mesh-core-render accessibility_for_tag && nix develop -c cargo test -p mesh-core-shell keyboard_` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run the task's focused `cargo test` command.
- **After every plan wave:** Run `nix develop -c cargo test -p mesh-core-shell keyboard_`.
- **Before `$gsd-verify-work`:** Run `nix develop -c cargo test -p mesh-core-config keyboard_ && nix develop -c cargo test -p mesh-core-elements focus_visible && nix develop -c cargo test -p mesh-core-render accessibility_for_tag && nix develop -c cargo test -p mesh-core-shell keyboard_`.
- **Max feedback latency:** 120 seconds.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 11-01-01 | 01 | 1 | KEY-02 | T-11-01 | `:focus-visible` reflects keyboard modality rather than plain logical focus, and keyboard focusability metadata stays aligned across button/input/slider/switch/checkbox. | unit | `nix develop -c cargo test -p mesh-core-elements focus_visible && nix develop -c cargo test -p mesh-core-render accessibility_for_tag` | ✅ | ⬜ pending |
| 11-01-02 | 01 | 1 | KEY-01 | T-11-02 | Traversal order is deterministic, wraps, skips hidden/disabled nodes, and honors `tabindex` / `tabindex=-1` without relying on transient node IDs. | integration | `nix develop -c cargo test -p mesh-core-shell keyboard_navigation` | ✅ | ⬜ pending |
| 11-02-01 | 02 | 2 | KEY-02 | T-11-03 | Focused controls respond to Enter, Space, arrows, Backspace, and focus/blur lifecycle through one shell-owned keyboard path. | integration | `nix develop -c cargo test -p mesh-core-shell keyboard_activation` | ✅ | ⬜ pending |
| 11-02-02 | 02 | 2 | KEY-02, KEY-03 | T-11-04 | Focused-element `keydown` / `keyup` handlers run without bypassing control-safe defaults, and Phase 10 `Ctrl+C` ownership still wins when a selection exists. | integration | `nix develop -c cargo test -p mesh-core-shell keyboard_handlers` | ✅ | ⬜ pending |
| 11-03-01 | 03 | 3 | KEY-03 | T-11-05 | Shell settings and module defaults merge deterministically for keyboard bindings and per-surface overrides. | unit | `nix develop -c cargo test -p mesh-core-config keyboard_settings` | ✅ | ⬜ pending |
| 11-03-02 | 03 | 3 | KEY-03 | T-11-06 | Shell-global shortcuts remain authoritative, focused-surface shortcuts activate only on the focused surface, and advertised shortcut metadata stays synchronized with runtime routing. | integration | `nix develop -c cargo test -p mesh-core-shell keyboard_shortcuts` | ✅ | ⬜ pending |
| 11-04-01 | 04 | 4 | KEY-04 | T-11-07 | Navigation-bar and audio-popover proof surfaces expose real keyboard behavior for buttons and sliders without unexpected surface-wide key capture. | integration | `nix develop -c cargo test -p mesh-core-shell navigation_bar_keyboard` | ✅ | ⬜ pending |
| 11-04-02 | 04 | 4 | KEY-04 | T-11-08 | Regression coverage spans buttons, sliders, inputs, pointer-to-keyboard coherence, and docs/examples stay consistent with the runtime contract. | integration/docs | `nix develop -c cargo test -p mesh-core-shell keyboard_regression` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠ flaky*

---

## Wave 0 Requirements

Existing Rust test infrastructure covers all Phase 11 requirements. No new framework bootstrap work is required before implementation starts.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Layer-shell keyboard focus acquisition for navigation-bar in a live compositor | KEY-01, KEY-03 | Automated tests can prove shell routing, but they cannot fully validate compositor-specific keyboard interactivity behavior for `keyboard_mode` changes. | Start MESH in a Wayland session, focus the navigation-bar surface, press `Tab`, `Shift+Tab`, `Enter`, `Space`, arrows, and the configured surface shortcut (for example `m`). Confirm shell-global shortcuts still win and unfocused surfaces do not react. |
| Pointer-to-keyboard coherence on real text-entry controls | KEY-02 | The shell tests cover heuristic state transitions, but the final focus ring feel on pointer-focused text inputs should still be confirmed in a live surface. | Open a surface with a real input, click into it, confirm `:focus-visible` remains shown, then click a non-text control and confirm the strong keyboard ring clears while logical focus remains accurate. |

---

## Validation Sign-Off

- [x] All tasks have automated verify commands or existing infrastructure coverage
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all missing test-infrastructure dependencies
- [x] No watch-mode flags
- [x] Feedback latency < 120s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
