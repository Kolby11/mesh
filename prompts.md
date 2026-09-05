# Refactor

Audit the entire MESH codebase using the 16 section identities in
`.planning/log/sections.md`. That map is historical evidence; refresh package
coverage from the workspace and use `docs/crate-boundaries.md` for current
dependency direction.

The audit is read-only: inspect and report findings, but do not implement fixes.

Review every section through three layers:

1. Performance improvements
2. Dead code, duplication, and redundancy
3. Logic, correctness, and improvements to core mechanics based on MESH’s principles

Use subagents, process the codebase section by section, and write the findings immediately after
completing each section.

## Read first

Before starting, read:

- `AGENTS.md`
- `docs/architecture/overview.md`
- `docs/spec/README.md`
- `docs/spec/00-philosophy.md`
- `docs/crate-boundaries.md`
- relevant files under `docs/spec/`
- `.planning/log/sections.md`
- `.planning/STATUS.md`
- `docs/BACKLOG.md`
- `.planning/README.md`
- `.planning/log/performance-log.md`
- existing `.planning/log/sections/*/improvements.md`

Rules:

- `docs/spec/` is the authoritative contract.
- `docs/spec/00-philosophy.md` owns philosophy and vocabulary. Detailed specs
  own schemas and shipped/target status; this prompt does not redefine them.
- Do not report missing **Target** behavior as a current bug.
- Distinguish accepted targets from undecided options, including a possible
  TypeScript/JavaScript runtime. An undecided option is not implementation debt.
- Check the rejected-experiments table before suggesting performance work.
- Do not repeat rejected experiments without new evidence.
- Existing section reports are historical evidence. Do not overwrite them.

## Output location

Create:

`.planning/codebase/audits/YYYY-MM-DD-whole-codebase/`

Use the actual audit date.

Write:

```text
00-coverage.md
sections/
  01-core-foundation-contracts.md
  02-module-system-and-installation.md
  03-service-contracts.md
  04-themes.md
  05-localization-i18n.md
  06-host-resources-and-icon-packs.md
  07-component-language.md
  08-ui-element-core.md
  09-interaction-and-motion.md
  10-frontend-compiler-and-host.md
  11-luau-runtime-and-sandbox.md
  12-rendering-and-paint.md
  13-surface-policy-and-configuration.md
  14-wayland-platform-and-presentation.md
  15-shell-core-and-orchestration.md
  16-developer-and-authoring-tools.md
cross-section-findings.md
FINAL.md
```

Use apply_patch for repository writes.

## Backlog handling

At the beginning, add one short item to docs/BACKLOG.md saying that the whole-codebase audit is in
progress.

When the audit is finished:

- remove that temporary item;
- add genuinely new open findings to the appropriate existing backlog sections;
- keep each backlog entry to one to three lines;
- do not add priority labels;
- do not duplicate existing backlog items;
- link new items to the relevant audit report;
- append a short completion record to the current monthly log.

Do not turn the backlog into the audit report. Detailed evidence belongs in the audit files.

## Coverage

Before reviewing sections, build an inventory of the codebase using rg --files and Cargo workspace
metadata.

Include:

- Rust source;
- tests and benchmarks;
- Cargo manifests and build scripts;
- .mesh files;
- Luau files;
- module manifests;
- interfaces;
- shipped configuration;
- CLI and LSP code;
- relevant tools and fixtures.

Exclude:

- build output;
- .git;
- vendored dependencies;
- binary assets;
Assign every included file to one of the 16 sections from .planning/log/sections.md.

Write the assignment to:

.planning/codebase/audits/YYYY-MM-DD-whole-codebase/00-coverage.md

For each section, track:

- assigned files;
- inspected files;
- excluded files and why;
- files that still need review.

Do not claim that the entire codebase was reviewed while files remain unaccounted for.

## Subagents

For each section, spawn three fresh gpt-5.6-luna subagents with xhigh reasoning:

- Performance agent
- Dead-code and redundancy agent
- Logic and core-mechanics agent

Run them in parallel when possible.

Each agent must inspect the section’s assigned files, relevant tests, callers, consumers, modules,
and neighboring package seams.

The main agent coordinates the work, verifies important findings, combines duplicates, and writes
the section reports.

## Performance agent

Look for:

- unnecessary allocations, clones, hashing, parsing, locking, copying, and conversions;
- repeated traversal or recomputation;
- overly broad rebuilds, invalidation, layout, repaint, or damage;
- blocking I/O or process spawning on hot paths;
- unbounded queues, caches, recursion, or per-frame work;
- missed batching, caching, incremental processing, or concurrency;
- expensive startup, idle, interaction, reload, and shutdown paths.

Requirements:

- Trace realistic end-to-end execution paths.
- Check existing benchmarks and performance history first.
- Separate measured problems from hypotheses.
- Do not claim a speedup without measurements.
- For each suggestion, describe the workload and benchmark needed to verify it.

