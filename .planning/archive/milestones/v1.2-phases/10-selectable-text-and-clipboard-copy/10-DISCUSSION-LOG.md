# Phase 10: Selectable Text and Clipboard Copy - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-06
**Phase:** 10-Selectable Text and Clipboard Copy
**Areas discussed:** Selectable scope, Selection boundaries, Highlight styling, Copy ownership

---

## Selectable scope

| Option | Description | Selected |
|--------|-------------|----------|
| A | Only `<text selectable="true">...` | ✓ |
| B | All `text` nodes unless explicitly disabled | |
| C | Only user-facing copy surfaces by default, controls/tooltips off unless opted in | |

**User's choice:** Explicit opt-in only for selectable text.
**Notes:** User was unsure which shell surfaces should be selectable and asked to anchor the decision in mainstream shell precedent. Discussion converged on a conservative shell pattern closer to GNOME/KDE/Windows shell chrome rather than broad copyability.

| Option | Description | Selected |
|--------|-------------|----------|
| A | Never selectable inside interactive controls | ✓ |
| B | Selectable only on long drag; normal click still wins | |
| C | Selectable whenever marked selectable, even inside controls | |

**User's choice:** No selection inside interactive controls.
**Notes:** This keeps button, switch, and slider interactions unambiguous in the first release.

| Option | Description | Selected |
|--------|-------------|----------|
| A | No, selection walks text nodes only | ✓ |
| B | Yes, but only as transparent bridges between adjacent text nodes | |
| C | Yes, selection can span any nested component structure if rendered text is continuous | |

**User's choice:** Text nodes only.
**Notes:** This intentionally avoids a broader DOM-like selection model in the first release.

| Option | Description | Selected |
|--------|-------------|----------|
| A | Conservative: nothing becomes selectable until modules opt in | |
| B | Make obvious read-only copy selectable now, like labels/status text | ✓ |
| C | Broad default for most text, then carve out exceptions later | |

**User's choice:** Narrow `B`, then refined further.
**Notes:** The user ultimately chose a small non-interactive proof fixture or passive read-only text block, not shell chrome.

---

## Selection boundaries

| Option | Description | Selected |
|--------|-------------|----------|
| A | Only within a single selectable text node | ✓ |
| B | Across adjacent selectable text nodes in the same read-only text block | |
| C | Across any selectable text nodes in the same surface, even if structurally separate | |

**User's choice:** Single selectable text node only.
**Notes:** This is the strongest narrowing choice and directly constrains planning.

| Option | Description | Selected |
|--------|-------------|----------|
| A | Natural line-wrap selection inside that text block | |
| B | Only contiguous character ranges in source order, even if the visual result feels odd | ✓ |
| C | First release supports single-line only; wrapped selection later | |

**User's choice:** `B`
**Notes:** The user's reply was `1A 2B 3C 4A`. In the surrounding discussion, this was interpreted and summarized as allowing wrapped text selection within a single text node while keeping the implementation bounded. Planning should preserve the user's intent but may want to sanity-check this interpretation against the explicit option text if ambiguity matters.

| Option | Description | Selected |
|--------|-------------|----------|
| A | Copy only the visible selected text | |
| B | Copy the full underlying selected text, even if some of it is clipped/ellipsized | |
| C | Disallow selection on clipped/ellipsized text in Phase 10 | ✓ |

**User's choice:** No clipped/ellipsized selection in Phase 10.
**Notes:** This is one of the places where user preference now conflicts with current `TEXT-04` wording.

| Option | Description | Selected |
|--------|-------------|----------|
| A | Clamp the selection to the last valid selectable character | ✓ |
| B | Cancel the whole selection | |
| C | Keep the anchor and resume if the pointer re-enters selectable text before mouse-up | |

**User's choice:** Clamp to the last valid selectable character.
**Notes:** Chosen as the most predictable behavior around shell chrome and mixed selectable/non-selectable areas.

---

## Highlight styling

| Option | Description | Selected |
|--------|-------------|----------|
| A | New dedicated theme tokens like `color.selection-background` and `color.selection-foreground` | ✓ |
| B | Derive from existing tokens for now, and add dedicated tokens later if needed | |
| C | Hardcode a temporary engine default first, theme integration later | |

**User's choice:** Add dedicated theme tokens now.
**Notes:** This keeps selection visuals aligned with the existing theme-owned design system.

| Option | Description | Selected |
|--------|-------------|----------|
| A | Selection colors fully override the normal text colors while selected | ✓ |
| B | Keep text foreground if readable, but always override background | |
| C | Skip selection highlight on unusually styled text in Phase 10 | |

**User's choice:** Full override while selected.
**Notes:** Clear, consistent, and easier to reason about in the first release.

| Option | Description | Selected |
|--------|-------------|----------|
| A | No, keep it shell/theme-owned and consistent everywhere | ✓ |
| B | Limited override via a small set of style hooks | |
| C | Yes, full component-level control of selection colors | |

**User's choice:** No per-component selection styling in Phase 10.
**Notes:** This keeps the first release small and cohesive.

| Option | Description | Selected |
|--------|-------------|----------|
| A | Strong and unmistakable, even if a bit blunt | |
| B | Subtle and elegant, closer to passive shell chrome | |
| C | Follow the base theme contrast rules, whatever that yields | ✓ |

**User's choice:** Theme contrast should decide the effective visibility.
**Notes:** The user preferred a theme-driven result over a hardcoded strong/subtle bias.

---

## Copy ownership

| Option | Description | Selected |
|--------|-------------|----------|
| A | Only when a Phase 10 text selection exists; otherwise normal focused-control behavior wins | ✓ |
| B | Selected text always wins, even if an input/control is focused | |
| C | Copy selected text only when no control is focused | |

**User's choice:** Standard copy shortcut only when a Phase 10 text selection exists.
**Notes:** This preserves normal input/control expectations.

| Option | Description | Selected |
|--------|-------------|----------|
| A | Keep the selection visible | ✓ |
| B | Clear the selection immediately | |
| C | Make this a future config/theme choice; pick one conservative default now | |

**User's choice:** Keep selection visible after copy.
**Notes:** Chosen as the more standard text-selection feel.

| Option | Description | Selected |
|--------|-------------|----------|
| A | Any plain click outside the selected text | |
| B | Any new pointer-down anywhere | |
| C | Keyboard input aimed at another control, surface hide/rebuild that removes the node, or explicit click elsewhere | ✓ |

**User's choice:** Clear on explicit click elsewhere, control-directed keyboard input, or removal of the selected node.
**Notes:** This keeps selection persistent without making it sticky forever.

| Option | Description | Selected |
|--------|-------------|----------|
| A | No, stick to standard explicit copy shortcut only | ✓ |
| B | Support copy-on-select | |
| C | Support X11-style selection buffer / middle-click paste too | |

**User's choice:** No copy-on-select or primary-selection behavior.
**Notes:** The user wanted a standard explicit copy path only.

---

## the agent's Discretion

- Exact runtime representation for selection anchors/ranges.
- Exact glyph hit-testing strategy for wrapped text.
- Exact clipboard plumbing path.
- Exact minimal proof surface and test fixture.

## Deferred Ideas

- Cross-node and nested-tree text selection.
- Selection on clipped or ellipsized text.
- Copy-on-select and X11 primary-selection behavior.
- Navigation-bar as the full proof surface.
