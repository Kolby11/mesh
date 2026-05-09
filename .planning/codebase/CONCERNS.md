# Codebase Concerns

**Analysis Date:** 2026-05-06

## Tech Debt

**Module/package terminology is split across docs and implementation:**
- Issue: `docs/module-system.md` defines `package.json` plus `mesh` as the target module manifest, while `docs/installation.md` still presents `plugin.json` as the authoritative package root. Runtime loaders accept both shapes, but author-facing docs do not give one consistent source of truth.
- Files: `docs/module-system.md`, `docs/installation.md`, `crates/core/extension/plugin/src/package.rs`, `crates/core/extension/plugin/src/manifest.rs`
- Impact: New module authors can create `plugin.json` manifests from `docs/installation.md` that do not match the package model described in `docs/module-system.md`; planners and tests can target the wrong manifest shape.
- Fix approach: Make `docs/installation.md` describe `package.json` + `mesh` as the primary installer format, keep `plugin.json`, `package.json`, and `mesh.toml` as compatibility-only paths, and align examples with `ModulePackageManifest` in `crates/core/extension/plugin/src/package.rs`.

**Root module graph and catalog fixtures are different sources of truth:**
- Issue: `config/package.json` loads modules from `../modules` and enables only `@mesh/navigation-bar`, `@mesh/pipewire-audio`, and `@mesh/pulseaudio-audio`. Separate package-style manifests under `config/modules/@mesh/**/package.json` include `@mesh/networkmanager`, `@mesh/upower`, `@mesh/panel`, `@mesh/quick-settings`, and `@mesh/shell-theme`, but those files are not loaded by `load_installed_module_graph(&config/package.json)`.
- Files: `config/package.json`, `config/modules/@mesh/networkmanager/package.json`, `config/modules/@mesh/upower/package.json`, `config/modules/@mesh/panel/package.json`, `config/modules/@mesh/quick-settings/package.json`, `config/modules/@mesh/shell-theme/package.json`, `crates/core/extension/plugin/src/package.rs`
- Impact: Docs and shell tests can assume network, power, panel, and theme modules exist in the active graph while the actual root graph exposes only navigation bar and audio providers.
- Fix approach: Either move `config/modules/@mesh/**` into the root graph's `mesh.modules` and `mesh.providers`, or document them as an unused registry/catalog fixture and keep runtime tests pointed at `modules/**`.

**Manifest loader precedence keeps `package.json` ahead of `package.json`:**
- Issue: `load_module_manifest()` checks `package.json` before `package.json`, so a module directory containing both files resolves to the legacy manifest even though `docs/module-system.md` says new examples should use `package.json`.
- Files: `crates/core/extension/plugin/src/package.rs`, `crates/core/extension/plugin/src/manifest.rs`, `modules/frontend/navigation-bar/package.json`
- Impact: Adding `modules/frontend/navigation-bar/package.json` beside the current `package.json` will not activate the new manifest unless `package.json` is removed first.
- Fix approach: Decide whether coexistence is supported. If yes, prefer `package.json` and add a regression test for `package.json` + `package.json`; if no, document that migration requires deleting the legacy manifest.

**Backend and scripting fixture paths point at a non-existent tree:**
- Issue: Several backend runtime and scripting tests read scripts from `../../../../packages/plugins/backend/core/**/src/main.luau`, but the repo contains live scripts under `modules/backend/**/src/main.luau` and docs-only plugin READMEs under `docs/plugins/**`.
- Files: `crates/core/runtime/backend/src/lib.rs`, `crates/core/runtime/scripting/src/backend.rs`, `modules/backend/pipewire-audio/src/main.luau`, `modules/backend/pulseaudio-audio/src/main.luau`, `docs/plugins/backend/core/reference-media/README.md`
- Impact: Tests for bundled audio, network, theme, and reference media providers panic before exercising runtime behavior.
- Fix approach: Update fixture paths to the current `modules/**` tree, add missing script fixtures for documented providers, or convert docs-only providers to committed module fixtures.

