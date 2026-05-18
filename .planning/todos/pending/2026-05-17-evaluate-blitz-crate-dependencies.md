---
created: 2026-05-17T22:14:41.564Z
title: Evaluate Blitz crate dependencies
area: planning
resolves_phase: 42
files: []
---

## Problem

The user flagged Blitz's crate split and dependency choices as relevant prior
art for MESH's future DOM/layout/rendering/windowing architecture. Blitz keeps
the core DOM abstraction separate from parsing, rendering, networking, and
shell integration:

- `blitz-dom` owns the core DOM abstraction, style resolution, layout, and
  event handling, but not parsing, rendering, or system integration.
- `blitz-traits` provides minimal shared types/traits so crates can
  interoperate without directly depending on each other.
- Additional crates layer networking, painting, HTML parsing, and shell/window
  integration around the core.
- AnyRender now lives separately at
  `https://github.com/dioxuslabs/anyrender`.

The open architectural question is whether MESH should adopt or evaluate the
same proven ecosystem dependencies instead of reinventing those layers:
Stylo for CSS parsing/resolution, Taffy for box-level layout, Parley for text
layout, reqwest for resource fetching, anyrender for drawing abstraction,
html5ever/xml5ever for HTML/XHTML parsing if needed, and winit/accesskit/muda
for shell/window/accessibility/menu integration.

## Solution

Before future rendering, DOM, layout, or shell-integration phases, run a
focused architecture evaluation:

- Map MESH's current rendering/layout/input/accessibility modules against the
  Blitz crate boundary model: core DOM, traits, net, paint, html, shell.
- Identify which dependencies can be reused directly, which are overkill for
  MESH's shell/toolkit constraints, and which would conflict with existing
  retained rendering work.
- Prefer proven crates where they fit, especially Taffy, Parley, AccessKit,
  Winit, and a drawing abstraction such as AnyRender, but avoid adopting a web
  engine shape if it would undermine MESH's retained shell-specific model.
- Capture findings in a future roadmap/spike before committing to broad
  rewrites.
