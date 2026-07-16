# MESH Documentation

This directory contains current public contracts and verified author or
maintainer guidance. Start with the [project README](../README.md), then use
the sections below according to the question being answered.

## Document authority

MESH uses four explicit document classes:

1. **`docs/spec/` — public contract.** Each section says whether behavior is
   `Shipped` or `Target`. When implementation and target differ, both must be
   stated explicitly.
2. **`docs/` — current guidance.** Architecture, configuration, authoring,
   testing, module indexes, and implementation references verified against the
   source tree.
3. **`.planning/` — history and evidence.** Milestone plans, experiments,
   performance logs, migrations, and superseded design discussions. These may
   explain why a decision was made but do not override the specification.
4. **[`BACKLOG.md`](BACKLOG.md) — unfinished work.** The only active backlog;
   other documents may link to it but must not maintain competing TODO lists.

`CLAUDE.md` remains at the repository root as tool-specific project guidance,
not product documentation.

## Core guides

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

Historical renderer migration, UI transition, performance-roadmap, benchmark,
and iteration narratives live under `.planning/`.
