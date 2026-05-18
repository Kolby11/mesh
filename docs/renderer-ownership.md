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
| Software painter | authoritative | `crates/core/frontend/render/src/surface/painter.rs` | Paints current widget trees to software pixel buffers and remains the default paint execution path. |
| Presentation boundary | authoritative | `crates/core/presentation/src/lib.rs` | Selects dev-window or layer-shell presentation and keeps `PixelBuffer` presentation ownership outside renderer experiments. |
| Wayland surface backend | authoritative | `crates/core/presentation/src/wayland_surface/backend.rs` | Owns Wayland surface attach, copy, and damage behavior that broad migration must preserve or intentionally replace. |

MIGR-02: existing renderer modules are classified as authoritative, adapter-owned, or replacement candidate before broad adoption.

## Adapter-Owned Boundaries

| Boundary | Status | Paths or evidence | Promotion condition |
|----------|--------|-------------------|---------------------|
| Focused proof snapshots | adapter-owned | `FocusedProofSnapshot` in `crates/core/frontend/render/src/proof.rs` | Can grow only while current retained tree, render object, display-list, and presentation authority remain stable. |
| Focused accessibility updates | adapter-owned | `FocusedAccessKitUpdate` and Phase 44 AccessKit-compatible update evidence | Can promote only after retained-node update behavior is proven across shipped surfaces and platform runtime needs. |
| Focused text/paint evidence | adapter-owned | `parley_text`, selected paint slots, and `phase44_navigation_audio_surface_emits_focused_proof_snapshot` | Can promote only after candidate text/paint paths preserve shipped behavior, selection geometry, and profiling evidence. |
| Crate-facing conversion modules | adapter-owned | Non-fatal diagnostics with prefix `focused renderer proof:` and future conversion modules | Can promote only when replacement candidates satisfy all observability and rollback gates. |
| Renderer library feature scaffold | adapter-owned | `crates/core/frontend/render/Cargo.toml` and `crates/core/frontend/render/src/library_adapters.rs` | May promote only when later phases preserve NodeId identity, invalidation, damage/profiling, diagnostics, theme-owned selection, AccessKit-compatible update evidence, and rollback gates. |

## Replacement Candidates

| Candidate | Status | Candidate use | Current limitation |
|-----------|--------|---------------|--------------------|
| Parley | replacement candidate | Future text layout/shaping path behind theme-owned selection and retained text evidence. | Not currently authoritative for all text behavior or editing semantics. |
| AnyRender/Vello-style rendering | replacement candidate | Future paint backend abstraction under retained display-list ownership. | Not currently authoritative for production paint execution. |
| AccessKit runtime expansion | replacement candidate | Future accessibility runtime beyond retained-node update evidence. | Phase 44 proves an update boundary, not a complete cross-platform runtime. |
| Stylo-style resolution | replacement candidate | Future style/profile evaluation if MESH needs richer CSS capability. | Must preserve bounded `.mesh` UI semantics and avoid browser-platform overreach. |
| Skia fallback | replacement candidate | Fallback paint backend if the preferred abstraction path cannot satisfy MESH needs. | Fallback evidence only; not selected as the primary v1.8 path. |
| Blitz | replacement candidate | Reference architecture and possible future reconsideration. | Blitz remains reference/blocker evidence, not a production authoring model. |

## Promotion Rule

A replacement candidate cannot become authoritative until it preserves or replaces NodeId identity, typed invalidation, damage, profiling, diagnostics, theme-owned selection behavior, and AccessKit-compatible update evidence.

Promotion also requires the dependency record and broad-adoption checklist in `docs/renderer-migration.md`.
