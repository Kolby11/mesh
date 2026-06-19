# MESH — Active Backlog

Items marked `→ vX.Y` are tracked as GSD milestones in `.planning/ROADMAP.md`.

---

## Shell features

- [ ] Icon rendering using icon packs — XDG resolution and SVG rasterization pipeline needs end-to-end proof on a real module surface
- [ ] Layer system — specify which Wayland layer (background/bottom/top/overlay) a surface targets; needed for proper popover/overlay stacking
- [ ] Positioning system — `position: relative / absolute / fixed` in layout and paint; needed for tooltips, context menus, dropdowns → v1.22
- [ ] Settings module — surface for managing installed modules, active providers, theme, i18n → v1.22
- [ ] Popups / overlays — transient surfaces with custom content and dismiss behavior → v1.22
- [ ] Clean up backend modules and interfaces — consider moving the interface contract declaration from the separate `modules/interfaces/` directory into the implementing backend module, or bundling it as core metadata; evaluate impact on multi-provider resolution before changing

### Module system redesign research — 2026-06-18

High-level direction:

- [ ] Keep the current core rule: a module is the installable unit, an interface is the contract, a backend provider implements the contract, a frontend consumes the contract, and libraries/resource packs are reusable dependencies.
- [x] Do not preserve old conventions for compatibility. Prefer a clean canonical module model over long-lived legacy aliases/shims; migrate shipped modules and remove old public shapes instead of supporting both indefinitely. First enforcement slice landed 2026-06-18: module loading now rejects legacy `package.json`, `mesh.toml`, and old top-level `id/type/api_version` `module.json`; root graph loading now requires canonical `name/version/mesh`.
- [ ] Keep `config/module.json` / future user root graph lean: installed modules, enabled flags, active providers, active layout entrypoint, active theme/mode. Do not add root buckets for frontend dependencies, backend dependencies, icons, fonts, i18n, etc.
- [x] Make `InstalledModuleGraph` the single runtime source of truth for shell graph consumers. Shell now caches the installed graph and shares it across interface registration, frontend filtering, graph-declared i18n catalogs, backend launch, resources, and diagnostics. Remaining future cleanup: replace recursive module discovery with graph-first `ModuleInstance` loading instead of only using the graph as the authoritative filter/source of truth.
- [x] Simplify author-facing `module.json` around two questions: `mesh.uses` for dependencies/capabilities/resources and `mesh.provides` for contributions/settings/resources. Normalize that author shape into the existing internal `dependencies`, `capabilities`, `entrypoints`, and `contributes` structs. Backend provider contracts remain under `mesh.implements` so `provides` is no longer overloaded.

Recommended author-facing shape:

```json
{
  "name": "@mesh/example",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "frontend",
    "entry": "src/main.mesh",
    "uses": {
      "interfaces": { "mesh.audio": ">=1.0" },
      "modules": { "@mesh/audio-popover": ">=0.1.0" },
      "resources": { "icons": ["@mesh/icons-default"] },
      "capabilities": ["shell.surface", "service.audio.read"]
    },
    "provides": {
      "layout": [{ "id": "main", "entry": "src/main.mesh" }],
      "settings": { "schema": {} },
      "i18n": [{ "locale": "en", "path": "config/i18n/en.json" }]
    }
  }
}
```

Component usage contracts to preserve and clarify:

- [ ] Frontend surface modules declare a `.mesh` entrypoint, surface settings schema, surface layout policy, accessibility metadata, required component-module imports, interface dependencies, icon requirements, i18n catalogs, and keybind actions. Today these are spread across `entrypoints`, `dependencies.modules`, `dependencies.icons`, `iconRequirements`, `contributes.i18n`, `contributes.layout`, `contributes.settings`, `surfaceLayout`, and `accessibility`; collapse the authoring story while keeping typed graph indexes. Progress: graph now emits a typed frontend surface record for main entrypoint + settings namespace + accessibility + surface layout, reports `missing_frontend_surface_layout` / `missing_frontend_accessibility`, and exposes the surface contract through `mesh.debug` module graph entries.
- [x] Frontend component imports are a good pattern: `require("@mesh/audio-popover")` and `require("./components/volume-button.mesh")` are explicit, and cross-module component imports are already validated against declared module dependencies. Keep this rule and make the diagnostic author-facing. Catalog diagnostics now point authors to explicit script imports and `mesh.uses.modules`.
- [x] Interface usage should be explicit dependencies, not only capabilities. If a script calls `require("mesh.audio@>=1.0")`, the manifest should declare `uses.interfaces["mesh.audio"]`; capabilities grant read/control power, but the interface dependency declares the contract being consumed. Frontend catalog validation now rejects undeclared enabled-graph `mesh.*` imports.
- [x] Optional interface usage needs first-class syntax. Example: `@mesh/quick-settings` tries `mesh.brightness@>=1.0`, then falls back to shell events, but no brightness interface/provider is installed in the root graph. Implemented manifest syntax as `uses.optionalInterfaces`; debug diagnostics now expose optional unavailable/inactive status and per-module optional interfaces. Remaining work is presenting fallback/health in settings UI.
- [x] Backend provider modules declare one or more implemented interfaces, native binary requirements, exec capabilities, optional stream tools, settings schema, and lifecycle entrypoint. Example: `@mesh/hyprland-wm` needs `hyprctl`, optional `socat`/`nc`, emits `mesh.hyprland` state/events, and implements `switch_workspace`. Provider graph diagnostics now require `baseModule` contracts to be declared in `mesh.uses.modules`; shipped backends declare their interface modules alongside binaries, capabilities, optional tools, entry, and implements metadata.
- [x] Interface modules are contract packages: state fields, methods, events, capability requirements, version, domain, and relationship. Keep them data-only and generic. Decide later whether core interfaces stay in `modules/interfaces/*` or can be bundled with providers without breaking multi-provider resolution. Graph diagnostics now emit `missing_interface_contract_file` when the declared contract file is absent on disk.
- [x] Library modules should be first-class. Common D-Bus helpers, polling/stream parsing, command result shaping, formatting helpers, and reusable UI utilities should be provided as `kind: "library"` modules and imported with `require("@mesh/backend-kit")` or similar. Libraries must not grant capabilities; consuming modules still request capabilities. Validated: library modules declaring `mesh.capabilities.required` are now rejected at parse time.

Icons and resources:

- [x] Treat semantic icon names as a module contract. Frontends use `<icon name="audio-volume-high" />`; icon packs map semantic names to concrete theme assets. Manifests should declare required and optional semantic icons, and the graph should diagnose missing mappings. Graph diagnostics now emit `missing_required_icon` even when no icon pack is enabled and `missing_optional_icon` for optional names that no enabled icon pack maps.
- [x] Add tooling/linting to extract static and obvious dynamic icon names from `.mesh` files and compare them to `iconRequirements`. Current gap fixed 2026-06-18 by adding `battery-caution` to default/material icon packs; linting added 2026-06-18 via `undeclared_icon_use` graph diagnostic; scanner upgraded to recursive (covers `src/components/`).
- [x] Keep icon pack modules ordinary modules. Users should be able to swap the active icon pack or install multiple packs; module code should not depend on a concrete icon theme. Manifest validation now restricts `mesh.icon_pack` to `kind: "icon-pack"` modules; graph tests cover multiple enabled icon-pack modules contributing mappings side by side, with frontend pack selection remaining module-id based.
- [x] Apply the same pattern to future font packs, sound packs, language packs, theme packs, and other resource packs: frontend/backend modules depend on resource capabilities by semantic id, resource modules contribute concrete files/mappings. Font/theme/icon contribution fields are now kind-scoped to ordinary resource-pack modules (`font-pack`, `theme`, `icon-pack`) while bundled i18n catalogs remain valid on normal modules and standalone language packs use the same catalog contribution shape.

Capabilities, permissions, and dependencies:

- [x] Separate dependencies from capabilities in docs and validation. Dependencies say what must be present: modules, interfaces, resource packs, binaries. Capabilities say what host power is granted: `service.audio.read`, `service.audio.control`, `exec.hyprctl`, `locale.write`, `theme.read`. `mesh.uses` validation now rejects module/resource dependencies that are not module ids, interface dependencies that are module ids or non-dotted names, and capabilities that look like module or interface dependencies.
- [x] Validate capability/interface alignment. If an interface contract requires `service.power.read`, a provider should declare it; a frontend requiring `mesh.power` should declare the read capability if it reads proxy state and control capability if it calls methods. Graph diagnostics now load contract `[capabilities].required` from interface modules and emit `missing_provider_required_capability` / `missing_interface_required_capability` for enabled providers and frontends.
- [x] Keep native binary metadata as install/runtime health input. Backend binary deps now drive install checks, runtime health, diagnostics, backend lifecycle status, and package-manager suggestions from one manifest declaration.
- [x] Add health propagation to the graph/runtime story: missing required binary -> backend unavailable; selected provider unavailable -> interface unavailable; frontend sees interface health, not provider-specific failure handling. Graph health records now derive provider/interface/frontend status from required-binary diagnostics and active provider selection; debug module graph entries expose health; backend candidate statuses surface frontend-facing interface outages.

Backends, interfaces, and service events:

- [x] Prefer interface method calls over raw event channels for domain commands. Example: `wm.switch_workspace(1)` is better than `mesh.events.publish("mesh.hyprland.switch_workspace", ...)`; raw events should be shell/system commands or explicit extension channels. Navigation workspace switching now uses only the `mesh.hyprland` proxy method; graph diagnostics scan static `.mesh` sources and emit `raw_interface_domain_event_publish` for `mesh.events.publish("mesh.*", ...)`.
- [ ] Make event channels typed and declared. Backend `mesh.service.emit_event("WorkspaceChanged", payload)` should be validated against the interface contract; frontend `audio.VolumeChanged:on(...)` should be known to the compiler/diagnostics. Progress: graph diagnostics now load interface `[[events]]` declarations and emit `undeclared_interface_event_emit` when backend Luau statically calls `mesh.service.emit_event("...")` with an event name absent from the implemented interface contract.
- [x] Keep shell-owned events (`shell.position-surface`, `shell.set-theme`, `shell.activate-popover`, debug commands, shutdown) separate from interface-domain events (`mesh.audio.*`, `mesh.hyprland.*`). Document which namespace module authors can publish to. Docs now list the declared shell-owned channels; graph diagnostics emit `raw_interface_domain_event_publish` for static `mesh.*` publishes and `unknown_shell_event_publish` for undeclared static `shell.*` publishes.
- [ ] Eliminate service-specific Rust branches where possible. Current audio optimistic state and some debug/profiling paths are pragmatic, but new module domains should route through interfaces/contracts/providers.

Keyboard and interaction:

- [x] Preserve the current keybind flow: manifest declares `mesh.keybinds.<action>`, template nodes subscribe with `keybind="{this.keybinds.action.id}"` and `onkeybind={handler}`, runtime resolves user overrides and locale defaults, then annotates accessibility/debug metadata. Graph diagnostics now scan static `.mesh` keybind subscriptions and emit `undeclared_keybind_subscription` / `keybind_subscription_missing_handler` when template usage falls out of sync with manifest declarations.
- [x] Move all legacy settings-derived shortcut declarations behind migration diagnostics. New modules should declare keybind identity, label, description, category, default trigger, localized triggers, and scope in the manifest. Settings-only shortcut declarations no longer create effective keybinds; legacy settings for manifest-declared ids are reported as ignored so authors migrate the full action metadata into `mesh.keybinds`.
- [x] Keep locale-specific access keys. `localizedTriggers` are useful for different keyboard mnemonics by language; validation should report missing/empty keys and duplicate effective bindings. Graph diagnostics now emit `duplicate_keybind_trigger` for cross-module trigger collisions.
- [x] Make popover/focus ownership a documented module pattern. `mesh.popover.activate(surface_id, event, { focus = true })` registers trigger return focus and can temporarily transfer keyboard mode; modules need clear rules for when a popover owns focus and when it closes on focus leave. Docs now define the trigger-event requirement, `options.focus`, return-focus tracking, close-on-focus-leave, and the rule that shell owns focus state while modules request activation/hide.
- [x] Surface layout settings should describe keyboard behavior (`none`, `on_demand`, `exclusive`) and size policy, but runtime keyboard focus state should remain shell-owned. `mesh.surfaceLayout.keyboard_mode` now provides the module default, shipped frontend manifests declare keyboard policy, user settings still override durable policy, and docs keep runtime focus state owned by the shell.

