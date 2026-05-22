//! AnyRender paint adapter - proof-only encoding evidence.
//!
//! Adapter-owned per Phase 49 D-01/D-02: encodes retained display-list paint
//! commands into an `anyrender::recording::Scene` and returns the count of
//! encoded scene ops. The software painter (`surface/painter.rs`) remains
//! authoritative for pixel output (PAINT-03).
//!
//! Coverage per Phase 49 D-09: backgrounds, borders, icons. Text is encoded
//! only when both `renderer-parley` and `renderer-anyrender` are active
//! (handled by later adoption work); without both flags, text commands emit a
//! single non-fatal `FocusedProofDiagnostic` and return 0.
//!
//! Deferred lossless subset per Phase 49 D-10 (PAINT-01 "documented subset"):
//! `DisplayPaintContent::Slider`, `DisplayPaintContent::Input`, and
//! `DisplayPaintCommandKind::Scrollbars` are intentionally NOT encoded.
//! Never panics.

#![cfg(feature = "renderer-anyrender")]

use anyrender::PaintScene;
use anyrender::recording::Scene;
use peniko::Fill;
use peniko::kurbo::{Affine, Rect, RoundedRect, RoundedRectRadii, Stroke};

use crate::DisplayPaintCommand;
use crate::display_list::{DisplayPaintCommandKind, DisplayPaintContent};
use crate::proof::FocusedProofDiagnostic;

fn to_peniko_color(c: mesh_core_elements::style::Color) -> peniko::Color {
    color::Rgba8 {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
    .into()
}

fn command_rect(command: &DisplayPaintCommand) -> Rect {
    let l = &command.node.layout;
    Rect::new(
        l.x as f64,
        l.y as f64,
        (l.x + l.width) as f64,
        (l.y + l.height) as f64,
    )
}

fn encode_background(scene: &mut Scene, command: &DisplayPaintCommand) -> usize {
    if command.node.style.background_color.a == 0 {
        return 0;
    }
    let rect = command_rect(command);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        to_peniko_color(command.node.style.background_color),
        None,
        &rect,
    );
    1
}

fn encode_border(scene: &mut Scene, command: &DisplayPaintCommand) -> usize {
    let bw = &command.node.style.border_width;
    let avg = (bw.top + bw.right + bw.bottom + bw.left) / 4.0;
    if avg <= 0.0 {
        return 0;
    }
    let rounded = RoundedRect::from_rect(
        command_rect(command),
        RoundedRectRadii::from_single_radius(command.node.style.border_radius as f64),
    );
    scene.stroke(
        &Stroke::new(avg as f64),
        Affine::IDENTITY,
        to_peniko_color(command.node.style.border_color),
        None,
        &rounded,
    );
    1
}

fn encode_icon(scene: &mut Scene, command: &DisplayPaintCommand) -> usize {
    // The proof posture encodes the icon bounds. Real SVG/glyph rasterization
    // remains owned by the current software painter.
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        to_peniko_color(command.node.style.color),
        None,
        &command_rect(command),
    );
    1
}

