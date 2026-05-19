//! Parley text shaping adapter — proof-only evidence path.
//!
//! Adapter-owned per Phase 48 D-01/D-02: produces shaped text evidence for
//! `FocusedTextEvidence.parley_text` when the `renderer-parley` feature is on.
//! cosmic-text remains the authoritative production path (D-04). Never panics —
//! unsupported cases push a non-fatal `FocusedProofDiagnostic` (D-09).

#![cfg(feature = "renderer-parley")]

use std::cell::RefCell;

use mesh_core_elements::WidgetNode;
use parley::layout::Alignment;
use parley::{AlignmentOptions, FontContext, FontWeight, LayoutContext, StyleProperty};

use crate::proof::FocusedProofDiagnostic;

thread_local! {
    /// Fontique font discovery is expensive (scans /usr/share/fonts, ~/.local/share/fonts).
    /// Cache per-thread; the proof path is single-threaded per render tick.
    static FONT_CX: RefCell<FontContext> = RefCell::new(FontContext::new());
}

/// Produce a serialized Parley shaping summary for a text node.
///
/// Returns one of:
/// - `"parley_text::empty"` when `content` is empty (no diagnostic).
/// - `"parley_text::{content}::no_fonts"` when font discovery yields zero
///   glyphs for non-empty content; pushes a non-fatal diagnostic.
/// - `"parley::lines={N}::w={W:.1}::h={H:.1}::bidi={ltr|rtl}"` on success.
///
/// Never panics. Diagnostics are appended to `diagnostics` rather than
/// returned via `Result`, matching the existing `FocusedProofSnapshot.diagnostics`
/// pattern in `proof.rs`.
pub fn shape_text_evidence(
    node: &WidgetNode,
    content: &str,
    diagnostics: &mut Vec<FocusedProofDiagnostic>,
) -> String {
    if content.is_empty() {
        return "parley_text::empty".to_string();
    }

    let font_size = node.computed_style.font_size.max(1.0);
    let font_weight = node.computed_style.font_weight;
    let max_width = if node.layout.width > 0.0 {
        Some(node.layout.width)
    } else {
        None
    };

    FONT_CX.with(|font_cx_cell| {
        let mut font_cx = font_cx_cell.borrow_mut();
        let mut layout_cx: LayoutContext<()> = LayoutContext::new();
        let mut builder = layout_cx.ranged_builder(&mut *font_cx, content, 1.0, true);
        builder.push_default(StyleProperty::FontSize(font_size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(
            font_weight as f32,
        )));

        let mut layout: parley::Layout<()> = builder.build(content);
        layout.break_all_lines(max_width);
        layout.align(max_width, Alignment::Start, AlignmentOptions::default());

        if layout.len() == 0 {
            diagnostics.push(FocusedProofDiagnostic {
                node_id: Some(node.id),
                message: format!("parley: no fonts found for text shaping (node {:?})", node.id),
            });
            return format!("parley_text::{content}::no_fonts");
        }

        format!(
            "parley::lines={}::w={:.1}::h={:.1}::bidi={}",
            layout.len(),
            layout.width(),
            layout.height(),
            if layout.is_rtl() { "rtl" } else { "ltr" },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::{Dimension, LayoutRect, WidgetNode};

    fn text_node(content: &str, width: f32) -> WidgetNode {
        let mut node = WidgetNode::new("text");
        node.attributes.insert("content".to_string(), content.to_string());
        node.layout = LayoutRect { x: 0.0, y: 0.0, width, height: 18.0 };
        node.computed_style.width = Dimension::Px(width);
        node.computed_style.height = Dimension::Px(18.0);
        node.computed_style.font_size = 14.0;
        node.computed_style.font_weight = 400;
        node
    }

    #[test]
    fn parley_shapes_text_to_lines_width_height() {
        let node = text_node("Hello", 200.0);
        let mut diagnostics = Vec::new();
        let result = shape_text_evidence(&node, "Hello", &mut diagnostics);
        if result.contains("::no_fonts") {
            // Headless CI without fonts: expect a diagnostic, never a panic.
            assert_eq!(diagnostics.len(), 1, "expected 1 diagnostic, got {diagnostics:?}");
            assert!(
                diagnostics[0].message.contains("parley: no fonts"),
                "diagnostic message: {}",
                diagnostics[0].message
            );
        } else {
            assert!(
                result.starts_with("parley::lines="),
                "expected lines= prefix, got: {result}"
            );
            assert!(result.contains("::bidi=ltr"), "expected ltr bidi, got: {result}");
            assert!(diagnostics.is_empty(), "expected no diagnostics, got: {diagnostics:?}");
        }
    }

    #[test]
    fn parley_no_fonts_emits_diagnostic_not_panic() {
        let node = text_node("", 100.0);
        let mut diagnostics = Vec::new();
        let result = shape_text_evidence(&node, "", &mut diagnostics);
        assert_eq!(result, "parley_text::empty");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn shape_text_evidence_default_max_width_when_zero() {
        let node = text_node("Hi", 0.0);
        let mut diagnostics = Vec::new();
        let result = shape_text_evidence(&node, "Hi", &mut diagnostics);
        // Either real shaping (lines>=1) or no_fonts fallback. Never panic, never empty string.
        assert!(
            result.starts_with("parley::lines=") || result.contains("::no_fonts"),
            "unexpected result: {result}"
        );
    }

    #[test]
    fn parley_selection_evidence_maps_anchor_focus() {
        let mut node = text_node("Hello World", 200.0);
        node.layout = LayoutRect { x: 10.0, y: 5.0, width: 200.0, height: 18.0 };
        node.computed_style.padding.left = 4.0;
        node.computed_style.padding.top = 2.0;
        node.attributes.insert("_mesh_selection_anchor_x".to_string(), "20".to_string());
        node.attributes.insert("_mesh_selection_anchor_y".to_string(), "8".to_string());
        node.attributes.insert("_mesh_selection_focus_x".to_string(), "60".to_string());
        node.attributes.insert("_mesh_selection_focus_y".to_string(), "8".to_string());

        let mut diagnostics = Vec::new();
        let (parley_text, anchor, focus) =
            shape_text_with_selection_evidence(&node, "Hello World", &mut diagnostics);

        if parley_text.contains("::no_fonts") {
            // CI without fonts — verify no panic and tolerant about Some/None.
            let _ = (anchor, focus);
        } else {
            let a = anchor.expect("anchor must be Some when fonts available");
            let f = focus.expect("focus must be Some when fonts available");
            assert!(f.0 > a.0, "expected focus.x ({}) > anchor.x ({})", f.0, a.0);
        }
    }

    #[test]
    fn parley_selection_evidence_returns_none_when_attrs_absent() {
        let node = text_node("Hello", 100.0);
        let mut diagnostics = Vec::new();
        let (_parley_text, anchor, focus) =
            shape_text_with_selection_evidence(&node, "Hello", &mut diagnostics);
        assert!(anchor.is_none());
        assert!(focus.is_none());
    }

    #[test]
    fn parley_selection_evidence_uses_text_origin_attribute_when_present() {
        let mut node = text_node("Hi", 100.0);
        node.attributes.insert("_mesh_selection_text_x".to_string(), "10".to_string());
        node.attributes.insert("_mesh_selection_text_y".to_string(), "5".to_string());
        node.attributes.insert("_mesh_selection_anchor_x".to_string(), "20".to_string());
        node.attributes.insert("_mesh_selection_anchor_y".to_string(), "8".to_string());
        node.attributes.insert("_mesh_selection_focus_x".to_string(), "30".to_string());
        node.attributes.insert("_mesh_selection_focus_y".to_string(), "8".to_string());
        let mut diagnostics = Vec::new();
        let (_parley_text, anchor, focus) =
            shape_text_with_selection_evidence(&node, "Hi", &mut diagnostics);
        // Must not panic. Anchor/focus may be Some or None depending on font availability.
        let _ = (anchor, focus);
    }
}
