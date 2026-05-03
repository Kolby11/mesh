# Requirements: MESH v1.0 Scripting API Stabilization

**Defined:** 2026-05-01
**Core Value:** A developer with zero MESH knowledge can write a working top panel plugin and backend service in one sitting, guided only by the API documentation.

## v1 Requirements

### Backend Host API

- [ ] **HOST-01**: Backend Luau scripts can call `mesh.exec(cmd, args)` with structured arguments and receive stdout, stderr, and exit status.
- [ ] **HOST-02**: Backend Luau scripts can call `mesh.exec_shell(cmd)` for shell-style commands and receive stdout, stderr, and exit status.
- [ ] **HOST-03**: Backend Luau scripts can call `mesh.config()` and receive plugin settings as a Luau table.
- [ ] **HOST-04**: Backend Luau scripts can call `mesh.log(level, msg)` and produce structured log entries associated with the plugin.
- [ ] **HOST-05**: Backend Luau scripts can call `mesh.service.emit(payload)` and publish JSON-compatible state payloads to the shell.
- [ ] **HOST-06**: Backend Luau scripts can call `mesh.service.set_poll_interval(ms)` and affect the backend poll loop without restarting the shell.

### Service Proxy

- [x] **PROXY-01**: Frontend `.mesh` scripts can call `require('@mesh/<service>')` and receive a proxy table for the active provider.
- [x] **PROXY-02**: Service proxy tables expose the latest backend-emitted state fields as Luau values.
- [ ] **PROXY-03**: Service proxy tables expose command methods declared by the service contract.
- [x] **PROXY-04**: Backend state emissions invalidate frontend components that consume that service state, so rerender sees the latest proxy values without requiring proxy-scoped callback APIs.
- [x] **PROXY-05**: Service updates stay separate from element events; frontend scripts can observe the latest proxy state on rerender without `on_<service>_update()` handlers.
- [x] **PROXY-06**: Frontend scripts fail visibly, with diagnostics, when requiring a missing or invalid service contract.

### Frontend Reactivity

- [x] **FRONT-01**: Assigning a reactive global inside a frontend `<script>` marks the component dirty.
- [x] **FRONT-02**: Dirty frontend script state triggers a widget tree rebuild on the next paint.
- [x] **FRONT-03**: Element `on_click` handlers run reliably with the current script state.
- [x] **FRONT-04**: Element `on_change` handlers run reliably for interactive controls such as sliders and toggles.
- [x] **FRONT-05**: Handler failures are reported through diagnostics instead of silently failing.

### Core Services and Surfaces

- [ ] **SURF-01**: The top panel renders live data from at least one real backend service.
- [ ] **SURF-02**: The quick settings surface renders live audio state from a real backend provider.
- [x] **SURF-03**: The quick settings surface can change audio volume and mute state through service commands.
- [ ] **SURF-04**: The quick settings surface renders live network state from a real backend provider.
- [ ] **SURF-05**: The quick settings surface can toggle or command network state through the service proxy contract where supported.
- [ ] **SURF-06**: Audio, network, power, and media service contracts document their state fields, callbacks, and commands.

### Icon Rendering

- [ ] **ICON-01**: XDG icon names resolve through configured icon theme search paths.
- [ ] **ICON-02**: SVG icons rasterize correctly through the render pipeline.
- [ ] **ICON-03**: Raster icons decode and paint correctly at requested sizes.
- [ ] **ICON-04**: Missing icons produce diagnostics and non-crashing fallback behavior.

### Documentation and Validation

- [ ] **DOCS-01**: The scripting API reference documents frontend reactivity, event handlers, and service proxy usage.
- [ ] **DOCS-02**: The scripting API reference documents backend host APIs and service emission patterns.
- [ ] **DOCS-03**: A fresh reference backend service plugin validates the documented backend API.
- [ ] **DOCS-04**: A fresh reference frontend `.mesh` component validates the documented frontend API.
- [ ] **DOCS-05**: The reference plugin proves backend emissions update frontend UI without reading Rust source.

## v2 Requirements

### Developer Tooling

- **LSP-01**: `.mesh` LSP completions cover service proxy fields and commands.
- **LSP-02**: `.mesh` LSP hover documentation reflects the scripting API reference.

### Additional Shell Surfaces

- **NOTIF-01**: Notification center surface consumes a live notification service.
- **LAUNCH-01**: Launcher surface is stabilized against the same scripting API contract.

### Distribution

- **PKG-01**: Plugin package manager supports installing third-party plugins.
- **PKG-02**: Signed or sandboxed plugin packages protect users from untrusted plugin code.

## Out of Scope

| Feature | Reason |
|---------|--------|
| LSP completions and hover for `.mesh` service APIs | Runtime correctness and documentation are higher priority for v1. |
| Notification center surface | Explicitly deferred from this milestone. |
| Launcher surface stabilization | Explicitly deferred from this milestone. |
| Plugin package manager or signed packages | Later distribution work, not required to stabilize the MVP scripting API. |
| Rewriting core services in Rust | Project decision keeps service-specific logic in Luau backend plugins. |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| HOST-01 | Phase 1 | Pending |
| HOST-02 | Phase 1 | Pending |
| HOST-03 | Phase 1 | Pending |
| HOST-04 | Phase 1 | Pending |
| HOST-05 | Phase 1 | Pending |
| HOST-06 | Phase 1 | Pending |
| PROXY-01 | Phase 2 | Complete |
| PROXY-02 | Phase 2 | Complete |
| PROXY-03 | Phase 2 | Pending |
| PROXY-04 | Phase 2 | Complete |
| PROXY-05 | Phase 2 | Complete |
| PROXY-06 | Phase 2 | Complete |
| FRONT-01 | Phase 3 | Complete |
| FRONT-02 | Phase 3 | Complete |
| FRONT-03 | Phase 3 | Complete |
| FRONT-04 | Phase 3 | Complete |
| FRONT-05 | Phase 3 | Complete |
| SURF-01 | Phase 4 | Pending |
| SURF-02 | Phase 4 | Pending |
| SURF-03 | Phase 4 | Complete |
| SURF-04 | Phase 4 | Pending |
| SURF-05 | Phase 4 | Pending |
| SURF-06 | Phase 2 | Pending |
| ICON-01 | Phase 5 | Pending |
| ICON-02 | Phase 5 | Pending |
| ICON-03 | Phase 5 | Pending |
| ICON-04 | Phase 5 | Pending |
| DOCS-01 | Phase 6 | Pending |
| DOCS-02 | Phase 6 | Pending |
| DOCS-03 | Phase 6 | Pending |
| DOCS-04 | Phase 6 | Pending |
| DOCS-05 | Phase 6 | Pending |

**Coverage:**
- v1 requirements: 32 total
- Mapped to phases: 32
- Unmapped: 0

---
*Requirements defined: 2026-05-01*
*Last updated: 2026-05-01 after milestone v1.0 start*
