---
created: 2026-07-28T00:00:00.000Z
title: Toplevel window surfaces — xdg_toplevel role and runtime widget↔window promotion
area: presentation
files:
  - crates/core/presentation/src/wayland_surface/backend.rs
  - crates/core/presentation/src/wayland_surface/handlers.rs
  - crates/core/presentation/src/wayland_surface/state.rs
  - crates/core/surface-config/src/lib.rs
  - crates/core/extension/module/src/manifest/model.rs
  - crates/core/shell/src/shell/types.rs
  - docs/spec/01-module-system.md
  - docs/spec/11-automation-ipc.md
---

## Status

Phases 1–3 shipped 2026-07-29 — static `role: "window"`, popups over toplevels,
title/app_id/resizable/decorations, close handling, and the diagnostics.
**Phase 4 shipped 2026-07-30** — runtime promote/demote for whole surfaces, the
`promotable` manifest opt-in, the `:windowed` CSS state, and the IPC verbs.
**Phase 5 shipped 2026-08-20** — an embedded retained widget can be promoted to
an independent `xdg_toplevel`, demoted back into its parent, assigned an
explicit window role, and closed without tearing down the parent component VM.
Records: [`../../log/2026-07.md`](../../log/2026-07.md) and
[`../../log/2026-08.md`](../../log/2026-08.md). The original design below is
retained as the rationale and historical implementation guide.

Two things §6 did not anticipate, both in the phase-4 record: the role-mismatch
diagnostic from phase 1 makes a surface that holds *both* roles unexpressible, so
`promotable` had to become a manifest field and the diagnostic's one exemption;
and §7's assumption that the component knows which role it is in is wrong — the
role can change from outside it, so it is CSS state (`:windowed`) rather than
something a handler reads.

Three things the design did not anticipate, all found against a live compositor
and all recorded in the log entry: the tooltip overlay reserve inflates a
toplevel's pinned size; a detached-buffer hide cannot be remapped reliably, so
windows are destroyed and recreated like popovers; and `destroy_surface` only
handled layer surfaces. §2 below also overstates the min-size hint — a resizable
window publishes no min size at all, because the measured size is a component's
natural size, not its minimum.

## Goal

A MESH frontend module can be realized as an ordinary compositor window
(`xdg_toplevel`) instead of shell chrome (`zwlr_layer_surface_v1`). Settings,
the module browser, and developer tools should open as windows that tile,
float, move between workspaces, and close like any other app — while panels,
launchers, and notification popups stay layer surfaces.

Second, a surface's role should be changeable at runtime: a widget mounted on
the panel can be *promoted* into a standalone window (and demoted back)
without losing its component state.

## Current state (verified 2026-07-28)

- `SurfaceRole` in `wayland_surface/backend.rs:143` already models per-surface
  roles: `Layer(LayerSurface)` and `Popup(PopupRole)`. Adding `Window(Window)`
  is a natural third variant, not a new axis.
- `XdgShell` is already bound, but only as a ping/pong + positioner factory.
  `handlers.rs:717-741` deliberately avoids `delegate_xdg_shell!` and stubs
  `ZxdgDecorationManagerV1` with `unreachable!()` because MESH never creates
  toplevels. Both comments become wrong the moment this lands.
- `SurfaceLayoutSection` (`manifest/model.rs:751`) and
  `SurfaceLayoutSettings` (`surface-config/src/lib.rs:14`) are layer-shell
  shaped throughout: `anchor`, `layer`, `exclusive_zone`, `margins`,
  `keyboard_mode`. There is no role field.
- `SurfaceTarget` (`shell/src/shell/types.rs:28`) distinguishes parent vs.
  popup with `popup_parent_surface: Option<String>` — an implicit two-state
  role encoded as an option. A third role needs this made explicit.
- Sizing is CSS-driven and measured at paint time (spec 03 §2); the surface
  wrapper is measured from content and handed to the compositor.
- Popups are created via `zwlr_layer_surface_v1.get_popup`
  (`popup.rs`), which does not apply to toplevel parents.

## Design

### 1. Role in the manifest

Add `role` to the `mesh.surface` block, defaulting to `"layer"`:

```json
"surface": { "role": "window", "title": "MESH Settings", "resizable": true }
```

`role: "window"` makes `anchor`, `layer`, `exclusive_zone`, `margins`, and
`keyboard_mode` inapplicable — reject them as a manifest diagnostic rather
than silently ignoring them (per the no-compat rule: one meaning per field).
Window-only fields: `title` (localizable `{ t, fallback }`), `app_id`
(defaults to the module name so compositor rules can target it), `resizable`,
and optionally `decorations: "server" | "client"`.

`SurfaceLayoutSettings` becomes an enum over the two placement shapes rather
than one struct with half its fields unused per role.

### 2. Sizing is inverted, and that is the hard part

Layer surfaces: MESH measures content → tells the compositor the size.
Toplevels: the compositor sends a configure with a suggested size → the client
must lay out into it (0×0 means "pick your own", which only happens on the
first map).

