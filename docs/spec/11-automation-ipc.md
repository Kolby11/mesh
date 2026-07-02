# 11 — Automation IPC

> Part of the [MESH Specification](README.md).

One capability-gated IPC surface lets external processes observe and drive
the shell: scripting tools, test harnesses, and the MCP server for LLMs
([12](12-mcp.md)). It is deliberately **not** a new model — it re-exposes
existing internals: the semantic tree ([09](09-accessibility.md)), element
actions (the `refs.*` action set, [03 §1](03-components.md)), shell-owned
surface channels, and the settings store ([08](08-settings.md)).

**Status: target.** The building blocks are shipped (semantic tree,
`ElementAction` routing, shell channels, settings engine); the socket, the
protocol, and the grant flow are the work.

## 1. Transport & protocol

- Unix domain socket at `$XDG_RUNTIME_DIR/mesh/automation.sock` (mode
  `0700` directory; socket owned by the user — filesystem permissions are
  the ambient authentication layer).
- JSON-RPC 2.0, newline-delimited, over the socket. Methods are
  request/response; subscriptions deliver JSON-RPC notifications — the same
  "methods + typed events" primitive as the module event bus
  ([01 §4.3](01-module-system.md)).
- `automation.hello` is mandatory first: client name, protocol version,
  requested scopes. The shell answers with granted scopes and the protocol
  version. Versioning is semver; additive = minor.

## 2. Scopes — read and act are separate grants

| Scope | Grants | Privilege |
| ----- | ------ | --------- |
| `automation.read` | Tree snapshots, surface list, settings reads, event subscriptions | `elevated` |
| `automation.act` | Element actions, surface control, settings writes | `high` |

Grants are per client name, persisted as shell settings
(`shell.automation.clients`), and prompted through core-owned trusted UI on
first connect (modules cannot draw over the consent dialog —
[01 §7](01-module-system.md)). A connection with no grant can call only
`hello`. Everything an automation client does is attributable in
diagnostics (client name on every logged action).

Modules themselves do not get this IPC — module-side automation is the
normal capability-gated host API. This surface is for *external* processes.

## 3. Read surface (`automation.read`)

### 3.1 Semantic tree

```
tree.snapshot { surface?, include_geometry? } -> { surfaces: [SemanticNode…] }
tree.find     { query } -> [NodeRef…]
```

`SemanticNode` is the accessibility node ([09 §1](09-accessibility.md)):
`node_ref`, role, name, description, value, states, geometry (last-painted
layout box), children. `node_ref` is stable for the node's retained
lifetime: `surface_id` + the retained node key.

`tree.find` accepts structured queries (`role=button`,
`name~="volume"`, `surface=@mesh/navigation-bar#top`) so clients don't
re-implement tree walking.

### 3.2 State reads

```
surfaces.list  {} -> [{ id, module, instance, visible, anchor, layer, geometry }]
settings.get   { namespace, key? } -> effective value + supplying layer
modules.graph  {} -> the mesh.debug.module_graph payload (uses/provides/health)
```

### 3.3 Events

```
subscribe { channels: ["tree.changed", "surface.shown", "surface.hidden",
                       "module.health", "settings.changed", "interface.health/*"] }
```

`tree.changed` is damage-coalesced (per surface, debounced) — a firehose of
per-node diffs is explicitly out of scope for v1.

## 4. Act surface (`automation.act`)

### 4.1 Element actions

Exactly the `refs.*` action set, routed through the same shell paths as
script-initiated actions (synthesized clicks, real focus transfer, shared
scroll offsets, input values):

```
element.click            { node_ref, options? }
element.focus / blur     { node_ref }
element.set_value        { node_ref, value }
element.scroll_into_view { node_ref, options? }
element.scroll_to        { node_ref, top, left?, options? }
```

Actions on stale `node_ref`s fail with a structured error (`stale_node`);
clients re-snapshot and retry. No coordinate-based synthetic input in v1 —
semantic targeting only (this is what keeps automation robust to theme and
layout changes).

### 4.2 Surface control

Thin wrappers over the declared shell channels
(`shell.show-surface` / `hide` / `toggle`, `shell.set-theme`,
`shell.set-locale`):

```
surface.show / hide / toggle { surface }
shell.publish { channel, payload }     # declared shell.* channels only
```

`shell.publish` validates the channel against the declared shell-owned list;
interface-domain commands are *not* exposed as raw publishes — drive UI or
use settings, the same rule modules live under
([01 §4.3](01-module-system.md)).

### 4.3 Settings writes

```
settings.set   { namespace, key, value }
settings.unset { namespace, key }
```

Validated against the owning props declaration like every other writer
([08 §1](08-settings.md)).

## 5. Testing & tooling consumers

The IPC is the intended substrate for end-to-end shell tests (drive a real
surface semantically, assert tree state), `mesh` CLI subcommands
(`mesh tree`, `mesh settings` against a running shell), and the MCP server.
One surface to harden instead of three.

## 6. Non-goals

- No screen capture in v1 (`shell.screenshot` stays a separate high
  capability decision).
- No raw input injection (keyboard/pointer event synthesis) — semantic
  actions only.
- No remote (TCP) transport; local socket only. Anything remote wraps the
  socket with its own auth.
