# Section 16 — Developer/authoring tools

## Scope and coverage

Read-only scan. No implementation changes, builds, tests, live settings mutations or benchmarks were performed. Existing uncommitted changes were inspected as pending work. This section report records source evidence and explicit coverage limits; the whole-codebase audit is unfinished.


## Process tree

LSP document open/change/save -> parse/version-check and publish diagnostics -> refresh generation -> canonical graph discovery and host-resource explanation -> generation-checked registry publication -> completion/hover/definition. CLI calls canonical package/profile/settings owners; CLI implementation was only sampled, not fully reviewed.

S16-PERF-21: every LSP document event synchronously reconstructs full graph and host-resource catalogs.
Confidence: confirmed source work; no timing measurement.
backend.rs did_open (:99–105), did_change (:108–116) and did_save (:119–122) invoke refresh_registry. refresh_registry (:270–294) calls synchronous ModuleRegistry::try_discover directly in an async handler. module_registry.rs:81–85 invokes load_authoring_snapshot, which traverses and hashes installed modules; from_snapshot (:105–154) rebuilds maps/interfaces/themes/locales and discover_resources. discover_resources (:314–319) calls discover_system_resources, including system font/icon discovery. The registry publication generation gate (:297–306) prevents stale commits only after all that cost. Plain unsaved .mesh edits therefore rescan disk graph/resources although the unsaved source buffer is not supplied to discovery. This may compete with shell CPU/IO during authoring, but is not direct evidence for Settings UI's four seconds.
Recommendation: scope registry invalidation to graph/manifest/dependency/resource inputs, preserve open-buffer overlays, debounce/coalesce and offload blocking discovery, reuse immutable host-resource catalogs by roots/revision. Measure 95-module workspace with 100 unsaved .mesh edits and manifest changes, graph loads/bytes/font scans and completion p50/p95, plus controlled stale refresh publication. Overlaps broad whole-catalog authoring refresh backlog, but per-keystroke LSP trigger and host-resource discovery are specific new evidence.

S16-LOGIC-21: formatting indentation allocation is unbounded client input.
Confidence: confirmed source behavior; no exploit or memory allocation test run.
backend.rs:232–236 builds " ".repeat(params.options.tab_size.max(1) as usize). tab_size is u32, allowing a multi-gigabyte indent string before formatting a tiny open .mesh document. The document read lock is held while the allocation/formatting runs. Reject unsupported indentation widths or enforce a bounded formatter option, returning a typed request error. Test tab sizes 0,1,4,normal maximum,u32::MAX and nested input, preserving standard output. No matching backlog item found.

Potential race withheld from confirmed findings: update_document drops its update lock before diagnostics publication; did_close removes versions then publishes an unversioned clear. An earlier update could publish diagnostics after close. Versioned diagnostics may be discarded by clients, and a controlled client transport scheduling test is needed. Do not describe this as a proven user-visible stale-diagnostic defect.
Redundancy: ModuleRegistry::discover/refresh wrappers have real compatibility roles; no deletion recommendation from excerpts. Existing source normalization/schema ownership duplicated between tooling and runtime remains a broad audit concern, not a newly demonstrated removable copy.

## Review layers and existing overlap

Performance, redundancy and correctness were assessed on the inspected seams above. Unmeasured work is distinguished from measured cost. Source-only hypotheses do not establish the reported four-second Settings delay. Existing broader backlog items remain open; current local implementation edits must not be counted as independently verified completed findings.

## Refuted suspicions

No fixed four-second settings debounce was identified. Unknown filesystem events retain conservative reconciliation. Caller-specific caveats and refuted or withheld suspicions are stated above; unavailable target behavior is not classified as a shipped defect.

## Tests and benchmarks needed

Run each finding's proposed deterministic regression and representative repeated release workload before changing code or claiming performance gains. None was run during this read-only scan.

## November 7 tooling continuation

No builds, tests or source edits were performed. The findings below were independently checked by the parent against the cited source. Exact inspected ranges remain distinct from complete file reads in the coverage inventory.

### S16-PERF-22 — No-op Git candidates are classified before their version is known

`update.rs::CandidateModule::is_unchanged` compares both version and revision. `collect_update_candidates` initializes `candidate_version` as an empty string, and `stage_candidate_graph` calls `is_unchanged` before candidate graph loading fills that version. A normal nonempty locked version therefore makes an identical-revision Git candidate enter `stage_git_revision` rather than the unchanged-tree copy path.

