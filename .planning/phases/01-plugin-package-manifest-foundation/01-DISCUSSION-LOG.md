# Phase 1: Plugin Package Manifest Foundation - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-03
**Phase:** 1-Plugin Package Manifest Foundation
**Areas discussed:** Provider activation policy, package installation pivot

---

## Provider Activation Policy

| Option | Description | Selected |
|--------|-------------|----------|
| Single active provider per service | Pick highest priority and run only that provider. Simple MVP default but less user control. | |
| Run all enabled providers | Every provider runs and one is selected as primary. Flexible but lifecycle-heavy. | |
| Manual provider selection | Require config to choose the provider. Predictable but more setup. | |
| Hybrid | Highest priority by default; explicit config can override. | ✓ |

**User's choice:** Hybrid.
**Notes:** The user then clarified that provider selection should ultimately be grounded in a central package/install manifest rather than being treated as a lifecycle-only detail.

---

## Package Installation Pivot

| Option | Description | Selected |
|--------|-------------|----------|
| Keep package installation as side note | Capture as future direction while continuing backend lifecycle Phase 1. | |
| Make package manifest Phase 1 | Reorder active milestone so unified plugin installation/package graph is implemented first. | ✓ |

**User's choice:** Make package installation the first implementation focus.
**Notes:** The user wants a shell-owned package.json-like manifest that records user-specified frontend plugins, backend plugins, frontend-to-backend dependencies, backend categories, and active provider choices. Backend plugins should be installable directly for backend-only categories such as shortcuts, though most backend plugins will arrive as frontend dependencies.

## the agent's Discretion

- Choose exact manifest filename and Rust module boundaries during planning.
- Keep schema minimal while preserving the package graph concepts the user locked.

## Deferred Ideas

- Remote package download, registry dependency fetching, signing, sandboxing, marketplace UX, and full hot-install flows.
