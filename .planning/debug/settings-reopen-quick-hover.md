---
status: awaiting_human_verify
trigger: "when i close the settings window, iam then unableto reopen it by clicking the button. Also the quick settings popup does not stay there when trying to hover over it it just disappears"
created: "2026-08-04T00:00:00+02:00"
updated: "2026-08-04T11:08:00+02:00"
---

# Settings Reopen and Quick Settings Hover

## Symptoms

- Expected behavior: Closing the Settings window leaves its launcher able to open a fresh window; moving from the Quick Settings trigger into its popup keeps the popup visible.
- Actual behavior: Settings cannot be reopened after closing its window, and Quick Settings disappears when the pointer tries to enter it.
- Error messages: none supplied.
- Timeline: unknown; reproduced in the current live build.
- Reproduction: Open Settings, close its window, and click the Settings launcher again. Hover the Quick Settings trigger, then move the pointer into the popup.

## Current Focus

- hypothesis: confirmed and fixed: Quick Settings now uses an embedded component lifecycle with delayed authored close; hidden windows now invalidate the presentation caches that must be rebuilt on show
- test: focused Quick Settings crossing and Settings close/reopen lifecycle regressions
- expecting: both targeted tests remain green; reporter confirms both workflows under the live Wayland compositor
- next_action: await human verification of Settings close/reopen and trigger-to-Quick-Settings pointer crossing
- reasoning_checkpoint:
    hypothesis: "Quick Settings disappears because `hide_settings_selector(true)` sets `settings_surface_hidden=true` immediately; Settings stays closed because Wayland destroys the hidden toplevel while `last_surface_config` remains cached, causing the replacement configure to be skipped."
    confirming_evidence:
      - "The Quick Settings crossing regression directly observes zero child requests immediately after the trigger leave handler; it expected one."
      - "The window lifecycle regression directly observes no destroyed surface/cache invalidation after close, while the production Wayland hidden-present path destroys window entries."
      - "Render configuration is conditional on `last_surface_config` changing; a destroyed presentation entry plus unchanged cache makes visible present a no-op."
    falsification_test: "The hypothesis is false if preserving authored Quick Settings state does not retain the child through popup entry, or if clearing the hidden window caches does not produce a replacement configure and present after ShowSurface."
    fix_rationale: "Delay the authored Quick Settings close until the hover bridge expires and cancel it from the child's pointer-enter event; make shell own window hide teardown and invalidate exactly the caches that describe the destroyed object."
    blind_spots: "A live compositor may expose additional timing behavior not modeled by the testing backend; self-verification covers the shell/presentation contract but final confirmation still needs the reporter's Wayland session."
- reasoning_checkpoint:
- tdd_checkpoint:

## Evidence

- timestamp: 2026-08-04T09:40:52+02:00
  checked: debug knowledge base and project work tracking
  found: no knowledge-base file exists; docs/BACKLOG.md already contains this exact transient-surface reuse item, and the worktree has unrelated in-flight edits that must be preserved
  implication: there is no prior resolved pattern to assume, this work is authorized by the existing backlog, and source changes must avoid overlapping the current navigation sizing edits

- timestamp: 2026-08-04T09:40:52+02:00
  checked: initial code search for dismissal, settings, popup hover, and reopen paths
  found: presentation exposes `take_dismissed_popups`; shell drains it in runtime/render, tracks dismissed child node keys, and the navigation settings component calls `mesh.popover.hide(..., { bridge = true })` on trigger leave
  implication: both symptoms cross the shell/presentation boundary, but top-level window closure and child-popup hover dismissal have distinct state machines that must be traced separately

- timestamp: 2026-08-04T09:55:00+02:00
  checked: complete in-tree child reconciliation and the shipped navigation component patterns
  found: `settings-button.mesh` sets `settings_surface_hidden = true` synchronously on trigger leave, so the next component tree omits the Quick Settings popover before the core's child-surface bridge can preserve it; theme and language popovers instead keep authored state open, schedule a 400ms close handler, and cancel that handler from a child `PopoverEnter` event
  implication: the Quick Settings symptom is a state-management bug in the module integration, not a failure of the core pointer-enter cancellation path

