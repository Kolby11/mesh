# Phase 101: Per-Region Damage - Research

**Researched:** 2026-06-10
**Domain:** Wayland damage protocol / Rust present path refactor
**Confidence:** HIGH

## Summary

Phase 101 is a pure plumbing refactor: `effective_damage.rects` (a `Vec<DamageRect>` already computed correctly inside paint) must flow all the way to `wl_surface::damage_buffer` calls at commit time, replacing the single unioned `Option<DamageRect>` that is threaded today.

The render side already stores per-region damage in `EffectiveDamage::rects`. The bottleneck is `take_present_damage()` which returns `Option<DamageRect>` — it discards the per-rect granularity computed in `paint()` and collapses it into one rect via `merge_optional_damage`. Every layer below that (`PresentationEngine::present_with_damage`, `LayerShellBackend::present_with_damage`, `SurfaceEntry::copy_into_shm_buffer`, `SurfaceEntry::attach_shm_buffer`) follows the same single-rect contract.

The change is a signature change that touches exactly six locations: the `ShellComponent` trait, `FrontendSurfaceComponent`'s storage + `paint()` + `take_present_damage`, the shell render dispatch loop, `PresentationEngine::present_with_damage`, and the Wayland backend's attach helper. The debug overlay already tracks `damage_rect_count` in `RetainedPaintSnapshot` and serialises it to the debug bus — no new field is needed; DMGE-03 requires the count to reflect the rects sent to the compositor per commit, which is what the present path will carry.

**Primary recommendation:** Thread `Vec<DamageRect>` from `take_present_damage` → render dispatch → `PresentationEngine::present_with_damage` → `attach_shm_buffer`, calling `wl_surface.damage_buffer` once per rect (capped at 16). The SHM `pending_damage` accumulation in `copy_into_shm_buffer` must be adapted to accumulate a list rather than a union so that stale SHM slots are recopied fully when their `pending_damage` exceeds 16 rects.

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Cap at 16 rects per commit (DMGE-02); when >16 rects exist, fall back to the full-surface unioned DamageRect as a single element rather than truncating to the first 16 partial rects
- `Vec<DamageRect>` flows from `effective_damage.rects` (already computed in `select_effective_damage_rects`) through `take_present_damage()` to the Wayland commit
- Empty `Vec` means no changed pixels → skip present entirely (current `None` → skip behavior must be preserved)
- Debug overlay damage rect count goes in profiling/debug section alongside existing damage area metrics (DMGE-03)

### Claude's Discretion
All implementation choices are at Claude's discretion — pure infrastructure phase.

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DMGE-01 | Shell passes a `Vec<DamageRect>` from the retained renderer through the present path instead of a single unioned rect | `effective_damage.rects` already exists; signature change flows it up through `take_present_damage` → render dispatch → `PresentationEngine` |
| DMGE-02 | Presentation calls `wl_surface::damage_buffer` once per dirty rect (capped at 16) per frame commit | `attach_shm_buffer` calls `damage_buffer` once today; change to loop; fallback to full-surface union when count > 16 |
| DMGE-03 | Debug/profiling exposes damage rect count per frame alongside existing damage metrics | `RetainedPaintSnapshot.damage_rect_count` already computed and serialised — expose present-time rect count in profiling snapshot |
</phase_requirements>

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Per-region damage accumulation | Component / paint layer | — | `EffectiveDamage::rects` produced in `FrontendSurfaceComponent::paint()` |
| Present-time damage list storage | `FrontendSurfaceComponent` | — | Replaces `last_present_damage: Option<DamageRect>` |
| Trait contract change | `ShellComponent` trait | `FrontendSurfaceComponent` impl | API boundary between shell and component |
| Damage-to-protocol mapping | Wayland backend `SurfaceEntry` | `PresentationEngine` | Only the Wayland path calls `damage_buffer`; dev-window ignores damage |
| SHM buffer stale-copy tracking | `SurfaceEntry::copy_into_shm_buffer` | — | Must accumulate per-frame damage lists across SHM pool slots |
| Debug metrics | `RetainedPaintSnapshot` / profiling serialiser | — | Already carries `damage_rect_count`; present-path count needs surfacing |

---

## Standard Stack

No new external crates required. All work is internal Rust refactoring within existing crates.

### Existing crates involved

