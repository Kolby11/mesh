# Section 13 — Surface policy and configuration audit

**Audited:** 2026-08-20  
**Package:** `mesh-core-surface-config`  
**Scope:** manifest surface declarations, sparse module settings, props
overrides, role/placement resolution, shell lifecycle, and the
`SurfaceConfig` hand-off to presentation. No production code was changed.

Four Luna xhigh passes were launched for the requested process-tree,
logic/order, direct code-error, and schema/lifecycle specialist reviews. The
workers were shut down after repeated timeouts; this report combines the local
source audit, exact call-chain inspection, and focused tests rather than
claiming unreturned worker findings.

## Logical process tree

```text
module.json / manifest mesh.surface
  -> manifest normalization and module-graph surface diagnostics
  -> core fallback SurfaceLayoutSettings
  -> sparse settings namespace (<module>.surface + props + related fields)
  -> schema validation
       -> accepted values
       -> rejected values + SettingsDiagnostic
       -> manifest baseline + accepted user override precedence
  -> resolved FrontendModuleSettingsState
       -> SurfaceLayoutSettings
       -> effective props/settings JSON
       -> reload diagnostics
  -> frontend component instance/profile binding
       -> visible/role/promotable state
       -> runtime role or keyboard overrides
       -> CSS content measurement
  -> shell render-layout policy
       -> role, window options, anchor/layer, size, exclusive zone
       -> keyboard mode, margins, blur, visibility
       -> measured content and tooltip/input padding
  -> shell runtime lifecycle
       -> configure/reconfigure or defer until measurement
       -> show/hide, frame/configure readiness, role promotion
       -> settings reload and generation invalidation
  -> presentation SurfaceConfig
       -> role-specific Wayland object
       -> layer-shell or xdg-toplevel protocol requests
       -> output clamp, buffer/input geometry, configure ack
  -> compositor callbacks
       -> configured size/window states/focus
       -> shell remeasurement and next policy pass
```

The policy boundary should make these invariants explicit:

1. Invalid author manifest values produce module diagnostics; they do not
   silently turn into a different surface policy.
2. Precedence is deterministic: core defaults < manifest declaration < valid
   user override, while author-only safety controls such as promotion are not
   user-bypassable.
3. A role change is one transaction: authorization, child-surface teardown,
   compositor-object replacement, cached-size invalidation, remeasurement,
   configure, and first present either complete as one lifecycle or retain the
   old role.
4. Layer-only and window-only fields are inert, diagnosed consistently, and
   never leak into the other role's protocol requests.
5. Every behavior-affecting `SurfaceConfig` field participates in semantic
   change detection, including blur namespace and creation-time decoration
   policy.
6. Zero means “not measured” inside the shell only until the policy resolves
   it; no unbacked zero can reach layer-shell where it means output spanning.
7. Geometry is bounded and checked before protocol conversion; reloads cannot
   publish stale dimensions or stale input padding.

## Findings

### 1. P1 — User role settings bypass the `promotable` author guard

`resolve_frontend_module_settings_with_props` accepts a stored `surface.role`
override at `crates/core/surface-config/src/lib.rs:200-205`, regardless of
`layout.promotable`. The shell then applies the resolved role directly in
`crates/core/shell/src/shell/component/rendering/mod.rs:178-181`. The explicit
runtime request path does check `surface_promotable()` at
`crates/core/shell/src/shell/runtime/request.rs:1391-1399`, but that guard is
not used when a settings reload changes the role.

**Failure:** A module that declares `promotable: false` can be changed from a
layer surface to a window by writing `<module>.surface.role`. The next render
reconfigures the compositor object even though the author explicitly refused
runtime role changes.

**Improvement:** Separate author role capability from the user-selected role.
Either reject stored role changes unless the manifest is promotable, or define
role selection as startup-only and require the typed role request API for live
promotion. Route both settings and IPC changes through one transactional role
transition function. Add a reload regression for a non-promotable surface.

