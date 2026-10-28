# Whole-codebase audit — interim findings

This audit is incomplete. Three subagents and the coordinator sampled current
source and compared findings with the backlog and older audits; they did not
inspect every file or complete the 16-section procedure in `prompts.md`.
The workspace inventory returned 884 tracked and untracked non-ignored paths
from `rg --files`; that count is not a reviewed-file count. Cargo workspace
metadata was obtained through `nix develop`. The five foundation crates'
existing library tests passed in debug profile (89 passed, 3 release-only
benchmarks ignored); no new regression tests or benchmarks were run. The
module crate's existing library tests also passed (346 passed, 1 ignored).
The working tree already contained unrelated changes.

## Findings

1. **Fork creation depends on JSON formatting and leaves partial copies.**
   `crates/core/extension/module/src/package/fork.rs:83` copies both trees
   before `rename_manifest` at line 105 searches for an exact `"name": `
   substring. Valid minified JSON fails, leaving a destination that prevents
   retry. Use structured editing and staged publication; test formatting
   variants and failures at each copy/write step. Confidence: confirmed by
   source trace; new relative to the backlog.
2. **Removing an optional positional argument is classified as additive.**
   `crates/core/extension/service/src/compatibility.rs:538` permits removal,
   while `crates/core/runtime/scripting/src/context/proxy.rs:320` rejects an
   old caller that supplies it. Classify removal as breaking and test both
   supplying and omitting the argument. Confidence: confirmed by source trace;
   new relative to the backlog.
3. **Resolved stylesheet image paths are rejected.** Component and theme
   stylesheets rewrite relative URLs to absolute paths at
   `crates/core/ui/component/src/style.rs:289` and
   `crates/core/foundation/theme/src/css.rs:207`, while
   `crates/core/ui/elements/src/style/parse.rs:804` rejects absolute paths.
   This conflicts with shipped relative URL resolution in
   `docs/spec/04-styling.md:757`. Preserve validated provenance through style
   lowering and test component, imported, and theme images end to end.
   Theme `@module` custom rule bodies also bypass URL rewriting before later
   parsing (`theme/src/css.rs:171` and `theme/src/lib.rs:1860`); cover them
   separately in the same regression matrix.
   Confidence: high, source trace; new relative to the backlog.
4. **Tab traversal excludes offscreen scroll descendants.**
   `crates/core/ui/interaction/src/focus.rs:211` prunes nodes outside clips,
   and `next_focus_target` at line 128 traverses only the remaining nodes.
   Controls below the viewport cannot be reached by Tab. Include eligible
   scroll descendants and reveal the selected target; test forward and reverse
   traversal in nested scroll areas. Confidence: high, source trace; new.
5. **Easing keywords do not implement their CSS curves.**
   `crates/core/ui/animation/src/easing.rs:40` maps `ease` and `ease-in-out`
   to the same polynomial and uses cubic polynomials for the other keywords.
   Use standard keyword curves and test interior points. Confidence: confirmed
   by source trace; new relative to the backlog.
6. **A descendant holding command pipes can stall shutdown.**
   `crates/core/runtime/scripting/src/backend/exec.rs:661` stops monitoring
   cancellation when the direct child exits, then joins blocking pipe readers;
   shutdown joins the worker at line 431 without a deadline. A background
   descendant retaining a pipe can hold the worker and child budget after the
   caller times out. Test timeout, group cleanup, budget release and shutdown
   with a pipe-holding descendant. Confidence: high, source trace; related to
   older deadline work but this failure mode was not found in the backlog.
7. **Transient Wayland write backpressure propagates as a fatal error.**
   `crates/core/presentation/src/wayland_surface/backend/events.rs:157` returns
   flush errors, maps `WouldBlock` to `SurfaceCreate` at line 275, and polls
   without writable readiness at line 180. Handle pending writes and retry on
   writable readiness; inject `WouldBlock` followed by success and test a real
   disconnect separately. Confidence: high, source trace; new.
8. **Long diagnostic codes lose their identity.**
   `crates/core/foundation/diagnostics/src/lib.rs:338` truncates the issue
   code before using it as a key, while `resolve_issue` at line 430 looks up
   the original code. Long codes cannot be resolved as recorded and codes
   sharing a prefix collide. Reject oversize identifiers or use a bounded,
   collision-resistant identity, and test long UTF-8 codes and resolution.
   Confidence: confirmed by source trace; new.
