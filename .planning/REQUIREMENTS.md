# Requirements: MESH v1.6 Localized Keybind Management

**Defined:** 2026-05-13
**Core Value:** MESH should let plugin authors build distinctive shell UI and service integrations while the shell stays observable, deterministic, and responsive on real interaction paths.

## v1.6 Requirements

### Declaration Contract

- [ ] **KEYB-01**: Frontend modules can declare semantic keybind actions with stable action ids.
- [ ] **KEYB-02**: Each keybind action can define handler, target control reference, scope, label/i18n key, and default trigger metadata.
- [ ] **KEYB-03**: Manifest/settings parsing validates keybind declarations into typed Rust structures instead of relying on ad hoc JSON at dispatch time.

### Locale Resolution

- [ ] **LOCL-01**: The shell resolves keybinds from user overrides, locale-specific defaults, and generic module defaults in deterministic precedence order.
- [ ] **LOCL-02**: Modules can define localized access keys, including English `Accept -> A` and Slovak `Prijat -> P` style mappings.
- [ ] **LOCL-03**: Missing locale-specific bindings fall back to generic defaults without breaking the action.

### Runtime Dispatch

- [ ] **DISP-01**: Resolved keybinds dispatch named script handlers with action id, trigger kind, key/modifier data, locale, target metadata, and resolved label.
- [ ] **DISP-02**: Keybind dispatch can activate functions, buttons, popovers, and service commands through existing module script patterns.
- [ ] **DISP-03**: Existing shell-global shortcuts, text input, focus traversal, and built-in button/toggle/slider keyboard behavior keep priority over module keybinds where required.

### Diagnostics and Conflicts

- [ ] **DIAG-01**: Duplicate keybinds in the same surface/scope emit visible diagnostics.
- [ ] **DIAG-02**: Malformed triggers, missing handlers, missing targets, and invalid locale bindings emit non-fatal diagnostics while valid keybinds continue working.
- [ ] **DIAG-03**: User override keys remain stable by module id and action id, not localized display text.

### Accessibility and Proof

- [ ] **A11Y-01**: Resolved shortcut/access-key metadata is exposed through accessibility annotations using the same resolver as dispatch.
- [ ] **PROOF-01**: Navigation bar and audio popover prove localized module keybinds, user overrides, diagnostics, and script dispatch on real shipped surfaces.

## Future Requirements

### Global Shortcuts

- **GLOB-01**: Module actions can opt into compositor-global shortcut registration through XDG Desktop Portal GlobalShortcuts or another compositor-approved mechanism.
- **GLOB-02**: Global shortcut registration exposes permission/configuration status and trigger descriptions to module authors and users.

### Settings UI

- **KSET-01**: Users can inspect and remap module keybinds through a shell settings surface.
- **KSET-02**: Users can reset keybinds to module, locale, or shell defaults.

## Out of Scope

| Feature | Reason |
|---------|--------|
| Compositor-global shortcuts via XDG Desktop Portal | Wayland global shortcuts require portal sessions, user permission/configuration, and activation signals; module/surface keybinds should be stable first. |
| Full keybind settings UI | This milestone should produce the data model and overrides needed by a later UI without broadening into settings-surface design. |
| Automatic translation or automatic key guessing | Modules/localizers should provide explicit labels and key hints; auto-generated access keys can collide or be unusable on real keyboards. |
| Replacing focus traversal or widget activation | Existing Tab, Escape, Enter, Space, slider, text input, and clipboard behavior is relied on by shipped surfaces. |
| Skia-backed rendering investigation | Deferred beyond v1.6 because localized keybind management is the active milestone scope. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| KEYB-01 | Phase 32 | Pending |
| KEYB-02 | Phase 32 | Pending |
| KEYB-03 | Phase 32 | Pending |
| LOCL-01 | Phase 33 | Pending |
| LOCL-02 | Phase 33 | Pending |
| LOCL-03 | Phase 33 | Pending |
| DISP-01 | Phase 34 | Pending |
| DISP-02 | Phase 34 | Pending |
| DISP-03 | Phase 34 | Pending |
| DIAG-01 | Phase 35 | Pending |
| DIAG-02 | Phase 35 | Pending |
| DIAG-03 | Phase 35 | Pending |
| A11Y-01 | Phase 36 | Pending |
| PROOF-01 | Phase 36 | Pending |

**Coverage:**
- v1.6 requirements: 14 total
- Mapped to phases: 14
- Unmapped: 0

---
*Requirements defined: 2026-05-13*
*Last updated: 2026-05-13 after v1.6 requirements approval*
