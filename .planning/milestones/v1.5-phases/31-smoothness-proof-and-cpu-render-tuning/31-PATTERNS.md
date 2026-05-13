# Phase 31: Smoothness Proof and CPU Render Tuning - Pattern Map

**Mapped:** 2026-05-12
**Files analyzed:** 11
**Analogs found:** 11 / 11

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/core/shell/src/shell/component/shell_component.rs` | controller/orchestrator | request-response + transform | `crates/core/shell/src/shell/component/shell_component.rs` | exact |
| `crates/core/frontend/render/src/display_list.rs` | service/model | transform | `crates/core/frontend/render/src/display_list.rs` | exact |
| `crates/core/frontend/render/src/surface/icon.rs` | service/utility | file-I/O + transform | `crates/core/frontend/render/src/surface/icon.rs` | exact |
| `crates/core/frontend/render/src/surface/text.rs` | service/utility | transform | `crates/core/frontend/render/src/surface/text.rs` | exact |
| `crates/core/frontend/render/src/surface/profiling.rs` | utility | event-driven counters | `crates/core/frontend/render/src/surface/profiling.rs` | exact |
| `crates/core/frontend/render/src/surface/mod.rs` | provider/bridge | transform + event-driven counters | `crates/core/frontend/render/src/surface/mod.rs` | exact |
| `crates/core/shell/src/shell/runtime/debug.rs` | provider/serialization | request-response | `crates/core/shell/src/shell/runtime/debug.rs` | exact |
| `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs` | test | request-response proof | `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs` | exact |
| `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-01-BENCHMARK.md` | documentation | batch evidence | `.planning/phases/30-raster-cache-hardening-for-icons-images-and-text/30-01-BENCHMARK.md` | role-match |
| `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-UAT.md` | documentation | manual UAT | `.planning/phases/29-damage-indexed-paint-execution-and-repaint-policy/29-UAT.md` | role-match |
| `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-VERIFICATION.md` | documentation | batch verification | `.planning/phases/30-raster-cache-hardening-for-icons-images-and-text/30-VERIFICATION.md` | role-match |

## Pattern Assignments

### `crates/core/shell/src/shell/component/shell_component.rs` (controller/orchestrator, request-response + transform)

**Analog:** `crates/core/shell/src/shell/component/shell_component.rs`

**Repaint-policy selection pattern** (lines 722-793):
```rust
fn select_effective_damage(
    metrics: DisplayListMetrics,
    surface: DamageRect,
    requires_tree_rebuild: bool,
    reorder_damage: Option<DamageRect>,
    tooltip_damage: Option<DamageRect>,
) -> EffectiveDamage {
    if metrics.full_surface_damage {
        return EffectiveDamage {
            rect: Some(surface),
            full_surface: true,
            policy: DisplayListRepaintPolicy::FullSurface,
        };
    }
    // merge base damage with reorder/tooltip before policy choice
    let policy = select_damage_policy(
        metrics,
        requires_tree_rebuild,
        reorder_damage.is_some() || tooltip_damage.is_some(),
        damage.area(),
    );
    // only FullSurface promotes to whole-surface clear/paint
}
```

**Threshold pattern to tune conservatively** (lines 769-792):
```rust
let changed_entries = metrics
    .entries_rebuilt
    .saturating_add(metrics.entries_removed);
let mostly_changed_entries =
    metrics.entries_total > 0 && changed_entries * 4 >= metrics.entries_total * 3;
let large_damage = metrics.surface_area > 0 && candidate_area * 2 >= metrics.surface_area;

if large_damage || (requires_tree_rebuild && mostly_changed_entries) {
    DisplayListRepaintPolicy::FullSurface
} else if has_extra_damage_sources {
    DisplayListRepaintPolicy::BoundingRect
} else {
    DisplayListRepaintPolicy::MinimalDamage
}
```

**Clear/background and profiling payload pattern** (lines 378-485):
```rust
let selected_paint = self
    .retained_display_list
    .select_paint_commands(paint_damage, effective_damage.policy);
self.invalidation_snapshot = Some(mesh_core_debug::ProfilingInvalidationSnapshot {
    paint: retained_paint_snapshot(selected_paint.metrics(), effective_damage),
    text: mesh_core_debug::TextCacheSnapshot::default(),
    ..snapshot_fields
});