9. **Filesystem grant glob matching can grow combinatorially.**
   `crates/core/foundation/capability/src/lib.rs:600` permits repeated
   wildcards; the recursive matching at line 648 explores partitions on
   nonmatches. Backend file operations evaluate the grant synchronously.
   Bound the input or use bounded-complexity matching while preserving
   semantics. Benchmark release builds with 1/4/8 `**` segments against
   8/16/32 path segments, plus ordinary grants; measure latency and
   allocations. Complexity is confirmed by source; practical impact is
   unmeasured. No corresponding rejected experiment was found.
10. **Diagnostic saturation can hide an error.** At
    `crates/core/foundation/diagnostics/src/lib.rs:392`, new issues beyond
    capacity are replaced by an overflow issue whose severity is always a
    warning at line 624. If warnings fill the table, a subsequently dropped
    error leaves health degraded rather than error. Preserve the worst omitted
    severity and test saturation, repeated errors, and recovery. Confidence:
    confirmed by source trace; new.
11. **Authoring diagnostics read catalogs without the runtime's bound.**
    `crates/core/extension/module/src/package/installed_graph/diagnostics.rs:653`
    checks a catalog path and then reads the entire file independently. An
    oversized catalog can consume unbounded memory, and replacement between
    path checking and opening can change the file read. Reuse the bounded,
    descriptor-relative catalog reader; test oversized files and symlink
    replacement. This path requires opt-in authoring diagnostics. Confidence:
    high, source trace; new relative to the catalog-refresh backlog item.
12. **Fork rebase can leave a mixed state after failure.**
    `crates/core/extension/module/src/package/fork.rs:260` writes merged files
    in place, removes the old base, copies a new base and only then updates the
    record. An I/O failure or interruption can leave partial edits or a missing
    baseline with an old record. Stage and commit the fork and baseline with
    recovery; inject failures during edits, baseline replacement and record
    publication. Confidence: high, source trace; new.
13. **New roots in composition-backed profiles are treated as orphans.**
    `crates/core/extension/module/src/package/management.rs:83` adds a UI
    root to the profile, but `package/composition.rs:231` marks every root
    absent from the base composition orphaned and `to_profile` filters it out.
    Distinguish user-added roots from overrides of removed inherited roots;
    test installation and activation with a composition-backed profile as well
    as real orphan retention. Confidence: high, source trace; new.
14. **Source scan budgets do not bound traversal or rejection records.**
    `crates/core/extension/module/src/package/installed_graph/scan.rs:390`
    continues recursively walking directories after the file budget is reached
    and adds a rejection for every further matching file. Deep or large trees
    can consume unbounded stack, time, and rejection memory even though source
    bytes are capped. Bound entry count and depth and summarize excess; test
    deep trees, non-source entries, and many rejected sources. Confidence:
    confirmed source trace; distinct from the older fixed source-byte gap.
15. **Forking a root produces an invalid profile key.**
    `crates/core/extension/module/src/package/fork.rs:462` changes the root's
    module to the fork but retains the upstream instance key. Profile
    validation at `package/profile.rs:1184` requires the key to begin with
    the root module ID. Re-key the instance and dependent references while
    handling inherited-root deactivation; test an active inherited root through
    fork and profile activation. Confidence: high, source trace; new.
16. **Surface entrypoint indexing bypasses source-read bounds.**
    `crates/core/extension/module/src/package/installed_graph/contributions.rs:85`
    reads and parses the full entrypoint to derive props before the bounded
    authoring scan. Use a bounded, contained reader and emit an over-budget
    diagnostic; test oversized or growing entrypoints and unaffected modules.
    Confidence: confirmed source trace; related to the older source-budget work.
17. **Concurrent publication can accept a different generation snapshot.**
    `crates/core/extension/module/src/package/content_store.rs:177` checks an
    existing snapshot for equality before staging, but a losing rename race at
    line 222 accepts the winner without comparing contents. Recheck the winner
    and test simultaneous identical and different snapshots. Confidence: high
    for the public method's race; production reach depends on caller locking.
18. **Extension contribution documentation claims a VM per contribution.**
    `crates/core/extension/module/src/manifest/model.rs:760` says it runs in
    its own VM; the platform contract allows a shared VM with private
    environments. Verify the execution path and correct the public comment to
    describe the actual isolation and grants. Confidence: confirmed comment
    drift; it is not evidence that shared-VM execution violates the contract.
