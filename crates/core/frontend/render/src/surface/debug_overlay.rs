/// Debug overlay renderer.
///
/// Phase 16 moves the inspector panel into a shell-shipped `.mesh` surface.
/// The native overlay now only owns optional layout-bounds painting.
use super::buffer::PixelBuffer;
use super::painter::{ClipRect, FrontendRenderEngine};
use mesh_core_elements::style::Color;
use mesh_core_elements::tree::WidgetNode;

/// Allocation-free input for the native performance HUD. The shell copies
/// only the bounded scalar values needed by the renderer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebugPerfHudSnapshot {
    pub frame_times_micros: [u64; 16],
    pub frame_time_count: usize,
    pub redraw_count: u64,
    pub retained_generation: u64,
    pub dirty_nodes: u64,
    pub entries_rebuilt: u64,
    pub damage_rect_count: u64,
    pub damage_area: u64,
    pub surface_area: u64,
    pub full_surface_damage: bool,
}

#[derive(Debug, Default)]
pub struct DebugOverlayRestore {
    regions: Vec<DebugOverlayRestoreRegion>,
}

#[derive(Debug)]
struct DebugOverlayRestoreRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

impl DebugOverlayRestore {
    pub fn restore(mut self, buffer: &mut PixelBuffer) {
        for region in self.regions.drain(..).rev() {
            for row in 0..region.height {
                let destination = ((region.y + row) * buffer.stride + region.x * 4) as usize;
                let source = (row * region.width * 4) as usize;
                let len = (region.width * 4) as usize;
                if destination + len <= buffer.data.len() && source + len <= region.bytes.len() {
                    buffer.data[destination..destination + len]
                        .copy_from_slice(&region.bytes[source..source + len]);
                }
            }
        }
    }

    fn capture(&mut self, buffer: &PixelBuffer, rect: ClipRect) {
        let x = rect.x.max(0) as u32;
        let y = rect.y.max(0) as u32;
        let far_x = rect.x.saturating_add(rect.width).max(0) as u32;
        let far_y = rect.y.saturating_add(rect.height).max(0) as u32;
        let width = far_x.min(buffer.width).saturating_sub(x.min(buffer.width));
        let height = far_y
            .min(buffer.height)
            .saturating_sub(y.min(buffer.height));
        if width == 0 || height == 0 {
            return;
        }
        let mut bytes = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let start = ((y + row) * buffer.stride + x * 4) as usize;
            let len = (width * 4) as usize;
            if start + len > buffer.data.len() {
                return;
            }
            bytes.extend_from_slice(&buffer.data[start..start + len]);
        }
        self.regions.push(DebugOverlayRestoreRegion {
            x,
            y,
            width,
            height,
            bytes,
        });
    }
}

/// Mirrors `node_is_explicitly_hidden` in `painter/tree.rs` — the real
/// painter's definition of "not part of this surface's visible output".
/// Promoted `<popover>` wrappers are tagged `hidden="true"` and collapsed to
/// 0x0-with-overflow-visible so their (still full-size) subtree stays intact
/// for the dedicated child `xdg_popup` surface's own paint/bounds pass, while
/// the parent surface skips painting them. The bounds overlay must apply the
/// same skip, or it walks into that leftover full-size subtree and draws a
/// second, stale set of boxes at the collapsed in-flow position in the parent
/// surface — on top of the correct boxes the child surface already drew.
fn node_is_hidden_from_bounds(node: &WidgetNode) -> bool {
    use mesh_core_elements::style::{Display, Visibility};
    node.computed_style.display == Display::None
        || matches!(
            node.computed_style.visibility,
            Visibility::Hidden | Visibility::Collapse
        )
        || node.attributes.get("hidden").is_some_and(|value| {
            matches!(
                value.as_str(),
                "" | "true" | "1" | "hidden" | "disabled" | "checked"
            )
        })
}

// Layout bounds palette — depth 0..7
const BOUNDS_COLORS: [Color; 8] = [
    Color {
        r: 255,
        g: 80,
        b: 80,
        a: 180,
    },
    Color {
        r: 255,
        g: 160,
        b: 60,
        a: 180,
    },
    Color {
        r: 220,
        g: 220,
        b: 60,
        a: 180,
    },
    Color {
        r: 80,
        g: 220,
        b: 80,
        a: 180,
    },
    Color {
        r: 60,
        g: 200,
        b: 255,
        a: 180,
    },
    Color {
        r: 120,
        g: 100,
        b: 255,
        a: 180,
    },
    Color {
        r: 255,
        g: 80,
        b: 200,
        a: 180,
    },
    Color {
        r: 200,
        g: 200,
        b: 200,
        a: 180,
    },
];

