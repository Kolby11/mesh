# Backend host events

Backend providers acquire native event sources inside `start(self)`. Each source
returns a stream handle with `id`, `generation`, `program`, `args` and `status`.
A missing capability returns `false`; malformed arguments raise a diagnostic.
Native connection and registration happen asynchronously. `started` confirms
registration; `failed` followed by `exited` reports a connection or source error.

| API | Capability | Records |
| --- | --- | --- |
| `mesh.dbus_subscribe(bus, filter)` | `dbus.subscribe:<bus>:<sender>` matching `filter.sender` | JSON signal records |
| `mesh.pipewire_subscribe()` | `pipewire.observe` | JSON Node/Metadata invalidations |
| `mesh.netlink_subscribe(groups)` | `net.netlink` | `changed` per route datagram |
| `mesh.wayland_subscribe("foreign-toplevel")` | `wayland.observe` | JSON wlr foreign-toplevel records |
| `mesh.wayland_subscribe("workspace")` | `wayland.observe` | JSON ext-workspace records |
| `mesh.watch_path(absolute_path)` | `fs.watch:<pattern>` matching the path | `modified` per inotify wakeup |
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
them. Runtime callback budgets apply to each resumption, including its final
command result and state snapshot. Transient output charges are released when
the resumption ends; queued publications remain charged until consumed or
discarded. Standalone snapshot exports have their own callback budget scope.

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

## D-Bus method calls

`mesh.dbus_call(bus, call)` sends one method call and returns its reply. It
needs a `dbus.call:<bus>:<destination>:<interface>` grant matching the call;
`*` as the destination permits any bus name for that interface. A read-only
`dbus.read:<bus>:<destination>:<interface>` grant permits only
`org.freedesktop.DBus.Properties.Get`/`GetAll` whose first argument is that
interface, plus `org.freedesktop.DBus.Introspectable.Introspect`, at that
destination; UPower and NetworkManager read their state this way. `GetAll`
replies carry `body[1].value[<Property>].value`.

```lua
local reply = mesh.dbus_call("system", {
    destination = "org.freedesktop.UPower",
    path = "/org/freedesktop/UPower/devices/DisplayDevice",
    interface = "org.freedesktop.DBus.Properties",
    member = "Get",
    signature = "ss",
    args = { "org.freedesktop.UPower.Device", "Percentage" },
})
if reply.success then
    local percentage = reply.body[1].value.value
end
```

`signature` describes `args` and is omitted for a call without arguments.
Arguments map from Luau by type: integers for `y n q i u x t` (range-checked),
numbers for `d`, booleans, strings for `s o g`, arrays for `a…`, arrays of fields
for structures, and tables with string keys for dictionaries (integer and boolean
keys are written as strings). A variant argument is `{ signature = "s", value = … }`.
File descriptors are not supported. Malformed arguments fail without contacting
the bus. Values beyond 2^53 lose precision in Luau numbers.

The reply has `success`, `signature`, `body`, `error` and `error_name`. `body`
uses the same `{signature, value}` entries as signal records, so a reply value can
be passed back as a variant argument. A D-Bus error reply sets `error_name` (for
example `org.freedesktop.DBus.Error.UnknownMethod`); denied grants, connection
failures, the callback timeout and replies over the output limit set only `error`.

In a coroutine the call suspends on the bounded host worker pool like
`mesh.socket_request`; elsewhere it blocks the backend callback. Each call opens a
private bus connection, so it suits occasional reads and controls rather than
per-frame polling.

## HTTP requests

`mesh.http_request(spec)` requires `net.http` and a started backend. It yields
on the bounded host I/O workers inside a coroutine; synchronous callers retain
blocking behaviour with their frame clock paused. The request accepts `url`,
`method` (default `GET`), string `headers`, UTF-8 `body`, `timeout_ms` (default
15,000; 1–60,000), and `cache` (default `false`). Only HTTP/HTTPS URLs without
embedded credentials are accepted. TLS certificates are verified; requests starting with HTTPS cannot
redirect to HTTP. At most five redirects are followed, and the deadline includes
response-body reads. Request bodies are limited to 1 MiB, with bounded URL and
header sizes.

```lua
local reply = mesh.http_request({ url = "https://example.org/status", timeout_ms = 10000 })
if reply.success then
    local status = mesh.json.decode(reply.body)
end
local image = mesh.http_request({ url = "https://example.org/art.png", cache = true })
-- image.path is a local file suitable for <image src>.
```

The result has `success`, `status`, `headers`, `body`, `path`, and `error`.
HTTP errors retain the status and text body; transport errors have status zero.
Ordinary bodies must be UTF-8 and fit the callback's JSON/output budgets.
`cache = true` is GET-only, reads at most 8 MiB, and returns a content-addressed
file under `$XDG_CACHE_HOME/mesh/http/<module hash>` (or `$HOME/.cache/...`),
with an empty body. Binary files are mode 0600; each module's disk cache is
bounded to 32 MiB and 128 files. Cached files may be evicted; consumers should
retain their current result, not an unbounded map of old paths. Failed HTTP
responses do not create cache entries. Both weather and optional MPRIS remote
art use this API; neither requires `curl`.

## D-Bus services

`mesh.dbus_serve(bus, spec)` owns one well-known name and exports one interface
at one object path. It needs a `dbus.own:<bus>:<name>` grant and returns a stream
handle, or `false` without the grant. The name is requested without queueing; a
name another client owns fails the stream (`replace = true` asks a current owner
that allows it to hand the name over). Stopping the stream releases the name.

