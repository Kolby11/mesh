---
phase: 46
slug: renderer-library-dependency-and-adapter-foundation
status: verified
threats_open: 0
asvs_level: 1
created: 2026-05-18
---

# Phase 46 - Security

Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Cargo feature boundary | Renderer-library crates are present in production manifests but disabled by default. | Build-time dependency selection and optional transitive dependency fan-out. |
| Internal renderer adapter boundary | `mesh-core-render` exposes renderer-library status records for later adapter work. | Internal Rust status metadata; no layout, paint, text, shell, presentation, or author-facing data crossing. |
| Documentation/adoption gate boundary | Renderer migration docs define when optional libraries may become behavior. | Human-readable dependency, rollback, CI, and promotion policy used by future phases. |

---

## Threat Register

| Threat ID | Category | Component | Disposition | Mitigation | Status |
|-----------|----------|-----------|-------------|------------|--------|
| T-46-01-01 | Tampering | Cargo manifests | mitigate | `mesh-core-render` has `default = []`; all renderer candidates are optional dependencies behind explicit `renderer-*` feature names. Focused default check passed. | closed |
| T-46-01-02 | Denial of Service | Cargo/Rust toolchain | mitigate | Workspace pins Rust-compatible versions: `taffy 0.10.1`, `parley 0.7.0`, `accesskit 0.24.0`, `anyrender 0.10.0`, and `vello_encoding 0.5.1`; docs record Rust 1.88-incompatible later versions as deferred. | closed |
| T-46-01-03 | Denial of Service | Renderer dependency fan-out | mitigate | Full `vello` is not present in manifests; only optional `vello_encoding` is available behind `renderer-vello-encoding`; enabled aggregate check passed. | closed |
| T-46-02-01 | Tampering | `mesh-core-render` adapter seam | mitigate | `library_adapters.rs` contains data-only status records and rollback authority; scan found no calls into painter, display-list, shell, or presentation paths. | closed |
| T-46-02-02 | Repudiation | Renderer-library feature status | mitigate | `renderer_library_statuses_track_feature_flags` and `renderer_library_rollback_authority_stays_mesh_software_renderer` cover disabled/enabled status and rollback authority; focused tests passed. | closed |
| T-46-02-03 | Spoofing | Author-facing `.mesh` contract | mitigate | Adapter seam stays inside `mesh-core-render`; renderer contract states Phase 46 is disabled by default and `.mesh` syntax, layout semantics, service proxies, shell lifecycle, and author APIs do not change. | closed |
| T-46-03-01 | Information Disclosure | Dependency risk documentation | mitigate | `docs/renderer-migration.md` contains the Phase 46 dependency record with feature names, versions, Rust constraints, default-disabled status, CI gates, and rollback path. | closed |
| T-46-03-02 | Spoofing | Renderer contract docs | mitigate | `docs/frontend/renderer-contract.md` explicitly states the features are internal and disabled by default and author APIs do not change. | closed |
| T-46-03-03 | Denial of Service | Shipped renderer/shell surfaces | mitigate | Plan 03 records green focused gates for default and enabled render builds, renderer-library tests, proof tests, and Phase 44 shell regression tests; workspace failure is documented as a pre-existing Phase 26 profiling baseline blocker outside Phase 46. | closed |

*Status: open - closed*
*Disposition: mitigate (implementation required) - accept (documented risk) - transfer (third-party)*

---

## Accepted Risks Log

No accepted risks.

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-05-18 | 9 | 9 | 0 | Codex |

---

## Audit Evidence

- PASS: `rg -n "^vello\\s*=" Cargo.toml crates/core/frontend/render/Cargo.toml` returned no matches.
- PASS: `rg -n "paint_frontend_tree|RetainedDisplayList::|FrontendSurfaceComponent|taffy::|parley::|accesskit::|anyrender::|vello_encoding::" crates/core/frontend/render/src/library_adapters.rs` returned no matches.
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render renderer_library` ran 2 tests, all passed.
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo check -p mesh-core-render --features renderer-libraries`.
- PASS: Artifact inspection confirmed optional feature gates, status seam exports, rollback authority, dependency record, ownership classification, and author-contract non-effect language.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-05-18
