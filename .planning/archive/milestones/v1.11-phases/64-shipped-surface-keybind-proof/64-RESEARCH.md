# Phase 64 Research

## Existing Coverage

- Navigation already declares `mesh.keybinds.mute`, subscribes the volume button, and has real-surface dispatch coverage.
- Audio popover has strong slider, button, focus, and transition coverage, but no manifest keybind action on the real surface.
- Locale/override/no-binding behavior is covered by focused resolver tests from Phases 61-62.

## Proof Gap

KPROOF-02 specifically asks for audio-popover keybinds or access keys. The smallest real-surface proof is to declare an audio-popover `toggle_mute` access key and subscribe the existing mute button's handler.
