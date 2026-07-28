# Phase 5: Icon Rendering Reliability - Context

**Gathered:** 2026-05-03T13:02:38Z
**Status:** Ready for planning

<domain>
## Phase Boundary

This phase makes `<icon name="...">` reliable for shipped shell surfaces. It should introduce a semantic icon contract backed by dedicated icon configuration, user-switchable mapping profiles, configured icon pack roots, SVG and raster rendering through the existing render pipeline, and graceful missing-icon diagnostics. The phase should prove panel, quick settings, and navigation bar can render icons through semantic names without pack-specific filenames or special-case filesystem paths in `.mesh` call sites.

</domain>

<decisions>
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

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Planning Scope
- `.planning/PROJECT.md` - milestone goal, external-developer target, and active requirement that icon rendering works for XDG icon names.
- `.planning/REQUIREMENTS.md` - Phase 5 requirement IDs `ICON-01` through `ICON-04`.
- `.planning/ROADMAP.md` - Phase 5 goal, success criteria, dependency on Phase 4, and UI hint.
- `.planning/STATE.md` - current project state and carry-forward decisions.

### Prior Decisions
- `.planning/phases/04-real-core-surfaces/04-CONTEXT.md` - core surfaces that Phase 5 must support without special-case asset paths.
- `.planning/phases/03-frontend-reactivity-and-events/03-CONTEXT.md` - navigation bar as an existing proof surface and event/render expectations.
- `.planning/phases/02-service-proxy-delivery/02-CONTEXT.md` - public API predictability and diagnostics expectations.

### Icon Design References
- `docs/llm-context.md` - current icon-system notes, four known icon rendering bugs, and expected `<icon name="...">` pipeline.
- `docs/theming/icons.md` - longer-term icon contract ideas: named icons, pack chains, aliases, multicolor opt-out, and theme-driven icon appearance.
- `docs/theming/themes.md` - theme token context for inherited color and visual consistency.

### Icon and Rendering Code
- `crates/core/ui/icon/src/lib.rs` - current icon resolver, cache, bundled Material fallback, and existing icon tests.
- `crates/core/ui/render/src/surface/icon.rs` - current SVG/raster draw path and image cache.
- `crates/core/ui/render/src/surface/painter.rs` - `render_icon_node()` reads `src`, `name`, and `size` attributes and calls icon drawing.
- `crates/core/ui/render/src/style.rs` - current default icon style and transparent icon placeholder behavior.
- `crates/core/ui/elements/src/element.rs` - `IconElement` field contract (`name`, `src`, `size`) shared with tooling.
- `crates/core/foundation/diagnostics/src/lib.rs` - diagnostic/health integration point for missing icon warnings.

### Existing Icon Assets and Packs
- `crates/core/ui/icon/assets/material/` - bundled Material SVG fallback assets currently used by tests and core surfaces.
- `packages/plugins/icon-packs/papirus/plugin.json` - current icon-pack plugin metadata stub.

### Proof Surfaces
- `packages/plugins/frontend/core/panel/src/main.mesh` - Phase 4 panel proof, currently uses semantic audio icon names.
- `packages/plugins/frontend/core/quick-settings/src/main.mesh` - quick settings nav/status icon usage.
- `packages/plugins/frontend/core/quick-settings/src/components/audio-section.mesh` - audio icon usage inside the primary control surface.
- `packages/plugins/frontend/core/navigation-bar/src/main.mesh` - navigation bar root proof surface.
- `packages/plugins/frontend/core/navigation-bar/src/components/volume-button.mesh` - icon-heavy audio control component.
- `packages/plugins/frontend/core/navigation-bar/src/components/settings-button.mesh` - simple named icon usage.
- `packages/plugins/frontend/core/navigation-bar/src/components/theme-button.mesh` - dynamic named icon usage.
- `packages/plugins/frontend/core/navigation-bar/src/components/battery-widget.mesh` - dynamic battery icon usage.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/ui/icon/src/lib.rs` already has a resolver entry point, resolution cache, explicit-path handling, XDG-style search paths, bundled Material fallback, and tests for explicit paths and bundled SVG fallback.
- `crates/core/ui/render/src/surface/icon.rs` already has raster image decode/cache, SVG rasterization via `resvg`, and named-icon drawing through `mesh_core_icon::resolve_icon`.
- `crates/core/ui/render/src/surface/painter.rs::render_icon_node()` already centralizes icon rendering from `WidgetNode` attributes, making it the likely integration point for semantic resolution and diagnostics.
- Bundled Material SVG assets already cover common shipped surface names such as `audio-volume-high`, `audio-volume-muted`, `network-wireless`, `settings`, `bluetooth`, and battery icons.
- `packages/plugins/icon-packs/papirus/plugin.json` is an existing icon-pack metadata foothold, even though Phase 5 decisions favor explicit configured pack roots over automatic plugin registry behavior.

### Established Patterns
- Core rendering is generic; it should not encode surface-specific icon names or service-specific display logic.
- Shipped surfaces already author icons with `<icon name="...">`, so Phase 5 should preserve this authoring model and route pack-specific details into config/mapping.
- Existing docs describe `size` as a resolution hint and computed layout as the rendered size; keep that model.
- Diagnostics should be visible but non-fatal, consistent with prior phases' handling of missing services and handler failures.

### Integration Points
- Dedicated icon config loading likely connects shell/config startup to `mesh-core-icon` resolution.
- Semantic mapping resolution should sit behind the existing `resolve_icon(name, size)` style API or a replacement resolver with similar call sites, so render code remains mostly generic.
- Missing-icon diagnostics need plugin context. The planner should determine whether to pass plugin/surface identity into icon rendering or record misses at a higher layer before paint.
- Profile switching needs cache invalidation for resolved icon paths and decoded image cache behavior where relevant.
- Proof tests should cover SVG, raster, and missing fallback without requiring real system icon themes.

</code_context>

<specifics>
## Specific Ideas

- A dedicated icon config can define pack roots and mapping profiles, for example an active `rounded` profile plus `filled` and `lucide` alternatives.
- A semantic mapping entry should be an ordered fallback list, for example `audio-muted = ["material-rounded:volume_off", "lucide:volume-x", "material:volume_off"]`.
- Core surfaces should keep semantic names in `.mesh`; mappings translate those names into pack-specific asset names.
- The missing-icon fallback must be built into core so it still appears when the configured placeholder icon is itself missing.
- Profile switching only needs config/test proof in Phase 5; a user-facing switcher belongs elsewhere.

</specifics>

<deferred>
## Deferred Ideas

- A user-facing UI for switching icon profiles is deferred. Phase 5 only needs config-level switching.
- Full complex SVG fidelity, including gradients, masks, embedded images, and complete multicolor rendering semantics, is deferred.
- A broader icon-pack registry with automatic provider chains can build on this phase later; Phase 5 should use explicit configured pack roots.

</deferred>

---

*Phase: 05-Icon Rendering Reliability*
*Context gathered: 2026-05-03T13:02:38Z*
