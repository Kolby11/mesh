# Phase 34: Script Dispatch and Input Precedence - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md - this log preserves the alternatives considered.

**Date:** 2026-05-14T18:51:31+02:00
**Phase:** 34-Script Dispatch and Input Precedence
**Areas discussed:** Dispatch Payload Shape, Handler Binding Model, Input Precedence Rules, Shipped Proof Targets

---

## Dispatch Payload Shape

| Option | Description | Selected |
|--------|-------------|----------|
| Full resolved action record | Include action id, trigger kind, key, modifiers, source, locale, label, label i18n key, and target metadata when available. | |
| Behavior-first minimal payload | Include only action id, trigger kind, key, modifiers, source, and locale; leave label/target enrichment for later. | |
| Split payload | Keep `event.keybind` minimal, but put resolved label and target details under separate fields. | |
| User correction | Do not include i18n/label data in keybind dispatch because one action can be activated on multiple elements. | ✓ |

**User's choice:** Keybind dispatch should not include i18n data; keybinds can be activated on multiple elements, so the manifest should be rethought and dispatch should stay action/trigger focused.
**Notes:** The broader module manifest redesign was captured as a deferred idea.

| Option | Description | Selected |
|--------|-------------|----------|
| Element-owned handlers/targets | Manifest declares action ids and triggers; elements subscribe with `keybind`/`onkeybind`. | ✓ |
| Both manifest and element binding | Keep room for manifest `handler`/`target_ref`, but element subscribers win when present. | |
| Manifest-owned binding | One manifest action points to one handler/target; multi-element cases need separate action ids. | |

**User's choice:** Element-owned handlers/targets.
**Notes:** This decision makes handler and target metadata subscriber-specific rather than manifest-owned.

| Option | Description | Selected |
|--------|-------------|----------|
| Dispatch to all subscribers | Every rendered element with the same action id and handler receives the event. | |
| Dispatch to focused subscriber first | If a focused element subscribes to that action, only it receives the event; otherwise dispatch matching surface subscribers. | ✓ |
| Require unique active subscriber | Multiple subscribers are ambiguous and do not dispatch until diagnostics exist. | |

**User's choice:** Dispatch to focused subscriber first.
**Notes:** This keeps multi-element subscriptions viable without firing multiple unrelated handlers.

| Option | Description | Selected |
|--------|-------------|----------|
| Structured fields | `source` is `user_override`, `locale_default`, or `module_default`, with a separate `locale` field when applicable. | ✓ |
| Compact source string | Keep current style such as `source = "locale:sk"`. | |
| No source metadata | Scripts get only action id, trigger kind, key, and modifiers. | |

**User's choice:** Structured fields.
**Notes:** Locale is present only when a locale default supplied the trigger.

---

## Handler Binding Model

| Option | Description | Selected |
|--------|-------------|----------|
| Keep manifest action minimal | Keybind actions only declare trigger data and stable ids; no `handler`, `target_ref`, label, or i18n fields. | ✓ |
| Keep fields but ignore at dispatch | Leave room in the model for future manifest-owned binding, but do not use it now. | |
| Compatibility only | Keep old fields if they already exist, but mark them legacy/unused. | |

**User's choice:** Keep manifest action minimal.
**Notes:** Phase 34 should avoid adding normalized action fields that imply one action owns one handler/target.

| Option | Description | Selected |
|--------|-------------|----------|
| Namespaced element subscribers | Imported child components subscribe normally and dispatch calls the child namespaced handler. | ✓ |
| Top-level surface only | Only root surface scripts receive keybind dispatch. | |
| Both, but parent wins | Child components may subscribe, but parent subscribers win for the same action. | |

**User's choice:** Namespaced element subscribers.
**Notes:** Existing compiled handler namespacing should be preserved.

| Option | Description | Selected |
|--------|-------------|----------|
| Require both | Dispatch only calls nodes with both `keybind` and `onkeybind`. | ✓ |
| Keybind implies click | A node with `keybind` but no `onkeybind` dispatches `onclick`. | |
| Keybind only emits generic event | Shell emits a generic action event elsewhere. | |

**User's choice:** Require both.
**Notes:** Nodes with only `keybind` may still be useful later for annotation.

---

## Input Precedence Rules

