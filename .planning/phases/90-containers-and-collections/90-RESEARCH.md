---
phase: 90-containers-and-collections
title: Research
status: complete
---

# Research

- Existing cross-surface popover support is already substantial: activation, focus transfer, escape return, and trigger registration are shell-owned.
- Debug inspector tabs are a low-risk shipped proof because they already behave like exclusive tabs through Luau handlers.
- Debug inspector surface rows are static bounded rows, so `list`/`list-item` can prove source semantics without introducing virtualization.
- The correct first behavior is source-aware activation/focus, not a new runtime tag set.
