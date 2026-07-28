# Log

Dated history. **Append-only** — entries are never edited after the fact.

An entry that turns out to be wrong is corrected by a *new* entry that says so
and links back. Past entries hold the baselines that later work is measured
against; rewriting one destroys the comparison that made it worth keeping.

## Files

| File | Covers |
| --- | --- |
| `YYYY-MM.md` | The running work log. One file per month, newest entry at the top. |
| [`performance-log.md`](performance-log.md) | Performance history back to 2026-07-02, and the rejected-experiments table. |
| [`sections.md`](sections.md) | The ten-subsystem map. Section letters (A–V) used across the logs refer to it. |
| [`backlog-archive-2026-07-28.md`](backlog-archive-2026-07-28.md) | Verbatim snapshot of `docs/BACKLOG.md` before the 2026-07-28 split. Holds the long progress narratives for items that are still open. |
| `<topic>-YYYY-MM-DD.md` | A single completed effort large enough to deserve its own file, e.g. `interaction-identity-2026-07-28.md`. |

Performance work goes in `performance-log.md`; everything else goes in the
monthly file. When a monthly entry would run past roughly a screen, give it its
own topic file and leave a one-line pointer in the month.

## Entry format

```markdown
## 2026-07-28 — short title

`commit-sha` · area: style resolution

What changed, in a few sentences. What it means for someone reading this in six
months and deciding whether to touch the same code.

**Measured.** Release, 456-node representative tree, three interleaved runs:
0.416–0.424ms before versus 0.390–0.397ms after (1.05–1.09x). Gated as
`shared_theme_defaults_speedup`.

**Left open.** Typed property values and one-time token lowering.
```

Only the heading and the first line are required. Add the rest when there is
something real to put in them.

## What earns an entry

- Anything that closes a backlog item — the item leaves the backlog, its record
  lands here.
- Any performance change, with its measurement.
- Any approach that was tried, measured, and abandoned. These are the most
  valuable entries in the file: they stop the next attempt from re-running a
  known dead end. Add it to the rejected-experiments table in
  `performance-log.md` as well when it is performance work.
- Any decision that changes the shape of the system, with the reasoning.

Routine commits do not need an entry. `git log` already has them; the log is
for the things a commit message is too small to hold.

## Measurements

A performance claim without its workload cannot be reproduced, and an
unreproducible number is worse than none — it gets trusted anyway. Record:

- build profile (release, unless there is a reason it is not),
- workload shape and size (node counts, iteration counts, tree structure),
- before and after as ranges across repeated runs, not single numbers,
- the gate name, if the speedup is checked in CI.

Interleave before/after runs when the effect is small enough that machine drift
between consecutive batches could account for it, and say that you did.
