# Testing Patterns

**Analysis Date:** 2026-05-06

## Test Framework

**Runner:**
- Rust built-in test harness from `cargo test`.
- Config: workspace `Cargo.toml`; no `jest.config.*`, `vitest.config.*`, or custom Rust test runner config is present.
- Tests are embedded in Rust source files with `#[cfg(test)] mod tests` and `#[test]`. Inventory scan found 335 `#[test]` functions and 32 `#[cfg(test)]` blocks across `crates/**/*.rs`.

**Assertion Library:**
- Rust standard assertions: `assert!`, `assert_eq!`, `assert_ne!`, `matches!`, and explicit `panic!` in pattern-match fallthroughs.
- JSON assertions use `serde_json::json!` and direct equality, for example `crates/core/shell/src/shell/component/tests.rs` and `crates/core/runtime/scripting/src/context.rs`.

**Run Commands:**
```bash
nix develop -c cargo test --workspace              # Run all tests with required Wayland/xkbcommon system libraries
nix develop -c cargo test -p mesh-core-plugin      # Run one crate
nix develop -c cargo test -p mesh-core-plugin installed_module_graph
nix develop -c cargo test -p mesh-core-scripting require_missing_interface
nix develop -c cargo test --workspace -- --list    # List tests after compiling test binaries
cargo fmt --check                                  # Format check when system deps are not needed
nix develop -c cargo clippy --workspace --all-targets
```

Direct `cargo test --workspace -- --list` outside the Nix dev shell currently fails while compiling `smithay-client-toolkit` because `xkbcommon.pc` is not available to `pkg-config`. Use `nix develop -c ...` for workspace test and clippy commands.

## Test File Organization

**Location:**
- Unit tests live in `#[cfg(test)] mod tests` at the bottom of the source file under test, for example `crates/core/extension/plugin/src/package.rs`, `crates/core/ui/component/src/parser.rs`, `crates/core/runtime/scripting/src/context.rs`, and `crates/core/foundation/diagnostics/src/lib.rs`.
- A large component behavior test module lives in a dedicated sibling file, `crates/core/shell/src/shell/component/tests.rs`, and is included from the shell component module.
- Shell-level integration-style tests live inside `crates/core/shell/src/shell/mod.rs` because they need private shell helpers and state.
- No top-level `tests/` directory is present. Do not add one unless testing a public crate boundary is more appropriate than private module access.

**Naming:**
- Name tests as behavior statements in `snake_case`, for example `installed_module_graph_rejects_unknown_active_provider`, `module_manifest_loader_prefers_package_json_over_plugin_json`, and `quick_settings_wifi_row_publishes_connect_for_wifi_network_ids`.
- Prefix related tests with the component/feature under test, for example `installed_module_graph_*` in `crates/core/extension/plugin/src/package.rs`, `module_package_manifest_*` in `crates/core/extension/plugin/src/package.rs`, and `quick_settings_*` in `crates/core/shell/src/shell/component/tests.rs`.
- Use test selectors that map to those prefixes when verifying a focused change, for example `cargo test -p mesh-core-plugin installed_module_graph`.

**Structure:**
```text
crates/
  core/
    extension/plugin/src/package.rs        # package manifest, graph, provider tests
    extension/plugin/src/manifest.rs       # legacy manifest and dependency graph tests
    extension/service/src/*.rs             # interface/contract/registry tests
    runtime/scripting/src/context.rs       # frontend Luau proxy/runtime tests
    runtime/scripting/src/backend.rs       # backend Luau host API tests
    runtime/backend/src/lib.rs             # async backend service loop tests
    shell/src/shell/mod.rs                 # shell graph/lifecycle/integration tests
    shell/src/shell/component/tests.rs     # component runtime and real surface script tests
    ui/component/src/parser.rs             # .mesh parser tests
    ui/elements/src/style.rs               # CSS/style parser and resolver tests
    ui/render/src/**/*.rs                  # compile/render/pixel tests
```

## Test Structure

