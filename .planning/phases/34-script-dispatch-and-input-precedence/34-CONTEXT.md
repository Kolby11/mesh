# Phase 34: Script Dispatch and Input Precedence - Context

**Gathered:** 2026-05-14T18:51:31+02:00
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 34 dispatches resolved module keybind actions into frontend scripts while preserving existing shell-global, text-input, focus traversal, selection-copy, and built-in widget-control behavior. It consumes the Phase 32 keybind declaration contract and Phase 33 resolver, but does not add conflict diagnostics, full accessibility metadata, a keybind settings UI, compositor-global shortcuts, or broad module-manifest redesign.

</domain>

<decisions>
## Implementation Decisions

### Dispatch Payload Shape
- **D-01:** `event.keybind` is action/trigger focused. It should not include keybind i18n, action label, or label i18n data because one resolved action can be subscribed to by multiple elements and labels/targets are element-specific.
- **D-02:** The dispatch payload should include action id, trigger kind, key, modifiers, structured source, and locale when applicable.
- **D-03:** `source` should be structured as `user_override`, `locale_default`, or `module_default`. Include a separate `locale` field only when a locale default supplied the trigger.
- **D-04:** Target metadata should come from the actual subscriber element receiving `onkeybind`, using the existing keyboard/current-target payload shape where possible.

### Handler Binding Model
- **D-05:** The normalized keybind action model should stay minimal: stable action id plus trigger data. Do not add manifest-level `handler`, `target_ref`, action label, or action i18n fields for dispatch.
- **D-06:** Handler and target binding are element-owned through `keybind` plus `onkeybind`.
- **D-07:** Imported child components are normal keybind subscribers. Preserve compiled handler namespacing so child component subscribers such as `VolumeButton` can receive dispatch through their namespaced handlers.
- **D-08:** Dispatch requires both `keybind` and `onkeybind`. A node with only `keybind` may later carry metadata/annotation, but Phase 34 dispatch should not call it.
- **D-09:** If multiple rendered elements subscribe to the same action id, dispatch to the focused subscriber first. If no focused subscriber exists for that action, fall back to dispatching matching surface subscribers.

### Input Precedence Rules
- **D-10:** Module keybind dispatch must run after protected focused behavior. Shell-global/debug shortcuts come first, then text input, Tab/Escape, Ctrl+C selection copy, and built-in focused widget activation; module keybinds run after those; custom focused key handlers run after module keybinds unless a protected built-in path consumed the key.
- **D-11:** Protected widget behavior means built-in widget keys only: Enter/Space for buttons/toggles and arrow keys for sliders, plus text input, Tab/Escape, and Ctrl+C selection.
- **D-12:** Custom `keydown`/`keyup` handlers do not automatically outrank module keybinds.
- **D-13:** Keybind matching must be strict. Shortcut triggers match key and modifiers exactly. Access-key triggers match unmodified keys only.
- **D-14:** Phase 34 should add regression coverage proving shell-global/debug shortcuts win before module keybinds.

### Shipped Proof Targets
- **D-15:** Keep shipped proof narrow and representative: navigation-bar mute service command, audio popover open via keybind, and one non-service function action such as theme toggle.
- **D-16:** Add focused localized access-key proof now because Phase 34 consumes the Phase 33 resolver. Broader localized shipped-surface proof remains Phase 36.
- **D-17:** Update shipped declarations narrowly in `@mesh/navigation-bar` for the proof actions. Do not broaden into full audio-popover keybind declaration work.
- **D-18:** Verification should be focused automated regressions: payload shape, subscriber focus precedence, strict modifiers, protected widget/input behavior, debug precedence, and the three shipped proof actions. Live UAT is not required for this phase.

### the agent's Discretion
- The planner may choose the exact internal struct names and file placement for enriched resolved keybind records, provided behavior matches the decisions above and reuses the Phase 33 resolver path.
- The planner may choose the exact non-service proof action, with theme toggle as the preferred example if it fits the existing navigation-bar tests cleanly.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Scope
- `.planning/ROADMAP.md` - Phase 34 goal, planned work, success criteria, and v1.6 milestone boundaries.
- `.planning/REQUIREMENTS.md` - `DISP-01`, `DISP-02`, and `DISP-03` requirements.
- `.planning/STATE.md` - Prior milestone and Phase 32/33 decisions that constrain this phase.
- `.planning/phases/32-keybind-declaration-contract/32-01-SUMMARY.md` - Keybind declaration contract and compatibility bridge delivered in Phase 32.
- `.planning/phases/33-locale-aware-keybind-resolution/33-CONTEXT.md` - Locked Phase 33 decisions for resolver precedence, locale scope, and deferred diagnostics/accessibility.
- `.planning/phases/33-locale-aware-keybind-resolution/33-01-SUMMARY.md` - Locale-aware resolver implementation summary.

