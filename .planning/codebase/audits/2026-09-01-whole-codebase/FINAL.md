# MESH whole-codebase audit — 2026-09-01

## Overall summary

This audit set accounts for all **648 in-scope non-binary files** and all **30
current Cargo workspace packages**. Sections 01–15 were already completed in
the dated audit scaffold and were not re-run, per the user's instruction.
Section 16 was already complete in historical planning and is represented by a
dated reuse record that does not assert a new section pass. The cross-section
review was run against the current source and all existing reports.

No production code was changed. Repository writes are limited to audit
artifacts, backlog reconciliation, status, and the monthly completion log.

The strongest recurring conclusion is that MESH has made substantial progress
toward immutable candidates, canonical graph snapshots, generation-aware
provider routing, journaled package changes, bounded SHM, and syntax-aware LSP
analysis. The remaining risk is consistency at the seams: one activation
generation must bind module identity, policy, contracts, resources, settings,
components, and presentation; side channels must reject stale work; and tools
must remain projections of canonical contracts.

## Coverage

| Section | Assigned | Inspected/reused | Result |
| --- | ---: | ---: | --- |
| 01 Core foundation contracts | 14 | 14 | Complete dated report |
| 02 Module system and installation | 72 | 72 | Complete dated report |
| 03 Service contracts | 9 | 9 | Complete dated report |
| 04 Themes | 18 | 18 | Complete dated report |
| 05 Localization / i18n | 11 | 11 | Complete dated report |
| 06 Host resources and icon packs | 18 | 18 | Complete dated report |
| 07 Component language | 13 | 13 | Complete dated report |
| 08 UI element core | 51 | 51 | Complete dated report |
| 09 Interaction and motion | 18 | 18 | Complete dated report |
| 10 Frontend compiler and host | 68 | 68 | Complete dated report |
| 11 Luau runtime and sandbox | 59 | 59 | Complete dated report |
| 12 Rendering and paint | 57 | 57 | Complete dated report |
| 13 Surface policy and configuration | 4 | 4 | Complete dated report |
| 14 Wayland platform and presentation | 26 | 26 | Complete dated report |
| 15 Shell core and orchestration | 134 | 134 | Complete dated report |
| 16 Developer and authoring tools | 76 | Historical reuse | Already complete in planning; not re-audited |
| **Total** | **648** | **648 accounted** | **0 unassigned** |

Inventory details, exclusions, package metadata, and assignment rules are in
[`00-coverage.md`](00-coverage.md).

## Most important correctness findings