## Dead-code and redundancy agent

Look for:

- unused functions, types, fields, variants, modules, and feature branches;
- public APIs without real consumers;
- stale migration or compatibility code;
- duplicated parsing, validation, resolution, state, schemas, caches, and lifecycle logic;
- the same policy implemented in core, shell, CLI, LSP, or modules;
- wrappers that add no useful boundary;
- stale comments, tests, or fixtures.

Requirements:

- Search repository-wide before calling something dead.
- Account for traits, callbacks, serialization, manifests, Luau, components, and dynamic
  registration.

- Distinguish confirmed dead code from possibly unused code.
- Identify the correct canonical owner for duplicated logic.
- Describe the risk and tests needed before removal.

## Logic and core-mechanics agent

First build a simple process tree for the section:

- inputs;
- validation;
- state ownership;
- main calls;
- state transitions;
- errors;
- commit and rollback;
- recovery;
- cleanup.

Then inspect for:

- logic errors;
- inconsistent sources of truth;
- partial or non-atomic updates;
- stale generations, revisions, callbacks, caches, or events;
- incomplete cleanup;
- lifecycle and cancellation problems;
- missing validation;
- package-boundary violations;
- incorrect capability enforcement;
- better ways to structure the core mechanics.

Apply `docs/spec/00-philosophy.md`, especially its ownership test (§2), element
admission test (§4), and review guidance (§8). Trace each finding to the relevant
detailed contract and implementation status. In particular, do not classify a
core-owned settings/storage or management transaction as misplaced domain logic,
or shared-VM execution as a violation by itself. Inspect private environments,
explicit communication, authorization, resource bounds, and failure handling
individually. Use `docs/crate-boundaries.md` for dependency direction.

The agent may suggest a better architecture instead of preserving the current flow, but it must
explain the migration cost.

## Findings format

Each finding should include:

- ID such as S03-PERF-01, S03-DEAD-01, or S03-LOGIC-01;
- title;
- source files and relevant symbols or line numbers;
- current behavior;
- why it matters;
- recommended improvement;
- test or benchmark needed;
- confidence: confirmed, high, medium, or speculative;
- whether it is new, already in the backlog, already in an older audit, or related to a rejected
  experiment.

Do not report style preferences as findings.

## Write after every section

After the three agents finish a section, the main agent must:

1. Check that every assigned file was reviewed.
2. Send follow-up work for anything missed.
3. Combine duplicate findings.
4. Verify important findings directly in the source.
5. Compare findings with the backlog and older audits.
6. Write the section report immediately.
7. Reopen and verify the report.
8. Update 00-coverage.md.
9. Only then continue to the next section.

Each section report should contain:

# Section NN — Name

## Scope and coverage

## Process tree

## Performance findings

## Dead code and redundancy

## Logic and core mechanics

## Existing backlog or audit overlap

## Refuted suspicions

## Tests and benchmarks needed

## File coverage

Do not wait until the end to write section findings. A completed section must be saved before
starting the next one.

## Cross-section review

After all 16 section reports are written, spawn three fresh subagents for:

- cross-section performance;
- duplicated ownership and redundancy;
- cross-section logic and lifecycle.

They must inspect source directly and trace at least:

- manifests and profiles through graph resolution and activation;
- service interfaces through provider selection and frontend consumption;
- .mesh parsing through compilation, Luau, and element-tree creation;
- elements and layout through rendering, damage, and presentation;
- Wayland input through handlers, state updates, and repaint;
- settings, theme, locale, and resources through revision propagation;
- package operations through durable state and live activation;
- reload, recovery, and shutdown;
- CLI and LSP validation through canonical core contracts.

Write their combined findings to:

.planning/codebase/audits/YYYY-MM-DD-whole-codebase/cross-section-findings.md

## Final report

Write the complete synthesis to:

.planning/codebase/audits/YYYY-MM-DD-whole-codebase/FINAL.md

Include:

- overall summary;
- coverage table for all 16 sections;
- most important correctness findings;
- performance findings;
- dead-code and redundancy findings;
- core-mechanics and architecture findings;
- cross-section findings;
- suggested execution order;
- tests and benchmarks needed;
- links to every section report.

Clearly distinguish:

- new findings;
- existing backlog work;
- findings already covered by older audits;
- performance hypotheses;
- rejected or refuted ideas.

## Final checks

Before finishing:

- verify all 16 section reports exist;
- verify every section contains all three review layers;
- verify every in-scope file is accounted for;
- verify the unassigned-file count is zero;
- verify important findings have exact source evidence;
- verify performance suggestions include a workload and measurement plan;
- verify links between reports work;
- reconcile new findings with the backlog without priority labels;
- run git diff --check;
- list every changed file in the final response.

Do not implement any fixes. Continue autonomously until all sections, cross-section review, final
report, backlog reconciliation, and coverage checks are complete.
