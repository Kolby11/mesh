# `@mesh/networkmanager`

Network backend implemented against **NetworkManager** over the system D-Bus.

- **Type:** `backend`
- **Implements:** interface `mesh.network` (contract `@mesh/network-contract`)
- **Backend name:** `NetworkManager`
- **Priority:** `100`
- **Entrypoint:** `src/main.luau`

## Capabilities

Required:

- `service.network.read` — read devices, active connections, Wi-Fi scan results
- `service.network.control` — connect, disconnect, manage connections
- `dbus.system` — high-privilege, required to talk to the NetworkManager daemon

## Responsibilities

Implements the methods declared by `mesh.network`:

- list devices and active connections (`active_connection()` is consumed by
  `@mesh/panel`)
- scan Wi-Fi networks and initiate connections
- toggle radios (Wi-Fi, Bluetooth-coordinated state if applicable)
- emit the contract's events so frontend surfaces redraw the status icon

## Notes

`dbus.system` is a **high-privilege** capability (see
[`spec/pluggable-backend.md`](../../../../../spec/pluggable-backend.md)). Only
trusted, signed backends should request it.
