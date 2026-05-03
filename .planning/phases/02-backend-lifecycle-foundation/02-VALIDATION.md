---
phase: 02
slug: backend-lifecycle-foundation
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-03
---

# Phase 02 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness with Tokio tests |
| **Config file** | `Cargo.toml` workspace and package-level `Cargo.toml` files |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` |
| **Full suite command** | `nix develop -c cargo test -p mesh-core-plugin installed_module_graph && nix develop -c cargo test -p mesh-core-shell backend_lifecycle && nix develop -c cargo test -p mesh-core-backend spawn_backend_service && nix develop -c cargo test -p mesh-core-scripting backend` |
| **Estimated runtime** | ~90 seconds |

---

## Sampling Rate

- **After every task commit:** Run the plan-local quick command.
- **After every plan wave:** Run the full suite command above.
- **Before `$gsd-verify-work`:** Full suite must be green.
- **Max feedback latency:** 90 seconds.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 02-01-01 | 01 | 1 | BPLUG-01 | T-02-01 | Invalid manifests never launch | unit | `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` | yes | pending |
| 02-01-02 | 01 | 1 | BPLUG-02 | T-02-02 | Explicit active provider only | unit | `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` | yes | pending |
| 02-01-03 | 01 | 1 | BPLUG-02 | T-02-03 | Disabled providers are excluded | unit | `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` | yes | pending |
| 02-02-01 | 02 | 1 | BPLUG-03 | T-02-04 | `init()` gates poll and commands | async unit | `nix develop -c cargo test -p mesh-core-backend spawn_backend_service` | yes | pending |
| 02-02-02 | 02 | 1 | BPLUG-04 | T-02-05 | Poll failure threshold stops runtime | async unit | `nix develop -c cargo test -p mesh-core-backend spawn_backend_service` | yes | pending |
| 02-02-03 | 02 | 1 | BPLUG-04 | T-02-06 | Poll interval changes remain honored | async unit | `nix develop -c cargo test -p mesh-core-backend spawn_backend_service` | yes | pending |
| 02-03-01 | 03 | 2 | BPLUG-05 | T-02-07 | Runtime replacement closes old receivers | unit | `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` | yes | pending |
| 02-03-02 | 03 | 2 | BPLUG-05 | T-02-08 | Handler insertion follows successful validation | unit | `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` | yes | pending |
| 02-04-01 | 04 | 2 | BPLUG-01..BPLUG-05 | T-02-09 | Lifecycle diagnostics dedupe repeated failures | unit | `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` | yes | pending |
| 02-04-02 | 04 | 2 | BPLUG-01..BPLUG-05 | T-02-10 | Status exposes lifecycle stage and provider identity | unit | `nix develop -c cargo test -p mesh-core-shell backend_lifecycle` | yes | pending |

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. No new test framework or harness installation is required.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live shell startup with real PipeWire/PulseAudio availability | BPLUG-02 | Depends on host system binaries and running services | After automated tests pass, run shell startup in the normal dev environment and inspect logs/status for exactly one active `mesh.audio` provider. |

---

## Validation Sign-Off

- [x] All tasks have automated verify commands or existing Wave 0 infrastructure.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all MISSING references.
- [x] No watch-mode flags.
- [x] Feedback latency target is under 90 seconds for focused runs.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-03
