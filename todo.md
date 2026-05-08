Span takes defaultly full width of the parent component, the tags should initially take the space as possible. So defaultly the size of text inside

Icon rendering using icon packs

Settings module to manager modules and core settings like theme and i18n

Popups, also with custom content rendering if users desire

Keybind management

Layer system, so that we can specify what to render on what layer

make sure positioning system works (relative, absolute, fixed)

Variable state management and and binding for components

Clean up the backend modules and interfaces, right now interfaces are separate from the backend, we should check our options and consider moving the interface into the module itself

Remove the icon assts from the core/ui. The icons should be installed into a folder outside the core

# Separate milestons

- GPU rendering
- i18n configurations
- package manager
- lsp / extension
- unify configurations to use .json configuration
- Improve Icon packs
- Keyboard control with custom keybinds
- 



# Major performance fixes

See `docs/performance-roadmap.md` for the durable roadmap.

Current retained-rendering status:

- Stable runtime node IDs are implemented from `_mesh_key`.
- Style-only renders now mutate the retained cached `WidgetNode` tree instead of
  cloning it.
- Retained widget-tree dirty summaries now track inserted, removed, layout,
  style, attribute, child-order, and state changes by stable node ID.
- Full dirty renders still rebuild the widget tree.
- There is not yet a retained render-object tree, retained display list,
  incremental layout consumer, or damage tracking.

Implementation order:

1. Retained widget tree with stable node identity and dirty summaries. Done for
   the widget layer.
2. Dirty-type invalidation for script/state, style, layout, paint, text,
   accessibility, metrics, and surface configuration.
3. Incremental style and layout propagation.
4. Retained display list plus damage tracking.
5. Text shaping and glyph cache.
6. Typed attribute/style slots and interned identifiers.
7. Selector indexing for restyle.
8. Display-list batching.
9. GPU backend through wgpu/Vulkan/OpenGL.
10. Parallel paint/layout after ownership bottlenecks are removed.

Key sequencing note: adding a GPU backend before retained rendering gives
diminishing returns, because rebuilding the tree and uploading brand-new paint
data every frame wastes much of the GPU win. The right sequence is retained
tree, dirty invalidation, damage tracking, retained display list, then GPU.
