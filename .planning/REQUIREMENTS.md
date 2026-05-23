# Requirements: MESH v1.11 Surface Keybind Completion

**Defined:** 2026-05-23
**Core Value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## v1 Requirements

### Action Dispatch

- [x] **KDISP-01**: Frontend module authors can declare semantic surface keybind actions in canonical `module.json` and have those actions dispatch through the existing shell component handler path.
- [x] **KDISP-02**: Surface keybind dispatch preserves shell-global shortcut precedence, text input handling, text selection copy, focus traversal, and default widget activation behavior.
- [x] **KDISP-03**: Keybind actions can target declared controls or focused-surface handlers without relying on localized labels or raw display text.
- [x] **KDISP-04**: Shipped navigation and audio surfaces exercise real manifest-owned keybind dispatch rather than legacy settings-only shortcuts.

### Resolution And Overrides

- [x] **KRES-01**: Effective focused-surface bindings resolve from user override, exact active locale, parent locale, generic manifest trigger, then no binding.
- [x] **KRES-02**: User overrides are keyed by surface id and stable action id, and cannot create canonical declarations.
- [x] **KRES-03**: Localized access-key defaults remain scoped to `access_key` actions while shortcut actions keep generic defaults unless a user override exists.
- [x] **KRES-04**: Legacy `settings.keyboard.shortcuts` remains a compatibility fallback only when canonical manifest contributions do not declare the action.

### Diagnostics And Safety

- [ ] **KDIAG-01**: Malformed keybind declarations emit non-fatal diagnostics with module id, surface id, action id, and concise reason.
- [ ] **KDIAG-02**: Duplicate effective bindings on the same focused surface emit diagnostics without making dispatch nondeterministic.
- [ ] **KDIAG-03**: Missing targets, unsupported trigger forms, and unresolved overrides emit diagnostics instead of silently dropping behavior.
- [ ] **KDIAG-04**: Override validation rejects or ignores unsafe bindings that would steal reserved shell-global shortcuts, text input ownership, or selection-copy behavior.

### Accessibility And Observability

- [ ] **KACC-01**: Resolved shortcut and access-key metadata is exposed through existing accessibility annotations for target controls where available.
- [ ] **KACC-02**: Debug or profiling payloads can show resolved surface keybind metadata and keybind diagnostics without making settings the canonical declaration source.
- [ ] **KACC-03**: Author docs explain the surface keybind lifecycle: manifest declaration, localized triggers, user overrides, diagnostics, accessibility metadata, and focused-surface scope.

### Shipped Surface Proof

- [ ] **KPROOF-01**: Navigation-bar tests prove manifest-owned mute or equivalent actions dispatch correctly with shell-global precedence preserved.
- [ ] **KPROOF-02**: Audio-popover tests prove surface keybinds or access keys work on real controls without regressing slider, button, focus, or text input behavior.
- [ ] **KPROOF-03**: Locale and override regression tests prove deterministic resolution across exact locale, parent locale, generic default, user override, and no-binding cases.
- [ ] **KPROOF-04**: Final verification runs the focused shell/component test suites needed to prove no regressions to existing keyboard behavior.

## Future Requirements

### Platform Shortcuts

- **KGLOBAL-01**: Compositor-global shortcuts can be declared, permissioned, and routed through XDG Desktop Portal or compositor-specific APIs.
- **KGLOBAL-02**: Global shortcut behavior can be inspected and diagnosed separately from focused-surface keybinds.

### Settings UI

- **KUI-01**: Users can view, search, remap, reset, and diagnose surface keybind overrides in a dedicated settings surface.
- **KUI-02**: The settings UI can preview localized labels and conflict diagnostics without becoming the canonical declaration source.

### Generated Access Keys

- **KGEN-01**: MESH can suggest or generate locale-aware access keys from labels when authors opt in.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Compositor-global shortcuts | Platform/session permission behavior is separate from focused-surface dispatch and remains future work. |
| Full keybind settings UI | v1.11 validates override schema and runtime behavior but does not build a remapping surface. |
| Automatic translation or access-key generation | Authors own localized trigger defaults for this milestone. |
| Replacing focus traversal or widget activation | Existing keyboard behavior is a compatibility boundary this milestone must preserve. |
| Raw label-based dispatch | Stable action ids and target references are required for localization safety. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| KDISP-01 | Phase 60 | Complete |
| KDISP-02 | Phase 60 | Complete |
| KDISP-03 | Phase 60 | Complete |
| KDISP-04 | Phase 60 | Complete |
| KRES-01 | Phase 61 | Complete |
| KRES-02 | Phase 61 | Complete |
| KRES-03 | Phase 61 | Complete |
| KRES-04 | Phase 61 | Complete |
| KDIAG-01 | Phase 62 | Pending |
| KDIAG-02 | Phase 62 | Pending |
| KDIAG-03 | Phase 62 | Pending |
| KDIAG-04 | Phase 62 | Pending |
| KACC-01 | Phase 63 | Pending |
| KACC-02 | Phase 63 | Pending |
| KACC-03 | Phase 63 | Pending |
| KPROOF-01 | Phase 64 | Pending |
| KPROOF-02 | Phase 64 | Pending |
| KPROOF-03 | Phase 64 | Pending |
| KPROOF-04 | Phase 64 | Pending |

**Coverage:**
- v1 requirements: 19 total
- Mapped to phases: 19
- Unmapped: 0

---
*Requirements defined: 2026-05-23*
*Last updated: 2026-05-23 after starting v1.11*
