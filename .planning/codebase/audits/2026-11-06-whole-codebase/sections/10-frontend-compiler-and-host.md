# Section 10 — Frontend compiler and host

## Scope and coverage

Continuation of the unfinished [October review](../../2026-10-29-whole-codebase/sections/10-frontend-compiler-and-host.md). The current inventory assigns 206 files to this section. Frontend manifests and localization catalogs are cross-section inputs owned by sections 02 and 05. This report is interim: three independent agents reviewed performance, redundancy, and correctness, then began expanding file coverage. Their turns ended at an account usage limit. Many runtime bodies and tests still require complete reading; no whole-section or whole-codebase completion is claimed.

The parent verified the settings/event-loop and selected frontend findings directly and began implementing confirmed defects under the user's explicit request to make fixes as well as audit. Historical reports remain unchanged. The [coverage inventory](../00-coverage.md) distinguishes assigned files from partial and full reads.

## Process tree

Module manifests and source files feed compilation and the shared frontend catalog. A root instance adopts a compiled module, resolves capability grants and settings, creates Luau runtimes, and builds or patches widget trees. Style, layout, accessibility, retained display commands, resource preparation and presentation follow. Runtime identities, component-local state and durable instance storage must stay coherent across settings updates, catalog publication, hot reload and rollback.

Global appearance writes cross the settings control plane before notifying roots. Filesystem notifications can independently enter graph reconciliation. That seam crosses sections 02, 04, 06 and 15 and must not activate an unacknowledged profile document or charge an ordinary settings save for unrelated module preparation.

## Performance findings

### S10-PERF-21 — Appearance changes rebuild authored trees across roots

`component/shell_component/mod.rs::theme_changed` invokes `reset_render_caches`, discarding memoized component subtrees, widget/frame snapshots, intrinsic and Taffy state, retained display state and child display commands. `shell/runtime/theme.rs::mark_components_theme_changed` invokes it on every root. Settings eagerly instantiates inactive page components too, so the cold tree build includes more than its visible page.

Confirmed repeated work; its share of the reported roughly four-second live delay is not established. Existing `theme_switch_frame_cost_profile` measures one 920×900 Settings Appearance surface and ten same-theme invalidations, not request delivery, worker persistence, multiple surfaces, resource settlement or physical presentation. A first release process in this continuation measured mean invalidation 6.065 ms and first paint 57.699 ms; it ran during compilation and is diagnostic evidence, not a stable benchmark or speedup claim.

Retain authored structure and refresh theme-derived style/animation summaries, then reuse geometry only when resolved layout inputs and text measurements permit. Do not repeat the rejected complete-style-cache retention experiment. Measure actual alternating palettes, font family, text size and shell scale across multiple surfaces; collect template, style, layout, resource and commit phases and preserve final-value correctness.

### S10-PERF-22 — Frame publication reconstructs observation metadata

`component.rs::publish_frontend_frame` calls `service_observation_summary` on each publication. The implementation in `shell_component/mod.rs` walks all embedded runtimes, clones tracked field/subscription information, deduplicates and sorts it. Publication also clones/sorts diagnostic issues. Scroll-only, animation-only and sparse text frames pay this work despite unchanged subscriptions.

Confirmed source path; unmeasured. Cache immutable summaries by producer revisions, including runtime add/remove, newly observed fields and subscription changes. Unknown dependency summaries must remain conservative. Measure 1/100/1,000 runtimes, 0/16/256 diagnostics and 300 one-node, scroll or animation frames; count visited runtimes, cloned bytes and allocations alongside latency. Existing backlog observation-summary work overlaps this finding.

### S10-PERF-23 — Local resolution clones before memo lookup

`frontend/compiler/src/lib.rs::local_component` scans alias/scoped keys and returns a cloned `ComponentFile`; component composition resolves it before subtree memo lookup. `has_local_component` also delegates to this cloning lookup. Preserve ambiguity detection and owner scoping while separating identity lookup from immutable AST access.