### 2. P1 — `promotable` is accepted and ejected as a setting but ignored by
resolution

The settings schema accepts `surface.promotable` at
`surface-config/src/lib.rs:603-613`, and `surface_layout_to_json` emits it at
`:386-392`, but `resolve_frontend_module_settings_with_props` never reads a
checked `promotable` value while applying the user surface fields
(`:200-309`). The manifest value is read at `:118-120`.

**Failure:** A user can persist or eject `promotable: true` and receive no
effective change, while the same settings block can change `role`. The stored
schema therefore advertises a control whose semantics are inconsistent with
the safety policy.

**Improvement:** Make the ownership explicit. Prefer removing `promotable`
from user-overridable `SURFACE_FIELDS` and from ejected settings because it is
an author capability, or implement it as a validated policy value with a
transactional role-change check. Add tests for manifest false/true, stored
false/true, and role reload combinations.

### 3. P1 — Invalid manifest enum values silently fall back to another policy

`SurfaceLayoutSection` stores raw strings (`crates/core/extension/module/src/manifest/model.rs:752-808`).
`surface_layout_from_manifest` calls `parse_*` and simply ignores failures at
`crates/core/surface-config/src/lib.rs:115-163`; its API has no diagnostics.
The module graph diagnostics at
`crates/core/extension/module/src/package/installed_graph/diagnostics.rs:386-435`
check missing declarations and role-field mismatches, but not invalid role,
anchor, layer, keyboard, or decoration values.

**Failure:** `anchor: "botom"`, `role: "windwo"`, or an invalid keyboard mode
loads as the generic default with no author-facing error. The surface can be
placed somewhere different from what its manifest says.

**Improvement:** Validate manifest surface enums and semantic constraints at
manifest/module-graph load time using the same canonical parsers and emit a
diagnostic with path, value, and allowed values. Do not make the resolver's
silent fallback the only defense. Add malformed-manifest tests for every enum.

### 4. P1 — Presentation change detection omits blur and window decorations

`surface_config_fingerprint` hashes role, geometry, padding, margins, and
keyboard mode at `crates/core/presentation/src/wayland_surface/backend/config.rs:192-216`,
but omits `cfg.blur` and `cfg.window.decorations`. The shell's
`config_changed` comparison does notice these fields at
`crates/core/shell/src/shell/runtime/render/mod.rs:268-288`, so it calls
presentation `configure`; `SurfaceEntry::needs_reconfigure` then compares the
incomplete fingerprint at `crates/core/presentation/src/wayland_surface/backend/entry.rs:198-205`.

**Failure:** Toggling `surface.blur` can leave the existing layer namespace
without its `:blur` suffix, so compositor blur rules do not change. Changing
window decorations on an existing toplevel can be accepted by shell state but
never recreate the role object, even though decorations are a creation-time
request at `entry.rs:220-225`.

**Improvement:** Include all semantic fields in the fingerprint, and classify
creation-time fields separately so a decoration change explicitly recreates or
rejects the live window. Add presentation tests for blur-only reloads and
client/server decoration changes.

### 5. P1 — Settings role reload is not the same transaction as explicit role
promotion

The explicit role request path destroys child surfaces, destroys the old
compositor object, clears keyboard overrides and cached geometry, then marks a
full present at `crates/core/shell/src/shell/runtime/request.rs:1428-1471`.
Settings reload only replaces `self.surface_layout` and invalidates surface
config at `crates/core/shell/src/shell/component/shell_component/mod.rs:1183-1249`.

**Failure:** Even after the authorization issue is fixed, a role change coming
from settings can take a different path with stale popup/component bookkeeping,
focus state, or old configured-size assumptions. A half-applied role change can
leave the new protocol object waiting on state associated with the old role.

