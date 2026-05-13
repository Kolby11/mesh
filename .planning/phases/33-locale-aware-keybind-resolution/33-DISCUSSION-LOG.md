# Phase 33: Locale-Aware Keybind Resolution - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md - this log preserves the alternatives considered.

**Date:** 2026-05-13T22:07:21+02:00
**Phase:** 33-Locale-Aware Keybind Resolution
**Areas discussed:** Locale schema, Fallback rules, Shortcut vs access key

---

## Locale Schema

| Option | Description | Selected |
|--------|-------------|----------|
| Per-action `localized_triggers` map | Keeps localized trigger defaults next to the stable action declaration. | yes |
| Separate top-level locale table | Cleaner for large locale packs, but more structure than this phase needs. | |
| Use translation files only | Avoids manifest growth, but weakens validation and collision handling. | |

**User's choice:** Per-action `localized_triggers` map.
**Notes:** Localized entries override trigger only. Handler, target, scope, label, and label_i18n_key remain stable per action.

---

## Fallback Rules

| Option | Description | Selected |
|--------|-------------|----------|
| Exact plus parent fallback | User override wins, then exact locale, parent locale, generic trigger, then no binding. | yes |
| Exact only | Simpler, but `sk-SK` would not use a declared `sk` binding. | |
| Use current `LocaleEngine.fallback_chain` only | Centralizes fallback, but current behavior is translation-focused. | |

**User's choice:** Exact plus parent fallback.
**Notes:** Blank, missing, or malformed localized trigger entries silently fall back during Phase 33; diagnostics are deferred to Phase 35.

---

## Shortcut vs Access Key

| Option | Description | Selected |
|--------|-------------|----------|
| Localize access keys only | Matches Microsoft-style `Accept -> A`, Slovak `Prijat -> P`; avoids changing muscle-memory shortcuts. | yes |
| Localize both access keys and shortcuts | More flexible, but higher risk of surprising users across locales. | |
| Same mechanism for both with docs warning | Resolver supports both, but author guidance discourages shortcut localization. | |

**User's choice:** Localize access keys only.
**Notes:** Existing generic shortcuts and user overrides remain supported. Locale-specific regular shortcut defaults are out of scope for Phase 33.

---

## the agent's Discretion

- Exact Rust file/module placement for the resolver.
- Whether parent-locale expansion is implemented in `mesh-core-locale`, shell keyboard code, or a small keybind resolver helper.

## Deferred Ideas

- Locale-specific regular shortcut defaults.
- Conflict diagnostics for duplicate or malformed locale bindings.
- Expanded dispatch payloads and resolved labels.
- Accessibility metadata proof.
