# Archive

**Frozen.** Nothing here is maintained, and nothing here is authoritative.

These files are kept because they explain how the project arrived where it is.
Read them for context; do not read them for current state. Where an archived
document contradicts [`docs/spec/`](../../docs/spec/) or
[`docs/BACKLOG.md`](../../docs/BACKLOG.md), it is simply out of date.

Paths inside these files were not rewritten when they moved here, so internal
links may point at locations that no longer exist.

## Contents

| Path | What it is |
| --- | --- |
| `gsd-state/` | Tool-era tracking state, superseded 2026-07-28 by [`../STATUS.md`](../STATUS.md) and [`../log/`](../log/). |
| `milestones/` | Per-milestone roadmaps, audits, and state for v1.0 through v1.21. |
| `phases/` | Phase plans, research, and verification for the last milestone worked under the old system. |
| `quick/` | Small tracked tasks from the old workflow. |
| `performance-roadmap.md` | Superseded performance roadmap. The live performance record is [`../log/performance-log.md`](../log/performance-log.md). |

## `gsd-state/`

MESH used the "get-shit-done" (GSD) planning tool until 2026-07-28, when it was
removed. These are its state files:

- `STATE.md` — last written 2026-07-15. It reports milestone v1.21 phase 104 at
  33%, which had already stopped matching the actual work well before the tool
  was removed; the tree had moved on to the performance-checkpoint stream. It is
  a good illustration of why the replacement keeps status on one short page that
  is cheap to overwrite.
- `PROJECT.md`, `ROADMAP.md`, `MILESTONES.md`, `REQUIREMENTS.md`,
  `RETROSPECTIVE.md` — project framing under that tool.
- `HANDOFF.json`, `config.json` — tool state, of no further use.
- `v1.*-MILESTONE-AUDIT.md` — completion audits per milestone.

The milestone audits and `RETROSPECTIVE.md` still hold real reasoning about why
things were built the way they were. The rest is process residue.
