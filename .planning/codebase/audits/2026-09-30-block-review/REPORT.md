# 2026-09-30 block review

Read-only review of the whole workspace in seven architectural blocks, run
against [00 — Platform Philosophy](../../../../docs/spec/00-philosophy.md), the
[crate boundaries](../../../../docs/crate-boundaries.md), the backlog, the
rejected-experiments table, and the
[2026-09-01 audit](../2026-09-01-whole-codebase/FINAL.md). Findings already in
the backlog or in the 2026-09-01 audit are not repeated unless new evidence was
found; that is noted per item.

**Status of this report:** evidence, not open work. Open items are tracked,
by implementation priority, in the "2026-09-30 block review and
module-architecture direction" section of `docs/BACKLOG.md`. No source files
were changed by the review. Performance items are
hypotheses with a named workload; none were measured.

**Confidence key:** *confirmed* = reproduced by a probe or re-read in source by
the coordinator; *high* = traced in source by the reviewing agent; *medium* =
plausible, depends on runtime conditions.

| Block | IDs | Findings |
| --- | --- | --- |
| Runtime: Luau scripting, sandbox, backends, service contracts | RT-01..11 | 11 |
| Module system, profiles, settings, capabilities, packages | MOD-01..12 | 12 |
| Component language and frontend compiler/host | CMP-01..13 | 13 |
| UI elements, style, animation, interaction | UI-01..12 | 12 |
| Rendering, presentation, Wayland, surface policy | RND-01..09 | 9 |
| Shell core (`mesh-core-shell`) | SH-01..13 | 13 |
| Themes, locale, resources, CLI/LSP, shipped modules | RES-01..15 | 15 |

---

## 1. Fix first — correctness and isolation

These break a stated platform guarantee (isolation, capability grants,
bounded failure) or silently show the user wrong output.

### Capability and isolation holes

