# Interaction identity migration — 2026-07-28

Archived completion record for the former active-backlog item “Remaining
interaction identity is string-keyed end to end.”

## Outcome

The shell’s steady-state interaction storage and annotation now use stable
`NodeId` values:

- Scroll offsets, animations, hit results, overflow annotation, and
  scroll-into-view state.
- Checked state and its previous-frame restyle snapshot.
- Text input, select, and radio-group values.
- Slider live values and script-value snapshots.
- Retained hover paths and their previous-frame snapshots.
- Authoritative focus and focus-visible state plus their restyle snapshots.

Readable structural keys remain only at imperative-ref, tooltip, keyboard, and
event-dispatch boundaries. They are no longer consumed by steady-state
interaction annotation or targeted-restyle lookup.

## Evidence and regression gates

Existing release measurements recorded in the former backlog:

| Checkpoint | Gate | Result |
| --- | --- | --- |
| Scroll offsets | `node_id_scroll_offsets_speedup` | 4.39–4.66x |
| Checked state | `node_id_checked_state_speedup` | 1.66–1.69x |
| Input values | `node_id_input_values_speedup` | 1.67–1.68x |

The canonical gate now also includes `node_id_slider_values_speedup`,
`node_id_hover_path_speedup`, and `node_id_focus_state_speedup`. Their release
measurements are pending repair of the local Rust linker wrapper.

Correctness coverage spans pointer and keyboard slider updates, hover
transitions, focus navigation, imperative refs, input preservation, and
targeted interaction restyling.