- timestamp: 2026-08-04T09:55:00+02:00
  checked: xdg-toplevel close, shell visibility, hidden present, and reconfigure paths
  found: close requests are converted to `visible=false`; hidden Wayland windows are destroyed intentionally; showing invalidates/rebuilds the component and a missing presentation object is ready for the configure pass. However close requests are held in a side queue and drained only inside `render_components`, after `dispatch_wayland` and request draining
  implication: object recreation appears designed correctly; the remaining falsifiable Settings hypothesis is request ordering, where a stale close can override a newer show handled earlier in the same frame

- timestamp: 2026-08-04T10:02:00+02:00
  checked: focused tests `navigation_settings_button_drops_its_tooltip_while_quick_settings_is_open` and `window_close_request_hides_the_surface_and_keeps_the_component`
  found: both pass; the navigation test explicitly expects the tooltip and authored state to close immediately after parent PointerLeave, while the window test stops after asserting hidden state and never re-shows or verifies a replacement configure/present
  implication: current tests preserve the Quick Settings regression and do not exercise the reported Settings reopen failure; new behavior-level coverage is required before changing the window lifecycle

- timestamp: 2026-08-04T10:31:00+02:00
  checked: new Quick Settings trigger-leave-to-popup-entry regression before production changes
  found: test fails deterministically because child surface requests drop from one to zero immediately after `onSettingsLeave`
  implication: the authored hidden flag, not the core hover bridge, removes the popup before pointer entry can cancel dismissal

- timestamp: 2026-08-04T10:31:00+02:00
  checked: new Settings close/destroy/re-show/configure/present regression before production changes
  found: test fails at the destruction boundary; the close changes visibility but neither destroys the testing presentation object nor invalidates the shell config cache
  implication: the shell does not model the Wayland destruction it triggers during hidden present, leaving stale cache state that suppresses recreation

- timestamp: 2026-08-04T11:08:00+02:00
  checked: Quick Settings component binding against the working Theme/Language pattern
  found: `@mesh/quick-settings` was declared as a top-level `frontend` with its own surface even though navigation embeds it as a component; after changing it to `component`, `bind:this` supplies the child reference and direct `quick_settings.open` mutation creates the promoted child request
  implication: the popup now has one coherent authored lifecycle, so trigger leave can delay closure and popup entry can cancel it

- timestamp: 2026-08-04T11:08:00+02:00
  checked: focused regressions after fixes
  found: `navigation_settings_button_drops_its_tooltip_while_quick_settings_is_open` passes through trigger leave, popup entry, and bridge expiry; `window_close_request_hides_the_surface_and_keeps_the_component` passes through destroy/cache invalidation, show, replacement configure, and present
  implication: both reported mechanisms are self-verified at their component and shell/presentation boundaries

## Eliminated

## Resolution

- root_cause: Quick Settings was modeled inconsistently as a top-level frontend while being embedded by navigation, and its owner synchronously removed authored popup state on trigger leave. Settings window hiding destroyed the Wayland toplevel while leaving shell-side `last_surface_config` and `known_surface_size` intact, so re-show skipped the configure required to recreate it.
- fix: Converted Quick Settings to an embedded component, drove its `open` state through a bound child reference, scheduled trigger/popup bridge closure with popup-enter cancellation, and routed explicit child closes through the owner. Window hide now destroys the presentation object and invalidates cached config/size so show configures and fully presents a replacement.
- verification: Both focused regressions pass: Quick Settings crossing 1/1 and Settings close/reopen 1/1. Live Wayland confirmation remains required.
- files_changed:
    - modules/frontend/navigation-bar/src/components/settings-button.mesh
    - modules/frontend/quick-settings/module.json
    - modules/frontend/quick-settings/src/main.mesh
    - crates/core/shell/src/shell/runtime/request.rs
    - crates/core/shell/src/shell/component/tests/common.rs
    - crates/core/shell/src/shell/component/tests/interaction/navigation.rs
    - crates/core/shell/src/shell/tests/common.rs
    - crates/core/shell/src/shell/tests/window_role.rs
