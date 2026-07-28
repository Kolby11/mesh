# Phase 98: Narrow Invalidation & Event Routing - Pattern Map

**Mapped:** 2026-06-09
**Files analyzed:** 5
**Analogs found:** 5 / 5

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/core/shell/src/shell/component.rs` | state/flags | event-driven | same file (extend bitflags + invalidation methods) | exact |
| `crates/core/shell/src/shell/component/rendering.rs` | engine | event-driven | same file (restyle_retained_tree, collect_interaction_changed_keys) | exact |
| `crates/core/shell/src/shell/component/shell_component.rs` | controller | event-driven | same file (handle_service_event, paint) | exact |
| `crates/core/foundation/debug/src/lib.rs` | model | batch | same file (ProfilingInvalidationSnapshot) | exact |
| `crates/core/shell/src/shell/component/tests/invalidation/basic.rs` | test | — | existing basic.rs tests | exact |
| `crates/core/shell/src/shell/component/tests/invalidation/profiling.rs` | test | — | existing profiling.rs tests | exact |

---

## Pattern Assignments

### `component.rs` — Add `SCRIPT_NARROW` bit and `invalidate_script_state_narrow()`

**Analog:** same file, `ComponentDirtyFlags` bitflags block at lines 67–135, invalidation methods at 487–554

**Existing bitflags block** (lines 67–80):
```rust
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub(super) struct ComponentDirtyFlags: u16 {
        const SCRIPT = 1 << 0;
        const STATE = 1 << 1;
        const STYLE = 1 << 2;
        const LAYOUT = 1 << 3;
        const PAINT = 1 << 4;
        const TEXT = 1 << 5;
        const ACCESSIBILITY = 1 << 6;
        const METRICS = 1 << 7;
        const SURFACE_CONFIG = 1 << 8;
        // ADD: const SCRIPT_NARROW = 1 << 9;
    }
}
```

**`requires_tree_rebuild()` predicate to NOT change** (lines 118–120):
```rust
pub(super) fn requires_tree_rebuild(self) -> bool {
    self.intersects(Self::SCRIPT | Self::TEXT)
    // SCRIPT_NARROW intentionally excluded — it does not trigger TREE_REBUILD
}
```

**Existing `invalidate_script_state()` pattern to copy for the narrow variant** (lines 503–511):
```rust
pub(super) fn invalidate_script_state(&mut self) {
    self.surface_pixels_invalid = true;
    self.invalidate(ComponentDirtyFlags::TREE_REBUILD);
}
// ADD alongside:
pub(super) fn invalidate_script_state_narrow(&mut self) {
    self.surface_pixels_invalid = true;
    self.invalidate(ComponentDirtyFlags::SCRIPT_NARROW);
}
```

**`take_dirty_for_paint()` return shape to extend** (lines 533–554):
```rust
pub(super) fn take_dirty_for_paint(
    &mut self,
) -> (bool, bool, ComponentDirtyFlags, ComponentDirtyFlags) {
    let legacy_dirty = self.dirty && self.dirty_types.is_empty();
    let legacy_style_only = self.style_only_dirty && self.dirty_types.is_empty();
    let flags = self.dirty_types;
    let requires_tree_rebuild = legacy_dirty || flags.requires_tree_rebuild();
    let can_use_retained_path =
        !requires_tree_rebuild && (legacy_style_only || !flags.is_empty());
    self.last_dirty_types = flags;
    self.dirty_types = ComponentDirtyFlags::empty();
    self.dirty = false;
    self.style_only_dirty = false;
    (requires_tree_rebuild, can_use_retained_path, flags, self.last_dirty_types)
}
// Note: SCRIPT_NARROW is not TREE_REBUILD and not "retained style path" —
// it's a third branch. The caller in paint() must check flags.contains(SCRIPT_NARROW).
```

**`cached_service_payloads` field for old-payload access** (line 294):
```rust
cached_service_payloads: HashMap<String, std::sync::Arc<serde_json::Value>>,
```

---

### `rendering.rs` — Add `narrow_script_update()` / tree diff entry point

**Analog:** `restyle_retained_tree()` (lines 156–167) and `finalize_tree()` (lines 169–321) and `collect_interaction_changed_keys()` (lines 329–360)

**`restyle_retained_tree()` structure to parallel** (lines 156–167):
```rust
pub(super) fn restyle_retained_tree(
    &mut self,
    theme: &Theme,
    width: u32,
    height: u32,
    dirty_types: ComponentDirtyFlags,
) -> Option<WidgetNode> {
    let mut tree = self.last_tree.take()?;
    self.active_theme.replace(theme.clone());
    self.finalize_tree(&mut tree, theme, width, height, "restyle", dirty_types);
    Some(tree)
}
// narrow_script_update() follows the same Option<WidgetNode> return shape.
// Instead of taking last_tree, it calls build_tree() to get a fresh tree,
// diffs against last_tree (if Some), and returns the fresh tree with narrow dirty info.
```

**`build_tree()` call site** (lines 109–149) — narrow path still calls this:
```rust
pub(super) fn build_tree(&mut self, theme: &Theme, width: u32, height: u32) -> WidgetNode {
    if self.render_hooks_pending {
        self.call_render_hooks();
        self.render_hooks_pending = false;
    }
    // ...
    let mut tree = self.compiled.build_tree_with_state(...);
    self.record_profiling_stage(ProfilingStage::TreeBuild, tree_build_started, Some("rebuild"));
    self.finalize_tree(&mut tree, theme, width, height, "rebuild", ComponentDirtyFlags::TREE_REBUILD);
    tree
}
// Narrow path: call build_tree() normally, then diff against last_tree snapshot.
```

**`finalize_tree()` trigger_kind pattern** (lines 200–202) — narrow path keeps "rebuild" trigger so the index stays fresh:
```rust
if trigger_kind == "rebuild" {
    self.node_service_field_deps = NodeServiceFieldDependencies::build(tree);
}
// Narrow path passes trigger_kind="rebuild" to ensure node_service_field_deps
// is rebuilt after the build_tree() call. Do not use "narrow" as trigger_kind
// here, or the reverse index will become stale.
```

**`collect_interaction_changed_keys()` pattern for affected-set collection** (lines 329–360):
```rust
fn collect_interaction_changed_keys(&self, tree: &WidgetNode) -> HashSet<String> {
    let mut changed_keys: HashSet<String> = HashSet::new();
    // union of old and new hovered paths, focused keys
    for key in &self.previous_hovered_path { changed_keys.insert(key.clone()); }
    for key in &self.hovered_path { changed_keys.insert(key.clone()); }
    if changed_keys.is_empty() { return changed_keys; }
    // expand to all descendants
    let mut all_affected: HashSet<String> = HashSet::new();
    for changed_key in &changed_keys {
        all_affected.insert(changed_key.clone());
        collect_descendant_keys(tree, changed_key, &mut all_affected);
    }
    all_affected
}
// Tree diff for SCRIPT_NARROW follows the same expansion pattern:
// collect changed leaf NodeIds, then add ancestor chains to dirty set.
```

**`record_profiling_stage` pattern** (lines 4–36):
```rust
pub(super) fn record_profiling_stage(
    &mut self,
    stage: mesh_core_debug::ProfilingStage,
    started_at: std::time::Instant,
    trigger_kind: Option<&str>,
) {
    if !self.profiling_enabled { return; }
    self.profiling_records.push(ComponentProfilingRecord {
        stage,
        duration: started_at.elapsed(),
        module_id: Some(self.compiled.manifest.package.id.clone()),
        trigger_kind: trigger_kind.map(str::to_string),
    });
}
// Use trigger_kind: Some("narrow") for new narrow-path stages.
```

**Threshold guard placement** — before any mutation (pattern from RESEARCH.md Pattern 3):
```rust
// Inside narrow_script_update(), BEFORE any dirty mutation:
let total_nodes = tree.node_count();
let affected_count = changed_leaf_ids.len();
if total_nodes == 0 || affected_count * 2 > total_nodes {
    // >50% affected — fall back. Return None so paint() uses build_tree() full path.
    return None;
}
```

---

### `shell_component.rs` — Field-level filtering in `handle_service_event()`

**Analog:** same file, `handle_service_event()` at lines 119–182 and `paint()` at lines 289–332

**`handle_service_event()` current invalidation call site** (lines 177–180):
```rust
if needs_rebuild {
    self.render_hooks_pending = true;
    self.invalidate_script_state();  // Phase 98: conditionally replace with narrow variant
}
```

**Pattern for old-payload extraction** — save BEFORE the insert at line 134:
```rust
// Phase 98: save previous before overwriting
let previous_payload = self.cached_service_payloads.get(service_name).cloned();
self.cached_service_payloads
    .insert(service_name.clone(), payload.clone().into());