19. **Manifest normalization duplicates runtime declarations.**
    `crates/core/extension/module/src/package/module_manifest.rs:31,87,431,445`
    normalizes during parsing and again during runtime conversion, while list
    merges append entries. Shipped binary dependencies and extension-point
    contributions are duplicated; `crates/core/shell/src/shell/backend/candidates.rs:165`
    repeats binary checks. Make normalization idempotent and test parse-to-runtime
    parity. Confidence: confirmed source trace; rendering impact is not
    established and performance is unmeasured.
20. **Composition duplicates settings merge logic.**
    `crates/core/extension/module/src/package/composition.rs:398` duplicates
    `mesh_core_config::merge_json` with identical object recursion and value
    replacement. Share the canonical helper and test nested object, array,
    scalar, and null precedence. Confidence: confirmed source duplication;
    maintainability impact, not measured performance.
21. **Dependency closure can reactivate a trust-rejected module.**
    `crates/core/extension/module/src/package/installed_graph/graph.rs:550`
    initially disables modules that fail signature or trust policy, but passes
    every manifest into dependency resolution and resets each node's enabled
    state from the required closure at line 610. A trusted root can thereby
    reactivate a rejected dependency. Exclude trust-ineligible candidates or
    enforce trust throughout closure resolution and block dependents with a
    diagnostic. Test a trusted root requiring an unsigned module below the
    minimum and an invalid-signature module, plus a disabled but trusted
    dependency that should still be inferred. An isolated scratch executable
    reproduced an enabled Local dependency and a `trust_policy_blocked`
    diagnostic under a Verified minimum. Confidence: reproduced; add a durable
    regression to the module suite.
22. **Uninstall misses tracked-fork mappings.**
    `crates/core/extension/module/src/package/profile.rs:400` omits both keys
    and values of `dependency_forks` from reference detection and forced
    cleanup. `package/management.rs:218` relies on those helpers, while graph
    loading requires both modules in a mapping. Uninstall can leave a profile
    that fails to load. Include both sides in reference and cleanup policy;
    test upstream-only and fork-only mappings in active and inactive profiles.
    Forced cleanup also removes roots without removing node-slot maps keyed by
    those instances. Test that cleanup too. Confidence: high, source trace; new.
23. **Revision-checked profile writes can persist invalid profiles.**
    `crates/core/extension/module/src/package/profile.rs:1037` saves the
    candidate without calling `validate`, unlike `save_locked`. The CLI's
    `profile set` path at `crates/tools/cli/src/main.rs:910` accepts an
    arbitrary namespace and uses this writer, so it can report success for a
    document that later fails loading. Validate before publication; test
    invalid namespace, root identity and schema version while asserting the
    prior bytes and revision survive. Confidence: confirmed source trace; new.
24. **Profile prune leaves orphaned node-slot overrides.**
    `crates/core/extension/module/src/package/composition.rs:237` records
    slot orphans as `nodeSlots.<instance>`, but
    `crates/tools/cli/src/main.rs:968` removes only matching root and settings
    keys. The slot survives despite a successful prune message. Use structured
    orphan identities and remove the corresponding slot map; test root-only,
    slot-only and combined cases. Confidence: confirmed source trace; new.
25. **Binary dependency checks accept non-executable files.**
    `crates/core/extension/module/src/package/installed_graph/health.rs:169`
    checks only whether a path is a regular file, and backend activation uses
    that result at `crates/core/shell/src/shell/backend/candidates.rs:165`.
    A same-named non-executable file can be reported available although launch
    fails. Align with runtime executable resolution and test permissions and
    PATH ordering. `package/tests/manifest.rs:7` currently asserts a regular
    non-executable file is available and needs correction. Confidence: confirmed
    source trace; practical effect depends on the provider.
26. **Invalid extension-point host versions act as unrestricted.**
    `crates/core/extension/module/src/package/module_manifest.rs:610` validates
    host names and slots without validating the version requirement;
    `package/installed_graph/contributions.rs:524` rejects mismatch only if
    parsing succeeds. An invalid requirement therefore accepts contributions.
    Validate at load and fail closed during resolution; test invalid and
    incompatible ranges. Confidence: high, source trace; new.
27. **Single-contribution extension points still render all entries.**
    `crates/core/extension/module/src/package/installed_graph/contributions.rs:619`
    reports overfill for `multiple: false` but retains all entries; the shell
    renders the whole list in `crates/core/shell/src/shell/component/composition.rs:458`.
    Define and enforce deterministic rejection or selection and test both the
    graph result and rendered slot. Confidence: high, source trace; new.
