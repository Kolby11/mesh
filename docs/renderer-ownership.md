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
| Retained runtime tree identity | authoritative | `crates/core/shell/src/shell/component/runtime_tree.rs` | Owns stable runtime node identity and retained dirty-category tracking. |
| Render object synchronization | authoritative | `crates/core/frontend/render/src/render_object.rs` | Owns retained render-object slots for geometry, material, text, and accessibility dirtiness. |
| Retained display-list ownership | authoritative | `crates/core/frontend/render/src/display_list.rs` | Owns paint command identity, selection payloads, damage data, repaint policy, and batching evidence. |
| Software painter | authoritative | `crates/core/frontend/render/src/surface/painter.rs` | Paints current widget trees to software pixel buffers and remains the default paint execution path. |
| Presentation boundary | authoritative | `crates/core/presentation/src/lib.rs` | Selects dev-window or layer-shell presentation and keeps `PixelBuffer` presentation ownership outside renderer experiments. |
| Wayland surface backend | authoritative | `crates/core/presentation/src/wayland_surface/backend.rs` | Owns Wayland surface attach, copy, and damage behavior that broad migration must preserve or intentionally replace. |

MIGR-02: existing renderer modules are classified as authoritative, adapter-owned, or replacement candidate before broad adoption.

## Adapter-Owned Boundaries

## Replacement Candidates

## Promotion Rule
