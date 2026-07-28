---
phase: 40
slug: migration-diagnostics-and-author-docs
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-18
---

# Phase 40 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust cargo tests plus grep documentation checks |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `cargo test -p mesh-core-module package::tests` |
| **Full suite command** | `cargo test -p mesh-core-module package::tests && cargo test -p mesh-core-shell shell::component::tests::interaction::navigation` |
| **Estimated runtime** | ~90 seconds focused; workspace suite longer |

## Sampling Rate

- **After every task commit:** Run the task's focused verify command.
- **After every plan wave:** Run `cargo test -p mesh-core-module package::tests`.
- **Before `$gsd-verify-work`:** Run the full suite command and the docs grep checks from the relevant plans.
- **Max feedback latency:** 120 seconds for focused checks.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 40-01-01 | 01 | 1 | MIGR-01 | T-40-01-01 | Legacy manifest inputs surface warning diagnostics, unsupported inputs surface blocking errors | unit | `cargo test -p mesh-core-module package::tests manifest` | yes | pending |
| 40-01-02 | 01 | 1 | MIGR-01 | T-40-01-02 | Docs name replacement/removal guidance without public alias wording | grep | `rg -n "Migration Diagnostics|replace package.json with module.json|remove plugin.json" docs/module-system.md docs/module-vocabulary.md` | yes | pending |
| 40-01-03 | 01 | 1 | MIGR-01 | T-40-01-03 | Diagnostic regression tests verify severity and suggested action | unit | `cargo test -p mesh-core-module package::tests diagnostic` | yes | pending |
| 40-02-01 | 02 | 2 | MIGR-01 | T-40-02-01 | Installation docs teach canonical `module.json` schema, not old top-level shape | grep | `rg -n "module.json|mesh.apiVersion|mesh.kind|internal migration" docs/installation.md` | yes | pending |
| 40-02-02 | 02 | 2 | MIGR-01 | T-40-02-02 | Theming, font, locale, and settings docs use canonical module wording | grep | `rg -n "module.json|mesh.kind|mesh.contributes" docs/font-system.md docs/theming/themes.md docs/theming/locales.md docs/settings/README.md` | yes | pending |
| 40-02-03 | 02 | 2 | MIGR-01 | T-40-02-03 | LLM context does not teach stale `package.json` authoring as the target model | grep | `rg -n "module.json|canonical author-facing manifest" docs/llm-context.md` | yes | pending |
| 40-03-01 | 03 | 2 | MIGR-02 | T-40-03-01 | Typed installed-graph keybind records expose trigger and localized trigger data | unit | `cargo test -p mesh-core-module package::tests keybind` | yes | pending |
| 40-03-02 | 03 | 2 | MIGR-02 | T-40-03-02 | Shell shortcut resolution preserves manifest declaration plus user override behavior | unit | `cargo test -p mesh-core-shell shell::component::tests::interaction::navigation` | yes | pending |
| 40-03-03 | 03 | 2 | MIGR-02 | T-40-03-03 | Docs describe `settings.keyboard.surface_shortcuts` as overrides, not canonical declarations | grep | `rg -n "surface_shortcuts|manifest keybind|legacy settings fallback" docs/settings/README.md docs/module-system.md` | yes | pending |

## Wave 0 Requirements

Existing Rust and documentation infrastructure covers all phase requirements.

## Manual-Only Verifications

All phase behaviors have automated or grep-based verification.

## Validation Sign-Off

- [x] All tasks have `<verify>` commands.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency target < 120s for focused checks.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** pending

