# Section 14 — Wayland and presentation

## Scope and coverage

Read-only scan. No implementation changes, builds, tests, live settings mutations or benchmarks were performed. Existing uncommitted changes were inspected as pending work. This section report records source evidence and explicit coverage limits; the whole-codebase audit is unfinished.


## Process tree

shell SurfaceConfig -> canonical semantic diff -> configure/recreate existing role -> compositor configure/buffer lifecycle -> SHM/GPU frame preparation -> damage/input regions -> attach/commit -> callbacks/releases. Logical extent, wire-span zero and physical buffer size are deliberately distinct.
Fully read this continuation: backend/protocol.rs, damage.rs and shm.rs. Other inspected files remain partial in coverage JSON. All three layers assessed on these seams, not every presentation source/test.

Confirmed finding: S13-LOGIC-21 center omission crosses into needs_reconfigure/configure, as above. The fix belongs in canonical policy and its projections, not an ad hoc extra presentation comparison.
Refuted/limited suspicions: current damage-copy functions validate checked arithmetic, source/canvas extents and crop physical damage. SHM allocations have dimension/per-buffer/per-surface/pool bounds and viewporter-aware size classes. No new confirmed direct-copy overflow was found in the fully read code. Protocol helpers already guard invalid zero-span combinations and retain distinction between resolved extent and wire extent; do not propose reverting to raw zero dimensions.
Existing in-progress fix: backend/events.rs now treats flush WouldBlock as backpressure and watches writable readiness. It is a local uncommitted change; do not file the old WouldBlock error as untouched current code. The previously discussed GPU two-second completion deadline is a fallback/error timer, not evidence of an ordinary settings debounce.
Performance: configuration/input-region work still deserves workload-driven separation from paint, but this overlaps S14-PERF-002/003 backlog. No fresh measured optimization or confirmed dead API was established. Test centering with live protocol requests and preserve configure/readiness, callbacks, damage and release ordering.

## Review layers and existing overlap

Performance, redundancy and correctness were assessed on the inspected seams above. Unmeasured work is distinguished from measured cost. Source-only hypotheses do not establish the reported four-second Settings delay. Existing broader backlog items remain open; current local implementation edits must not be counted as independently verified completed findings.

## Refuted suspicions

No fixed four-second settings debounce was identified. Unknown filesystem events retain conservative reconciliation. Caller-specific caveats and refuted or withheld suspicions are stated above; unavailable target behavior is not classified as a shipped defect.

## Tests and benchmarks needed

Run each finding's proposed deterministic regression and representative repeated release workload before changing code or claiming performance gains. None was run during this read-only scan.

## November 7 production-source continuation

The continuation fully read the remaining presentation production Rust sources and their embedded tests; standalone backend tests and the live matrix are being accounted for separately. Source confirmation is not a live reproduction.

### S14-LOGIC-22 — Physical output mode pixels become logical fallback dimensions

`backend/surfaces.rs::logical_size_of` (`:825–844`) prefers the compositor's optional logical size, but otherwise returns current mode dimensions directly. SCTK describes those dimensions as mode pixels and separately exposes scale and output transform. The fallback ignores both. A physical 3840×2160 mode with integer scale 2 can therefore be treated as 3840×2160 logical rather than 1920×1080; rotations also require axis handling. Callers use the result for initial container size and resolved/clamped output extents.

**Confidence: high, source-confirmed fallback error; no compositor reproduction.** Preserve authoritative logical size when present and normalize only the physical fallback with one tested helper. Cover scale 1/2, normal/90/270/flipped transforms, absent/invalid modes and explicit logical-size precedence. This is related to existing initial-output/query work but occurs after an output is identified; do not conflate it with guessing which monitor owns a new surface.

### S14-LOGIC-23 — Seat cancellation omits atomic IME edits and focus enters

