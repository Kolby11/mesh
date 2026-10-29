# Section 10 — Frontend compiler and host

## Scope and coverage

Review in progress. The prior inventory assigns 139 files and the continuation inventory identifies additional files; the working tree also contains an in-flight frontend-runtime extraction. The observations below are interim and do not establish complete file, test or caller coverage.

## Process tree

Module graph and source paths feed compilation, import resolution and contract validation. The catalog publishes compiled roots and dependency data. A host resolves component instances and their state, evaluates template expressions through the runtime, constructs or patches element trees, and passes effects and dirty state to shell orchestration. Reload and recovery must preserve consistent generations and usable prior state.

## Performance findings

Local component lookup scans the alias-keyed map and clones the resolved component (`frontend/compiler/src/lib.rs:491`); host resolution calls it before component memo handling (`shell/component/composition.rs:180`). This is an unmeasured hypothesis related to existing S10 import work. Benchmark 1/100/1,000 bindings and 10/100/1,000 repeated instances, cold builds, warm memo hits and leaf updates; record map visits, cloned AST bytes, allocations and frame p50/p95. Preserve owner scoping, ambiguity detection and contribution parity.

## Dead code and redundancy

Compiler accessibility projection and element normalization separately extract overlapping role, label, state and numeric attributes (`frontend/compiler/src/render/elements.rs:480`; `ui/elements/src/accessibility.rs:500`). Their baseline and live-tree responsibilities differ. Consider sharing only extraction/coercion with initial/patch and full/dirty parity for precedence, typed values, labels, hidden nodes and authored baselines; measure cold typed attributes before changing it.

## Logic and core mechanics

**S10-LOGIC-01 — Production preview expressions have no evaluator.** `build_preview_tree_with_state` supplies no composition (`frontend/compiler/src/lib.rs:648`), and expression evaluation without one returns null in non-test builds (`render/expr_eval.rs:30`). Under `cfg(test)` the same path uses real Luau, so unit tests do not exercise production behavior. The scripting `LocaleBoundState` documentation explicitly recommends the preview API for translations (`runtime/scripting/src/context/state.rs:559`). Supply a real evaluator or require one explicitly, and add non-unit integration coverage for text, attributes, conditionals, loops and localization. Confirmed from source; actual external consumers remain unknown. This is related to the older expression-semantics audit, not a claim that its removed handwritten evaluator remains.

**S10-LOGIC-02 — Later successful expressions resolve an active failure diagnostic.** A failed template expression records an active issue and marks the frame failed (`shell/component/diagnostics.rs:93`), but each subsequent successful expression resolves the same instance issue (`shell/component/composition.rs:122`). Frame finalization still retains the old tree or displays the failure placeholder (`shell/component/rendering/mod.rs:516`), so the failure can remain present without an active diagnostic. Resolve only after the complete expression transaction succeeds; test failing then successful expressions in one instance, failed-frame retention and later recovery. High confidence from source; runtime regression not yet run.

**S10-LOGIC-03 — Contribution reuse omits the manifest revision.** Primary catalog reuse compares the manifest fingerprint (`frontend/runtime/src/catalog.rs:859`), but contribution reuse checks only source path and source fingerprint (`:943`). A manifest-only change can therefore retain a contribution's old `compiled.manifest` while its primary entry changes. Include the manifest identity in contribution reuse and test changed dependencies, metadata and settings with unchanged source. High confidence from source; an end-to-end regression remains needed. Runtime grants are obtained from activation-resolved capabilities, so this evidence does not establish a permission escalation.

## Existing backlog or audit overlap

The lookup hypothesis overlaps S10 performance backlog work. The older expression evaluator finding describes historical code and must not be repeated as current behavior.

## Refuted suspicions

Current test fallback uses real Luau; the old handwritten expression evaluator is not the current implementation.

## Tests and benchmarks needed

Run the preview integration regression and import workloads above. No performance speedup is claimed.

## File coverage

In progress. See the original and continuation inventories; comprehensive section and cross-boundary coverage has not yet been established.
