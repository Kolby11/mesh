---
phase: 01
slug: plugin-package-manifest-foundation
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-05-03
updated: 2026-05-03
---

# Phase 01 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | Cargo workspace |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-plugin module_package` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-plugin -p mesh-core-shell installed_module_graph` |
| **Estimated runtime** | ~20 seconds |

## Sampling Rate

- **After every task commit:** Run the task-specific `nix develop -c cargo test ...` command in the plan.
- **After every plan wave:** Run `nix develop -c cargo test -p mesh-core-plugin -p mesh-core-shell installed_module_graph`.
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 30 seconds.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 1 | PINST-01 | T-01-01, T-01-02 | parses root `~/.mesh/package.json` shape and rejects unsafe `MESH_HOME` | unit | `nix develop -c cargo test -p mesh-core-plugin module_package_paths module_root_manifest` | yes | pending |
| 01-01-02 | 01 | 1 | PINST-03, PINST-06 | T-01-03, T-01-04 | parses module `package.json`, stores Git origin metadata, and avoids downloader behavior | unit/static | `nix develop -c cargo test -p mesh-core-plugin module_package_manifest` | yes | pending |
| 01-01-03 | 01 | 1 | PINST-01 | T-01-03 | prefers `package.json` while preserving `plugin.json` compatibility | unit | `nix develop -c cargo test -p mesh-core-plugin module_manifest_loader` | yes | pending |
| 01-02-01 | 02 | 1 | PINST-05 | T-01-07 | derives module-kind views from one canonical module map | unit | `nix develop -c cargo test -p mesh-core-plugin installed_module_graph_kind_views` | yes | pending |
| 01-02-02 | 02 | 1 | PINST-02, PINST-03, PINST-04 | T-01-05, T-01-06, T-01-08 | exposes frontend backend requirements, multiple providers, active provider, and priority fallback | unit | `nix develop -c cargo test -p mesh-core-plugin installed_module_graph_providers` | yes | pending |
| 01-02-03 | 02 | 1 | PINST-05 | T-01-07 | indexes layout, theme, icon, font, i18n, and settings contributions without path escape | unit | `nix develop -c cargo test -p mesh-core-plugin installed_module_graph_contributions` | yes | pending |
| 01-03-01 | 03 | 2 | PINST-01, PINST-06 | T-01-10 | repo fixtures mirror the `~/.mesh` package layout and contain no remote installer behavior | static | `grep -R "\"@mesh/pipewire-audio\"\\|\"@mesh/panel:main\"\\|\"mesh.audio\"" config/package.json config/modules` | yes | pending |
| 01-03-02 | 03 | 2 | PINST-04, PINST-05 | T-01-09, T-01-11 | shell can load graph choices without rewriting lifecycle spawning | integration unit | `nix develop -c cargo test -p mesh-core-plugin -p mesh-core-shell installed_module_graph` | yes | pending |
| 01-03-03 | 03 | 2 | PINST-06 | T-01-12 | docs and path helpers present `~/.mesh/settings.json` and `~/.mesh/themes/` as primary user paths | static | `grep -R "~/.mesh/settings.json\\|~/.mesh/themes\\|mesh.contributes.themes" docs/settings/README.md docs/theming/themes.md crates/core/foundation/config/src/lib.rs crates/core/foundation/theme/src/lib.rs` | yes | pending |

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

## Manual-Only Verifications

All phase behaviors have automated verification.

## Validation Sign-Off

- [x] All tasks have automated verify commands or static checks
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all missing references
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-05-03
