---
phase: 55
slug: effects-layers-shadows-blur-images-and-gradients
status: verified
threats_open: 0
asvs_level: 1
created: 2026-05-23
verified_at: 2026-05-23
---

# Phase 55 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Style profile → retained render data | Author CSS-like values lower into `ComputedStyle`, render-object hashes, and display-list data. | Background image paths, gradient colors, blur/filter/shadow values. |
| Retained render data → painter backend | Backend-neutral commands are handed to Skia execution. | Painter commands for layers, images, gradients, shadows, filters, and diagnostics. |
| Image source path → local raster loader | Bounded style image path reaches cached image loading. | Relative image path string and decoded RGBA pixels. |
| Phase validation artifacts → milestone state | Test and validation results update planning artifacts. | Requirement status, validation flags, summary/verification evidence. |

---

## Threat Register

| Threat ID | Category | Component | Disposition | Mitigation | Status |
|-----------|----------|-----------|-------------|------------|--------|
| T-55-01 | Tampering | style/render data boundary | mitigate | `BackgroundPaint`, `StyleImageSource`, and `StyleLinearGradient` are backend-neutral; final grep proves no `skia_safe` leak into style/display-list/render-object data. | closed |
| T-55-02 | Repudiation | unsupported author CSS | mitigate | Unsupported `background-image` values produce diagnostics containing `unsupported background-image`; style tests cover url, linear-gradient, and unsupported forms. | closed |
| T-55-03 | Tampering | retained effect lowering | mitigate | Direct and retained image/gradient command-class parity tests prove retained lowering matches widget-tree lowering. | closed |
| T-55-04 | Information disclosure | image source data | mitigate | Style parser accepts only relative `url(...)` paths; retained data stores backend-neutral path strings, not Skia/native image handles. | closed |
| T-55-05 | Tampering | Skia layer execution | mitigate | `skia_effect_layer_*` pixel tests pass before `layers: true`; see advisory on grouped isolation semantics. | closed |
| T-55-06 | Information disclosure | image loading | mitigate | Style image paths are bounded at parse time; image execution emits `missing image asset` diagnostics for unavailable paths. | closed |
| T-55-07 | Repudiation | painter diagnostics | mitigate | `PainterDiagnostic` includes backend id, feature id, message, and optional `PainterDiagnosticSource`; diagnostic tests cover unsupported cases. | closed |
| T-55-08 | Denial of service | excessive blur | mitigate | `MAX_EFFECT_BLUR_RADIUS` bounds blur support; excessive blur emits a non-fatal diagnostic. | closed |
| T-55-09 | Tampering | retained backend-neutral data | mitigate | Final validation ran backend-neutrality grep against display-list, render-object, and element style data. | closed |
| T-55-10 | Repudiation | phase validation | mitigate | `55-VALIDATION.md` was marked complete only after focused suites and backend-neutrality proof passed; commands are recorded in summaries and verification. | closed |

*Status: open · closed*  
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

No accepted risks.

---

## Advisory Notes

| Advisory ID | Related Threat | Note |
|-------------|----------------|------|
| A-55-01 | T-55-05 | `55-REVIEW.md` records that current layer opacity/filter semantics are applied per command rather than through a true grouped cross-command Skia `saveLayer`. The implemented subset is tested and non-blocking for this threat audit, but overlapping descendants under group opacity/filter should be handled in follow-up work. |
| A-55-02 | T-55-06 | Skia image/gradient helpers compile with deprecation warnings for current `skia-safe` APIs. This is not a security issue, but should be cleaned up during maintenance. |

---

## Evidence

| Evidence | Result |
|----------|--------|
| `cargo test -p mesh-core-elements style_background -- --nocapture` | passed |
| `cargo test -p mesh-core-render painter_effect -- --nocapture` | passed |
| `cargo test -p mesh-core-render display_list_effect -- --nocapture` | passed |
| `cargo test -p mesh-core-render skia_effect_layer -- --nocapture` | passed |
| `cargo test -p mesh-core-render skia_effect_image_gradient -- --nocapture` | passed |
| `rg "skia_safe" crates/core/frontend/render/src/display_list.rs crates/core/frontend/render/src/render_object.rs crates/core/ui/elements/src && exit 1 || exit 0` | passed |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-05-23 | 10 | 10 | 0 | Codex |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-05-23