**Improvement:** Produce a typed `SurfacePolicyChange` diff and send it through
one shell-owned transition supervisor. It should validate promotion, close or
reparent children, clear role-specific focus/size state, recreate the protocol
object, and only commit the new effective policy after the transition reaches a
known state. Add settings-reload role transition tests with open popovers and
focus ownership.

### 6. P2 — Settings role-field diagnostics do not cover all inert fields

The settings-only inert lists are
`surface-config/src/lib.rs:679-680`: `LAYER_ONLY_KEYS` excludes keyboard mode
and all four margin fields, while the manifest model correctly includes
`margins` and `keyboardMode` in `layer_only_fields` at
`manifest/model.rs:815-825`. `surface_layout_to_json` also writes keyboard mode
and margins unconditionally at `surface-config/src/lib.rs:408-423`, including
for windows where presentation ignores them.

**Failure:** A window settings block can contain layer-only keyboard/margin
values without the warning emitted for anchor/layer/exclusive zone/blur, and
eject can persist inert fields that look active. This makes settings editing
and manifest diagnostics disagree about the same policy.

**Improvement:** Define one role-field metadata table used by manifest
diagnostics, settings validation, ejection, and protocol lowering. Emit one
consistent warning or omit inert fields by role. Test window/layer settings and
ejected JSON for every role-specific field.

### 7. P2 — Ejection loses localization identity and does not clearly distinguish
effective values from overrides

`surface_layout_to_json` serializes a `LocalizedText` title using only
`fallback_text()` at `surface-config/src/lib.rs:394-405`. It also omits the
effective module-derived `app_id` when the layout stores `None`, while the shell
derives that id from the module at
`crates/core/shell/src/shell/component/rendering/mod.rs:225-243`.

**Failure:** Ejecting a localized manifest title pins the current fallback and
loses its translation key. The generated settings file looks like a complete
effective policy but is actually a mixture of explicit overrides and values
that will still be derived from the manifest.

**Improvement:** Give ejection an explicit mode: preserve author localization
and emit a structured localized value, or deliberately materialize a literal
effective value while documenting that it pins the fallback. Include derived
app id and source metadata if the command promises a complete effective
configuration. Add round-trip tests with translated titles and locale changes.

### 8. P2 — Geometry policy is split across resolver, shell, and presentation

The resolver returns zero dimensions by design because CSS measures content
(`surface-config/src/lib.rs:11-14`, `:87-103`). The shell then interprets zero,
measurement readiness, tooltip reserve, and role-specific sizing across
`crates/core/shell/src/shell/component/rendering/mod.rs:162-213` and
`crates/core/shell/src/shell/runtime/render/mod.rs:250-373`; the presentation
layer independently maps zero to layer-shell spanning or a fallback at
`crates/core/presentation/src/wayland_surface/backend/protocol.rs:80-128`.

**Failure:** A new caller can pass a legitimate “not measured” zero into a
protocol path where zero means “span this axis,” or apply padding/reserve to a
different coordinate space. The existing guards prevent known regressions, but
the contract is distributed and easy to violate.

**Improvement:** Introduce explicit types such as `UnmeasuredSize`,
`ContentExtent`, `SurfaceExtent`, and `LayerWireSize`, with one conversion
function at the shell/presentation seam. Make an unresolved configuration
unrepresentable as a wire `SurfaceConfig`. Add property tests for each role,
edge, exclusive-zone, zero, and padding combination.

### 9. P2 — Surface settings lack a single semantic diff and generation

The resolver returns a complete `SurfaceLayoutSettings`, but reload handling
compares broad values in the shell (`component/shell_component/mod.rs:1184-1188`)
and the presentation layer separately hashes a different subset
(`presentation/.../config.rs:192-216`). Runtime keyboard overrides and
creation-time window policy are maintained in separate state fields.

**Failure:** A field can trigger shell work but not presentation work, or can
be treated as a harmless reconfigure when it actually requires object
replacement. This is the same class of drift demonstrated by the omitted blur
and decoration fingerprint fields.