// ...
// Then diff:
let changed_fields = collect_changed_fields(service_name, previous_payload.as_deref(), payload);
let has_intersecting_nodes = changed_fields.iter().any(|(svc, field)| {
    !self.node_service_field_deps.nodes_reading_field(svc, field).is_empty()
});
if needs_rebuild {
    self.render_hooks_pending = true;
    if has_intersecting_nodes {
        self.invalidate_script_state_narrow();  // narrow path eligible
    } else {
        // No template nodes read any changed field — skip invalidation entirely
        // (Lua-side tracked_service_fields check already ran above and set needs_rebuild)
        self.invalidate_script_state();
    }
}
```

**`paint()` path selection to extend** (lines 325–332):
```rust
let mut tree = if use_retained_style_path {
    match self.restyle_retained_tree(theme, content_width, content_height, dirty_types) {
        Some(t) => t,
        None => self.build_tree(theme, content_width, content_height),
    }
} else {
    self.build_tree(theme, content_width, content_height)
};
// Phase 98: add a third branch for SCRIPT_NARROW before the existing two:
// if dirty_types.contains(ComponentDirtyFlags::SCRIPT_NARROW) && !requires_tree_rebuild {
//     match self.narrow_script_update(theme, content_width, content_height) {
//         Some(t) => t,
//         None => self.build_tree(theme, content_width, content_height),
//     }
// } else if use_retained_style_path { ... }
```

---

### `debug/src/lib.rs` — Extend `ProfilingInvalidationSnapshot`

**Analog:** same file, `ProfilingInvalidationSnapshot` struct at lines 173–182

**Current struct** (lines 173–182):
```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfilingInvalidationSnapshot {
    pub full_rebuild: bool,
    pub retained_path: bool,
    pub retained_generation: u64,
    pub component: ComponentInvalidationCounts,
    pub retained: RetainedInvalidationCounts,
    pub paint: RetainedPaintSnapshot,
    pub text: TextCacheSnapshot,
}
// Add two fields:
//     pub narrow_path: bool,
//     pub affected_node_count: u64,
```

**`ComponentInvalidationCounts` struct pattern to add `script_narrow` field** (lines 184–195):
```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ComponentInvalidationCounts {
    pub script: u64,
    pub state: u64,
    pub style: u64,
    pub layout: u64,
    pub paint: u64,
    pub text: u64,
    pub accessibility: u64,
    pub metrics: u64,
    pub surface_config: u64,
    // Add: pub script_narrow: u64,
}
```

The `to_debug_counts()` method in `component.rs` (lines 122–134) must also be updated to populate the new `script_narrow` field from `flags.contains(Self::SCRIPT_NARROW)`.

---

### `tests/invalidation/basic.rs` — Unit tests for new routing

**Analog:** existing `basic.rs`, specifically `typed_invalidations_distinguish_restyle_from_script_rebuild` (lines 36–61)

**Test structure to copy** (lines 36–61):
```rust
#[test]
fn typed_invalidations_distinguish_restyle_from_script_rebuild() {
    let mut component = test_frontend_component("<template><button /></template>");
    component.dirty = false;
    component.style_only_dirty = false;
    component.dirty_types = ComponentDirtyFlags::empty();

    component.invalidate_interaction_restyle();
    assert!(component.wants_render());
    let (requires_tree_rebuild, can_use_retained_path, flags, _) = component.take_dirty_for_paint();
    assert!(!requires_tree_rebuild);
    assert!(can_use_retained_path);
    // ... flag assertions
}
// New tests follow identical structure. Examples:
// fn script_narrow_flag_does_not_trigger_tree_rebuild()
// fn service_event_skipped_when_no_intersecting_fields()
// fn threshold_fallback_exceeds_half()
```

**Service event test pattern from `service_update_marks_component_dirty_only_when_tracked_fields_change`** (lines 4–33):
```rust
#[test]
fn service_update_marks_component_dirty_only_when_tracked_fields_change() {
    let previous = serde_json::json!({ "percent": 65, "muted": false, ... });
    let unchanged_tracked = serde_json::json!({ "percent": 65, "muted": false, ... });
    let changed_tracked = serde_json::json!({ "percent": 66, ... });
    let tracked_fields = HashSet::from(["percent".to_string(), "muted".to_string()]);
    assert!(!tracked_service_fields_changed(Some(&previous), &unchanged_tracked, &tracked_fields));
    assert!(tracked_service_fields_changed(Some(&previous), &changed_tracked, &tracked_fields));
}
// New field-diff helper tests follow this inline-json + assert pattern.
```

---

### `tests/invalidation/profiling.rs` — Integration + pixel equivalence tests

**Analog:** existing profiling.rs, `log_phase31_proof()` and `phase26_real_surface_baseline_emits_canonical_proof_measurements` (lines 46–100+)

**FNV hash helper to add** (inline — matches `RuntimeTreeHasher` in `runtime_tree.rs:224`):
```rust
fn fnv_hash_buffer(buffer: &PixelBuffer) -> u64 {
    const OFFSET: u64 = 14695981039346656037;
    const PRIME: u64 = 1099511628211;
    let mut hash = OFFSET;
    for byte in &buffer.data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}
