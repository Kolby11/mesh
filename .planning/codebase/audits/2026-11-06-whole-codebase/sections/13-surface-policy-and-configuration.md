# Section 13 — Surface policy and configuration

## Scope and coverage

Read-only scan. No implementation changes, builds, tests, live settings mutations or benchmarks were performed. Existing uncommitted changes were inspected as pending work. This section report records source evidence and explicit coverage limits; the whole-codebase audit is unfinished.


Coverage: all four assigned files read completely, including all embedded tests:
- crates/core/surface-policy/Cargo.toml
- crates/core/surface-policy/src/lib.rs (1,088 lines)
- crates/core/surface-config/Cargo.toml
- crates/core/surface-config/src/lib.rs (1,898 lines)
Relevant presentation and manifest consumers were read in excerpts, not fully audited.

## Process tree

manifest defaults -> checked sparse namespace/prop scopes -> SurfaceLayoutSettings -> declared SurfacePolicySnapshot -> SurfacePolicyPatch::between -> compiler applies patch and assigns revision -> semantic diff/transition plan -> shell and presentation. Props normalize through mesh-core-component; role metadata is shared with manifest/settings diagnostics.

S13-LOGIC-21 / cross-section14: center is omitted from canonical policy identity.
Confidence: confirmed source behavior and reachable live-config path. New specific finding, no duplicate backlog line found.
SurfaceLayoutSettings.center exists at surface-config/lib.rs:42 and is read from manifest (:262) and stored settings (:374). Its policy_snapshot (:79) has no center projection because SurfacePolicySnapshot (surface-policy/lib.rs:178–202) lacks the field. Presentation SurfaceConfig.center exists (presentation/backend/config.rs:354), but policy_snapshot (:431) likewise omits it. config semantic_diff (:492) compares those incomplete snapshots. SurfaceEntry::needs_reconfigure (entry.rs:490–496) therefore returns false for an otherwise-identical live center toggle; WaylandSurfaceBackend::configure (surfaces.rs:235–246) only updates padding in that case. Yet protocol.rs:89–90 uses cfg.center to choose TOP|BOTTOM versus TOP-only anchoring for fixed-height left/right rails. Changing center alone can fail to reposition the live surface until another non-noop change.
The metadata table also lacks center: surface-policy/lib.rs:58–120 contains only Anchor with settings key anchor. Manifest SurfaceLayoutSection::has_role_field (module/manifest/model.rs:876–887) checks self.anchor, never self.center. Thus window + center does not receive the role-mismatch warning promised for layer-only placement fields in spec01 section3.3. Include this as the same canonical-vocabulary omission, not an unrelated second bug.
Recommendation: add center to authoritative normalized identity, patch and layer geometry diff, and to role-specific vocabulary/diagnostics (possibly as part of Anchor metadata with an explicit presence predicate). Test center-only settings changes on fixed-height left/right layer surfaces, layer-config diff, manifest window+center diagnostics, promotable exemption, and no unnecessary role recreation. Spec01 section3.3 explicitly marks centering shipped. No live reproduction was run.

S13-LOGIC-22: public between(base,target) patch cannot clear optional snapshot values.
Confidence: confirmed helper semantics; actual user-visible reset failure unproven.
SurfacePolicyPatch::between (surface-policy/lib.rs:329–353) constructs target.window_title/window_app_id with then(...).flatten(), and edge/content_size/surface_size similarly flatten the target Option. For base Some(value) -> target None, the patch contains None, which apply_to (:401–440) interprets as keep base, not clear it. compile(contract(base),between(base,target)) cannot round-trip every valid target. Settings resolution generally resets preferences by rederiving a declared baseline with an omitted override, so this is not by itself proof that removing a Settings title override is broken. Recommend explicit Clear representation only if the API intends a complete diff; otherwise narrow and document the precondition. Regression: property-style round-trip across optional Some/None transitions and precedence/reset behavior, with revision correctness.

Performance layer: source confirms repeated raw clone/validation and a fresh SurfacePolicyCompiler per resolution (surface-config/lib.rs:383–392), but no measurement establishes this as material. The prop validator linearly searches definitions for each entry (:905–911), including every instance scope; known-name advice can repeat work. Measure 10/100/1,000 prop definitions × 1/100/1,000 instances, changed single field versus full reload, allocations and validation time. Preserve exact diagnostic order and canonical normalization. This overlaps existing S13 compiled-by-revision/field-group-diff backlog work; do not present it as a new speedup.
Redundancy layer: role-change predicate appears in SurfaceLayoutSettings adapter (surface-config/lib.rs:196–202) and DeclaredSurfaceContract::role_change_allowed (policy/lib.rs:293–295). Both express current==requested || promotable using different boundary enums. Neither is dead; small duplication does not justify deleting the adapter without API/caller migration. No high-confidence dead-code removal recommended. Existing defaults/parsers generally use canonical module values and tests guard alias/schema parity.

## Review layers and existing overlap

Performance, redundancy and correctness were assessed on the inspected seams above. Unmeasured work is distinguished from measured cost. Source-only hypotheses do not establish the reported four-second Settings delay. Existing broader backlog items remain open; current local implementation edits must not be counted as independently verified completed findings.

## Refuted suspicions

No fixed four-second settings debounce was identified. Unknown filesystem events retain conservative reconciliation. Caller-specific caveats and refuted or withheld suspicions are stated above; unavailable target behavior is not classified as a shipped defect.

## Tests and benchmarks needed

Run each finding's proposed deterministic regression and representative repeated release workload before changing code or claiming performance gains. None was run during this read-only scan.

## File coverage

All four assigned files were read in full, including embedded tests: `crates/core/surface-policy/Cargo.toml`, `crates/core/surface-policy/src/lib.rs`, `crates/core/surface-config/Cargo.toml`, `crates/core/surface-config/src/lib.rs`. Neighboring manifest/presentation callers were inspected partially. One reviewer covered all three layers; independent exhaustive caller/test rereviews are not claimed.
