---
phase: 02-service-proxy-delivery
reviewed: 2026-05-02T00:00:00Z
depth: standard
files_reviewed: 18
files_reviewed_list:
  - crates/core/runtime/scripting/src/context.rs
  - crates/core/shell/src/shell/component.rs
  - crates/core/shell/src/shell/service.rs
  - crates/core/extension/service/src/contract.rs
  - crates/core/extension/service/src/interface.rs
  - packages/plugins/backend/core/audio-interface/interface.toml
  - packages/plugins/backend/core/network-interface/interface.toml
  - packages/plugins/backend/core/power-interface/interface.toml
  - packages/plugins/backend/core/media-interface/interface.toml
  - packages/plugins/backend/core/networkmanager-network/src/main.luau
  - crates/tools/lsp/src/knowledge/mesh_api.rs
  - docs/plugins/backend/core/README.md
  - packages/plugins/frontend/core/panel/src/main.mesh
  - packages/plugins/frontend/core/quick-settings/src/main.mesh
  - packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh
  - packages/plugins/frontend/core/quick-settings/src/components/wifi-section.mesh
  - packages/plugins/frontend/core/quick-settings/src/components/bluetooth-section.mesh
  - docs/plugins/frontend/core/README.md
findings:
  critical: 3
  warning: 6
  info: 4
  total: 13
status: issues_found
---

# Phase 02: Code Review Report

**Reviewed:** 2026-05-02
**Depth:** standard
**Files Reviewed:** 18
**Status:** issues_found

## Summary

This phase delivers the service proxy runtime: interface contracts, the `require`/`mesh.service.use` proxy in the scripting context, field-level change tracking, and updated bundled frontend plugins that consume it. The architecture is sound — proxies read live payload tables, track field access, and route commands through published events. Several correctness defects and one architectural violation were found.

---

## Critical Issues

### CR-01: `update_local_audio_percent` violates architectural constraint — core injecting derived display state into script state

**File:** `crates/core/shell/src/shell/component.rs:689-708`

**Issue:** `update_local_audio_percent` mutates the Lua-side `audio` state object stored in each runtime's `ScriptState` by directly writing `percent` into a JSON object it reconstructs in core. This is explicitly prohibited by both `CLAUDE.md` and the documented architecture: "Frontend plugins must never read derived state that was injected by core." The function additionally writes an `audio` key that shadows the field-level proxy table (`__mesh_svc_audio`), which means during slider drag the proxy field reads (`audio.percent`) will return the core-injected value, not the backend-emitted one. This creates a split-source-of-truth bug: the proxy reflects core's guess while the service event handler updates `__mesh_svc_audio` with the actual backend payload.

The function is also coupled to a specific service name ("audio"), violating the "no hardcoded service names in core" rule:

```rust
// Lines 689–708 — writes directly into per-runtime ScriptState
fn update_local_audio_percent(&self, percent: u32) {
    let percent = percent.min(100);
    for runtime in self.runtimes.lock().unwrap().values_mut() {
        ...
        let mut audio = runtime.script_ctx.state().get("audio")   // reads core state key "audio"
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(obj) = audio.as_object_mut() {
            obj.insert("percent".into(), serde_json::Value::from(percent)); // injects display-ready field
        }
        runtime.script_ctx.state_mut().set("audio", audio);   // overwrites state
    }
}
```

**Fix:** Remove `update_local_audio_percent` entirely. The slider's optimistic UI during drag should be handled inside the frontend plugin script, not in core. The frontend plugin can keep a local `dragging_percent` global and use it in `onRender()`. Core's job is to forward the command event and apply the next backend emission. The function's call sites in `update_slider_from_position` (line 676) and `slider_release_request` (line 714) should be reviewed — the `mesh-action = "audio-volume"` special case in core is itself the root cause and should be removed.

---

### CR-02: `script_events_to_requests` — channel name split on last `.` can produce wrong interface name for deeply nested interfaces

**File:** `crates/core/shell/src/shell/service.rs:92-96`

**Issue:** The fallback arm that converts unknown published event channels to `ServiceCommand` uses `rfind('.')` to split on the last dot:

```rust
other => other.rfind('.').map(|pos| CoreRequest::ServiceCommand {
    interface: other[..pos].to_string(),
    command: other[pos + 1..].to_string(),
    payload: event.payload,
}),
```

