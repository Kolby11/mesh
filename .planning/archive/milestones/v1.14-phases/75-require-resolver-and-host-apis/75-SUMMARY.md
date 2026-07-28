---
phase: 75
phase_name: require-resolver-and-host-apis
status: complete
completed: 2026-05-26
requirements:
  - LUAREQ-01
  - LUAREQ-02
  - LUAREQ-03
  - LUAREQ-04
  - LUAREQ-05
---

# Phase 75: Require Resolver And Host APIs - Summary

## Delivered

- Frontend `require(...)` now resolves existing host API tables through canonical `mesh.*` specifiers for locale, UI, events, logging, and popover APIs.
- `mesh.i18n` now works as a Luau helper library alias alongside existing `@mesh/i18n`.
- Existing service/interface proxy resolution, including `require("mesh.audio@>=1.0")`, remains compatible.
- Unsupported imports remain pcall-safe without creating interface diagnostics; unavailable interfaces still produce visible diagnostics.
- The `.mesh` parser now discovers simple Luau-native `local Alias = require("...")` imports for local components, module components, and interface APIs.
- Legacy `import Alias from "..."` syntax remains compatible.

## Files Changed

- `crates/core/runtime/scripting/src/context/runtime.rs`
- `crates/core/runtime/scripting/src/context/tests.rs`
- `crates/core/ui/component/src/parser/script.rs`
- `crates/core/ui/component/src/parser.rs`

## Verification

Passed:

```bash
nix develop -c cargo test -p mesh-core-scripting
nix develop -c cargo test -p mesh-core-component
nix develop -c cargo fmt --check
```
