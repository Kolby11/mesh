# Text Rendering Integration: Next Steps

The current refactoring has successfully separated rendering into the `mesh-render-engine` crate, but text rendering needs to be fully integrated. Here's what needs to happen:

## Current Status

### What Works
- ✅ Icon rendering fully integrated
- ✅ Slider rendering implemented
- ✅ Scrollbar rendering implemented
- ✅ Background and border rendering
- ✅ Z-index based child rendering

### What Needs Work
- ⚠️ Text rendering (placeholder only)
- ⚠️ Input node rendering (placeholder only)
- ⚠️ Tooltip text rendering (placeholder only)

## Implementation Plan

### 1. Integrate TextRenderer into FrontendRenderEngine

**File to modify**: `crates/mesh-render-engine/src/painter.rs`

Currently, `TextRenderContext` is a placeholder. It needs to:

```rust
pub struct FrontendRenderEngine {
    text_renderer: TextRenderer,  // Import from mesh-renderer::text
}
```

**Import change needed**:
```rust
use mesh_renderer::text::TextRenderer;
```

### 2. Complete render_text_node()

This method needs to:
1. Get text from node attributes
2. Handle text overflow (ellipsis)
3. Handle RTL text direction
4. Measure and position text
5. Render with proper clipping

**Key logic** (from original `mesh-renderer::painter::Painter`):
- Text overflow handling with ellipsis
- RTL text direction detection
- Text alignment (left, right, center)
- Font metrics computation

### 3. Complete render_input_node()

This method needs to:
1. Display placeholder or value text
2. Show caret when focused
3. Handle text color for placeholder state
4. Vertical centering of text

### 4. Complete render_tooltip()

The current implementation doesn't render text. Needs:
1. Measure tooltip text dimensions
2. Position tooltip to avoid screen edges
3. Render text inside tooltip box
4. Apply tooltip styling (fonts, colors)

## Files Involved

1. **mesh-render-engine/src/painter.rs**
   - `struct FrontendRenderEngine` - add text_renderer field
   - `render_text_node()` - implement full text rendering
   - `render_input_node()` - implement cursor and text rendering
   - `render_tooltip()` - implement text rendering in tooltip

2. **mesh-renderer/src/text.rs** (reference)
   - `TextRenderer::measure_styled()` - get text dimensions
   - `TextRenderer::render_clipped()` - render text to buffer

## Reference Implementation

The original `mesh-renderer/src/painter.rs` contains the complete implementation that can be used as reference:

- Lines 175-230: `paint_text_node()` - text rendering with overflow
- Lines 233-285: `paint_tooltip()` - tooltip implementation
- Lines 287-360: `paint_input_node()` - input with caret
- Helper functions: `truncate_with_ellipsis()` for text overflow

## Integration Steps

1. Add `text_renderer: TextRenderer` field to `FrontendRenderEngine`
2. Initialize it in `FrontendRenderEngine::new()`
3. Implement `render_text_node()` using text_renderer
4. Implement `render_input_node()` using text_renderer
5. Update `render_tooltip()` to use text_renderer
6. Test with a simple frontend plugin that has text nodes
7. Verify font loading and rendering

## Compilation Check

After implementing, verify:
```bash
cd crates/mesh-render-engine && cargo check
```

All warnings related to unused text rendering parameters should disappear.
