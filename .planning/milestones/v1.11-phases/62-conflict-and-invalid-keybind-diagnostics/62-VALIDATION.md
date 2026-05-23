# Phase 62 Validation

## Must Pass

- Focused keybind navigation tests.
- New diagnostics tests for:
  - malformed declarations,
  - duplicate effective bindings,
  - unresolved override ids,
  - unsafe override rejection,
  - missing runtime subscribers.

## Commands

```bash
nix develop -c cargo test -p mesh-core-shell keybind_diagnostic -- --nocapture
nix develop -c cargo test -p mesh-core-shell shell::component::tests::interaction::navigation -- --nocapture
```