Confirmed source behavior, already observed in the October interim report. Measure 1/100/1,000 aliases and 10/100/1,000 repeated instances, including cold builds and warm memo hits; count map visits, cloned AST bytes and allocations.

### S10-PERF-24 — Sparse writes still discover dirty runtimes by scanning

`component/template_patch.rs` scans the runtime map after collecting the initially requested instances and linearly checks membership in the requested list. A producer-owned dirty-runtime set could preserve callback/live-reference writes without inspecting clean runtimes. Measure one changed instance among 1/100/1,000, including neighboring-instance writes and transactional expression failure. Source-confirmed, unmeasured; overlaps change-proportional frame work.

### S10-PERF-25 — Checked-control patches discover part hosts globally

`component/rendering/mod.rs::collect_part_hosts` traverses the tree for changed checked controls. Retained snapshot paths could locate the changed hosts directly, with structural fallback. Measure one switch among 100/1,000/10,000 nodes, verify part geometry and inherited style parity, and count visits. Source-confirmed, unmeasured.

### S10-PERF-26 — Catalog preparation rereads overlapping source sets

`catalog.rs` computes watched-source fingerprints separately for primaries and contributions; shared compiled records and compiled revisions have additional fingerprint paths. Parsed component cache hits clone ASTs under a global mutex. Share exact-byte transaction snapshots rather than caching later reads under an earlier parse identity. Measure 10/100/1,000 roots with shared/disjoint imports, source edits and graph-only activation, recording reads, bytes, lock waits and memory. Existing source-identity and catalog backlog work covers this area.

### S10-PERF-27 — Binding lowering repeats during rendering

`frontend/compiler/src/render/mod.rs` invokes `lower_value_bindings`, which clones element ASTs and builds generated callback source through the expression cache. Compilation also performs binding lowering while collecting expressions. An immutable compiled binding recipe could avoid repetition while retaining authored spans and exact handler order. Measure 100/1,000/10,000 bound controls across cold compile and repeated full builds. Source-confirmed and already noted historically; no timing claim.

### Cross-section observation — Settings notifications load unrelated source trees

The watcher previously emitted generation-only notifications. Every notification entered `reconcile_installed_graph`, which loaded an authoring snapshot before its no-op comparison. `package/authoring.rs::with_content_revision` computes every module tree digest; `package/lock.rs::module_tree_digest` traverses and reads its files. Atomic settings saves therefore entered synchronous whole-graph work on the shell thread. Notifications can precede the persistence worker's completion message, exposing the written profile before its authoritative in-memory commit.

The in-progress fix retains event paths, merges coalesced paths with unknown events taking precedence, bypasses graph loading for known settings writes whose configured profile matches the committed authoring document, and defers profile checks while durable writes are pending. Unknown/directory/overflow and retired-generation events retain conservative reconciliation. A structural regression counts authoring graph loads; a release profile compares broad graph reading with the routed notification. This is narrower than implementing a full watcher/content revision index.

## Dead code and redundancy

- **S10-DEAD-01:** `service_deps.rs` populates a forward node-to-field map whose accessor is test-only. Production consumers use the independent reverse index. Remove or test-gate forward storage only after measuring 100/1,000/10,000-node builds with 0/1/8 reads and preserving reverse-query tests. Already observed historically; not a measured settings-delay cause.
- **S10-DEAD-02:** `UiTag::{List,ListItem,Separator,Spacer,Toggle}` have no constructors in current lowering; normalization emits their canonical runtime elements. These are public variants, so repository nonuse alone does not justify removal without an API decision.
- **S10-DEAD-03:** `RuntimeGenerationIndex::register` and `sync` duplicate entry creation, count accounting and ancestor revision updates. Both have real callers; one implementation can retain the two semantic public names. Registration, duplicate-registration and memo tests are the required regressions.
- **S10-DEAD-04:** The compiler's public `props_settings_schema` wrapper forwards to `mesh-core-component`; production module/shell callers already use the canonical owner. Only local tests use the wrapper. Public compatibility intent must be settled before removal.
- **S10-DEAD-05:** Three structural JSON hashing implementations remain in runtime fingerprint, component tree fingerprint and interaction state. Current implementations tag numeric subtypes. Duplication is real, but differing domain tags do not prove incorrectness; benchmark any hot-path consolidation.
- **S10-DEAD-06:** Settings `audio-page.mesh` computes and publishes `mute_label`/`unmute_label`, while buttons translate their labels directly and repository-wide searches find no reads of those globals. Remove redundant public state writes while retaining locale-switch coverage. Source-confirmed; not implemented in this pass.

