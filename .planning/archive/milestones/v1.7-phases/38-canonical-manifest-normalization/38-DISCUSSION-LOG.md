# Phase 38: Canonical Manifest Normalization - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-17T20:41:41+02:00
**Phase:** 38-Canonical Manifest Normalization
**Areas discussed:** Canonical manifest shape, Migration loader behavior, Diagnostics severity, Root graph and shipped artifacts

---

## Runtime Note

The interactive AskUserQuestion gate was unavailable in this Codex runtime. Per
the workflow fallback, all high-signal gray areas were discussed with
conservative defaults derived from Phase 37's locked decisions and the current
codebase.

---

## Canonical Manifest Shape

| Option | Description | Selected |
|--------|-------------|----------|
| `module.json` with top-level `name/version` and `mesh` | Reuses the existing `ModulePackageManifest` schema while renaming the public file from package to module. | ✓ |
| Keep legacy `id/type/api_version` `module.json` | Less migration work, but blesses an old schema as canonical. | |
| Invent a third schema | Maximum churn and no codebase precedent. | |

**User's choice:** Workflow fallback selected the Phase 37-aligned option.
**Notes:** Existing `module.json` files may still be legacy and must not be mistaken for the target shape.

---

## Migration Loader Behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Prefer canonical `module.json`, keep old loaders internal with diagnostics | Matches hard replacement while preserving repo migration sequencing. | ✓ |
| Treat old names as compatibility aliases | Rejected by Phase 37. | |
| Remove all legacy loaders immediately | Clean target, but risks breaking shipped artifacts before they are migrated. | |

**User's choice:** Workflow fallback selected the Phase 37-aligned option.
**Notes:** Ambiguous multiple manifest files should be blocking; old single manifest files can load only as internal migration paths.

---

## Diagnostics Severity

| Option | Description | Selected |
|--------|-------------|----------|
| Blocking ambiguity errors plus migration warnings | Strict where data conflicts; practical where migration sequencing needs old inputs. | ✓ |
| Warnings only | Risks silently accepting ambiguous manifests. | |
| Silent normalization | Conflicts with Phase 37 diagnostic goals. | |

**User's choice:** Workflow fallback selected strict ambiguity plus actionable migration warnings.
**Notes:** Diagnostics should say `replace with` or `remove`, not `alias`.

---

## Root Graph And Shipped Artifacts

| Option | Description | Selected |
|--------|-------------|----------|
| Migrate checked-in root graph and module manifests now | Makes Phase 38 prove the canonical model in repo fixtures. | ✓ |
| Only add loaders | Leaves public examples and runtime fixtures stale. | |
| Defer root graph | Keeps `config/package.json` old language in the central installed graph. | |

**User's choice:** Workflow fallback selected migration now, with internal loaders only where needed.
**Notes:** Preserve active providers, layout entrypoint, v1.1 provider behavior, and v1.6 keybind data.

## Planner Discretion

- Planner may split type renames, loader behavior, shipped artifact migration, and diagnostics into separate plans.
- Planner should choose test scope based on touched runtime paths.

## Deferred Ideas

- Typed contribution indexing belongs to Phase 39.
- Broad docs/examples migration belongs to Phase 40.
- Shipped proof belongs to Phase 41.