if effective_damage.full_surface {
    buffer.clear(mesh_core_elements::style::Color::TRANSPARENT);
} else {
    buffer.clear_rect(damage.x, damage.y, damage.width, damage.height, Color::TRANSPARENT);
}
let paint_metrics = paint_display_list_for_module_with_profiling_metrics(...);
snapshot.text = text_cache_snapshot(paint_metrics.text);
snapshot.paint.raster_cache_hits = paint_metrics.raster_cache_hits;
```

**Implementation guidance:** Tune only `select_damage_policy` thresholds or the small caller inputs around `reorder_damage`/`tooltip_damage`. Keep the existing flow: compute effective damage, select retained commands, clear full buffer or damage rect, paint with the selected clip, then publish text/raster counters into the invalidation snapshot.

### `crates/core/frontend/render/src/display_list.rs` (service/model, transform)

**Analog:** `crates/core/frontend/render/src/display_list.rs`

**Metrics contract pattern** (lines 81-128):
```rust
pub struct DisplayListMetrics {
    pub damage_area: u64,
    pub surface_area: u64,
    pub full_surface_damage: bool,
    pub skipped_paint_pixels: u64,
    pub omitted_subtrees: u64,
    pub preclipped_descendants: u64,
    pub repaint_policy: DisplayListRepaintPolicy,
    pub filtered_span_count: u64,
    pub filtered_command_count: u64,
    pub filtered_commands_skipped: u64,
    pub filtered_fallback_count: u64,
    pub batch_count: u64,
    pub barriers: DisplayBatchBarrierCounts,
}
```

**Retained display-list filtering pattern** (lines 657-717):
```rust
let Some(damage) = damage else {
    metrics.repaint_policy = DisplayListRepaintPolicy::MinimalDamage;
    metrics.filtered_commands_skipped = full_commands;
    return SelectedDisplayListPaint { commands: Vec::new(), metrics };
};

if matches!(policy, DisplayListRepaintPolicy::FullSurface) {
    metrics.filtered_command_count = full_commands;
    metrics.filtered_fallback_count = u64::from(!self.paint_commands.is_empty());
    return SelectedDisplayListPaint { commands: self.paint_commands.clone(), metrics };
}

selected_indices.sort_unstable();
selected_indices.dedup();
let commands: Vec<_> = selected_indices
    .iter()
    .filter_map(|index| self.paint_commands.get(*index).cloned())
    .collect();
```

**Guardrail tests to copy/extend** (lines 2161-2259):
```rust
assert!(selected.metrics().filtered_command_count < full_order.len() as u64);
assert!(selected.metrics().filtered_commands_skipped > 0);
assert!(selected.metrics().filtered_span_count > 0);
assert_eq!(filtered_order, projected_full);

assert!(
    ids.contains(&1),
    "partial repaint must replay root background under damaged child pixels"
);

assert_eq!(selected.commands().len(), list.paint_commands().len());
assert_eq!(selected.metrics().filtered_fallback_count, 1);
```

**Opacity/cache conservatism test pattern** (lines 2345-2437):
```rust
assert_eq!(
    crate::surface::icon::cached_file_resource_opacity(&opaque_path, 10, 10, tint, false),
    crate::surface::icon::CachedResourceOpacity::Opaque
);
assert_eq!(
    crate::surface::icon::cached_file_resource_opacity(&translucent_path, 10, 10, tint, false),
    crate::surface::icon::CachedResourceOpacity::Translucent
);
assert_eq!(metrics.barriers.icon, 1);
assert_eq!(metrics.barriers.translucency, 1);
assert_eq!(transparent_metrics.barriers.opacity, 1);
```

**Implementation guidance:** If culling or batching heuristics are touched, preserve ordered survivors, root/background replay, scrollbar inclusion through spans, full-surface fallback accounting, and conservative opacity/translucency barriers.

### `crates/core/frontend/render/src/surface/icon.rs` (service/utility, file-I/O + transform)

**Analog:** `crates/core/frontend/render/src/surface/icon.rs`

**Cache key and bounded capacity pattern** (lines 12-80):
```rust
static IMAGE_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedImage>>> = OnceLock::new();
static RASTER_CACHE: OnceLock<Mutex<RasterVariantCache>> = OnceLock::new();
const RASTER_CACHE_CAPACITY: usize = 256;

