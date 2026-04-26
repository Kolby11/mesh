# `@mesh/upower`

Power and battery backend implemented against **UPower** over the system D-Bus.

- **Type:** `backend`
- **Implements:** interface `mesh.power` (contract `@mesh/power-contract`)
- **Backend name:** `UPower`
- **Priority:** `100`
- **Entrypoint:** `src/main.luau`

## Capabilities

Required:

- `service.power.read` — battery level, charging state, time estimates, power draw
- `dbus.system` — UPower lives on the system bus

Optional:

- `service.power.control` — switch between power profiles when UPower exposes
  them. If denied, the backend operates in read-only mode.

## Responsibilities

Implements the methods declared by `mesh.power`:

- return a `battery()` snapshot (level, charging, time-to-empty, time-to-full,
  power draw, current/full charge) — consumed by shell frontends
- list and switch power profiles (if `service.power.control` is granted)
- emit the contract's events when state changes

## Fallback behavior

Frontends treat the service as optional. If UPower is not running, the power
service resolves to `nil` and surfaces display `"N/A"` rather than failing.