pub struct DebugOverlay;

impl DebugOverlay {
    pub fn new() -> Self {
        Self
    }

    /// Draw coloured bounding-box outlines for every node in the widget tree.
    pub fn paint_layout_bounds(&self, root: &WidgetNode, buffer: &mut PixelBuffer, scale: f32) {
        let engine = FrontendRenderEngine::new();
        self.paint_layout_bounds_with_engine(&engine, root, buffer, scale);
    }

    pub(crate) fn paint_layout_bounds_with_engine(
        &self,
        engine: &FrontendRenderEngine,
        root: &WidgetNode,
        buffer: &mut PixelBuffer,
        scale: f32,
    ) {
        let bw = buffer.width as i32;
        let bh = buffer.height as i32;
        let full = ClipRect {
            x: 0,
            y: 0,
            width: bw,
            height: bh,
        };
        paint_bounds_recursive(engine, root, buffer, scale, full, 0, 0.0, 0.0);
    }

    /// Paint the active element-picker target using the familiar devtools
    /// translucent blue fill and a high-contrast outline.
    pub fn paint_element_highlight(
        &self,
        buffer: &mut PixelBuffer,
        scale: f32,
        bounds: (f32, f32, f32, f32),
    ) {
        let engine = FrontendRenderEngine::new();
        let (x, y, width, height) = bounds;
        let rect = ClipRect {
            x: (x * scale).round() as i32,
            y: (y * scale).round() as i32,
            width: (width * scale).round().max(0.0) as i32,
            height: (height * scale).round().max(0.0) as i32,
        };
        if rect.width <= 0 || rect.height <= 0 {
            return;
        }
        let full = ClipRect {
            x: 0,
            y: 0,
            width: buffer.width as i32,
            height: buffer.height as i32,
        };
        paint_bounds_rect(
            &engine,
            buffer,
            rect,
            Color {
                r: 66,
                g: 165,
                b: 245,
                a: 72,
            },
            full,
        );
        let outline = Color {
            r: 33,
            g: 150,
            b: 243,
            a: 255,
        };
        for edge in [
            ClipRect {
                width: rect.width,
                height: 2,
                ..rect
            },
            ClipRect {
                y: rect.y + rect.height - 2,
                width: rect.width,
                height: 2,
                ..rect
            },
            ClipRect {
                width: 2,
                height: rect.height,
                ..rect
            },
            ClipRect {
                x: rect.x + rect.width - 2,
                width: 2,
                height: rect.height,
                ..rect
            },
        ] {
            paint_bounds_rect(&engine, buffer, edge, outline, full);
        }
    }

