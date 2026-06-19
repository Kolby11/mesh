# Module Architecture — Design Directions

Status: brainstorm / decision record. Created 2026-06-19.
Implementation: A+B landed 2026-06-19 (compact `mesh.surface` block; see
`module-system.md` → "Surface configuration"). C landed 2026-06-19 (consumer
capabilities pruned from shipped backends; `provider_declares_consumer_capability`
diagnostic). D part 1 landed 2026-06-19 (sole-implementer provider
auto-selection). D part 2 (optional `interface.toml`) and F pending; E deferred.

This document captures the directions considered for making the MESH module
system **easy to use, extensible, unified (theming/keyboard/icons/i18n), and
configurable per module**, the arguments for and against each, and the path we
selected. It complements the existing redesign backlog in `todo.md` ("Module
system redesign research") and the shipped model in
[`module-system.md`](module-system.md) / [`module-vocabulary.md`](module-vocabulary.md).

---

## What we keep (not up for debate)

The core spine is correct and stays:

- **Everything is a module.** One installable unit, one manifest shape.
- **Interface / provider / frontend separation.** Frontends depend on a
  contract (`mesh.audio`), backends implement it, the root graph selects the
  active provider. This is the seam that lets users ship their own backend
  without touching any frontend.
- **Core is a wiring layer.** Service logic lives in Luau; Rust routes
  generically. No `if service == "audio"` branches.
- **Installed graph is the single source of truth** at runtime.

The redesign is about removing **friction**, not replacing the spine.

---

## The evaluation lens: "simple" is two axes

A change can be simple on one axis and complex on the other. Conflating them is
the main design trap here.

- **Ergonomic-simple** — fewer things to *write* in a manifest/file.
- **Conceptual-simple** — fewer things to *understand* about the system.

"Unified" tracks the **conceptual** axis. A change that deletes boilerplate but
adds hidden/implicit behavior buys ergonomic-simple at the cost of
conceptual-simple, and is *not* unifying even though it feels lighter.

Every direction below is graded on both axes.

---

## Problems in the current model (evidence)

1. **Manifest ceremony.** `navigation-bar/module.json` is ~191 lines; ~110 are
   a `provides.settings.schema.surface` block (anchor/layer/width/height/
   keyboard_mode/visible_on_start) that is copy-pasted into every frontend
   module (`quick-settings` repeats it almost verbatim). Boilerplate the author
   should never see.
2. **Configuration is split across 3+ locations with no guessable rule.**
   `provides.settings.schema.surface` (user-editable) vs `surfaceLayout`
   (renderer policy) vs `config/settings.json` (overrides) vs root
   `config/module.json`. `keyboard_mode` lives in *two* of them. The settings
   flow needs a diagram in `llm-context.md` to explain.
3. **Docs already drifted from code.** The docs say a backend must *not* request
   `service.audio.read`/`control`, but `pipewire-audio/module.json` requests
   both. Hand-maintained capability lists are too easy to get wrong, so nobody
   can tell what is actually load-bearing.
4. **High ceremony floor for innovation.** To surface e.g. a CPU temperature an
   author must create an interface module (+`interface.toml`), a backend module,
   register both in `config/module.json`, and select the provider — four
   artifacts before anything renders.
5. **"Unified" only in philosophy, not in surface.** Theming, keyboard, icons,
   and i18n are each a separate mini-schema (`mesh.theme`, `mesh.keybinds`,
   `mesh.uses.resources.icons` + `iconRequirements`, `mesh.i18n` +
   `provides.i18n`). They rhyme but the author learns four shapes.

---

## Directions considered

### A. Base surface schema (convention over configuration)

Core ships the canonical surface schema (anchor/layer/size/keyboard/visibility)
automatically for every `kind: "frontend"` module. The author declares only
**deltas** — defaults and clamps:

```json
"surface": { "anchor": "top", "height": 56, "size": "fixed" }
```

This single object replaces both the ~110-line `settings.schema.surface` block
and `mesh.surfaceLayout`.

- Ergonomic: ↑↑ (≈150 lines → ≈5)
- Conceptual: ↑ (one canonical surface model every surface shares)
- Cost: slight loss of discoverability — the full schema now lives in core, not
  in the author's file. Mitigated by tooling/`mesh.debug` that can print the
  effective expanded schema.
- **Verdict: genuinely unifying. Selected.**

### B. One config block, tagged by *audience* not by *location*

Collapse `surfaceLayout` + `provides.settings` into a single `config` block
where each field is tagged `editable: true|false` (or `policy: true`).
User-editable fields generate settings UI; policy fields are renderer hints.
Same schema language, one place to look, override file unchanged.

- Ergonomic: ↑
- Conceptual: ↑↑ (collapses two blocks + the flow diagram into one model)
- Caveat: a few fields are not cleanly binary — `keyboard_mode` is a module
  default *and* user-overridable *and* runtime-transferable. The tag describes
  who edits the durable default; runtime focus transfer stays shell-owned. The
  model still holds.
- **Verdict: genuinely unifying. Selected.**

### C. Capability inference — REJECTED as stated, REFRAMED

Original idea: derive capabilities from usage (`implements mesh.audio` → provider
powers; calling a `control` method → `service.audio.control`).

- Ergonomic: ↑
- Conceptual: ↓ — capabilities are the **security model**. Inference makes "what
  can this module do?" unanswerable by reading the file. Auditability is a
  feature here, not boilerplate.
- The drift in problem #3 is a *bug to delete*, not evidence that inference is
  needed.
- **Reframe (selected): keep capabilities fully explicit; delete the redundant /
  derivable entries from the vocabulary so there is nothing to get wrong.** E.g.
  a backend implementing `mesh.audio` should never restate consumer
  capabilities. That is unifying; inference is not.

### D. Tiered authoring (inline interface vs full split) — REJECTED as stated, REFRAMED

Original idea: allow a small module to declare an *inline* interface, then
"promote" it to a standalone interface module later.

- Ergonomic: ↑↑
- Conceptual: ↓↓ — two mental models for the same thing plus a migration step
  between them is the literal opposite of unified.
- The instinct (lower the wall for custom backends) is right; a second path is
  the wrong fix.
- **Reframe (selected): make the *single* path cheap.** Auto-register a provider
  when exactly one module implements an interface; make `interface.toml`
  optional for v0 by inferring the contract from the backend's emitted state;
  reduce required `config/module.json` wiring. One model, less ritual.

### E. Unify the four contribution systems — CONDITIONAL

Make theming tokens, icons, i18n catalogs, and keybinds instances of one typed
`contributes` model with consistent declaration + diagnostics.

- Ergonomic: → (neutral)
- Conceptual: ↑↑ *if the shared shape is honest*, ↓ if forced. Icons, i18n,
  keybinds, and tokens genuinely differ; forcing them into one schema risks a
  leaky abstraction harder to understand than four clear ones.
- **Verdict: pursue only where they share real structure (id, source path,
  override rules, diagnostics) while keeping kind-specific fields. Not a goal in
  itself. Deferred behind A/B.**

### F. Root-graph auto-discovery

`config/module.json` hand-lists every module with `path` + `kind` + `enabled`,
re-stating each module's own manifest. Discovery auto-populates from
`modulesDir`; the root file holds only **decisions** — active providers,
disabled modules, layout entrypoint, active theme/locale/icon pack.

- Ergonomic: ↑
- Conceptual: ↑ (root file describes choices, not inventory)
- Cost: implicit-enable needs a clear default + an explicit disable list.
- **Verdict: safe, mild win. Selected (lower priority).**

---

## Decision

**Selected path: A + B as the headline change, plus C-reframed and D-reframed,
with F as follow-on and E deferred.**

Rationale:

- **A + B is the real unlock.** They attack problems #1 and #2 directly and are
  the only directions that improve *both* axes. They make authoring and
  configuring a module feel trivial *and* uniform — the intersection of "easy",
  "unified", and "configurable". They also make generated settings UI
  (`todo.md`: "Settings UI should be generated from contributed schemas")
  natural, because there is now one schema model to render.
- **C-reframed** is cheap cleanup that fixes a live drift bug (#3) and keeps the
  security model explicit. Pure win, no new concepts.
- **D-reframed** removes the innovation ceremony wall (#4) without adding a
  second authoring path — preserving the one-model property A/B establish.
- **F** shrinks the root graph (#-adjacent) once the manifest shape settles.
- **E** is deferred: it is conceptually attractive but risks a forced
  abstraction; revisit only after A/B prove the unified config model and only
  where the four systems share honest structure.

What we explicitly do **not** do: capability inference (C original) or a
parallel inline-interface path (D original). Both buy ergonomic-simple at the
cost of conceptual-simple — exactly the failure mode this redesign exists to
avoid.

### Sequencing

1. **A+B** — unified `surface` / `config` block; expand in core into schema +
   policy; validate against `navigation-bar` and `quick-settings` (most-repeated
   boilerplate, two different size policies, so a good stress test).
2. **C-reframed** — prune redundant capabilities from vocabulary + shipped
   manifests; add a diagnostic for restating derivable capabilities.
3. **D-reframed** — auto-register single-implementer providers; optional
   `interface.toml` for v0.
4. **F** — root-graph auto-discovery; root file holds decisions only.
5. **E** — revisit unified `contributes` shape, scoped to honest shared
   structure only.

### Open questions

- Exact tag for B: `editable: false` vs `policy: true` vs a separate `policy`
  sub-block. Decide while drafting A+B against the two real modules.
- For D-reframed, how is an inferred-contract backend represented in the graph so
  multi-provider resolution still works if a second implementer appears later?
- F's implicit-enable default and the shape of the explicit disable list.
