# Typography, interaction and motion capabilities

Requested scope: support these techniques throughout editable shell compositions.
“Origin-aware animation” is interpreted as motion originating at the trigger or
interaction point; “stagger” as sequenced entry/exit. This is source inspection,
not live compositor or visual verification. Missing search matches are candidates
for implementation investigation, not proof that no equivalent API exists.

## Ownership and delivery

Core owns text measurement/shaping, hit testing, semantic relationships, layout,
and animation lifecycle. Modules/themes own optical nudges, readable measures,
affordances, disclosure policy and sequencing choices. Extend ordinary props,
styles and host APIs; do not invent a native element for each design technique.
Each implementation updates the relevant spec/guide and includes behavioral or
render regressions. Keep all open tasks in docs/BACKLOG.md.

## Capability assessment and acceptance criteria

| Technique | Current evidence | Gap or required validation |
| --- | --- | --- |
| Kerning | `Prop::FontFeatures`, layout lowering and text contexts carry shaping feature strings. | Verify `kern` changes actual shaping and identical measurement/paint, including fallback fonts; document the supported attribute syntax and font-dependent behavior. |
| Tabular numbers | Same font-feature path can express a `tnum` request. | Verify real fonts produce equal digit advances, stable counters and cache invalidation; expose/document an author-friendly route without promising unsupported font features. |
| Tracking | `letter-spacing` resolves in `style/resolve/declaration.rs`, is measured and animated. | Verify units, inheritance, grapheme/ligature spacing and RTL; document working syntax and add only missing behavior. |
| Leading | `line-height` resolves, reaches layout/text and transitions. | Verify multiline baselines, clipping and independent text scaling, including font fallback; document supported units rather than assuming browser CSS parity. |
| X-height | No dedicated x-height sizing contract was identified; `Dimension` is Auto/Px/Percent/Content/Fit. | Design font metric access and optional size adjustment, including missing-metric fallback and cache invalidation; distinguish text x-height from icon optical-size axes. |
| Optical centre | Alignment/transforms already allow authored offsets. | Provide theme/component examples and metric-based baseline/ink-bound alignment where justified; retain authored correction for asymmetric icons, rather than automatic universal optical centering. |
| Line length | Existing width/max-width can constrain text in pixels or percentages. | Add font-relative readable measure (`ch`/`ex` or an equivalent typed API) consistently through measurement, scaling and font changes. |
| Widow avoidance | Ordinary wrapping exists; no balanced/pretty wrapping contract was identified. | Design bounded last-line word avoidance/balancing for UI text, with locale/grapheme handling and deterministic measurement/paint parity; pagination widows/orphans are outside this shell requirement. |
| Layout shift | Retained geometry and async resource loading exist; text changes may remeasure. | Define intrinsic-size reservation/aspect ratio and fallback-font metric policy; verify delayed resources, counters, disclosure and optimistic rollback do not unexpectedly move unrelated controls. Intentional layout changes remain allowed. |
| Touch target | Padding/min sizes can enlarge controls; scrollbar has private hit slop. | Specify reusable minimum target tokens and optional hit expansion with overlap, clipping, transformed coordinates and semantic bounds; verify touch routing separately from pointer hit testing. |
| Safe area | Popup constraints and output geometry exist; no general component-facing safe-area contract was identified. | Expose authoritative surface-local usable insets, distinguishing compositor work area from occlusions; specify unsupported-data fallback, multi-output/rotation/scale and live invalidation without guessing global coordinates. |
| Affordance | Native controls and theme state styling exist. | Module/theme examples and checks for hover/focus/pressed/disabled/busy, keyboard discoverability, contrast and non-color cues; no new affordance element. |
| Optimistic updates | Existing backlog records Settings pending overrides masking snapshots indefinitely. | Extend that item with correlated requests, authoritative reconciliation, rollback/error, rapid edits, out-of-order responses and profile/unmount cancellation; keep domain optimism in modules. |
| Combobox | `select` maps to ChoiceMenu; menus and popovers support navigation/typeahead. No editable combobox contract was identified. | Define an editable input plus listbox behavior contract: IME-safe filtering, active vs selected option, arrows/Enter/Escape/Tab, popup focus, async results, accessible relationships and virtualization. Reuse primitives where they enforce the contract. |
| Progressive disclosure | `details`, popovers and managed open/close/focus behavior exist. | Verify collapsed descendants leave focus/semantic traversal correctly and lazy content cleans up safely; document nested disclosure and advanced-settings patterns rather than add a special DSL. |
| Origin-aware animation | `transform-origin` and anchor-based popover placement exist. | Derive optional origin from resolved trigger/interaction geometry after edge constraints, including promoted popups, RTL, scaling and output changes; explicit authored origins remain supported. |
| Stagger | Keyframe delays already exist. | Document stable-key list sequencing using ordinary expressions; fill any index/delay API gap and specify insertion/removal, cancellation, bounded delay and reduced motion. |
| Interruptible animations | `transition.rs` has sampled-value retargeting, reversal shortening and regression tests. | Verify rapid toggles across popup/surface lifecycle, keyframes and delayed staggers; define cancellation/completion semantics and cleanup. Do not replace already working transition reversal. |
| OKLCH | `css_profile_paints_accept_theme_color_syntax` covers OKLCH paint parsing; theme parser yields RGBA bytes. | Document current conversion/output limits; design explicit perceptual interpolation for gradients/transitions with alpha, achromatic hue and gamut handling. Parsing alone does not establish wide-gamut output. |
| Shared element transitions | Keyed component identity and per-node transitions exist; no shared-element coordinator was identified. | Design explicit scoped source/destination IDs, capture/overlay ownership, geometry/clip mapping, destination readiness, focus semantics, interruption and reduced motion; start within one surface and scope cross-surface support to demonstrated presentation capabilities. |

Evidence roots: `crates/core/ui/elements/src/{prop.rs,layout/lowering.rs,style/types.rs,style/resolve/declaration.rs,element/contracts.rs}`;
`crates/core/frontend/render/src/{display_list/paint_node.rs,surface/text.rs}`;
`crates/core/ui/animation/src/{transition.rs,keyframes.rs}`;
`crates/core/ui/interaction/src/scroll.rs`; `docs/frontend/elements.md`;
`crates/core/ui/elements/src/style/tests/resolution.rs`.

## Suggested dependency order

Verify existing typography and interaction contracts first; add text metrics,
wrapping and target/safe-area primitives next. Build combobox behavior and repair
optimistic reconciliation independently. Establish animation cancellation and
resolved origins before shared-element coordination. Ship module/theme examples
with each capability, with keyboard, reduced-motion, localization and scale checks.
