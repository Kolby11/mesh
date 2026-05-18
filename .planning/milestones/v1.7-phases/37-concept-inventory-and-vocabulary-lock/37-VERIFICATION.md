---
phase: 37-concept-inventory-and-vocabulary-lock
status: passed
verified: 2026-05-17
requirements: [CONC-01, CONC-02, CONC-03]
score: 3/3
human_verification: []
gaps: []
---

# Phase 37 Verification

## Verdict

Passed. Phase 37 achieved the goal: MESH now has a canonical module vocabulary,
an old-term inventory, author-facing hard replacement docs, and explicit
runtime/future-phase handoff guidance.

## Requirement Traceability

| Requirement | Status | Evidence |
| ----------- | ------ | -------- |
| CONC-01 | Passed | `docs/module-vocabulary.md` defines module, module kind, interface, provider, contribution, dependency, capability, resource pack, library, settings, and entrypoint with developer and end-user wording. |
| CONC-02 | Passed | `docs/module-vocabulary.md` inventories old names with replace/remove/internal-only migration dispositions; `.planning/REQUIREMENTS.md` and `.planning/ROADMAP.md` no longer target public compatibility aliases. |
| CONC-03 | Passed | `docs/module-vocabulary.md` reconciles v1.1 provider selection and v1.6 keybind declarations, then preserves them in runtime inventory and Phase 38-41 handoff rules. |

## Must-Have Checks

- `docs/module-vocabulary.md` contains the canonical principle `A module is the installable MESH unit.`
- Public naming rules state old names are replacement debt and temporary loaders are internal implementation details.
- Runtime inventory covers `ModulePackageManifest`, `RootPackageManifest`, `PackageSection`, `PackageManifestError`, `localized_triggers`, `settings.keyboard.shortcuts`, and `ModuleContributionIndex`.
- Author docs use `module.json`, link to `module-vocabulary.md`, and remove stale synonym wording.
- Backend docs state `Frontend modules never depend on backend provider modules.`
- Health docs distinguish operating-system package names from MESH module names.
- Icon docs explicitly state resolver aliases are resource lookup rules, not vocabulary compatibility aliases.
- Roadmap Phase 38 targets `module.json`; Phase 40 targets replacement/removal guidance.

## Automated Evidence

Commands run:

```bash
rg -n "A module is the installable MESH unit|Old names are replacement debt|Runtime Inventory|Future-Phase Handoff|module.json|internal-only migration loaders|typed contribution indexes|Frontend modules never depend on backend provider modules|Operating-system package names are not MESH module names|Icon resolver aliases are resource lookup rules" docs/module-vocabulary.md docs/module-system.md docs/extensibility.md docs/modules/README.md docs/modules/backend/core/README.md docs/health.md docs/theming/icons.md .planning/ROADMAP.md .planning/REQUIREMENTS.md
rg -n "explicit compatibility aliases|package.json.mesh|compatibility aliases|treat them as synonyms during the transition|Legacy package.json manifests may still use provides during migration|Semantic aliases are a compatibility layer" .planning/REQUIREMENTS.md .planning/ROADMAP.md docs/module-system.md docs/extensibility.md docs/modules/README.md docs/modules/backend/core/README.md docs/health.md docs/theming/icons.md
ls .planning/phases/37-concept-inventory-and-vocabulary-lock/*-SUMMARY.md
```

The stale-guidance grep found no deprecated synonym or loader guidance. It did
find the intentional icon sentence `Icon resolver aliases are resource lookup
rules, not vocabulary compatibility aliases.`, which is the required
clarification rather than a compatibility-alias endorsement.

## Plan Completion

| Plan | Status | Summary |
| ---- | ------ | ------- |
| 37-01 | Complete | `37-01-SUMMARY.md` |
| 37-02 | Complete | `37-02-SUMMARY.md` |
| 37-03 | Complete | `37-03-SUMMARY.md` |

## Review And Gates

- Code review: clean (`37-REVIEW.md`).
- Regression gate: skipped; no prior v1.7 verification files exist.
- Schema drift: clear.
- Codebase drift: non-blocking SDK check skipped due to local Node EPERM; no source code structure changed in this documentation phase.

## Residual Risk

No Phase 37 gaps. Runtime renames and loader behavior changes are intentionally
deferred to Phase 38-40 and tracked in the runtime inventory and handoff rules.