// Same FNV-1a constants as RuntimeTreeHasher (runtime_tree.rs:224).
// Do NOT use std::collections::DefaultHasher — it is not cross-run stable.
```

**`buffer_pixel()` accessor pattern for pixel data** (common.rs lines 320–328):
```rust
pub(super) fn buffer_pixel(buffer: &PixelBuffer, x: u32, y: u32) -> [u8; 4] {
    let offset = (y * buffer.stride + x * 4) as usize;
    [
        buffer.data[offset],
        buffer.data[offset + 1],
        buffer.data[offset + 2],
        buffer.data[offset + 3],
    ]
}
// PixelBuffer.data is a public Vec<u8> — iterate &buffer.data for FNV hash.
```

**`real_frontend_module_component()` pattern for integration tests** (common.rs lines 420+):
```rust
pub(super) fn real_frontend_module_component(
    module_id: &str,
    interface_catalog: InterfaceCatalog,
) -> FrontendSurfaceComponent { ... }
// Use with audio_network_catalog() for scenarios involving audio service events.
```

**`log_phase31_proof()` format as template for Phase 98 logging** (lines 46–70):
```rust
fn log_phase31_proof(scenario, records, snapshot) {
    eprintln!(
        "PHASE31_PROOF scenario={} ... retained={} full_rebuild={}",
        scenario, ..., snapshot.retained_path, snapshot.full_rebuild
    );
}
// Phase 98: add narrow_path={} and affected_node_count={} to the format string.
// Use trigger_kind="narrow" in profiling stage records from narrow_script_update().
```

**Test mount + paint cycle pattern** (profiling.rs lines 72–95):
```rust
let theme = default_theme();
let mut component = real_frontend_module_component("@mesh/navigation-bar", audio_network_catalog());
component.set_profiling_enabled(true);
let mut buffer = PixelBuffer::new(960, 80);
component.paint(&theme, 960, 80, &mut buffer).unwrap();
let snapshot = component.take_invalidation_snapshot().expect("...");
```

---

## Shared Patterns

### FNV-1a Deterministic Hashing
**Source:** `crates/core/shell/src/shell/component/runtime_tree.rs` lines 224–243 (`RuntimeTreeHasher`)
**Apply to:** pixel equivalence test helper in `tests/invalidation/profiling.rs`
```rust
const FNV_OFFSET: u64 = 14695981039346656037;
const FNV_PRIME: u64 = 1099511628211;
struct RuntimeTreeHasher(u64);
impl Default for RuntimeTreeHasher {
    fn default() -> Self { Self(FNV_OFFSET) }
}
impl Hasher for RuntimeTreeHasher {
    fn finish(&self) -> u64 { self.0 }
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }
}
```

### `RetainedNodeDirtyFlags` Structural-Change Detection
**Source:** `crates/core/shell/src/shell/component/runtime_tree.rs` lines 76–84
**Apply to:** tree diff logic in `rendering.rs` `narrow_script_update()`
```rust
pub(super) struct RetainedNodeDirtyFlags: u16 {
    const INSERTED = 1 << 0;
    const LAYOUT   = 1 << 1;
    const STYLE    = 1 << 2;
    const ATTRIBUTES = 1 << 3;
    const CHILDREN = 1 << 4;  // structural — forces TREE_REBUILD fallback
    const STATE    = 1 << 5;
}
// Structural check: flags.intersects(CHILDREN | INSERTED) → fall back to TREE_REBUILD.
```

### `NodeServiceFieldDependencies.nodes_reading_field()` Reverse Lookup
**Source:** `crates/core/shell/src/shell/component/runtime_tree.rs` lines 738–744
**Apply to:** `handle_service_event()` field-intersection check in `shell_component.rs`
```rust
pub(super) fn nodes_reading_field(&self, service: &str, field: &str) -> &HashSet<NodeId> {
    static EMPTY: std::sync::OnceLock<HashSet<NodeId>> = std::sync::OnceLock::new();
    let key = (service.to_string(), field.to_string());
    self.reverse.get(&key).unwrap_or_else(|| EMPTY.get_or_init(HashSet::new))
}
// Returns empty set (not None/Option) — safe to call `.is_empty()` directly.
```

### `RetainedNodeSnapshot.diff_flags()` Per-Node Diff
**Source:** `crates/core/shell/src/shell/component/runtime_tree.rs` lines 200–221
**Apply to:** `narrow_script_update()` in `rendering.rs` to classify leaf vs structural change
```rust
fn diff_flags(&self, next: &Self) -> (RetainedNodeDirtyFlags, u32) {
    let mut flags = RetainedNodeDirtyFlags::empty();
    if self.layout != next.layout      { flags |= RetainedNodeDirtyFlags::LAYOUT; }
    if self.style_hash != next.style_hash { flags |= RetainedNodeDirtyFlags::STYLE; }
    if self.attributes_hash != next.attributes_hash { flags |= RetainedNodeDirtyFlags::ATTRIBUTES; }
    if self.child_ids != next.child_ids { flags |= RetainedNodeDirtyFlags::CHILDREN; }
    // ...
    (flags, changed_state_bits)
}
// CHILDREN set → structural change → narrow path must fall back to TREE_REBUILD.
```

### `WidgetNode::node_count()` for Threshold
**Source:** `crates/core/ui/elements/src/tree.rs` line 111
**Apply to:** threshold guard in `narrow_script_update()` in `rendering.rs`
```rust
pub fn node_count(&self) -> usize {
    1 + self.children.iter().map(|c| c.node_count()).sum::<usize>()
}
// Use: affected_count * 2 > tree.node_count() → fall back to TREE_REBUILD.
```

### bitflags Pattern
**Source:** `component.rs` lines 67–135
**Apply to:** adding `SCRIPT_NARROW = 1 << 9` to the existing bitflags block
- New bit must be `1 << 9` (next available after `SURFACE_CONFIG = 1 << 8`)
- Compound constants (`TREE_REBUILD`, `INTERACTION_RESTYLE`, etc.) are `pub(super) const` — `SCRIPT_NARROW` must NOT be added to `TREE_REBUILD`

---

## No Analog Found

All files to be modified have close existing analogs in the codebase. No new patterns from RESEARCH.md need to be applied without a codebase counterpart.

---

## Metadata

**Analog search scope:** `crates/core/shell/src/shell/component/`, `crates/core/foundation/debug/src/`
**Files scanned:** 8
**Pattern extraction date:** 2026-06-09
