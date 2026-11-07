# Section 11 — Luau runtime and sandbox

## Scope and coverage

Read-only scan. No repository files, source, tests, manifests or configuration changed; no builds/tests/process probes run. Scratch coverage: /tmp/mesh-section11-coverage.json. Cumulative prior+continuation coverage is 93 assigned files, 28 full-file reads, 13 partial reads, 52 unreviewed. These are file-reading counts, not independent exhaustive three-layer completion. A report must remain interim.

## Process tree

 graph-authorized manifests/capabilities -> runtime policy and resource budget -> private script environments on allowed shared realm -> start(self)/registered commands/tasks/streams -> capability-gated host APIs -> typed state/result/event conversion -> publication to shell -> cancellation, resource release and storage flush. Provider Luau owns domain interpretation; core owns authorization, budgets, host transport and cleanup. Shared VM by itself is permitted by spec 00 §7; do not label it a violation. Spec 01 near 1318 describes bounded backend output windows; finer frontend structural reconciliation is separately Target.

## Logic and core mechanics

### S11-LOGIC-21 — Deleting backend storage fails to release aggregate resource charge.
Confirmed source. scripting/src/storage.rs:908–913 handles Lua nil by remove(key), write_sink and return; non-nil path :925–936 snapshots and calls StorageCharge. backend/runtime.rs:2383–2405 records charged storage bytes in an AtomicU64 and calls ResourceBudget::recharge_storage(previous,next); policy.rs:242–247 only releases aggregate when invoked with a smaller next size. Consequently storing a large value then deleting it leaves its aggregate charge until a later non-nil write recalculates storage. Subsequent queued/output work can be falsely rejected. This is not demonstrated as the four-second delay. Regression: small aggregate budget; write near-limit key; delete via real Lua table; assert storage is empty and aggregate shrinks before any later write; then queue/output allocation succeeds. Also verify nil deletion of absent key and rollback on failed non-nil write. New by current backlog source search. Initial preloaded storage is also uncharged until first mutation because charged bytes starts at zero and helper doesn't invoke charge at setup; treat this separately as an accounting-policy gap, not an asserted security bypass without aggregate/storage-policy review.

### S11-LOGIC-22 — Wallpaper preview cache ignores source modification and keeps failed entries.
Confirmed source. modules/backend/wallpaper-directory/src/main.luau:17–18 maps previews/panoramas by path; :27–34 projects those values; :81 and :95 skip work whenever a path has an entry. Shell thumbnail scripts at :10–11 include mtime in disk keys, but those scripts aren't rerun after first memory-cache insertion. Replacing an image in place leaves old thumbnail/color/panorama after refresh; failed rasterization also stores path fallback so subsequent refresh never retries; removed paths aren't pruned, growing maps with library churn. Regression: scan A, replace bytes at same path, refresh, assert newly generated preview/color; failed conversion then repair; remove/readd; bounded map size. Need timestamp-resolution edge cases because disk key uses seconds (%Y). New by backlog search. Preserve last-good preview while asynchronously producing new content; no implementation in audit.

### S11-LOGIC-23 — NetworkManager deduplication can lose connected network identity.
Confirmed source. modules/backend/networkmanager-network/src/main.luau:56–83 iterates APs, updates top-level active ssid/signal at :64–66, but deduplicates by SSID retaining only first row at :68–75. If the active AP appears after another AP sharing SSID, its row is skipped and the retained row stays connected=false with first AP signal/security. Sorting does not repair it. Select/merge one deterministic representative per SSID, preferring connected AP, then strongest according to declared UI semantics; don't merge differing security silently without a policy decision. Regression: two APs same SSID active second and first, order reversal, varying strengths/security; assert one row and correct connected flag/top-level consistency. No live or harness execution. New by source/backlog search.

## Performance findings

### S11-PERF-21 — Device information polling repeats static subprocess probes.
Confirmed repeated work, unmeasured impact. modules/backend/device-info/src/main.luau:142–176 launches uname three times, os-release/hostname/desktop/cpuinfo/meminfo/lspci/uptime probes; on_poll :187–188 calls complete read_state every 30 seconds. Comment :181–182 claims it avoids repeated expensive probes but implementation does not. Cache stable identity/hardware separately from uptime and use appropriate slow/revisioned refresh for facts that may change. Generic host reads/batched exec should remain capability-gated; domain policy stays provider-owned. Benchmark release idle provider for 10 minutes: subprocess count, provider wall/CPU time, output allocation and state notifications; compare uptime-only versus full recovery refresh and test hostname/hotplug recovery. Not a direct Settings transaction path, no four-second attribution. New by backlog search.

