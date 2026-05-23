# Phase 60: Surface Keybind Dispatch Runtime - Context

**Gathered:** 2026-05-23
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 60 turns canonical, manifest-owned surface keybind declarations into real focused-surface runtime dispatch. It must route matching key events through the existing shell component handler path while preserving current keyboard ownership rules: shell-global shortcuts run before component input; Tab/Escape focus behavior, Ctrl+C selection copy, text input, focused key handlers, and default widget activation remain compatibility boundaries.

This phase does not own locale fallback expansion, broad conflict diagnostics, accessibility/debug metadata beyond the existing event payload shape, compositor-global shortcuts, or a settings UI. Those belong to later v1.11 phases or future milestones.

</domain>

<decisions>
## Implementation Decisions

### Runtime Path
- **D-01:** Reuse the existing `FrontendSurfaceComponent` keyboard input path and `dispatch_surface_shortcut` flow. Do not add a new global keybind dispatcher for Phase 60.
- **D-02:** Surface keybinds dispatch through explicit template subscribers using `keybind="<action-id>"` plus `onkeybind={handler}`. The manifest declares the action and trigger; markup chooses the target/control that handles it.
- **D-03:** Manifest actions must never dispatch directly to raw handler names. Runtime dispatch finds subscribers in the current widget tree and calls the existing namespaced component handler path.

### Keyboard Precedence
- **D-04:** Preserve the current precedence: shell-global shortcuts before component input; inside component input, Tab/Escape handling and Ctrl+C selection copy stay before surface keybind dispatch.
- **D-05:** Surface keybind dispatch stays before focused `keydown` handlers and default widget activation for non-text-focused cases so semantic actions can work regardless of which control is focused in the surface.
- **D-06:** Focused text input remains protected. A bare printable keybind must not steal normal text entry when an input owns focus; planners should either keep printable text input on the text-input event path or gate bare printable surface keybinds away from focused inputs.

### Subscriber Semantics
- **D-07:** If multiple nodes subscribe to the same action, dispatch to subscribers in deterministic tree order and aggregate their `CoreRequest`s.
- **D-08:** If an action has no subscribers in the current tree, dispatch should be a no-op for Phase 60. Missing-target diagnostics belong to Phase 62.
- **D-09:** The keybind event payload should continue to include `keybind.id`, `trigger_kind`, and resolution `source` so scripts can distinguish manifest defaults, localized defaults, and user overrides without parsing raw keyboard state.

### Proof Surface
- **D-10:** Navigation bar is the primary Phase 60 proof surface because it already declares `mesh.keybinds.mute`, subscribes a volume button handler, and has existing real-surface keyboard tests.
- **D-11:** Audio popover should be used only for regression/proof where it already has relevant controls. Do not expand Phase 60 into broad audio popover redesign or keybind UI work.
- **D-12:** Preserve legacy settings fallback tests. Phase 60 should prove manifest-owned dispatch is primary without deleting compatibility coverage.

### Folded Todos
- No pending todos were folded into Phase 60. The matching todo, `2026-05-15-define-module-install-requirement-resolution.md`, is broader module/resource install resolution work and should remain pending for a later phase.

### the agent's Discretion
- The planner may choose helper names and exact file boundaries, but should keep changes close to existing keyboard/input code and tests.
- The planner may add focused tests before production edits if they clarify current behavior, especially around focused text input protection and no-subscriber behavior.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Scope
- `.planning/ROADMAP.md` — Phase 60 goal, requirements, and v1.11 execution rules.
- `.planning/REQUIREMENTS.md` — KDISP-01 through KDISP-04 acceptance boundaries.
- `.planning/PROJECT.md` — v1.11 milestone scope and explicit deferrals for global shortcuts and settings UI.

