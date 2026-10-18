# Backend host events

Backend providers acquire native event sources inside `start(self)`. Each source
returns a stream handle with `id`, `generation`, `program`, `args` and `status`.
A missing capability returns `false`; malformed arguments raise a diagnostic.
Native connection and registration happen asynchronously. `started` confirms
registration; `failed` followed by `exited` reports a connection or source error.

| API | Capability | Records |
| --- | --- | --- |
| `mesh.dbus_subscribe(bus, filter)` | `dbus.session` or `dbus.system` | JSON signal records |
| `mesh.pipewire_subscribe()` | `pipewire.observe` | JSON Node/Metadata invalidations |
| `mesh.netlink_subscribe(groups)` | `net.netlink` | `changed` per route datagram |
| `mesh.wayland_subscribe("foreign-toplevel")` | `wayland.observe` | JSON wlr foreign-toplevel records |
| `mesh.watch_path(absolute_path)` | `fs.watch` | `modified` per inotify wakeup |
| `mesh.socket_stream(absolute_path)` | `net.socket` | Newline-delimited socket records |

These APIs observe protocols. Providers keep domain interpretation, state reads,
commands, and fallback policy in Luau. They do not grant domain control access.

## Delivery and lifetime

`on_stream_event(self, event)` receives `started`, `line`, `eof`, `failed`,
`exited` and `overflow` records, with the owning handle in `event.stream`.
The line payload is `event.line`. Alternatively, define
`on_stream_batch(self, program, lines)` or `on_stream_line(self, program, line)`
and `on_stream_closed(self, program, success)`.

Stream callbacks run as coroutines. They may use `task.sleep`, `mesh.exec`,
`mesh.read_file` and `mesh.socket_request`; commands and independent tasks can
continue while a callback waits. Later stream callbacks wait until the current
one completes. Adjacent legacy lines from the same handle form one ordered batch.
Only successful completion publishes a snapshot or buffered provider events.
Suspended callbacks and scheduled resumptions retain their own publications; failure or cancellation discards
them. Runtime callback budgets apply to each resumption.

Sources share the generation's stream admission, queue and byte budgets. Records
are limited to 64 KiB; source-specific bounds allow 512 PipeWire Node/Metadata
objects and 1,024 Wayland toplevel handles. PipeWire owns one native loop on a
blocking executor thread per admitted subscription; it does not occupy a D2 host
I/O worker. Other native sources wait through Tokio readiness.

`mesh.close_stream(handle)` requests cancellation of an owned stream and returns
whether cancellation was requested. A handle from another generation returns
`false`. Normal provider shutdown cancels and awaits every source. Dropping the
runtime also wakes native loop cancellation. No monitor subprocess is needed for
native subscriptions.

An overflow means consumers must resynchronize their domain state. Kernel inotify
overflow, route receive overflow/truncation, oversized records, connection loss,
and object-limit exhaustion fail the source. Scripts should use `exited` to start
a monitor or poll fallback, including when asynchronous registration fails.

`mesh.service.set_poll_interval(0)` disables periodic polling. Positive values
retain the 50 ms minimum. A provider may retain a slow safety poll while using
native events for prompt changes. `stop(self)` can also yield; teardown cancels
previous tasks and gives the stop hook a two-second deadline before cleanup.

## D-Bus filters

`bus` is `"system"` or `"session"`. The filter requires `sender` and
`path_namespace`; optional `interface` and `member` narrow delivery. Names and
paths are validated using the D-Bus type system; arbitrary match-rule strings
are not accepted. For example:

```lua
state = { last_path = "" }
local source
function start(self)
    source = mesh.dbus_subscribe("system", {
        sender = "org.freedesktop.NetworkManager",
        path_namespace = "/org/freedesktop/NetworkManager",
        interface = "org.freedesktop.DBus.Properties",
        member = "PropertiesChanged",
    })
end

function on_stream_event(self, event)
    if not source or event.stream.id ~= source.id then return end
    if event.type == "line" then
        local signal = mesh.json.decode(event.line)
        state.last_path = signal.path
        -- Use the protocol fields to choose which domain state to refresh.
    elseif event.type == "exited" then
        mesh.service.set_poll_interval(5000)
    end
end
```

Signal records carry `path`, `interface`, `member`, `signature` and `body`.
The signature is zvariant's parsed body signature (multiple arguments are a
structure, such as `(su)`). Body entries use zvariant's JSON representation,
including each value's signature and value. Empty bodies are empty arrays.
D-Bus uses a bounded 64-message subscription queue before the shared stream queue.

## Other protocol records

PipeWire records have `event` and the registry object's `id`: `added`, `removed`,
`node-params` for subscribed Node Props, and `metadata`. Client connect/disconnect
noise is excluded. Providers read the relevant state after these invalidations;
records do not contain audio samples or implement volume policy.

Netlink supports route groups `link`, `ipv4-address`, `ipv6-address`,
`ipv4-route` and `ipv6-route`. At least one is required. Only kernel-originated
datagrams are accepted. `changed` is an invalidation, not a decoded network model.

Wayland currently supports `zwlr_foreign_toplevel_manager_v1` up to version 3.
Records have `event` and a connection-local `id`: `added`, `title`, `app-id`,
`state`, `done` and `closed`. Title/app-id values are strings; state values are
protocol enum numbers. `done` delimits a batch. Unsupported compositors fail
locally so providers can select compositor-specific socket or polling fallbacks.
