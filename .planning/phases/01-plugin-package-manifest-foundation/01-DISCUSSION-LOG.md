# Phase 1: Plugin Package Manifest Foundation - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-03
**Phase:** 1-Plugin Package Manifest Foundation
**Areas discussed:** Provider activation policy, package installation pivot, existing-manifest relationship, ~/.mesh layout and module naming

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

---

## Existing-Manifest Relationship

| Option | Description | Selected |
|--------|-------------|----------|
| Installed state references plugin manifests | The shell-owned package manifest records installed/enabled selections and provider choices while existing `plugin.json` files remain the source for metadata, dependencies, capabilities, entrypoints, and `provides`. | ✓ |
| Installed state duplicates full plugin metadata | The package manifest copies every plugin's metadata into one central file. Simpler to inspect, but risks drift from plugin-owned manifests. | |
| Replace plugin manifests with package manifest | Move package metadata into the installed package file. Centralized, but breaks the existing plugin model and plugin-owned distribution shape. | |

**User's choice:** No new interactive answer was available in this rerun; this preserves the prior package-manifest decision and applies codebase scout findings.
**Notes:** Existing code already normalizes `plugin.json`/`mesh.toml` into `Manifest`, with `DependenciesSection` and `ProvidedInterface` covering the concepts this phase needs. The context now tells downstream agents to build the installed package graph from shell-owned installed state plus each referenced plugin's normalized manifest.

---

## ~/.mesh Layout and Module Naming

| Option | Description | Selected |
|--------|-------------|----------|
| Adopt `~/.mesh` + module terminology | User-owned shell files live in `~/.mesh`, installed extensions are called modules, central installed state is `~/.mesh/package.json`, installed modules live in `~/.mesh/modules/`, and each module has its own `package.json`. | ✓ |
| Keep current plugin naming and paths | Continue with `plugin.json`, `packages/plugins`, and documented `~/.config/mesh` naming. Less migration work, but conflicts with the user's preferred mental model. | |
| Hybrid only in docs | Use "module" in docs but keep package/config schema named around plugins. Lower immediate cost, but risks long-term terminology drift. | |

**User's choice:** Adopt `~/.mesh` + module terminology.
**Notes:** The user specified `~/.mesh/package.json` with fields such as `frontendDependencies`, `backendDependencies`, `icons`, `fonts`, and i18n support; `~/.mesh/modules/`; module-level `package.json`; `~/.mesh/settings.json`; a folder for switchable color themes; Git origin metadata on modules; and an entrypoint for defining the base shell layout from installed modules. Actual Git download/install remains deferred by Phase 1 scope, but origin metadata should be represented.

## the agent's Discretion

- Choose Rust module boundaries during planning; the central shell manifest filename is locked as `~/.mesh/package.json`.
- Keep schema minimal while preserving the package graph concepts the user locked.
- Choose exact validation layering, but invalid installed package entries should become typed package-graph errors suitable for later diagnostics rather than silent skips.
- Decide how to bridge existing `plugin.json`/`Plugin*` code to the new module/package naming without over-expanding Phase 1.

## Deferred Ideas

- Remote package download, Git clone/fetch/install behavior, registry dependency fetching, signing, sandboxing, marketplace UX, and full hot-install flows.
