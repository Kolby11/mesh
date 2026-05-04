# Phase 5: Backend Diagnostics and MVP Proof - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-04
**Phase:** 05-backend-diagnostics-and-mvp-proof
**Areas discussed:** Reference plugin shape, Failure visibility contract, Diagnostic dedup and escalation, Proof scope

---

## Reference Plugin Shape

| Option | Description | Selected |
|--------|-------------|----------|
| Brand-new minimal provider | A fresh backend plugin built specifically as the reference path. Cleanest proof, least legacy baggage. | ✓ |
| Upgrade `mock-notifications` | Fastest path to a clearly fake/demo-oriented proof plugin, but weaker as a real-service example. | |
| Upgrade `mpris-media` | More realistic than a fake demo plugin, but brings in more external behavior and placeholder baggage. | |
| Another existing provider | Reuse a different current backend plugin as the proof target. | |

**User's choice:** Brand-new minimal provider
**Notes:** The user wants the proof to use a fresh plugin instead of retrofitting an existing placeholder or legacy provider.

---

## Failure Visibility Contract

| Option | Description | Selected |
|--------|-------------|----------|
| Keep last good state, mark provider degraded | Least disruptive to consumers, but stale data remains visible. | |
| Clear public state to unavailable/error immediately | Honest runtime state; consumers stop seeing stale data once failure is known. | ✓ |
| Split behavior by failure type | For example, load/init clears immediately while poll keeps last state until threshold. | |
| Custom rule | User-defined failure visibility model. | |

**User's choice:** Clear public state to unavailable/error immediately
**Notes:** The user prefers honest runtime state over continuity from stale last-known-good state.

---

## Diagnostic Dedup and Escalation

| Option | Description | Selected |
|--------|-------------|----------|
| Dedup by provider + stage + message | Simple and close to the current diagnostics shape. | |
| Dedup by provider + stage, with count/timestamp updates | Groups similar repeated failures without creating new entries every cycle. | ✓ |
| Dedup by provider + stage, then escalate after threshold | Adds a stronger escalation rule after repeated failures. | |
| Custom rule | User-defined dedup/escalation policy. | |

**User's choice:** Dedup by provider + stage, with count/timestamp updates
**Notes:** The user explicitly chose count/timestamp rollups so similar repeated failures stay visible without spamming by message variant.

---

## Proof Scope

| Option | Description | Selected |
|--------|-------------|----------|
| Minimal proof | One fresh plugin, one command path, core tests, and a short note. | |
| Standard proof | One fresh plugin, happy-path plus failure-path tests, and a concise author note. | |
| Broad proof | Fresh plugin, stronger lifecycle/diagnostic matrix, stronger docs, and more than one behavioral path. | ✓ |
| Custom scope | User-defined proof bar. | |

**User's choice:** Broad proof
**Notes:** The user wants stronger lifecycle/diagnostic coverage and stronger docs than a minimal smoke-test proof.

---

## the agent's Discretion

- Exact reference provider domain
- Exact unavailable/error public state shape
- Exact count/timestamp storage model for provider-plus-stage diagnostic buckets
- Exact test/doc artifact mix needed to satisfy the broad proof bar

## Deferred Ideas

- WiFi and Bluetooth should live as a module/interface concern, but that design discussion was explicitly deferred to a later phase.