28. **Automatic contribution instances can collide across modules.**
    `crates/core/shell/src/shell/component/composition.rs:454` uses the local
    contribution ID as placement identity; `component.rs:350` interns it
    with host and slot but not source module. Two contributors with the same
    local ID can therefore share a memo/runtime key. Include the source module
    in automatic identity and test two such contributors across rebuild and
    removal. Confidence: high, source trace; exact memo impact needs regression.
29. **Graph loading reads external contracts without a size bound.**
    `crates/core/extension/module/src/package/installed_graph/load.rs:750`
    reads the complete interface contract synchronously after checking its path.
    This bypasses source budgets and exposes graph loading to large files and
    path replacement races. Use a descriptor-relative bounded read and isolated
    diagnostics; test oversized, growing and symlink-swapped files. Confidence:
    confirmed source trace; performance impact unmeasured.
30. **Slot replacement retains the old contribution's identity and props.**
    `crates/core/extension/module/src/package/composition.rs:415` replaces
    only the source module ID. The shell then looks up the new module with the
    old local contribution ID in `shell/component/composition.rs:469`, which
    can yield an error placeholder; matching IDs still carry old metadata.
    Resolve replacement to the target's own declared contribution and validate
    its props; test different IDs, entries and missing targets. Confidence:
    high, source trace; new.
31. **Signed graph loading starts a verifier process per module.**
    `crates/core/extension/module/src/package/installed_graph/load.rs:331`
    assesses loaded modules serially, and `package/trust.rs:292` invokes
    OpenSSL using temporary files for each signed provenance. Measure release
    graph construction for 1/32/128 signed and unsigned modules, including
    process count, CPU, p50/p95 and warm/cold runs. If material, evaluate a
    vetted in-process verifier or a bounded cache keyed by the complete signed
    identity; test key rotation, trust changes and corrupted signatures.
    Confidence: confirmed source path; impact unmeasured and not a speedup claim.
32. **Profile commands cannot directly deactivate an inherited root.**
    `crates/tools/cli/src/main.rs:870` calls `set_instance_active` on the
    sparse profile; `package/profile.rs:380` only looks in its explicit roots.
    An inherited root without a delta therefore reports no instance, even
    though `docs/spec/01-module-system.md:900` permits deactivation. Removing
    an explicit override can also reveal the inherited root while reporting it
    removed. Resolve against the composition and write an appropriate sparse
    active override; distinguish reset from removal. Test inherited, overridden
    and added roots. Confidence: high, source trace; new.
33. **Blocked roots can leave otherwise unselected dependencies active.**
    `crates/core/extension/module/src/package/resolution.rs:76` removes blocked
    roots but adds every nonblocked member of the original required closure.
    An isolated graph reproduction with a root requiring one healthy and one
    missing dependency returned the healthy dependency enabled after blocking
    the root, despite its explicit root entry being disabled. Recompute
    reachability from viable roots and test shared and independent-root cases.
    Confidence: reproduced; add a durable graph regression.
34. **Store pointer writes follow a predictable temporary symlink.**
    `crates/core/extension/module/src/package/content_store.rs:637` checks
    the final target but opens a predictable PID-based sibling with create and
    truncate, following an existing symlink. An actor able to write the store
    directory could redirect the write; same-process concurrent callers also
    share the name. Use exclusive unique, no-follow temporary creation and
    preserve durability; test a planted symlink and concurrent writers.
    Confidence: confirmed source behavior; exploitability depends on directory
    permissions and caller locking.
35. **Package commit can report failure after durable success.**
    `crates/core/extension/module/src/package/transaction.rs:715` persists
    the committed journal and marks the transaction finished before returning
    a possible workspace-cleanup error. Callers receive an error even though
    the mutation has committed and a retry can conflict. Expose a typed
    committed-with-cleanup-needed outcome and recover cleanup; inject cleanup
    failures and assert durable state, recovery and caller-visible results.
    Confidence: high, source trace; related to existing post-commit recovery work.
36. **Malformed extension-point prop types are accepted.**
    `crates/core/extension/module/src/package/module_manifest.rs:1509` checks
    that each type is nonempty but does not parse it;
    `package/installed_graph/contributions.rs:659` accepts a parser error on
    the assumption another diagnostic exists. Validate declarations at load
    and fail closed in resolution; test malformed and valid composite and
    optional types. Confidence: high, source trace; new.
37. **One malformed detached signature aborts graph loading.**
    `crates/core/extension/module/src/package/installed_graph/load.rs:331`
    collects provenance assessments with `?`, while `package/trust.rs:267`
    returns errors for malformed or unreadable detached signatures. One such
    unlocked module can therefore abort the entire graph rather than being
    attributed and isolated. Convert failures to rejected per-module
    assessments and test an unrelated healthy module and a blocked dependent.
    Confidence: high, source trace; new.
