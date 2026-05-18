---
phase: 40-migration-diagnostics-and-author-docs
verified: 2026-05-18T11:06:46Z
status: passed
score: "13/13 must-haves verified"
overrides_applied: 0
---

# Phase 40: Migration Diagnostics and Author Docs Verification Report

**Phase Goal:** Turn old terminology and manifest shapes into visible, author-facing replacement/removal guidance across bundled docs, examples, and diagnostics.
**Verified:** 2026-05-18T11:06:46Z
**Status:** passed
**Re-verification:** No - initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Legacy terminology and manifest shapes in bundled modules/docs are updated or receive replacement/removal guidance with concrete removal targets. | VERIFIED | `load_module_manifest()` emits concrete actions for `package.json`, legacy `module.json`, `mesh.toml`, `plugin.json`, and ambiguous manifests in `crates/core/extension/module/src/package/installed_graph.rs:1074`, `:1099`, `:1141`, `:1158`, `:1182`; docs table mirrors these actions in `docs/module-system.md:59`. |
| 2 | Diagnostics distinguish blocking load errors from migration warnings. | VERIFIED | `ModuleManifestDiagnosticSeverity::{Warning, Error}` exists in `error.rs:4`; blocking `plugin.json`/ambiguous manifests return `Diagnostic` errors while accepted legacy inputs return warning diagnostics; package tests assert both severities in `tests.rs:361`, `:400`, `:416`, `:440`, `:467`. |
| 3 | Existing v1.6 keybind declaration/resolution data remains addressable under the canonical contribution model. | VERIFIED | `ContributedKeybindAction` carries `trigger` and `localized_triggers` in `installed_graph.rs:1023`; graph indexing clones both from `manifest.mesh.keybinds.actions` at `installed_graph.rs:878`; tests assert default `m` and localized `sk` trigger `s` in `tests.rs:1323`. |
| 4 | Module authors have a documented migration path from old examples to the new module model. | VERIFIED | `docs/installation.md:11` teaches `module.json`; `docs/module-system.md:59` provides migration diagnostics; `docs/llm-context.md:146` names the canonical author-facing manifest. |
| 5 | Old manifest names are replacement debt or internal-only migration inputs, not public aliases. | VERIFIED | `docs/module-system.md:61` says old names are not author-facing aliases; `docs/module-vocabulary.md:30` says old names are replacement debt; old-term grep hits are legacy fixtures or explicit migration guidance. |
| 6 | Diagnostics distinguish blocking load errors from migration warnings. | VERIFIED | Same runtime and test evidence as truth 2, including `plugin.json` error and legacy accepted-manifest warnings. |
| 7 | Migration guidance uses concrete replacement/removal wording. | VERIFIED | Runtime suggested actions and docs use exact replacement/removal wording for all old inputs. |
| 8 | Canonical author examples use `module.json` with top-level `name`/`version` and `mesh.apiVersion`/`mesh.kind`. | VERIFIED | `docs/installation.md:22` describes top-level `name`/`version` and nested `mesh`; frontend/backend author docs reference `module.json` and `mesh.kind`. |
| 9 | `package.json`, old `module.json`, `plugin.json`, and `mesh.toml` are not new-author targets. | VERIFIED | Docs direct authors to `module.json`; old names appear only in migration diagnostics, vocabulary inventory, or legacy fixture tests. |
| 10 | OS package-manager names and resource lookup aliases are not MESH module vocabulary aliases. | VERIFIED | `docs/module-vocabulary.md:139` explicitly separates resource lookup aliases and OS package names from vocabulary aliases. |
| 11 | Paused v1.6 keybind declarations remain part of canonical module manifest data. | VERIFIED | `KeybindAction` contains `trigger` and `localized_triggers` in `manifest/model.rs:429`; canonical JSON round-trip test preserves `localizedTriggers` in `manifest/tests.rs:383`. |
| 12 | Typed installed-graph keybind contributions expose enough data for later dispatch, conflict, and accessibility phases. | VERIFIED | `ContributedKeybindAction` exposes source, module id, action id, scope, label, description, category, default trigger, and localized triggers in `installed_graph.rs:1015`; docs state later phases can inspect this data in `docs/module-system.md:173`. |
| 13 | `settings.keyboard.surface_shortcuts` are user overrides, not canonical module declarations. | VERIFIED | Shell resolution starts with manifest declarations and appends legacy settings declarations only when action ids are absent in `keyboard.rs:175`; `surface_shortcuts` overrides resolved manifest ids in `keyboard.rs:160`; docs state the boundary in `docs/settings/README.md:129`. |

