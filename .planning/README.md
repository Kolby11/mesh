# MESH planning

Progress tracking for MESH. Plain Markdown, updated by hand, no tooling — if a
file here needs a program to stay correct, it is the wrong shape.

This directory answers three questions, and each has exactly one home:

| Question | File | Shape |
| --- | --- | --- |
| What am I doing right now? | [`STATUS.md`](STATUS.md) | One page. Overwritten as work moves. |
| What is still open? | [`../docs/BACKLOG.md`](../docs/BACKLOG.md) | Terse checklist. Items leave when done. |
| What happened, and what did it measure? | [`log/`](log/) | Append-only, dated. Never edited after the fact. |

The split exists because the old backlog tried to be all three at once and grew
to 1,250 lines, with individual items carrying hundred-line progress narratives.
A backlog that records history stops being scannable, and history that lives
inside a checklist gets silently rewritten.

## The rules

1. **The backlog says what is open, not what happened.** An item is one to three
   lines: what to do, and why it is not done. No dated progress paragraphs, no
   benchmark numbers. If an item needs detail, link to a log entry.
2. **Finishing an item removes it from the backlog.** Its record goes to the
   log. `[x]` entries do not accumulate in `docs/BACKLOG.md`; a completed item
   that stays visible is just history in the wrong file.
3. **The log is append-only.** Correct a wrong entry with a new dated entry that
   says so. Never revise a past one — the numbers in it are the baseline
   something later gets compared against.
4. **Every performance claim carries its measurement.** Machine, build profile,
   workload shape, before and after, and the gate name if one exists. A speedup
   without its workload cannot be reproduced or trusted.
5. **Record what failed.** An approach that was measured and reverted is worth
   more than one that was never tried, because it stops the next person
   repeating it. See the rejected-experiments table in
   [`log/performance-log.md`](log/performance-log.md).
6. **`STATUS.md` is disposable.** It describes the present. Overwrite it freely;
   the log is what remembers.

## Layout

```
.planning/
  README.md      this file — how the system works
  STATUS.md      current position: in flight, next, blocked
  log/           dated history; append-only
  archive/       frozen; superseded or tool-era records
  todos/         standalone plans too big for a backlog line
  codebase/      structural analysis of the source tree
  research/      external investigation feeding decisions
  spikes/        throwaway experiments and their conclusions
  prototypes/    working code kept for reference
  debug/         recorded debugging investigations
  notes/         everything that outlived its conversation
  frontend/  renderer/  seeds/
```

### `log/`

Dated history. See [`log/README.md`](log/README.md) for what goes where and the
entry format. Current files:

- `2026-07.md` — the running monthly work log.
- `performance-log.md` — performance history and the rejected-experiments
  table, back to 2026-07-02.
- `backlog-archive-2026-07-28.md` — verbatim snapshot of the pre-split backlog,
  holding the detailed progress narratives for items still open.
- `sections.md` — the ten-subsystem map that section letters (A–V) refer to.

### `archive/`

Frozen. Nothing here is maintained and nothing here is authoritative — it is
kept because it explains how the project arrived where it is. See
[`archive/README.md`](archive/README.md).

### `todos/`

A backlog line that needs a design before it can be executed gets a file in
`todos/pending/`, and the backlog line links to it. Moves to `todos/completed/`
when the work lands, with the outcome appended.

## Relationship to `docs/`

`docs/` is the contract: [`docs/spec/`](../docs/spec/) defines behavior and
marks each section `Shipped` or `Target`; the guides describe how things
currently work. `.planning/` is evidence and intent. When the two disagree about
what MESH *does*, the spec wins — a planning document may explain why a decision
was made, but it never overrides the specification.