38. **Interface blocking is not propagated through module dependencies.**
    `crates/core/extension/module/src/package/installed_graph/graph.rs:889`
    disables frontends with incompatible required interfaces after module closure was
    computed, without recalculating dependent blocking or reachability. A
    module requiring such a frontend can remain enabled, as can now-unneeded
    dependencies. Reconcile closure after interface decisions and test a
    dependent root, shared viable dependencies and unrelated roots, using an
    installed incompatible provider (a missing provider may degrade). Confidence:
    high, source trace; graph-level regression needed; related to finding 33.
39. **Generated service mocks can violate their contracts.**
    `crates/core/extension/service/src/generator.rs:181` selects defaults by
    base type without accounting for arrays, nested required fields or numeric
    constraints, and always returns `{ok=true}` from methods. Generated mocks
    can therefore fail typed state and return validation. Generate valid values
    where possible or report unsupported schemas, and execute generated mocks
    through Luau and compiled contract validation for arrays, nested records,
    ranges and method returns. Confidence: confirmed source trace; new and
    relevant to the proposed component test harness.
40. **Service members can collide with proxy namespaces.**
    `crates/core/extension/service/src/contract.rs:1188` reserves `supports`
    but accepts `state` and `events`; `runtime/scripting/src/context/proxy.rs:245`
    intercepts those names before direct member lookup. Valid declarations can
    thus be inaccessible through the direct proxy API. Reserve those names or
    redesign access, and test each member category through real Luau and
    generated declarations. Confidence: high, source trace; new.
41. **Required named-record additions can be classified additive.**
    `crates/core/extension/service/src/compatibility.rs:512` classifies every
    added named field as additive, while contract validation rejects a record
    missing a required field. Existing method callers sending the older record
    can therefore break after a supposedly compatible change. Account for input
    and output variance or classify required additions conservatively; test
    method input, state/output and optional field cases. Confidence: high,
    source trace; new.
42. **Nontrailing optional positional arguments weaken proxy arity checks.**
    The service compiler accepts an optional argument before a required one;
    `crates/core/runtime/scripting/src/context/proxy.rs:31` counts required
    arguments rather than finding the last required position. A single supplied
    argument can pass arity for `(optional, required)` without providing the
    latter. Reject nontrailing optionals or validate positions correctly; test
    omitted required and explicit nil with the real Luau proxy. Confidence:
    high, source trace; distinct from optional-argument removal.
43. **Service symbols can generate invalid Luau.**
    `crates/core/extension/service/src/contract.rs:916` checks symbol
    uniqueness but not Luau identifier syntax or keywords. The generator at
    `service/src/generator.rs:317` emits names directly, so punctuation or
    keywords can produce invalid declarations and mocks. Define supported names
    or escape keys and handle invalid function/parameter identifiers; parse or
    execute generated artifacts for these cases. Confidence: high, source trace;
    new and relevant to authoring tests.
44. **Module theme rule provenance uses the wrong indices after merging.**
    `crates/core/foundation/theme/src/lib.rs:1503` appends rules but records
    incoming zero-based indices without the target's prior length. Later layer
    provenance can overwrite earlier entries and omit appended indices. Offset
    by the existing rule count and test overlapping module and theme layers.
    Confidence: high, source trace; cascade order itself is unaffected.
45. **Theme token alias resolution has no depth or work bound.**
    `crates/core/foundation/theme/src/lib.rs:1183` recursively follows aliases
    and fallbacks with cycle detection but no depth limit; stack membership is
    a linear scan. Long acyclic chains can consume deep native stack and
    quadratic checks. Bound or iteratively resolve dependencies with a clear
    diagnostic; test long chains and nested fallbacks, and benchmark release
    depths 10/100/1,000. Confidence: source-confirmed complexity; practical
    threshold and impact unmeasured.
46. **Global and module theme rules duplicate lowering logic.**
    `crates/core/foundation/theme/src/lib.rs:1887` and `:1930` separately
    lower root tokens, tag defaults and selector declarations with identical
    logic. Share the lowering step while retaining their distinct at-rule
    policies; test selector order, duplicate declarations and unsupported
    nested rules. Confidence: confirmed duplication; maintainability impact.