A channel like `"mesh.audio.set_volume"` correctly splits to `interface = "mesh.audio"`, `command = "set_volume"`. However if a third-party interface were named `"alice.thermal.sensors.read"` the split would produce `interface = "alice.thermal.sensors"`, `command = "read"` — which may or may not be the intended interface name. More concretely: if a method is named with a dot in it (unlikely but not validated anywhere), the split silently produces an incorrect interface. This is a latent correctness issue rather than currently broken code, but the proxy itself explicitly encodes the channel as `format!("{}.{}", iface, method.name)` in `context.rs:871`, so a method name with a dot would cause a mismatch immediately.

There is no validation in `InterfaceMethod::name` or in `ContractMethodToml` that names are dot-free.

**Fix:** Validate method names at contract load time to reject names containing `.`:

```rust
// In load_interface_contract, after parsing methods:
for method in &contract.methods {
    if method.name.contains('.') {
        return Err(ContractError::Parse {
            path: path.clone(),
            source: toml::de::Error::custom(format!(
                "method name '{}' must not contain '.'", method.name
            )),
        });
    }
}
```

Alternatively, encode the command channel with a separator other than `.` to eliminate the ambiguity at the source.

---

### CR-03: `on_command_connect` / `on_command_disconnect` — shell command injection via unsanitized `id` field

**File:** `packages/plugins/backend/core/networkmanager-network/src/main.luau:136-158`

**Issue:** The `connect` and `disconnect` handlers validate the MAC address format for Bluetooth IDs (`id:match('^[%x:]+$') and #id == 17`), but the `else` branch for network UUIDs passes `id` directly to `nmcli connection up uuid <id>` and `nmcli connection down uuid <id>` via `mesh.exec_shell`. If `id` contains shell metacharacters (spaces, semicolons, backticks, `$(...)` etc.), this is a command injection. UUID values arrive from frontend payloads, which originate from `pcall`-wrapped Lua code that reads from the service proxy — the data ultimately comes from the backend's own `nmcli` output, but a malicious plugin or crafted service payload could supply arbitrary strings.

```lua
-- Line 141-142 — id is not sanitized before interpolation into shell string
local result = mesh.exec_shell('nmcli connection up uuid ' .. id)
```

**Fix:** Use `mesh.exec` with a split argument array instead of `mesh.exec_shell` with string interpolation:

```lua
local result = mesh.exec('nmcli', {'connection', 'up', 'uuid', id})
```

This prevents the shell from interpreting metacharacters in `id`. Apply the same fix to `disconnect` (line 156) and also to `on_command_set_wifi_enabled` (line 196), which passes a hardcoded safe string but should use `mesh.exec` for consistency and future safety.

---

## Warnings

### WR-01: `require` version parsing — `rsplit_once('@')` can misparse module paths that legitimately contain `@` in the middle

**File:** `crates/core/runtime/scripting/src/context.rs:551-558`

**Issue:** The version-splitting logic uses `rsplit_once('@')` and then checks whether the left part starts with `@mesh.`, `@mesh/`, or `mesh.`. This works correctly for `"@mesh/audio@>=1.0"` but would misparse a hypothetical module like `"@mesh/foo@bar@>=1.0"` — `rsplit_once` would split on the last `@`, giving `left = "@mesh/foo@bar"` and `right = ">=1.0"`. While no current module names contain internal `@`, this is a silent correctness assumption with no validation or error path. The version is also only extracted inside a `starts_with` check so non-`@mesh` modules never get versions, which could silently drop version constraints.

**Fix:** Use `split_once('@')` on only the suffix portion (after the module name terminates), or parse the module string with an explicit grammar: `<module-name>@<version>` where `<module-name>` is everything up to the first `@` after any namespace prefix.

---

### WR-02: `ScriptState::keys()` — `contains` check is O(n) per proxy key, causing O(n*m) key deduplication

**File:** `crates/core/runtime/scripting/src/context.rs:138-148`

**Issue:** `keys()` iterates all proxy keys and calls `Vec::contains` (O(n)) for each one, making the full deduplication O(n*m) where n is the number of local variables and m is the number of proxies. In practice proxy counts are small, but this function is called during every `sync_state_from_lua` pass. More importantly, the merge can silently drop a proxy key if the same name exists as a local variable — the code states "Proxies may shadow local variables" but `keys()` returns local variables first and only appends proxy keys if not already present, which is the opposite of shadow semantics: local wins over proxy in `keys()`, but proxy wins over local in `get()`. This inconsistency means a template iterating `keys()` would see a stale local-variable copy of a field that the proxy has overridden.

