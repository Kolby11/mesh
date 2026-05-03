---
phase: 04
slug: real-core-surfaces
status: approved
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-03
---

# Phase 04 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `cargo test -p mesh-core-shell` |
| **Full suite command** | `cargo test -p mesh-core-scripting && cargo test -p mesh-core-backend && cargo test -p mesh-core-shell` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run the plan's quick verification command.
- **After every plan wave:** Run `cargo test -p mesh-core-scripting && cargo test -p mesh-core-backend && cargo test -p mesh-core-shell`.
- **Before `$gsd-verify-work`:** Full suite plus static `rg` checks must be green.
- **Max feedback latency:** 120 seconds for automated checks in this phase.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 04-01-01 | 01 | 1 | SURF-03 | T-04-01 / T-04-02 | Audio command payloads use the finalized proxy/provider contract | unit/integration | `cargo test -p mesh-core-scripting && cargo test -p mesh-core-backend` | Yes | pending |
| 04-01-02 | 01 | 1 | SURF-03 | T-04-01 / T-04-02 | Providers accept normalized volume payloads without shell-specific hacks | integration | `cargo test -p mesh-core-backend` | Yes | pending |
| 04-02-01 | 02 | 2 | SURF-02, SURF-03 | T-04-03 / T-04-04 | Quick settings audio displays live state and guards unavailable controls | static/integration | `cargo test -p mesh-core-shell` | Yes | pending |
| 04-02-02 | 02 | 2 | SURF-04, SURF-05 | T-04-05 / T-04-06 | Quick settings Wi-Fi displays live state and guards unsafe commands | static/integration | `cargo test -p mesh-core-shell` | Yes | pending |
| 04-03-01 | 03 | 3 | SURF-01 | T-04-07 | Top panel stays compact and displays live service-backed indicators | integration | `cargo test -p mesh-core-shell` | Yes | pending |
| 04-03-02 | 03 | 3 | SURF-01..SURF-05 | T-04-08 | End-to-end tests prove public proxy API use and fallback copy | integration/static | `cargo test -p mesh-core-shell` | Yes | pending |

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Running shell surfaces display real host audio/network values | SURF-01, SURF-02, SURF-04 | CI may not have PipeWire/PulseAudio/NetworkManager/Wayland services | Run the shell in a dev environment with available providers; open top panel and quick settings; confirm live values and disabled fallback states. |
| Audio and Wi-Fi controls affect real system state | SURF-03, SURF-05 | Mutating host audio/network state is environment-dependent and permission-sensitive | In a controlled dev session, use quick settings volume/mute and Wi-Fi toggle; confirm provider emits updated state and UI rerenders. |

---

## Validation Sign-Off

- [x] All tasks have automated verify or explicit manual validation.
- [x] Sampling continuity: no 3 consecutive tasks without automated verify.
- [x] Wave 0 covers all missing references.
- [x] No watch-mode flags.
- [x] Feedback latency < 120s.
- [x] `nyquist_compliant: true` set in frontmatter.

**Approval:** approved 2026-05-03
