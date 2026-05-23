# Phase 58: Backend Capabilities, Diagnostics, And Rollback - Context

**Gathered:** 2026-05-23
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 58 makes painter backend behavior inspectable and reversible. It owns active backend identity, capability snapshots, unsupported-feature diagnostics, and rollback documentation. It does not implement a second backend or promote Vello/AnyRender production behavior.

</domain>

<decisions>
## Implementation Decisions

### Backend Snapshot Surface
- Expose backend information as backend-neutral snapshots from `FrontendRenderEngine`, not by leaking Skia types or crate-private backend structs.
- Include backend id, capability booleans, recent unsupported-feature diagnostics, and rollback authority.
- Keep diagnostics source context optional and serializable so future shell debug payloads can add node/style context without API churn.
- Preserve the existing Skia default backend.

### Rollback
- Use the existing renderer rollback authority (`mesh-software-renderer`) as the published fallback identity.
- Treat unsupported painter commands as diagnostics, not automatic backend swaps.
- Document that backend selection and rollback remain explicit migration controls until shipped-surface proof accepts parity.
- Keep the public API additive so Phase 59 can consume it for shipped proof.

### the agent's Discretion
The agent may choose exact snapshot struct names and test selectors as long as the public surface stays backend-neutral and avoids Skia-specific type leakage.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `PaintBackend`, `PainterBackendCapabilities`, and `PainterDiagnostic` already exist in `crates/core/frontend/render/src/surface/painter/backend.rs`.
- `FrontendRenderEngine` already stores recent painter diagnostics and exposes the active backend id.
- `renderer_library_rollback_authority()` already identifies the current rollback authority.

### Established Patterns
- Painter backend tests live in `crates/core/frontend/render/src/surface/painter/tests.rs`.
- Render crate docs in `crates/core/frontend/render/README.md` describe backend-neutral painter responsibilities.
- Public render APIs are re-exported from `surface/mod.rs` and `lib.rs`.

### Integration Points
- Phase 59 can call `FrontendRenderEngine::paint_backend_snapshot()` during shipped-surface proof.
- Shell debug payloads can later serialize the snapshot without depending on crate-private Skia backend types.

</code_context>

<specifics>
## Specific Ideas

Keep the phase small and focused: expose and test the snapshot API, then document its intended diagnostic/rollback use.

</specifics>

<deferred>
## Deferred Ideas

Runtime backend-selection configuration and a second production paint backend remain future work.

</deferred>