**Fix:** Either clearly document that `keys()` returns proxy keys before local keys (giving them precedence consistently with `get()`), or restructure so proxy keys are emitted first:

```rust
fn keys(&self) -> Vec<String> {
    let mut keys: Vec<String> = self.proxies.keys().cloned().collect();
    for k in self.variables.keys() {
        if !self.proxies.contains_key(k.as_str()) {
            keys.push(k.clone());
        }
    }
    keys
}
```

---

### WR-03: `FrontendSurfaceComponent::render_import` — props values are always coerced to `String`, dropping typed values passed from parent templates

**File:** `crates/core/shell/src/shell/component.rs:1184-1188` and `1227-1230`

**Issue:** In `render_import`, all props from the host template are converted to `serde_json::Value::String` regardless of their actual type:

```rust
let props_json: HashMap<String, serde_json::Value> = props
    .iter()
    .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
    .collect();
```

The template syntax `<WifiSection wifi_enabled="{wifi_enabled}" />` passes `wifi_enabled` as a string `"true"` or `"false"`. The child component then reads it as a string, not a boolean. The `wifi-section.mesh` script reads the prop as `wifi_enabled` (which is a Lua global populated from `state.set(key, Value::String("true"))`). Any script code checking `if wifi_enabled then ...` would always evaluate to truthy since a non-empty string is truthy in Lua, but checking `if wifi_enabled == false then ...` would be wrong. This is a systemic type-loss issue for all boolean and numeric props.

**Fix:** Attempt to deserialize the string value to a typed JSON value before falling back to string:

```rust
let props_json: HashMap<String, serde_json::Value> = props
    .iter()
    .map(|(key, value)| {
        let typed = serde_json::from_str::<serde_json::Value>(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.clone()));
        (key.clone(), typed)
    })
    .collect();
```

This would correctly parse `"true"` → `Bool(true)`, `"42"` → `Number(42)`.

---

### WR-04: `networkmanager-network` — `on_command_wifi_scan` emits `devices: {}` (empty), wiping device list

**File:** `packages/plugins/backend/core/networkmanager-network/src/main.luau:180-188`

**Issue:** `on_command_wifi_scan` emits a full payload with `devices = {}` (an empty table), which overwrites the device list that was populated by `on_poll`. Any frontend component reading `network.devices` after a WiFi scan will see an empty list until the next poll cycle (up to 15 seconds later). The scan command should either emit the full device list or not include `devices` in its response (if the protocol allows partial updates).

```lua
mesh.service.emit({
    available    = true,
    wifi_enabled = fetch_wifi_enabled(),
    connections  = fetch_connections() or {},
    devices      = {},          -- ← wipes net_devices and bt_devices
    networks     = networks,
    source_plugin = '@mesh/networkmanager',
})
```

**Fix:**

```lua
mesh.service.emit({
    available    = true,
    wifi_enabled = fetch_wifi_enabled(),
    connections  = fetch_connections() or {},
    devices      = fetch_net_devices() or {},
    networks     = networks,
    source_plugin = '@mesh/networkmanager',
})
```

---

### WR-05: `locale_changed` clears all runtimes but does not re-apply service payloads, so proxy reads return nil until the next backend emission

**File:** `crates/core/shell/src/shell/component.rs:1510-1516`

**Issue:** `locale_changed` calls `self.runtimes.lock().unwrap().clear()` and then `init_root_runtime()`, which creates a fresh `ScriptContext`. This new context has no service payloads applied (no `__mesh_svc_<name>` globals set), so any `onRender()` call that reads proxy fields will return `nil` for all fields until the backend emits again. On a 15-second poll interval this could leave the UI in a degraded state for a significant duration after a locale switch.

**Fix:** After `init_root_runtime()`, re-apply the last known service payloads to the new runtimes. The component would need to cache the most recent payload per service (or request a re-broadcast from the shell).

---

### WR-06: `call_namespaced_handler` drops `self.dirty = true` assignment after the early return for an empty `runtimes` get

**File:** `crates/core/shell/src/shell/component.rs:1094-1110`

