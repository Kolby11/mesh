# Phase 33: Locale-Aware Keybind Resolution - Context

**Gathered:** 2026-05-13T22:07:21+02:00
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 33 adds locale-aware resolution for the keybind declarations created in Phase 32. The resolver should produce effective trigger choices from stable module action ids, user overrides, active locale, parent locale, and generic defaults. It should not expand dispatch payloads, implement conflict diagnostics, expose accessibility metadata, add a keybind settings UI, or add compositor-global shortcuts.

</domain>

<decisions>
## Implementation Decisions

### Locale Schema
- **D-01:** Locale-specific bindings live inside each keybind action as `localized_triggers`.
- **D-02:** Locale keys are locale identifiers such as `en`, `sk`, and later regional variants such as `sk-SK`.
- **D-03:** Localized entries override trigger data only. They must not override `handler`, `target_ref`, `scope`, `label`, or `label_i18n_key`.

### Fallback Rules
- **D-04:** Resolver precedence is: user override by `surface_id + action_id`, exact active locale trigger, parent locale trigger, generic action trigger, then no binding.
- **D-05:** Parent locale fallback should be implemented for regional active locales, for example `sk-SK` may use a `sk` localized trigger when `sk-SK` is not declared.
- **D-06:** Missing, incomplete, or blank localized trigger entries silently fall back in Phase 33. Visible diagnostics for malformed locale bindings are deferred to Phase 35.

### Shortcut vs Access Key
- **D-07:** Phase 33 should localize access keys only. Regular shortcut localization is out of scope for this phase to avoid surprising users by changing muscle-memory shortcuts across locales.
- **D-08:** Existing generic shortcut defaults and user overrides remain supported. Locale-specific shortcut defaults should not be introduced as a Phase 33 proof target.

### the agent's Discretion
- The planner may choose the exact Rust module/file placement for resolver types, as long as it reuses the Phase 32 manifest model and shell keyboard bridge patterns.
- The planner may decide whether parent-locale expansion lives in `mesh-core-locale`, shell keyboard code, or a small keybind resolver helper, provided the public behavior matches D-04 and D-05.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Scope
- `.planning/ROADMAP.md` - Phase 33 goal, planned work, success criteria, and milestone boundaries.
- `.planning/REQUIREMENTS.md` - `LOCL-01`, `LOCL-02`, and `LOCL-03` requirements.
- `.planning/STATE.md` - Prior milestone and Phase 32 decisions that constrain this phase.
- `.planning/phases/32-keybind-declaration-contract/32-01-SUMMARY.md` - What Phase 32 delivered and intentionally deferred.

### Existing Code
- `crates/core/extension/module/src/manifest/model.rs` - Current typed keybind declarations and validation.
- `crates/core/shell/src/shell/component/input/keyboard.rs` - Current surface shortcut resolution and user override bridge.
- `crates/core/foundation/locale/src/lib.rs` - Locale engine, active locale, translation fallback chain, and existing fallback behavior.
- `crates/core/foundation/config/src/lib.rs` - `KeyboardSettings.surface_shortcuts` override model.
- `modules/frontend/navigation-bar/module.json` - Shipped surface keybind declaration proof from Phase 32.
- `modules/frontend/navigation-bar/config/settings.json` - Current legacy settings shortcut metadata.

### Research Basis
- `.planning/research/STACK.md` - Project stack and module system notes.
- `.planning/research/FEATURES.md` - v1.6 keybind feature research.
- `.planning/research/ARCHITECTURE.md` - v1.6 keybind architecture research.
- `.planning/research/PITFALLS.md` - v1.6 keybind pitfalls.
- `.planning/research/SUMMARY.md` - Synthesized research for localized keybind management.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `KeybindAction`, `KeybindTrigger`, `KeybindScope`, and `KeybindTriggerKind` already exist in `crates/core/extension/module/src/manifest/model.rs`; Phase 33 should extend this model rather than create a parallel settings-only schema.
- `LocaleEngine::current()` and `LocaleEngine::fallback_chain()` already expose active locale state and translation fallback data; Phase 33 can reuse or complement this for keybind locale fallback.
- `KeyboardSettings.surface_shortcuts` already stores stable user overrides by surface id and action id; this remains the highest-precedence resolver input.

### Established Patterns
- Manifest parsing and validation are covered in `mesh-core-module` tests; new `localized_triggers` parsing should follow the Phase 32 keybind tests.
- Shell runtime shortcut behavior is tested in `crates/core/shell/src/shell/component/tests/interaction/navigation.rs`; resolver tests should preserve current navigation-bar behavior.
- The shell test suite needs `nix develop` for native Wayland dependencies such as `xkbcommon`.

### Integration Points
- `FrontendSurfaceComponent::resolved_surface_shortcuts` is the current runtime integration point for effective surface shortcuts.
- Locale changes flow through `Shell::mark_components_locale_changed` and `FrontendSurfaceComponent::locale_changed`; resolver behavior must be able to reflect the active locale after this path runs.
- Module i18n settings can currently set a frontend component locale through `settings_json.i18n.default_locale`; this should be considered when choosing resolver input.

</code_context>

<specifics>
## Specific Ideas

- The user specifically wants Microsoft-style localized access key behavior, for example English `Accept -> A` and Slovak `Prijat -> P`.
- The resolver should be deterministic and testable with exact precedence assertions.
- Stable action ids remain the identity for overrides and should never depend on translated display text.

</specifics>

<deferred>
## Deferred Ideas

- Locale-specific regular shortcut defaults are deferred beyond Phase 33.
- Conflict diagnostics for duplicate or malformed locale bindings remain Phase 35.
- Expanded script dispatch payloads with action id, locale, target metadata, and resolved label remain Phase 34.
- Accessibility metadata and shipped-surface proof remain Phase 36.

</deferred>

---

*Phase: 33-Locale-Aware Keybind Resolution*
*Context gathered: 2026-05-13T22:07:21+02:00*