struct RasterCacheKey {
    source_kind: RasterSourceKind,
    source_identity: PathBuf,
    width: u32,
    height: u32,
    tint: u32,
    multicolor: bool,
    freshness: Option<FileFreshness>,
}

while self.entries.len() > RASTER_CACHE_CAPACITY {
    let Some(evicted) = self.order.pop_front() else { break; };
    self.entries.remove(&evicted);
}
```

**Freshness, cacheability, and opacity pattern** (lines 142-255):
```rust
fn raster_file_key(...) -> Option<RasterCacheKey> {
    Some(RasterCacheKey {
        source_identity: source_identity(path),
        width,
        height,
        tint: encode_tint(tint),
        multicolor,
        freshness: Some(file_freshness(path)?),
        ..key_fields
    })
}

fn svg_file_is_cacheable(path: &Path) -> bool {
    let Ok(svg_data) = std::fs::read_to_string(path) else { return false; };
    !svg_has_external_resource_reference(&svg_data)
}

let Some(variant) = cached_variant(&key) else {
    return CachedResourceOpacity::Unknown;
};
```

**Hit/miss/bypass recording pattern** (lines 397-465):
```rust
if let Some(key) = key.as_ref()
    && let Some(variant) = cached_variant(key)
{
    profiling::record_raster_cache_hit(variant.fully_opaque);
    blit_variant(buffer, &variant, dest_x, dest_y);
    return;
}

if key.is_some() {
    profiling::record_raster_cache_miss();
} else {
    profiling::record_raster_cache_bypass();
}
profiling::record_icon_image_raster(raster_started.elapsed());
```

**Guardrail tests to copy/extend** (lines 719-930, 934-946):
```rust
assert_eq!(metrics.raster_cache_hits, 0);
assert_eq!(metrics.raster_cache_misses, 0);
assert_eq!(metrics.raster_cache_bypasses, 2);

assert_eq!(second_metrics.raster_cache_hits, 1);
assert_eq!(second_metrics.raster_cache_misses, 0);
assert_eq!(second_metrics.icon_image_raster_micros, 0);

assert_eq!(metrics.raster_cache_hits, 0);
assert_eq!(metrics.raster_cache_misses, 1);
```

**Implementation guidance:** Tune capacity only with five-scenario evidence showing repeat misses after warmup. Do not weaken freshness, external SVG bypass, tint/multicolor key separation, missing-icon cache reuse, or opaque/translucent hit reporting.

### `crates/core/frontend/render/src/surface/text.rs` (service/utility, transform)

**Analog:** `crates/core/frontend/render/src/surface/text.rs`

**Text metrics and bounded layout cache pattern** (lines 16-43):
```rust
const TEXT_LAYOUT_CACHE_CAPACITY: usize = 128;

pub struct TextCacheMetrics {
    pub layout_hits: u64,
    pub layout_misses: u64,
    pub layout_invalidations: u64,
    pub shaped_entries: u64,
    pub glyph_cache_active: bool,
    pub shaping_micros: u64,
}
```

**Layout hit/miss and eviction pattern** (lines 378-426):
```rust
if let Some(cosmic) = self.layout_cache.remove(key) {
    self.metrics.layout_hits = self.metrics.layout_hits.saturating_add(1);
    return cosmic;
}

self.metrics.layout_misses = self.metrics.layout_misses.saturating_add(1);
let shaping_started = std::time::Instant::now();
// shape with cosmic_text
self.metrics.shaping_micros = self.metrics.shaping_micros.saturating_add(...);

if self.layout_cache.len() >= TEXT_LAYOUT_CACHE_CAPACITY
    && !self.layout_cache.contains_key(&key)
    && let Some(evicted) = self.layout_cache.keys().next().cloned()
{
    self.layout_cache.remove(&evicted);
    self.metrics.layout_invalidations = self.metrics.layout_invalidations.saturating_add(1);
}
```

**Guardrail tests to copy/extend** (lines 614-723):
```rust
assert_eq!(metrics.layout_misses, 1);
assert_eq!(metrics.layout_hits, 1);
assert_eq!(metrics.shaped_entries, 1);
assert!(metrics.glyph_cache_active);

