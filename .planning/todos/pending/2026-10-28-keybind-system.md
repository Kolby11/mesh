# Keybind system: scopes, automatic assignment, key hints, help window

Backlog: the keybind-system item in [`docs/BACKLOG.md`](../../../docs/BACKLOG.md).
Contract today: [spec 10 §2, §4, §5](../../../docs/spec/10-keyboard.md).

## Status (2026-10-28)

Shipped, with tests: scopes (`focus`, `surface`, shell), `auto` triggers and the
allocator with its saved-assignment cache, shell actions, hold/toggle key hints
painted for CPU and scene canvases, the `mesh.keybinds` service and
`@mesh/keybind-help`. Spec: [10 §2](../../../docs/spec/10-keyboard.md).

Open (see the backlog line): settings page rows, inline remap and "reassign all"
in the help window (the service already has `remap`), `mesh-shell keys …` IPC,
ordinal keys, a live GPU and popup check of hints. Differences from the design
below: help is `F1` only (`Ctrl+H` is the hints chord); auto actions are taken in
action-id order because manifests keep actions in a map; the help window reads
the new `mesh.keybinds` service, not `mesh.debug`.

## Goal

1. Keybinds exist at three scopes: **shell-global**, **surface/module**, and
   **focused component**.
2. Authors mark actions as auto-assigned; the shell gives them keys from the
   user's ordered preference list (for example `qwertasd`).
3. A **key-hint mode** draws each bound control's key in a corner of the
   control, held or toggled.
4. A **help window**, openable at any time, lists every binding with its
   description, plus the bindings of the focused component.

## What already ships

- Modules declare actions in `mesh.contributes.keybinds` (label, description,
  category, `trigger`, `localizedTriggers`); controls subscribe with
  `keybind="id"` and `onkeybind`. Dispatch is surface-wide
  (`dispatch_surface_shortcut`), modal scopes respected, modifiers matched.
- Resolution: user override → locale access key → generic trigger
  (`resolved_surface_shortcuts`). Overrides live in
  `shell.keyboard.surface_shortcuts[surface][action]`.
- Diagnostics for duplicates, missing handlers and undeclared subscriptions;
  `mesh.debug.keybinds`; shortcut metadata reaches accessibility.
- Spec 10 §5 defers global hotkeys: layer-shell clients cannot grab keys.

## Gaps

- No scope other than "this surface". A slider's arrow keys and a bar's `f`
  cannot be told apart in the help data, and nothing is shell-wide.
- Every trigger is hand-picked, so two modules collide on `f` by accident.
- Nothing shows the user what is bound. No help surface, no on-control hints.
- Core owns no declarations of its own (help, hints, profile switch).

## Design

### 1. Scopes

