---
status: planned
type: quick
created: 2026-07-15
slug: parameterize-brightness-component-scroll
---

# Quick Plan: Parameterize Brightness Component Scroll Scaling

## Goal

Expose the brightness button's per-scroll brightness amount as a typed component prop that can later be supplied by settings, while retaining the current default of five percentage points for both wheel and two-finger input.

## Tasks

### 1. Add the typed brightness scroll-sensitivity prop

**Files:**

- `modules/frontend/navigation-bar/src/components/brightness-button.mesh`
- `modules/frontend/navigation-bar/src/main.mesh`

Add a numeric `scroll_sensitivity` prop using the existing `<props>` convention, with a default of `5` and explicit sensible `min`, `max`, and `step` metadata so it is ready for the generated settings path. Keep the setting owned by the brightness component; only adjust the navigation-bar embed if an explicit instance binding is required by the existing component-prop resolution model.

Replace the hard-coded `5` in the shared wheel/two-finger handler with one normalized amount used consistently for optimistic display state and the `mesh.brightness` `increase`/`decrease` command. Normalize defensively at the point of use: coerce with `tonumber`, reject non-finite or non-positive values, clamp to the declared range, and fall back to `5`. Do not change normalized input direction or start multiplying by raw `dy`; the default must remain one five-point adjustment per handled scroll event.

**Acceptance:**

- The component declares a settings-ready numeric sensitivity/scale prop.
- Omitting the prop preserves the current `{ "amount": 5 }` behavior.
- A valid override changes both the optimistic label/icon level and service-command amount for wheel and trackpad paths.
- Invalid runtime values cannot emit zero, negative, non-finite, or unreasonably large brightness amounts.

### 2. Extend shipped-surface regression coverage

**File:** `crates/core/shell/src/shell/component/tests/integration/real_surfaces.rs`

Extend the focused navigation brightness integration coverage to prove the declared default remains `5` for wheel and two-finger scroll, then initialize the real component with a non-default settings/instance prop value and assert both directions emit the configured amount. Include a defensive case for an invalid override (or the nearest invalid value that can reach the runtime after typed validation) and verify the handler safely falls back/clamps rather than producing an invalid service payload.

**Acceptance:**

- Tests exercise the real shipped navigation-bar composition rather than only a synthetic script.
- Default wheel and two-finger assertions continue to pass unchanged.
- A non-default sensitivity produces the expected `mesh.brightness` command payload.
- Defensive normalization has focused regression proof.

## Verification

- `nix develop -c cargo test -p mesh-core-shell shipped_navigation_brightness -- --nocapture`
- `nix develop -c cargo test -p mesh-core-shell props -- --nocapture`
- `cargo fmt --check`

## Constraints and Risks

- Keep this change local to component configuration and existing brightness behavior; do not add the settings UI or a new global configuration system.
- Component props may arrive as strings at embedded runtime boundaries, so script-side numeric coercion is required even though `<props>` performs typed validation.
- Preserve the current sign convention: positive `dy` increases and negative `dy` decreases brightness.