| Crate | Role in this phase |
|-------|--------------------|
| `mesh-core-frontend-host` | `ShellComponent` trait — change `take_present_damage` return type |
| `mesh-core-shell` (component) | `FrontendSurfaceComponent` storage, `paint()`, `take_present_damage` |
| `mesh-core-shell` (runtime) | `render.rs` dispatch loop — reads `take_present_damage`, passes to `present_with_damage` |
| `mesh-core-presentation` | `PresentationEngine::present_with_damage` — change `Option<DamageRect>` → `Vec<DamageRect>` |
| `mesh-core-presentation` (wayland) | `LayerShellBackend`, `SurfaceEntry` — `copy_into_shm_buffer`, `attach_shm_buffer` |
| `mesh-core-debug` | `RetainedPaintSnapshot` already has `damage_rect_count`; confirm present-path field |

### Package Legitimacy Audit

No external packages are installed in this phase.

---

## Architecture Patterns

### Present-path data flow (current)

```
FrontendSurfaceComponent::paint()
  effective_damage.rects: Vec<DamageRect>   ← computed correctly, multi-rect
  paint_damage = if full { Some(surface) } else { effective_damage.rect }
  last_present_damage = merge_optional_damage(last, paint_damage, surface)
                                             ← discards per-rect granularity here
                                                  |
                          Option<DamageRect>  ◄───┘
                                  |
take_present_damage() → Option<DamageRect>
                                  |
Shell render.rs::render_components()
  present_damage: Option<DamageRect>
                                  |
PresentationEngine::present_with_damage(…, Option<DamageRect>)
                                  |
SurfaceEntry::copy_into_shm_buffer(…, Option<DamageRect>)
  → single DamageRect  (union or full)
                                  |
SurfaceEntry::attach_shm_buffer(…, DamageRect)
  wl_surface.damage_buffer(x, y, w, h)   ← single call
  wl_surface.commit()
```

### Present-path data flow (target)

```
FrontendSurfaceComponent::paint()
  effective_damage.rects: Vec<DamageRect>   ← unchanged
  last_present_damage_rects: Vec<DamageRect>  ← replaces last_present_damage
                                  (accumulate rects each paint via push_damage_rect)
                                  |
take_present_damage() → Vec<DamageRect>    ← return and clear
                                  |
Shell render.rs::render_components()
  present_damage: Vec<DamageRect>
  empty → skip present (preserves None-skip behaviour)
                                  |
PresentationEngine::present_with_damage(…, &[DamageRect])
                                  |
SurfaceEntry::copy_into_shm_buffer(…, &[DamageRect])
  → single copy_damage union (shm copy region stays unioned for correctness)
                                  |
SurfaceEntry::attach_shm_buffer(…, &[DamageRect])
  if rects.len() > 16:
    wl_surface.damage_buffer(union.x, union.y, union.w, union.h)  ← 1 call
  else:
    for rect in rects:
      wl_surface.damage_buffer(rect.x, rect.y, rect.w, rect.h)    ← N calls
  wl_surface.commit()
```

### SHM pending_damage accumulation

`SurfaceEntry::copy_into_shm_buffer` today accumulates `pending_damage: Option<DamageRect>` (a growing union) for each SHM slot to handle missed frames. After the change:

- Keep `pending_damage: Option<DamageRect>` as the accumulated "must-recopy" union for each SHM slot. The buffer copy region must cover all damage since the slot was last written — a union is correct here because the copy region needs to cover all bytes that may differ.
- The per-rect list is passed through to `attach_shm_buffer` from the caller (render path) only for the `damage_buffer` protocol calls.
- This means `copy_into_shm_buffer` should accept `&[DamageRect]` for the protocol side but continue to compute `copy_damage` as a union (the existing `union_damage` logic) for SHM copy correctness.

### Anti-Patterns to Avoid

- **Truncating to first 16 rects:** Leaving stale pixels in rects 17+ is a compositor correctness bug. Always fall back to the full-surface union when count exceeds cap.
- **Passing rects to SHM copy logic:** The SHM copy region must be a union bounding all pending damage for the slot. Do not pass individual rects to `copy_bgra_damage_to_canvas`.
- **Forgetting dev-window path:** `DevWindow::present()` ignores damage — its signature does not change, but `PresentationEngine::present_with_damage` must not forward the vec to it.
- **Silent skip on empty vec:** The render loop must gate `present_with_damage` on `!present_damage.is_empty()` (empty = skip), exactly mirroring the current `is_some()` gate.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead |
|---------|-------------|-------------|
| Rect union for SHM copy | Custom accumulation | Existing `union_damage()` helper in `backend.rs` |
| Rect merging/clipping | New merge logic | Existing `push_damage_rect`, `bounding_damage_rect`, `clip_damage` in `shell_component.rs` |
| Damage list accumulation | New data structure | `Vec<DamageRect>` with existing helpers; no new scratch buffer needed at present layer |