```lua
local service
function start(self)
    service = mesh.dbus_serve("session", {
        name = "org.example.Echo",
        path = "/org/example/Echo",
        interface = "org.example.Echo",
        methods = { Echo = { ["in"] = "s", out = "s" } },
        signals = { Echoed = "s" },
    })
end

function on_stream_event(self, event)
    if not service or event.stream.id ~= service.id or event.type ~= "line" then return end
    local call = mesh.json.decode(event.line)
    local text = call.body[1].value
    mesh.dbus_reply(service, call.id, { text })
    mesh.dbus_emit(service, "Echoed", { text })
end
```

Each call to a declared method arrives as a `line` record with `type = "call"`,
`id`, `member`, `sender`, `signature` and `body` (the `{signature, value}` shape
used by signal records). The host rejects calls whose signature differs from
the declared `in` with `InvalidArgs`, answers `Introspect` (generated from the
spec) and `Ping`, and answers anything else with `UnknownMethod`.

`mesh.dbus_reply(handle, id, args)` encodes `args` against the method's `out`
signature; `mesh.dbus_reply_error(handle, id, error_name, message)` sends a D-Bus
error. Each call is answered once; one the provider leaves unanswered gets
`NoReply` after 25 seconds, and more than 64 unanswered calls are refused with
`LimitsExceeded`. `mesh.dbus_emit(handle, member, args)` emits a declared signal.
All three raise an error for malformed values and return `false` for a handle from
another generation. Arrays longer than 1,024 items cannot travel in a call
record. One whose items are all byte values (raw pixel data, for example) is
registered as an `application/octet-stream` [blob](#blobs) owned by the provider
and arrives as `{ blob = <handle>, len = <length> }`, which `mesh.blob.put`
accepts in place of bytes; any other arrives as `{ elided = <length> }`.

Service specs can also declare read-only properties as
`properties = { Enabled = { signature = "b", value = false } }`. The host
validates initial values, serves `Properties.Get`/`GetAll`, rejects `Set`, and
includes properties in introspection. `mesh.dbus_set_property(handle, name,
value)` validates the existing declared type, updates the value and emits
`PropertiesChanged` when it changes. A stale generation returns `false`.

For clients with dynamic bus names, `dbus.read:session:*:<interface>` authorizes
only that interface's property reads. `dbus.subscribe:session:*:<interface>`
authorizes filters with that exact interface and a concrete sender/path; it
does not grant unrestricted bus observation.

## StatusNotifier tray provider

`@mesh/status-notifier-tray` implements `mesh.tray` from
`@mesh/tray-interface`. Select it through normal provider bindings; it does not
add a tray surface or take over a running desktop watcher by default. It owns a
persistent host name, joins an existing KDE/freedesktop watcher when available,
and otherwise exports the corresponding watcher. Registration accepts service
names or a sender-relative object path. The provider publishes item metadata,
tracks item signals and bus-owner loss, retries watcher connections, and
forwards `activate`, `secondary_activate`, `context_menu` and `scroll` commands.
Menu-only items activate through `ContextMenu`.

Items expose semantic icon names (including attention and overlay names),
tooltip text, a menu object path and icon-theme path. A pixmap-only item
exposes `icon_pixmap`, a [blob](#blobs) of the pixmap closest to 32px (at most
512×512) in `image/x-mesh-argb32`; pass it to `<image src>`. Names are
preferred. Registration is bounded to 32 items, and published metadata is
bounded. Consumers render these fields using their own modules; a composed
DBusMenu UI is not included. See the [StatusNotifier specification](https://specifications.freedesktop.org/status-notifier-item/latest-single/).

## Blobs

Image bytes do not travel in service state. A provider registers them with
`mesh.blob.put(bytes, mime, meta?)`, which returns a content-addressed handle,
`mesh-blob:sha256-<64 hex digits>`, and puts that handle in state. Contracts
type such fields `blob` (usually `blob?`); the shell rejects a value that is
not a well-formed handle. Consumers pass the handle to `<image src>`. The
renderer decodes and scales it on the resource worker, and only the elements
showing it repaint when it is ready.

```lua
local icon = mesh.blob.put(pixels, "image/x-mesh-argb32", { width = 32, height = 32 })
state = { items = { { id = "app", icon = icon } } }
```

`bytes` is a string, a `buffer`, an array of byte values (what D-Bus `ay`
decodes to), or a `{ blob = handle }` record the host registered (see
[D-Bus services](#d-bus-services)); the last gives the provider's own bytes a
format without copying them through Luau. Accepted types are `image/png`,
`image/jpeg`, `image/bmp`, and the raw layouts `image/x-mesh-argb32` (A, R, G, B
bytes, the StatusNotifier layout), `image/x-mesh-rgba8` and `image/x-mesh-rgb8`.
Raw layouts need `width` and `height` (1–4096) and take an optional row
`stride`. `application/octet-stream` is accepted but never displayed. A blob
holds at most 8 MiB and a provider at most 32 MiB; `put` raises an error past
either limit or for malformed bytes.

Lifetime follows published state. Each state snapshot pins exactly the handles
it references; a snapshot that references a handle this provider did not
register is rejected, so state never points at bytes that have not arrived. A
blob no snapshot references is released by the second publication after its
`put`, which leaves a callback time to register bytes before the state that
uses them. Stopping or restarting the provider releases all of its blobs.
Rendered images stay in the raster cache after release, so a surface keeps
showing pixels it already painted until its state changes. Event payloads and
command results do not pin blobs.

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

`"workspace"` binds `ext_workspace_manager_v1` version 1. Records: `group-added`,
`group-workspace-enter`/`group-workspace-leave` (with `workspace`),
`group-removed`, `added`, `workspace-id`, `name`, `coordinates` (array),
`state` (bitfield: 1 active, 2 urgent, 4 hidden), `removed`, and a
manager-wide `done` that delimits each batch. At most 1,024 handles are
tracked per connection.
