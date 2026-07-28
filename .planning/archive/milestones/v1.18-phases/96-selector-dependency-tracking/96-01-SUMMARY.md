---
phase: 96-selector-dependency-tracking
plan: 01
type: execute
subsystem: style-resolution
tags: [selector-indexing, state-dependency, reverse-index, performance, invalidation-prep]
requires: []
provides: ["state_to_rules reverse index on StyleRuleIndex", "rules_for_state_bit() O(1) lookup"]
affects:
  - crates/core/ui/elements/src/style/resolve.rs
requirements: [SEL-01, SEL-02]
tech-stack:
  added: []
  patterns: ["HashMap<u32, Vec<usize>> for per-bit rule-index buckets"]
key-files:
  created: []
  modified:
    - crates/core/ui/elements/src/style/resolve.rs
decisions: []
metrics:
  duration_seconds: 165
  completed_date: "2026-06-07T19:29:36Z"
---

# Phase 96 Plan 01: Selector Reverse Index Summary

**One-liner:** Added `state_to_rules: HashMap<u32, Vec<usize>>` reverse index to `StyleRuleIndex` mapping individual state bits to dependent rule indices, with `rules_for_state_bit()` providing O(1) lookups for future narrow invalidation.

## What was built

Augmented `StyleRuleIndex` with a reverse state-to-rules index (`state_to_rules`) that maps individual state bits (e.g., `STATE_HOVERED = 1 << 0`) to the rule indices that depend on that specific state. This separates per-bit dependencies from the existing combined-bitmask `state: Vec<(u32, Vec<usize>)>` used for forward candidate-rule lookup.

**Changes in `crates/core/ui/elements/src/style/resolve.rs`:**

1. **`state_to_rules` field** (line 198): New `HashMap<u32, Vec<usize>>` on `StyleRuleIndex` struct, populated during index construction alongside the existing `state` Vec.

2. **`rules_for_state_bit(bit: u32) -> &[usize]` method** (line 312): Public API returning all rule indices that depend on a given state bit. O(1) HashMap lookup. Returns empty slice for unused bits.

3. **Compound selector state indexing** (line 275): Modified `index_selector()` to also iterate state parts from `Selector::Compound` selectors (e.g., `button:hover`) so compound rules populate `state_to_rules` in addition to their primary index key.

4. **5 unit tests** (lines 1642-1726): Cover zero rules, single-state rule, state-bit discrimination, compound selectors, and multiple rules per bit.

## Task Execution

| Task | Name | Type | Commit | Status |
|------|------|------|--------|--------|
| 1 | Add state_to_rules field + populate during indexing | auto | `ba55d9c` | Done |
| 2 | Add rules_for_state_bit() with unit tests | auto (TDD) | `b58563d` (RED), `1f41d2a` (GREEN) | Done |

## Verification Results

- `cargo test -p mesh-core-elements state_to_rules` — **5/5 new tests pass**
- `cargo test -p mesh-core-elements` — **101 existing tests pass, zero regressions**
- `cargo check -p mesh-core-elements` — **zero errors, zero warnings**
- `cargo clippy -p mesh-core-elements` — **zero new lint warnings**

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Plan test code used incorrect type names for actual codebase**
- **Found during:** task 2 (test writing)
- **Issue:** Plan's sample test code referenced `StyleDeclaration` (actual type is `Declaration`), `StyleValue::Literal("red")` (correct but struct name `Declaration` not `StyleDeclaration`), `container_queries: Vec::new()` (actual field is `container_query: Option<ContainerQuery>`), and `Selector::State(None, state)` (actual variant is `State(String, String)` requiring an empty-string tag).
- **Fix:** Adapted all test code to use actual codebase types: `Declaration`, `container_query: None`, `Selector::State("".to_string(), state.to_string())`.
- **Files modified:** `crates/core/ui/elements/src/style/resolve.rs`
- **Commit:** `b58563d`

**2. [Rule 1 - Bug] Compound selectors with state parts not populating reverse index**
- **Found during:** task 2 GREEN phase (Test 4 failed)
- **Issue:** `index_selector()` calls `selector_index_key()` which returns only the primary key for compound selectors — the tag takes priority, so `button:hover` was indexed only as a tag rule, skipping the hover state. The plan's requirement that "every rule with a state selector" populates `state_to_rules` was not met for compound selectors.
- **Fix:** Added compound-part iteration in `index_selector()` that calls `index_state_selector()` for each `Selector::State` part found in compound selectors, before the primary-key indexing. This ensures compound rules like `button:hover` correctly index their state bits in `state_to_rules`.
- **Files modified:** `crates/core/ui/elements/src/style/resolve.rs`
- **Commit:** `1f41d2a`

## Self-Check: PASSED

- `crates/core/ui/elements/src/style/resolve.rs` — FOUND (modified with 103 insertions)
- Commit `ba55d9c` — FOUND (feat: state_to_rules field)
- Commit `b58563d` — FOUND (test: RED phase)
- Commit `1f41d2a` — FOUND (feat: GREEN phase)
- All 5 new tests pass, zero regressions on existing 101 tests