47. **Theme detail pages repeatedly serialize the full snapshot.**
    `crates/core/foundation/theme/src/transport.rs:14` serializes and hashes
    the whole document for each page, then serializes it again for the chunk.
    Measure 24 KiB/256 KiB/2 MiB snapshots with 1/16 consumers, recording
    serialized bytes, allocations and p95; preserve stale revision and UTF-8
    behavior. Consider bounded revision-owned encoded details only if measured.
    Confidence: confirmed repeated work; practical impact unmeasured.
48. **Dotted module IDs are split incorrectly in scoped theme tokens.**
    `crates/core/foundation/theme/src/lib.rs:2106` splits the module/token
    name at the first dot, although valid publisher and module IDs can contain
    dots. Overrides and non-flattened scoped lookup can therefore use the wrong
    prefix; exact flattened lookup can still succeed. Use an unambiguous boundary and test dotted publisher and module
    names, CSS spelling and override round trips. Confidence: high, source trace.
49. **Strict container comparisons are lowered as inclusive.**
    `crates/core/foundation/theme/src/css.rs:873` maps `>` and `>=` to the
    same bound, and `ui/component/src/style.rs:25` evaluates bounds
    inclusively. A strict query therefore matches exactly at its threshold.
    Preserve strictness or reject that syntax; test below, equal and above width
    and height thresholds, including interval combinations. Confidence: high,
    source trace; new cross-section finding.
50. **Core translation APIs disagree on English fallback.**
    `crates/core/foundation/locale/src/lib.rs:606` and `:767` traverse only
    the selected chain, while `resolve_in_core` at :680 appends English as
    a last resort. The public point, argument and bulk APIs can disagree when
    the chain excludes English. Share precedence and test all three; no
    repository production caller of this core translator was found.
    Confidence: confirmed source trace; live user impact not established.
51. **Date and time formatters are constructed per call.**
    `crates/core/foundation/locale/src/lib.rs:1208` constructs ICU formatters
    each time; shipped clock components call these through Luau. Benchmark
    release warm/cold 1/60/1,000 calls for `en`, `sk`, `fr`, multiple styles,
    plus 60 script-to-render clock ticks. Record allocations and p50/p95;
    compare output, timezone and locale-change behavior before adopting a
    bounded cache. Confidence: confirmed repeated construction, impact unmeasured.
52. **Unmatched closing braces can pass catalog validation.**
    `crates/core/foundation/locale/src/lib.rs:994` checks for `}` only in
    the remainder after processing placeholders. A closing brace before a later
    `{name}` is skipped, so malformed messages can be accepted. Check each
    scanned prefix and test unmatched braces before, between and after valid
    placeholders, including Unicode. Confidence: confirmed source trace.
53. **Icon coverage advice and live resolution use different fallbacks.**
    `crates/core/foundation/resources/src/coverage.rs:447` tests exact and
    dash-parent names, while `crates/core/ui/icon/src/fallback.rs:21` also
    tests canonical aliases. A pack mapping `network-wireless` can resolve
    `wifi` at runtime while coverage advice reports a gap. Share fallback
    vocabulary in a dependency-neutral owner; test aliases, dash parents and
    ordered chains against the live registry. The advisor also compares bare
    names against owner-scoped mappings encoded as `owner:name`
    (`shell/discovery.rs:254`), so it can miss a valid scoped mapping. Preserve
    owner identity and test two owners plus global mappings. Confidence:
    confirmed source trace.
54. **Duplicate JSON glyph names escape validation.**
    `crates/core/ui/icon/src/xdg.rs:123` first parses the map into a JSON
    value, which collapses duplicate keys before the later duplicate check.
    Text-form maps reject duplicates, while JSON silently retains one value.
    Use a duplicate-detecting map visitor and test duplicate and escaped-
    equivalent keys. Confidence: confirmed source trace.
55. **Icon overrides do not resolve MESH pack-qualified targets.**
    `crates/core/ui/icon/src/registry.rs:710` sends user and author overrides
    to `try_target` without an owning pack, so it tries file/XDG targets but
    does not look up a MESH mapping pack or its font aliases. A documented
    `pack/name` override can fall through instead of pinning the mapping.
    Resolve qualified overrides through the specified pack and test precedence,
    font glyphs and prepared asynchronous lookups. Confidence: high, source trace.
56. **An absent XDG data home can put system resources ahead of legacy user paths.**
    `crates/core/foundation/resources/src/lib.rs:789` filters nonexistent data
    paths, then treats the first surviving directory as data home when ordering
    `~/.icons` and `~/.fonts`. If the real data home is absent, a system
    directory can be inserted first. Preserve the actual data-home identity and
    test duplicate user/system resources with an absent data home. Confidence:
    high, source trace; verify expected precedence across supported hosts.