---

## Common Pitfalls

### Pitfall 1: SHM buffer pending_damage must stay as a union
**What goes wrong:** If `pending_damage` in `SurfaceShmBuffer` is changed to `Vec<DamageRect>`, stale SHM slots accumulate many rects and the copy region becomes complex.
**Why it happens:** A SHM slot may be written to on frame N but not presented until frame N+3 (frame callbacks, pool rotation). The copy region must cover all changes since the slot was last written. A union is correct for copy; per-rects are correct for the `damage_buffer` calls.
**How to avoid:** Keep `pending_damage: Option<DamageRect>` on `SurfaceShmBuffer` for the copy path. Separate the copy region from the damage-buffer region at the `attach_shm_buffer` call site.
**Warning signs:** Stale pixels appearing on surfaces after multi-frame gaps.

### Pitfall 2: The 16-cap fallback must be a full-surface union, not truncation
**What goes wrong:** Capping at 16 by taking `rects[..16]` leaves compositor unnotified about dirty pixels in the dropped rects — compositor may not redraw those regions.
**Why it happens:** `wl_surface::damage_buffer` is cumulative per commit; only told-about rects are guaranteed to be scanned by the compositor.
**How to avoid:** When `rects.len() > 16`, call `damage_buffer` once with the bounding union of all rects.
**Warning signs:** Partial surface staleness when many widgets change simultaneously.

### Pitfall 3: The empty-vec skip-present semantics must be preserved
**What goes wrong:** Changing the skip gate from `present_damage.is_none()` to `present_damage.is_empty()` is correct, but any call path that passes an empty vec to `present_with_damage` bypasses the skip and triggers a commit with no damage — wasted round-trips and stale compositor state.
**Why it happens:** Test helpers and force-full-present paths in `render.rs` that construct `present_damage` manually may forget the guard.
**How to avoid:** The guard in `render.rs` is the canonical skip point. All construction paths (force_full_present, debug_overlay paint) must produce non-empty vecs.
**Warning signs:** Unexpected `wl_surface::commit` calls during idle frames.

### Pitfall 4: Tests call `take_present_damage().is_some()` — must update to `!is_empty()`
**What goes wrong:** After the signature change, `take_present_damage()` returns `Vec<DamageRect>`. Existing test assertions using `.is_some()` will fail to compile.
**Why it happens:** Three test sites use the old `Option` return:
- `shell/component/tests/integration/real_surfaces.rs:117`
- `shell/component/tests/integration/real_surfaces.rs:148`
- `shell/component/tests/invalidation/profiling.rs:94`
- `shell/component/tests/interaction/policy.rs:97`
**How to avoid:** Update each to `!component.take_present_damage().is_empty()`.

### Pitfall 5: `policy.rs` calls `take_present_damage()` as a drain, not a check
**What goes wrong:** `shell/component/tests/interaction/policy.rs:75` calls `component.take_present_damage()` as a discard (no assertion). This compiles fine but the discard call still needs the type to be consumed; if forgotten, the `Vec` return is a no-op rather than an error.
**Why it happens:** The old `Option::take` idiom was idiomatic for draining. `Vec` drain still works — just call it without binding.
**How to avoid:** No change needed — the call still works as a drain. Just make sure it's not accidentally checked with `.is_some()` elsewhere.

---

## Code Examples

### Current: last_present_damage storage in `FrontendSurfaceComponent`

```rust
// crates/core/shell/src/shell/component.rs line ~390
last_present_damage: Option<DamageRect>,
```

### Target: per-region storage

```rust
last_present_damage_rects: Vec<DamageRect>,
```

Initialization in `FrontendSurfaceComponent::new()` (line ~507): `last_present_damage_rects: Vec::new()`.

### Current: paint accumulation (shell_component.rs line 643)

```rust
self.last_present_damage =
    merge_optional_damage(self.last_present_damage, paint_damage, surface_damage);
```

### Target: paint accumulation — extend the rects vec