### S11-PERF-22 — Every D-Bus method call recreates worker/runtime/connection.
Confirmed setup work, unmeasured payoff. scripting/src/backend/dbus_call.rs:258–279 starts scoped OS thread and new current-thread Tokio runtime per call, joins it; perform :299–302 connects/authenticates a new private bus connection. backend/runtime.rs:1915 and :1954 use this same call helper for blocking/yielding wrappers. NetworkManager read_state calls properties for manager/devices/wireless/every AP; UPower and tray also use this path. A generation-owned executor/connection cache could amortize setup while retaining bus-target separation, capability checks, output limits, cancellation/reconnection and generation ownership. Don't just share connections indiscriminately: private connection is documented contract and failure isolation must be preserved. Measure 1/10/100 property calls and 1/10/100 AP refreshes, connection/thread counts, allocation, wall latency, cancellation and provider shutdown. Existing general runtime host-boundary performance backlog overlaps, exact setup chain new. No speedup claimed.

### S11-LOGIC-24 — UPower battery property signals don't refresh battery state.
Confirmed filter behavior; responsiveness impact high confidence, not an unimplemented universal event target. modules/backend/upower-power/src/main.luau:153–158 subscribes all UPower path signals; on_stream_batch :168–169 refreshes only DeviceAdded/DeviceRemoved or paths containing line_power. Battery PropertiesChanged for percentage/state/energy therefore waits for 10-second poll (:150, on_poll) despite subscription. Handle relevant battery properties and invalidated fields, coalesce signal bursts; preserve rediscovery and fallback poll. Regression emit battery PropertiesChanged and assert state before poll, unrelated signals no read, battery removal/line_power changes still work. Could overlap historical event-provider work; not found as specific current backlog item.

## Dead code and redundancy

 scripting/lib.rs crate docs still describe reactive state through mesh.state.set and bindings through mesh.service.bind/on; current contexts use tracked members/effect and typed proxies. Validate docs against shipped scripting contract before editing. Exec's canonical executable resolution performs filesystem/PATH resolution for every policy lookup, but changing this requires a security/replacement-identity decision; no dead-code or caching recommendation without that analysis. D-Bus argument/body encoding centralization is already present and should not be duplicated by providers.

## Refuted suspicions

 mesh.exec workers use bounded output and process groups; known inherited-pipe descendant cleanup backlog remains open, not a newly discovered issue. Exec streams do reap_finished on drain; don't report their process table as the IPC JoinSet leak. task.delay/task.sleep validate finite delay and cap pending tasks; no unbounded pending-task assertion. Current vm JSON hashes tag numeric subtypes, so historical subtype collision is fixed. No tests were run; all findings are source confirmations or explicitly unmeasured hypotheses.

## File coverage

Exact cumulative coverage is in /tmp/mesh-section11-coverage.json. New full reads in this continuation: context/runtime/vm.rs; context/runtime/tasks.rs; backend/host_events.rs (embedded tests included); backend/errors.rs; backend/logging.rs; backend.rs; context.rs; modules/backend/upower-power/src/main.luau; modules/backend/networkmanager-network/src/main.luau. Additional partial reads: exec.rs through output/pipe lifecycle (~1–711, not embedded tests); exec_stream.rs:1–235; dbus_call encoding/call/perform/test excerpts; launch_app.rs mixed truncated read; host_api.rs policy/test excerpts; lib.rs short documentation/export file. storage.rs:817–960 and backend/runtime.rs:1888–1961,2370–2415 independently rechecked. Prior full/partial evidence preserved separately in coverage JSON. Most backend native-source implementations, proxy/state/template internals and tests remain unreviewed or partial. Do not label section complete.

## Existing backlog or audit overlap