assert_eq!(metrics.layout_misses, 7);
assert_eq!(metrics.layout_hits, 1);
assert_eq!(metrics.shaped_entries, 7);
```

**Implementation guidance:** Any cache tuning must keep text, family, size, weight, line height, width, and alignment as layout-affecting inputs. Do not collapse render and selection keys unless tests prove selection geometry and rendered pixels remain stable.

### `crates/core/frontend/render/src/surface/profiling.rs` and `surface/mod.rs` (utility/provider, event-driven counters)

**Analogs:** `crates/core/frontend/render/src/surface/profiling.rs`, `crates/core/frontend/render/src/surface/mod.rs`

**Raster counter pattern** (profiling.rs lines 4-55):
```rust
pub struct RasterMetrics {
    pub icon_image_raster_micros: u64,
    pub raster_cache_hits: u64,
    pub raster_cache_misses: u64,
    pub raster_cache_bypasses: u64,
    pub raster_cache_opaque_hits: u64,
    pub raster_cache_translucent_hits: u64,
}

pub fn record_raster_cache_hit(opaque: bool) {
    metrics.raster_cache_hits = metrics.raster_cache_hits.saturating_add(1);
    if opaque { metrics.raster_cache_opaque_hits += 1; } else { ... }
}
```

**Paint profiling bridge pattern** (mod.rs lines 24-33, 158-198):
```rust
pub struct PaintProfilingMetrics {
    pub text: TextCacheMetrics,
    pub traversal_micros: u64,
    pub icon_image_raster_micros: u64,
    pub raster_cache_hits: u64,
    pub raster_cache_misses: u64,
    pub raster_cache_bypasses: u64,
    pub raster_cache_opaque_hits: u64,
    pub raster_cache_translucent_hits: u64,
}

profiling::reset_raster_metrics();
engine.reset_text_cache_metrics();
let traversal_started = std::time::Instant::now();
engine.render_display_list_for_module(...);
let raster = profiling::raster_metrics();
PaintProfilingMetrics { text: engine.text_cache_metrics(), ... }
```

**Implementation guidance:** If Phase 31 needs new evidence fields, add them through this existing metrics bridge and then serialize through the existing debug invalidation payload. Prefer existing counters for benchmark acceptance before adding fields.

### `crates/core/shell/src/shell/runtime/debug.rs` (provider/serialization, request-response)

**Analog:** `crates/core/shell/src/shell/runtime/debug.rs`

**Canonical scenario list pattern** (lines 154-184):
```rust
let scenarios = [
    BenchmarkScenarioId::Hover,
    BenchmarkScenarioId::SurfaceOpenClose,
    BenchmarkScenarioId::PointerUpdate,
    BenchmarkScenarioId::KeyboardTraversal,
    BenchmarkScenarioId::BackendUpdate,
]
.into_iter()
.map(|id| benchmark_scenario_snapshot(...))
.collect();
```

**Profiling payload serialization pattern** (lines 717-795):
```rust
"paint": {
    "damage_area": snapshot.paint.damage_area,
    "surface_area": snapshot.paint.surface_area,
    "skipped_paint_pixels": snapshot.paint.skipped_paint_pixels,
    "omitted_subtrees": snapshot.paint.omitted_subtrees,
    "preclipped_descendants": snapshot.paint.preclipped_descendants,
    "repaint_policy": snapshot.paint.repaint_policy.as_str(),
    "filtered_command_count": snapshot.paint.filtered_command_count,
    "filtered_commands_skipped": snapshot.paint.filtered_commands_skipped,
    "raster_cache_hits": snapshot.paint.raster_cache_hits,
    "raster_cache_misses": snapshot.paint.raster_cache_misses,
},
"text": {
    "layout_hits": snapshot.text.layout_hits,
    "layout_misses": snapshot.text.layout_misses,
    "shaping_micros": snapshot.text.shaping_micros,
}
```

**Implementation guidance:** Keep payload names stable. If a new Phase 31 field is unavoidable, add it beside related `paint` or `text` fields and cover it with the existing profiling/debug tests rather than creating a second diagnostics surface.

### `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs` (test, request-response proof)

**Analog:** `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs`

**Scenario setup pattern** (lines 44-204):
```rust
let mut hover_component =
    real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