| Option | Description | Selected |
|--------|-------------|----------|
| After protected focused behavior | Shell-global/debug first; then text input, Tab/Escape, Ctrl+C, focused widget activation; then module keybinds; then custom focused handlers. | ✓ |
| Current broad behavior | Keep module keybind dispatch before focused handlers/widgets except existing protected cases. | |
| Only when no focus | Module keybinds fire only with no focused input/control. | |

**User's choice:** After protected focused behavior.
**Notes:** Module keybinds must not steal from built-in shell and widget behavior.

| Option | Description | Selected |
|--------|-------------|----------|
| Built-in widget keys only | Protect Enter/Space for buttons/toggles and arrow keys for sliders; custom handlers do not automatically outrank module keybinds. | ✓ |
| All focused handlers protected | Any focused node with key handlers gets first refusal. | |
| Inputs only | Only text input and selection are protected. | |

**User's choice:** Built-in widget keys only.
**Notes:** Custom `keydown`/`keyup` handlers run after module keybinds unless a protected built-in path consumed the key.

| Option | Description | Selected |
|--------|-------------|----------|
| Strict trigger match | Shortcuts match key and modifiers exactly; access keys match unmodified keys only. | ✓ |
| Current key-only match | Match key name only and ignore modifiers. | |
| Loose access keys, strict shortcuts | Shortcuts are strict; access keys can fire with Shift. | |

**User's choice:** Strict trigger match.
**Notes:** This replaces current key-only matching for Phase 34 behavior.

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, add regression coverage | Prove shell-global/debug shortcuts win before module keybinds. | ✓ |
| Document only | Note the precedence but rely on existing behavior. | |
| Defer debug proof | Focus only on module dispatch and widget/input behavior. | |

**User's choice:** Yes, add regression coverage.
**Notes:** Debug precedence is part of `DISP-03`.

---

## Shipped Proof Targets

| Option | Description | Selected |
|--------|-------------|----------|
| Minimal representative set | Navigation-bar mute service command, audio popover open via keybind, and one non-service function action such as theme toggle. | ✓ |
| Audio-focused set | Mute command, audio popover open/close, and audio popover slider/button actions. | |
| Broad shipped surface set | Navigation bar, audio popover, theme/settings controls, and service commands all get keybind declarations now. | |

**User's choice:** Minimal representative set.
**Notes:** Keeps Phase 34 proof narrow and avoids Phase 36 breadth.

| Option | Description | Selected |
|--------|-------------|----------|
| Add focused localized proof now | Prove at least one localized access-key dispatch path because Phase 34 consumes the Phase 33 resolver. | ✓ |
| Generic triggers only | Prove dispatch mechanics now; Phase 36 handles localized shipped-surface behavior. | |
| Unit-level localized only | Test resolver plus dispatch together, but do not update shipped declarations yet. | |

**User's choice:** Add focused localized proof now.
**Notes:** Broader localized shipped-surface proof remains Phase 36.

| Option | Description | Selected |
|--------|-------------|----------|
| Update shipped declarations narrowly | Add real keybind declarations for proof actions in `@mesh/navigation-bar`. | ✓ |
| Proof-only tests | Use test manifests/components only. | |
| Update navigation bar and audio popover | Add declarations across both shipped surfaces now. | |

**User's choice:** Update shipped declarations narrowly.
**Notes:** Do not broaden into full audio-popover keybind declaration work.

| Option | Description | Selected |
|--------|-------------|----------|
| Focused automated regressions | Tests prove payload shape, subscriber focus precedence, strict modifiers, protected behavior, debug precedence, and proof actions. | ✓ |
| Automated plus live UAT | Same tests plus a manual live shell check. | |
| Unit-only | Keep this phase to resolver/dispatch unit tests. | |

**User's choice:** Focused automated regressions.
**Notes:** Live UAT is not required for Phase 34.

---

## the agent's Discretion

- Choose exact internal struct names and file placement for resolved dispatch records.
- Choose the exact non-service proof action, with theme toggle preferred if it fits existing tests cleanly.

## Deferred Ideas

- Rethink the whole module manifest system and how it works as a whole; this belongs with the existing unified package/module manifest future phase.
- Create unified package and module manifest phase was reviewed and not folded into Phase 34.
- Audio popover transition delay polish was reviewed and not folded into Phase 34.
