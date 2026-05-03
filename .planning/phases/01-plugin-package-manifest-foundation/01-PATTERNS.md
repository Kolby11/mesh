# Phase 01 Pattern Map

## Scope

Phase 01 creates a local package.json-like installed-plugin manifest and normalized package graph. Closest existing patterns are plugin manifest parsing, shell config path helpers, and backend provider selection.

## Files to Create or Modify

| Planned file | Role | Closest existing analog |
|--------------|------|-------------------------|
| `crates/core/extension/plugin/src/package.rs` | Installed plugin package schema, parser, validation, graph | `crates/core/extension/plugin/src/manifest.rs` |
| `crates/core/extension/plugin/src/lib.rs` | Public exports for package graph types | Existing `manifest` exports |
| `config/plugins.json` | Repo-local sample/default package manifest | `config/shell-settings.json`, existing `packages/plugins/**/plugin.json` |
| `crates/core/shell/src/shell/mod.rs` or small helper module | Shell-facing load/proof path for package graph | `Shell::new()`, `load_shell_settings()`, `spawn_backend_plugins()` |

## Established Patterns

### Manifest Parsing

Use typed `serde` structs in `mesh-core-plugin`, mirroring `Manifest` in `crates/core/extension/plugin/src/manifest.rs`.

Current pattern:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub package: PackageSection,
    #[serde(default)]
    pub dependencies: DependenciesSection,
}
```

Apply the same pattern to `InstalledPluginPackage`, `InstalledPluginEntry`, and provider selection structs. Avoid ad hoc JSON string lookup.

### Normalized Compatibility APIs

`Manifest::declared_provides()` normalizes legacy and new-style manifest declarations. Use a similar split:

- raw user-authored `InstalledPluginPackage`
- normalized `InstalledPluginGraph`

This prevents shell code from understanding every package manifest edge case.

### Shell Config Path Helpers

`mesh-core-config` exposes helpers like `default_settings_path()` that honor env vars and repo-local defaults. For Phase 01, use the same pattern but avoid putting package graph types in `mesh-core-config` if that would create upward dependency pressure.

Recommended path helper name: `default_plugin_package_path()` or `default_installed_plugins_path()`.

### Backend Provider Selection

Current shell backend selection groups candidates by service and sorts by priority:

```rust
candidates.sort_by(|a, b| {
    b.priority
        .cmp(&a.priority)
        .then_with(|| a.plugin_id.cmp(&b.plugin_id))
});
```

Phase 01 should not rewrite lifecycle spawning, but active provider graph APIs should make Phase 02 able to replace or augment this priority-only choice with package-manifest selection.

## Integration Notes

- Keep local package manifest semantics independent from remote package download.
- Treat backend category as the user-facing grouping (`audio`, `network`, `shortcuts`) while retaining interface names (`mesh.audio`) for runtime wiring.
- Sample package manifest should use real existing plugin IDs so tests can assert meaningful graph behavior:
  - frontend: `@mesh/panel`, `@mesh/quick-settings`
  - backend: `@mesh/pipewire-audio`, `@mesh/pulseaudio-audio`
  - category: `audio`
  - active provider: `@mesh/pipewire-audio`