**Large modules concentrate unrelated responsibilities:**
- Issue: Several files combine parsing, validation, graph construction, runtime orchestration, and broad test suites in single modules.
- Files: `crates/core/shell/src/shell/mod.rs`, `crates/core/extension/plugin/src/package.rs`, `crates/core/runtime/scripting/src/context.rs`, `crates/core/runtime/scripting/src/backend.rs`, `crates/core/ui/elements/src/style.rs`
- Impact: Changes to module graph behavior, backend lifecycle, or scripting semantics require editing high-blast-radius files and scanning large embedded test sections.
- Fix approach: Extract graph validation, fixture loading, backend launch candidate construction, and scripting host APIs into smaller modules with focused tests.

## Known Bugs

**Backend runtime tests fail from missing provider script paths:**
- Symptoms: `cargo test -p mesh-core-backend -- --nocapture` runs 20 tests; 14 pass and 6 fail with `No such file or directory` from `std::fs::read_to_string(...).unwrap()`.
- Files: `crates/core/runtime/backend/src/lib.rs`, `modules/backend/pipewire-audio/src/main.luau`, `docs/plugins/backend/core/reference-media/README.md`
- Trigger: Run `cargo test -p mesh-core-backend -- --nocapture`.
- Workaround: Run focused runtime tests that do not read `packages/plugins/**`, or restore/update the missing fixtures.

**Scripting bundled-provider tests fail from missing provider script paths:**
- Symptoms: `cargo test -p mesh-core-scripting reference_media -- --nocapture` fails 2 tests, and `cargo test -p mesh-core-scripting bundled -- --nocapture` fails 3 tests, all from the helper that reads `packages/plugins/backend/core/**`.
- Files: `crates/core/runtime/scripting/src/backend.rs`, `modules/backend/pipewire-audio/src/main.luau`, `modules/backend/pulseaudio-audio/src/main.luau`
- Trigger: Run the `reference_media` or `bundled` test filters in `mesh-core-scripting`.
- Workaround: Run `cargo test -p mesh-core-scripting exec_ -- --nocapture` for the exec-capability subset; it passes independently of missing fixtures.

**Shell module graph tests encode stale package choices:**
- Symptoms: `installed_module_graph_exposes_shell_package_choices` expects active `mesh.network`, active `mesh.power`, and layout module `@mesh/panel`, but `config/package.json` contains only audio providers and `@mesh/navigation-bar`.
- Files: `crates/core/shell/src/shell/mod.rs`, `config/package.json`, `config/modules/@mesh/networkmanager/package.json`, `config/modules/@mesh/upower/package.json`
- Trigger: Run the shell tests in an environment with Wayland build dependencies available.
- Workaround: Use `cargo test -p mesh-core-plugin package -- --nocapture` for graph tests that match the current `config/package.json`; it passes 26 tests.

## Security Considerations

**Backend command execution has no timeout or output limit:**
- Risk: `mesh.exec()` calls `StdCommand::new(program).args(args).output()` synchronously inside the backend script runtime. A granted backend can hang the async backend task with a long-running command or consume memory with unbounded stdout/stderr.
- Files: `crates/core/runtime/scripting/src/backend.rs`, `crates/core/runtime/backend/src/lib.rs`, `modules/backend/pipewire-audio/src/main.luau`, `modules/backend/pulseaudio-audio/src/main.luau`
- Current mitigation: Command form requires program plus args, single-string shell form is rejected in tests, and capabilities are checked per binary with `exec.<binary>` or broad `exec.command`.
- Recommendations: Run command execution behind a timeout, cap captured output, and prefer an async process wrapper so a backend poll cannot block the backend event loop.

**Non-core interface capability checks are permissive:**
- Risk: `InterfaceProxy::can_read()` allows non-`mesh.` interfaces without a `service.*` capability. This is explicitly documented in code as transition behavior.
- Files: `crates/core/runtime/scripting/src/host_api.rs`, `crates/core/runtime/scripting/src/context.rs`, `crates/core/extension/service/src/interface.rs`
- Current mitigation: Core `mesh.*` interfaces require `service.<name>.read` or `service.<name>.control`, and unavailable interface lookups produce diagnostics.
- Recommendations: Load contract-level capabilities for all interface modules and remove the fallback that grants reads to non-core interfaces.