**Issue:** When the runtime for `instance_key` does not exist, `call_namespaced_handler` returns `Ok(Vec::new())` without setting `self.dirty = true`. When the runtime exists and the handler succeeds, `self.dirty = true` is set unconditionally. The flag assignment happens at line 1105 which is after the `?` propagation of `call_handler`. If `call_handler` returns an error, `dirty` is not set — which is correct. But if it returns `Ok`, `dirty` is always set even if no state actually changed. This is harmless (extra repaint) but documents that the dirtyness tracking here is coarser than the tracked-field logic in `handle_service_event`. More concretely: the missing dirty flag on the "runtime not found" path means the surface may not repaint after a click that targets a component whose runtime was evicted or not yet created.

**Fix:** Consider whether "handler not found" should still mark dirty (it likely shouldn't). Document the intent or add a comment. If the handler truly made no state changes the surface wastes a repaint frame.

---

## Info

### IN-01: `mesh_api.rs` documents `mesh.state.set` and `mesh.state.get` but these are not installed by `install_host_api`

**File:** `crates/tools/lsp/src/knowledge/mesh_api.rs:10-23`

**Issue:** The LSP knowledge base documents `mesh.state.set` and `mesh.state.get` as available APIs, but `install_host_api` in `context.rs` does not install any `mesh.state` sub-table. The actual mechanism for reactive state is bare global assignment, not `mesh.state.set(...)`. This LSP documentation is misleading — if a user writes `mesh.state.set("foo", 42)` following the docs, they will get a runtime error (nil indexing on a missing `state` sub-table of `mesh`).

**Fix:** Either remove these entries from `MESH_API_ENTRIES` or install a stub `mesh.state` table that forwards to global assignment/read with a deprecation warning.

---

### IN-02: `quick-settings/src/main.mesh` — `sync_nav_classes` sets reactive globals `wifi_nav_class`, `bt_nav_class`, `audio_nav_class` but the template does not use them

**File:** `packages/plugins/frontend/core/quick-settings/src/main.mesh:96-100`

**Issue:** `sync_nav_classes` sets three reactive globals (`wifi_nav_class`, `bt_nav_class`, `audio_nav_class`) that are not referenced anywhere in `<template>`. The nav buttons use `class="nav-btn"` statically, not `class="{wifi_nav_class}"`. These reactive writes pollute the state diff, cause unnecessary comparisons in `tracked_service_fields_changed`, and are dead code.

**Fix:** Either wire the class attributes in the template to these variables, or remove `sync_nav_classes` and its three call sites.

---

### IN-03: `networkmanager-network/src/main.luau` — `on_command_connect` applies Bluetooth routing based on MAC address pattern, which is not declared in the network interface contract

**File:** `packages/plugins/backend/core/networkmanager-network/src/main.luau:129-143`

**Issue:** The `connect` command handler silently bifurcates: if `id` looks like a MAC address it uses `bluetoothctl`, otherwise `nmcli`. This coupling of Bluetooth and Wi-Fi/Ethernet into a single network backend is an undocumented behavior — the `network-interface` contract's `connect` method says nothing about Bluetooth IDs. A frontend that constructs a `connection_id` argument from `network.devices` (which now includes Bluetooth devices mixed in) will inadvertently invoke Bluetooth operations through the network service command. The `network-interface/interface.toml` should document this behavior in its `connect` method, or Bluetooth should be a separate interface.

**Fix:** Document in `interface.toml` that `connect` accepts both network UUIDs and Bluetooth MAC addresses, or separate Bluetooth device management into its own interface contract.

---

### IN-04: `context.rs` — `default_lua_value_for_type` returns `LuaValue::Integer(0)` for `"float"` type

**File:** `crates/core/runtime/scripting/src/context.rs:934`

**Issue:** The `"float"` type arm returns `LuaValue::Integer(0)` instead of `LuaValue::Number(0.0)`. These are distinct Lua types. A script that checks `type(audio.volume) == "number"` would pass, but code that calls `:format("%.2f", result)` on the default return from a method declared as returning `float` would behave differently than when a real float is returned. This is a minor type mismatch but creates inconsistency between the default return value and a real backend emission.

**Fix:**

```rust
"float" | "number" => LuaValue::Number(0.0),
"integer" | "int" => LuaValue::Integer(0),
```

---

_Reviewed: 2026-05-02_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