## Logic and core mechanics

### S10-LOGIC-21 — Shared-module reload can retire a sibling root without replacing it

The initiating root's `reload_source` adopts the compiled replacement. Shell subsequently notifies other roots through the shared catalog. Previously `frontend_catalog_changed` retired runtimes belonging to changed modules but did not adopt the fresh primary compilation or create its own root runtime. Shell refreshed source mtimes, making a later independent reload unreliable as recovery.

The in-progress fix applies the source-reload lifecycle to sibling roots, flushes storage before replacement initialization, retains prior state on initialization failure, and avoids initializing the initiating root twice. Tests exercise two roots, fresh text and handlers, storage counts, and failed initialization retaining the old tree/runtime. The initial regression exposed a second ordering requirement: creating the replacement before old-runtime storage flush rereads stale storage. The implementation was corrected to use the existing detach/restore lifecycle.

### S10-LOGIC-22 — Tooltip offsets accumulate shell scale

`shell_component/internals.rs::refresh_tooltip_settings_from_theme` previously called a revision-gated settings refresh and then multiplied retained offsets. Repeated paints at scale 1.5 compounded values if defaults were missing or partial. The fix starts from unscaled settings for each projection, merges current defaults and applies scale once. Its regression covers repeated refreshes, missing/partial defaults, persisted offsets and returning to scale 1.0. The focused debug regression passed.

### S10-LOGIC-23 — Glass-bar volume scroll reads the wrong payload

The bar reads `event.delta_y`/`event.dy`, while core dispatch publishes `event.delta.y`. It also reverses the core direction convention and omits the explicit touchpad handler. The fix reads the nested payload and handles wheel and two-finger input consistently. A real component-input regression is in progress.

### S10-LOGIC-24 — Control-center battery percentages are rescaled

`glass-bar/control-center.mesh` still multiplies levels at or below one by 100 despite the power contract specifying 0–100 percentages. The bar, Orbit and battery widget have already been corrected; this remaining branch displays 1% as 100%. Remove the rescale and test 0, 0.5, 1, 2 and 100 with rendered footer text. Verification is in progress.

### S10-LOGIC-25 — Cross-slot move is split across uncorrelated writes

`composition-editor/main.mesh::moveNodeToSlot` removes from one slot and stages a later insertion in `pending_cross_slot`. A following composition update consumes the stage without verifying the source removal's committed revision or the initiating profile. Target rejection/disappearance can leave the source placement removed. Use an authoritative atomic placement move or explicit revision-bound recovery. Source-confirmed control flow; a controlled failure/revision regression is still needed before choosing the implementation.

## Existing backlog or audit overlap

The appearance latency, source-byte fingerprint, observation summary, inherited-rule pointer identity and production preview evaluator findings overlap existing backlog work. The production preview still returns null without a real evaluator outside unit tests; its separate backlog entry remains open.

## Refuted suspicions

- The parsed component cache now stores source-only ASTs and applies asset bases per file; the old identical-source/wrong-directory defect is fixed.
- Contribution reuse now checks manifest fingerprints.
- Catalog reload uses conditional publication/rebasing, and rollback advances its version; the old unconditional-publication/ABA findings are stale.
- Numeric JSON hashes now distinguish signed, unsigned and floating representations.
- Several old module findings have been fixed: false prop overrides, Wi-Fi stale state, numeric slider payloads, UTF-8 title truncation, theme mode observation and profile-menu supersession.
- Workspace indicator translation uses a fixed authored stride, but transform translations are scaled together with element widths/gaps; this is not an established scale bug.
- The two-second maintenance deadline is a recovery interval. Explicit settings worker completion and filesystem notifications wake the loop; a fixed four-second debounce has not been found.