**Executable path capabilities are basename-based:**
- Risk: `exec_program_capability()` derives capability from the executable basename, so any absolute or relative path ending in an allowed basename maps to the same capability.
- Files: `crates/core/runtime/scripting/src/backend.rs`, `modules/backend/pipewire-audio/package.json`, `modules/backend/pulseaudio-audio/package.json`
- Current mitigation: Bundled backend scripts call fixed program names such as `wpctl`, `pactl`, and `aplay`; single-string shell execution is not supported.
- Recommendations: Restrict `mesh.exec()` to bare binary names resolved through trusted PATH lookup or explicitly declared binary dependency paths.

## Performance Bottlenecks

**Polling backends execute multiple synchronous commands every 500ms:**
- Problem: Audio providers poll at 500ms and call external commands on every poll; PipeWire runs `wpctl status` plus `wpctl get-volume`, while PulseAudio runs two `pactl` commands.
- Files: `modules/backend/pipewire-audio/src/main.luau`, `modules/backend/pulseaudio-audio/src/main.luau`, `crates/core/runtime/backend/src/lib.rs`
- Cause: Provider scripts use polling through `mesh.service.set_poll_interval(500)` and `mesh.exec()` instead of event subscriptions.
- Improvement path: Add event-driven provider APIs where available, cache resolved sink IDs, and increase default poll intervals for stable state.

**Backend update deduplication compares full JSON payloads:**
- Problem: Every emitted backend state is compared as a full `serde_json::Value` before publishing.
- Files: `crates/core/runtime/backend/src/lib.rs`
- Cause: `publish_changed_update()` stores the last payload and compares full payload equality on every update.
- Improvement path: Keep payloads small for current providers; for larger provider states, introduce provider-side revision fields or field-level diffing.

**Shell and render tests depend on system Wayland libraries:**
- Problem: Focused shell tests could not compile outside the Nix dev shell because `smithay-client-toolkit` requires `xkbcommon.pc`.
- Files: `crates/core/presentation/Cargo.toml`, `crates/core/frontend/render/Cargo.toml`, `flake.nix`, `crates/core/shell/Cargo.toml`
- Cause: Shell depends on render/Wayland crates and there is no headless test feature for lifecycle-only tests.
- Improvement path: Add a headless feature or split lifecycle tests into crates that do not compile Wayland/render dependencies.

## Fragile Areas

**Installed module graph validation is strict but fixture coverage is split:**
- Files: `crates/core/extension/plugin/src/package.rs`, `config/package.json`, `modules/frontend/navigation-bar/package.json`, `modules/backend/pipewire-audio/package.json`, `modules/backend/pulseaudio-audio/package.json`
- Why fragile: `InstalledModuleGraph::from_parts()` rejects missing modules, kind mismatches, disabled active providers, and provider/interface mismatches. That is good behavior, but docs, config fixtures, and shell tests currently target different expected graphs.
- Safe modification: Update `config/package.json`, live module manifests, and graph tests together; verify with `cargo test -p mesh-core-plugin package -- --nocapture`.
- Test coverage: Package graph tests pass, but shell graph tests are blocked here by missing `xkbcommon` and contain stale expectations.

**Backend lifecycle depends on both graph manifests and discovered plugin manifests:**
- Files: `crates/core/shell/src/shell/mod.rs`, `crates/core/extension/plugin/src/package.rs`, `crates/core/extension/plugin/src/manifest.rs`
- Why fragile: `backend_launch_candidates_from_graph()` validates graph nodes, then looks up runtime `PluginInstance` entries and falls back to legacy backend discovery if graph loading fails.
- Safe modification: Preserve provider IDs across graph nodes and plugin discovery, and add tests for mixed `package.json`/`package.json` discovery before changing loader precedence.
- Test coverage: Shell lifecycle tests cover missing entrypoints, disabled backends, active providers, and stale failures, but they require a system environment with Wayland build dependencies.

**Reference provider documentation has no committed implementation fixture:**
- Files: `docs/plugins/backend/core/reference-media/README.md`, `crates/core/runtime/backend/src/lib.rs`, `crates/core/runtime/scripting/src/backend.rs`
- Why fragile: The docs instruct authors to copy `@mesh/reference-media`, and tests claim to verify it, but the referenced script path is absent.
- Safe modification: Commit `modules/backend/reference-media/package.json` and `modules/backend/reference-media/src/main.luau`, then update docs/tests to that path.
- Test coverage: Reference media test filters currently fail before executing provider behavior.

## Scaling Limits

