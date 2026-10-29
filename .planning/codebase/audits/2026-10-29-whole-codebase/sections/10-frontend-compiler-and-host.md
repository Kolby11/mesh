# Section 10 — Frontend compiler and host

## Scope and coverage

Review in progress. The prior inventory assigns 139 files and the continuation inventory identifies additional files; the working tree also contains an in-flight frontend-runtime extraction. The observations below are interim and do not establish complete file, test or caller coverage.

## Process tree

Module graph and source paths feed compilation, import resolution and contract validation. The catalog publishes compiled roots and dependency data. A host resolves component instances and their state, evaluates template expressions through the runtime, constructs or patches element trees, and passes effects and dirty state to shell orchestration. Reload and recovery must preserve consistent generations and usable prior state.

## Performance findings

Local component lookup scans the alias-keyed map and clones the resolved component (`frontend/compiler/src/lib.rs:491`); host resolution calls it before component memo handling (`shell/component/composition.rs:180`). This is an unmeasured hypothesis related to existing S10 import work. Benchmark 1/100/1,000 bindings and 10/100/1,000 repeated instances, cold builds, warm memo hits and leaf updates; record map visits, cloned AST bytes, allocations and frame p50/p95. Preserve owner scoping, ambiguity detection and contribution parity.

Catalog creation separately rereads watched files for its source fingerprint and compiled revision (`frontend/runtime/src/catalog.rs:319`, `frontend/compiler/src/lib.rs:435`). Shared sources can multiply I/O. Measure 10/100/1,000 roots with shared versus disjoint imports and cold edits versus graph-only activation, read counts/bytes, memory and activation latency. A single transaction snapshot could reduce duplication if it preserves contained reads and concurrent-edit detection. This is unmeasured and overlaps catalog refresh work; the independent correctness race is described below.

Template evaluation repeatedly calls the process-global expression cache and clones captured locals (`frontend/compiler/src/render/expr_eval.rs:19`, `frontend/compiler/src/lib.rs:79`); two-way binding lowering also clones element ASTs during render (`frontend/compiler/src/render/mod.rs:920`, `bindings.rs:4`). Measure release keyed lists and bound controls at 100/1,000/10,000 rows or controls, varying bindings, item size and fresh versus one-item patches. Track cache-lock visits, cloned bytes, allocations and frame latency; preserve Luau scope, dynamic key and event parity. These are unmeasured and overlap existing binding work.

`NodeServiceFieldDependencies::build` constructs a forward index alongside its reverse index (`frontend/runtime/src/service_deps.rs:18`), while its forward accessor is test-only and production callers use reverse lookup. Verify external and future API intent, then measure 100/1,000/10,000 nodes with 0/1/8 fields, allocations and structural rebuild latency before changing it. This overlaps service-observation backlog work.

Slot composition constructs contributions before applying the host's maximum (`frontend/compiler/src/render/mod.rs:397`, `:716`; `shell/component/composition.rs:406`). Graph resolution retains matching contributions beyond that limit. Measure 1/10/100 contributions at maximum 1/4/unlimited, cold and repeat mount, initialized components, allocations and latency; preserve ordering, placement, diagnostics and lifecycle semantics. This is an unmeasured hypothesis; whether discarded instances can produce observable side effects requires a separate integration test.

## Dead code and redundancy

Compiler accessibility projection and element normalization separately extract overlapping role, label, state and numeric attributes (`frontend/compiler/src/render/elements.rs:480`; `ui/elements/src/accessibility.rs:500`). Their baseline and live-tree responsibilities differ. Consider sharing only extraction/coercion with initial/patch and full/dirty parity for precedence, typed values, labels, hidden nodes and authored baselines; measure cold typed attributes before changing it.

JSON value hashing is implemented in frontend runtime and shell fingerprint paths with different numeric subtype encoding (`frontend/runtime/src/fingerprint.rs:20`, `shell/component/runtime_tree/fingerprint.rs:380`, `shell/component/interaction_state.rs:888`). The runtime encoding can produce identical input bytes for distinct integer and float values; slot memoization compares the resulting fingerprint (`shell/component/memo.rs:171`). Confirm actual invalidation and dynamic-prop reachability before claiming a user-visible defect. Consolidation needs typed scalar, array/object, slot-prop and interaction-cache parity tests.

