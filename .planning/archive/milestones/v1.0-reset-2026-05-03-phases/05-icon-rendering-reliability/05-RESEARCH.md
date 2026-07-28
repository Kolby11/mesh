# Phase 05: Icon Rendering Reliability - Research

**Researched:** 2026-05-03
**Domain:** Rust UI icon resolution, XDG icon theme lookup, SVG/raster painting, diagnostics
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
## Implementation Decisions

### Lookup Source of Truth
- **D-01:** Phase 5 should introduce an icon contract based on semantic MESH icon names, not hardcoded XDG names in surface code.
- **D-02:** Semantic names such as `audio-muted` resolve through user-selectable mapping profiles. Each mapping profile can target a different visual style, such as rounded, filled, or lucide-style icons.
- **D-03:** Icon mapping configuration should live in dedicated icon config files, separate from generic shell settings, so users can switch mappings deliberately.
- **D-04:** One dedicated icon config should own both configured icon pack roots and mapping profiles.
- **D-05:** Installed icon packs should be discovered from explicit configured pack IDs and filesystem roots. Phase 5 should not rely on fuzzy automatic XDG theme discovery as the source of pack identity.
- **D-06:** Each semantic icon mapping is an ordered fallback list. The first available icon wins; if all candidates miss, the missing-icon behavior below applies.
- **D-07:** Frontend plugins or components that use icons should declare the semantic icons and icon packs they depend on so missing assets can raise clear diagnostics.
- **D-08:** Plugin loading should not fail only because an icon is missing. Missing required semantic icons warn, degrade, and render fallback.

### Missing-Icon Behavior
- **D-09:** Unresolved icons should render a neutral placeholder glyph, emit diagnostics, and keep layout stable.
- **D-10:** Missing-icon diagnostics should deduplicate by plugin plus semantic icon name. They must not be emitted on every paint frame.
- **D-11:** Core should own one tiny built-in vector fallback so missing-icon rendering never depends on the configured icon system working.
- **D-12:** Unresolved icons should mark plugin health as warning/degraded while keeping the plugin active.

### Rendering Fidelity
- **D-13:** Icons inherit the surrounding color by default, like text.
- **D-14:** Multicolor assets, such as app logos or flags, can opt out of inherited tinting and preserve original colors.
- **D-15:** Layout and CSS box size win for rendered size. The `size` attribute is only a lookup hint for choosing candidate assets.
- **D-16:** Phase 5 needs practical monochrome SVG support: rasterize common symbolic SVGs and tint by alpha mask. Full complex SVG fidelity is not required in this phase.
- **D-17:** Raster icons should also use alpha-mask tint unless marked multicolor.

### Proof Surfaces
- **D-18:** Phase 5 proof should cover panel, quick settings, and navigation bar.
- **D-19:** The proof must include SVG rendering, raster rendering, and missing-icon fallback.
- **D-20:** Config-level profile switching is enough for Phase 5. No visible UI switch is required.
- **D-21:** Shipped core surfaces should use semantic icon names only. Pack-specific filenames and filesystem paths belong in icon mapping/config, not in `.mesh` call sites.

### the agent's Discretion
- The planner may choose the exact dedicated config format, as long as it supports configured pack roots, named mapping profiles, one active profile, and ordered fallback lists.
- The planner may choose the exact built-in vector fallback shape, as long as it is neutral, layout-stable, and independent of icon pack resolution.
- The planner may choose the exact diagnostic data structure and cache invalidation strategy, as long as diagnostics are deduplicated by plugin plus semantic name and profile switching can be verified.

### Deferred Ideas (OUT OF SCOPE)
## Deferred Ideas

