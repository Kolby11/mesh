# Scripting scope, native providers and bulk data channels

**Status:** proposed direction from a 2026-10-28 design discussion. Not
accepted into the spec yet. Backlog lines are under "Native providers and bulk
data channels" in [`docs/BACKLOG.md`](../../../docs/BACKLOG.md). Each step
needs a measured prototype before it is adopted. This note builds on
[the runtime architecture direction](2026-10-10-runtime-architecture-direction.md)
(D4: typed patches, no JSON on the frame path).

## 1. Scope: a curated platform for shell experiences

MESH stays a platform for building **shell experiences** (bars, launchers,
settings, integrations), not a general app framework. The comparison points:

| | API breadth | Trust model |
| --- | --- | --- |
| Qt / QML / Quickshell | Very large (C++ framework); QML JS is glue with only Qt globals | Trusted: any QML file can load any plugin; Quickshell `Process` runs arbitrary commands |
| Electron | Very large (Node + browser) | Mostly trusted |
| MESH | Curated host APIs | Sandboxed, capability-gated |

What sets MESH apart is that modules written by other people can be installed
safely, not that it has fewer APIs. A broad API is fine as long as each piece
sits behind a capability. Node-style `fs`/`child_process` access would reduce
the capability model to bookkeeping and commit MESH to tracking Node
compatibility forever.

**Proposed script API baseline:** the WinterTC "Minimum Common API" subset
(`fetch`, `URL`, `TextEncoder`/`TextDecoder`, timers, `AbortController`,
`crypto.getRandomValues`, `structuredClone`, streams), each mapped onto a
capability (`fetch` requires `network`). No Node or DOM compatibility. This
applies whatever the script language is.

## 2. Script language evaluation

The motivation is Luau's small library ecosystem. At the time of the
discussion, `mlua` is used across about 31 files in
`crates/core/runtime/scripting`, shipped modules contain about 3.8k lines of
`.luau` and 66 `<script>` blocks, and template expressions are Luau. Several
measured wins (Rust-side dependency probes, per-item state-sync diffing, the
storage-read rejected experiments) are Luau-specific and would need
re-measuring.

| Option | Libraries | Speed vs Luau | Memory / idle | Sandbox | Verdict |
| --- | --- | --- | --- | --- | --- |
| Luau (current) | Small | Baseline; among the fastest interpreters | Very low | Built for it | Keep |
| QuickJS-ng (`rquickjs`) + TS types stripped at load (oxc/swc) | Pure-JS npm | Expected somewhat slower | Low | Good: no standard I/O unless provided | Best alternative |
| V8 (`deno_core`) | Best | Much faster on hot code (JIT) | MBs per isolate, ~30MB+ binary, GC pauses | Good | Conflicts with the low-idle requirement |
| Boa | Same as QuickJS in principle | Slower than QuickJS | Moderate | Good | Not mature enough |
| Python (PyO3 / RustPython) | Large, much of it needs C extensions | Slower | Heavy, GIL | Very hard to sandbox | Poor fit |
| Rhai / Rune | Almost none | Slower | Low | Good | Worse on the actual complaint |
| WASM (wasmtime) | Any compiled language | Fast compute, expensive UI-callback boundary | Moderate | Strongest | Awkward for reactive UI scripts |

These speed and memory figures are general expectations, not MESH
measurements. Script execution is a minority of frame cost (one profiled poll
workload: Luau execution plus GC about 10–15%; see `performance-log.md`); the
Rust-side rendering pipeline would not change.

**QuickJS + TypeScript in practice:**

- TypeScript is stripped to JS at load time (sub-millisecond per file, cached).
  Type checking runs in the editor or CI (`tsc`/tsgo). MESH ships a `.d.ts`
  for props, storage, service proxies and host APIs. `.mesh` `<script>`
  blocks would need the LSP to forward to the TypeScript language server.