`RuntimeGenerationIndex::register` and `sync` duplicate insertion and generation-bump logic (`frontend/runtime/src/generation.rs:64`); both have production callers, so neither is dead. A shared update function can retain both semantic APIs. Test registration order, repeated sync, child-before-parent registration and subtree revisions. Public adapter methods, `UiTag` variants and fixture compatibility paths without identified production callers remain API-review candidates, not confirmed dead code.

## Logic and core mechanics

**S10-LOGIC-01 — Production preview expressions have no evaluator.** `build_preview_tree_with_state` supplies no composition (`frontend/compiler/src/lib.rs:648`), and expression evaluation without one returns null in non-test builds (`render/expr_eval.rs:30`). Under `cfg(test)` the same path uses real Luau, so unit tests do not exercise production behavior. The scripting `LocaleBoundState` documentation explicitly recommends the preview API for translations (`runtime/scripting/src/context/state.rs:559`). Supply a real evaluator or require one explicitly, and add non-unit integration coverage for text, attributes, conditionals, loops and localization. Confirmed from source; actual external consumers remain unknown. This is related to the older expression-semantics audit, not a claim that its removed handwritten evaluator remains.

**S10-LOGIC-02 — Later successful expressions resolve an active failure diagnostic.** A failed template expression records an active issue and marks the frame failed (`shell/component/diagnostics.rs:93`), but each subsequent successful expression resolves the same instance issue (`shell/component/composition.rs:122`). Frame finalization still retains the old tree or displays the failure placeholder (`shell/component/rendering/mod.rs:516`), so the failure can remain present without an active diagnostic. Resolve only after the complete expression transaction succeeds; test failing then successful expressions in one instance, failed-frame retention and later recovery. High confidence from source; runtime regression not yet run.

**S10-LOGIC-03 — Contribution reuse omits the manifest revision.** Primary catalog reuse compares the manifest fingerprint (`frontend/runtime/src/catalog.rs:859`), but contribution reuse checks only source path and source fingerprint (`:943`). A manifest-only change can therefore retain a contribution's old `compiled.manifest` while its primary entry changes. Include the manifest identity in contribution reuse and test changed dependencies, metadata and settings with unchanged source. High confidence from source; an end-to-end regression remains needed. Runtime grants are obtained from activation-resolved capabilities, so this evidence does not establish a permission escalation.

**S10-LOGIC-04 — Source fingerprints can describe bytes that were not compiled.** The compiler parses a source read (`frontend/compiler/src/compile.rs:383`), while catalog fingerprinting and compiled revision construction reread watched paths afterward (`frontend/runtime/src/catalog.rs:319`, `frontend/compiler/src/lib.rs:435`). If a file changes between these operations, the old AST can be identified by the new bytes; a later graph rebuild can reuse it. Reload also records watched mtimes after compilation (`shell/runtime/reload.rs:32`), potentially acknowledging the new version without compiling it. Derive fingerprints from exact parsed bytes or validate a coherent pre/post snapshot and retry; test edits during compilation, watcher recovery and last-known-good publication. High confidence from source; a controlled scheduling regression remains needed.

**S10-LOGIC-05 — Parsed component cache retains another file's asset base.** The cache is keyed by source text, but a miss absolutizes relative URLs against the source file's directory before inserting the parsed component (`frontend/compiler/src/compile.rs:383`). A scratch reproduction compiled identical source from directories `a` and `b`; both compiled declarations pointed to `a/icon.png`. Parallel catalog compilation can make the selected base depend on scheduling. Cache the unbound AST and apply the asset base per file, or include the canonical base in the key. Test identical files in separate modules, local imports, concurrent compilation and reload. Confirmed at the compiler API boundary; downstream resource access and containment effects need separate verification.

## Existing backlog or audit overlap

The lookup hypothesis overlaps S10 performance backlog work. The older expression evaluator finding describes historical code and must not be repeated as current behavior.

## Refuted suspicions

Current test fallback uses real Luau; the old handwritten expression evaluator is not the current implementation.

## Tests and benchmarks needed

Run the preview integration regression and import workloads above. No performance speedup is claimed.

## File coverage

In progress. See the original and continuation inventories; comprehensive section and cross-boundary coverage has not yet been established.
