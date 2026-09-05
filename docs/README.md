# MESH Documentation

This directory contains current public contracts and verified author or
maintainer guidance. Start with the [project README](../README.md), then use
the sections below according to the question being answered.

## Document authority

MESH uses four explicit document classes:

1. **`docs/spec/` — public contract.** Each section says whether behavior is
   `Shipped` or `Target`. When implementation and target differ, both must be
   stated explicitly. [Platform philosophy](spec/00-philosophy.md) is the
   canonical home for vocabulary, ownership rules, and accepted principles;
   detailed chapters own their concrete contracts and implementation status.
2. **`docs/` — current guidance.** Architecture, configuration, authoring,
   testing, module indexes, and implementation references verified against the
   source tree.
3. **[`../.planning/`](../.planning/README.md) — intent and evidence.** Current
   status, audit evidence, pending design notes, renderer migration decisions,
   and dated history. These may explain why a decision was made but do not
   override the specification.
4. **[`BACKLOG.md`](BACKLOG.md) — unfinished work.** The only active backlog;
   other documents may link to it but must not maintain competing TODO lists.
   It records what is *open* — progress narratives and measurements belong in
   the log, and a completed item leaves the backlog rather than accumulating
   as a checked box.

[`AGENTS.md`](../AGENTS.md) remains at the repository root as agent-facing
project guidance, not product documentation. `CLAUDE.md` and `opencode.json`
are thin pointers to it so every coding tool reads one set of instructions.

## Core guides

- [Platform philosophy](spec/00-philosophy.md)
- [Architecture](architecture/overview.md)
- [Getting started](guides/getting-started.md)
- [Development](guides/development.md)
- [Testing](testing/overview.md)
- [Configuration](configuration/overview.md)
- [Active backlog](BACKLOG.md)

## Specification

[The unified specification](spec/README.md) defines the module system,
installation, components, styling, resources, settings, accessibility,
keyboard behavior, automation, and MCP direction.

## Author reference

- [`.mesh` syntax](frontend/mesh-syntax.md)
- [Elements](frontend/elements.md)
- [Renderer contract](frontend/renderer-contract.md)
- [CSS coverage](css-coverage.md)
- [Shipped module index](modules/README.md)

## Maintainer reference

- [Crate boundaries](crate-boundaries.md)
- [Renderer ownership](renderer-ownership.md)
- [Performance profiling](performance-profiling.md)

## Progress and history

- [Current status](../.planning/STATUS.md) — what is in flight now.
- [Work log](../.planning/log/README.md) — dated, append-only history including
  performance measurements and the rejected-experiments table.
- [How the planning system works](../.planning/README.md).