**Single root graph has no lockfile-backed installer path:**
- Current capacity: `config/package.json` can load the modules explicitly listed in `mesh.modules`.
- Limit: Adding registry resolution, version reconciliation, staged installs, signatures, and rollback described in `docs/installation.md` requires new installer infrastructure; current code loads local manifests only.
- Scaling path: Implement package resolution and lockfile writing around `RootPackageManifest` and `InstalledModuleGraph` before treating `docs/installation.md` CLI flows as supported.

**Backend provider startup is single-active per interface:**
- Current capacity: Multiple providers are indexed and sorted, but only the explicit active provider is launched for an interface.
- Limit: Multi-active provider categories such as icons/locales and fallback startup semantics require separate graph policy; the shell explicitly does not auto-start fallback backends after active provider failure.
- Scaling path: Keep backend interfaces single-active, and model multi-active resources through `mesh.contributes` indexes instead of backend lifecycle slots.

## Dependencies at Risk

**`smithay-client-toolkit` / `xkbcommon`:**
- Risk: Shell tests and shell compilation require system `xkbcommon` discovery through `pkg-config`; without it, the build script panics before tests run.
- Impact: Contributors outside `nix develop` cannot run shell-level tests even when touching only module graph or backend lifecycle code.
- Migration plan: Document `nix develop` as required for shell/render tests, and split module graph/lifecycle unit tests away from render dependencies where possible.

**Legacy manifest formats:**
- Risk: `package.json`, `plugin.json`, and `mesh.toml` remain loadable while docs promote `package.json`, and loader precedence can make a stale legacy file override a new package file.
- Impact: A migration can appear complete in docs/config while runtime still reads legacy manifests.
- Migration plan: Add deprecation warnings with `ModuleManifestSource::LegacyPluginJson`, prefer `package.json` after a compatibility window, and add tests for directories containing multiple manifest files.

## Missing Critical Features

**Library module resolver is not implemented:**
- Problem: `docs/module-system.md` describes `library` modules and recommends adding a library resolver, while runtime support only indexes contributed libraries in the installed graph.
- Blocks: Backend and frontend modules cannot reliably import shared Luau libraries such as `@mesh/backend-kit` or `@mesh/audio-interface/audio_types`.

**Installer CLI described in docs is not implemented:**
- Problem: `docs/installation.md` documents install/update/pin/search/doctor flows, lockfiles, staged installs, signatures, and rollback, but the current codebase exposes manifest loading and local graph construction.
- Blocks: Third-party module installation, package registry integration, reproducible installed state, and user provider pinning through the documented CLI.

**Committed interface packages are missing from the active graph:**
- Problem: Docs describe interface modules such as `@mesh/audio-interface`, `@mesh/network-interface`, and `@mesh/power-interface`, but the active `config/package.json` does not install interface modules.
- Blocks: Full contract-driven validation and non-core capability enforcement for provider/consumer interactions.

## Test Coverage Gaps

**Current module graph fixture is only covered in `mesh-core-plugin`:**
- What's not tested: Shell-level expectations for the actual `config/package.json` after the module-system change.
- Files: `crates/core/extension/plugin/src/package.rs`, `crates/core/shell/src/shell/mod.rs`, `config/package.json`
- Risk: Package graph tests pass while shell tests expect a different graph.
- Priority: High

**Manifest coexistence behavior lacks explicit coverage:**
- What's not tested: Directories containing both `package.json` and `package.json`.
- Files: `crates/core/extension/plugin/src/package.rs`, `crates/core/extension/plugin/src/manifest.rs`, `modules/frontend/navigation-bar/package.json`
- Risk: A package migration can silently keep using `package.json`.
- Priority: High

**Backend provider fixture tests do not cover current live module paths:**
- What's not tested: `modules/backend/pipewire-audio/src/main.luau` and `modules/backend/pulseaudio-audio/src/main.luau` through the backend runtime tests.
- Files: `crates/core/runtime/backend/src/lib.rs`, `crates/core/runtime/scripting/src/backend.rs`, `modules/backend/pipewire-audio/src/main.luau`, `modules/backend/pulseaudio-audio/src/main.luau`
- Risk: Live bundled provider scripts can regress while tests panic on missing old paths.
- Priority: High

---

*Concerns audit: 2026-05-06*