### Prior Keybind Decisions
- `.planning/MILESTONES.md` — v1.6 paused scope and high-priority remaining keybind work.
- `.planning/milestones/v1.7-REQUIREMENTS.md` — preserved keybind declaration/resolution model and future KEYB-01 scope.
- `.planning/milestones/v1.7-phases/40-migration-diagnostics-and-author-docs/40-03-SUMMARY.md` — manifest-first keybind declarations, typed graph records, modifier enforcement, and settings fallback boundary.
- `.planning/milestones/v1.7-phases/40-migration-diagnostics-and-author-docs/40-PATTERNS.md` — canonical `module.json` to installed graph keybind flow and legacy fallback pattern.

### Code And Docs
- `crates/core/shell/src/shell/component/input/mod.rs` — component-level keyboard precedence and current `dispatch_surface_shortcut` call site.
- `crates/core/shell/src/shell/component/input/keyboard.rs` — resolved shortcuts, subscriber collection, keybind event payload, modifier matching, and accessibility annotation helpers.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` — focused shortcut, manifest-first, modifier, locale, and real navigation-bar tests.
- `crates/core/extension/module/src/manifest/model.rs` — canonical keybind action, trigger, scope, and validation model.
- `crates/core/extension/module/src/package/installed_graph.rs` — typed installed graph keybind contribution indexing.
- `modules/frontend/navigation-bar/module.json` — shipped navigation-bar manifest keybind declaration.
- `modules/frontend/navigation-bar/src/components/volume-button.mesh` — shipped keybind subscriber markup and handler.
- `modules/frontend/navigation-bar/config/settings.json` — legacy/default focused-surface shortcut fallback fixture.
- `docs/module-system.md` — author-facing `mesh.keybinds` contribution contract.
- `docs/settings/README.md` — `keyboard.surface_shortcuts` as user override data and legacy fallback input only.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ResolvedSurfaceShortcut` in `crates/core/shell/src/shell/component/input/keyboard.rs`: already carries action id, key, modifiers, trigger kind, and source.
- `keybind_subscribers` and `collect_keybind_subscribers`: already find `keybind` + `onkeybind` subscribers in the runtime tree.
- `dispatch_surface_shortcut`: already builds a keyboard event and calls namespaced handlers for matching subscribers.
- Existing navigation tests in `crates/core/shell/src/shell/component/tests/interaction/navigation.rs`: cover settings fallback, manifest override resolution, modifiers, locale fallback, and real navigation behavior.

### Established Patterns
- Canonical manifest declarations are primary; `settings.keyboard.surface_shortcuts` is user override data and legacy settings declarations are fallback only when a manifest action id is absent.
- Keyboard behavior is compatibility-sensitive. Existing input order gives Tab/Escape and Ctrl+C selection copy explicit priority before surface keybind matching.
- Runtime events are JSON payloads built by component helpers, then dispatched through existing script handler plumbing.

### Integration Points
- `FrontendSurfaceComponent::handle_input` in `input/mod.rs` is the main integration point for preserving precedence.
- `FrontendSurfaceComponent::resolved_surface_shortcuts` and `surface_shortcut_declarations` are the integration points for manifest versus legacy settings declaration sources.
- Shipped surface proof should use `modules/frontend/navigation-bar` and the existing real-surface tests rather than synthetic-only fixtures.

</code_context>

<specifics>
## Specific Ideas

- The runtime should behave like a focused-surface action system, not an app-global shortcut registry.
- The user goal is to "finish the keybind system for the surfaces"; the relevant completion bar for Phase 60 is dispatch working through real focused surfaces without keyboard regressions.
- Interactive decision selection was unavailable in this runtime, so the conservative defaults above were selected from prior locked project decisions.

</specifics>

<deferred>
## Deferred Ideas

- Compositor-global shortcuts through XDG Desktop Portal or compositor-specific APIs remain future milestone work.
- Full user-facing keybind remapping UI remains future milestone work.
- Broad module install/resource requirement resolution remains pending outside Phase 60.

### Reviewed Todos (not folded)
- `2026-05-15-define-module-install-requirement-resolution.md` — deferred because it covers module install/resource requirements across icons, sounds, fonts, keybinds, languages, themes, providers, and settings, while Phase 60 is limited to focused-surface dispatch runtime.

</deferred>

---

*Phase: 60-Surface Keybind Dispatch Runtime*
*Context gathered: 2026-05-23*
