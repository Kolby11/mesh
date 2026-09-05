# MESH

Read first: [`docs/architecture/overview.md`](docs/architecture/overview.md) and
[`docs/spec/README.md`](docs/spec/README.md).
Then read the canonical [platform philosophy](docs/spec/00-philosophy.md).

---

## Where work is tracked

**Check these at the start of any session that changes code.**

| Question | File |
| --- | --- |
| What should I work on? | [`docs/BACKLOG.md`](docs/BACKLOG.md) — the single list of open work |
| What is in flight right now? | [`.planning/STATUS.md`](.planning/STATUS.md) |
| Has this been tried before? | [`.planning/log/`](.planning/log/README.md) — dated history and measurements |

There is **one** backlog. Never create a competing TODO list, task file, or
progress tracker anywhere else in the repo, and never add a "TODO" section to a
doc — put the item in `docs/BACKLOG.md` or it does not exist.

### When you finish a piece of work

1. **Delete the item from `docs/BACKLOG.md`.** Do not mark it `[x]` and leave
   it — completed items leave the file entirely.
2. **Write its record in the log.** Performance work goes in
   [`.planning/log/performance-log.md`](.planning/log/performance-log.md);
   everything else goes in the current month's `.planning/log/YYYY-MM.md`.
   Create that file if the month has none.
3. **Update [`.planning/STATUS.md`](.planning/STATUS.md)** if what is in flight
   changed. It is short and disposable — overwrite it.

### When you start something new that isn't in the backlog

Add it to `docs/BACKLOG.md` first, as one to three lines: what to do, and why
it is not done. If it needs a design before it can be executed, write that in
`.planning/todos/pending/` and link to it from the backlog line.

### Rules the files enforce

- **The backlog says what is open, not what happened.** No dated progress
  paragraphs, no benchmark numbers, no completed items. It grew to 1,250 lines
  once by ignoring this; keep items to one to three lines.
- **The log is append-only.** Never edit a past entry. Correct a wrong one with
  a new dated entry that links back — past entries hold the baselines later work
  is compared against.
- **Every performance claim carries its measurement**: build profile, workload
  shape and size, before/after as ranges across repeated runs, and the gate name
  if one exists. A speedup without its workload is not reproducible.
- **Record what failed.** An approach that was measured and reverted is worth
  more than one never tried. Check the rejected-experiments table in
  `.planning/log/performance-log.md` *before* attempting an obvious
  optimization — several are already known dead ends.
Full rules: [`.planning/README.md`](.planning/README.md).

---

## Product contract

[Platform Philosophy](docs/spec/00-philosophy.md) is the canonical source for
what MESH is, terminology, core/module ownership, element standards,
configuration ownership, isolation, and language direction. Read it before
architecture changes or codebase audits. Keep detailed schemas and shipped/target
status in the relevant [specification chapter](docs/spec/README.md); do not
maintain another product philosophy in agent instructions or scan prompts.

MESH is a Wayland-native shell-building platform above an existing compositor.
Core owns platform invariants and built-in mechanisms; editable modules own
desktop experiences and domain integrations. Settings, devtools, and package
UIs are ordinary `.mesh` components using capability-gated core services.

## Implementation guidance

- Apply the ownership test in [00 §2](docs/spec/00-philosophy.md#2-core-owns-platform-invariants-modules-own-experiences)
  and the element admission test in [00 §4](docs/spec/00-philosophy.md#4-elements-enforce-shared-standards).
  Respect [crate dependency boundaries](docs/crate-boundaries.md).
- Use the canonical vocabulary in [00 §3](docs/spec/00-philosophy.md#3-modules-components-and-composition).
  Core elements expose base typed APIs; user components compose them; UI
  modules have one default public component and explicitly declared additional
  contributions. Luau and LSP APIs must preserve those boundaries.
- `module.json` is the only accepted module manifest, with all MESH behavior
  under `mesh`. Reject legacy manifests with migration diagnostics. See
  [01](docs/spec/01-module-system.md) for kinds, contracts, and capabilities.
- Implement domain backend behavior through the extension runtime host API,
  not in Rust shell code. Add generic host capabilities when necessary.
  Built-in settings/storage, inspection, and package/profile mechanisms are
  explicitly core-owned; their UIs remain modules.
- Luau through `mlua` is the current runtime. Execute scripts with the real
  runtime, not handwritten string parsing/interpreting; custom execution
  substitutes are migration debt to remove, not a model to expand.
  TypeScript/JavaScript is an undecided alternative, not an approved migration.
  WASM and native Rust module tiers are not committed targets.
- Prefer normal Lua/Luau syntax and semantics. Add special parsing, DSL behavior,
  magic globals, or nonstandard syntax only for a clear product need that
  ordinary host APIs and language constructs cannot cleanly meet.
- Backend `main.luau` exposes `start(self)`; setup and poll registration belong
  inside it instead of top-level side effects.
- Component `<props>` and their style/script/settings projections are shipped.
  Backend/interface props and script-side layer introspection remain targets;
  use [03](docs/spec/03-components.md) and [08](docs/spec/08-settings.md) for
  precise status. Scripts control effective props while persisted user
  preferences remain distinct and inspectable by contract.
- Module execution stays within the shell process. Private environments,
  explicit public communication, capability checks, execution budgets, and
  local error reporting are separate obligations; a shared VM is permitted.
  Do not claim protection against every native process crash.
- Accessibility, localization, diagnostics, semantic resource resolution,
  scoped persistence, and low idle/redraw overhead are platform requirements.
  Implement the detailed element and service contracts instead of duplicating
  those standards in each desktop feature.

## Review and audit guidance

Use [prompts.md](prompts.md) for the whole-codebase audit procedure and the
historical [.planning/log/sections.md](.planning/log/sections.md) for section
identities. Verify current packages and ownership against the workspace and
[crate boundaries](docs/crate-boundaries.md); historical reports do not override
current specs.

Distinguish shipped defects from unimplemented targets and undecided language
options. Record new open work only in `docs/BACKLOG.md`, and preserve the
append-only history rules above.