`state.rs::is_keyboard_event` (`:1371–1381`) recognizes Key, Char, TextInput and TextDelete, but not TextInputEdit or KeyboardEnter. `cancel_keyboard_input_state_for_seat` (`:1162–1173`) uses that predicate to retain queued events. `handlers.rs::remove_seat` cancels input then destroys the seat; handlers enqueue KeyboardEnter and atomic TextInputEdit with seat ownership before this boundary. `events.rs` later drains events without the seat metadata, allowing those two queued variants to outlive their owner.

**Confidence: confirmed production source path; no live/test execution.** Include all keyboard-owned variants in canonical modality classification. Queue all six variants for a removed seat, preserve another seat's events, clear repeat/modifiers, and exercise IME done plus seat removal in one dispatch batch. Existing cancellation tests cover Key/Char and do not prove the omitted cases. Surface teardown already filters all surface-owned events; the finding is specifically seat cancellation.

### Additional performance and redundancy review

Some input handlers resolve surfaces by scanning their values, and dispatch refreshes surface backpressure across the surface set. These are source-backed repeated traversals, not measured bottlenecks. Measure 1/8/32/128 surfaces under high-rate pointer, output and buffer-release events before replacing them with indexes or dirty sets. Existing broad presentation overhead backlog work overlaps. No new dead public API was established: the testing backend and SCTK delegation have real integration roles.

Exact callbacks/frame generations, bounded SHM allocation and damage retention have substantive code/tests. No additional overflow, callback-generation or four-second debounce defect was confirmed by reading these paths. Do not treat the two-second GPU failure deadline as normal latency.

### S14-LOGIC-24 — Live matrix does not enforce its claimed backend or resize scenario

`tests/live_compositor_matrix.rs::live_engine` sets `MESH_BACKEND=layer-shell` and says connection failures must fail, then calls `PresentationEngine::select`, which can fall back to DevWindow on Wayland initialization error. Individual GPU tests may then return a successful skip unless GPU is explicitly required. This does not mean the entire matrix passes against DevWindow: other tests expect behavior that can fail there.

The in-flight resize scenario allocates its replacement canvas at `canvas.width(), canvas.height()` (`:406–409`) and does not change the surface extent. It tests canvas replacement, not the claimed resize. **Confidence: confirmed validation gaps; no test execution.** Use a fallible explicit backend selection, assert backend identity, and change logical/physical dimensions while a prior scene is outstanding. Check unavailable compositor/layer-shell preconditions and that stale-size completions cannot commit after an actual resize.

### Completed section 14 file-reading coverage

The continuation reconciled the inventory against current platform/Wayland and presentation paths: **28 assigned files, 28 fully read, zero partial/unreviewed/unassigned**. This includes the two manifests, all production sources, embedded/separate tests and the live compositor matrix. Twenty-five full reads came from this continuation and three from the prior protocol/damage/SHM pass. All three review layers were assessed by the section reviewer; three independent full rereviews and exhaustive external callers are not claimed. No builds, tests or compositor runs were performed.

The platform clipboard helper's earlier write-failure cleanup defect is fixed: it closes input and reaps the child. Its normal `write_all`/`wait` still run synchronously without a timeout in shell request processing (`runtime/request.rs:1769–1774`). A stalled clipboard helper is an unmeasured liveness hypothesis, requiring a controlled non-reading/non-exiting helper test. It is not evidence for ordinary settings latency.

## File coverage

Across this section and sections 13–16, 166 assigned files were accounted for: 10 full reads, 22 partial reads and 134 unreviewed. The [inventory](../00-coverage.md) records each file's actual status. Full reads beyond section 13: `crates/core/platform/wayland/Cargo.toml`, `crates/tools/lsp/src/lib.rs`, `crates/core/shell/src/shell/ipc.rs`, and presentation `backend/damage.rs`, `backend/shm.rs`, `backend/protocol.rs`. All other inspected files are partial; all remaining assignments stay pending. Counts overlap prior passes and do not imply completion.