| Scope | Declared by | Active when | Dispatched by |
| --- | --- | --- | --- |
| `shell` | core (built-in table, same shape as a module declaration) | any MESH surface holds keyboard focus | shell, **before** surface dispatch |
| `surface` | module manifest (today's behaviour, the default) | the declaring surface holds focus | surface (existing path) |
| `focus` | module manifest, `"scope": "focus"` | focus is inside the subscribing element's subtree | surface, after `surface` actions |

- A `focus` action is subscribed on a container (`<column keybind="volume.step">`)
  and fires only while focus is inside it. Narrowest scope wins on a clash:
  `focus` > `surface` > `shell`. Reserved shell keys (Tab, Enter, Space, Escape,
  arrows) cannot be claimed by any scope; the existing diagnostic path reports it.
- Truly global keys remain the compositor's job. Ship IPC commands so a
  compositor bind can reach the shell actions:
  `mesh-shell keys help | hints | hints-toggle` (docs show Hyprland `bind =` lines,
  as for `mesh-shell focus`). No change to the spec 10 §5 stance.

### 2. Shell actions

Core declares `shell.help`, `shell.hints`, `shell.switch-profile` in a built-in
table that merges into the same registry, so they are remappable, shown in the
help window and subject to the same conflict checks. Defaults: help `F1`,
hints `Ctrl+H` hold/toggle, see §4.

### 3. Automatic assignment

New trigger kind: `{ "kind": "auto" }` (optional `"prefer": "f"` hint).

- User preference `shell.keyboard.key_priority`: an ordered string or array,
  default `"asdfqwerzxcv"`-style home-row-first list. Digits and F-keys can be
  appended as overflow.
- Allocator is a **pure function**: inputs are the surface's declarations in a
  stable order (module id, then declaration order), the explicit and override
  bindings, the reserved set, and the priority list; output is action → key.
  Auto actions take the first priority key that no explicit binding, override or
  earlier auto action holds. `prefer` is tried first only when
  `shell.keyboard.mnemonic_first` is on (off by default, so the user's order
  rules).
- **Stability.** Pure ordering makes keys move whenever a button is added.
  Persist resolved auto keys in a sparse `shell.keyboard.assigned[surface][action]`
  cache: a cached key is reused while still free and valid, new actions take the
  next free key, and a "Reassign all" command clears the cache. Users can pin
  (writes an override) or clear any one.
- Overflow (more actions than keys): leave the action unbound, report one
  diagnostic, and list it in the help window as "unassigned".
- Repeated items (`{#for}` rows) do not get auto keys; a later `ordinal` kind
  (1–9) can cover them. Out of scope for the first cut.
- Single unmodified printable keys are suspended while an editable text control
  has focus; chords still work. Verify today's behaviour first (see Phase 0).
- Layouts: priorities are logical characters, so `qwertz` and Slovak users type
  what they see. Reserved-key and conflict checks compare logical keys.

### 4. Key-hint mode

Shell-owned, because the badge must look and behave the same on every module
([00 §4](../../../docs/spec/00-philosophy.md#4-elements-enforce-shared-standards)).

- Trigger chord `shell.hints` (default `Ctrl+H`). Tap (under 250 ms) toggles a
  sticky mode; holding shows hints until release. Preference
  `shell.keyboard.hint_mode`: `hold`, `toggle` or `both` (default).
- Rendering: after layout, collect nodes that hold a resolved binding for the
  focused surface and its promoted popups. Emit overlay display commands, painted
  last, outside layout, non-interactive: a key-cap badge anchored to a
  preference corner (`shell.keyboard.hint_corner`, mirrored for RTL), nudged
  inward when the control touches the surface edge. Scope `focus` bindings show
  only while focus is inside their container, so the screen is not flooded.
- Cost: the pass runs only on mode change and when bindings or bounds change; no
  per-frame work while hints are off (idle-overhead requirement).
- Style: core-owned default keycap, overridable through theme tokens
  (`--keycap-*`); no literal colours.
- The badge text is the same string that goes into `aria-keyshortcuts`.

### 5. Help window

- Mechanism in core, UI as a module, matching the settings and devtools rule.
  New built-in service `mesh.keybinds`: `groups[]` of `{scope, module, surface,
  actions[{id, label, description, category, binding, source, conflict}]}`,
  plus `context` (the surface and focused element the user came from) and
  methods `remap(surface, action, key)`, `clear(surface, action)`, `reassign()`.
- The help module `@mesh/keybind-help` (ordinary `.mesh`): sections "Here"
  (focused component), "This surface", "Shell", then every other surface;
  search box; conflict and unassigned markers; inline remap writing the same
  `surface_shortcuts` overrides.
- Opened by `shell.help` from any MESH surface. It takes the keyboard, so the
  service snapshots the previous focus (`(surface, key)` is already recorded for
  popover return) when it opens, and the window shows that snapshot.
- Settings UI (generated, spec 08) gets the priority list, hint mode, hint
  corner and "reassign all".

## Phases

0. **Baseline.** Tests pinning current behaviour: unmodified key while an input
   is focused; `focus`-less dispatch; modal scope. Decide stability policy.
1. **Scopes and shell actions.** `scope` field in manifest and schema, built-in
   shell declarations, shell-before-surface dispatch, narrowest-wins resolution,
   `mesh-shell keys …` IPC. Update spec 10 and the LSP manifest schema.
2. **Auto assignment.** `auto` trigger, pure allocator with unit tests
   (determinism, collisions, overrides, reserved keys, overflow, cache reuse),
   `key_priority` and `assigned` in settings, generated settings rows.
3. **`mesh.keybinds` service and help window.** Contract, snapshot on focus
   change, module, i18n, real-surface test (open from a bar, shows the bar's
   actions first).
4. **Hint overlay.** Overlay collection and painting for GPU and CPU paths,
   hold/toggle state machine, RTL and edge nudging, damage and idle tests.
5. **Migration.** Move existing hand-picked triggers (glass dock and bar,
   dashboard, radial launcher `f`) to `auto` where a fixed key is not a
   contract, update docs and the module guide.

## Verification

- Allocator and state-machine logic: unit tests, no UI.
- Real-surface tests per phase with the existing mount helpers.
- Hint overlay: pixel tests under two theme packs and at two scales, plus an
  idle-redraw check that hints off adds no frames.
- Live Wayland check of hold versus toggle timing and of the compositor-bind
  IPC path before removing the backlog line.

## Decisions

Settled 2026-10-28.

- Defaults: `help` is `F1`; `hints` is `Ctrl+H` (tap toggles, hold shows).
  Both are remappable like any action.
- The auto-assignment cache (`shell.keyboard.assigned`) is stored per user, not
  per profile, because muscle memory follows the person. Priority list, hint mode
  and hint corner are per user as well.
- Hints also appear on controls in promoted popups of the focused surface, and
  not on other surfaces.