- Works: pure-logic ESM/CJS packages (date-fns, zod, fuse.js, lodash-es,
  marked). Does not work: Node APIs (unless shimmed onto host APIs), the DOM,
  native `.node` addons. `fetch`/timers/`URL` only work if MESH provides them.
- Packages are bundled into the module at install time (esbuild/rolldown) as
  one self-contained file; the shell never resolves `node_modules`.

**Recommended order:** first close the library gap inside Luau with a
capability-gated host standard library (JSON, date/time formatting, regex,
fuzzy matching, HTTP) and `require` of vendored pure-Luau packages through
declared dependencies. Prototype QuickJS + TS only if that isn't enough, behind
a language-neutral script boundary, alongside Luau rather than replacing it,
and benchmark it on the existing gates.

## 3. Native Rust providers (out-of-process)

Backends implement typed JSON interface contracts; consumers depend on the
contract, not the provider's language. A native provider is just another way
to implement the same contract. Provider selection, validation and supervised
restart, quarantine and failover stay unchanged.

| | In-process `.so` (Qt-style) | **Separate process** | WASM |
| --- | --- | --- | --- |
| Crash isolation | A bug kills the shell | Supervisor restarts it | Yes |
| ABI | Rust has none; C ABI or `abi_stable`, rebuilt for each MESH version | Wire protocol | WIT |
| Hot reload / unload | Unloading a `.so` is unsound in practice | Kill and respawn | Yes |
| System crates (zbus, pipewire-rs, tokio) | Yes | Yes | Only through host APIs |
| Sandboxing | None possible | OS-level (Landlock, seccomp, bubblewrap) | Strong |
| Per-call overhead | ~0 | Tens of µs over IPC | Small |
| Other languages | C ABI only | Any | WASM targets |

Qt can use in-process plugins because its code is trusted and the Linux C++
ABI is fairly stable; neither holds for MESH. Backends publish at a human rate
(even a visualizer at ~60 Hz), so IPC overhead does not matter. Chosen: a
**separate process**.

Design:

1. **Manifest:** `mesh.runtime: "native"` plus a binary or build step;
   `provides`/`implements`/contracts unchanged.
2. **Protocol** over a Unix socket the shell creates: state snapshots/patches,
   method calls/replies, events, a lifecycle handshake (protocol version,
   contract version, `start`, shutdown) and a heartbeat. Core validates every
   record against the contract, as it does for Luau providers.
3. **SDK crate `mesh-provider`:** a build script generates traits and types
   from the `module.json` contract; `mesh_provider::run(impl)` owns socket,
   framing and reconnects; `ctx.publish(state)` sends patches.
4. **Trust:** native code has its process's OS access, so capabilities cannot
   be enforced inside it. The module declares a `native` capability; the
   package UI marks it **unsandboxed**. Optional Landlock/bubblewrap launch with
   declared paths and buses as defence in depth, not a guarantee.
5. **Supervision:** process exit is handled like a Luau provider crash.

