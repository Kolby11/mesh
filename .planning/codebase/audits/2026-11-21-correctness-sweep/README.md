# MESH correctness sweep, 2026-11-21

Parallel source review covered representative runtime, frontend, presentation,
module and authoring paths. The [November 6 inventory](../2026-11-06-whole-codebase/00-coverage.md)
tracks the remaining file-level work for the whole-codebase audit.

## Changes verified by focused regressions

| Area | Flow reviewed | Outcome |
| --- | --- | --- |
| Component language and interaction | Template input → Luau expression boundaries. Retained geometry → pointer focus and tab traversal. | Compare complete Unicode scalars, recognize loop options only at an assignment, and traverse visible overflow inside an ancestor clip. |
| Frontend runtime | Scroll position → measured and estimated rows → retained window | Recover after jumps in either direction and include the leading spacer gap when anchoring the visible row. |
| Provider lifecycle | Host callbacks and process exits → provider state → typed results | Match stream identity, use elapsed network sample time, preserve negative pointer speed and report successful recordings. Keep optional provider results and transport outcomes distinct. |
| Presentation | In-process dispatch → poll → scene submission and import | Resume on ready work, reject scene geometry superseded by resize or scale, and send an output-clamped wire size for fixed layer axes. The live matrix verifies the selected backend and changed extent. |
| Theme and authoring | Module/local token references → resolved theme. Catalog source → authoring diagnostics. | Resolve a scoped alias through its module's tokens and then global tokens. Share the runtime's bounded catalog read with authoring diagnostics. |
| Shell diagnostics | Snapshot capture → cache key and publication | Associate the payload with the generation captured before collection. A concurrent diagnostic update triggers a rebuild. |
| Package tooling | Root inventory → editable source → stage, lock, and store | Update, verify, and roll back at the declared path. A local Git fixture covers staging, replacement, verification, and rollback at `editable/audio`. |
| LSP | Document edits and saves → module registry → diagnostics | Keep unsaved edits local and refresh the on-disk registry on a worker. Successful refreshes update open-document diagnostics. Resolve module ownership when `modulesDir` contains `..`. [Measurements](../../../log/performance-log.md#2026-11-21--refresh-the-on-disk-lsp-registry-after-saves). |

## Additional findings

- Paint culling consults the owner's visual clip after subtree preclipping.
  An offscreen owner can omit a visible descendant, and the subtree bound
  walk still needs the viewport transform for fixed descendants. A regression
  should compare full commands and compositor blur regions under transformed,
  scrolled and composited ancestors.
- Variable-height window selection scans row pitches to locate the band and
  compute spacers. The compiler also materializes the iterable before emitting
  a window. Measure driver visits, allocations and frame time with 2,000 and
  20,000 mixed-height rows before choosing a data structure.
- Backend blob publication currently precedes callback success and shell
  contract acceptance. Hold references until the replacement snapshot is
  accepted, and check that an aborted update leaves the current blob readable.
- A suspended hypridle config write can overlap a poll or preference update.
  Serialize the write-and-launch sequence and exercise an edit during a pending
  write. Recorder attempts also need unique paths when their timestamps match.
- Flattening a module ID into the same dotted key as a token name admits
  collisions. Keep those two identifiers distinct through lookup and cycle
  detection. Test a module whose ID adds a dotted suffix to another module ID.

Open items are tracked in the [backlog](../../../../docs/BACKLOG.md).

## Verification

- The frontend runtime suite passed 754 active tests. The 2,000-row fixture
  exercises both scroll directions. Parser and interaction suites passed.
- Scripting and shell library suites passed 412 and 437 active tests.
- The headless workspace suite passed all targets except a shell sizing
  fixture that opened Wayland. The corrected fixture uses the test backend.
  The full GPU-feature shell library rerun passed all 438 active cases.
- Module and CLI suites passed 374 and 19 active tests.
- Live Hyprland 0.56.2 passed the queued-input regression and all five active
  compositor matrix cases with GPU required.
- All six production icon cases passed in ten consecutive reruns. The fixtures
  distinguish an unresolved cold icon from an in-progress theme switch with a
  previously rendered icon.
- An LSP stdio smoke test verified that saving a module token declaration
  clears the warning in the already-open component without editing it.