    /// Paint a compact, native performance HUD and flash the component damage
    /// rectangles. The HUD deliberately uses a tiny bitmap font so observing a
    /// text-heavy workload does not perturb the text shaping/glyph caches being
    /// measured.
    pub fn paint_performance_hud(
        &self,
        buffer: &mut PixelBuffer,
        scale: f32,
        snapshot: &DebugPerfHudSnapshot,
        paint_damage: &[crate::DamageRect],
    ) -> DebugOverlayRestore {
        let engine = FrontendRenderEngine::new();
        let mut restore = DebugOverlayRestore::default();
        let full = ClipRect {
            x: 0,
            y: 0,
            width: buffer.width as i32,
            height: buffer.height as i32,
        };

        // Paint flashing is intentionally translucent and happens before the
        // opaque HUD card, so the counters remain legible over damaged content.
        for damage in paint_damage {
            let rect = ClipRect {
                x: (damage.x as f32 * scale).floor() as i32,
                y: (damage.y as f32 * scale).floor() as i32,
                width: (damage.width as f32 * scale).ceil().max(1.0) as i32,
                height: (damage.height as f32 * scale).ceil().max(1.0) as i32,
            };
            let thickness = scale.round().max(1.0) as i32;
            for edge in [
                ClipRect {
                    height: thickness,
                    ..rect
                },
                ClipRect {
                    y: rect.y + rect.height - thickness,
                    height: thickness,
                    ..rect
                },
                ClipRect {
                    width: thickness,
                    ..rect
                },
                ClipRect {
                    x: rect.x + rect.width - thickness,
                    width: thickness,
                    ..rect
                },
            ] {
                restore.capture(buffer, edge);
                paint_bounds_rect(
                    &engine,
                    buffer,
                    edge,
                    Color {
                        r: 255,
                        g: 42,
                        b: 128,
                        a: 220,
                    },
                    full,
                );
            }
        }

        let unit = scale.round().max(1.0) as i32;
        let panel_width = (184 * unit).min(full.width.max(0));
        let panel_height = (70 * unit).min(full.height.max(0));
        if panel_width <= 0 || panel_height <= 0 {
            return restore;
        }
        restore.capture(
            buffer,
            ClipRect {
                x: 0,
                y: 0,
                width: panel_width,
                height: panel_height,
            },
        );
        paint_bounds_rect(
            &engine,
            buffer,
            ClipRect {
                x: 0,
                y: 0,
                width: panel_width,
                height: panel_height,
            },
            Color {
                r: 12,
                g: 16,
                b: 24,
                a: 232,
            },
            full,
        );

        // One bar per recent total-surface sample. Green fits under 8.33ms,
        // amber under 16.67ms, and red identifies missed 60Hz frames.
        let history = &snapshot.frame_times_micros[..snapshot.frame_time_count.min(16)];
        let bar_width = 9 * unit;
        let graph_bottom = 30 * unit;
        for (index, duration) in history.iter().enumerate() {
            let height =
                ((*duration).min(33_334) * (24 * unit) as u64 / 33_334).max(unit as u64) as i32;
            let color = if *duration <= 8_333 {
                Color {
                    r: 72,
                    g: 214,
                    b: 125,
                    a: 255,
                }
            } else if *duration <= 16_667 {
                Color {
                    r: 255,
                    g: 190,
                    b: 64,
                    a: 255,
                }
            } else {
                Color {
                    r: 255,
                    g: 84,
                    b: 96,
                    a: 255,
                }
            };
            paint_bounds_rect(
                &engine,
                buffer,
                ClipRect {
                    x: (4 + index as i32 * 11) * unit,
                    y: graph_bottom - height,
                    width: bar_width,
                    height,
                },
                color,
                full,
            );
        }

        let last_frame = snapshot
            .frame_times_micros
            .get(snapshot.frame_time_count.saturating_sub(1).min(15))
            .copied()
            .unwrap_or_default();
        draw_hud_metric(
            &engine,
            buffer,
            full,
            4 * unit,
            36 * unit,
            &HUD_F,
            last_frame,
            unit,
        );
        draw_hud_metric(
            &engine,
            buffer,
            full,
            58 * unit,
            36 * unit,
            &HUD_R,
            snapshot.redraw_count,
            unit,
        );
        draw_hud_metric(
            &engine,
            buffer,
            full,
            112 * unit,
            36 * unit,
            &HUD_D,
            snapshot.dirty_nodes,
            unit,
        );
        draw_hud_metric(
            &engine,
            buffer,
            full,
            4 * unit,
            52 * unit,
            &HUD_E,
            snapshot.entries_rebuilt,
            unit,
        );
        draw_hud_metric(
            &engine,
            buffer,
            full,
            58 * unit,
            52 * unit,
            &HUD_X,
            snapshot.damage_rect_count,
            unit,
        );
        let damage_percent = if snapshot.surface_area == 0 {
            0
        } else {
            snapshot.damage_area.saturating_mul(100) / snapshot.surface_area
        };
        draw_hud_metric(
            &engine,
            buffer,
            full,
            112 * unit,
            52 * unit,
            &HUD_P,
            damage_percent,
            unit,
        );
        if snapshot.full_surface_damage {
            paint_bounds_rect(
                &engine,
                buffer,
                ClipRect {
                    x: 170 * unit,
                    y: 52 * unit,
                    width: 8 * unit,
                    height: 8 * unit,
                },
                Color {
                    r: 255,
                    g: 84,
                    b: 96,
                    a: 255,
                },
                full,
            );
        }
        restore
    }
}

fn draw_hud_metric(
    engine: &FrontendRenderEngine,
    buffer: &mut PixelBuffer,
    clip: ClipRect,
    x: i32,
    y: i32,
    label: &[u8; 5],
    value: u64,
    unit: i32,
) {
    draw_hud_bitmap(
        engine,
        buffer,
        clip,
        x,
        y,
        label,
        unit,
        Color {
            r: 96,
            g: 165,
            b: 250,
            a: 255,
        },
    );
    draw_hud_number(engine, buffer, clip, x + 5 * unit, y, value, unit);
}