**Improvement:** Resolve to an immutable, revisioned `SurfacePolicySnapshot`
with a typed semantic diff (`Noop`, `InputRegionOnly`, `LayerConfigure`,
`WindowRecreate`, `RoleTransition`, `MeasureAgain`). Have shell and
presentation consume that diff instead of repeating field lists.

## Unconstrained feature direction

The better architecture is a policy compiler with three explicit products:

1. `DeclaredSurfaceContract`: validated manifest role, role capability,
   role-specific fields, localized title, and author constraints.
2. `EffectiveSurfacePolicy`: sparse settings plus manifest/default precedence,
   normalized props, source/provenance, diagnostics, and a monotonically
   revisioned snapshot.
3. `SurfaceTransitionPlan`: a semantic diff that tells the shell whether to
   keep, remeasure, reconfigure, recreate, or reject the change, including
   child surfaces, focus, input regions, and presentation readiness.

This would make surface policy a reusable contract for future output profiles,
multi-monitor placement, accessibility/input policy, remote display targets,
and GPU/Wayland backends. It also creates a clean place for capability-aware
policy: a requested window role or blur intent can be accepted, downgraded, or
rejected with an explicit diagnostic instead of silently falling back.

## Recommended implementation order

1. Add manifest semantic validation and tests for invalid enum values,
   role-specific fields, empty/invalid window identity, and margins.
2. Remove or formally protect user `promotable`; route settings role changes
   through the same authorized transition supervisor as explicit requests.
3. Create the canonical role-field metadata and complete surface ejection,
   diagnostics, and settings precedence behavior.
4. Define `SurfacePolicySnapshot` and typed semantic diffs; include blur,
   decorations, namespace, padding, keyboard mode, and all geometry inputs.
5. Replace split zero/measurement/physical-size logic with typed conversion at
   the shell/presentation boundary, preserving full-present and input-region
   invariants.
6. Add transactional reload/role/popup/focus tests, then extend multi-output
   and capability negotiation behavior.

## Regression matrix

| Area | Regression | Primary location |
| --- | --- | --- |
| Promotion | Non-promotable settings role change is rejected and leaves the old role | `crates/core/shell/src/shell/tests/window_role.rs` |
| Promotion | Promotable settings reload with an open popup tears down and recreates all dependent surfaces | `crates/core/shell/src/shell/tests/window_role.rs` / `popover.rs` |
| Manifest | Invalid role/anchor/layer/keyboard/decorations produce diagnostics and do not silently default | module manifest and graph tests |
| Schema | User settings/ejection warn or omit every inert role-specific field | `crates/core/surface-config/src/lib.rs` |
| Fingerprint | Blur-only and decoration-only changes reach the required presentation action | `crates/core/presentation/src/wayland_surface/backend/tests/config.rs` |
| Ejection | Localized title and derived app id have explicit round-trip semantics | `crates/core/surface-config/src/lib.rs` |
| Geometry | Zero/unmeasured/content/padded/physical extents never cross the wrong seam | shell surface-layout and presentation protocol tests |
| Reload | Invalid settings fall back without mutating runtime props or surface policy | shell component settings integration tests |
| Bounds | Large/negative margins and exclusive zones have checked, documented behavior | surface-config and presentation config tests |

## Verification performed

- `nix develop -c cargo test -p mesh-core-surface-config`: **28 passed, 0
  failed**.
- `nix develop -c cargo test -p mesh-core-shell --lib surface_layout`: **17
  passed, 0 failed**.
- `nix develop -c cargo test -p mesh-core-shell --lib window_role`: **8 passed,
  0 failed**.
- `nix develop -c cargo check -p mesh-core-surface-config -p mesh-core-shell
  -p mesh-core-presentation`: passed.

These green tests cover the current intended paths, including previous
visibility, first-measurement, padding, and role-transition regressions. They
do not cover the settings-role authorization, invalid-manifest diagnostics, or
blur/decoration fingerprint gaps listed above.
