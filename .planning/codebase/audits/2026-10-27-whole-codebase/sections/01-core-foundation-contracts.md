# Section 01 — Core foundation contracts

## Scope and coverage

Three independent passes inspected all 14 assigned files under
`crates/core/foundation/{capability,config,debug,diagnostics,files}/`, including
manifests and embedded tests. They sampled relevant shell and backend callers;
this is not an exhaustive review of every external consumer. See
[coverage](../00-coverage.md) for the file list. No assigned files remain.

## Process tree

Manifests and approvals enter catalog validation, then effective grants reach
runtime checks. Settings are loaded and schema-validated into raw and effective
projections, staged, revision-checked under a lock, atomically replaced, and
published to consumers. Contained resources use descriptor-relative bounded
reads. Issue events enter bounded instance maps, produce health and snapshots,
and feed generation-keyed debug publication. Failures are reported through
typed errors or diagnostics; settings recovery after a post-rename failure
crosses the control-plane boundary.

## Performance findings

### S01-PERF-01 — Wildcard grants can cause combinatorial matching

`capability/src/lib.rs:600` accepts repeated wildcard segments and
`glob_segments` / `glob_segment` at line 648 recursively try partitions.
Backend file authorization evaluates grants synchronously. Adversarial
nonmatching paths can therefore take combinatorial work outside ordinary script
instruction accounting. Use a bounded algorithm or bounded inputs, retaining
matching semantics. Benchmark release builds with 1/4/8 `**` segments against
8/16/32 path segments and ordinary grants, measuring latency and allocation;
test Unicode and home expansion. **Confidence:** confirmed complexity;
practical impact unmeasured. **Status:** new; no matching rejected experiment.

## Dead code and redundancy

### S01-DEAD-01 — Parallel config diagnostic model has no internal consumers

`config/src/lib.rs:27` defines `ConfigDiagnostic` and its severity; the
`ConfigError::Diagnostic` variant at line 601 has no construction sites in
repository source. Settings use `SettingsDiagnostic` instead. Consolidate or
remove the former after checking external API compatibility, then run the
workspace check and settings error-rendering tests. **Confidence:** confirmed
repository-unconsumed; external consumers unknown. **Status:** new cleanup.

`load_shell_settings`, `CapabilityCatalog::service_permissions`, and the
unversioned `SettingsStore::save` have no production repository callers in this
pass. They are public, so their external contracts need review before removal;
the capability accessor overlaps the older introspection finding.

## Logic and core mechanics

### S01-LOGIC-01 — Truncated diagnostic identifiers cannot resolve reliably

`diagnostics/src/lib.rs:338` truncates codes before insertion while
`resolve_issue` at line 430 looks up the original string. Long codes cannot
resolve by their original identity and equal prefixes collide. Reject oversized
codes or use bounded collision-resistant identities consistently; test long
UTF-8 input, colliding prefixes and record/resolve. **Confidence:** confirmed
source trace. **Status:** new; [interim evidence](../INTERIM.md).

### S01-LOGIC-02 — Diagnostic overflow can hide an error

`diagnostics/src/lib.rs:392` drops new issues at capacity, replacing them with
an overflow marker whose severity is warning at line 624. A table full of
warnings followed by an error remains degraded rather than error. Preserve the
worst omitted severity and test saturation, repeated errors and recovery.
**Confidence:** confirmed source trace. **Status:** new.

### S01-LOGIC-03 — Debug cache can tag stale data with a newer generation

`crates/core/shell/src/shell/runtime/debug.rs:76` builds a payload and
`store_debug_snapshot_payload` recomputes its generation key at line 114.
If diagnostics change after their snapshot is captured but before that key is
read, the payload can be cached under the newer generation until another
change. Capture and validate the generation around construction or retain the
pre-build key. Add a barrier-driven concurrent mutation regression.
**Confidence:** high, static interleaving; not reproduced. **Status:** new,
related to historical snapshot consistency work.

## Existing backlog or audit overlap

Existing schema-projection and grant-copy performance concerns have benchmark
fixtures; do not assume a speedup without measurement. The 2026-09-01 S01
report is historical: current code and tests address its instance filtering,
locked CAS, debug capability, array validation, diagnostic bounds and coherent
per-instance snapshot concerns. Revalidate any remaining historical item.

## Refuted suspicions

The old unused diagnostics dependency report is stale: the manifest now uses
serde and test-only serde_json. The former tooltip caller of
`load_shell_settings` is gone. Per-instance diagnostic snapshots use one lock,
and allocation counters are thread-local; the earlier repeated-lock and
global-atomic performance assumptions do not describe current code.

## Tests and benchmarks needed

Add the focused regressions above and the specified wildcard workload; validate
public API compatibility before any cleanup. Existing library tests passed in
the debug profile: `nix develop -c cargo test -p mesh-core-diagnostics -p
mesh-core-capability -p mesh-core-config -p mesh-core-files -p mesh-core-debug
--lib` reported 89 passed and 3 release-only benchmarks ignored.

## File coverage

Assigned and inspected: all 14 files in the section's inventory, including
inline tests and five manifests. Remaining: zero. Binary assets, build output,
vendored dependencies and Git data are excluded globally; other sections are
caller context, not assigned to this section.