## Tests and benchmarks needed

Complete real request-to-first-visible-frame and all-surfaces-ready tracing, including resource and compositor phases. Repeat release notification comparisons and Settings profiles with workload sizes and machine contention stated. Complete current runtime/transaction regressions and then the remaining per-file reads before finalizing this section or advancing the formal audit sequence.

## Read-only continuation after scope clarification

The user clarified that this task is a scan, not implementation. The observations below come from a subsequent read-only agent pass. Existing worktree fixes described above predate that clarification; they are not proof that their entire backlog areas are complete. No builds or source changes were made in this read-only pass.

### S10-PERF-28 — Unique-key churn retains absent runtimes and their work

`component/runtime.rs::ensure_runtime_for_compiled` and `ensure_runtime_for_local_component` insert contexts by instance key. The keyed component memo regression (`tests/integration/component_memo.rs:352`) confirms that these identities include authored item IDs. Ordinary structural finalization (`shell_component/mod.rs:1807`) collects live instance keys but prunes only prop recipes. Repository-wide removal searches found runtime retirement at whole-surface unmount/reload and selective catalog replacement, not successful ordinary tree reconciliation. Service delivery's take/restore paths retain every entry.

`runtime.rs::next_script_task_deadline`, `run_due_script_tasks`, and effect flushing continue visiting all retained contexts independently of tree membership. Churning through unique keyed children can therefore grow runtime, memo and interned-key state and keep absent contexts' tasks active. **Confidence: confirmed source behavior; memory growth and timer effects have not been reproduced or measured.** Conditional state retention may be intentional, and finer structural reconciliation is a Target in spec 03; the finding is an unbounded lifetime/work policy gap, not a blanket assertion that every missing child unmount violates shipped behavior.

Keep one visible child while rotating through 1,000 unique keys. Measure live/rendered and retained runtime counts, callbacks, memo entries, heap use, service-delivery visits and absent-child task emissions. Also test hide/show reuse of the same key before choosing explicit disposal versus bounded suspension semantics.

### S10-PERF-29 — Appearance projections repeat for unrelated settings

`modules/frontend/settings/src/components/appearance-page.mesh:562–582` combines resource catalog rows, active theme/card previews, locale, shell/profile offer, font size/scale and glass state in one effect. Size-only updates run `sync_system_resources` (`:409`) and `sync_theme` (`:368`). Each theme's recommendation (`:327`) rebuilds installed font/icon membership sets. This repeats approximately theme-count × catalog-size projection work and republishes arrays whose compound-state conversion can broaden template invalidation. **Confidence: confirmed projection path, unmeasured cost.** Existing appearance-latency and observation-granularity backlog work overlaps it.

Split projections by actual producer revisions and reuse installed-resource membership sets per catalog revision. Measure 1/10/100 themes × 24/240/1,200 resources while changing only text size/scale. Count row reconstructions, state conversions, expressions and dirty nodes. Do not attribute the reported four seconds to this path without a correlated trace.

### S10-LOGIC-26 — Glass preview writes can invalidate their authoritative projection

With absent stored strength tokens, `sync_glass` (`appearance-page.mesh:509`) reads `glass_blur`/`glass_liquid` and invokes `show_glass("theme", ...)`. Drag handlers (`:557`) write those same members and select the local `"custom"` marker. Public member reads are tracked by `scripting/context/runtime/effects.rs`, so a drag can invalidate the broad effect and overwrite the preview marker before release. **Confidence: high from source; no handler-to-paint regression was run.** Keep drag preview state distinct from stored-token projection. Test absent/present tokens, drag without release, external updates and rejected persistence.

### S10-LOGIC-27 — Runtime revision resets contradict its monotonic contract

