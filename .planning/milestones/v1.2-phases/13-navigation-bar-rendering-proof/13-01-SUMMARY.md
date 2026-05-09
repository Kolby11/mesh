---
phase: 13-navigation-bar-rendering-proof
plan: 01
subsystem: shipped-nav-surface
tags: [navigation-bar, selectable-text, layout]

requires:
  - phase: 10-01
    provides: Opt-in selectable text semantics on passive text nodes

provides:
  - Shipped navigation bar reorganized into explicit status and control clusters
  - Primary passive selectable status text on the real surface
  - Main-surface ownership of the audio popover preserved

affects:
  - phase-13-control-enrichment
  - phase-13-motion-proof
  - phase-13-responsive-proof

tech-stack:
  added: []
  patterns:
    - "Primary shell proof surfaces keep popover ownership in the top-level module"
    - "Selectable text remains narrow and passive on shipped shell chrome"

key-files:
  created:
    - .planning/phases/13-navigation-bar-rendering-proof/13-01-SUMMARY.md
  modified:
    - modules/frontend/navigation-bar/src/main.mesh
    - modules/frontend/navigation-bar/src/components/meta-label.mesh
    - modules/frontend/navigation-bar/src/components/meta-pill.mesh

key-decisions:
  - "The navigation bar now exposes one passive status cluster plus one control cluster instead of the old undifferentiated control strip."
  - "The primary selectable proof stays on a passive top-level status text node; no control wrapper became selectable."
  - "Audio popover wiring remains in `main.mesh` so the shipped shell surface continues to own placement and hide/show behavior."

requirements-completed: [NAV-01, NAV-03]

duration: 1 session
completed: 2026-05-08
---

# Phase 13 Plan 01: Shell Surface Restructure and Selectable Status Cluster

**The shipped navigation bar now reads as shell status plus controls, with selectable passive copy on the primary surface itself.**

## Accomplishments

- Rebuilt `modules/frontend/navigation-bar/src/main.mesh` around explicit `status-cluster` and `control-cluster` wrappers.
- Added a passive primary status text node with `selectable="true"` and short system-style audio detail copy below it.
- Kept `AudioPopover` rendering and `onToggleAudioSurface` ownership in the root module.
- Updated the helper label/pill components so they support the new status-cluster contract rather than the old placeholder dashboard language.

## Task Commits

Not created in this workspace run. The implementation is validated but currently uncommitted.

## Verification

- `grep -n "status-\|control-cluster" modules/frontend/navigation-bar/src/main.mesh`
- `grep -n 'selectable="true"' modules/frontend/navigation-bar/src/main.mesh`
- `grep -n "AudioPopover hidden={audio_surface_hidden}\|function onToggleAudioSurface" modules/frontend/navigation-bar/src/main.mesh`

## Deviations From Plan

None. The status copy stayed passive and the popover remained owned by the root surface as planned.

## Self-Check: PASSED

- Summary file exists.
- The shipped surface now contains the required cluster structure and selectable passive text proof.
- No button component gained `selectable="true"`.

---
*Phase: 13-navigation-bar-rendering-proof*
*Completed: 2026-05-08*