/// Encode a single display-list paint command into an AnyRender recording scene.
/// Returns the number of scene ops encoded; 0 means no-op or deferred subset.
pub fn encode_command_to_scene(
    command: &DisplayPaintCommand,
    diagnostics: &mut Vec<FocusedProofDiagnostic>,
) -> usize {
    // DEFERRED per Phase 49 D-10: scrollbar painting is outside the lossless
    // subset this adapter proves.
    if matches!(command.kind, DisplayPaintCommandKind::Scrollbars) {
        return 0;
    }

    let mut scene = Scene::new();
    let mut encoded = 0usize;

    match &command.node.content {
        DisplayPaintContent::None => {
            encoded += encode_background(&mut scene, command);
            encoded += encode_border(&mut scene, command);
        }
        DisplayPaintContent::Icon(_) => {
            encoded += encode_background(&mut scene, command);
            encoded += encode_border(&mut scene, command);
            encoded += encode_icon(&mut scene, command);
        }
        DisplayPaintContent::Text(_) => {
            #[cfg(not(feature = "renderer-parley"))]
            diagnostics.push(FocusedProofDiagnostic {
                node_id: Some(command.node.id),
                message: "anyrender: combined parley+anyrender text path not active".to_string(),
            });
            #[cfg(feature = "renderer-parley")]
            let _ = diagnostics;
        }
        DisplayPaintContent::Slider(_) | DisplayPaintContent::Input(_) => {
            // DEFERRED per Phase 49 D-10: controls stay on the software painter
            // until their full lossless command subset is specified.
        }
    }

    debug_assert_eq!(scene.commands.len(), encoded);
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "renderer-parley"))]
    use crate::display_list::DisplayTextPaint;
    use crate::display_list::{
        DisplayIconPaint, DisplayInputPaint, DisplayListClip, DisplayPaintNode, DisplayPaintStyle,
        DisplayScrollbars, DisplaySliderPaint,
    };
    use mesh_core_elements::style::{
        Color, Edges, Overflow, TextAlign, TextDirection, TextOverflow,
    };
    use mesh_core_elements::{BoxShadow, LayoutRect, VisualFilter};

    fn base_style() -> DisplayPaintStyle {
        DisplayPaintStyle {
            background_color: Color::TRANSPARENT,
            border_color: Color::TRANSPARENT,
            border_width: Edges::zero(),
            border_radius: 0.0,
            color: Color::BLACK,
            padding: Edges::zero(),
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            font_family: String::new(),
            font_size: 14.0,
            font_weight: 400,
            line_height: 16.0,
            text_align: TextAlign::Left,
            text_overflow: TextOverflow::Clip,
            text_direction: TextDirection::Ltr,
            opacity: 1.0,
            box_shadow: BoxShadow::NONE,
            filter: VisualFilter::NONE,
            backdrop_filter: VisualFilter::NONE,
            icon_fill: None,
            icon_weight: None,
            icon_grade: None,
            icon_optical_size: None,
        }
    }

    fn cmd(content: DisplayPaintContent, kind: DisplayPaintCommandKind) -> DisplayPaintCommand {
        DisplayPaintCommand {
            node: DisplayPaintNode {
                id: 1,
                layout: LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 40.0,
                },
                style: base_style(),
                content,
                scrollbars: DisplayScrollbars::default(),
            },
            clip: DisplayListClip {
                x: 0,
                y: 0,
                width: 100,
                height: 40,
            },
            kind,
        }
    }

    #[test]
    fn anyrender_encodes_background_command() {
        let mut c = cmd(DisplayPaintContent::None, DisplayPaintCommandKind::Node);
        c.node.style.background_color = Color::WHITE;
        let mut diagnostics = Vec::new();
        assert_eq!(encode_command_to_scene(&c, &mut diagnostics), 1);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn anyrender_encodes_border_command() {
        let mut c = cmd(DisplayPaintContent::None, DisplayPaintCommandKind::Node);
        c.node.style.border_color = Color::BLACK;
        c.node.style.border_width = Edges {
            top: 2.0,
            right: 2.0,
            bottom: 2.0,
            left: 2.0,
        };
        let mut diagnostics = Vec::new();
        assert_eq!(encode_command_to_scene(&c, &mut diagnostics), 1);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn anyrender_encodes_icon_command() {
        let icon = DisplayIconPaint {
            src: None,
            name: Some("audio-volume-high".to_string()),
            size: Some(24),
        };
        let c = cmd(
            DisplayPaintContent::Icon(icon),
            DisplayPaintCommandKind::Node,
        );
        let mut diagnostics = Vec::new();
        assert!(encode_command_to_scene(&c, &mut diagnostics) >= 1);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn anyrender_skips_slider_input_with_documented_comment() {
        let slider = DisplaySliderPaint {
            min: 0.0,
            max: 100.0,
            value: 50.0,
            vertical: false,
        };
        let c = cmd(
            DisplayPaintContent::Slider(slider),
            DisplayPaintCommandKind::Node,
        );
        let mut diagnostics = Vec::new();
        assert_eq!(encode_command_to_scene(&c, &mut diagnostics), 0);
        assert!(diagnostics.is_empty());

        let input = DisplayInputPaint {
            value: String::new(),
            placeholder: String::new(),
            mask_text: false,
            focused: false,
        };
        let c = cmd(
            DisplayPaintContent::Input(input),
            DisplayPaintCommandKind::Node,
        );
        assert_eq!(encode_command_to_scene(&c, &mut diagnostics), 0);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn anyrender_skips_scrollbars_kind() {
        let c = cmd(
            DisplayPaintContent::None,
            DisplayPaintCommandKind::Scrollbars,
        );
        let mut diagnostics = Vec::new();
        assert_eq!(encode_command_to_scene(&c, &mut diagnostics), 0);
        assert!(diagnostics.is_empty());
    }

    #[cfg(not(feature = "renderer-parley"))]
    #[test]
    fn anyrender_text_without_parley_emits_diagnostic() {
        let text = DisplayTextPaint {
            text: "Hello".to_string(),
            selection: None,
        };
        let c = cmd(
            DisplayPaintContent::Text(text),
            DisplayPaintCommandKind::Node,
        );
        let mut diagnostics = Vec::new();
        assert_eq!(encode_command_to_scene(&c, &mut diagnostics), 0);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("combined parley+anyrender"));
    }
}