`generation.rs::revision` documents a monotonic aggregate, but `rebuild` and `clear` reset its counter to zero. `component.rs::current_frontend_effect_revision` includes that counter. Distinct runtime sets can therefore reuse the same runtime stamp. Normal source reload also advances the catalog version, so a reachable queued stale effect crossing an unchanged-catalog reset is **not established**. This remains a confirmed public-counter inconsistency and a controlled regression requirement, overlapping the October interim observation. Test clear/rebuild with a retained effect and the same catalog revision before changing authority checks.

### Additional read-only coverage

This agent fully read `generation.rs`, `component/primitives.rs`, `component/disclosure.rs`, compiler `tags.rs`, `render/expr_eval.rs`, `render/style_context.rs`, and ABI `lib.rs`. It partially read compiler `compile.rs`/`bindings.rs`/`style.rs`; runtime `catalog.rs`/`component.rs`; component runtime/memo/rendering/shell-component seams; keyed memo tests; shell-adapter `lib.rs`; and Appearance template/script (CSS partial). These reads overlap prior passes. The other 186 of 206 assigned section entries were not fully reviewed by this agent. No aggregate completion claim follows from adding these counts.

### S10-PERF-30 — Neighbor synchronization repeatedly reconstructs and searches the binding graph

The parent directly inspected `component/runtime.rs:1616–1706`: `script_binding_neighbors` reconstructs an edge list from every live binding and callback, then scans that entire list for each reached vertex. `has_binding_neighbors` computes the complete connected component solely to answer a boolean. The root handler path (`:1504–1518`) can run that search once for the boolean and again to resynchronize neighbors. The per-neighbor external-access flag gates state synchronization only after graph discovery has finished.

**Confidence: confirmed repeated work and algorithmic shape; unmeasured frame impact.** Maintain revisioned adjacency or producer-reported touched contexts while preserving bidirectional callback/live-reference propagation and connected-component semantics. Measure sparse handler calls in 1/100/1,000-instance stars and chains, count edge visits/allocations, and verify callbacks, cyclic references and same-batch failure behavior. This overlaps change-proportional runtime work; it is not evidence that ordinary global settings changes traverse a large binding graph.

## File coverage

## November 7 focus and compiler continuation

The parent fully read `component/input/focus.rs`, `component/diagnostics.rs`, compiler `bindings.rs` and `render/style_context.rs`, including embedded tests. Cross-section excerpts: shell `runtime/request.rs:2702–2808`, host `shell_component/mod.rs:2833–2867`, ordinary focus-handler tests in `interaction/pseudo.rs` and `interaction/policy.rs`, and compiler binding patch evaluation in `render/mod.rs:1378–1430`. No builds, tests or source edits were performed.

### S10-LOGIC-28 — Failed focus transfer clears the source before validating its destination

Shell `apply_transfer_tab_focus` first calls `release_focus_for_transfer`, installs a `KeyboardMode::None` override on the source, and updates its surface state. Only afterward does it look up the destination component. If the destination is missing, it logs that the transfer is ignored and returns an empty request queue, without restoring the source focus or keyboard mode. `keyboard_focus_surface` is only assigned on success and can continue to name the cleared source. A stale return target after another surface is removed is a relevant lifecycle case; that live ordering has not been reproduced.

**Confidence: confirmed failure-path mutations; no live reproduction.** Validate and prepare the destination before committing source changes, or restore the complete prior state on failure. Cover missing destination, destination removal, empty traversal and valid forward/return transfers through the real shell handler. Existing popover tests exercise successful targets; they do not establish this failure path is safe. This spans section 15 orchestration and section 10 focus state.

### Focus callback consistency — source-backed follow-up, not a reproduced UI failure

Ordinary `set_focus_target` routes changes through `apply_focus_target_after_transaction`, which invokes blur and focus handlers. `clear_focus_for_transfer` and `apply_focus_transfer_from_traversal` directly update focus fields and interaction state instead, and the shell-facing trait methods return no callback requests. Automatic focus and keyboard-entry paths also reuse direct assignment. The difference is confirmed; the exact callback contract for every automatic entry path needs clarification before declaring all such transitions defective. A real cross-surface regression should compare callback counts and request effects with ordinary Tab traversal. No missing-target or callback fix was implemented.

