---
phase: 03-frontend-reactivity-and-events
reviewed: 2026-05-02T19:04:51Z
depth: standard
files_reviewed: 9
files_reviewed_list:
  - crates/core/foundation/diagnostics/src/lib.rs
  - crates/core/runtime/scripting/src/context.rs
  - crates/core/shell/src/shell/component.rs
  - crates/core/shell/src/shell/layout.rs
  - crates/core/shell/src/shell/mod.rs
  - crates/core/shell/src/shell/types.rs
  - crates/core/ui/render/src/render.rs
  - docs/plugins/frontend/core/README.md
  - packages/plugins/frontend/core/navigation-bar/src/components/volume-button.mesh
findings:
  critical: 3
  warning: 0
  info: 0
  total: 3
status: issues_found
---

# Phase 03: Code Review Report

**Reviewed:** 2026-05-02T19:04:51Z
**Depth:** standard
**Files Reviewed:** 9
**Status:** issues_found

## Summary

Reviewed the Phase 03 frontend reactivity and event changes at standard depth. The implementation has user-visible correctness regressions around the documented render lifecycle, popover positioning, and inline volume control command routing.

## Critical Issues

### CR-01: Documented `onRender()` hook is never invoked during normal renders

**File:** `crates/core/shell/src/shell/component.rs:581`

**Issue:** `build_tree()` reads `runtime_state()` and immediately builds the widget tree, but no production path calls a component script's `onRender()` before taking that state snapshot. The docs and migrated frontend components rely on `onRender()` to derive globals from service proxy fields. For example, `volume-button.mesh` updates `icon_name`, `slider_value`, and `audio_tooltip` only inside `onRender()`, so service updates can mark the component dirty without ever refreshing the values the template actually renders. The only `onRender()` calls found are in tests.

**Fix:**
```rust
fn build_tree(&mut self, theme: &Theme, width: u32, height: u32) -> WidgetNode {
    self.call_render_hooks();
    self.active_theme.replace(theme.clone());
    let root_state = self.runtime_state(self.id()).unwrap_or_default();
    // existing build logic...
}

fn call_render_hooks(&mut self) {
    let mut runtimes = self.runtimes.lock().unwrap();
    for runtime in runtimes.values_mut() {
        if runtime.script_ctx.has_handler("onRender") {
            if let Err(source) = runtime.script_ctx.call_handler("onRender", &[]) {
                if let Some(diagnostics) = &self.diagnostics {
                    diagnostics.record_handler_error(
                        runtime.plugin_id.clone(),
                        "onRender",
                        source.to_string(),
                    );
                }
            }
        }
    }
}
```

### CR-02: Popover positioning computes top margin relative to the surface height

**File:** `crates/core/shell/src/shell/component.rs:1197`

**Issue:** `build_click_event()` sets `position.margin_top` to `(bottom - tree.layout.height).max(0.0)`. For a top navigation bar where the clicked button bottom equals the bar height, this produces `0`, so `volume-button.mesh` positions `@mesh/volume-bar` at the top edge instead of below the trigger. The surrounding type comments say `PositionSurface` should place the surface at the requested top-left coordinates.

**Fix:**
```rust
let position = serde_json::json!({
    "margin_left": left.round() as i32,
    "margin_top": bottom.round() as i32,
});
```

If some surface types need coordinate transforms, keep raw `bounds` in the event and perform that transform in `apply_position()` using the target surface layout instead of baking a top-bar-specific subtraction into every click event.

### CR-03: Inline volume slider sends a payload current audio backends ignore

**File:** `packages/plugins/frontend/core/navigation-bar/src/components/volume-button.mesh:82`

**Issue:** `onVolumeChange()` calls `audio:set_volume("default", normalized)`, which the proxy maps to a `ServiceCommand` payload shaped like `{ device_id = "default", volume = 0.0..1.0 }`. The bundled PipeWire and PulseAudio backends currently read `payload.percent` in `on_command_set_volume()`, and the shell's built-in slider path also dispatches `{ percent = percent }`. As a result, dragging the new inline slider will optimistically update the UI but the backend command resolves to `0` and can mute/drop volume instead of setting the selected value.

**Fix:**
```luau
if audio_ok and audio then
    mesh.events.publish("mesh.audio.set-volume", { percent = percent })
end
```

Alternatively, update the audio backend command handlers and the shell's `mesh-action="audio-volume"` path to honor the contract payload (`device_id`, `volume`) consistently, converting normalized `volume` to provider percent at the backend boundary.

---

_Reviewed: 2026-05-02T19:04:51Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_
