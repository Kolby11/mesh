# Renderer Ownership Classification

## Classification Rules

- authoritative: current source of truth until a future migration step deliberately replaces it.
- adapter-owned: bridge evidence or conversion code that may grow, shrink, or disappear while current authoritative boundaries remain stable.
- replacement candidate: future adoption target that is not currently a public author-facing guarantee.

## Authoritative Boundaries

| Boundary | Status | Paths | Why authoritative |
|----------|--------|-------|-------------------|
| Component source parsing | authoritative | `crates/core/ui/component/src/lib.rs` | Parses author-facing `.mesh` single-file components before renderer migration touches runtime output. |
| Frontend compilation and imports | authoritative | `crates/core/frontend/compiler/src/compile.rs` | Resolves frontend manifests, local components, source tags, and widget-tree construction inputs. |
| Taffy-backed layout | authoritative | `crates/core/ui/elements/src/layout.rs` | Computes in-scope row, column, stack, fixed size, gap, padding, absolute positioning, and container-width geometry after Phase 47 while writing results back to retained MESH `WidgetNode.layout`. |
| Retained runtime tree identity | authoritative | `crates/core/shell/src/shell/component/runtime_tree.rs` | Owns stable runtime node identity and retained dirty-category tracking. |
| Render object synchronization | authoritative | `crates/core/frontend/render/src/render_object.rs` | Owns retained render-object slots for geometry, material, text, and accessibility dirtiness. |
| Retained display-list ownership | authoritative | `crates/core/frontend/render/src/display_list.rs` | Owns paint command identity, selection payloads, damage data, repaint policy, and batching evidence. |
| Render engine and display list | authoritative | `crates/core/frontend/render/src/render_object.rs`, `crates/core/frontend/render/src/display_list.rs`, `crates/core/frontend/render/src/surface/painter.rs` | Owns retained render state, paint command ordering, damage/repaint selection, and translation from MESH widget/style/layout data into backend paint operations. |
| Skia paint backend | authoritative | `crates/core/frontend/render/src/surface/painter/backend.rs` | Owns the low-level painter/raster work below MESH paint commands: antialiasing, paths, rounded rects, strokes, shadows, blur/image filters, blend modes, clipping, layers/saveLayer, and related Skia canvas behavior. |
| Painter backend diagnostics and rollback visibility | authoritative | `FrontendRenderEngine::paint_backend_snapshot()` in `crates/core/frontend/render/src/surface/painter.rs` | Publishes backend id, backend-neutral capabilities, recent unsupported-feature diagnostics, and rollback authority without exposing Skia-specific types. |
| Presentation boundary | authoritative | `crates/core/presentation/src/lib.rs` | Selects dev-window or layer-shell presentation and keeps `PixelBuffer` presentation ownership outside renderer experiments. |
| Wayland surface backend | authoritative | `crates/core/presentation/src/wayland_surface/backend.rs` | Owns Wayland surface attach, copy, and damage behavior that broad migration must preserve or intentionally replace. |

MIGR-02: existing renderer modules are classified as authoritative, adapter-owned, or replacement candidate before broad adoption.

## Adapter-Owned Boundaries

| Boundary | Status | Paths or evidence | Promotion condition |
|----------|--------|-------------------|---------------------|
| Focused proof snapshots | adapter-owned | `FocusedProofSnapshot` in `crates/core/frontend/render/src/proof.rs` | Can grow only while current retained tree, render object, display-list, and presentation authority remain stable. |
| Focused accessibility updates | adapter-owned | `FocusedAccessKitUpdate` compatibility evidence and `build_accesskit_runtime_update` retained-node `accesskit::TreeUpdate` conversion | Retained-node update construction is production adapter evidence after Phase 50; platform publication still requires a future runtime/platform gate. |
| Focused text/paint evidence | adapter-owned | `parley_text`, selected paint slots, and `phase44_navigation_audio_surface_emits_focused_proof_snapshot` | Can promote only after candidate text/paint paths preserve shipped behavior, selection geometry, and profiling evidence. |
| Crate-facing conversion modules | adapter-owned | Non-fatal diagnostics with prefix `focused renderer proof:` and future conversion modules | Can promote only when replacement candidates satisfy all observability and rollback gates. |
| Renderer library feature scaffold | adapter-owned | `crates/core/frontend/render/Cargo.toml` and `crates/core/frontend/render/src/library_adapters.rs` | May promote only when later phases preserve NodeId identity, invalidation, damage/profiling, diagnostics, theme-owned selection, AccessKit-compatible update evidence, and rollback gates. |

## Replacement Candidates

| Candidate | Status | Candidate use | Current limitation |
|-----------|--------|---------------|--------------------|
| Parley | replacement candidate | Future text layout/shaping path behind theme-owned selection and retained text evidence. | Not currently authoritative for all text behavior or editing semantics. |
| Vello-style rendering | replacement candidate | Future paint backend implementation under retained display-list ownership. | Not currently authoritative for production paint execution; must implement the same high-level painter contract without taking ownership of MESH layout, style, damage, or presentation. |
| AccessKit platform publication | replacement candidate | Future accessibility service/platform publication beyond retained-node `TreeUpdate` construction. | Phase 50 builds real AccessKit updates but does not publish them to a compositor or screen-reader runtime. |
| Stylo-style resolution | replacement candidate | Future style/profile evaluation if MESH needs richer CSS capability. | Must preserve bounded `.mesh` UI semantics and avoid browser-platform overreach. |
| Blitz | replacement candidate | Reference architecture and possible future reconsideration. | Blitz remains reference/blocker evidence, not a production authoring model. |

## Vello Compatibility Notes

Vello compatibility is a contract-shaping constraint, not Phase 51 production
scope. The painter API should stay backend-neutral while allowing Skia to use
Skia-specific primitives internally.

Clean mapping candidates:

- `DrawRect`
- `DrawRoundedRect`
- `DrawPath`
- basic `PushClip` / `PopClip`
- basic `PushLayer` / `PopLayer`
- simple gradients and images when source ownership is defined

Approximation or capability-gated candidates:

- `DrawShadow`
- blur filters
- `ApplyFilter` with backdrop behavior
- blend modes beyond the common subset
- saveLayer-style effects whose exact semantics differ from Skia

Deferred or future-gated candidates:

- `DrawText` if MESH keeps shaping/rasterization in `TextRenderer`
- image decoding/source lifetime semantics
- complex image-filter composition

Skia-specific types stay inside `SkiaPaintBackend`. Retained display-list data,
render-object data, and the painter command API must not expose `skia_safe`
types such as `Canvas`, `Paint`, `Path`, `RRect`, `ImageFilter`, or
`SaveLayerRec`.

## Promotion Rule

A replacement candidate cannot become authoritative until it preserves or replaces NodeId identity, typed invalidation, damage, profiling, diagnostics, theme-owned selection behavior, and AccessKit-compatible update evidence.

Promotion also requires the dependency record and broad-adoption checklist in `docs/renderer-migration.md`.

## v1.10 Painter Engine Proof

The v1.10 painter engine keeps MESH authoritative for `.mesh` parsing, style
resolution, layout, animation state, z-order traversal, retained display-list
ordering, damage selection, diagnostics, profiling, module boundaries, input,
and presentation. Skia is authoritative only below the painter backend boundary.

The shipped proof slice is the navigation bar and audio popover. Required proof
commands are recorded in `docs/renderer-migration.md` under the v1.10 painter
engine record.