### S10-LOGIC-29 — Commit-only IME edits can change native text without scheduling a repaint

`component/input/mod.rs:1052–1122` applies TextInputEdit deletion/commit to native input values, but invalidates text state only when preedit changes. With no previous or incoming preedit, changed text merely clears selection and dispatches input/change handlers. A handlerless control produces no script writes or fallback invalidation in the inspected wrapper/held-tree path. Presentation `PendingTextInput::take_edit` emits this commit-only/delete-only transaction shape. Ordinary TextInput and TextDelete explicitly invalidate, confirming the inconsistency.

**Confidence: confirmed missing invalidation on a production-reachable path; no IME/pixel reproduction.** Invalidate native text changes independently of preedit changes. Regress a previously painted clean handlerless input with commit-only non-ASCII text and deletion-only edits, asserting wants_render and updated value/geometry. Include no-op/effect-only handlers and unchanged-value transactions. The existing IME test changes preedit and script state, masking this branch.

### S10-LOGIC-30 — Shortcut overrides can steal native slider and configured activation keys

`input/keyboard.rs::unsafe_override_reason` (`:1264–1279`) reserves hardcoded Tab/Escape/Enter/Space/Ctrl+C, without current KeyboardSettings. Shortcut dispatch (`:99–109`) precedes focused slider stepping (`:169–202`) and native activation. An unmodified ArrowRight override is admitted and can consume the slider's default increment key. Custom activation keys have the same policy gap, subject to separate editable-input suppression and menu handling.

**Confidence: confirmed validation/dispatch mismatch against shipped spec 10's reserved native behavior; no remap reproduction.** Validate against the current native-key policy or order reserved native mechanics before actions while preserving focus/surface priority. Cover slider arrows, configured button/toggle/slider keys, modifiers, menus, editable controls and diagnostics. The existing unsafe-override test covers Tab only.

### S10-PERF-31 — Shortcut dispatch repeats tree searches per subscriber

`input/keyboard.rs:759–782` recollects subscribers for each candidate and recomputes active modal scope per matching subscriber. Keyboard event construction resolves bounds through additional searches. Debug action publication repeats subscriber scans per declaration. **Confidence: confirmed repeated traversal; latency unmeasured.** Measure 1/100/1,000 subscriptions/declarations with modal/focus scopes, count visits/allocations and require identical order, payload bounds and scope priority. Use revisioned indexes or one collected node/bounds pass; this overlaps existing change-proportional traversal work and is not a settings-delay diagnosis.

### Input continuation coverage and refutations

All 4,712 lines in keyboard.rs, input/mod.rs and widgets.rs were read, including embedded tests. Cross-file handler, invalidation, IME and unsafe-override fixtures were partially inspected. Hidden surface-action subscriptions were not promoted as universally defective: surface actions can intentionally live on wrapper containers. Cached pointer-release activation rechecks eligibility; UTF-8 deletion clamps scalar boundaries; primitive focus refresh consumes held activation keys. No safe API deletion, general shared-VM leak or obsolete typed-handler-path defect was established. No code edits, builds, tests or benchmarks were performed.

### Refuted wrapper suspicion

`BoundTemplateStore` omits loop metadata forwarding, but its inspected caller is recorded-binding reevaluation rather than structural loop rendering. That omission alone does not establish lost keyed identities. Binding callbacks preserve captured lexical state and assign typed values before authored handlers in the existing test sources; those tests were read, not executed.

The performance agent inspected excerpts in 42 of 294 broad frontend-related paths. The redundancy agent recorded 12 full-file reads and 16 partial reads before its last inventory update; additional module inspection was reported afterward without a complete refreshed list. The logic agent read catalog/reload/compiler seams and approximately 40 module scripts, not the entire runtime/test corpus. The counts overlap and must not be summed. Only explicitly recorded full/partial reads are carried into the inventory; all other files remain pending. Shipped behavior and targeted tests are not substitutes for reviewing every assigned file.