57. **Override-only font roles are omitted from renderer aliases.**
    `crates/core/foundation/resources/src/font.rs:443` collects standard and
    pack-defined roles but omits user/author override keys. Direct resolution
    honors an override, while renderer alias projection can omit a custom role
    present only in that override. Include override keys and test `font-family`
    with an override-only role, precedence and revision changes. Confidence:
    high, source trace; real text-render regression needed.
58. **Template braces inside XML comments are parsed as live syntax.**
    `crates/core/ui/component/src/parser/markup.rs:218` lexes braces before
    XML comments are discarded at :429. An unmatched brace or directive in a
    comment can reject or alter an otherwise valid template; CDATA is likewise
    treated as literal only after lexing. Make brace lexing comment/CDATA-aware
    and test both, quoted attributes, spans and editor
    recovery. Confidence: high, source trace.
59. **Standalone expression validation ignores lexical scope.**
    `crates/core/ui/component/src/parser/script.rs:113` collects expression
    names without scopes, while `mesh-core-expression` already computes scoped
    references; `frontend/compiler/src/compile.rs:1178` uses the former for
    standalone validation. Inline function parameters can be reported as
    unknown. Share the scoped metadata and test locals, function parameters,
    loops and real unknown globals. Confidence: high, source trace.
60. **Style directives evaluate their expression multiple times.**
    `crates/core/ui/component/src/parser/markup.rs:790` emits the expression
    up to three times while checking nil/false and producing the value. A
    stateful call can run repeatedly and produce a different checked value
    from the emitted one. Evaluate once and test a counter plus nil, false and
    string results through real Luau and rendered style. Confidence: confirmed
    source trace.
61. **Large integral prop values change during CSS projection.**
    `crates/core/ui/component/src/lib.rs:491` and `:784` cast integral `f64`
    values to `i64` when formatting, while validation accepts finite values
    outside that range. Saturation can make CSS differ from script/settings
    values. Define numeric bounds or use faithful finite-float formatting; test
    large positive and negative values through JSON/Luau and CSS projection.
    Confidence: confirmed source trace.
62. **Ordinary Luau line comments beginning `--[` are rejected.**
    `crates/core/ui/component/src/parser/brace.rs:337` assumes any `--[` is
    a long comment, although only a valid long-bracket opener begins one.
    Recognize a valid opener before selecting long-comment scanning; test line
    comments, valid long comments and unterminated valid openers. Confidence:
    high, source trace.
63. **Component import analysis does not decode Luau string escapes.**
    `crates/core/ui/component/src/parser/script.rs:590` returns raw token text
    for import arguments, so an escaped path can differ from the string Luau
    actually evaluates. Reuse the literal decoder and test escaped and
    long-bracket imports through compilation and runtime. Confidence: high,
    source trace; this is not a path-escape claim.
64. **Authored marker-like attribute values can become bindings.**
    `crates/core/ui/component/src/parser/markup.rs:672` recognizes a
    generated expression marker by its text alone. A literal attribute equal
    to a marker for another expression can silently become that binding.
    Track generated markers by source span or escape authored literals; test
    marker-shaped values before and after expressions and out-of-range values.
    Confidence: high, source trace.

An isolated real-parser executable reproduced findings 58–64: comment/CDATA
and `--[` line-comment rejection, repeated directive lowering, unscoped function
parameter references, a `1e20` prop projected as `9223372036854775807`,
escaped-import classification and a literal marker becoming a binding. Full
compiler behavior for the unscoped helper still needs a regression. These
reproductions do not replace durable tests in component and frontend suites.

## Component testing and profiling opportunities

- **Reusable component scenarios.** The realistic fixtures in
  `crates/core/shell/src/shell/component/tests/common.rs:711` are private Rust
  helpers, while `docs/testing/overview.md:37` directs authors to those helpers.
  After SH-08, expose a supported harness using the actual compiler and runtime
  with deterministic providers, settings, theme, locale, viewport, scale and
  controllable time. Drive input, state, props and reload, and assert semantics,
  typed requests, diagnostics, geometry and optional pixels. Pilot audio
  popover state/action/failure, Orbit layout and offscreen focus. Keep real
  Wayland and GPU checks separate. This is a design proposal; scope and API
  need validation against the runtime extraction.
