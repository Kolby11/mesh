# Phase 37: Concept Inventory and Vocabulary Lock - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md - this log preserves the alternatives considered.

**Date:** 2026-05-17T20:11:12+02:00
**Phase:** 37-Concept Inventory and Vocabulary Lock
**Areas discussed:** Todo folding, Hard vocabulary replacement, Developer and end-user vocabulary, Concept boundaries, Extensibility with consistency

---

## Todo Folding

| Option | Description | Selected |
|--------|-------------|----------|
| Module todos | Fold the unified manifest and install-resolution todos; ignore the audio popover false match. | yes |
| All matches | Fold all three matched todos, including the unrelated audio popover polish item. | |
| None | Keep all pending todos outside Phase 37 context. | |

**User's choice:** Structured question tool was unavailable; workflow fallback used the recommended path.
**Notes:** The two module-model todos directly affect Phase 37 vocabulary and Phase 38/39 planning. The Phase 31 audio popover todo is unrelated polish debt.

---

## Hard Vocabulary Replacement

| Option | Description | Selected |
|--------|-------------|----------|
| Hard replacement | Replace old names and reject public aliases. | yes |
| Compatibility aliases | Keep old names documented as aliases during migration. | |
| Mixed migration | Keep aliases in docs but warn in diagnostics. | |

**User's choice:** "we should replace the old names, no compatibility alliases, so for example package will be renamed to module hardly"
**Notes:** This supersedes roadmap and requirements wording that still mention compatibility aliases.

---

## Developer And End-User Vocabulary

| Option | Description | Selected |
|--------|-------------|----------|
| Strict technical terms everywhere | Use the canonical vocabulary in all docs and UI. | |
| Layered vocabulary | Developers get precise terms; end users see concrete module/provider/resource language with technical detail available. | yes |
| Simplified user labels only | Hide most technical concepts from users. | |

**User's choice:** Inferred from the request to make the model usable for developers and end users.
**Notes:** Diagnostics should teach the canonical model without presenting old terms as valid alternatives.

---

## Concept Boundaries

| Option | Description | Selected |
|--------|-------------|----------|
| Broad module umbrella | Everything installable is a module; separate concepts describe what it needs, contributes, and can do. | yes |
| Separate package/module layers | Keep package for distribution and module for runtime units. | |
| Provider-centered model | Make providers the central extensibility concept. | |

**User's choice:** Inferred from hard renaming `package` to `module` and the v1.7 goal of one coherent model.
**Notes:** The context locks dependency, capability, contribution, interface, provider, library, and resource pack as separate concepts under the module umbrella.

---

## Extensibility With Consistency

| Option | Description | Selected |
|--------|-------------|----------|
| Open but typed | Allow new interfaces/providers/resources/libraries, with typed registries and diagnostics enforcing consistency. | yes |
| Strict central catalog | Require extension authors to fit a small approved set of core categories. | |
| Freeform extension bags | Allow arbitrary manifest data and leave interpretation to modules. | |

**User's choice:** Inferred from "extensible but consistent and permits innovation."
**Notes:** The selected model preserves innovation through base, extension, and independent interfaces while keeping the Rust core generic and diagnostics strict.

---

## the agent's Discretion

- The exact inventory table shape may be decided during planning.
- The planner may decide sequencing for docs-first versus code-name inventory, but the output must be concrete enough for Phase 38-41.

## Deferred Ideas

- Runtime removal of old manifest support belongs to Phase 38 or later after shipped artifacts are migrated.
- Detailed provider conflict resolution, resource cascade resolution, and settings materialization belong to Phase 38/39.
- Paused v1.6 keybind dispatch, conflict diagnostics, and accessibility proof remain future work after v1.7.