### Research Basis
- `.planning/research/SUMMARY.md` - Synthesized v1.6 keybind research and dispatch-order recommendation.
- `.planning/research/FEATURES.md` - Feature expectations for script dispatch payloads and scoped module keybind behavior.
- `.planning/research/ARCHITECTURE.md` - Recommended resolver/dispatch integration and precedence architecture.
- `.planning/research/PITFALLS.md` - Known pitfalls around overloaded keybind concepts, precedence, and shared resolved records.

### Existing Code
- `crates/core/shell/src/shell/component/input/keyboard.rs` - Current resolved shortcut records, subscriber collection, dispatch, and accessibility annotation bridge.
- `crates/core/shell/src/shell/component/input/mod.rs` - Keyboard input precedence for text input, Tab/Escape, Ctrl+C, surface shortcut dispatch, and focused widget behavior.
- `crates/core/extension/module/src/manifest/model.rs` - Current normalized keybind action and trigger model.
- `crates/core/foundation/config/src/lib.rs` - User surface shortcut override model keyed by surface id and action id.
- `modules/frontend/navigation-bar/module.json` - Shipped navigation-bar keybind declarations to update narrowly for proof actions.
- `modules/frontend/navigation-bar/src/main.mesh` - Navigation-bar handlers for mute, audio popover activation, and shipped control behavior.
- `modules/frontend/navigation-bar/src/components/volume-button.mesh` - Existing child component subscriber using `keybind` and `onkeybind`.
- `crates/core/shell/src/shell/component/tests/interaction/navigation.rs` - Existing keyboard shortcut, locale resolver, navigation-bar, popover, and theme activation regression tests.
- `crates/core/shell/src/shell/component/tests/integration/service.rs` - Existing service command routing proof pattern.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ResolvedSurfaceShortcut` in `crates/core/shell/src/shell/component/input/keyboard.rs` already carries action id, key, trigger kind, and resolution source. Phase 34 can expand or wrap this into the resolved dispatch record.
- `build_keyboard_event` already provides key, modifiers, surface metadata, current node metadata, bounds, and `current_target`; use this for subscriber target metadata instead of manifest-owned target refs.
- `keybind_subscribers` and compiled handler namespacing already support child component handlers such as `VolumeButton` subscribing with `keybind` and `onkeybind`.
- Existing navigation-bar tests already prove service commands, popover activation, theme activation, and locale resolver behavior on real shipped components.

### Established Patterns
- Manifest parsing and resolver behavior are tested in focused Rust tests and real-surface component tests.
- User overrides remain keyed by `surface_id + action_id` and must not depend on translated labels or element-specific labels.
- Shell core should stay generic: keybind dispatch may activate service commands through existing script proxy behavior, but service-specific logic stays in Luau/frontends/backends.
- Existing focused-control behavior is explicit in the keyboard input pipeline and should be protected before module keybind dispatch.

### Integration Points
- `FrontendSurfaceComponent::resolved_surface_shortcuts` is the resolver input for dispatch and accessibility annotation.
- `FrontendSurfaceComponent::dispatch_surface_shortcut` is the current dispatch path that should move from key-only shortcut matching to strict trigger matching and richer event payloads.
- `FrontendSurfaceComponent::handle_input` in `input/mod.rs` owns precedence ordering and must be adjusted so protected focused behavior comes before module keybind dispatch.
- `modules/frontend/navigation-bar/module.json` and `modules/frontend/navigation-bar/src/main.mesh` are the narrow shipped-surface proof targets.

</code_context>

<specifics>
## Specific Ideas

- The user explicitly corrected that keybind actions can be activated on multiple elements, so dispatch payloads must not carry keybind-level i18n/label data.
- The broader module manifest system should be rethought as a whole later, but Phase 34 should not expand into that redesign.
- Theme toggle is the preferred non-service function proof if it remains clean against existing navigation-bar tests.

</specifics>

<deferred>
## Deferred Ideas

- Rethink the whole module manifest system and how it works as a whole. This belongs with the existing unified package/module manifest future phase, not Phase 34.
- Conflict diagnostics for duplicate bindings and malformed declarations remain Phase 35.
- Full accessibility metadata and broad localized shipped-surface proof remain Phase 36.
- Full keybind settings UI and compositor-global shortcuts remain out of scope for v1.6 Phase 34.

### Reviewed Todos (not folded)
- Create unified package and module manifest phase - reviewed and deferred because it is a separate future manifest/system-design phase.
- Audio popover transition delay polish - reviewed and deferred because it is accepted v1.5 polish debt outside Phase 34 dispatch scope.

</deferred>

---

*Phase: 34-Script Dispatch and Input Precedence*
*Context gathered: 2026-05-14T18:51:31+02:00*