hover_component.set_profiling_enabled(true);
hover_component.paint(&theme, 960, 80, &mut hover_buffer).unwrap();
hover_component.take_profiling_records();
hover_component.take_invalidation_snapshot();
hover_component.handle_input(&theme, 960, 80, ComponentInput::PointerMove { x, y }).unwrap();
hover_component.paint(&theme, 960, 80, &mut hover_buffer).unwrap();
let hover_records = hover_component.take_profiling_records();
let hover_invalidation = hover_component.take_invalidation_snapshot().unwrap();
```

**Benchmark log pattern** (lines 206-306):
```rust
eprintln!(
    "PHASE26_BASELINE hover style_restyle={}us paint={}us traversal={}us text_hits={} text_misses={} shaping={}us raster_hits={} raster_misses={} raster_bypasses={} retained={} full_rebuild={}",
    stage_max_micros(&hover_records, mesh_core_debug::ProfilingStage::StyleRestyle),
    stage_max_micros(&hover_records, mesh_core_debug::ProfilingStage::Paint),
    stage_max_micros(&hover_records, mesh_core_debug::ProfilingStage::PaintTraversal),
    hover_invalidation.text.layout_hits,
    hover_invalidation.text.layout_misses,
    hover_invalidation.text.shaping_micros,
    hover_invalidation.paint.raster_cache_hits,
    hover_invalidation.paint.raster_cache_misses,
    hover_invalidation.paint.raster_cache_bypasses,
    hover_invalidation.retained_path,
    hover_invalidation.full_rebuild
);
```

**Proof assertions pattern** (lines 1-41, 331-350):
```rust
assert!(
    snapshot.text.layout_hits + snapshot.text.layout_misses > 0,
    "{label} should report text layout cache activity"
);
assert!(
    snapshot.paint.raster_cache_hits
        + snapshot.paint.raster_cache_misses
        + snapshot.paint.raster_cache_bypasses
        > 0,
    "{label} should report icon/image raster cache activity"
);
assert_raster_cache_reuse("hover", &hover_invalidation);
assert_raster_cache_reuse("pointer_update", &pointer_update_invalidation);
assert_raster_cache_reuse("keyboard_traversal", &keyboard_invalidation);
assert_raster_cache_reuse("backend_update", &backend_update_invalidation);
```

**Implementation guidance:** Reuse the same test and five scenario IDs. If adding Phase 31-specific eprintln labels, keep the same row shape plus repaint policy/filtering fields needed for before/after comparison.

### `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-01-BENCHMARK.md` (documentation, batch evidence)

**Analog:** `.planning/phases/30-raster-cache-hardening-for-icons-images-and-text/30-01-BENCHMARK.md`

**Artifact structure pattern** (lines 1-20, 47-66):
```markdown
---
phase: 30
plan: 01
title: Raster cache hardening benchmark proof
created: 2026-05-12
status: complete
canonical_scenarios:
  - hover
  - surface_open_close
  - pointer_update
  - keyboard_traversal
  - backend_update
---

## Canonical Scenario Evidence

Captured with:
`env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture`
```

**Phase 31 benchmark table should include:** scenario, Phase 26 baseline paint/traversal, Phase 30 cache evidence, Phase 31 after paint/traversal, repaint policy, filtered/skipped commands, text hits/misses/shaping, raster hits/misses/bypasses, UAT status, and interpretation. Do not accept counter-only wins.

### `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-UAT.md` (documentation, manual UAT)

**Analog:** `.planning/phases/29-damage-indexed-paint-execution-and-repaint-policy/29-UAT.md`

**Manual UAT structure pattern** (lines 1-42):
```markdown
---
status: complete
phase: 29-damage-indexed-paint-execution-and-repaint-policy
source:
  - .planning/phases/29-damage-indexed-paint-execution-and-repaint-policy/29-01-SUMMARY.md
