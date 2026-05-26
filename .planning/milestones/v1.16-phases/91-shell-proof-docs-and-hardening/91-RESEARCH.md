---
phase: 91-shell-proof-docs-and-hardening
title: Research
status: complete
---

# Research

- Shipped surface migrations are safest when they preserve existing class names and handler wiring.
- Audio popover is already a separate shell popover surface; changing the root source tag to `popover` adds semantic proof without changing runtime placement.
- Debug inspector is a dialog-like surface and already has stable tests for all four views.
- Backend services view still uses boxed rows and empty state; it can mirror the surfaces view migration.