**Suite Organization:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_module_graph_returns_explicit_active_provider() {
        let root = root_with_modules(
            &[
                ("@mesh/pipewire-audio", ModuleKind::Backend),
                ("@mesh/pulseaudio-audio", ModuleKind::Backend),
            ],
            &[("mesh.audio", "@mesh/pipewire-audio")],
            None,
        );
        let graph = InstalledModuleGraph::from_parts(root, audio_modules()).unwrap();

        assert_eq!(
            graph.active_provider("mesh.audio").unwrap().module_id,
            "@mesh/pipewire-audio"
        );
    }
}
```

This pattern is from `crates/core/extension/plugin/src/package.rs`: helper fixtures first, direct construction of domain values, then precise assertions.

**Patterns:**
- Keep fixture builders inside the test module when they need private types, for example `root_with_modules`, `loaded_module`, `audio_modules`, and `interface_module` in `crates/core/extension/plugin/src/package.rs`.
- Use inline raw string JSON for manifest parsing tests so the manifest shape is visible in the test, for example `module_root_manifest_parses_minimal_package_json` and `module_package_manifest_parses_backend_package_json`.
- Use `tempfile` or local temp helpers for filesystem loader tests. `mesh-core-service`, `mesh-core-shell`, `mesh-core-icon`, and `mesh-core-render` declare `tempfile = "3"` in their crate manifests.
- Use `EnvGuard`-style RAII guards when mutating process environment variables, as in `crates/core/extension/plugin/src/package.rs`.
- Match output request slices explicitly when testing component scripts and service commands, as in `crates/core/shell/src/shell/component/tests.rs`.
- Prefer direct error assertions for validation logic: `assert!(InstalledModuleGraph::from_parts(...).is_err())` or `assert!(matches!(mesh_home(), Err(PackageManifestError::InvalidMeshHome(_))))`.

## Mocking

**Framework:** No mocking framework is used.

**Patterns:**
```rust
fn loaded_module(
    name: &str,
    kind: ModuleKind,
    dependencies: MeshDependencies,
    provides: Vec<MeshProvidesDeclaration>,
    contributes: MeshContributes,
) -> LoadedModuleManifest {
    LoadedModuleManifest {
        manifest: ModulePackageManifest {
            name: name.into(),
            version: "0.1.0".into(),
            description: None,
            license: None,
            repository: None,
            mesh: MeshModuleSection {
                api_version: "0.1".into(),
                kind,
                capabilities: CapabilitiesSection::default(),
                i18n: MeshI18nSupport::default(),
                entrypoints: MeshEntrypoints::default(),
                dependencies,
                provides,
                implements: Vec::new(),
                interface: None,
                contributes,
                experimental: serde_json::Value::Null,
            },
        },
        path: PathBuf::from(format!("{name}/package.json")),
        source: ModuleManifestSource::PackageJson,
    }
}
```

This pattern from `crates/core/extension/plugin/src/package.rs` shows the preferred approach: build real domain structs with small helper constructors instead of using mocks.

**What to Mock:**
- Mock external system command results through runtime host test hooks or inline Luau scripts, not by invoking tools such as `wpctl`, `pactl`, `nmcli`, or `upower`.
- Mock module graphs with in-memory `RootPackageManifest` and `LoadedModuleManifest` values for graph behavior in `crates/core/extension/plugin/src/package.rs`.
- Mock shell component runtime dependencies with test contexts and captured `CoreRequest` values in `crates/core/shell/src/shell/component/tests.rs`.
- Mock filesystem modules with temporary directories when testing manifest loader precedence and path validation in `crates/core/extension/plugin/src/package.rs`.

**What NOT to Mock:**
- Do not mock the package manifest parser when changing module-system conventions. Parse actual `package.json` strings through `RootPackageManifest::from_json_str`, `ModulePackageManifest::from_json_str`, or `load_installed_module_graph`.
- Do not mock the `.mesh` parser for component syntax changes. Use `parse_component` from `crates/core/ui/component/src/parser.rs`.
- Do not mock interface/provider resolution when testing shell backend launch behavior. Use `InstalledModuleGraph` and `InterfaceRegistry` paths like `backend_lifecycle_uses_explicit_active_provider_from_package_graph` in `crates/core/shell/src/shell/mod.rs`.

## Fixtures and Factories

**Test Data:**
```rust
fn root_with_modules(
    modules: &[(&str, ModuleKind)],
    providers: &[(&str, &str)],
    layout: Option<&str>,
) -> RootPackageManifest {
    RootPackageManifest {
        schema_version: 1,
        modules_dir: "modules".into(),
        modules: modules
            .iter()
            .map(|(id, kind)| {
                (
                    (*id).into(),
                    InstalledModuleEntry {
                        kind: *kind,
                        path: format!("modules/{id}"),
                        enabled: true,
                    },
                )
            })
            .collect(),
        providers: providers
            .iter()
            .map(|(interface, module_id)| ((*interface).into(), (*module_id).into()))
            .collect(),
        layout: layout.map(|entrypoint| RootLayoutSelection {
            entrypoint: entrypoint.into(),
        }),
        theme: None,
    }
}
```

**Location:**
- Manifest and graph fixtures live in `crates/core/extension/plugin/src/package.rs`.
- Runtime script fixtures are inline raw Luau strings in `crates/core/runtime/scripting/src/context.rs`, `crates/core/runtime/scripting/src/backend.rs`, and `crates/core/shell/src/shell/component/tests.rs`.
- Real repo fixtures are used by package graph tests through `config/package.json` and `config/modules/@mesh/*/package.json`. Keep these files aligned with the assertions in `crates/core/extension/plugin/src/package.rs` and `crates/core/shell/src/shell/mod.rs`.
- Legacy manifest compatibility fixtures are real files under `modules/frontend/navigation-bar/module.json` and `modules/backend/*/module.json`.

## Coverage

**Requirements:** No enforced coverage threshold or coverage tool config is present.

**View Coverage:**
```bash
# Not configured in this repo.
# Add cargo-llvm-cov only if the project decides to enforce coverage.
```

Use targeted behavior tests as the primary quality gate. For broad changes, run the affected crate tests plus `nix develop -c cargo test --workspace`.

## Test Types

**Unit Tests:**
- Parser and validation tests assert direct return values and error variants in files such as `crates/core/ui/component/src/parser.rs`, `crates/core/extension/plugin/src/package.rs`, `crates/core/extension/service/src/contract.rs`, and `crates/core/foundation/diagnostics/src/lib.rs`.
- Style/render tests assert parsed values, resolved style, layout state, and pixels in files such as `crates/core/ui/elements/src/style.rs`, `crates/core/ui/render/src/surface/painter.rs`, and `crates/core/ui/render/src/surface/icon.rs`.

**Integration Tests:**
- Package graph tests integrate root config, module package manifests, provider selection, contributed resources, layout entrypoints, and legacy manifest loading in `crates/core/extension/plugin/src/package.rs`.
- Shell tests integrate `InstalledModuleGraph`, `InterfaceRegistry`, backend launch candidate selection, lifecycle status, and active-provider state in `crates/core/shell/src/shell/mod.rs`.
- Component runtime tests load representative Luau snippets and convert published events into `CoreRequest` values in `crates/core/shell/src/shell/component/tests.rs`.
- Backend runtime tests exercise Luau host APIs, exported state snapshots, command handling, and polling behavior in `crates/core/runtime/scripting/src/backend.rs` and `crates/core/runtime/backend/src/lib.rs`.

**E2E Tests:**
- Not used. There is no Playwright, WebDriver, compositor-driven, or full shell E2E test harness in the repo.
- Rendering proof is currently unit/integration level through software buffers and pixel assertions in `crates/core/ui/render/src/surface/*.rs`.

## Common Patterns

**Async Testing:**
```rust
#[test]
fn backend_runtime_behavior() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        // spawn backend service, send commands, receive events, assert payloads
    });
}
```

Use a Tokio runtime explicitly in sync `#[test]` functions when testing shell/backend async behavior. The workspace uses Tokio but does not consistently use `#[tokio::test]`; follow the local pattern in `crates/core/shell/src/shell/mod.rs` and `crates/core/runtime/backend/src/lib.rs`.

**Error Testing:**
```rust
#[test]
fn require_missing_interface_emits_visible_diagnostic() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@mesh/diagnostic-test", caps).unwrap();
    ctx.load_script(r#"function init() require("@mesh/audio@>=1.0") end"#).unwrap();

    let err = ctx.call_init().unwrap_err();
    assert!(matches!(err, ScriptError::InterfaceUnavailable(_)));
    let diagnostics = ctx.drain_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].interface, "mesh.audio");
}
```

This pattern from `crates/core/runtime/scripting/src/context.rs` is the model for runtime error paths: assert the error, then assert visible diagnostics.

**Module-System Test Requirements:**
- When changing `docs/module-system.md`, `README.md`, `docs/plugins/README.md`, or examples under `config/modules/@mesh/*/package.json`, update tests in `crates/core/extension/plugin/src/package.rs` that parse package JSON and graph fixtures.
- Add tests for every new `mesh` manifest field in both parsing and validation layers. Use `ModulePackageManifest::from_json_str` for module manifests and `RootPackageManifest::from_json_str` for `config/package.json` shape changes.
- Provider resolution changes need tests for explicit selection, multiple installed providers, unknown provider rejection, interface mismatch rejection, and priority/default behavior in `crates/core/extension/plugin/src/package.rs`.
- Shell provider lifecycle changes need tests in `crates/core/shell/src/shell/mod.rs` proving `backend_launch_candidates_from_graph` launches only active providers and records invalid/missing provider status without service-specific branches.
- Legacy compatibility changes need tests for `module.json`, `plugin.json`, and `package.json` precedence in `load_module_manifest`.
- Resource contribution changes need path traversal tests like `installed_module_graph_rejects_library_path_escape` and `installed_module_graph_rejects_contribution_path_escape`.

---

*Testing analysis: 2026-05-06*
