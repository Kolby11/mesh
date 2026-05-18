# Architecture Research: v1.8 Rendering Engine Architecture

## Existing Architecture Constraints

- MESH already has retained widget identity, typed invalidation, retained render objects, retained display data, damage tracking, text caching, selector indexing, profiling snapshots, and shipped-surface benchmarks.
- The Rust core must remain generic; shell surfaces and services should not force renderer-specific special cases.
- Plugin-authored `.mesh` UI, theme tokens, diagnostics, and profiling payloads are part of the product contract.

## Candidate Paths

### Path A: Adopt Blitz as Base

Use Blitz as the renderer/DOM/layout/style foundation and adapt MESH surfaces into Blitz-compatible inputs.

**Advantages:** fastest path to a modular Rust HTML/CSS renderer; reuses Stylo/Taffy/Parley integration; aligns with the user's requested inspiration.

**Risks:** MESH may inherit Dioxus/native assumptions, browser-like DOM semantics, and renderer lifecycle constraints that conflict with shell surfaces, diagnostics, retained invalidation, and Wayland-specific behavior.

### Path B: Borrow Blitz Architecture, Keep MESH Renderer Ownership

Treat Blitz as the reference architecture. Evaluate its crate boundaries and adopt selected pieces such as Taffy, Parley, Skia/AnyRender, or specific style/layout ideas behind MESH-owned adapters.

**Advantages:** keeps MESH's retained graph, profiling, diagnostics, and module model authoritative while reducing reinvention.

**Risks:** slower than direct adoption; requires careful adapter design; easy to create duplicated layout/style abstractions if the boundary is vague.

### Path C: MESH-Owned Renderer with Focused Crates

Build a new MESH renderer stack around focused crates: Taffy for layout, Parley for text, Skia or AnyRender for paint/GPU, AccessKit for accessibility, and optional html5ever/xml5ever only for imported document formats.

**Advantages:** maximum control over shell-specific behavior and migration sequencing.

**Risks:** highest implementation cost; repeats integration work Blitz is already doing; requires strong test coverage to avoid regressions.

## Recommended Build Order

1. Architecture spike: map current MESH renderer stages to Blitz and focused-crate equivalents.
2. Prototype Blitz rendering for one static shipped-surface equivalent.
3. Prototype focused-crate rendering for the same surface using MESH retained data.
4. Compare with a fixed scorecard: responsiveness, invalidation, diagnostics, accessibility, build complexity, and migration risk.
5. Implement only the chosen proof slice in production paths.

## Decision Bias

Default to Path B unless prototype evidence shows direct Blitz adoption is clean. MESH's value is not generic HTML rendering; it is deterministic, observable shell UI authored by plugins.