1. **Activation still has a durable/live split.** Profile/package state can
   reach a commit boundary before every fallible settings/theme/locale effect
   succeeds. A failed post-commit control-plane refresh is recorded as a
   warning after the profile is committed. See
   [`X-LOGIC-01`](cross-section-findings.md#x-logic-01--durable-state-and-live-activation-still-have-a-post-commit-split)
   and Section 15's activation findings.

2. **Candidate preparation can mix generations.** Profile activation loads a
   fresh graph but uses `self.modules` for capability, frontend, and backend
   preparation; graph commit does not replace that map before non-package
   activation paths continue. See
   [`X-LOGIC-02`](cross-section-findings.md#x-logic-02--candidate-graph-data-can-be-combined-with-stale-live-module-identities).

3. **The public active snapshot can be stale.** Direct settings/theme/locale
   commits update mutable runtime state and broadcast effects without
   republishing the `ActiveSnapshot` revisions and settings projection. See
   [`X-LOGIC-03`](cross-section-findings.md#x-logic-03--activesnapshot-can-lag-direct-settingsthemelocale-commits).

4. **Pending activation can lose the latest filesystem revision.** Watcher
   events arriving while resource/backend preparation is pending are rejected
   without a guaranteed retry after completion or abort. See
   [`X-LOGIC-04`](cross-section-findings.md#x-logic-04--filesystem-graph-events-can-be-lost-while-activation-is-pending).

5. **The frame-to-compositor path has several semantic authorities.** Element
   state, layout, transforms, hit testing, display-list signatures, damage,
   logical/physical extents, SHM copies, and protocol commits do not yet share
   one typed frame snapshot. See
   [`X-LOGIC-07`](cross-section-findings.md#x-logic-07--the-element-to-presentation-path-needs-one-semantic-frame-contract)
   and Sections 08, 09, 12, 13, and 14.

6. **Capability enforcement must converge on one effective runtime policy.**
   Legacy/raw fallbacks and separate service/UI/IPC routes remain a boundary
   risk even though many normal paths now check provider identity and policy.
   See
   [`X-LOGIC-06`](cross-section-findings.md#x-logic-06--capability-enforcement-must-remain-closed-across-service-ui-ipc-and-tools).

## Performance findings

- The shell loop performs broad control-plane, input, component, effect,
  render, and presentation work every frame; retained/display-list updates can
  still traverse broad trees. See `X-PERF-01`, `X-PERF-03`, and `X-PERF-04` in
  [`cross-section-findings.md`](cross-section-findings.md).
- Profile setup, package activation, authoring refresh, composition/provider
  resolution, and module-tree hashing repeat work across layers. See
  `X-PERF-02` and `X-PERF-05`.
- Service fan-out clones full JSON payloads into independent runtime stores;
  the impact is workload-dependent and requires the stated payload/runtime
  benchmark before changing isolation or storage.
- No speedup is claimed without a workload and repeated measurement. Proposed
  workloads include module/root/provider counts, node counts and dirty sets,
  payload sizes, event rates, cache-miss resources, release profile, p50/p95/
  max latency, allocations, queue depth, damage area, and activation time.

These are hypotheses or repeated-work observations, not measured regressions
unless the cited historical performance log says otherwise. The existing
backlog already contains the major retained-render, runtime-boundary,
presentation, and blocking-I/O work; no rejected optimization was revived.

## Dead-code and redundancy findings

- Contract schemas and projections remain duplicated across graph, service,
  component/compiler, shell, CLI, and LSP layers. The canonical owners should
  be the module graph/manifest, compiled service contract, component AST,
  runtime capability policy, package transaction, and shell orchestration
  respectively. See `X-DEAD-01`.
- Candidate module identity and `ActiveSnapshot` duplicate or lag behind
  activation plan data. These are correctness-relevant redundant authorities,
  not safe removal candidates until generation tests exist.
- The typed package transaction now is shared by shell and CLI, and canonical
  LSP manifest validation, UTF-16 protocol positions, indexed service delivery,
  bounded SHM, and LSP refresh generations were verified as addressed or
  narrowed. They are not reported as current blanket defects.
- Possible unused compatibility wrappers in backend candidate code remain
  unconfirmed and require call-graph and export review before removal.

## Core-mechanics and architecture findings

The recommended architectural direction is one revisioned activation
coordinator that owns an immutable candidate bundle:

```text
graph + module identities + contracts + capabilities + roots
  + settings + theme + locale + resources + providers
  -> prepared hidden runtime
  -> durable journal commit and live-generation acknowledgement
  -> atomic publication
  -> generation-tagged effects/callbacks/resources/presentation
  -> idempotent retirement and recovery
```

After activation, a typed semantic frame snapshot should derive layout,
interaction, rendering, damage, SHM upload, and compositor state. Tools should
consume the same canonical graph/contract snapshots while preserving editor
source spans. These changes have medium-to-high migration cost because they
cross package APIs, journals, runtime reuse, retained state, and test seams;
the section reports specify incremental adapters and failure-injection tests.

## Cross-section findings

The full cross-section report, including process trees, exact source evidence,
agent reconciliation, workloads, and refutations, is
[`cross-section-findings.md`](cross-section-findings.md).

The three fresh cross passes found:

- performance: repeated graph/composition and authoring work, broad retained
  collection, payload cloning, broad invalidation, and asymmetric queues;
- ownership: candidate/live module divergence, stale active-snapshot ownership,
  split provider selection, and remaining contract projections;
- logic/lifecycle: stale-root reuse, dropped watcher revisions, durable/live
  commit split, and the need for a universal generation envelope.

## Suggested execution order

1. Close path/capability/contract fail-open boundaries and add denial tests.
2. Finish the revisioned activation coordinator, including candidate module
   identity, `ActiveSnapshot`, pending watcher revisions, journal recovery, and
   live-generation acknowledgement.
3. Make provider, resource, runtime, component, watcher, and presentation
   callbacks generation-safe and shutdown-idempotent.
4. Establish the semantic frame snapshot and fix render/damage/input/
   presentation parity with golden and protocol tests.
5. Consolidate canonical contract projections and authoring/tooling refresh.
6. Run the workload benchmarks, then optimize only paths that clear a measured
   gate; retain rejected-experiment constraints from the performance log.

## Tests and benchmarks needed

- Failure injection at every package journal, profile pointer, resource,
  provider readiness, component publication, control-plane, presentation, and
  retirement boundary, including restart recovery.
- Same-version manifest edits, provider switches, capability revocation,
  settings/theme/locale/resource mutation, lazy child creation, and retained
  root reuse across activation generations.
- Pending watcher events and newest-revision retry after both successful and
  failed activation.
- Capability-denial matrix across frontend/backend/service/resource/IPC/CLI/
  LSP routes.
- Contract parity tests across runtime, shell, CLI, doctor, and LSP with
  malformed manifests, partial `.mesh`, service contracts, and sparse settings.
- Semantic-frame pixel, hit-test, damage, SHM, and protocol tests for transforms,
  blur, opacity, borders, fractional scale, region-only commits, configure
  races, popup identity, occlusion, and multi-output.
- Release-profile performance runs using the workload shapes in the section
  reports and cross report, with repeated ranges and no unsupported speedup
  claims.

## New, existing, historical, and refuted status

- **New in this completion pass:** cross-section findings `X-LOGIC-02`,
  `X-LOGIC-03`, and `X-LOGIC-04`; they are linked into the shell-core backlog.
- **Existing dated-audit findings:** section reports 01–15 contain the detailed
  new/overlap classifications from the prior completed audit and are linked
  below; grouped open items were reconciled into the backlog without priority
  labels.
- **Existing backlog/older audit:** activation, lifecycle, capability,
  rendering, presentation, runtime, and authoring items already present in
  `docs/BACKLOG.md` were not duplicated.
- **Performance hypotheses:** all `X-PERF-*` items require the stated workload
  and measurement plan; none asserts a speedup.
- **Rejected/refuted:** indexed service delivery, canonical LSP validation and
  UTF-16 positions, package journaling/recovery, nonblocking resource
  preparation, bounded SHM, and listed cache/scratch/display-list experiments
  were not reported as unresolved blanket failures.

## Section reports

1. [Section 01 — Core foundation contracts](sections/01-core-foundation-contracts.md)
2. [Section 02 — Module system and installation](sections/02-module-system-and-installation.md)
3. [Section 03 — Service contracts](sections/03-service-contracts.md)
4. [Section 04 — Themes](sections/04-themes.md)
5. [Section 05 — Localization / i18n](sections/05-localization-i18n.md)
6. [Section 06 — Host resources and icon packs](sections/06-host-resources-and-icon-packs.md)
7. [Section 07 — Component language](sections/07-component-language.md)
8. [Section 08 — UI element core](sections/08-ui-element-core.md)
9. [Section 09 — Interaction and motion](sections/09-interaction-and-motion.md)
10. [Section 10 — Frontend compiler and host](sections/10-frontend-compiler-and-host.md)
11. [Section 11 — Luau runtime and sandbox](sections/11-luau-runtime-and-sandbox.md)
12. [Section 12 — Rendering and paint](sections/12-rendering-and-paint.md)
13. [Section 13 — Surface policy and configuration](sections/13-surface-policy-and-configuration.md)
14. [Section 14 — Wayland platform and presentation](sections/14-wayland-platform-and-presentation.md)
15. [Section 15 — Shell core and orchestration](sections/15-shell-core-and-orchestration.md)
16. [Section 16 — Developer and authoring tools reuse record](sections/16-developer-and-authoring-tools.md)

Historical Section 16 detail remains in
[`../../../log/sections/16-developer-and-authoring-tools/improvements.md`](../../../log/sections/16-developer-and-authoring-tools/improvements.md).
