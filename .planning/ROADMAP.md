# Roadmap: MESH v1.11 Surface Keybind Completion

## Milestones

- [ ] **v1.11 Surface Keybind Completion** — Phases 60-64 planned
- [x] **v1.10 Painter Engine** — Phases 51-59 shipped 2026-05-23
- [x] **v1.9 Renderer Library Integration** — Phases 46-50 shipped 2026-05-21
- [x] **v1.8 Rendering Engine Architecture** — Phases 42-45 shipped 2026-05-18

## Intent

Finish the paused surface-scoped keybind system now that canonical module manifests, typed contribution records, retained rendering, and shipped-surface proof infrastructure are stable. v1.11 turns manifest-owned keybind declarations and localized trigger resolution into real focused-surface runtime behavior with diagnostics, override safety, accessibility metadata, and navigation/audio proof.

MESH keeps shell-owned input precedence. Shell-global shortcuts, text input, selection copy, focus traversal, and default widget activation must continue to win over or compose with surface keybinds deterministically. Compositor-global shortcuts and a full remapping UI remain future work.

## Phase Summary

| # | Phase | Goal | Requirements | Success Criteria |
|---|-------|------|--------------|------------------|
| 60 | Surface Keybind Dispatch Runtime | 1/1 | Complete    | 2026-05-23 |
| 61 | Localized Resolution And Override Safety | 1/1 | Complete    | 2026-05-23 |
| 62 | Conflict And Invalid-Keybind Diagnostics | 1/1 | Complete    | 2026-05-23 |
| 63 | Accessibility Metadata And Observability | Publish resolved keybind metadata through accessibility/debug paths and document the author contract | KACC-01, KACC-02, KACC-03 | 3 |
| 64 | Shipped Surface Keybind Proof | Prove navigation/audio keybind behavior end to end and lock regression suites for existing keyboard behavior | KPROOF-01, KPROOF-02, KPROOF-03, KPROOF-04 | 4 |

## Execution Rules

- Keep canonical keybind declarations in `module.json` and typed installed-graph contribution records.
- Treat settings as user overrides or compatibility fallback only; do not make settings the declaration source again.
- Preserve shell-global shortcut precedence, text input handling, selection copy, focus traversal, and default widget activation.
- Prefer diagnostics over silent drops for malformed, conflicting, unsupported, or unresolved keybind data.
- Prove behavior on real shipped surfaces, not only synthetic component fixtures.

## Phases

### Phase 60: Surface Keybind Dispatch Runtime

**Goal:** Route manifest-owned semantic keybind actions through the focused-surface component input path while preserving existing keyboard ownership rules.

**Requirements:** KDISP-01, KDISP-02, KDISP-03, KDISP-04

**Status:** Planned

**Success criteria:**
1. Canonical `module.json` keybind contributions can dispatch semantic actions through the existing component handler path.
2. Shell-global shortcuts still run before focused-surface keybinds.
3. Text input, selection copy, focus traversal, and default widget activation keep their current precedence.
4. Navigation/audio shipped-surface fixtures use manifest-owned actions instead of relying only on legacy settings shortcuts.

### Phase 61: Localized Resolution And Override Safety

**Goal:** Make effective keybind resolution deterministic and safe across user overrides, locale-specific access keys, parent locale fallback, generic triggers, and legacy fallback.

**Requirements:** KRES-01, KRES-02, KRES-03, KRES-04

**Status:** Planned

**Success criteria:**
1. Resolution order is user override, exact active locale, parent locale, generic manifest trigger, then no binding.
2. Overrides are keyed by surface id and stable action id.
3. Localized triggers apply only to access-key actions unless an explicit user override exists.
4. Legacy `settings.keyboard.shortcuts` remains available only as a compatibility fallback for missing manifest actions.

### Phase 62: Conflict And Invalid-Keybind Diagnostics

**Goal:** Emit actionable, non-fatal diagnostics for keybind declarations and overrides that cannot be used safely or deterministically.

**Requirements:** KDIAG-01, KDIAG-02, KDIAG-03, KDIAG-04

**Status:** Planned

**Success criteria:**
1. Malformed declarations include module id, surface id, action id, and reason in diagnostics.
2. Duplicate effective bindings on one focused surface diagnose without making dispatch order ambiguous.
3. Missing targets, unsupported trigger forms, and unresolved overrides diagnose instead of disappearing silently.
4. Unsafe overrides that would steal reserved shell-global shortcuts, text input, or selection copy are rejected or ignored with diagnostics.

### Phase 63: Accessibility Metadata And Observability

**Goal:** Surface resolved keybind metadata to accessibility and debug/diagnostic consumers, and document the completed author contract.

**Requirements:** KACC-01, KACC-02, KACC-03

**Status:** Planned

**Success criteria:**
1. Target controls expose resolved shortcut or access-key metadata through existing accessibility annotations where available.
2. Debug/profiling payloads can show resolved keybind metadata and diagnostics for focused surfaces.
3. Author docs explain declaration, localized triggers, overrides, diagnostics, accessibility metadata, and focused-surface scope.

### Phase 64: Shipped Surface Keybind Proof

**Goal:** Prove the completed surface keybind system on real navigation/audio surfaces and lock regression coverage for existing keyboard behavior.

**Requirements:** KPROOF-01, KPROOF-02, KPROOF-03, KPROOF-04

**Status:** Planned

**Success criteria:**
1. Navigation-bar tests prove manifest-owned mute or equivalent actions dispatch correctly with shell-global precedence preserved.
2. Audio-popover tests prove surface keybinds or access keys work without regressing slider, button, focus, or text input behavior.
3. Locale and override tests cover exact locale, parent locale, generic default, user override, and no-binding cases.
4. Final verification runs focused shell/component suites for keyboard, keybind, navigation, and audio-surface behavior.

## Backlog

### Future: Compositor-Global Shortcuts

Compositor-global shortcuts remain deferred until focused-surface keybind semantics are stable. Future work needs platform/session permission design, diagnostics separate from focused surfaces, and likely XDG Desktop Portal or compositor-specific integration.

### Future: Keybind Settings UI

A full user-facing remapping UI remains deferred. v1.11 validates override schema and runtime behavior so a later settings surface can safely inspect and modify overrides.

### Future: Generated Access Keys

Automatic locale-aware access-key generation remains deferred. v1.11 keeps authors responsible for explicit localized trigger defaults.