```rust
// When effective_damage.rects is non-empty and full_surface is false,
// push each rect into last_present_damage_rects via push_damage_rect.
// When full_surface, replace with a single surface_damage rect.
if effective_damage.full_surface {
    self.last_present_damage_rects.clear();
    self.last_present_damage_rects.push(surface_damage);
} else {
    for &rect in &effective_damage.rects {
        push_damage_rect(&mut self.last_present_damage_rects, rect, surface_damage);
    }
}
// If effective_damage.rects is empty (no paint), leave last_present_damage_rects unchanged.
// (An empty rects vec here means paint was skipped — no new damage to add.)
```

Note: The existing `merge_optional_damage` was designed to accumulate across re-render passes when `wants_immediate_rerender` causes paint to run twice before present. The Vec variant must do the same — do not clear before extending if this is an accumulation-across-immediate-rerender scenario. Review `force_full_present` path in `render.rs` lines 246-251 which overrides `present_damage` after `take_present_damage` — that path must construct a `Vec` with the full buffer DamageRect.

### Current: take_present_damage (shell_component.rs line 897)

```rust
fn take_present_damage(&mut self) -> Option<DamageRect> {
    self.last_present_damage.take()
}
```

### Target

```rust
fn take_present_damage(&mut self) -> Vec<DamageRect> {
    std::mem::take(&mut self.last_present_damage_rects)
}
```

Trait default in `ShellComponent` (host/src/lib.rs line 281) changes to return `Vec::new()`.

### Current: render dispatch (render.rs line 245)

```rust
let mut present_damage = self.components[index].component.take_present_damage();
// ...
if !visible || present_damage.is_some() {
    self.presentation_engine.present_with_damage(…, present_damage)
```

### Target

```rust
let mut present_damage: Vec<DamageRect> = self.components[index].component.take_present_damage();
if visible && self.components[index].force_full_present {
    if let Some(buffer) = self.components[index].paint_buffer.as_ref() {
        present_damage = vec![full_buffer_damage(buffer)];
    }
    self.components[index].force_full_present = false;
}
if visible && self.debug.show_layout_bounds {
    // ... paint layout bounds ...
    present_damage = vec![full_buffer_damage(buffer)];
}
// Empty vec = skip present
if !visible || !present_damage.is_empty() {
    self.presentation_engine.present_with_damage(…, &present_damage)
```

### Current: attach_shm_buffer (backend.rs line 250)

```rust
fn attach_shm_buffer(&mut self, qh, index, width, height, damage: DamageRect) {
    let wl_surface = self.layer_surface.wl_surface();
    wl_surface.damage_buffer(damage.x as i32, damage.y as i32,
                             damage.width as i32, damage.height as i32);
    // ...
}
```

### Target

```rust
fn attach_shm_buffer(&mut self, qh, index, width, height, damage_rects: &[DamageRect]) {
    let wl_surface = self.layer_surface.wl_surface();
    const MAX_PROTOCOL_DAMAGE_RECTS: usize = 16;
    if damage_rects.len() > MAX_PROTOCOL_DAMAGE_RECTS {
        // Fall back to bounding union to avoid unbounded protocol overhead
        let union = damage_rects.iter().copied().reduce(union_damage)
            .unwrap_or_else(|| full_damage(width, height));
        wl_surface.damage_buffer(union.x as i32, union.y as i32,
                                 union.width as i32, union.height as i32);
    } else {
        for rect in damage_rects {
            wl_surface.damage_buffer(rect.x as i32, rect.y as i32,
                                     rect.width as i32, rect.height as i32);
        }
    }
    // ...
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single unioned `Option<DamageRect>` to compositor | `Vec<DamageRect>` per commit, capped at 16 | Phase 101 | Compositor can composit partial redraws without scanning unchanged pixels |
| `MAX_DAMAGE_RECTS = 4` inside render path | Still 4 in render path (unchanged) | — | Render path and protocol cap are separate: render merges to ≤4 before present; present loop fans out all ≤4 via damage_buffer |

**Note on render cap vs. protocol cap:** `MAX_DAMAGE_RECTS = 4` in `shell/component.rs` bounds how many rects the paint system tracks. The 16-rect cap in `attach_shm_buffer` is a separate protocol-overhead guard for the Wayland side. In practice paint rarely exceeds 4 rects, so the fallback to bounding union at 16 will almost never trigger — but it is required by DMGE-02 for correctness.

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `wl_surface::damage_buffer` accepts multiple calls per commit and unions them compositor-side | Architecture Patterns | LOW — this is Wayland protocol spec behaviour [ASSUMED] |

All other claims verified directly from source code in this session.

---

## Open Questions

1. **Accumulation across immediate-rerender passes**
   - What we know: `wants_immediate_rerender()` can cause `paint()` to run twice before `take_present_damage()` is called. The current `merge_optional_damage` accumulates both paint passes into one rect.
   - What's unclear: Whether the new `Vec` variant should push without clearing on the second pass (accumulate) or replace (second pass wins).
   - Recommendation: Accumulate — call `push_damage_rect` into `last_present_damage_rects` without clearing at the start of paint. This matches existing `merge_optional_damage` semantics. The `effective_damage.rects.is_empty()` early-exit in `paint()` (line 540) guards the no-change case.

---

## Environment Availability

Step 2.6: SKIPPED — no external tool dependencies. Pure Rust refactor within existing crates.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in (`#[test]`, `cargo test`) |
| Config file | `Cargo.toml` per-crate |
| Quick run command | `cargo test -p mesh-core-shell -- component::tests` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DMGE-01 | `take_present_damage` returns `Vec<DamageRect>` non-empty after paint with 1 changed widget | unit | `cargo test -p mesh-core-shell -- component::tests::invalidation` | ✅ existing test updated |
| DMGE-01 | `take_present_damage` returns empty vec when no pixels changed | unit | `cargo test -p mesh-core-shell -- component::tests` | ✅ existing guards updated |
| DMGE-02 | Single-widget change produces exactly 1 damage rect in the vec | unit | `cargo test -p mesh-core-shell -- component::tests::invalidation::profiling` | ✅ existing profiling test updated |
| DMGE-02 | `attach_shm_buffer` loops `damage_buffer` once per rect up to 16 | unit | `cargo test -p mesh-core-presentation` | ❌ Wave 0 — new test needed |
| DMGE-02 | >16 rects falls back to single bounding union | unit | `cargo test -p mesh-core-presentation` | ❌ Wave 0 |
| DMGE-03 | Debug overlay `damage_rect_count` matches vec length sent to present | unit | `cargo test -p mesh-core-shell -- shell::tests` | ✅ existing shell tests — verify field |