D-Bus setup and JSON transport overlap general host-boundary performance work. Inherited command-pipe cleanup already has an explicit backlog item. The storage-delete, wallpaper-freshness and duplicate-SSID findings are more specific than existing broad audit entries. UPower signal handling relates to provider event responsiveness; do not report a missing target platform capability.

## Tests and benchmarks needed

Use real runtime storage writes with a bounded aggregate budget, deterministic provider host fixtures for cache freshness and duplicated APs, and replayed UPower signals before fallback polling. Measure the subprocess and D-Bus workloads above in release with repeated samples before making performance claims. This scan ran no builds or tests.

## November 7 host/source and proxy continuation

## November 7 remaining provider and subprocess continuation

### S11-LOGIC-28 — Tray text limits can invalidate an entire Unicode state snapshot

`status-notifier-tray/src/main.luau:83–85` limits text with byte-based `string.sub(1, 128)` before stripping controls. A valid title containing 127 ASCII bytes followed by `é` leaves a lone UTF-8 leading byte. Title, tooltip and several icon/path fields use the helper (`:129–134`). `backend/runtime.rs:1280–1312` converts the complete service state to JSON; mlua's serde conversion treats an invalid UTF-8 Lua string as bytes, which the JSON value visitor rejects. A single truncated label can therefore fail the whole snapshot rather than only shorten that label.

**Confidence: confirmed source data-boundary defect; no provider reproduction.** Truncate on a valid UTF-8 boundary under the chosen byte cap. Verify a real tray runtime fixture with 2/3/4-byte characters crossing the limit, multiple registered items and repeated updates. The frontend's existing UTF-8-safe title regression exercises another implementation and does not cover this helper. No live failure escalation was established.

### S11-LOGIC-29 — A subprocess stream runs before process admission and Started publication

`backend/exec_stream.rs:550–581` spawns `run_stream` before inserting the process/cancellation handle and queuing `Started`. On the shell's multithread Tokio runtime, the worker can enqueue output, EOF or terminal events before the caller completes admission. `push_event` updates shared status for each event, so a late `Started` can overwrite terminal status with Running. Early output-limit failures can also request cancellation before the process is discoverable.

**Confidence: confirmed race window and production runtime caller; no forced-interleaving reproduction.** Admit ownership and publish Started before releasing stream work, including cancellation/shutdown and failed-start paths. Use a deterministic barrier or multithread fixture with immediate output/exit and oversize output; assert event order, terminal status and child-budget cleanup. Existing default current-thread Tokio tests do not exercise this admission race.

### Provider continuation coverage

### S11-PERF-23 — Completed service-call results remain for the context lifetime

`context/runtime/lifecycle.rs:344–371` inserts terminal results into `service_call_completions`. Ticket polling in `context/proxy.rs:758–855` only reads/clones entries; no ticket lease/finalizer reclaims them. Repository references clear the map on reload/uninit, not normal completion or ticket collection. The shell removes its pending route and forwards completion into this independent context map. Shipped Settings audio handlers discard returned control tickets, so retention does not require intentionally retaining every ticket.

**Confidence: confirmed retention and shipped caller; memory/latency unmeasured.** Use ticket/awaiter-owned result lifetime and bound outstanding count/bytes while preserving repeated poll, await, cancellation and late-result semantics. Do not consume results on first poll without defining that API change. Measure 1k/10k/100k completed calls on one mounted context, retained versus discarded tickets, after Lua GC; record Rust result count/bytes and RSS across repeated release runs. Existing tests cover individual completions and await/cancel, not long-lived completion reclamation. No settings-delay attribution is established.

### Runtime test continuation

Fourteen more files were fully read, including twelve test/benchmark files and production lifecycle/callbacks. Backend tests were read through line 600 only. Section 11 now has 83 full and 10 partial reads; zero wholly untouched files does not mean all code has been read. In particular backend tests lines 601–4676 and other partial bodies remain. Tests confirm private shared-VM environments, callback ownership/weak reclamation and task limits; blanket shared-VM leak or absent task-bound claims were refuted. These are source inspections, not executed test results.

Eleven more assigned files were fully read, including remaining shipped provider sources, fixture scripts, launch_app and exec_stream with embedded tests. Two files upgraded earlier partial reads, giving 69 full, 11 partial and 13 unread at this checkpoint. Tray polling inherits the already-recorded D-Bus setup cost; PipeWire monitor self-noise accounting is deliberate and was not classified as redundant. No implementation, tests, builds or measurements were performed.