- A user-facing UI for switching icon profiles is deferred. Phase 5 only needs config-level switching.
- Full complex SVG fidelity, including gradients, masks, embedded images, and complete multicolor rendering semantics, is deferred.
- A broader icon-pack registry with automatic provider chains can build on this phase later; Phase 5 should use explicit configured pack roots.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| ICON-01 | XDG icon names resolve through configured icon theme search paths. | Use `mesh-core-icon` as the facade, load explicit configured pack roots/profiles, and use the existing `icon` crate for spec-aware theme matching from configured roots. [VERIFIED: .planning/REQUIREMENTS.md] [VERIFIED: crates/core/ui/icon/Cargo.toml] [CITED: https://docs.rs/icon/latest/icon/] |
| ICON-02 | SVG icons rasterize correctly through the render pipeline. | Keep `resvg` in `mesh-core-render`; render SVGs into a `tiny_skia::Pixmap`, then tint by alpha unless multicolor is set. [VERIFIED: crates/core/ui/render/src/surface/icon.rs] [CITED: https://docs.rs/resvg/latest/resvg/] |
| ICON-03 | Raster icons decode and paint correctly at requested sizes. | Use `image::open`, `to_rgba8`, and `imageops::resize`; rendered dimensions must come from layout box width/height, not the `size` lookup hint. [VERIFIED: crates/core/ui/render/src/surface/icon.rs] [CITED: https://docs.rs/image/latest/image/imageops/enum.FilterType.html] |
| ICON-04 | Missing icons produce diagnostics and non-crashing fallback behavior. | Add a missing-icon path that records one diagnostic per `(plugin_id, semantic_name)` and paints a built-in vector fallback into the existing icon box. [VERIFIED: crates/core/foundation/diagnostics/src/lib.rs] [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md] |
</phase_requirements>

## Summary

Phase 5 should treat `mesh-core-icon` as the stable icon subsystem boundary and stop exposing pack-specific names to shell surfaces. [VERIFIED: crates/core/ui/icon/src/lib.rs] [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md] The current code already parses `<icon name="...">`, paints named icons from `render_icon_node()`, resolves simple names/paths, decodes raster images, rasterizes SVG with `resvg`, and caches decoded images. [VERIFIED: crates/core/ui/component/src/parser.rs] [VERIFIED: crates/core/ui/render/src/surface/painter.rs] [VERIFIED: crates/core/ui/render/src/surface/icon.rs] The missing pieces for this phase are semantic mapping profiles, explicit pack root configuration, plugin/icon dependency declarations, deduplicated missing-icon diagnostics, profile cache invalidation, and proof coverage for SVG, raster, and missing fallback. [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md] [VERIFIED: crates/core/foundation/diagnostics/src/lib.rs]

Use a dedicated TOML icon config because the repo already uses TOML/Serde for shell config and JSON/Serde for settings, and TOML ordered arrays map cleanly to fallback lists. [VERIFIED: crates/core/foundation/config/src/lib.rs] [CITED: https://docs.rs/toml/latest/toml/fn.from_str.html] Prefer the already-present `icon = 0.2.0` crate for XDG theme matching because its docs and local source say it implements the XDG icon theme specification, supports custom search directories through `IconSearch::new_from` and `add_directories`, and has cache-capable types behind its `cache` feature. [VERIFIED: Cargo.lock] [VERIFIED: /home/kolby/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/icon-0.2.0/src/search.rs] [VERIFIED: /home/kolby/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/icon-0.2.0/src/cache.rs] [CITED: https://docs.rs/icon/latest/icon/]

**Primary recommendation:** Implement a configured `IconRegistry`/resolver in `mesh-core-icon` that maps semantic names to ordered `pack_id:asset_name` candidates, resolves each candidate through configured pack roots using `icon::IconSearch`, returns a typed resolution result, and lets render code paint either SVG/raster output or the built-in fallback with deduplicated diagnostics. [VERIFIED: crates/core/ui/icon/src/lib.rs] [VERIFIED: crates/core/ui/render/src/surface/icon.rs] [CITED: https://docs.rs/icon/latest/icon/]

## Project Constraints (from AGENTS.md)

No `AGENTS.md` file exists in this repo, so there are no additional project-local directives from that file. [VERIFIED: find . -name AGENTS.md]

Project-local `.codex/skills` and `.agents/skills` directories were not found, so no project skill rules apply beyond the GSD researcher instructions. [VERIFIED: find .codex/skills .agents/skills -maxdepth 2 -name SKILL.md]

Graphify is disabled, so semantic graph context was unavailable for this research. [VERIFIED: node /home/kolby/.codex/get-shit-done/bin/gsd-tools.cjs graphify status]

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|--------------|----------------|-----------|
| Semantic icon mapping and profile selection | Core UI icon subsystem | Foundation config | Mapping is a shell-wide rendering contract, and config owns active profile/pack roots. [VERIFIED: crates/core/ui/icon/src/lib.rs] [VERIFIED: crates/core/foundation/config/src/lib.rs] |
| XDG file resolution | Core UI icon subsystem | Filesystem | The resolver is already centralized in `mesh-core-icon`, and the XDG spec defines filesystem theme lookup. [VERIFIED: crates/core/ui/icon/src/lib.rs] [CITED: https://specifications.freedesktop.org/icon-theme/latest/index.html] |
| SVG and raster painting | Core UI render subsystem | Core UI icon subsystem | Render code owns pixels and already performs SVG/raster decoding/drawing after resolution returns a path. [VERIFIED: crates/core/ui/render/src/surface/icon.rs] |
| Missing-icon diagnostics | Shell component runtime | Foundation diagnostics | Diagnostics need plugin identity, which exists in component mount/runtime context, while the diagnostics crate owns health state. [VERIFIED: crates/core/shell/src/shell/mod.rs] [VERIFIED: crates/core/shell/src/shell/component.rs] [VERIFIED: crates/core/foundation/diagnostics/src/lib.rs] |
| Surface proof | Frontend core plugins | Render tests | Panel, quick settings, and navigation bar already contain `<icon name=...>` call sites and must remain semantic. [VERIFIED: packages/plugins/frontend/core/panel/src/main.mesh] [VERIFIED: packages/plugins/frontend/core/quick-settings/src/main.mesh] [VERIFIED: packages/plugins/frontend/core/navigation-bar/src/components/volume-button.mesh] |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `mesh-core-icon` | workspace `0.1.0` | Facade for semantic config, profile resolution, XDG lookup, built-in fallback identity, and resolver cache. | It is the existing crate boundary for `resolve_icon(name, size)`. [VERIFIED: crates/core/ui/icon/src/lib.rs] [VERIFIED: Cargo.toml] |
| `icon` | `0.2.0` locked | Spec-aware XDG icon/theme lookup from configured directories. | It is already a dependency and documents full XDG spec support plus configurable search dirs. [VERIFIED: crates/core/ui/icon/Cargo.toml] [VERIFIED: Cargo.lock] [CITED: https://docs.rs/icon/latest/icon/] |
| `mesh-core-render` | workspace `0.1.0` | Software pixel painting for `<icon>` nodes. | It already owns `draw_named_icon`, `draw_icon_from_path`, and layout box painting. [VERIFIED: crates/core/ui/render/src/surface/icon.rs] [VERIFIED: crates/core/ui/render/src/surface/painter.rs] |
| `resvg` | `0.44.0` locked | SVG parsing/rasterization through `usvg` and `tiny_skia`. | It is already used by render code and re-exports `usvg`/`tiny_skia`. [VERIFIED: crates/core/ui/render/Cargo.toml] [VERIFIED: Cargo.lock] [CITED: https://docs.rs/resvg/latest/resvg/] |
| `image` | `0.24.9` locked | Raster decode and resize. | It is already used to decode to RGBA and resize with `FilterType::Lanczos3`. [VERIFIED: crates/core/ui/render/Cargo.toml] [VERIFIED: Cargo.lock] [CITED: https://docs.rs/image/latest/image/imageops/enum.FilterType.html] |
| `mesh-core-diagnostics` | workspace `0.1.0` | Plugin health and deduplicated warning/error state. | It already has per-plugin handles and a dedupe pattern for handler errors. [VERIFIED: crates/core/foundation/diagnostics/src/lib.rs] |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `serde` | `1.x` workspace | Deserialize icon config and manifest additions. | Use for typed TOML/JSON config structs. [VERIFIED: Cargo.toml] |
| `toml` | `0.8.23` locked | Dedicated icon config parsing. | Use for `config/icons.toml` or an override path. [VERIFIED: Cargo.lock] [VERIFIED: crates/core/foundation/config/src/lib.rs] |
| `tempfile` | `3.x` dev | Hermetic icon theme fixtures. | Use for tests that create pack roots, SVGs, PNGs, and missing candidates. [VERIFIED: crates/core/ui/icon/Cargo.toml] |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `icon` crate | Keep the current manual resolver | Manual lookup currently ignores `index.theme` semantics, inheritance, closest-size matching, and XDG scale behavior. [VERIFIED: crates/core/ui/icon/src/lib.rs] [CITED: https://specifications.freedesktop.org/icon-theme/latest/index.html] |
| TOML icon config | JSON settings file | JSON works locally, but TOML is already used for shell config and is more readable for named tables plus ordered fallback arrays. [VERIFIED: crates/core/foundation/config/src/lib.rs] |
| Built-in vector fallback | Fallback icon asset file in a pack | A pack asset can be missing; Phase 5 requires fallback independent of icon resolution. [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md] |

**Installation:**

No new crates are required for the recommended plan because `icon`, `resvg`, `image`, `serde`, `toml`, and `tempfile` are already present in the workspace lockfile or crate manifests. [VERIFIED: Cargo.lock] [VERIFIED: crates/core/ui/icon/Cargo.toml] [VERIFIED: crates/core/ui/render/Cargo.toml]

**Version verification:** Rust crate versions were verified from `Cargo.lock`: `icon 0.2.0`, `resvg 0.44.0`, `image 0.24.9`, `toml 0.8.23`, `anyhow 1.0.102`, `dirs 4.0.0`, and `tempfile 3.x`. [VERIFIED: Cargo.lock]

## Architecture Patterns

### System Architecture Diagram

```text
.mesh <icon name="semantic-name" size="18">
  -> parser/compiler creates WidgetNode attributes
  -> shell component runtime has plugin/component identity
  -> painter render_icon_node reads layout box, style color, name, size
  -> IconRegistry resolves semantic name in active profile
       -> ordered candidates pack_id:asset_name
       -> configured pack root lookup via icon::IconSearch/IconSearch::new_from
       -> bundled Material fallback only when configured as a pack/root
  -> RenderIconResult
       -> Found(path, multicolor=false) -> SVG/raster alpha-mask tint
       -> Found(path, multicolor=true)  -> preserve source colors
       -> Missing                  -> diagnostics dedupe + built-in vector fallback
  -> PixelBuffer paint at layout width/height
```

All stages above correspond to existing parser, widget node, painter, icon resolver, render, and diagnostics boundaries except the new `IconRegistry` result type and config/profile layer. [VERIFIED: crates/core/ui/component/src/parser.rs] [VERIFIED: crates/core/ui/elements/src/tree.rs] [VERIFIED: crates/core/ui/render/src/surface/painter.rs] [VERIFIED: crates/core/ui/icon/src/lib.rs] [VERIFIED: crates/core/foundation/diagnostics/src/lib.rs]

### Recommended Project Structure

```text
crates/core/ui/icon/src/
|-- lib.rs              # public resolver facade and compatibility API
|-- config.rs           # IconConfig, pack roots, profiles, active profile
|-- registry.rs         # IconRegistry, cache, profile switching
|-- xdg.rs              # configured-root XDG lookup adapter around icon crate
`-- fallback.rs         # built-in vector fallback model independent of packs

crates/core/ui/render/src/surface/
`-- icon.rs             # paint RenderIconResult, tint, multicolor, fallback draw

crates/core/foundation/diagnostics/src/
`-- lib.rs              # add missing-icon degraded diagnostics helper/dedupe
```

This structure keeps config parsing, lookup, and painting in existing crates instead of moving icon logic into surface plugins. [VERIFIED: crates/core/ui/icon/src/lib.rs] [VERIFIED: crates/core/ui/render/src/surface/icon.rs] [VERIFIED: docs/llm-context.md]

### Pattern 1: Typed Icon Config With Explicit Roots

**What:** Load a dedicated config with `active_profile`, explicit `packs`, and profile mappings from semantic names to ordered candidates. [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md]

**When to use:** Use for all named icons in shipped surfaces and tests. [VERIFIED: packages/plugins/frontend/core/panel/src/main.mesh] [VERIFIED: packages/plugins/frontend/core/quick-settings/src/main.mesh]

**Example:**

```toml
# Source: recommended format based on Phase 5 locked decisions and existing TOML config loader.
active_profile = "rounded"

[[packs]]
id = "material"
root = "crates/core/ui/icon/assets/material"
theme = "hicolor"

[[packs]]
id = "test-raster"
root = "target/test-icons/raster"
theme = "hicolor"

[profiles.rounded.icons]
audio-muted = ["material:audio-volume-muted", "material:volume-off"]
audio-high = ["material:audio-volume-high"]
network-wireless = ["material:network-wireless"]
settings = ["material:settings"]
missing-proof = ["material:no-such-icon"]
```

The exact filename and override env var are planner discretion, but use the existing `MESH_SETTINGS_PATH` pattern for testability, for example `MESH_ICON_CONFIG_PATH`. [VERIFIED: crates/core/foundation/config/src/lib.rs] [ASSUMED]

### Pattern 2: Resolver Returns a Typed Result

**What:** Return enough information to distinguish found, missing, multicolor, source path, selected candidate, and semantic name. [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md]

**When to use:** Use anywhere render or diagnostics must avoid conflating "nothing found" with "transparent icon painted". [VERIFIED: crates/core/ui/render/src/surface/icon.rs]

**Example:**

```rust
// Source: pattern derived from existing resolve_icon/draw_named_icon boundaries.
pub enum IconResolution {
    Found {
        semantic_name: String,
        candidate: String,
        path: PathBuf,
        multicolor: bool,
    },
    Missing {
        semantic_name: String,
        tried: Vec<String>,
    },
}
```

### Pattern 3: XDG Lookup Adapter Around `icon`

**What:** Build `icon::IconSearch::new_from(configured_roots).search().icons()` when icon config changes, then call `find_icon(asset_name, size, scale, theme)` for each ordered fallback candidate. [VERIFIED: /home/kolby/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/icon-0.2.0/src/search.rs] [VERIFIED: /home/kolby/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/icon-0.2.0/src/icon.rs]

**When to use:** Use for configured icon pack roots and XDG theme directories. [CITED: https://specifications.freedesktop.org/icon-theme/latest/index.html]

**Example:**

```rust
// Source: icon crate docs/local source.
let icons = icon::IconSearch::new_from(vec![pack_root])
    .search()
    .icons();
let found = icons.find_icon("audio-volume-muted", lookup_size, 1, "hicolor");
```

### Pattern 4: Diagnostics Dedupe by Plugin Plus Semantic Name

**What:** Store missing icon events in diagnostics state keyed by `(plugin_id, semantic_name)` and set health to degraded only on first insert. [VERIFIED: crates/core/foundation/diagnostics/src/lib.rs] [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md]

**When to use:** Use when render or preflight sees an unresolved required icon. [VERIFIED: crates/core/shell/src/shell/mod.rs]

**Example:**

```rust
// Source: existing record_handler_error dedupe pattern.
pub fn record_missing_icon(&self, semantic_name: impl Into<String>, tried: Vec<String>) -> bool {
    // Insert (self.plugin_id.clone(), semantic_name) into a HashSet.
    // Return true only when this is the first time this plugin missed that semantic icon.
    // Set health to HealthStatus::Degraded(...), do not increment error_count as a fatal error.
}
```

### Anti-Patterns to Avoid

- **Pack-specific names in `.mesh`:** Surface files must not use `volume_off`, `wifi`, or filesystem paths when the intended contract is a MESH semantic icon. [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md]
- **Logging on every paint:** Icon rendering can happen every frame, so misses must be cached/deduped. [VERIFIED: crates/core/ui/render/src/surface/painter.rs] [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md]
- **Using `size` as rendered dimensions:** Current painter computes `w`/`h` from layout and passes `size` only to lookup; preserve that behavior. [VERIFIED: crates/core/ui/render/src/surface/painter.rs]
- **Treating a missing icon as plugin load failure:** Phase 5 requires warning/degraded health while keeping the plugin active. [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md]
- **Hand-parsing partial XDG theme metadata:** The XDG spec includes inheritance, hicolor fallback, exact/closest matching, scales, and unthemed icons. [CITED: https://specifications.freedesktop.org/icon-theme/latest/index.html]

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| XDG theme matching | A custom recursive directory walk as the final resolver | `icon::IconSearch` and `Icons::find_icon` | The spec requires theme inheritance, hicolor fallback, exact/closest matching, and standalone pixmap fallback. [CITED: https://specifications.freedesktop.org/icon-theme/latest/index.html] [CITED: https://docs.rs/icon/latest/icon/] |
| SVG parsing/rasterization | String edits or custom SVG parser | `resvg` with `usvg`/`tiny_skia` | `resvg` is already installed and exposes the render pipeline used by the current code. [VERIFIED: crates/core/ui/render/src/surface/icon.rs] [CITED: https://docs.rs/resvg/latest/resvg/] |
| Raster decoding | Manual PNG/JPEG parsing | `image` crate | `image::open` and resize filters are already in use. [VERIFIED: crates/core/ui/render/src/surface/icon.rs] [CITED: https://docs.rs/image/latest/image/imageops/enum.FilterType.html] |
| Config parsing | Ad hoc line splitting | `serde` plus `toml::from_str` | Existing config uses typed Serde parsing and returns structured errors. [VERIFIED: crates/core/foundation/config/src/lib.rs] [CITED: https://docs.rs/toml/latest/toml/fn.from_str.html] |
| Missing diagnostics dedupe | Frame-local booleans | Diagnostics `HashSet` key pattern | Existing diagnostics dedupes handler errors with a `HashSet`. [VERIFIED: crates/core/foundation/diagnostics/src/lib.rs] |

**Key insight:** The hard parts are contract boundaries and state invalidation, not pixel drawing. [VERIFIED: crates/core/ui/render/src/surface/icon.rs] XDG matching and SVG/raster decoding already have standard libraries in the workspace, while semantic mapping, plugin context, and diagnostic dedupe are MESH-specific integration work. [VERIFIED: Cargo.lock] [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md]

## Common Pitfalls

### Pitfall 1: Losing Plugin Context Before Diagnostics

**What goes wrong:** `draw_named_icon()` only receives a name and no plugin ID, so missing icons cannot be deduped by plugin plus semantic name. [VERIFIED: crates/core/ui/render/src/surface/icon.rs]
**Why it happens:** Rendering currently has no diagnostics handle in the icon draw function. [VERIFIED: crates/core/ui/render/src/surface/painter.rs]
**How to avoid:** Record missing icons during component render/preflight where component diagnostics exist, or pass a narrow render diagnostics sink into icon rendering. [VERIFIED: crates/core/shell/src/shell/component.rs]
**Warning signs:** Warnings mention only icon names or emit repeatedly while the same surface repaints. [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md]

### Pitfall 2: Cache Staleness After Profile Switching

**What goes wrong:** A resolved path cache keyed only by `(name, size)` returns the old profile's path after `active_profile` changes. [VERIFIED: crates/core/ui/icon/src/lib.rs]
**Why it happens:** The current resolver cache key has no profile/config generation. [VERIFIED: crates/core/ui/icon/src/lib.rs]
**How to avoid:** Include profile/config generation in cache keys or clear all resolver and decoded-image caches on profile reload. [VERIFIED: crates/core/ui/icon/src/lib.rs] [VERIFIED: crates/core/ui/render/src/surface/icon.rs]
**Warning signs:** Tests pass before profile switch but keep showing the same icon after config reload. [ASSUMED]

### Pitfall 3: Flattening Multicolor Assets

**What goes wrong:** App logos, flags, or other multicolor icons become monochrome because render code tints every non-transparent pixel. [VERIFIED: crates/core/ui/render/src/surface/icon.rs]
**Why it happens:** Current raster and SVG code uses alpha-mask tint with the inherited color. [VERIFIED: crates/core/ui/render/src/surface/icon.rs]
**How to avoid:** Carry `multicolor` from config/candidate metadata into render and preserve original RGBA when true. [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md]
**Warning signs:** A test PNG with red/green pixels renders as one theme color. [ASSUMED]

### Pitfall 4: Relying on Host System Icon Themes in Tests

**What goes wrong:** Tests pass on one developer machine and fail in Nix/CI because `/usr/share/icons` is absent or different. [VERIFIED: find /usr/share/icons /usr/share/pixmaps -maxdepth 2 -type d]
**Why it happens:** System icon availability is environment-specific. [CITED: https://specifications.freedesktop.org/icon-theme/latest/index.html]
**How to avoid:** Use `tempfile` roots with minimal `hicolor/index.theme`, SVG, PNG, and missing candidates. [VERIFIED: crates/core/ui/icon/Cargo.toml]
**Warning signs:** Tests require a desktop theme such as Adwaita or Papirus to be installed. [ASSUMED]

### Pitfall 5: Overloading `assets.icons`

**What goes wrong:** Manifest icon dependency declarations collide with existing `assets.icons`, which currently means an asset path. [VERIFIED: crates/core/extension/plugin/src/manifest.rs] [VERIFIED: packages/plugins/icon-packs/papirus/plugin.json]
**Why it happens:** The manifest already has `DependenciesSection.icon_packs` and `AssetsSection.icons`; semantic required icons need a separate meaning. [VERIFIED: crates/core/extension/plugin/src/manifest.rs]
**How to avoid:** Reuse `dependencies.icon_packs.required/optional` for pack dependencies and add a clearly named semantic declaration such as `icon_requirements.required`. [VERIFIED: crates/core/extension/plugin/src/manifest.rs] [ASSUMED]
**Warning signs:** Code cannot distinguish "where icon assets live" from "which semantic icons this plugin needs." [ASSUMED]

## Code Examples

### Config Loader Shape

```rust
// Source: existing config crate pattern plus Phase 5 icon config requirements.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct IconConfig {
    pub active_profile: String,
    #[serde(default)]
    pub packs: Vec<IconPackRoot>,
    #[serde(default)]
    pub profiles: std::collections::HashMap<String, IconProfile>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct IconPackRoot {
    pub id: String,
    pub root: std::path::PathBuf,
    #[serde(default = "default_hicolor")]
    pub theme: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct IconProfile {
    #[serde(default)]
    pub icons: std::collections::HashMap<String, Vec<String>>,
}
```

### Rendering Size Contract

```rust
// Source: crates/core/ui/render/src/surface/painter.rs
let size = node
    .attributes
    .get("size")
    .and_then(|s| s.parse::<u32>().ok())
    .unwrap_or(w.max(h) as u32);

// Keep using w/h for paint dimensions. Use size only for lookup.
```

### Alpha-Mask Tint Contract

```rust
// Source: crates/core/ui/render/src/surface/icon.rs
Color {
    r: tint.r,
    g: tint.g,
    b: tint.b,
    a: source_alpha,
}
```

### Built-In Fallback Paint

```rust
// Source: Phase 5 D-11 plus PixelBuffer blend API.
fn draw_missing_icon_fallback(buffer: &mut PixelBuffer, x: i32, y: i32, w: i32, h: i32, color: Color) {
    // Draw a small neutral vector, for example a boxed question-mark or warning outline,
    // using PixelBuffer::blend_pixel/fill primitives. Do not resolve another icon file here.
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual one-level or heuristic XDG walks | Spec-aware lookup with `index.theme`, inheritance, hicolor fallback, and closest match | Freedesktop Icon Theme Specification 0.13, published 2013-07-02 | Use `icon` crate or equivalent spec-aware logic, not an incomplete directory walk. [CITED: https://specifications.freedesktop.org/icon-theme/latest/index.html] [CITED: https://docs.rs/icon/latest/icon/] |
| Per-asset color variants | Inherited current color plus optional multicolor opt-out | Existing MESH icon/theme docs and Phase 5 decisions | Render monochrome assets as alpha masks and preserve multicolor only when metadata opts out. [VERIFIED: docs/theming/icons.md] [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md] |
| Debug placeholder icon box | Transparent icon layout plus explicit missing fallback | Existing code already removed purple placeholder and defaults icon to 18x18 transparent | Missing fallback should be intentional and diagnostic-backed. [VERIFIED: crates/core/ui/render/src/style.rs] |

**Deprecated/outdated:**
- Manual resolver as final XDG authority: insufficient for the XDG spec's theme inheritance and closest-size logic. [VERIFIED: crates/core/ui/icon/src/lib.rs] [CITED: https://specifications.freedesktop.org/icon-theme/latest/index.html]
- Pack-specific names in shipped `.mesh` files: contradicts the semantic MESH icon contract. [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md]
- System-theme-dependent proof tests: unreliable in the current environment because no `/usr/share/icons` or `/usr/share/pixmaps` directories were found by the availability probe. [VERIFIED: find /usr/share/icons /usr/share/pixmaps -maxdepth 2 -type d]

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Use `MESH_ICON_CONFIG_PATH` as the dedicated icon config override env var. | Architecture Patterns | Low: another name would work if documented and tested. |
| A2 | Add a manifest section named `icon_requirements.required` for semantic icon declarations. | Common Pitfalls | Medium: planner may choose a different schema, but it must not overload `assets.icons`. |
| A3 | Profile-switch stale-cache tests should detect unchanged rendered icons after reload. | Common Pitfalls | Low: exact test shape can vary. |
| A4 | Multicolor regression can be tested with a red/green PNG fixture. | Common Pitfalls | Low: SVG or PNG fixtures both satisfy the behavior. |

## Open Questions

1. **Where should live profile reload be triggered?**
   - What we know: Phase 5 only needs config-level switching, and current icon caches are process-local. [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md] [VERIFIED: crates/core/ui/icon/src/lib.rs]
   - What's unclear: Whether runtime hot reload is required or whether tests can instantiate a new registry/config. [ASSUMED]
   - Recommendation: Plan for an explicit `IconRegistry::reload(config)` or `set_config(config)` API and verify cache invalidation without adding a visible UI. [ASSUMED]

2. **Should `src` icons participate in semantic diagnostics?**
   - What we know: Phase decisions focus on named semantic icons, while current painter also supports `src` path icons. [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md] [VERIFIED: crates/core/ui/render/src/surface/painter.rs]
   - What's unclear: Whether path icons should warn through the same missing-icon health path. [ASSUMED]
   - Recommendation: Keep Phase 5 diagnostics scoped to `name` semantic icons and leave `src` path diagnostics for a later asset-validation phase. [ASSUMED]

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|-------------|-----------|---------|----------|
| `cargo` | Rust tests/build | yes | `cargo 1.95.0` | none needed. [VERIFIED: cargo --version] |
| `rustc` | Rust compilation | yes | `rustc 1.95.0` | none needed. [VERIFIED: rustc --version] |
| `nix` | Project dev shell | yes | `nix 2.31.4` | Plain `cargo test` may work, but project approved command uses Nix. [VERIFIED: nix --version] |
| `/usr/share/icons` | Host-system icon fallback | no | none found | Use temp/configured icon roots in tests. [VERIFIED: find /usr/share/icons /usr/share/pixmaps -maxdepth 2 -type d] |

**Missing dependencies with no fallback:**
- None for research/planning; host system icon dirs are absent but tests should not depend on them. [VERIFIED: find /usr/share/icons /usr/share/pixmaps -maxdepth 2 -type d]

**Missing dependencies with fallback:**
- System icon themes: use `tempfile` fixtures and bundled Material test assets. [VERIFIED: crates/core/ui/icon/Cargo.toml] [VERIFIED: crates/core/ui/icon/assets/material]

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness via Cargo. [VERIFIED: grep -R "\[test\]\|mod tests" crates] |
| Config file | Workspace `Cargo.toml`; no separate test config required. [VERIFIED: Cargo.toml] |
| Quick run command | `nix develop -c cargo test -p mesh-core-icon -p mesh-core-render` [VERIFIED: command run 2026-05-03] |
| Full suite command | `nix develop -c cargo test` [VERIFIED: Cargo.toml] |

Targeted baseline passed before Phase 5 implementation: `mesh-core-icon` 2 tests passed and `mesh-core-render` 13 tests passed with one existing dead-code warning in `surface/text.rs`. [VERIFIED: nix develop -c cargo test -p mesh-core-icon -p mesh-core-render]

### Phase Requirements -> Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|--------------|
| ICON-01 | Semantic name resolves through active profile to an ordered XDG candidate under configured roots. | unit | `nix develop -c cargo test -p mesh-core-icon icon_config_resolves_ordered_fallbacks` | No, Wave 0. [VERIFIED: crates/core/ui/icon/src/lib.rs] |
| ICON-01 | Profile switch changes selected candidate and invalidates old resolution cache. | unit | `nix develop -c cargo test -p mesh-core-icon icon_profile_switch_invalidates_cache` | No, Wave 0. [VERIFIED: crates/core/ui/icon/src/lib.rs] |
| ICON-02 | SVG fixture rasterizes into non-transparent pixels at layout dimensions and tints by alpha mask. | unit | `nix develop -c cargo test -p mesh-core-render svg_icon_rasterizes_and_tints` | No, Wave 0. [VERIFIED: crates/core/ui/render/src/surface/icon.rs] |
| ICON-03 | PNG fixture decodes, resizes to layout dimensions, and tints by alpha mask unless multicolor. | unit | `nix develop -c cargo test -p mesh-core-render raster_icon_decodes_resizes_and_tints` | No, Wave 0. [VERIFIED: crates/core/ui/render/src/surface/icon.rs] |
| ICON-04 | Missing semantic icon records one degraded diagnostic per `(plugin_id, semantic_name)` and paints fallback. | unit/integration | `nix develop -c cargo test -p mesh-core-diagnostics -p mesh-core-render missing_icon_dedupes_and_paints_fallback` | No, Wave 0. [VERIFIED: crates/core/foundation/diagnostics/src/lib.rs] |
| ICON-01..04 | Panel, quick settings, and navigation bar use semantic names and cover SVG/raster/missing proof. | integration | `nix develop -c cargo test -p mesh-core-shell icon_reliability_core_surfaces_proof` | No, Wave 0. [VERIFIED: packages/plugins/frontend/core/panel/src/main.mesh] [VERIFIED: packages/plugins/frontend/core/quick-settings/src/main.mesh] [VERIFIED: packages/plugins/frontend/core/navigation-bar/plugin.json] |

### Sampling Rate

- **Per task commit:** `nix develop -c cargo test -p mesh-core-icon -p mesh-core-render` [VERIFIED: command run 2026-05-03]
- **Per wave merge:** `nix develop -c cargo test -p mesh-core-icon -p mesh-core-render -p mesh-core-diagnostics -p mesh-core-shell` [VERIFIED: Cargo.toml]
- **Phase gate:** `nix develop -c cargo test` plus proof-surface test command green before `$gsd-verify-work`. [VERIFIED: .planning/config.json]

### Wave 0 Gaps

- [ ] `crates/core/ui/icon/src/config.rs` tests for parsing active profile, packs, mappings, ordered fallback lists, and missing active profile. [VERIFIED: crates/core/ui/icon/src/lib.rs]
- [ ] `crates/core/ui/icon/src/registry.rs` tests for configured root lookup, profile switching, bundled Material mapping, and cache invalidation. [VERIFIED: crates/core/ui/icon/src/lib.rs]
- [ ] `crates/core/ui/render/src/surface/icon.rs` tests for SVG tint, raster tint, multicolor preservation, and fallback drawing. [VERIFIED: crates/core/ui/render/src/surface/icon.rs]
- [ ] `crates/core/foundation/diagnostics/src/lib.rs` tests for missing-icon dedupe and degraded health. [VERIFIED: crates/core/foundation/diagnostics/src/lib.rs]
- [ ] Shell/component proof test that gives renderer/plugin diagnostics enough context to dedupe by plugin plus semantic name. [VERIFIED: crates/core/shell/src/shell/mod.rs] [VERIFIED: crates/core/shell/src/shell/component.rs]

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|------------------|
| V2 Authentication | no | No authentication surface in this phase. [VERIFIED: .planning/REQUIREMENTS.md] |
| V3 Session Management | no | No sessions in this phase. [VERIFIED: .planning/REQUIREMENTS.md] |
| V4 Access Control | yes | Plugin manifests already express capabilities/dependencies; icon missing must not bypass plugin load policy. [VERIFIED: crates/core/extension/plugin/src/manifest.rs] |
| V5 Input Validation | yes | Deserialize typed icon config with Serde/TOML; reject unknown active profile, unknown pack ID, empty semantic name, and paths outside configured roots. [VERIFIED: crates/core/foundation/config/src/lib.rs] [ASSUMED] |
| V6 Cryptography | no | No cryptography needed for icon rendering. [VERIFIED: .planning/REQUIREMENTS.md] |

### Known Threat Patterns for Rust Icon Loading

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Arbitrary path read through configured pack root or explicit `src` | Information Disclosure | Canonicalize configured roots and candidate paths; ensure resolved candidates remain under configured roots for semantic icons. [ASSUMED] |
| Malicious or oversized SVG causing high CPU/memory | Denial of Service | Keep Phase 5 SVG support practical and test with small fixtures; consider size limits before reading/rasterizing untrusted pack assets. [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md] [ASSUMED] |
| Diagnostic spam from missing icons | Denial of Service | Dedupe by `(plugin_id, semantic_name)` and never emit every paint frame. [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md] [VERIFIED: crates/core/foundation/diagnostics/src/lib.rs] |
| Config confusion from duplicate pack IDs | Tampering | Validate unique pack IDs and reject mappings that reference missing packs. [ASSUMED] |

## Sources

### Primary (HIGH confidence)

- `.planning/phases/05-icon-rendering-reliability/05-CONTEXT.md` - locked Phase 5 decisions and proof scope. [VERIFIED]
- `.planning/REQUIREMENTS.md` - ICON-01 through ICON-04 requirement text. [VERIFIED]
- `.planning/STATE.md` - current milestone state and carry-forward architecture decisions. [VERIFIED]
- `crates/core/ui/icon/src/lib.rs` - existing resolver/cache and bundled fallback tests. [VERIFIED]
- `crates/core/ui/render/src/surface/icon.rs` - current SVG/raster draw path and image cache. [VERIFIED]
- `crates/core/ui/render/src/surface/painter.rs` - icon node integration and layout/size behavior. [VERIFIED]
- `crates/core/foundation/diagnostics/src/lib.rs` - diagnostics health and dedupe pattern. [VERIFIED]
- `crates/core/extension/plugin/src/manifest.rs` - existing dependency and asset schema. [VERIFIED]
- Freedesktop Icon Theme Specification 0.13 - directory layout, hicolor fallback, lookup algorithm, cache notes. [CITED: https://specifications.freedesktop.org/icon-theme/latest/index.html]
- `icon` crate docs/local source - XDG lookup, custom search dirs, cache feature. [CITED: https://docs.rs/icon/latest/icon/] [VERIFIED: /home/kolby/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/icon-0.2.0/src]
- `resvg` docs - SVG rendering library with `usvg` and `tiny_skia` re-exports. [CITED: https://docs.rs/resvg/latest/resvg/]
- `image` docs - resize filter variants including `Lanczos3`. [CITED: https://docs.rs/image/latest/image/imageops/enum.FilterType.html]
- `toml` docs - `from_str` typed deserialization. [CITED: https://docs.rs/toml/latest/toml/fn.from_str.html]

### Secondary (MEDIUM confidence)

- `docs/llm-context.md` - current icon-system notes and historical bug list. [VERIFIED]
- `docs/theming/icons.md` - longer-term icon contract, inherited color, multicolor opt-out. [VERIFIED]
- `docs/theming/themes.md` - token/inheritance context. [VERIFIED]

### Tertiary (LOW confidence)

- Assumptions A1-A4 above, all explicitly marked for planner confirmation. [ASSUMED]

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - dependencies and versions verified from workspace manifests/lockfile plus official docs. [VERIFIED: Cargo.lock] [CITED: https://docs.rs/icon/latest/icon/]
- Architecture: HIGH - boundaries are anchored in existing crates and Phase 5 locked decisions. [VERIFIED: crates/core/ui/icon/src/lib.rs] [VERIFIED: crates/core/ui/render/src/surface/painter.rs] [VERIFIED: .planning/phases/05-icon-rendering-reliability/05-CONTEXT.md]
- Pitfalls: MEDIUM - most are verified from existing code; cache reload and exact manifest schema risks need planner/user confirmation. [VERIFIED: crates/core/ui/icon/src/lib.rs] [ASSUMED]

**Research date:** 2026-05-03
**Valid until:** 2026-06-02 for local architecture; 2026-05-10 for crate-version currency because Rust graphics crates move faster than the repo lockfile. [ASSUMED]