I18n and localized text:

- [x] Make i18n graph-driven. Frontend components now load translation catalogs from installed graph `contributed_i18n()` paths instead of scanning `config/i18n/{locale}.json` by convention.
- [x] Keep the localized manifest text shape `{ "t": "key.path", "fallback": "Text" }`. Raw strings are literals, not translation keys. Loader diagnostics now warn when keybind text or layout labels look like dotted i18n keys but were written as raw strings, and localized layout labels validate the same `t`/`fallback` shape.
- [x] Resolve localized manifest metadata through the active locale for script descriptors, keybind debug data, settings UI labels, layout labels, provider labels, and resource labels. Provider and resource labels now use LocalizedText types; debug snapshot exposes resolved labels with key/fallback metadata; resolve_manifest_text and resolve_debug_manifest_text use module-scoped translate_for_module lookup.
- [x] Let modules declare supported locales and contributed catalogs once. Template `t("key")`, manifest localized text, keybind labels, and settings labels should all use the same module-scoped catalog source. LocaleEngine gains load_module_translations and translate_for_module; graph catalog loading is module-scoped; redundant_supported_locales diagnostic guides authors to declare once in provides.i18n; navigation-bar supportedLocales removed.
- [x] Add diagnostics for catalogs that do not cover template `t(...)` keys or manifest translation keys. Graph diagnostics now emit `undeclared_i18n_key` for static `t('key')` calls in `.mesh` templates that are absent from the module's default-locale catalog. Scanner is recursive (covers `src/components/`). `nav.live` in navigation-bar was the first real bug caught.

Customization and reusability:

- [ ] Treat manifests as defaults and user config as overrides. Module authors provide settings schema/defaults; users choose active provider, layout entrypoint, theme, icon pack, locale, and per-module settings in the root graph/settings files.
- [ ] Support multiple instances of the same frontend module later. Module identity should not be the only surface identity; root graph should eventually support configured instances like two panels or repeated widgets with separate settings/storage scopes.
- [ ] Keep `self.storage` scoped to module/component/provider instance and use it for durable per-instance state, not installed graph state.
- [ ] Settings UI should be generated from contributed schemas by default, with optional custom `settings_ui` entrypoint for advanced modules.
- [ ] Diagnostics/settings UI should show each module's uses/provides graph: required interfaces, active provider, optional interfaces, required icons, native binaries, capabilities, settings namespace, i18n catalogs, keybinds, health. The `mesh.debug` snapshot now exposes a typed `module_graph` payload with installed module uses/provides, optional interfaces, icons, capabilities, settings namespaces, i18n catalogs, and graph diagnostics. The debug inspector now has a Modules tab that renders the first graph entries; remaining work is a full settings UI with filtering, active-provider detail, native binary health, keybinds, and per-module customization controls.

Concrete cleanup tasks:

- [x] Remove legacy manifest/field conventions after migration rather than preserving compatibility behavior. Examples completed 2026-06-18: top-level `id/type/api_version`, legacy top-level `settings`, `icon_requirements`, `surface_layout`, `package.json`, `mesh.toml`, and backend-provider `provides` as public vocabulary.
- [x] Add `mesh.uses` / object-shaped `mesh.provides` canonical facade and normalize it into existing manifest structs.
- [x] Migrate remaining legacy-shaped shipped manifests (`audio-popover`, `debug-inspector`, `text-selection-proof`, `material-symbols`) to canonical top-level `name` + `mesh`.
- [x] Make graph loading a cached shell runtime object shared by discovery, frontend catalog, interface registry, backend launch, resources, and diagnostics.
- [x] Replace direct `config/i18n` scanning with graph `contributed_i18n()` loading.
- [x] Add interface-dependency validation for `require("mesh.*")`.
- [x] Add icon-requirement lint for shipped frontends and fix `battery-caution` in the default icon pack or the battery component.
- [x] Add optional interface dependency support and update `quick-settings` brightness behavior to declare `mesh.brightness` optional.
- [x] Expose installed module uses/provides graph through the debug snapshot and `mesh.debug` service payload.
- [x] Add a debug-inspector Modules tab that renders installed module uses/provides summaries from `mesh.debug.module_graph`.
- [x] Document the module author workflow with examples for frontend surface, backend provider, interface module, library module, icon pack, language pack, and theme pack.

---

## Performance — remaining open items

Items owned by a milestone are listed with their milestone reference.

### P0 — scheduling and invalidation (→ v1.18 / v1.19)

- [ ] Replace fixed 16ms shell loop sleep with event/deadline-driven scheduler; remaining work: block on real Wayland/frame-callback wakeups instead of bounded polling → v1.19
- [ ] Stop broadcasting every backend service event to every component; first pass (observes_service_event) done; remaining: route by tracked fields / module dependencies → v1.18
- [ ] Narrow script/service invalidation below tree-rebuild + pixel repaint; add typed state dependencies → v1.18
- [ ] Avoid full-tree restyle for safe interaction changes; use selector-dependency analysis → v1.18

### P0 — scripting (→ v1.17)

- [ ] One `mlua::Lua` VM per ScriptContext (`runtime.rs:92`); move to per-thread VM with `_ENV` isolation → v1.17
- [ ] Bound instance proxy deep-clones full snapshot Value per component mount (`runtime.rs:284`); use Arc<Value> or metatable proxy → v1.17
- [ ] Tracked-fields and side-channel maps still cloned per state sync (`runtime.rs:202-203, 1021`); remaining: wrap in Arc and use copy-on-write → v1.17

### P1 — renderer hot paths

- [ ] Interaction frames still re-apply string style declarations per node (`apply_declaration_no_diagnostics` + theme defaults maps dominate the post-2026-06-10 toggle profile); folds into the typed/compiled declaration work → v1.23 and narrower invalidation → v1.18

- [ ] Avoid flattening retained display-list subtrees into a new flat command buffer on each update; move toward segment/rope-style command storage → v1.21
- [ ] `StyleNodeAttrs::from_node` re-splits class strings per restyle; cache split classes on the retained `WidgetNode` once attribute mutation goes through an invalidating API → v1.23
- [ ] Replace per-node string/hash-heavy style matching with interned/typed node keys; remaining after first pass: interned tags, classes, attribute keys → v1.23
- [ ] Improve text ellipsis clipping: compute truncation from shaped glyph advances instead of measuring substrings on first miss
- [ ] Retain Taffy node state across layout passes; `build_taffy_tree` rebuilds a fresh TaffyTree every layout → v1.21
- [ ] Affected-subtree template re-evaluation: `narrow_script_update` rebuilds the full tree (full template eval) then diffs; use `NodeServiceFieldDependencies` to re-evaluate only nodes whose tracked fields changed → v1.27
- [ ] Generation-aware retained-tree diff: `RetainedWidgetTree::update` FNV-hashes every node's style + attribute strings per paint; skip clean subtrees using dirty bits → v1.27
- [ ] Fuse the five per-frame `finalize_tree` annotation walks into one traversal; move hot annotations from string attributes to typed `WidgetNode` fields → v1.27

### P1 — backend modules

- [ ] Investigate `pw-dump --monitor` as a real volume event source for the pipewire-audio backend — `pw-mon` emits no `changed:` block for volume changes (verified with and without `--hide-params`), so the stream currently only signals client/object lifecycle, and volume detection leans on the safety poll
- [ ] Audit the other exec-polling backends (pulseaudio-audio still polls 2× `pactl` at 100ms) for the same exec-storm pattern fixed in pipewire-audio on 2026-06-10

