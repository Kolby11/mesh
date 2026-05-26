---
phase: 77
phase_name: component-definitions-and-binding
status: planned
created: 2026-05-26
requirements:
  - LUACOMP-01
  - LUACOMP-02
  - LUACOMP-03
  - LUACOMP-04
  - LUACOMP-05
  - LUACOMP-06
---

# Phase 77: Component Definitions And Binding - Plan

## Goal

Support require-based frontend component definitions, markup instantiation, direct public-field attributes, and `bind:this` mounted instance references.

## Tasks

### 77-01 Parser And Render Binding Metadata

**Files:**
- `crates/core/ui/component/src/template.rs`
- `crates/core/ui/component/src/parser/markup.rs`
- `crates/core/ui/component/src/parser.rs`
- `crates/core/frontend/compiler/src/render.rs`
- `crates/core/frontend/compiler/src/compile.rs`

**Work:**
- Add `bind:this` AST support.
- Pass instance binding names through the frontend render/composition boundary as internal metadata.
- Keep require-discovered component imports and legacy import syntax working.

### 77-02 Mounted Instance Metadata Binding

**Files:**
- `crates/core/runtime/scripting/src/context/runtime.rs`
- `crates/core/shell/src/shell/component/composition.rs`

**Work:**
- Use public member inspection to create a child instance snapshot.
- Bind that snapshot into the parent runtime state under the requested `bind:this` name.
- Do not pass internal binding metadata into child public fields.

## Verification

Run:

```bash
nix develop -c cargo test -p mesh-core-component
nix develop -c cargo test -p mesh-core-frontend
nix develop -c cargo test -p mesh-core-scripting
nix develop -c cargo test -p mesh-core-shell bind_this
nix develop -c cargo fmt --check
```
