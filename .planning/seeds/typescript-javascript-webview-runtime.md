---
title: Evaluate TypeScript, JavaScript, and WebView module execution
trigger_condition: Revisit when measured Luau limitations, strong ecosystem demand, or a renderer-platform decision justifies another runtime
planted_date: 2026-07-16
---

# Evaluate TypeScript, JavaScript, and WebView Module Execution

## Idea

Preserve the possibility of authoring MESH module behavior in TypeScript,
compiling it to JavaScript, and executing it in an embedded JavaScript runtime.
At that decision point, also evaluate whether embedding the script runtime alone
is worthwhile or whether a WebView-based component/rendering model would be the
more coherent JavaScript platform.

This is not current implementation direction. Luau remains the native runtime,
and MESH retains its native component, renderer, input, accessibility, and
Wayland surface architecture.

## Why revisit it

- TypeScript offers a familiar type system and a large authoring ecosystem.
- JavaScript may make existing libraries and developer tooling easier to reuse.
- A WebView could supply an integrated JavaScript, layout, styling, accessibility,
  and debugging platform instead of embedding JavaScript only for behavior.
- Conversely, a WebView may conflict with MESH's low-memory, shell-native,
  multi-surface, capability-controlled, compositor-integrated goals.

## Trigger conditions

Begin a measured investigation when at least one is true:

1. Profiling shows Luau execution or Rust/Luau host calls are a material shell
   bottleneck that cannot be addressed within the existing runtime.
2. Module authors demonstrate substantial demand for TypeScript or JavaScript
   library compatibility.
3. A mature embeddable JavaScript engine satisfies MESH requirements for
   sandboxing, memory isolation, startup time, deterministic host APIs, and Rust
   integration.
4. MESH is reconsidering its native renderer or intentionally expanding toward
   browser-compatible layout and styling.

## Required comparison

Compare at least these paths against the native Luau baseline:

1. TypeScript compiled to JavaScript, with JavaScript used only for behavior
   while MESH retains native components and rendering.
2. TypeScript compiled through a compatibility layer to Luau.
3. TypeScript/JavaScript components hosted in WebViews, with the browser engine
   owning more layout, styling, accessibility, and developer tooling.
4. Keeping Luau native while generating strict Luau types from MESH contracts.

Measure cold start, steady-state CPU, idle CPU, memory per module and surface,
host-call overhead, reload latency, package size, native dependencies, sandbox
strength, accessibility integration, input latency, Wayland multi-surface fit,
and debugging quality.

## Decision rule

Do not adopt a JavaScript runtime merely because TypeScript syntax is preferred.
Adopt it only when the complete runtime and authoring system produces a measured
ecosystem or product benefit large enough to justify carrying another execution
model. If JavaScript becomes native, decide explicitly whether native rendering
plus JavaScript or a WebView platform is the coherent long-term boundary; do
not drift into maintaining both accidentally.