So a window surface's root box is not "measured to content and pinned"; it is
"given a size, content lays out inside it". The measured content size becomes
`xdg_toplevel.set_min_size` plus the *initial* request, not the committed size
forever. This interacts directly with the CSS-driven surface sizing work and
the first-configure retry loop — the existing configure-wait deadline logic
assumes the compositor echoes our requested size, which a tiling compositor
will not do for a toplevel. Expect this to be where the bugs live.

Practical rule: for `role: "window"`, the surface root gets the configure size
as its available space; CSS `width`/`height` on the root become the initial
size hint and `min-width`/`min-height` become `set_min_size`.

### 3. Events layer surfaces never receive

- **`xdg_toplevel.close`** — no equivalent exists today; every current surface
  is destroyed by MESH's own decision. Needs a close path that lets the
  component veto or run teardown (`onclose`), then unmounts the surface
  without tearing down the module's services.
- **Window states** (maximized / fullscreen / activated / tiled edges) —
  surface as CSS state on the root (`:fullscreen`-like classes or root
  attributes) so components can restyle, e.g. drop rounded corners when tiled.
- **Keyboard focus** arrives by activation rather than
  `KeyboardInteractivity`; `effective_keyboard_mode_for` and the Hyprland
  focus-grab path (`state.rs:130`) must not run for window surfaces.

### 4. Decorations

MESH paints its own chrome, so client-side decoration is the coherent default:
request `zxdg_toplevel_decoration_v1` mode `client_side` and let the module
draw its own header. But `server_side` should be selectable per surface for
users on compositors that decorate uniformly. Either way the stub
`Dispatch<ZxdgDecorationManagerV1>` at `handlers.rs:731` gains real event
handling (the compositor can override the requested mode).

### 5. Popups over windows

`popup.rs` creates popups through `layer_surface.get_popup`. For a toplevel
parent it is `xdg_surface.get_popup` with the parent's `xdg_surface`. The
positioner math is unchanged; only the factory call branches on parent role.
Without this, a `<popover>` inside a windowed settings surface cannot open.

### 6. Runtime promotion (widget → window)

The component VM, retained tree, Lua state, and service subscriptions all
survive; only the presentation object is swapped:

1. Shell marks the surface's target role as `Window`.
2. Presentation destroys the `LayerSurface` and its buffers, creates a
   `Window` for the same `surface_id`, keeps the `SurfaceEntry` (scale, blur
   region, input region, output binding) apart from role-specific fields.
3. Surface is marked unconfigured; the next frame repaints full.

This requires that nothing outside the presentation layer caches
role-dependent state — today `SurfaceTarget.last_surface_config` holds a
`LayerSurfaceConfig` unconditionally, so it becomes role-tagged. The shared
surface VM work (`2026-06-20-shared-surface-vm-live-component-references.md`)
is a prerequisite for promoting a *widget embedded in another surface* into
its own window, because that widget does not own a VM today; promoting a
whole surface does not need it.

### 7. Triggering it

MESH keybinds are **focused-surface semantic actions** (spec 10 §2), not
global hotkeys — MESH cannot grab a compositor-global binding, and should not
try. So there are two paths, and only the second gives the user a global key:

- In-surface: a control declares a keybind action whose handler calls
  `this.surface.promote()` / `.demote()` — works while the surface has focus.
- External: `surface.promote` / `surface.demote` on the automation IPC act
  channel (spec 11 §4, `automation.act`), reachable via the `mesh` CLI. The
  user binds it in their compositor config
  (`bind = SUPER, S, exec, mesh surface promote @mesh/settings`).

The second is what makes "convert this widget into a window with a keybind"
actually work on Hyprland/Sway, and it reuses IPC surface control that already
exists in spec rather than inventing a hotkey daemon.

## Phasing

1. **Static windows.** `role: "window"`, `SurfaceRole::Window`, `WindowHandler`,
   configure-driven sizing, close handling, decorations. Ship
   `@mesh/settings` as a window. No promotion.
2. **Popups over toplevels** — unblocks real UI in windowed surfaces.
3. **Window states + CSS state hooks**, title/app_id from props, min/max size.
4. **Runtime promote/demote** for whole surfaces, plus the IPC verbs and CLI.
5. **Per-widget promotion** (depends on shared surface VM and on multi-instance
   frontend modules, both already in the backlog).

## Open questions

- Does a windowed surface belong to a shell profile the same way a panel does,
  or are windows *spawned* by an action and therefore not part of the profile's
  static root set? Leaning: profiles declare which modules *may* open as
  windows; instances are created on demand. This overlaps the multi-instance
  backlog item and should be decided with it.
- Should `role` be user-overridable in settings (a user preferring the
  launcher as a window)? The sparse-settings model allows it; the sizing
  inversion means a component written for one role may look wrong in the other.
- Window position: xdg-shell gives the client no say. Anything wanting
  placement control needs compositor rules keyed on `app_id`, which is a
  documentation problem, not a code one.
