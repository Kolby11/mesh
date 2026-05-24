# Roadmap: MESH

## Milestones

- 🚧 **v1.14 Unified Luau Import Contract** — Phases 74-78 planned
- ✅ **v1.13 Manifest I18n Contract** — Phases 70-73 shipped 2026-05-24 ([archive](milestones/v1.13-ROADMAP.md))
- ✅ **v1.12 Module Object Contract** — Phases 65-69 shipped 2026-05-23 ([archive](milestones/v1.12-ROADMAP.md))
- ✅ **v1.11 Surface Keybind Completion** — Phases 60-64 shipped 2026-05-23 ([archive](milestones/v1.11-ROADMAP.md))
- ✅ **v1.10 Painter Engine** — Phases 51-59 shipped 2026-05-23 ([archive](milestones/v1.10-ROADMAP.md))

## Intent

Unify MESH Luau authoring around explicit `require(...)` imports across frontend
and backend runtimes. Authors should learn one model for shell APIs,
service/interface proxies, module-specific variables/state, module objects,
libraries, and frontend components instead of switching between implicit global
`mesh`, explicit interface import globals, and `.mesh` component
`import ... from` syntax.

## Phase Summary

| # | Phase | Goal | Requirements | Success Criteria |
|---|-------|------|--------------|------------------|
| 74 | Import Resolver Contract | Define and implement the shared require resolver for shell APIs, service/interface proxies, and module scope imports. | LUAIMP-01, LUAIMP-02, LUAIMP-03, LUAIMP-04, LUAIMP-05 | 5 |
| 75 | Runtime Parity | Apply the canonical require model consistently across frontend and backend Luau contexts. | LUART-01, LUART-02, LUART-03, LUART-04, LUART-05, LUART-06 | 6 |
| 76 | Component Require Imports | Bring frontend local/module component imports into the unified require model while preserving current import syntax. | LUACOMP-01, LUACOMP-02, LUACOMP-03, LUACOMP-04 | 4 |
| 77 | Compatibility Migration Proof | Keep existing globals/imports working, add migration diagnostics, and migrate shipped proof modules to the new style. | LUACOMPAT-01, LUACOMPAT-02, LUACOMPAT-03, LUACOMPAT-04 | 4 |
| 78 | Author Docs And Final Proof | Publish the unified Luau import contract and prove it through docs, regression tests, and shipped modules. | LUADOC-01, LUADOC-02, LUADOC-03, LUADOC-04 | 4 |

## Execution Rules

- Treat `require(...)` as the canonical author-facing import mechanism for Luau runtimes.
- Preserve global `mesh` and current `.mesh import` syntax during v1.14; migration should be diagnostic and documented, not breaking.
- Keep frontend and backend runtime behavior aligned unless a capability or host-context difference is explicit.
- Do not invent JavaScript-style named imports until the Luau-native require contract is stable.
- Shell APIs must remain capability-gated and observable through diagnostics.
- Module-specific variables and state must be accessed through an explicit module-scope import that is isolated by module identity.
- Prove behavior on shipped navigation, audio popover, and backend provider scripts, not only synthetic fixtures.

## Phases

- [ ] Phase 74: Import Resolver Contract
- [ ] Phase 75: Runtime Parity
- [ ] Phase 76: Component Require Imports
- [ ] Phase 77: Compatibility Migration Proof
- [ ] Phase 78: Author Docs And Final Proof

### Phase 74: Import Resolver Contract

**Goal:** Define and implement the shared require resolver for shell APIs, service/interface proxies, and module scope imports.

**Requirements:** LUAIMP-01, LUAIMP-02, LUAIMP-03, LUAIMP-04, LUAIMP-05

**Status:** Not started

**Success criteria:**
1. One resolver path handles canonical shell API modules and service/interface proxies.
2. `require("mesh.audio@>=1.0")` behavior remains compatible.
3. Shell APIs such as locale/log/events/ui/popover are available through documented require specifiers.
4. Unsupported or unavailable imports produce consistent pcall-safe errors and diagnostics.
5. A canonical module-scope import exposes module-specific variables, persistent state, exports, and lifecycle metadata.

### Phase 75: Runtime Parity

**Goal:** Apply the canonical require model consistently across frontend and backend Luau contexts.

**Requirements:** LUART-01, LUART-02, LUART-03, LUART-04, LUART-05, LUART-06

**Status:** Not started

**Success criteria:**
1. Backend scripts can require canonical shell APIs and allowed interfaces.
2. Frontend scripts can require canonical shell APIs and allowed interfaces.
3. Capability denial is enforced and diagnosed uniformly in both runtime contexts.
4. Imported module-object APIs preserve v1.12 state/export/event semantics.
5. Module-specific state behaves identically in frontend and backend contexts for reads, writes, events, and reload boundaries.
6. Module-scoped variables remain isolated by module identity.

### Phase 76: Component Require Imports

**Goal:** Bring frontend local/module component imports into the unified require model while preserving current import syntax.

**Requirements:** LUACOMP-01, LUACOMP-02, LUACOMP-03, LUACOMP-04

**Status:** Not started

**Success criteria:**
1. Local `.mesh` components can be imported through a require-shaped authoring path.
2. Module-provided frontend components can be imported through a require-shaped authoring path.
3. Existing `import Alias from "..."` component syntax keeps working.
4. Compiler and runtime diagnostics identify missing, duplicate, or unsupported component import targets.

### Phase 77: Compatibility Migration Proof

**Goal:** Keep existing globals/imports working, add migration diagnostics, and migrate shipped proof modules to the new style.

**Requirements:** LUACOMPAT-01, LUACOMPAT-02, LUACOMPAT-03, LUACOMPAT-04

**Status:** Not started

**Success criteria:**
1. Existing global `mesh` calls continue working.
2. Existing explicit interface import globals continue working.
3. Diagnostics and docs point authors toward explicit require imports.
4. Navigation, audio popover, and backend providers demonstrate the new import style without behavior regressions.

### Phase 78: Author Docs And Final Proof

**Goal:** Publish the unified Luau import contract and prove it through docs, regression tests, and shipped modules.

**Requirements:** LUADOC-01, LUADOC-02, LUADOC-03, LUADOC-04, LUADOC-05

**Status:** Not started

**Success criteria:**
1. Module-system and frontend syntax docs describe canonical require namespaces and pcall behavior.
2. Backend docs show the same import model for services, shell APIs, module objects, and libraries.
3. Regression tests cover synthetic and shipped-module require behavior.
4. Requirements traceability and validation artifacts cover all v1.14 requirements.
5. Docs explain module-specific variables/state naming, persistence, observation, and reload behavior.

## Backlog

### Future: Named Import Syntax

Named import syntax remains future work. Authors can use Luau table destructuring
or local aliases after `require(...)` while the runtime contract stabilizes.

### Future: Package Distribution

Remote package fetching, third-party dependency resolution, and LSP import
completion remain future work after the runtime import contract is stable.