- **RT-01 — `getfenv` escapes component isolation (confirmed).** `getfenv`
  and `setfenv` remain in the shared realm; the template-expression wrapper
  depends on them (`runtime/scripting/src/context/runtime/vm.rs:33,53`).
  Holding any function from another component (`bind:this`, a callback) lets
  `getfenv(fn).mesh` reach its host API, capabilities and storage; a scratch
  probe against mlua/Luau returned the other side's secrets in three shapes.
  Breaks 00 §7 ("does not expose its private environment or transfer its
  capabilities"). Fix: remove `getfenv`/`setfenv` from realm globals, bind
  expression environments from Rust (`Chunk::set_environment`), deep-wrap
  live-binding values, hide `__subscribers`.
- **MOD-01 — interface contracts are claimed by priority (confirmed).**
  `installed_graph/graph.rs:736` replaces an inline interface declaration when
  another backend declares a higher `provider_priority`. A third-party backend
  declaring `mesh.wm` at priority 101 rewrites which capability gates each
  method and the privilege level of `service.wm.*`. Bind each interface name
  to one owner; treat conflicting declarations as an error.
- **MOD-02 — `Core` trust tier comes from the `@mesh/` name (confirmed).**
  `package/trust.rs:32-48` returns `Core` for any `@mesh/*` id, even from git.
  00 §2: "never a module-name check". Derive Core from provenance; reject
  `@mesh/*` from git/path sources.
- **MOD-03 — approvals survive uninstall.** Normal uninstall only removes the
  module entry (`shell/package.rs:305-313`, `cli/main.rs:1535-1539`);
  `capability_approvals` is dropped only with `--force`. A different module
  installed later under the same id inherits the old approvals, including
  optional `exec.command`/`net.socket`. Drop approvals on uninstall, or bind
  them to source identity.
- **MOD-04 — `exec.argv` globs match inside arguments.** `glob_match`
  (`runtime/scripting/src/backend/exec.rs:132-158`) treats `*` inside an
  argument as a substring wildcard; shipped approvals such as
  `sh -c "test -S * && echo ok"` and the Hyprland `dispatch` Lua string
  therefore allow arbitrary command/Lua injection while install review
  presents them as narrow grants. Reject in-argument globs for interpreters
  and code arguments in `CapabilityCatalog::validate`.
- **SH-01 — profile switch keeps the old capability grants (confirmed).**
  `ActivationPlan.effective_capabilities` is computed (`shell/profile.rs:1531,
  1731`) but `commit_pending_profile_switch` never assigns it to
  `self.effective_capabilities`. A provider that exists only in the new
  profile fails its first supervised restart with `missing_capability`;
  revoked approvals persist for provider switches.

### Shell-killing and self-blocking failure paths

- **SH-02 — control-plane service calls say "applied", then can kill the
  shell.** Inside the effect scheduler, `CoreRequest`s from `ServiceCall`s are
  queued as follow-ups and immediately answered `{"ok":true,"status":"applied"}`
  (`runtime/request.rs:2039-2054`). If the follow-up (`SetTheme`, `SetLocale`,
  `SetModuleProp`, …) then fails — e.g. a second durable write while one is
  pending (`runtime/theme.rs:231-236`) — the `Err` reaches `run()` via
  `self.process_effects()?` and the shell exits. Two quick theme clicks can
  trigger it.
- **SH-03 — the effect scheduler can quarantine the shell itself.** Every
  non-surface request (theme, locale, profile switch, clipboard, diagnostics,
  `Shutdown`) is charged to one `@mesh/shell` source
  (`runtime/request.rs:438-474`). Budget violations never decay; after three
  over-budget frames in a session, core's own effects are dropped. Carry the
  originating module on each request, never quarantine core, decay counts,
  prune `blocked_causal_chains`.
- **SH-12 — profile replacement discards unmount effects (confirmed).**
  `remove_profile_component` (`shell/profile.rs:2283`) ignores the
  `Ok(Vec<CoreRequest>)` returned by `unmount()`. Removal is also implemented
  three times with different cleanup lists; none purge scheduler queues for the
  removed surface. One `retire_component` path.
- **RT-03 — one stream-callback error restarts the provider; three
  quarantine it.** The backend sends a terminal `Failed{stage:"stream"}` but
  keeps running; the shell treats it as terminal
  (`shell/backend/lifecycle.rs:1197-1211`). Poll errors use a non-terminal
  three-strike rule. One malformed `pw-mon` line should not kill audio.
- **RT-05 — a full event queue looks like a dead receiver.** `try_send` on the
  256-slot bounded channel (`backend/src/lib.rs:333-340`) breaks the provider
  loop on `Full`; the terminal `Stopped` is also `try_send` and can be lost, so
  the shell keeps a dead slot marked active.
- **RT-02 — storage writes permanently consume the 8 MiB aggregate budget.**
  Each `self.storage` write charges the whole serialized document and nothing
  releases it (`backend/runtime.rs:1441-1458`, `storage.rs:917-921`); after
  ~800 writes to a 10 KiB document every exec/event/output reservation fails.
  Dormant only because no shipped backend uses storage yet.

### Silent wrong output

- **CMP-01 — templates calling helpers don't re-render (reproduced).** Read
  tracking records only names looked up directly by the expression
  (`vm.rs:46-53`); `{label()}` records `label`, not `count`, so
  `dirty_state_affects_template()` returns false and the shell skips the
  rebuild. `mesh-syntax.md` documents calling functions as supported.
- **UI-01 — descendant selectors silently become compounds (confirmed).**
  `parse_selector` loops on `cssparser::Parser::next()`, which skips
  whitespace, so the `Token::WhiteSpace` rejection
  (`foundation/theme/src/css.rs:69`) is unreachable. `.a:hover .b` matches one
  element with both classes. Dead shipped rules:
  `navigation-bar/.../volume-button.mesh:298,303`,
  `debug-inspector/.../view-tabs.mesh:76`. Use `next_including_whitespace()`,
  then fix those modules.
- **UI-02 — `var()` is resolved when its declaration is applied, not after the
  cascade.** `.btn{background:var(--bg)} .btn:hover{--bg:red}` never changes
  the node's own background; children do see `red`. Resolve custom properties
  in a first pass per node.
- **UI-03 / UI-04 — keyframe animations sample from the wrong base and
  interpolate whole snapshots.** Animation-only frames reuse last frame's
  `computed_style` as the keyframe base (`component/animation.rs:520-545`), so
  partial-stop properties drift and `fill-mode:none` never restores the
  authored value. `sample_rule` holds the first stop when there is no `0%` and
  interpolates omitted properties through the base. Keep an authored snapshot;
  lower keyframes per property.
- **UI-05 — any named `animation` disables every `transition` on the node**
  (`component/animation.rs:318-330`).
- **UI-06 — hit testing ignores z-index; paint honors it.** Paint sorts
  children by `z_index` (`render/src/display_list/build.rs:823-847`);
  interaction walks `children.iter().rev()`. Share one child-order helper.
- **UI-07 — tab-order comparator is not a total order.** `nodes_share_row` is
  an overlap test (`interaction/src/focus.rs:185-205`); three mixed-height
  controls form a cycle. Order is unstable and newer `sort_by` may panic.
- **UI-08 — `readonly` inputs are editable.** No editing path reads
  `readonly` (keyboard, IME commit, delete-surrounding). 00 §4: `input` owns
  value semantics.
- **CMP-03 — script prop overrides can be dropped.**
  `merge_reloaded_props` decides ownership by value equality, so a script
  override that once coincided with the host value is later overwritten
  (violates 03 §4 layer 5). Track script-assigned props explicitly.
- **RND-01 — overlapping damage regions paint translucent content twice.**
  Regions are cleared once and then replayed per region with no clear between
  passes (`surface/painter/tree.rs:301-342`); overlaps arise from fractional
  rounding and blur expansion. Visible as darker seams on translucent bars.
  The existing regression test compares against the same defect.
- **RND-06 — spanning bars ignore compositor width when margins are set.**
  `configured_width.max(output_width)` (`backend/config.rs:621-633`) paints a
  margin-8 top bar at full output width; use output size only when the
  configure is unspecified.
- **RND-07 — buffer scale vs buffer size when fractional-scale is bound
  without viewporter** can produce a size not divisible by the scale (fatal
  protocol error). Rare combination, nothing guards it.
- **RES-01 — one bad user token override breaks the whole theme and can block
  graph commit.** The dash spelling shown in spec 04 §5, or an override for an
  uninstalled module, returns `Err` from `compose_layers`, propagated through
  `commit_installed_module_graph` (`discovery.rs:2435`). Make overrides
  per-token diagnostics.
- **RES-02 — `var(--a) var(--b)` is misparsed as one alias; nested fallbacks
  stay literal** (`theme/src/lib.rs:1315-1375`).
- **RES-03 — `format_date` is wrong for cs/sk/ru and always UTC.** Czech gets
  Slovak names, Slovak lacks the genitive, Russian falls back to English.
  Use ICU and the local zone.
- **RES-07 — backends parse locale-dependent tool output.** `mesh.exec`
  inherits the shell locale; verified `LC_ALL=sk_SK upower -i` prints
  `63,14 Wh`, which the upower provider reads as 14. Default exec children to
  `LC_ALL=C.UTF-8`.
- **RES-14 — hard-coded `en` beats a module's `defaultLocale`**
  (`locale/src/lib.rs:2215-2232`), contrary to 07 §5.
- **MOD-05 — package rollback can erase concurrent profile writes.** Package
  transactions back up and restore `profiles/` and `active-profile` holding
  only `.mesh-package.lock`, not the control-plane lock
  (`transaction.rs:626-645,938-960`); an abort rewinds a profile commit and its
  revision, defeating the revision CAS.
- **MOD-06 — provider compatibility is decided twice with different rules**
  (`profile.rs:140-166` vs `graph.rs:258-280`), so the activation closure can
  pull in a provider the graph then marks ineligible.
- **MOD-07 / MOD-08 — profiles cannot subtract composition services/providers;
  force-uninstalling a composition silently converts dependent profiles to
  hand-built** and drops all inherited roots.
- **MOD-12 — a retained last-valid manifest is matched by directory only** and
  can pin old capabilities/entrypoints onto a changed source tree
  (medium; landed in `98efc62e` today).

---

## 2. Mechanics changes — redesigns worth planning

Each needs a design note in `.planning/todos/pending/` before it is scheduled.

| ID | Change | Why | Cost |
| --- | --- | --- | --- |
| RT-06 | Per-context resource accounting on the shared frontend VM: per-context budget counters, heap attribution by `used_memory()` deltas per callback, thread cap as backstop | One component's leak fails unrelated components' allocations/publishes; 00 §7 separates isolation from accounting | Moderate; `realm_policy` is already per context |
| RT-07 | Drop the per-command JSON "rollback" of `state`; publish snapshots only from successful callbacks | Rollback is partial (only `state`), breaks table identity, costs two full conversions per command | Small |
| RT-09 | Contract-declared coalescing key (`coalesce: {key: [...]}`) instead of guessing from field names like `device`, `sink`, `player` | Core currently invents audio/media semantics (00 §5); wrong guesses lose writes | Small–moderate |
| RT-04 | Stop blocking tokio workers in `mesh.exec` (thread + `recv_timeout` up to ~5 s); `block_in_place`/dedicated provider threads now, async exec API later | Slow children stall other providers, the bridge and IPC | Moderate |
| CMP-08 + CMP-02 | Handlers as real Luau function values instead of global-name strings split by a hand-written scanner; callbacks passed as functions, not tokens | Valid Luau (`{pick({a=1})}`, closures, methods, locals) is rejected or misrouted; AGENTS.md calls hand parsing migration debt | Medium |
| CMP-07 | `<props>` as a statically parsed Luau table, one grammar for component and future backend props | Today a third, custom notation with its own lexer and LSP support | Mechanical rewrite of shipped blocks |
| UI-09 | Per-element behaviour table in `mesh-core-interaction` keyed on typed element kinds; add input caret navigation/selection and slider Home/End/Page keys | 00 §4 element ownership; behaviours are ~6k lines of tag-string tables in the shell, and inputs have no arrow-key caret movement | Large, incremental per element |
| SH-11 | Immutable per-frame `PaintContext` instead of thread-local render setters (fonts, aliases, blur policy, icon registry, tooltip style) | Blocks the backlog's parallel-paint and pipelining items; causes SH-05 cache thrash | Medium–high |
| SH-08 | Extract `shell/component/**` (~22k lines, no `impl Shell`) into a `mesh-core-frontend-runtime` crate | Largest god-module; duplicates interaction/animation ownership; not testable without the shell | Medium, mostly moves |
| RND-09 | One final damage stage: collect all producers, convert once to device pixels, make disjoint, feed clear/paint/SHM/`damage_buffer` | Root cause of RND-01 and RND-08; prerequisite for direct SHM paint and the persistent-scene tile index | Moderate |
| SH-04 | No-damage backoff: if `wants_render` persists but nothing was presented, sleep to the next animation deadline | The anti-spin guard was removed (`bb7132bf`); fully clipped infinite animations can busy-loop | Small |
| MOD-09 / MOD-10 / SH-10 / RES-08 | Move install/uninstall planning, `effective_profile_settings`, config paths and the control-plane lock into core; CLI and shell only render | CLI and shell disagree on uninstall checks, effective settings and locale reads; two lock files can guard one control plane | Moderate |
| RES-06 | One `@mesh/mesh-default` theme with dark/light modes; quick settings calls `set_mode` | Today "Light" switches Nord users to a different theme; seven theme files duplicate ~366 lines | Small |
| RES-12 | Remove core's `DebugInspectorView` tab enum; the inspector module owns its views | Devtools UI is module-owned (00 §2); core's cycle cannot reach the module's Elements tab | Small |

---

## 3. Refactors and dead code

- **SH-07** — `#![allow(dead_code)]` on `shell/mod.rs` and `service.rs` hides
  43 dead items: legacy drain/render/dispatch paths, dead frontend
  activate/deactivate, never-read fields (`presented_last_frame`,
  `ServiceCapabilities.{read,theme,locale}`), a `SoundKind::Shutdown` that
  never plays, and test-only wrappers that pass `BackendIdentity::default()`
  into ~20 production wildcard branches. Remove the allow; gate wrappers with
  `cfg(test)`; drop the wildcard.
- **UI-10** — ~450 lines of translation-only hit-test/scroll code behind
  `#![allow(dead_code)]`, plus the unused `EventDispatcher`/`InputState`
  (~1k lines in `elements/src/events.rs`).
- **RT-08** — `RuntimeSupervisor`/`RuntimeBackoff`/`RuntimeQuarantine`/
  `RuntimeStateTransaction` duplicate the real supervision in module/shell and
  have no production callers; `ModuleRuntime`/`ExecutionTier{Wasm}` contradict
  00 §7.
- **CMP-05** — `frontend/compiler/src/expr.rs` is `cfg(test)`-only with ~200
  lines of tests of semantics production never runs (and gets `#items > 0`
  wrong); the unsandboxed `Lua::new()` preview evaluator is unreachable in
  production. Delete; test through the real resolver.
- **CMP-06** — legacy `import X from "…"` scanner still blanks source before
  Luau runs; no shipped module uses it.
- **CMP-12** — `mesh-core-frontend-host` is 23 lines with a trait nothing
  calls through. Fold into `frontend-abi`.
- **MOD-11** — snake_case serde aliases, the TOML `ShellConfig`, test-only
  `mesh.toml` discovery, `manifest/json.rs`/`toml.rs` (S02-DEAD-001, still
  open) and a dead `merge_root` branch.
- **SH-09** — icon-pack validation implemented three times (shell, LSP,
  icon registry). **RES-13** — contained safe-file reader copied into theme,
  locale and resources.
- **RES-09 / RES-10** — LSP schema drift (missing `mesh.fonts`/`font_pack`,
  offers legacy keys and nonexistent `service.hyprland.*`); service completion
  infers fields from backend `return {}` tables (offers `ok`/`error`) instead
  of contracts.
- **RES-11** — `modules/backend/{shell-theme,networkmanager-network,
  reference-media}` are test fixtures without manifests in the shipped tree,
  reached through a path-rewrite shim.
- **RES-05** — volume icon/label logic copied into four frontends; ship a
  small library or component module (not a core element, per 00 §4).
- **RES-04** — clock and quick settings are hard-coded English; percentages
  bypass locale formatting. Needs `format_time`/`format_percent` host APIs.
- **RT-10** — pending side-effect budget is released only by drain, not by
  context teardown. **RT-11** — contracts reject finite recursive shapes such
  as `children: [MenuItem]`.
- **CMP-11** — `prop()` diagnostics point at the first textual match; inline
  `style` `prop()` is never compile-checked. **CMP-13** — child settings
  re-resolved and error-logged on every non-memo build.
- **UI-12** — transition interpolation edge cases: percent/`fit-content` to px
  jumps to the end value, `auto` interpolates from 0px, and reversing a
  half-finished transition restarts with the full duration instead of CSS's
  reversing shortening (`animation/src/transition.rs:606-612,810-820`).
- **RND-05** — shell visual damage ignores ancestor transforms and scroll;
  instance of the tracked canonical transform/clip item.

---

## 4. Performance hypotheses (unmeasured)

None repeat the rejected-experiments table. Each lists the workload to measure.

| ID | Hypothesis | Workload |
| --- | --- | --- |
| RND-02 | Any layout change repaints every scroll viewport on the surface (`display_list/mod.rs:505-512`) | `settings_scroll_frame_gate` + a variant resizing one label; repainted pixels/frame |
| RND-03 | `refresh_frame_plan` rebuilds `nodes`/`transforms`/`scroll_scopes` every paint with no production reader | `navigation_frame_cost_profile`, `appearance_frame_cost_profile`; allocations |
| RND-04 | Visual-damage map rebuilt from the whole tree every paint; scratch-reusing variant unused | same profiles, paint stage time |
| RND-08 | Current damage sent twice to the compositor, halving the 16-rect cap before union collapse (`entry.rs:784-806`) | presentation test counting `damage_buffer` calls at 9 rects |
| SH-05 | Per-component `set_font_aliases` clears text layout cache and glyph atlas whenever modules' alias maps differ | two repainting surfaces, one with a font override; `TextCacheMetrics.layout_invalidations` |
| SH-06 | Each scheduled effect is `Debug`-formatted twice for fingerprint/weight | slider drag `ServiceCommand` at 60–144 Hz with 2–50 KB payloads |
| SH-13 | Idle loop rebuilds the watch set and stats every source path | idle desktop, 30–100 modules; wakes/s, allocations/wake |
| UI-11 | Keyframe rules rebuilt from strings every frame per animated node (evidence for S09-PERF-002) | 1/10/100 `animation: spin infinite` nodes |
| CMP-09 | Three Lua allocations per template-expression call | `appearance_frame_cost_profile`, 200-row `{#for}` |
| CMP-04 / CMP-10 | Global expression cache never evicts; lock held across a 16 MB-stack thread spawn per new expression/parse | LSP typing session RSS; cold compile of `modules/frontend/settings` |
| RT-04 | `mesh.exec` blocks tokio workers | 9 providers, 4 workers, one doing 200 ms execs at 10 Hz |
| RES-15 | ICU plural/decimal formatters rebuilt per call | 200 plural `t()` per frame |

---

## 5. Refuted or already fixed

Checked by the agents and not reported: the multi-property `transition:`
first-only limitation (fixed; one timeline per entry); visibility-at-start,
pause-resume jump, per-keyframe easing, pseudo-state table split, layout-error
validity, reduced motion (all fixed); presentation damage loss on attach
failure and independent rounding policies (fixed); 24 h watcher park, raw
eventfd, unbounded re-entrant drain (fixed in production; dead code remains,
SH-07); backend events from failed callbacks, pending-call leaks on slot stop,
ready-before-initial-state, `spawn_stream` bypass (fixed); LSP UTF-16 ranges,
LSP validator, LSP registry refresh (fixed); core injecting display fields into
service state (no instance found); expression splicing as injection (no
privilege gained); `BackendIdentity::default()` wildcard exploitable live
(no — epochs start at 1; test seam only).

## 6. Suggested order

1. **Isolation and grants:** RT-01, MOD-01, MOD-02, MOD-03, MOD-04, SH-01.
2. **Shell survival:** SH-02, SH-03, SH-12, RT-03, RT-05, RT-02, SH-04.
3. **Silent wrong output users see now:** UI-01 (+ module rule fixes),
   CMP-01, RND-01, RND-06, UI-08, RES-07, RES-01, UI-02.
4. **Animation correctness:** UI-03, UI-04, UI-05, then UI-11's compile-once.
5. **Control-plane consolidation:** MOD-05, MOD-06, MOD-09, MOD-10, SH-10,
   RES-08.
6. **Dead-code sweep** (cheap, reduces risk for everything after): SH-07,
   UI-10, RT-08, CMP-05, CMP-06, CMP-12, MOD-11.
7. **Mechanics designs:** RND-09, SH-11, SH-08, RT-06, CMP-08, UI-09.
8. **Measure the performance hypotheses** in section 4.
