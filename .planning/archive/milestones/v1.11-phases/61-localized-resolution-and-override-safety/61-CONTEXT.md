# Phase 61: Localized Resolution And Override Safety - Context

**Gathered:** 2026-05-23
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 61 makes focused-surface keybind resolution deterministic and safe across user overrides, localized defaults, parent locale fallback, generic manifest triggers, and legacy fallback declarations.

This phase owns resolution semantics and focused tests around effective binding choice. It does not own new conflict diagnostics, invalid declaration reporting, debug/profiling metadata, accessibility surfacing beyond existing annotations, compositor-global shortcuts, or a keybind settings UI. Those are Phase 62, Phase 63, or future-milestone work.

</domain>

<decisions>
## Implementation Decisions

### Resolution Order
- **D-01:** Effective binding precedence is locked as: user override, exact active locale, parent locale, generic manifest trigger, then no binding.
- **D-02:** Parent locale fallback should only happen after exact locale lookup misses or yields no usable key. Locale normalization may keep the existing underscore-to-hyphen behavior.
- **D-03:** Blank localized trigger keys mean "not usable for this locale" and should fall through to the next candidate instead of becoming an empty binding.

### Override Semantics
- **D-04:** User overrides remain keyed by surface id and stable action id through `keyboard.surface_shortcuts`. Overrides cannot create a keybind action that has no canonical manifest declaration or legacy fallback declaration.
- **D-05:** A user override replaces only effective trigger key data for an existing action. It must not replace the action id, subscriber target, label, category, description, or handler path.
- **D-06:** Override source metadata should remain visible as `KeybindResolutionSource::UserOverride` and `event.keybind.source = "user_override"`.

### Localized Trigger Scope
- **D-07:** Localized defaults are intended for `access_key` actions. Shortcut actions should keep generic shortcut defaults unless a user override exists.
- **D-08:** Existing tests that currently prove localized shortcut behavior should be adjusted if they conflict with D-07. The milestone requirement KRES-03 is the authority.
- **D-09:** Localized trigger kind and modifiers may still be preserved for access-key defaults so `trigger_kind`, modifier matching, and accessibility labels reflect the resolved localized trigger.

### Legacy Fallback
- **D-10:** Canonical manifest declarations are primary. Legacy `settings.keyboard.shortcuts` declarations are only a compatibility fallback when the manifest does not declare the action id.
- **D-11:** Legacy fallback actions should not gain localized defaults or manifest metadata. They are plain shortcut declarations with module-default source unless user override behavior for that fallback action is explicitly supported by existing settings.
- **D-12:** Preserve Phase 60 dispatch behavior: resolution changes must not reintroduce empty subscriber consumption, text input stealing, or shell-global shortcut precedence regressions.

### the agent's Discretion
- The planner may choose whether to enforce D-07 directly in `resolve_surface_shortcut_declaration` or by filtering localized triggers earlier in declaration construction, as long as the public resolution behavior and source metadata are correct.
- The planner may add helper functions for locale/override decisions if doing so makes tests easier to read.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Scope
- `.planning/ROADMAP.md` — Phase 61 goal, success criteria, and v1.11 execution rules.
- `.planning/REQUIREMENTS.md` — KRES-01 through KRES-04 are the authoritative acceptance boundaries.
- `.planning/PROJECT.md` — v1.11 scope and explicit deferrals for diagnostics, accessibility/debug metadata, global shortcuts, and settings UI.

### Prior Phase Context
- `.planning/phases/60-surface-keybind-dispatch-runtime/60-CONTEXT.md` — locked dispatch path, subscriber semantics, text input precedence, and proof-surface decisions.
- `.planning/phases/60-surface-keybind-dispatch-runtime/60-01-SUMMARY.md` — completed dispatch runtime behavior and tests Phase 61 must preserve.
- `.planning/phases/60-surface-keybind-dispatch-runtime/60-VERIFICATION.md` — Phase 60 passed KDISP requirements and should not regress.

### Prior Keybind Model
- `.planning/milestones/v1.7-phases/40-migration-diagnostics-and-author-docs/40-03-SUMMARY.md` — manifest-first keybind declarations, typed graph records, modifier enforcement, and settings fallback boundary.
- `.planning/milestones/v1.7-phases/40-migration-diagnostics-and-author-docs/40-PATTERNS.md` — canonical `module.json` to installed graph keybind flow and legacy fallback pattern.
- `.planning/milestones/v1.7-REQUIREMENTS.md` — preserved keybind declaration and resolution model from paused v1.6 work.

### Code And Docs
- `crates/core/shell/src/shell/component/input/keyboard.rs` — effective shortcut resolution, locale candidate selection, legacy fallback declarations, modifier matching, event source metadata, and accessibility shortcut formatting.
- `crates/core/shell/src/shell/component/input/mod.rs` — keyboard precedence around dispatch, text input, selection copy, and focused handlers.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` — existing manifest, override, locale, modifier, no-subscriber, text-input, and real navigation tests.
- `crates/core/foundation/config/src/lib.rs` — `KeyboardSettings` and `SurfaceShortcutOverride` schema.
- `crates/core/extension/module/src/manifest/model.rs` — `KeybindAction`, `KeybindTrigger`, `localized_triggers`, and trigger validation.
- `docs/settings/README.md` — `keyboard.surface_shortcuts` as user override data.
- `docs/module-system.md` — author-facing keybind declaration contract.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ResolvedSurfaceShortcut` already records action id, key, modifiers, trigger kind, and source.
- `KeybindResolutionSource` already distinguishes `UserOverride`, `LocaleDefault { locale }`, and `ModuleDefault`.
- `resolve_surface_shortcut_declaration` already applies override, locale candidate, and generic fallback ordering in one function.
- `keybind_locale_candidates` already generates exact and parent locale candidates after trimming and replacing underscores with hyphens.
- `surface_shortcut_declarations` already gives manifest actions priority and skips same-id legacy declarations.

### Established Patterns
- Tests in `navigation.rs` construct synthetic manifests/settings directly, which is the right level for focused resolution behavior.
- Real-surface proof should stay additive; Phase 61 should not redesign shipped navigation or audio markup.
- Settings fallback compatibility should be proven separately from manifest-owned actions.

### Integration Points
- `FrontendSurfaceComponent::resolved_surface_shortcuts` is the main Phase 61 implementation target.
- `resolve_surface_shortcut_declaration` is the likely place to enforce access-key-only localized defaults and override behavior.
- `surface_shortcut_declarations_from_settings` is the compatibility fallback boundary for legacy settings declarations.

</code_context>

<specifics>
## Specific Ideas

- Phase 61 should make KRES-03 explicit because current tests include localized shortcut behavior that may conflict with the milestone requirement.
- Tests should cover "override cannot create declarations" by configuring an override for an unknown action id and proving no shortcut resolves.
- Tests should cover exact locale, parent locale, generic fallback, user override, blank localized key fallback, and legacy fallback when no manifest action exists.

</specifics>

<deferred>
## Deferred Ideas

- Duplicate effective binding diagnostics and malformed declaration reporting belong to Phase 62.
- Debug/profiling payloads and accessibility metadata expansion belong to Phase 63.
- Audio-popover shipped-surface proof belongs to Phase 64 unless a focused regression naturally fits Phase 61.
- Full keybind settings UI and compositor-global shortcuts remain future milestone work.

</deferred>

---

*Phase: 61-Localized Resolution And Override Safety*
*Context gathered: 2026-05-23*