started: "2026-05-11T21:45:52+02:00"
updated: "2026-05-12T13:36:20+02:00"
---

## Tests

### 1. Debug Paint Policy Payload
expected: ...
result: issue
reported: ...
severity: major

## Summary
total: 4
passed: 2
issues: 1
pending: 0
skipped: 1
blocked: 0
```

**Phase 31 UAT rows should cover exactly:** `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`. For each row record expected smoothness, result, reported notes, correctness check, and severity if any regression appears.

### `.planning/phases/31-smoothness-proof-and-cpu-render-tuning/31-VERIFICATION.md` (documentation, batch verification)

**Analog:** `.planning/phases/30-raster-cache-hardening-for-icons-images-and-text/30-VERIFICATION.md`

**Verification structure pattern** (lines 1-34):
```markdown
---
phase: 30
title: Raster cache hardening verification
status: passed
verified: 2026-05-12
requirements: ["CACHE-01", "CACHE-02", "CACHE-03"]
---

## Commands

- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list`
- PASS: `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling`
- PASS: benchmark artifact contains `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`.

## Residual Risk

- Capacity tuning, threshold selection, and visible smoothness acceptance are deliberately deferred to Phase 31.
```

**Phase 31 verification must include:** fmt, focused render tests for touched modules, shell profiling proof, canonical benchmark capture, UAT completion, and explicit SMTH-03 statement that no GPU backend, parallel paint/layout, second benchmark harness, trace persistence, or broad UI redesign was added.

## Shared Patterns

### Repaint Policy Tuning

**Source:** `crates/core/shell/src/shell/component/shell_component.rs` lines 769-792
**Apply to:** `shell_component.rs`, benchmark proof, UAT interpretation

Tune measured thresholds in place. Use `DisplayListMetrics` fields already emitted by the render crate. Keep `has_extra_damage_sources` conservative as `BoundingRect`; only promote to `FullSurface` when measured area or changed-entry ratio justifies it.

### Retained Display-List Filtering

**Source:** `crates/core/frontend/render/src/display_list.rs` lines 657-717 and 2161-2259
**Apply to:** display-list tuning and tests

Preserve sort/dedup ordered survivors, root/background replay for partial damage, scrollbar span inclusion, and explicit full-surface fallback metrics.

### Profiling Payload Evidence

**Source:** `crates/core/shell/src/shell/component/shell_component.rs` lines 381-388 and 477-485; `crates/core/shell/src/shell/runtime/debug.rs` lines 717-795
**Apply to:** all tuning work and benchmark artifacts

The acceptance payload is `invalidation.paint` plus `invalidation.text`; do not create a second diagnostics path.

### Raster Cache Guardrails

**Source:** `crates/core/frontend/render/src/surface/icon.rs` lines 12-80, 142-255, 397-465, 719-930
**Apply to:** icon/image cache tuning

Preserve bounded capacity, freshness metadata, external-resource bypasses, path identity, tint/multicolor key separation, missing-icon reuse, and opaque/translucent hit classes.

### Text Cache Guardrails

**Source:** `crates/core/frontend/render/src/surface/text.rs` lines 16-43, 378-426, 614-723
**Apply to:** text layout/cache tuning

Keep layout-affecting inputs precise and keep `layout_hits`, `layout_misses`, `layout_invalidations`, `shaped_entries`, `glyph_cache_active`, and `shaping_micros` meaningful.

### Canonical Proof Artifacts

**Source:** `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs` lines 44-350; `30-01-BENCHMARK.md` lines 1-66
**Apply to:** `31-01-BENCHMARK.md`, `31-UAT.md`, `31-VERIFICATION.md`

Use the existing command:

```bash
env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture
```

Record all five canonical scenarios and compare against Phase 26 baseline plus Phase 30 cache proof. Manual UAT is required for final smoothness acceptance.

## No Analog Found

All expected Phase 31 files have local analogs. No new renderer backend, parallel worker, benchmark harness, or diagnostics surface should be planned.

## Metadata

**Analog search scope:** `.planning/phases`, `crates/core/frontend/render/src`, `crates/core/shell/src/shell/component`, `crates/core/shell/src/shell/runtime`
**Files scanned:** 18
**Pattern extraction date:** 2026-05-12
