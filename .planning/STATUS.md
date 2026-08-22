# Status

**Updated:** 2026-08-22

## Now

The locale scripting ABI is aligned: `mesh.i18n.t` is the module-scoped read
surface, `mesh.locale.current` is the locale read surface, and
`mesh.locale.set` is the locale write surface. Runtime injection and
`require` enforce `locale.read`/`locale.write` independently, and LSP
knowledge matches the runtime.

The next open localization item is centralized localized metadata and visible
miss handling with owner, fallback, source, and snapshot revision. Config (42)
and locale (12) test suites pass; scripting/LSP/module/shell validation remains
blocked by the pre-existing `InterfaceProvider` dereference error at
`crates/core/extension/service/src/interface.rs:229`.