### S11-LOGIC-25 — Unix socket connection establishment is outside the timeout

`backend/host_sources.rs::spawn_socket_stream` (`:94–98`) synchronously connects before enabling nonblocking mode and registering its async source. `socket_request_inner` (`:250–275`) creates a deadline but performs the same blocking connect before setting write/read timeouts. The direct stream Lua binding executes that connect inside its callback. Moving request work to TaskIo changes its thread, but does not put a deadline around the connect itself.

**Confidence: confirmed missing connect deadline; a saturated accept-backlog reproduction has not been run.** A blocked connection can occupy the callback or host worker beyond the configured request timeout. Use nonblocking establishment under an absolute deadline and cancellation while preserving useful missing-path diagnostics. Test a controlled listener whose backlog is full, both direct stream and request paths, shutdown cancellation, worker accounting and normal successful connection. Do not attribute the user's four-second delay to this scenario without a trace.

### S11-LOGIC-26 — Nested service state skips the optional-feature visibility gate

The outer proxy in `context/proxy.rs` checks FeatureGate before returning an unsupported member, but returns `.state` before that check. `create_service_state_proxy` (`:656–687`) receives no feature information and calls `service_payload_field` directly. Thus `thermal.fans` can be nil while `thermal.state.fans` returns a payload value for an unsupported feature. The absent-state builder also carries declared defaults without feature filtering.

**Confidence: confirmed API inconsistency against spec 01's shipped optional-feature contract; no runtime regression was run.** Shell validation rejects unsupported provider fields, limiting realistic payload routes; this is not an established capability bypass. Verify nested/direct parity using the existing unsupported-feature fixture, a missing provider and valid optional defaults, then use identical feature gating in both access paths. Existing tests cover the direct member only.

### S11-LOGIC-27 — Equal service payloads fail to advance the source-generation guard

`context/state.rs::ServiceContextState::update` rejects stale source generations, then returns early for equal fingerprint and JSON value without advancing source module/generation. The sequence generation 7/A/X → generation 8/B/X → delayed generation 7/A/Y can leave the context's guard at 7 and admit Y. The frontend cache in `component.rs` advances its own generation for equal data, while `shell_component/mod.rs` independently forwards the supplied snapshot to script contexts, so the cache and context can disagree.

**Confidence: confirmed local state-machine defect and real forwarding caller; arrival of this stale sequence in the live shell is not established.** Source identity must advance even if visible value and payload-generation stamp need not change. Regress the equal-content newer-generation hop, provider change, stale payload after it, zero/synthetic generations and unchanged-data notification suppression. Preserve shell-level identity validation; do not infer a privilege escalation or a reproduced stale desktop from this helper sequence alone.

### Continuation coverage and refutations

Later November 7 parent reads added freedesktop-notifications, mpris-media and pulseaudio-audio in full: 58 full, 13 partial and 22 unread. A suspected thirty-second initial PulseAudio state delay was refuted: backend `run_script_service` creates its interval with `make_interval(interval_ms, true)` after start completes, yielding an immediate first poll. Subsequent interval changes deliberately schedule a delayed tick. The keyboard hidden-subscriber question in section 10 was not promoted without resolving surface-action semantics; the interrupted agent had not saved complete coverage evidence for that pass.

November 7 checkpoint: 55 full reads, 13 partial and 25 unread of 93 assignments. The source agent completed the host APIs, template and element references; the parent additionally read backlight-brightness, hyprland-keybinds and hyprland-apps in full. Remaining agent work stopped at a usage limit. The counts describe source inspection, not completion of independent audit layers. The Hyprland applications index retains path/icon lookup results across polls; freshness and startup subprocess costs need separate contract and workload review before being promoted as additional findings.

The first source continuation brought section 11 to 43 full reads, 14 partial reads and 36 unreviewed entries of 93; further bounded reads are recorded in the inventory when verified. PipeWire/Wayland source lifetimes have explicit cancellation and limits. A proposed nested int/float prop-layer mismatch was not promoted: shipped prop domains are scalar and their canonical validation normalizes values, so the structured-prop scenario was not shown reachable. No source edits, tests or benchmarks were performed.