fn draw_hud_bitmap(
    engine: &FrontendRenderEngine,
    buffer: &mut PixelBuffer,
    clip: ClipRect,
    x: i32,
    y: i32,
    rows: &[u8; 5],
    unit: i32,
    color: Color,
) {
    for (row, bits) in rows.iter().enumerate() {
        for column in 0..3 {
            if bits & (1 << (2 - column)) != 0 {
                paint_bounds_rect(
                    engine,
                    buffer,
                    ClipRect {
                        x: x + column * unit,
                        y: y + row as i32 * unit,
                        width: unit,
                        height: unit,
                    },
                    color,
                    clip,
                );
            }
        }
    }
}

fn draw_hud_number(
    engine: &FrontendRenderEngine,
    buffer: &mut PixelBuffer,
    clip: ClipRect,
    mut x: i32,
    y: i32,
    value: u64,
    unit: i32,
) {
    let digits = value.to_string();
    for digit in digits.bytes().take(8) {
        let Some(rows) = HUD_DIGITS.get((digit.saturating_sub(b'0')) as usize) else {
            continue;
        };
        for (row, bits) in rows.iter().enumerate() {
            for column in 0..3 {
                if bits & (1 << (2 - column)) == 0 {
                    continue;
                }
                paint_bounds_rect(
                    engine,
                    buffer,
                    ClipRect {
                        x: x + column * unit,
                        y: y + row as i32 * unit,
                        width: unit,
                        height: unit,
                    },
                    Color {
                        r: 226,
                        g: 232,
                        b: 240,
                        a: 255,
                    },
                    clip,
                );
            }
        }
        x += 4 * unit;
    }
}

const HUD_DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];
const HUD_F: [u8; 5] = [0b111, 0b100, 0b110, 0b100, 0b100];
const HUD_R: [u8; 5] = [0b110, 0b101, 0b110, 0b101, 0b101];
const HUD_D: [u8; 5] = [0b110, 0b101, 0b101, 0b101, 0b110];
const HUD_E: [u8; 5] = [0b111, 0b100, 0b110, 0b100, 0b111];
const HUD_X: [u8; 5] = [0b101, 0b101, 0b010, 0b101, 0b101];
const HUD_P: [u8; 5] = [0b110, 0b101, 0b110, 0b100, 0b100];

impl Default for DebugOverlay {
    fn default() -> Self {
        Self::new()
    }
}

fn paint_bounds_recursive(
    engine: &FrontendRenderEngine,
    node: &WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    _clip: ClipRect,
    depth: usize,
    offset_x: f32,
    offset_y: f32,
) {
    if node_is_hidden_from_bounds(node) {
        return;
    }

    // Mirror the real painter's offset accumulation (`render_node_with_filter`
    // in painter/tree.rs): a node's own CSS `transform.translate_*` shifts
    // where it (and its subtree) actually paints, so the debug box must
    // apply the same shift — otherwise it's drawn at the pre-transform layout
    // position, which reads as offset up-left of the visibly transformed
    // element (e.g. bubble/popover entrance-transform elements).
    let transform = node.computed_style.transform;
    let offset_x = offset_x + transform.translate_x;
    let offset_y = offset_y + transform.translate_y;

    let color = BOUNDS_COLORS[depth % BOUNDS_COLORS.len()];
    let x = ((node.layout.x + offset_x) * scale) as i32;
    let y = ((node.layout.y + offset_y) * scale) as i32;
    let w = (node.layout.width * scale) as i32;
    let h = (node.layout.height * scale) as i32;

    if w > 0 && h > 0 {
        let bw = buffer.width as i32;
        let bh = buffer.height as i32;
        let full = ClipRect {
            x: 0,
            y: 0,
            width: bw,
            height: bh,
        };
        paint_bounds_rect(
            engine,
            buffer,
            ClipRect {
                x,
                y,
                width: w,
                height: 1,
            },
            color,
            full,
        );
        paint_bounds_rect(
            engine,
            buffer,
            ClipRect {
                x,
                y: y + h - 1,
                width: w,
                height: 1,
            },
            color,
            full,
        );
        paint_bounds_rect(
            engine,
            buffer,
            ClipRect {
                x,
                y,
                width: 1,
                height: h,
            },
            color,
            full,
        );
        paint_bounds_rect(
            engine,
            buffer,
            ClipRect {
                x: x + w - 1,
                y,
                width: 1,
                height: h,
            },
            color,
            full,
        );
    }

    let scroll = node.resolved_scroll_metrics();
    let child_offset_x = offset_x - scroll.x;
    let child_offset_y = offset_y - scroll.y;
    for child in &node.children {
        paint_bounds_recursive(
            engine,
            child,
            buffer,
            scale,
            _clip,
            depth + 1,
            child_offset_x,
            child_offset_y,
        );
    }
}