### Sampling Rate

- **Per task commit:** `cargo test -p mesh-core-shell -- component::tests`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] `crates/core/presentation/src/wayland_surface/backend_tests.rs` — unit test for `attach_shm_buffer` calling `damage_buffer` N times (DMGE-02). Must be a mock or stub-level test since real Wayland is unavailable in CI.
- [ ] `crates/core/presentation/src/wayland_surface/backend_tests.rs` — unit test for >16-rect fallback to bounding union.

*(Existing test infrastructure in `mesh-core-shell` covers most of the phase — Wave 0 only needs new presentation-layer tests.)*

---

## Security Domain

No security-relevant surface area. This phase touches only internal damage-tracking types and Wayland protocol message construction. No user input, no authentication, no cryptography, no capability changes.

---

## Sources

### Primary (HIGH confidence — verified from source)

- `crates/core/shell/src/shell/component.rs` — `EffectiveDamage`, `MAX_DAMAGE_RECTS`, `retained_paint_snapshot`, `push_damage_rect`
- `crates/core/shell/src/shell/component/shell_component.rs` — `paint()`, `take_present_damage()`, `merge_optional_damage`, `select_effective_damage_rects`
- `crates/core/frontend/host/src/lib.rs` — `ShellComponent` trait, `take_present_damage` default
- `crates/core/shell/src/shell/runtime/render.rs` — dispatch loop, `force_full_present`, skip gate
- `crates/core/presentation/src/lib.rs` — `PresentationEngine::present_with_damage`
- `crates/core/presentation/src/wayland_surface/backend.rs` — `SurfaceEntry`, `copy_into_shm_buffer`, `attach_shm_buffer`, `pending_damage`, `union_damage`
- `crates/core/foundation/debug/src/lib.rs` — `RetainedPaintSnapshot.damage_rect_count`, `damage_area`
- `crates/core/shell/src/shell/runtime/debug.rs` — serialisation of `damage_rect_count` to debug bus

### Secondary (MEDIUM confidence)

- Wayland protocol semantics for `wl_surface::damage_buffer` (multiple calls per commit are cumulative) — [ASSUMED] from Wayland protocol spec knowledge; not verified against wayland-rs docs in this session.

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new packages; all work within verified crates
- Architecture: HIGH — complete source read of all six integration points
- Pitfalls: HIGH — derived from actual code paths observed, not speculation
- Wayland damage_buffer multi-call semantics: MEDIUM — ASSUMED from spec, not verified via tool

**Research date:** 2026-06-10
**Valid until:** Stable — pure internal refactor, no external dependency drift
