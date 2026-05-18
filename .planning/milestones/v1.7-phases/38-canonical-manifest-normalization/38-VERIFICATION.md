---
phase: 38-canonical-manifest-normalization
status: passed
verified: 2026-05-17
requirements: [MAN-01, MAN-02, MAN-03]
score: 4/4
human_verification: []
---

# Phase 38 Verification: Canonical Manifest Normalization

## Goal

Align manifest parsing and runtime manifest normalization around the canonical
`module.json` contract while preserving existing behavior.

## Result

Passed. Phase 38 delivers the canonical `module.json` runtime path, internal-only legacy migration loaders, shipped root/module fixture migration, and actionable diagnostics for deprecated, duplicate, ambiguous, or unsupported manifest forms.

## Requirement Coverage

| Requirement | Status | Evidence |
|-------------|--------|----------|
| MAN-01 | Passed | `.planning/REQUIREMENTS.md` and `docs/module-system.md` now document canonical `module.json plus mesh`; Rust exposes `ModuleManifest`, `RootModuleGraphManifest`, `ModuleManifestError`, and `ModuleSection`. |
| MAN-02 | Passed | Legacy `package.json`, old `module.json`, and `mesh.toml` load through explicit migration sources; provider declarations and keybind data are covered by package and manifest tests. |
| MAN-03 | Passed | `ModuleManifestDiagnostic` carries severity, path, module id, field path, message, and suggested action; tests cover ambiguity, `plugin.json`, and replacement wording. |

## Must-Haves

| Check | Status |
|-------|--------|
| Canonical `module.json` parses as the target schema | Passed |
| Legacy forms are migration inputs, not public aliases | Passed |
| Duplicate manifest files are blocking | Passed |
| `plugin.json` is rejected with remove/replace guidance | Passed |
| Root graph loads from `config/module.json` | Passed |
| Active provider `@mesh/pipewire-audio` survives graph loading | Passed |
| Navigation-bar keybind and layout data survive canonical migration | Passed |
| OS package-manager names are not renamed | Passed |

## Automated Checks

- `cargo fmt --all -- --check` passed.
- `cargo test -p mesh-core-module package::tests` passed.
- `cargo test -p mesh-core-module manifest::tests` passed.
- `nix develop -c cargo test -p mesh-core-shell shell::tests::backend_lifecycle_uses_explicit_active_provider_from_package_graph -- --exact` passed.
- `nix develop -c cargo test -p mesh-core-shell shell::tests::load_frontend_components_keeps_shell_shipped_debug_inspector_even_when_not_in_package_graph -- --exact` passed.
- `gsd-sdk query verify.schema-drift "38"` reported no schema drift.

## Residual Risk

The full shell suite still reports two pointer-focus failures:

- `shell::tests::pointer_click_claims_keyboard_owner_without_forcing_exclusive_mode`
- `shell::tests::pointer_click_after_transfer_clears_transfer_forced_exclusive_override`

These failures are outside the manifest normalization path. Focused shell tests that exercise the migrated root graph and shipped module graph passed.

## Review

Advisory code review: clean. See `38-REVIEW.md`.

## Verdict

Phase 38 satisfies its roadmap goal and mapped requirements.

