---
status: partial
phase: 11-keyboard-navigation-and-shortcuts
source: [11-VERIFICATION.md]
started: 2026-05-06T14:31:51Z
updated: 2026-05-08T00:00:02Z
---

## Current Test

[testing complete]

## Tests

### 1. Wayland navigation-bar keyboard focus acquisition
expected: With the navigation bar focused in a live compositor, Tab and Shift+Tab traverse controls, Enter and Space activate focused controls, arrows step the audio slider, m triggers the mute shortcut, shell-global shortcuts still win, and unfocused surfaces do not react.
result: pass

### 2. Pointer-to-keyboard focus-visible coherence on a real input
expected: Clicking a text input keeps :focus-visible shown, then clicking a non-text control clears the strong visible-focus ring while logical focus remains correct.
result: blocked
blocked_by: other
reason: "i have no way to test this"

## Summary

total: 2
passed: 1
issues: 0
pending: 0
skipped: 0
blocked: 1

## Gaps