**Score:** 13/13 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/core/extension/module/src/package/error.rs` | Structured diagnostic severity and suggested action fields | VERIFIED | `ModuleManifestDiagnosticSeverity`, `field_path`, `message`, and `suggested_action` are implemented. |
| `crates/core/extension/module/src/package/installed_graph.rs` | Legacy manifest detection, migration diagnostics, and typed keybind records | VERIFIED | Loader diagnostics are wired; `ContributedKeybindAction` preserves trigger data. |
| `crates/core/extension/module/src/package/tests.rs` | Diagnostic and installed-graph keybind regression tests | VERIFIED | Tests assert severity, exact suggested actions, and keybind trigger preservation. |
| `crates/core/shell/src/shell/component/input/keyboard.rs` | Manifest-first shortcut resolution with settings override/fallback | VERIFIED | Manifest declarations precede legacy settings declarations; user overrides and modifier matching are implemented. |
| `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` | Shortcut resolution behavior coverage | VERIFIED | Tests cover manifest default, user override, localized trigger, legacy fallback, manifest-over-legacy precedence, and modifiers. |
| `docs/module-system.md` | Migration diagnostics and keybind migration author docs | VERIFIED | Contains `## Migration Diagnostics` and `### Keybind Contributions`. |
| `docs/installation.md` | Canonical author manifest examples | VERIFIED | Uses `module.json`, top-level `name`/`version`, and nested `mesh` metadata. |
| `docs/llm-context.md` | AI-facing canonical manifest context | VERIFIED | Names `module.json` as canonical author-facing manifest and no longer teaches package authoring as the target. |
| `docs/settings/README.md` | Keybind override boundary docs | VERIFIED | Defines `surface_shortcuts` as user override data and legacy settings declarations as fallback input only. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| Legacy manifest file detection | Structured diagnostics | `load_module_manifest()` returns `ModuleManifestError::Diagnostic` or loaded warnings | WIRED | File-name branches produce concrete severity/action diagnostics. |
| Canonical `mesh.keybinds` | Installed graph keybind records | `ModuleContributionIndex::index_module` clones action trigger data | WIRED | Default and localized triggers flow into `ContributedKeybindAction`. |
| Component manifest keybinds | Shell shortcut resolution | `manifest_surface_shortcut_declarations()` feeds `resolved_surface_shortcuts()` | WIRED | Manifest declarations are primary; legacy settings only fill absent ids. |
| User `surface_shortcuts` | Resolved keybind shortcuts | `keyboard_settings.surface_shortcuts` override by action id | WIRED | Tests prove override key `u` wins over manifest default `m`. |
| Keybind docs | Runtime contract | Docs describe `mesh.keybinds`, installed graph preservation, and settings override boundary | WIRED | Documentation matches code behavior and test names. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| `installed_graph.rs` manifest diagnostics | `LoadedModuleManifest.diagnostics` / `ModuleManifestError::Diagnostic` | Actual manifest file existence and legacy/canonical parsing in `load_module_manifest()` | Yes | FLOWING |
| `installed_graph.rs` keybind records | `ContributedKeybindAction.trigger`, `localized_triggers` | `manifest.mesh.keybinds.actions` cloned during graph indexing | Yes | FLOWING |
| `keyboard.rs` resolved shortcuts | `ResolvedSurfaceShortcut` | Manifest keybinds, `keyboard.surface_shortcuts` overrides, and legacy settings fallback | Yes | FLOWING |
| Docs migration tables | Static author guidance | Runtime diagnostic strings cross-checked against docs | Yes | VERIFIED |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Package diagnostics and graph regressions pass | `cargo test -p mesh-core-module package::tests` | 43 passed | PASS |
| Keybind graph preserves default/localized triggers | `cargo test -p mesh-core-module contribution_index_exposes_frontend_keybind_resource_interface_and_provider_records` | 1 passed | PASS |
| Diagnostic-focused package test passes | `cargo test -p mesh-core-module diagnostic` | 1 passed | PASS |
| Unsupported keybind modifiers are rejected | `cargo test -p mesh-core-module unsupported_modifier_is_rejected` | 2 passed | PASS |
| Shell shortcut resolution/navigation behavior passes under native deps | `nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation` | 23 passed | PASS |
| Old author-target wording is absent or migration-only | `rg` old-term sweeps across docs and tests | Hits were legacy fixtures or explicit migration guidance | PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| MIGR-01 | 40-01, 40-02 | Existing bundled modules and docs that still use legacy vocabulary or manifest shapes receive a clear migration path toward the canonical model. | SATISFIED | Runtime diagnostics provide concrete replacement/removal actions; author docs and LLM context point new authors at canonical `module.json`; old names remain only as migration guidance, vocabulary inventory, or legacy fixtures. |
| MIGR-02 | 40-03 | Paused v1.6 keybind declaration/resolution work is preserved as part of the manifest/contribution model so later keybind dispatch phases can resume without rework. | SATISFIED | Manifest keybind triggers and localized triggers round-trip, installed graph records expose them, shell resolution preserves manifest-first/user override/legacy fallback behavior. Note: `.planning/REQUIREMENTS.md` still marks MIGR-02 as Pending, but code evidence satisfies the requirement. |

No Phase 40 requirement IDs are orphaned: the plans declare MIGR-01 and MIGR-02, and `.planning/REQUIREMENTS.md` maps both to Phase 40.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `docs/llm-context.md` | 281, 320 | Existing icon-system TODO/placeholder guidance | INFO | Outside Phase 40 migration/keybind scope; not a manifest author-doc target and not a runtime stub. |
| `docs/theming/locales.md` | 86, 148 | Translation key contains `placeholder` | INFO | Legitimate locale key text, not a stub. |
| Rust files | various | Empty match arms such as `_ => {}` | INFO | Legitimate control-flow arms, not empty implementations. |

### Human Verification Required

None. This phase delivers runtime diagnostics, docs wording, and testable shortcut/data-flow behavior; no visual or external-service-only validation is required.

### Gaps Summary

No blocking gaps found. The phase goal is achieved in the codebase: migration diagnostics are explicit and tested, author documentation points to the canonical module model with replacement/removal guidance, and v1.6 keybind declaration/resolution data remains addressable through canonical manifest and contribution records.

---

_Verified: 2026-05-18T11:06:46Z_
_Verifier: the agent (gsd-verifier)_
