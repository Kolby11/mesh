# Phase 29 Plan 01 Benchmark Proof

**Captured:** 2026-05-11
**Purpose:** Record Phase 29 proof against the existing canonical shipped-surface benchmark IDs without adding a second benchmark harness.

## Canonical Scenario IDs

Phase 29 continues to use the five Phase 26 scenario IDs:

| Scenario ID | Shipped target | Current proof command |
|-------------|----------------|-----------------------|
| `hover` | `@mesh/navigation-bar` | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture` |
| `surface_open_close` | `@mesh/audio-popover` | Same command |
| `pointer_update` | `@mesh/navigation-bar audio controls` | Same command |
| `keyboard_traversal` | `@mesh/navigation-bar focus chain` | Same command |
| `backend_update` | `mesh.audio -> @mesh/pipewire-audio` | Same command |

## Captured Shipped-Surface Output

```text
PHASE26_BASELINE hover style_restyle=3069us paint=3739us traversal=1650us retained=false full_rebuild=true
PHASE26_BASELINE surface_open_close paint=5927us traversal=3632us shaping=1452us retained=false full_rebuild=true
PHASE26_BASELINE pointer_update layout=314us paint=2415us traversal=1419us retained=false full_rebuild=true
PHASE26_BASELINE keyboard_traversal style_restyle=3049us paint=3618us traversal=1495us retained=false full_rebuild=true
PHASE26_BASELINE backend_update paint=9638us traversal=6053us shaping=1441us retained=false full_rebuild=true
```

The command passed in this environment. Exact microsecond values are expected to vary between runs; the stable acceptance signal is that all five canonical scenario IDs still execute through the existing proof path.

## Filtered Execution Proof

Focused render and shell tests now provide deterministic Phase 29 proof:

| Proof | Command | Evidence |
|-------|---------|----------|
| Retained span metadata and repaint policy labels | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` | `display_list_records_span_metadata_and_policy_labels` asserts retained command span metadata and the exact labels `minimal_damage`, `bounding_rect`, and `full_surface`. |
| Sparse damage skips unrelated commands | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` | `display_list_filters_sparse_damage_without_reordering_commands` asserts `filtered_commands_skipped > 0`, fewer filtered commands than full commands, preserved command order, and scrollbar inclusion. |
| Full-surface fallback remains explicit | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render display_list` | `display_list_full_surface_policy_keeps_all_commands_and_records_fallback` asserts full command preservation and `filtered_fallback_count == 1`. |
| Debug payload carries aggregate proof | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` | Shell profiling tests assert `repaint_policy`, `filtered_span_count`, `filtered_command_count`, `filtered_commands_skipped`, and `filtered_fallback_count` under `invalidation.paint`. |

## Interpretation

Phase 29 changes the retained CPU path so partial damage can feed the painter a filtered ordered command input instead of always sending the full retained command list. The shipped-surface proof command still reports the canonical scenario timing rows, while the focused retained-render tests lock the new filtered-execution mechanics that later Phase 31 tuning can compare against visible smoothness.

## Limits

- This artifact records deterministic filtered-execution proof plus the existing shipped-surface benchmark command output. It does not claim final visible smoothness; Phase 31 owns that milestone-level acceptance.
- The Phase 26 proof command name is retained intentionally so the benchmark harness remains unchanged.
