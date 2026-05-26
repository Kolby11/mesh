---
phase: 75
phase_name: require-resolver-and-host-apis
status: planned
created: 2026-05-26
requirements:
  - LUAREQ-01
  - LUAREQ-02
  - LUAREQ-03
  - LUAREQ-04
  - LUAREQ-05
---

# Phase 75: Require Resolver And Host APIs - Plan

## Goal

Implement the shared require resolver for shell APIs, service/interface proxies, Luau libraries, and component definitions.

## Tasks

### 75-01 Frontend Require Resolver

**Files:**
- `crates/core/runtime/scripting/src/context/runtime.rs`
- `crates/core/runtime/scripting/src/context/tests.rs`

**Work:**
- Extract frontend `require` handling into a small internal resolver path.
- Resolve existing host API tables through canonical `mesh.*` require specifiers.
- Preserve interface proxy resolution and version parsing for `mesh.audio@>=1.0`.
- Preserve `@mesh/i18n` and support `mesh.i18n` as an equivalent helper library specifier.
- Keep unsupported/capability-denied/unavailable imports pcall-safe and diagnostic-visible.

**Validation:**
- Add tests for requiring shell host APIs and `mesh.i18n`.
- Re-run existing interface proxy and missing-interface diagnostics tests.

### 75-02 Component Require Import Discovery

**Files:**
- `crates/core/ui/component/src/parser/script.rs`
- `crates/core/ui/component/src/parser.rs`

**Work:**
- Parse simple Luau-native `local Alias = require("source")` declarations into `ComponentImport` records.
- Reuse the existing import target classifier for local components, module components, and interface APIs.
- Preserve source text so Phase 77 can define runtime component definition semantics.
- Keep legacy `import Alias from "..."` compatibility behavior.

**Validation:**
- Add parser tests for local component, module component, and interface API require imports.
- Ensure duplicate alias handling still rejects conflicting imports.

## Verification

Run:

```bash
nix develop -c cargo test -p mesh-core-scripting
nix develop -c cargo test -p mesh-core-component
nix develop -c cargo fmt --check
```
