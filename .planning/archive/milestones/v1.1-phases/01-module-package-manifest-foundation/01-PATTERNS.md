# Phase 01 Pattern Map

## Scope

Phase 01 creates the `~/.mesh/package.json` installed-module manifest, module-level `package.json` loading, and normalized installed module graph. The closest existing patterns are plugin manifest parsing, config/theme path helpers, shell provider selection, and frontend composition catalog code.

## Files to Create or Modify

| Planned file | Role | Closest existing analog |
|--------------|------|-------------------------|
| `crates/core/extension/plugin/src/package.rs` | Root package schema, module package schema, compatibility loader, graph normalization | `crates/core/extension/plugin/src/manifest.rs` |
| `crates/core/extension/plugin/src/manifest.rs` | Legacy `plugin.json` compatibility and package manifest conversion touchpoint | Existing `load_manifest()` / `JsonManifest` parser |
| `crates/core/extension/plugin/src/lib.rs` | Public exports for package/module graph types | Existing manifest exports |
| `config/package.json` | Repo-local fixture mirroring `~/.mesh/package.json` | `config/shell-settings.json`, existing `packages/plugins/**/plugin.json` |
| `config/modules/@mesh/*/package.json` | Module package fixtures | `packages/plugins/**/plugin.json` |
| `crates/core/foundation/config/src/lib.rs` | `~/.mesh/settings.json` path integration | `default_settings_path()` |
| `crates/core/foundation/theme/src/lib.rs` | `~/.mesh/themes/` path integration | `theme_dir_path()` |
| `crates/core/shell/src/shell/mod.rs` | Shell-facing package graph proof | `spawn_backend_plugins()` provider grouping and shell startup tests |
| `docs/settings/README.md` | User-facing settings path docs | Existing settings layer docs |
| `docs/theming/themes.md` | Theme module/user theme path docs | Existing token/theme docs |

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

Apply the same pattern to `RootPackageManifest`, `ModulePackageManifest`, `MeshModuleSection`, and graph node types. Avoid ad hoc JSON lookup.

### Raw vs. Normalized APIs

`Manifest::declared_provides()` normalizes legacy and new-style manifest declarations. Use the same split:

- raw user-authored `RootPackageManifest`
- raw module-authored `ModulePackageManifest`
- compatibility-loaded `LoadedModuleManifest`
- normalized `InstalledModuleGraph`

This prevents shell code from understanding every package manifest edge case.

### Contribution Pattern

Research recommends one `mesh.contributes` map for extensibility. Use it for layout, settings, themes, icons, fonts, and i18n rather than new root-level arrays.

Core rule: every module kind uses the same package envelope; module kinds only differ by `mesh.kind` and `mesh.contributes`.

### Path Helpers

`mesh-core-config` already exposes helpers like `default_settings_path()` that honor env vars and repo-local defaults. Phase 01 should introduce a single `~/.mesh` concept:

- `MESH_HOME` for tests
- `~/.mesh/package.json`
- `~/.mesh/settings.json`
- `~/.mesh/modules/`
- `~/.mesh/themes/`

Repo-local `config/` remains a development fixture fallback, not the conceptual user path.

### Backend Provider Selection

Current shell backend selection groups candidates by service and sorts by priority:

```rust
candidates.sort_by(|a, b| {
    b.priority
        .cmp(&a.priority)
        .then_with(|| a.plugin_id.cmp(&b.plugin_id))
});
```

Phase 01 should not rewrite lifecycle spawning, but graph APIs should expose:

- all providers for `mesh.audio`
- explicit active provider from root package
- fallback provider by priority

Phase 02 can replace lifecycle selection with these graph APIs.

## Integration Notes

- Prefer module naming in new schema/docs: `RootPackageManifest`, `ModulePackageManifest`, `InstalledModuleGraph`, `ModuleKind`.
- Preserve legacy `plugin.json` loading as a compatibility alias.
- Prefer module `package.json` over `plugin.json` when both exist.
- Store Git origin/repository metadata only; do not implement clone/fetch/download.
- Use real existing module IDs in fixtures:
  - frontend: `@mesh/panel`, `@mesh/quick-settings`
  - backend: `@mesh/pipewire-audio`, `@mesh/pulseaudio-audio`
  - theme/backend example: `@mesh/shell-theme`
  - active provider: `@mesh/pipewire-audio`
