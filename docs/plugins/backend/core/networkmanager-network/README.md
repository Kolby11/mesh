# `@mesh/networkmanager`

Network backend implemented against **NetworkManager** via the `nmcli` CLI.

- **Type:** `backend`
- **Implements:** interface `mesh.network` (contract `@mesh/network-contract`)
- **Backend name:** `NetworkManager`
- **Priority:** `100`
- **Entrypoint:** `src/main.luau`

## Capabilities

Required:

- `service.network.read` — read devices, active connections, Wi-Fi scan results
- `service.network.control` — connect, disconnect, manage connections
- `dbus.system` — reserved for the planned native NetworkManager integration
- `nmcli` — required for device, connection, and Wi-Fi control in the current implementation

## Responsibilities

Implements the methods declared by `mesh.network`:

- list devices and active connections (`active_connection()` is consumed by
  `@mesh/panel`)
- scan Wi-Fi networks and initiate connections
- toggle radios (Wi-Fi, Bluetooth-coordinated state if applicable)
- emit the contract's events so frontend surfaces redraw the status icon

## Current status

The checked-in backend is still a shell-based placeholder. It depends on
`nmcli` being present on the host and reports itself unavailable when that
tool is missing. A direct D-Bus implementation remains the long-term target.

## Notes

`dbus.system` is a **high-privilege** capability (see
[`spec/pluggable-backend.md`](../../../../../spec/pluggable-backend.md)). Only
trusted, signed backends should request it.