**Confidence: confirmed phase-order behavior; cost unmeasured.** Determine unchanged source identity before selecting staging work, while preserving deliberate `--replace` behavior and complete candidate-graph validation. Measure 1/10/100 unchanged Git modules: materialization, clones, filesystem/network work and wall time. This is specific evidence for the existing package no-op preparation backlog, not a duplicate independent task or a reason to skip validation.

### S16-LOGIC-22 — Lock verification silently discards digest errors

`update.rs::verify` (`:1068–1080`) converts `has_local_edits` errors with `.ok()` inside `filter_map`, omitting that locked module from results. `main.rs::cmd_lock` (`:1647–1675`) has no per-module failure value to report; an all-error result with valid provenance can print `no locked modules`, while mixed results can report zero differences. A failed source-tree validation is not successful verification.

**Confidence: confirmed error-erasure path; no CLI execution.** Return a typed per-module verification outcome and fail closed on read/validation failure. Cover valid, edited, missing, unreadable and symlink-invalid trees, including a lock where every digest fails. Provenance verification remains a separate obligation.

### S16-LOGIC-23 — Unicode whitespace can produce an invalid completion slice

`analyzer/mod.rs::try_upgrade_to_proxy_ctx` (`:34–57`) searches with `char::is_whitespace`, then adds one byte to the matched character's byte offset before slicing. For a multibyte whitespace scalar such as U+2003 or NBSP, that offset is not a UTF-8 boundary. If the proxy/ref/component/props completion branch is entered, the request handler can panic.

**Confidence: source-confirmed invalid slice under the stated branch conditions; no request reproduction.** Advance by the matched character's UTF-8 width or use lexer spans. Test complete and incomplete script input through the actual Document/completion entrypoint, not just a standalone string slice. The similar code in `util.rs` uses `is_ascii_whitespace`, so its one-byte advance is safe for that predicate and is not this bug. A request panic does not by itself prove the entire server terminates.

### S16-LOGIC-24 — Raw block lookup disagrees with parsed component structure

`document.rs::block_content_range` finds the first raw `<script`/other opening substring and first closing substring. Completion, hover and definition use this through `util.rs::block_at_offset`. A parser-accepted XML comment containing example script tags before the actual script can make those helpers identify the comment block instead of the real one. The canonical component parser already handles comments and quoted-string closing-tag context.

**Confidence: confirmed structural disagreement; no LSP regression run.** Use parsed block spans with the canonical parser's recovery behavior. Test commented example tags before template/script/style, quoted content and incomplete documents; retain correct byte/UTF-16 conversions.

### S16-LOGIC-25 — Component attribute completion confuses inputs with public script state

`analyzer/template.rs::complete_attrs` (`:75–92`) obtains custom-component attributes through `resolve_component_public_members`. That helper reads child source and `document.rs::public_component_members` derives bare reactive variables/functions, not declared `<props>`. A declared `label` prop with no matching bare global is omitted, while unrelated public state can be offered as an input. Public bind:this exports and typed component inputs have distinct contracts.

**Confidence: confirmed source/contract mismatch; no completion request regression.** Use canonical prop declarations for input completion while retaining public members for bind:this access. Cover declared props, unrelated public state, private locals, callbacks and imported/custom components.

### S16-LOGIC-26 — Formatting corrupts non-ASCII template content

`format.rs::canonicalize_template_attrs` (`:227–248`) copies ordinary UTF-8 bytes with `out.push(bytes[i] as char)`. Each byte of an accented character or emoji becomes a separate Unicode scalar. The template formatter invokes this helper for ordinary template lines, and the LSP returns a whole-document replacement.

**Confidence: source-confirmed data corruption; no formatting request executed.** Preserve untouched UTF-8 slices while rewriting expression attributes. Cover literal text, quoted attributes, mixed expressions, accented text and emoji through document formatting, with a second-format idempotence assertion.

### S16-PERF-23 — Semantic-token coordinates repeatedly rescan the document prefix

`semantic_tokens.rs::TokenBuilder::push` (`:507–514`) calls `line_and_character` (`:553–555`) for each token. `util.rs::offset_to_position` (`:146–165`) scans characters from the document start to that offset. With token count proportional to document size, total coordinate conversion work grows quadratically.

