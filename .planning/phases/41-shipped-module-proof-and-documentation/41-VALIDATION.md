---
phase: 41
slug: shipped-module-proof-and-documentation
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-18
---

# Phase 41 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust cargo tests plus grep documentation checks |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-module shipped` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-module package::tests && nix develop -c cargo test -p mesh-core-shell shell::tests` |
| **Estimated runtime** | ~120 seconds focused; workspace suite longer |

## Sampling Rate

- **After every task commit:** Run the task's focused verify command.
- **After every plan wave:** Run the focused package or shell test command for
  that wave.
- **Before `$gsd-verify-work`:** Run the full suite command and docs grep gate.
- **Max feedback latency:** 180 seconds for focused checks.

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 41-01-01 | 01 | 1 | PROOF-01 | T-41-01-01 | Shipped root graph includes canonical interface and icon-pack modules | unit | `nix develop -c cargo test -p mesh-core-module shipped_module` | yes | pending |
| 41-01-02 | 01 | 1 | PROOF-01 | T-41-01-02 | Shipped root graph proves canonical manifests and selected provider without fake parsing | unit | `nix develop -c cargo test -p mesh-core-module shipped_module` | yes | pending |
| 41-01-03 | 01 | 1 | PROOF-01 | T-41-01-03 | Missing/incompatible proof-path resources remain visible as diagnostics | unit | `nix develop -c cargo test -p mesh-core-module shipped_module_diagnostics` | yes | pending |
| 41-02-01 | 02 | 2 | PROOF-01 | T-41-02-01 | Shell runtime consumes graph provider records and does not infer service powers from provider identity | unit | `nix develop -c cargo test -p mesh-core-shell shell::tests installed_module_graph` | yes | pending |
| 41-02-02 | 02 | 2 | PROOF-01 | T-41-02-02 | Shipped navigation behavior remains manifest/interface driven | unit | `nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation` | yes | pending |
| 41-03-01 | 03 | 3 | PROOF-01 | T-41-03-01 | Author docs teach canonical workflow and strict vocabulary | grep | `rg -n "extend or add a MESH module|@mesh/navigation-bar|@mesh/audio-interface|@mesh/pipewire-audio|module.json|mesh.kind|mesh.implements|mesh.keybinds|diagnostics" docs/module-system.md docs/modules/frontend/core/navigation-bar/README.md docs/modules/backend/core/pipewire-audio/README.md docs/modules/backend/core/pulseaudio-audio/README.md docs/settings/README.md docs/llm-context.md` | yes | pending |

## Wave 0 Requirements

Existing Rust and documentation infrastructure covers all phase requirements.

## Manual-Only Verifications

All phase behaviors have automated or grep-based verification.

## Validation Sign-Off

- [x] All tasks have `<verify>` commands.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency target < 180s for focused checks.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** pending
