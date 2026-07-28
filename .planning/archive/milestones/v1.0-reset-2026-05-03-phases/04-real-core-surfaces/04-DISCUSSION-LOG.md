# Phase 4: Real Core Surfaces - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-03
**Phase:** 4-real-core-surfaces
**Areas discussed:** Surface proof, Quick settings audio, Quick settings network, Unavailable and permission-denied states

---

## Surface Proof

| Option | Description | Selected |
|--------|-------------|----------|
| Minimal live indicators | Panel shows live audio/network/power values, and opens quick settings for control. | ✓ |
| Interactive panel controls | Panel also directly changes volume/network state. | |
| You decide | Planner chooses the smallest proof that satisfies `SURF-01`. | |

**User's choice:** Minimal live indicators.
**Notes:** The top panel should prove real live service-backed display and routing into quick settings; direct service controls are not required there.

---

## Quick Settings Audio

| Option | Description | Selected |
|--------|-------------|----------|
| Full primary controls | Live percent/mute/backend label, volume slider using `audio.set_volume(...)`, plus mute/step controls. | ✓ |
| Buttons only | Keep volume up/down/mute, skip slider until later. | |
| You decide | Planner chooses the smallest control set that proves `SURF-02` and `SURF-03`. | |

**User's choice:** Full primary controls.
**Notes:** Quick settings should be the richer control surface for audio, using the finalized service proxy command path.

---

## Quick Settings Network

| Option | Description | Selected |
|--------|-------------|----------|
| Core Wi-Fi controls | Live Wi-Fi enabled state, available networks list when present, Wi-Fi on/off via `network.set_wifi_enabled(...)`, and connect/disconnect only where provider data is sufficient and safe. | ✓ |
| Full network manager surface | Scan, list, connect, disconnect, device status, and richer connection details all in Phase 4. | |
| Conservative toggle only | Live state plus Wi-Fi on/off; leave network list/connect/disconnect polish for later. | |

**User's choice:** Core Wi-Fi controls.
**Notes:** Network UI should cover the main Wi-Fi path without expanding into a full network manager suite.

---

## Unavailable and Permission-Denied States

| Option | Description | Selected |
|--------|-------------|----------|
| Visible disabled states | Show unavailable/permission-denied copy in the affected section, disable controls, and rely on diagnostics/logs for technical detail. | ✓ |
| Silent graceful fallback | Hide controls or show neutral placeholders, with errors only in diagnostics/logs. | |
| Developer-explicit errors | Show detailed provider/command failure messages directly in the UI. | |

**User's choice:** Visible disabled states.
**Notes:** The UI should be honest about unavailable or disallowed actions without turning provider failures into raw developer-facing error dumps.

---

## the agent's Discretion

- Exact visual composition for panel indicators and quick settings sections.
- Whether connect/disconnect controls appear initially or remain conditional based on provider data quality.
- Exact concise copy for disabled/unavailable states.

## Deferred Ideas

- Full network manager surface with polished scan, device detail, connection profile management, and exhaustive connect/disconnect flows.
- Direct top panel controls for audio or network state.
