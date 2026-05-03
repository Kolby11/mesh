# Roadmap: MESH v1.0 Scripting API Stabilization

**Created:** 2026-05-01
**Milestone:** v1.0 Scripting API Stabilization
**Granularity:** standard
**Phase numbering:** reset to Phase 1 because no prior roadmap exists

## Milestone Goal

Stabilize the Luau scripting runtime and plugin authoring surface so external developers can build complete backend service plugins and frontend `.mesh` components using documented, reliable APIs.

## Phases

### Phase 1: Backend Host API Contract

**Goal:** Implement and stabilize the backend Luau host APIs that service plugins need for command execution, config access, logging, service emission, and poll interval control.

**Requirements:** HOST-01, HOST-02, HOST-03, HOST-04, HOST-05, HOST-06

**Success Criteria:**
1. A backend Luau plugin can execute structured commands and shell commands and inspect stdout, stderr, and status.
2. A backend Luau plugin can read its configured settings through `mesh.config()`.
3. A backend Luau plugin can produce plugin-scoped structured logs.
4. A backend Luau plugin can emit service state and adjust polling behavior without shell restart.
5. Backend API failures are surfaced as diagnostics or explicit Luau errors, not silent failures.

**Dependencies:** None
**UI hint:** no

### Phase 2: Service Proxy Delivery

**Goal:** Make `require('@mesh/<service>')` the reliable frontend/backend bridge, including state field exposure, command methods, automatic reactive invalidation on service updates, and service contract diagnostics.

**Progress:** 1/3 plans complete — Plan 01 delivered proxy diagnostics, live state reads, field-level invalidation, and command-routing regressions.

**Requirements:** PROXY-01, PROXY-02, PROXY-03, PROXY-04, PROXY-05, PROXY-06, SURF-06

**Success Criteria:**
1. A frontend `.mesh` script receives live state from a backend service proxy.
2. Backend service emissions mark consuming frontend components dirty so rerender sees the latest proxy state without requiring service-specific callback APIs.
3. Service proxies stay a read-and-command surface; element event handlers such as `onclick` and `onchange` remain attached to template elements rather than service proxies.
4. Service command methods declared by contracts are callable through the proxy.
5. Missing or invalid service contracts produce visible diagnostics.

**Dependencies:** Phase 1
**UI hint:** no

### Phase 3: Frontend Reactivity and Events

**Goal:** Make frontend script reactivity and element event handlers predictable across service updates and user interactions.

**Requirements:** FRONT-01, FRONT-02, FRONT-03, FRONT-04, FRONT-05

**Success Criteria:**
1. Assigning a reactive global marks the component dirty every time.
2. Dirty state rebuilds the widget tree on the next paint.
3. `on_click` and `on_change` handlers run with current state and update the UI.
4. Handler failures are visible in diagnostics.
5. A minimal interactive `.mesh` component can prove event-to-state-to-render behavior end to end.

**Dependencies:** Phase 2
**UI hint:** yes

### Phase 4: Real Core Surfaces

**Goal:** Connect top panel and quick settings to real backend service data, with interactive audio and network controls using the finalized scripting contract.

**Progress:** 2/3 plans complete — Plan 01 delivered executable audio command contract compatibility for quick-settings volume control; Plan 02 delivered quick-settings audio and Wi-Fi controls backed by live proxy state with guarded unavailable/control-denied states.

**Requirements:** SURF-01, SURF-02, SURF-03, SURF-04, SURF-05

**Success Criteria:**
1. Top panel renders at least one real backend service value.
2. Quick settings renders live audio and network state.
3. Quick settings can change audio volume and mute state through service proxy commands.
4. Quick settings can issue supported network commands through the service proxy.
5. The surfaces exercise the same public APIs documented for external plugins.

**Dependencies:** Phase 3
**UI hint:** yes

### Phase 5: Icon Rendering Reliability

**Goal:** Fix the XDG icon rendering path so shell surfaces can rely on named icons, SVG rasterization, raster decode, and graceful missing-icon fallback.

**Requirements:** ICON-01, ICON-02, ICON-03, ICON-04

**Success Criteria:**
1. XDG icon names resolve from configured search paths.
2. SVG icons rasterize and paint correctly at requested sizes.
3. Raster icons decode and paint correctly at requested sizes.
4. Missing icons produce diagnostics and non-crashing fallback behavior.
5. Core surfaces render expected icons without special-case asset paths.

**Dependencies:** Phase 4
**UI hint:** yes

### Phase 6: Documentation and Reference Plugin

**Goal:** Write the scripting API reference and validate it by building a fresh backend service plus frontend component using only the documented API.

**Requirements:** DOCS-01, DOCS-02, DOCS-03, DOCS-04, DOCS-05

**Success Criteria:**
1. API reference covers frontend reactivity, event handlers, service proxies, backend host APIs, and service emission.
2. A new backend service plugin validates the backend documentation.
3. A new frontend `.mesh` component validates the frontend documentation.
4. The reference plugin proves backend emissions update frontend UI.
5. Documentation is accurate enough that a developer does not need to read Rust source for the covered APIs.

**Dependencies:** Phase 5
**UI hint:** no

## Traceability

| Requirement | Phase |
|-------------|-------|
| HOST-01 | Phase 1 |
| HOST-02 | Phase 1 |
| HOST-03 | Phase 1 |
| HOST-04 | Phase 1 |
| HOST-05 | Phase 1 |
| HOST-06 | Phase 1 |
| PROXY-01 | Phase 2 |
| PROXY-02 | Phase 2 |
| PROXY-03 | Phase 2 |
| PROXY-04 | Phase 2 |
| PROXY-05 | Phase 2 |
| PROXY-06 | Phase 2 |
| FRONT-01 | Phase 3 |
| FRONT-02 | Phase 3 |
| FRONT-03 | Phase 3 |
| FRONT-04 | Phase 3 |
| FRONT-05 | Phase 3 |
| SURF-01 | Phase 4 |
| SURF-02 | Phase 4 |
| SURF-03 | Phase 4 |
| SURF-04 | Phase 4 |
| SURF-05 | Phase 4 |
| SURF-06 | Phase 2 |
| ICON-01 | Phase 5 |
| ICON-02 | Phase 5 |
| ICON-03 | Phase 5 |
| ICON-04 | Phase 5 |
| DOCS-01 | Phase 6 |
| DOCS-02 | Phase 6 |
| DOCS-03 | Phase 6 |
| DOCS-04 | Phase 6 |
| DOCS-05 | Phase 6 |

**Coverage:**
- v1 requirements: 32 total
- Mapped to phases: 32
- Unmapped: 0

## Backlog

No backlog items yet.

### Phase 7: Plugin Download and Hot-Install Pipeline

**Goal:** [To be planned]
**Requirements**: TBD
**Depends on:** Phase 6
**Plans:** 0 plans

Plans:
- [ ] TBD (run /gsd-plan-phase 7 to break down)

---
*Roadmap created: 2026-05-01 after milestone v1.0 start*
