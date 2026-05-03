# Phase 05: Icon Rendering Reliability - Pattern Map

**Mapped:** 2026-05-03
**Status:** Ready for planning

<file_map>
## Planned Files and Closest Analogs

| Target | Role | Closest Existing Analog | Pattern to Preserve |
|--------|------|-------------------------|---------------------|
| `crates/core/ui/icon/src/config.rs` | Dedicated icon config structs and TOML parsing | `crates/core/foundation/config/src/lib.rs`; `crates/core/extension/plugin/src/manifest.rs` | Use typed Serde structs, defaults, and structured errors instead of string parsing. |
| `crates/core/ui/icon/src/registry.rs` | Semantic mapping, configured roots, profile generation, cache | `crates/core/ui/icon/src/lib.rs` | Preserve a small facade API while replacing `(name, size)` cache with config/profile-aware keys. |
| `crates/core/ui/icon/src/xdg.rs` | XDG lookup adapter | `crates/core/ui/icon/src/lib.rs` | Keep lookup behind `mesh-core-icon`; avoid putting XDG details in render or `.mesh` surfaces. |
| `crates/core/ui/icon/src/fallback.rs` | Built-in vector fallback model | `crates/core/ui/render/src/surface/icon.rs` | Render fallback without resolving another pack asset. |
| `crates/core/ui/render/src/surface/icon.rs` | SVG/raster/multicolor/fallback pixel drawing | Existing file | Keep layout dimensions as paint dimensions and `size` only as lookup hint. |
| `crates/core/ui/render/src/surface/painter.rs` | Icon node integration | Existing `render_icon_node()` | Keep `<icon name>` centralized; pass enough context/result metadata for fallback painting. |
| `crates/core/foundation/diagnostics/src/lib.rs` | Missing icon dedupe and degraded health | Existing `record_handler_error()` | Use a `HashSet` dedupe key and return true only for the first event. |
| `crates/core/extension/plugin/src/manifest.rs` | Semantic icon requirements in manifests | Existing `DependenciesSection`, `OptionalDependencyGroup`, `AssetsSection` | Add a distinct section; do not overload `assets.icons`. |
| `crates/core/shell/src/shell/component.rs` | Plugin-context proof and diagnostics integration | Existing component diagnostics and frontend catalog validation | Plugin ID exists here; use it for `(plugin_id, semantic_name)` missing-icon diagnostics. |
| `packages/plugins/frontend/core/*/plugin.json` | Core surface icon declarations | Existing dependency declarations | Add declared semantic icons/icon-pack requirements without changing `.mesh` names to pack-specific names. |
| `packages/plugins/frontend/core/*/*.mesh` | Proof surface call sites | Existing `<icon name="...">` usages | Keep semantic names in `.mesh`; no filesystem paths or pack-specific asset names. |
</file_map>

<existing_patterns>
## Existing Code Patterns

### Icon Resolver Facade

`crates/core/ui/icon/src/lib.rs` currently exposes:

- `resolve_icon(name: &str, size: u32) -> Option<PathBuf>`
- explicit file path passthrough
- process-global cache keyed by `(name, size)`
- manual theme/category walk through user/system paths
- bundled Material SVG fallback via `assets/material/{name}.svg`

The phase should keep a compatibility facade but move real behavior behind typed config and registry modules. The cache must include active profile/config generation or be cleared on profile reload.

### Render Pipeline

`crates/core/ui/render/src/surface/icon.rs` already:

- caches decoded raster images by path
- decodes PNG/JPG/JPEG/BMP through `image`
- rasterizes SVG through `resvg`
- paints alpha-mask pixels using the inherited `style.color`
- draws named icons from `draw_named_icon()`

Keep these mechanics. The new work is to carry `multicolor` and missing/fallback metadata into drawing.

### Layout Contract

`crates/core/ui/render/src/surface/painter.rs::render_icon_node()` computes `w` and `h` from layout and uses the `size` attribute only as a lookup hint. Phase 05 must preserve that: painted dimensions come from the CSS/layout box.

### Diagnostics Dedupe

`crates/core/foundation/diagnostics/src/lib.rs::record_handler_error()` stores a dedupe key in `DiagnosticsState.handler_errors`, updates health only on first insert, and returns a boolean indicating whether the event was new. Missing icon diagnostics should follow this shape with a separate `missing_icons: HashSet<(String, String)>` or equivalent keyed by plugin plus semantic icon.

### Manifest Parsing

`crates/core/extension/plugin/src/manifest.rs` normalizes both TOML and JSON manifests into `Manifest`. JSON dependency groups already support:

- `dependencies.icon_packs.required`
- `dependencies.icon_packs.optional`

Add a distinct semantic declaration such as `icon_requirements.required` and `icon_requirements.optional`. Do not reuse `assets.icons`, which currently means an asset path.

### Core Surface Icons

The proof surfaces already use semantic-like names:

- panel: dynamic audio names
- quick settings: `network-wireless`, `bluetooth`, dynamic audio names
- navigation bar: `settings`, dynamic theme/battery/audio names

The planner should preserve these call sites and move fallback/mapping details into config.
</existing_patterns>

<data_flow>
## Data Flow

```text
plugin.json icon requirements + config/icons.toml
  -> mesh-core-plugin manifest normalization
  -> mesh-core-icon IconConfig/IconRegistry
  -> <icon name="semantic-name" size="18">
  -> painter render_icon_node()
  -> IconRegistry::resolve(semantic, size)
       -> active profile ordered candidates
       -> configured pack root / XDG lookup
       -> IconResolution::Found or IconResolution::Missing
  -> render icon.rs paints SVG/raster/multicolor/fallback
  -> diagnostics records missing semantic icon once per plugin
```
</data_flow>

<landmines>
## Landmines

- Do not leave cache keys as only `(name, size)` after adding profile switching.
- Do not emit missing-icon diagnostics from a per-frame paint path without dedupe.
- Do not make missing icons fail plugin loading.
- Do not make fallback rendering depend on an icon pack asset.
- Do not rely on `/usr/share/icons` in tests; this environment may not have system icon themes.
- Do not put `volume_off`, `wifi`, absolute paths, or pack IDs into shipped `.mesh` files.
- Do not tint multicolor assets by default once metadata says `multicolor = true`.
</landmines>

<verification_patterns>
## Verification Patterns

- Use hermetic `tempfile` icon roots with minimal XDG-like structure and tiny SVG/PNG fixtures.
- Use targeted Cargo commands first:
  - `nix develop -c cargo test -p mesh-core-icon`
  - `nix develop -c cargo test -p mesh-core-render`
  - `nix develop -c cargo test -p mesh-core-diagnostics`
  - `nix develop -c cargo test -p mesh-core-shell`
- When `rg` is unavailable, use `grep -R`/`find` in static verification commands.
</verification_patterns>

