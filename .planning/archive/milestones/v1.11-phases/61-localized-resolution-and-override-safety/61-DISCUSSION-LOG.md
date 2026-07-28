# Phase 61: Localized Resolution And Override Safety - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-23
**Phase:** 61-localized-resolution-and-override-safety
**Areas discussed:** resolution order, override semantics, localized trigger scope, legacy fallback

---

## Resolution Order

| Option | Description | Selected |
|--------|-------------|----------|
| Preserve v1.6/v1.7 order | User override, exact locale, parent locale, generic manifest trigger, then no binding. | yes |
| Re-plan precedence | Reopen the precedence model and allow implementation to choose a different order. | |

**User's choice:** Inferred from locked milestone requirements and prior decisions.
**Notes:** KRES-01 explicitly defines the order and no user-facing discussion was needed.

---

## Override Semantics

| Option | Description | Selected |
|--------|-------------|----------|
| Existing action only | Overrides are keyed by surface id and action id, and cannot create declarations. | yes |
| Settings as declarations | Let settings introduce new actions. | |

**User's choice:** Inferred from KRES-02 and prior manifest-first decisions.
**Notes:** Settings remain override data or compatibility fallback only.

---

## Localized Trigger Scope

| Option | Description | Selected |
|--------|-------------|----------|
| Access keys only | Localized defaults apply to `access_key` actions; shortcut actions keep generic defaults unless user override exists. | yes |
| All trigger kinds | Let localized defaults override shortcut actions too. | |

**User's choice:** Inferred from KRES-03.
**Notes:** Existing tests may need to be updated if they still encode localized shortcut defaults.

---

## Legacy Fallback

| Option | Description | Selected |
|--------|-------------|----------|
| Missing manifest only | Legacy `settings.keyboard.shortcuts` is used only when no canonical manifest action exists for the same id. | yes |
| Merge all settings | Merge settings declarations into manifest actions even when canonical declarations exist. | |

**User's choice:** Inferred from KRES-04 and Phase 60 decisions.
**Notes:** Compatibility should be preserved without making settings canonical.

---

## the agent's Discretion

- Exact helper boundaries and test organization are left to the planner.
- The planner may decide whether to enforce localized access-key scope during declaration construction or during resolution.

## Deferred Ideas

- Conflict diagnostics, invalid declaration diagnostics, accessibility/debug metadata, settings UI, and compositor-global shortcuts stay out of Phase 61.