- **Repeatable component profiles.** Existing tree-build attribution in
  `crates/core/shell/src/shell/component/composition.rs:28` and per-surface
  timing/visit instrumentation provide a base, but current workload gates are
  hand-authored. Select an instance and scripted scenario, record warmup and
  samples with per-stage distributions, node visits and correctness assertions,
  and compare against profiling disabled to quantify overhead. Initial workloads:
  mount, no-op, state/text update, hover, animation and 16/1,024-item lists.
  Define direct and inclusive costs carefully and retain shared/unattributed
  ancestor work; surface totals cannot simply be divided among components.
- **Correlated shell measurements.** Existing session reporting groups events by
  time, and documented surface totals exclude asynchronous GPU and compositor
  latency. Add optional bounded correlation from input or service update through
  affected surfaces, GPU completion and commit, and compositor presentation
  feedback where available; distinguish unsupported endpoints and frame callback
  from display. Test coalescing, concurrent surfaces, out-of-order completions and
  fallback. This overlaps the existing S12 metrics and S15 scheduling work.
- **Comparable run records.** Record scenario version, build and revision,
  renderer, machine, viewport/scale, warmup, sample count and timing boundaries;
  preserve raw results and interleave before/after runs. Extend existing
  `tools/profile-shell` and `tools/profile-report`, rather than replacing D9 or
  GPU gates. Verify missing samples, metadata and parser determinism. No timing
  improvement is claimed without measurements.
- **Startup attribution.** `package/tests/shipped.rs:455` measures aggregate
  Luau scanning, not whole-shell startup. Add opt-in discovery, read, parse,
  signature, closure and contribution-index phase counts to a release workload
  varying module, dependency and signed-module counts; report p50/p95 and
  process/thread counts with diagnostic parity. Coordinate with existing S15
  profile/catalog benchmarks and signature-verification investigation.
- **Theme scenario.** In the component harness, mount two instances sharing a
  token family, switch theme/mode and a scoped token, then submit malformed
  stylesheet content and check last-known-good style, events and pixels against
  a full rebuild. Profile parsing/composition, affected restyle, layout and
  damaged pixels at 100/1,000/10,000 nodes. The existing theme-switch D9 gate
  excludes theme file loading and durable settings writes.
- **Resource scenario.** Mount components with 16/256/1,024 semantic icons and
  two module-specific chains; exercise cold worker completion, warm repaint,
  misses, pack switch/removal and stale completion. Assert owner-scoped repaint,
  pixel parity and generation safety. Measure lookup/cache, worker queue and
  bytes separately from warm frame p50/p95, visits and damage; compare profiling
  overhead against an uninstrumented run.
- **Authoring corpus.** Run valid and mid-edit component sources through runtime
  and tooling parsers, compilation and real Luau; check Unicode/CRLF spans,
  imports, props, event expressions and rebuild parity. Profile top-level scan,
  script AST/recovery lexing, markup lowering, expression worker/cache and
  style/semantic validation separately for 1/10/100 KiB files, 1/32/256 handlers
  and 1/10/100 import graphs. Preserve diagnostics and avoid per-node clocks in
  timing runs.
- **Graph-load profiling candidates.** Authoring diagnostics recompute visible
  token sets for each source file (`installed_graph/diagnostics.rs:688`), and
  backend event scans can repeat parsing for each provider interface. Measure
  release runs across 1/10/100 files, 0/10/100 tokens, and 1/4/16 interfaces;
  count parses, allocations and threads and verify identical diagnostics.
  Dependency closure can repeatedly scan edges on adverse chains
  (`package/resolution.rs:301`); measure 32/128/512/2,048 modules and wide
  graphs with blocked-set parity. Fork diffs and content hashing read complete
  files (`package/fork.rs:197`, `package/lock.rs:588`); measure peak RSS and
  p95 across 1/32/256 MiB assets before selecting streaming or bounds. These are
  unmeasured hypotheses, related in part to existing S02 graph and package work.

The theme and component URL scanners also duplicate a first-closing-parenthesis
scan, potentially mishandling quoted filenames containing `)`; investigate
alongside finding 3. This is a maintainability and correctness hypothesis, not
a measured performance finding.

## Coverage and next steps

The sample included portions of foundation files, themes, locale and surface
policy; package forks and service compatibility; command execution; focus,
animation, component parsing, compiler, accessibility, rendering and Wayland
event handling. Sections 01–07 are complete. Most other
files, remaining section-level reviews and cross-section lifecycle traces remain
outstanding. The inventory is in [00-coverage.md](00-coverage.md). Resume the
procedure in `prompts.md`, verify
the findings with regressions, and reconcile further discoveries with the
existing backlog and rejected performance experiments.