fn paint_bounds_rect(
    engine: &FrontendRenderEngine,
    buffer: &mut PixelBuffer,
    rect: ClipRect,
    color: Color,
    clip: ClipRect,
) {
    engine.fill_rect_clipped(buffer, rect, color, clip);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DamageRect;

    #[test]
    fn performance_hud_paints_frame_bands_counters_and_damage_flash() {
        let mut buffer = PixelBuffer::new(240, 120);
        let snapshot = DebugPerfHudSnapshot {
            frame_times_micros: [4_000, 12_000, 20_000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            frame_time_count: 3,
            redraw_count: 42,
            retained_generation: 7,
            dirty_nodes: 3,
            entries_rebuilt: 2,
            damage_rect_count: 1,
            damage_area: 100,
            surface_area: 1_000,
            full_surface_damage: true,
        };
        let restore = DebugOverlay::new().paint_performance_hud(
            &mut buffer,
            1.0,
            &snapshot,
            &[DamageRect {
                x: 200,
                y: 90,
                width: 12,
                height: 10,
            }],
        );

        assert_ne!(buffer.get_pixel(2, 2), Color::TRANSPARENT);
        assert_ne!(buffer.get_pixel(200, 90), Color::TRANSPARENT);
        assert_eq!(buffer.get_pixel(220, 110), Color::TRANSPARENT);
        assert!(buffer.data.iter().any(|channel| *channel != 0));
        restore.restore(&mut buffer);
        assert!(buffer.data.iter().all(|channel| *channel == 0));
    }

    #[test]
    fn performance_hud_clips_safely_to_tiny_surface() {
        let mut buffer = PixelBuffer::new(8, 6);
        let restore = DebugOverlay::new().paint_performance_hud(
            &mut buffer,
            1.5,
            &DebugPerfHudSnapshot {
                frame_times_micros: [33_000; 16],
                frame_time_count: 16,
                ..DebugPerfHudSnapshot::default()
            },
            &[DamageRect {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            }],
        );
        assert_eq!(buffer.data.len(), 8 * 6 * 4);
        assert!(buffer.data.iter().any(|channel| *channel != 0));
        restore.restore(&mut buffer);
        assert!(buffer.data.iter().all(|channel| *channel == 0));
    }

    // cargo test -p mesh-core-render --release -- performance_hud_native_paint_cost --ignored --nocapture
    #[test]
    #[ignore]
    fn performance_hud_native_paint_cost() {
        use std::hint::black_box;
        use std::time::Instant;

        let overlay = DebugOverlay::new();
        let snapshot = DebugPerfHudSnapshot {
            frame_times_micros: std::array::from_fn(|index| (index as u64 + 1) * 1_100),
            frame_time_count: 16,
            redraw_count: 10_000,
            dirty_nodes: 4,
            entries_rebuilt: 2,
            damage_rect_count: 2,
            damage_area: 2_400,
            surface_area: 2_073_600,
            ..DebugPerfHudSnapshot::default()
        };
        let damage = [DamageRect {
            x: 500,
            y: 400,
            width: 80,
            height: 30,
        }];
        let baseline_started = Instant::now();
        for _ in 0..10_000 {
            let buffer = PixelBuffer::new(640, 80);
            black_box(buffer.get_pixel(0, 0));
        }
        let baseline = baseline_started.elapsed();
        let started = Instant::now();
        for _ in 0..10_000 {
            let mut buffer = PixelBuffer::new(640, 80);
            let restore = overlay.paint_performance_hud(&mut buffer, 1.0, &snapshot, &damage);
            restore.restore(&mut buffer);
            black_box(buffer.get_pixel(0, 0));
        }
        let observed = started.elapsed();
        let hud_delta = observed.saturating_sub(baseline);
        eprintln!(
            "10,000 640x80 buffers: {baseline:?}; with native HUD: {observed:?}; HUD delta: {:?}",
            hud_delta
        );
        eprintln!(
            "MESH_PERF metric=perf_hud_micros_per_frame value={:.6}",
            hud_delta.as_secs_f64() * 1_000_000.0 / 10_000.0
        );
    }
}