**Confidence: source-confirmed repeated scanning; latency unmeasured.** Build a reusable line/UTF-16 index or advance a coordinate cursor while preserving token order and Unicode correctness. Measure generated 100/1,000/10,000-node documents across repeated release runs and compare resulting tokens exactly.

### Additional S16-LOGIC-23 caller — JSON contract completion

`json/complete.rs::complete_contract_text` (`:120–124`) searches for a character outside an ASCII identifier and advances its byte index by one before `split_at`. A non-ASCII character also matches that predicate, so a contract fragment ending in `é` can split inside UTF-8. This is a second affected completion path, distinct from the safe ASCII-whitespace predicate in `util.rs`.

### November 7 production coverage

All LSP Rust production bodies and CLI production bodies have now been read. Nineteen files in the tooling continuation were fully read; thirteen further files have explicit partial ranges because their embedded tests remain unread. This does not cover all section 16 scripts, editor assets or test fixtures, and does not establish independent completion of all three audit layers.

## Earlier coverage checkpoint

## November 7 tooling completion pass

### S16-LOGIC-27 — Backlog runners validate removal in the worktree instead of the feature commit

Both `scripts/codex-backlog-runner.sh:452–482` and `scripts/claude-backlog-runner.sh:434–465` require one new nonmerge commit and some backlog/log change, then grep the mutable worktree for the selected marker. They do not require exactly one removed unchecked item or absence of the selected item in the committed blob, despite the development guide's explicit guarantee. A commit can leave the item present while an uncommitted removal satisfies the gate; the runner then puts that removal in a separate recovery commit and reports the first as the validated feature.

**Confidence: confirmed control flow and guide mismatch; no runner execution.** Validate before/after committed contents and selected identity, enforce removal count and append-only required log evidence, then handle leftover recovery changes separately. Use isolated Git fixtures for correct single removal, multiple removals, unrelated committed edits plus uncommitted removal, reworded markers and log deletion.

### S16-LOGIC-28 — Failed perf decoding is reported as zero observed CPU

`tools/profile-report:121–155` suppresses decoder stderr and ignores `perf script` exit status. With valid activity/stage logs and a failed existing recording, main (`:279–305`) can still emit a normal summary with no samples and zero CPU. `perf evlist` failure also silently substitutes 499 Hz. An intentional log-only session is supported and must remain distinguishable from a recording that failed decoding.

**Confidence: confirmed error-to-normal-report path; no failing-decoder probe.** Preserve decoder status and mark CPU unavailable or fail that portion explicitly. Validate sampling metadata. Cover nonzero exit with no or partial samples, frequency discovery failure, explicit log-only mode and successful non-499-Hz input. This extends the existing comparable profiling workload item.

### S16-PERF-24 — Session reports rescan samples for every activity window

`tools/profile-report:162–167,295–323` repeatedly filters all CPU samples and stage rows per window and repeats membership scans for combined activity reports. Work grows with windows multiplied by sample/stage count. **Confidence: confirmed repeated scanning; offline tool cost unmeasured.** Index ordered timestamps and reuse window aggregates while preserving overlap boundaries and exact counts. Measure 10k/100k/1M samples with 100/1k/10k windows across repeated runs and require report parity. This is not shell settings latency.

### S16-DEAD-21 — Backlog launcher policy is duplicated

The Codex and Claude runners duplicate selection, Git validation, recovery and prompt policy, including the same defective commit gate; both are real documented entrypoints. Consolidate shared policy behind agent-specific invocation/protocol/retry adapters rather than removing one runner. Adapter fixtures must preserve their differing JSONL protocols, usage retries and context rollover. This is duplication, not proven dead code.

### Assigned-file reading complete

All 59 assigned section 16 files now have full source reads: twenty-five pending tooling/test/build/editor files and thirteen partial Rust tails were completed alongside twenty-one prior full reads. The [inventory](../00-coverage.md) records each path. Independent audit layers and whole-codebase completion remain open. Gate review refuted a suspected missing-metric pass: CPU gates reject missing/duplicate observations, and the GPU gate requires a passed test with `MESH_REQUIRE_GPU=1`. No source fixes, tests, builds or benchmarks were performed.

Earlier combined checkpoint, superseded by the November 7 reads above: across sections 13–16, 166 assigned files had 10 full reads, 22 partial reads and 134 unreviewed. The [inventory](../00-coverage.md) contains current cumulative status; section 16 now has 59/59 full source reads. The checkpoint counts must not be added to later passes.