### P1 — presentation and memory (→ v1.20)

- [ ] Preserve surface configuration state: remaining dirty-bit work so unchanged size/options skip config construction entirely → v1.20 (surface_id clone now skipped on stable frames — 2026-06-02)
- [ ] Track damage as multiple rects deeper into the retained renderer → v1.20
- [ ] Add performance profiles for canonical shell workloads (idle, pointer move, text update, scroll, icon grid, animation, theme reload, resize) → v1.21
- [ ] Send `wl_surface::set_opaque_region` from the present path; compute union of fully-opaque background rects from retained display list → v1.19
- [ ] Wire `wp_blur_v1` / `org_kde_kwin_blur_v1` for backdrop-filter blur regions → v1.20
- [ ] HiDPI: plumb `wp_fractional_scale_v1` + `wp_viewporter`; render at native pixel density → v1.20

### P2 — architecture

- [ ] Introduce interned `Symbol` / `TagId` types before further string-key cleanups → v1.23
- [ ] Add allocator-level profile mode (allocation counts per render pass) → v1.23
- [ ] Consider typed runtime node representation for hot paths (`WidgetNode` tag/attrs/content as strings today) → v1.23
- [ ] GPU rendering — after retained layout, smart invalidation, and damage tracking ship → v1.25

---

## Completed (recent)

- [x] Per-frame clone/parse batch from 2026-06-10 deep dive: (1) template expressions were re-parsed by the string interpreter on every evaluation — `eval_expr` now compiles to a memoized AST per expression string (thread-local cache, parse once per source string); (2) full `Theme` (token/defaults maps) was deep-cloned into `active_theme` on every tree build and retained restyle — now `Arc<Theme>` refreshed only when `theme_changed()` marks it stale, and the child-build/animation readers clone the Arc instead of the maps; (3) `runtime_state()` deep-cloned the whole script variable map per tree build — now an `Arc<ScriptState>` snapshot cached by a new `mutation_generation` counter (bumps on `set`, `set_host_value`, proxy register/unregister; safe because `Clone` drops proxies), `ScriptState` made `Sync` (Mutex snapshot cache, `Send + Sync` proxy closures); (4) added `[profile.release]` thin LTO + `codegen-units = 1` — the workspace had no release tuning at all — 2026-06-10
- [x] Interaction CPU burst (~50% spikes on click): perf-profiled a popover toggle storm; three per-frame full-tree passes dominated — `parse_transition_shorthand` re-parsed the same shorthand strings per node per frame (14.5%, now memoized thread-locally), `record_runtime_style_diagnostics` re-resolved every node's style a second time per restyle frame (9%, now runs only on tree rebuild), and `publish_element_metrics` built a full per-element JSON snapshot into Luau state every paint even though no shipped module reads `refs`/`elements` (11%, now gated on script usage detected at compile/reload). Measured ~87ms CPU/toggle after vs ~418ms before on the same hardware state — 2026-06-10
- [x] pipewire-audio backend exec storm: each `wpctl` run registered a PipeWire client, whose pw-mon Client added/removed event re-triggered `refresh_state()` → another `wpctl` — a self-sustaining loop (~22 spawns/sec, ~90% of a core in child CPU, plus ~6% in mesh-shell from constant wakeups). Fixed with a self-noise counter + batch classifier in `on_stream_batch` (refresh only on `changed:`/non-Client `added:`/unaccounted external client connects) and safety poll relaxed 100ms → 1000ms (250ms when pw-mon unavailable) — 2026-06-10
- [x] `surface_id.clone()` on every render frame for LayerSurfaceConfig namespace; now only clones when config actually changes — 2026-06-02
- [x] `format!("{:.2}")` allocated new String for slider value and scroll offsets every annotation; now writes into retained entry buffer — 2026-06-02
- [x] All P0/P1 items from the 2026-05-27 shell performance audit and 2026-05-28 Skia canvas pass — see git log