Frontends stay scripted: they are per-frame glue, and a process boundary would
hurt there. Native rendering for frontends is a core element (live texture or
canvas fed by a provider), admitted by the
[element test](../../../docs/spec/00-philosophy.md#4-elements-enforce-shared-standards),
not native code loaded into the UI tree.

A side effect: a backend that needs a large ecosystem can be written in Rust,
Go or Python against the same protocol.

## 4. Bulk data channels

There is one channel type today (structured state, methods, events), and
shipped modules push bulk data through it:

- `status-notifier-tray` puts raw ARGB pixmaps in service state as a string
  (`icon_argb`), passing through Luau and state records.
- `cava-visualizer` parses ASCII from cava at 30 fps in Luau and republishes
  the whole `bars` array as state every frame.

Use cases by data shape:

| Shape | Examples |
| --- | --- |
| Immutable blobs | tray pixmaps, notification `image-data`, album art, wallpaper thumbnails, window icons, clipboard images |
| Sample streams | audio spectrum, VU meters, CPU/network/temperature graphs, battery history |
| Frame streams | window previews (overview/alt-tab), camera preview, recording preview, video wallpaper, animated covers, native canvases |
| Large collections | launcher index, file search, clipboard/notification history, emoji picker, process list |
| High-rate frontend → backend | volume/brightness drags, seek scrubbing, color pickers |
| Frontend → backend bulk | drag-and-drop, pasted images |

Each channel is declared in the contract like state is.

1. **Blobs (content-addressed handles).** Providers register bytes once; state
   carries `mesh-blob:sha256-…`. Native providers pass a sealed `memfd` over
   the socket (`SCM_RIGHTS`); Luau calls `mesh.blob.put(bytes, mime, meta)`.
   Frontends use handles wherever images are accepted; the resource worker
   decodes asynchronously and repaints only its consumers. Reference-counted
   by published state; dropped on provider restart.
2. **Sample streams.** `"spectrum": { "stream": "f32[24]", "rate": 60 }`. A
   shared-memory slot where the latest value wins; elements bind directly
   (`<graph data="{visualizer.spectrum}">`) and mark only their own paint as
   changed, with no per-frame Luau or state diffing. Scripts can read at a
   throttled rate. Ring-buffer variant for history graphs.
3. **Frame streams.** dmabuf with modifier negotiation and `sync_file` fences,
   imported as Skia textures; shared-memory fallback for CPU rendering. A
   producer-owned pool of 3–4 buffers with release leases (as in Wayland
   presentation). The latest frame wins: frames that arrive faster than the
   surface's frame callback are dropped. The consumer reports the element's
   physical size, scale and accepted formats, so producers capture or render
   at display size. A core live-texture element whose display-list node only
   changes its paint generation, with damage limited to the element. Sources:
   native providers, and core-wired system sources Luau can request without
   touching pixels (`mesh.capture.toplevel(id)`, `mesh.pipewire.video(node)`).
   Capture requires a capability plus core-owned consent and a privacy
   indicator.
4. **Collections.** `"apps": { "collection": "AppEntry", "key": "id" }`.
   Consumers request `{offset, limit, filter, sort}`; providers answer with a
   page and later keyed diffs (insert/update/remove/move). Feeds the planned
   virtualized list (see the Performance section of the backlog). Items carry
   blob handles for their icons.
5. **Coalesced methods.** `"set_volume": { "coalesce": "latest", "key": "node" }`.
   Newer queued calls with the same key replace older ones. Shares coalescing
   keys with the planned optimistic proxy values (Authoring model backlog).
6. **Data transfer.** Drag-and-drop/paste payloads arrive as blob handles plus
   a MIME list; the capability is checked when the payload is read.

Rules that apply to every channel:

- Bulk bytes never go through JSON or scripts on the frame path; scripts
  handle handles and bindings.
- Declared in the contract: core validates shapes and formats, gates access by
  capability, and exposes rates, drops and sizes to devtools.
- Demand-driven: core tells producers when nothing is visible (hidden
  surface, scrolled out, closed popup) so they pause; idle cost stays near
  zero.
- Latest value wins with bounded memory: no unbounded queues; overload drops
  frames or samples rather than adding latency.
- Generation-owned lifetime: everything is torn down on provider restart or
  profile switch.
- Same API for Luau and native providers (one copy for Luau, zero copy over
  fds for native); consumers cannot tell which kind of provider they use.

## 5. Concerns to settle in the design

**Expensive to retrofit; decide first:**

1. **One schema, many code generators:** the contract is the single source for
   the Rust SDK, Luau types, any TS `.d.ts`, the LSP and devtools metadata.
   *Settled 2026-10-28:* every generator reads `CompiledContract`; Rust
   provider bindings and a versioned JSON descriptor for out-of-tree
   generators ship (`mesh-shell types --target`). The new channel kinds must
   extend the contract and the descriptor, not a generator.
2. **Protocol versioning:** the handshake checks protocol and contract
   versions; on mismatch, refuse with a migration diagnostic (no compat shims).
3. **Ordering across channels:** state can reference a blob or collection item
   that hasn't arrived. Send blobs before the state that references them, or
   apply sequence-numbered batches atomically.
   *Settled 2026-10-28 for blobs:* registration precedes reference. A state
   snapshot that references a handle its provider has not registered is
   rejected at the provider boundary. Native providers must keep the same
   rule: a blob's fd arrives before the patch that names it.
4. **Compositor portability for capture:** `ext-image-copy-capture-v1`
   (standard), `hyprland-toplevel-export`, `wlr-screencopy`. Core owns one
   capture-source abstraction with a backend per compositor protocol.
5. **Shared-memory safety:** a producer that shrinks a shared file mid-read
   crashes the shell with `SIGBUS`. Require `memfd` seals (`F_SEAL_SHRINK`;
   `F_SEAL_WRITE` for immutable blobs) and reject unsealed fds. Validate
   dmabuf size, stride and format before import.

**Security and resources:**

6. Quotas on per-provider blob memory, stream rate and collection size, with
   diagnostics; native processes in a systemd scope/cgroup with CPU and memory
   limits (the spec's execution-budget obligation for native code).
7. Distribution of native modules: build at install (needs a toolchain, slow,
   not reproducible), prebuilt per-arch binaries (needs signing), or a Nix
   build. Each changes the trust story in the package UI.
8. The automation IPC and MCP must never read capture frames; capture data is
   visible only to the element that displays it.

**Lifecycle:**

9. Start providers lazily, on first consumption of their interface, or via
   systemd socket activation; spawning many processes at login adds latency.
10. Suspend/resume and GPU resets invalidate dmabufs and EGL contexts: frame
    streams renegotiate instead of failing; PipeWire and D-Bus reconnect.
11. Multi-GPU: cross-device dmabufs may not import, so fall back to shared
    memory. Mixed refresh rates: pace each stream to the frame callbacks of
    the output it's shown on.
12. Profile switches preserve identical native provider instances and their
    open channels.

**Platform requirements:**

13. Accessibility: live textures and canvases need accessible names; graphs
    expose a semantic value summary; reduced motion pauses visualizers and
    animated covers.
14. Localization: native providers return catalog keys, not hard-coded text.
15. The protocol carries `self.storage` and props equivalents through core's
    scoped persistence; native providers do not write their own config files.
16. Power: cap frame and sample rates on battery in addition to demand-driven
    pausing.

**Tooling:**

17. Capture each provider's stderr into attributed diagnostics, turn crashes
    into readable reports, and add a `mesh dev` mode that runs a provider
    under a debugger or with hot respawn.
18. Mock providers and record/replay of channel traffic for frontend tests; an
    SDK test harness for native providers.
19. Prefer a few general elements (data-bound graph/canvas, virtualized list,
    live texture) over one bespoke element per use case.

**Migration:** move tray pixmaps and cava to the new channels and delete the
old paths outright.

## 6. Spec impact when accepted

- 00 §7 and `AGENTS.md`: "module execution stays within the shell process"
  gains an exception for native provider processes; the native tier moves
  from "not committed" to Target; record the shell-experience scope and the
  WinterTC-style API baseline.
- 01: `mesh.runtime`, the native provider protocol, the `native` trust marker,
  and channel declarations in contracts.
- 02: package UI trust presentation and native module distribution.
- 09/11: accessibility of live content; IPC exclusion of capture data.

## 7. Suggested order

1. Blobs: shipped 2026-10-28 for Luau providers (memfd transport waits for
   step 5).
2. Sample streams: port `cava-visualizer`, measure before and after.
3. Coalesced methods: already shipped as keyed `coalesce` (spec 01); the
   optimistic proxy values remain under Authoring model.
4. Collections, with the virtualized list (launcher).
5. Native provider protocol and `mesh-provider` SDK; prove it by porting
   `backlight-brightness`. Channels gain fd transport.
6. Frame streams and the live-texture element, starting with window previews.

Host standard library for Luau and the optional QuickJS + TS prototype
(§2) are independent of this order.
