/// Debug overlay renderer.
///
/// Paints over an already-rendered PixelBuffer: a right-side info panel
/// (plugins / interfaces / health) and optional coloured bounding boxes for
/// every node in the widget tree.
use super::buffer::PixelBuffer;
use super::painter::{ClipRect, fill_rect_clipped, fill_rounded_rect_clipped};
use super::text::TextRenderer;
use mesh_debug::{DebugSnapshot, DebugTab};
use mesh_ui::style::Color;
use mesh_ui::tree::WidgetNode;

const PANEL_WIDTH: i32 = 320;
const HEADER_H: i32 = 36;
const ROW_H: i32 = 22;
const PAD: i32 = 10;
const FONT: &str = "Inter";
const FONT_SM: f32 = 11.0;
const FONT_MD: f32 = 13.0;

// Palette
const BG: Color = Color {
    r: 18,
    g: 15,
    b: 24,
    a: 220,
};
const BORDER: Color = Color {
    r: 60,
    g: 50,
    b: 80,
    a: 255,
};
const TAB_ACTIVE_BG: Color = Color {
    r: 103,
    g: 80,
    b: 164,
    a: 255,
};
const TAB_INACTIVE_BG: Color = Color {
    r: 45,
    g: 38,
    b: 60,
    a: 255,
};
const TEXT_PRIMARY: Color = Color {
    r: 220,
    g: 210,
    b: 235,
    a: 255,
};
const TEXT_DIM: Color = Color {
    r: 140,
    g: 128,
    b: 160,
    a: 255,
};
const TEXT_ERROR: Color = Color {
    r: 240,
    g: 100,
    b: 100,
    a: 255,
};
const TEXT_OK: Color = Color {
    r: 100,
    g: 200,
    b: 130,
    a: 255,
};
const TEXT_WARN: Color = Color {
    r: 240,
    g: 190,
    b: 80,
    a: 255,
};

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

pub struct DebugOverlay {
    text: TextRenderer,
}

impl DebugOverlay {
    pub fn new() -> Self {
        Self {
            text: TextRenderer::new(),
        }
    }

    /// Paint the info panel over the right edge of `buffer`.
    pub fn paint_panel(
        &self,
        snapshot: &DebugSnapshot,
        active_tab: DebugTab,
        buffer: &mut PixelBuffer,
        scale: f32,
    ) {
        let bw = buffer.width as i32;
        let bh = buffer.height as i32;
        let pw = (PANEL_WIDTH as f32 * scale) as i32;
        let panel_x = bw - pw;

        let full = ClipRect {
            x: 0,
            y: 0,
            width: bw,
            height: bh,
        };
        let panel_clip = ClipRect {
            x: panel_x,
            y: 0,
            width: pw,
            height: bh,
        };

        // Panel background
        fill_rect_clipped(
            buffer,
            ClipRect {
                x: panel_x,
                y: 0,
                width: pw,
                height: bh,
            },
            BG,
            full,
        );

        // Left border
        fill_rect_clipped(
            buffer,
            ClipRect {
                x: panel_x,
                y: 0,
                width: 1,
                height: bh,
            },
            BORDER,
            full,
        );

        // Tab bar
        let tab_w = pw / 3;
        let tabs = [DebugTab::Plugins, DebugTab::Interfaces, DebugTab::Health];
        for (i, tab) in tabs.iter().enumerate() {
            let tx = panel_x + (i as i32) * tab_w;
            let tab_bg = if *tab == active_tab {
                TAB_ACTIVE_BG
            } else {
                TAB_INACTIVE_BG
            };
            fill_rect_clipped(
                buffer,
                ClipRect {
                    x: tx,
                    y: 0,
                    width: tab_w,
                    height: HEADER_H,
                },
                tab_bg,
                panel_clip,
            );
            self.draw_text(
                tab.label(),
                FONT_SM,
                400,
                TEXT_PRIMARY,
                buffer,
                (tx + PAD) as u32,
                ((HEADER_H / 2) - (FONT_SM as i32 / 2)).max(0) as u32,
                panel_clip,
            );
        }

        // Separator under tabs
        fill_rect_clipped(
            buffer,
            ClipRect {
                x: panel_x,
                y: HEADER_H,
                width: pw,
                height: 1,
            },
            BORDER,
            full,
        );

        let content_y = HEADER_H + 4;
        match active_tab {
            DebugTab::Plugins => self.paint_plugins(
                snapshot, buffer, panel_x, pw, content_y, bh, panel_clip, scale,
            ),
            DebugTab::Interfaces => self.paint_interfaces(
                snapshot, buffer, panel_x, pw, content_y, bh, panel_clip, scale,
            ),
            DebugTab::Health => self.paint_health(
                snapshot, buffer, panel_x, pw, content_y, bh, panel_clip, scale,
            ),
        }
    }

    fn paint_plugins(
        &self,
        snapshot: &DebugSnapshot,
        buffer: &mut PixelBuffer,
        panel_x: i32,
        pw: i32,
        start_y: i32,
        max_y: i32,
        clip: ClipRect,
        scale: f32,
    ) {
        let mut y = start_y;
        for entry in &snapshot.plugins {
            if y + ROW_H > max_y {
                break;
            }
            let row_clip = ClipRect {
                x: panel_x,
                y,
                width: pw,
                height: ROW_H,
            };
            let row_clip = intersect(row_clip, clip);

            // State colour dot
            let dot_color = state_color(&entry.state);
            fill_rounded_rect_clipped(
                buffer,
                ClipRect {
                    x: panel_x + PAD,
                    y: y + ROW_H / 2 - 4,
                    width: 8,
                    height: 8,
                },
                4.0,
                dot_color,
                row_clip,
            );

            // Plugin id
            let id_x = panel_x + PAD + 14;
            let max_label_w = (pw - PAD * 2 - 14 - 60) as f32 * scale;
            self.draw_text_clipped(
                &entry.id,
                FONT_MD,
                600,
                TEXT_PRIMARY,
                buffer,
                id_x as u32,
                y as u32,
                row_clip,
                Some(max_label_w),
            );

            // Type badge
            let badge_text = format!("{}  {}", entry.plugin_type, entry.state);
            self.draw_text(
                &badge_text,
                FONT_SM,
                400,
                TEXT_DIM,
                buffer,
                (panel_x + pw - PAD - 80).max(panel_x) as u32,
                (y + 2) as u32,
                row_clip,
            );

            // Error hint on second line
            if let Some(err) = &entry.last_error {
                let err_y = y + ROW_H;
                if err_y + ROW_H / 2 < max_y {
                    let err_clip = ClipRect {
                        x: panel_x,
                        y: err_y,
                        width: pw,
                        height: ROW_H / 2,
                    };
                    let err_clip = intersect(err_clip, clip);
                    let short: String = err.chars().take(48).collect();
                    self.draw_text(
                        &short,
                        FONT_SM,
                        400,
                        TEXT_ERROR,
                        buffer,
                        id_x as u32,
                        err_y as u32,
                        err_clip,
                    );
                    y += ROW_H / 2;
                }
            }

            y += ROW_H;

            // Divider
            if y < max_y {
                fill_rect_clipped(
                    buffer,
                    ClipRect {
                        x: panel_x + PAD,
                        y,
                        width: pw - PAD * 2,
                        height: 1,
                    },
                    BORDER,
                    clip,
                );
            }
        }

        if snapshot.plugins.is_empty() {
            self.draw_text(
                "No plugins loaded",
                FONT_MD,
                400,
                TEXT_DIM,
                buffer,
                (panel_x + PAD) as u32,
                start_y as u32,
                clip,
            );
        }
    }

    fn paint_interfaces(
        &self,
        snapshot: &DebugSnapshot,
        buffer: &mut PixelBuffer,
        panel_x: i32,
        pw: i32,
        start_y: i32,
        max_y: i32,
        clip: ClipRect,
        _scale: f32,
    ) {
        let mut y = start_y;
        for iface in &snapshot.interfaces {
            if y + ROW_H > max_y {
                break;
            }
            self.draw_text(
                &iface.name,
                FONT_MD,
                700,
                TEXT_PRIMARY,
                buffer,
                (panel_x + PAD) as u32,
                y as u32,
                clip,
            );
            y += ROW_H;

            for provider in &iface.providers {
                if y + ROW_H > max_y {
                    break;
                }
                let line = format!(
                    "  {} (priority {})",
                    provider.backend_name, provider.priority
                );
                self.draw_text(
                    &line,
                    FONT_SM,
                    400,
                    TEXT_DIM,
                    buffer,
                    (panel_x + PAD) as u32,
                    y as u32,
                    clip,
                );
                y += ROW_H - 4;
            }

            if iface.providers.is_empty() {
                self.draw_text(
                    "  (no providers)",
                    FONT_SM,
                    400,
                    TEXT_WARN,
                    buffer,
                    (panel_x + PAD) as u32,
                    y as u32,
                    clip,
                );
                y += ROW_H - 4;
            }

            if y < max_y {
                fill_rect_clipped(
                    buffer,
                    ClipRect {
                        x: panel_x + PAD,
                        y,
                        width: pw - PAD * 2,
                        height: 1,
                    },
                    BORDER,
                    clip,
                );
            }
            y += 2;
        }

        if snapshot.interfaces.is_empty() {
            self.draw_text(
                "No interfaces registered",
                FONT_MD,
                400,
                TEXT_DIM,
                buffer,
                (panel_x + PAD) as u32,
                start_y as u32,
                clip,
            );
        }
    }

    fn paint_health(
        &self,
        snapshot: &DebugSnapshot,
        buffer: &mut PixelBuffer,
        panel_x: i32,
        pw: i32,
        start_y: i32,
        max_y: i32,
        clip: ClipRect,
        _scale: f32,
    ) {
        let mut y = start_y;

        // Active surfaces section
        self.draw_text(
            "Surfaces",
            FONT_SM,
            700,
            TEXT_DIM,
            buffer,
            (panel_x + PAD) as u32,
            y as u32,
            clip,
        );
        y += ROW_H;
        for surface in &snapshot.active_surfaces {
            if y + ROW_H > max_y {
                break;
            }
            self.draw_text(
                surface,
                FONT_MD,
                400,
                TEXT_PRIMARY,
                buffer,
                (panel_x + PAD + 8) as u32,
                y as u32,
                clip,
            );
            y += ROW_H;
        }
        y += 4;

        fill_rect_clipped(
            buffer,
            ClipRect {
                x: panel_x + PAD,
                y,
                width: pw - PAD * 2,
                height: 1,
            },
            BORDER,
            clip,
        );
        y += 6;

        // Plugin health
        self.draw_text(
            "Plugin Health",
            FONT_SM,
            700,
            TEXT_DIM,
            buffer,
            (panel_x + PAD) as u32,
            y as u32,
            clip,
        );
        y += ROW_H;

        for entry in &snapshot.health {
            if y + ROW_H > max_y {
                break;
            }
            let color = health_color(&entry.status);
            self.draw_text(
                &entry.plugin_id,
                FONT_MD,
                400,
                TEXT_PRIMARY,
                buffer,
                (panel_x + PAD + 8) as u32,
                y as u32,
                clip,
            );
            self.draw_text(
                &entry.status,
                FONT_SM,
                400,
                color,
                buffer,
                (panel_x + pw / 2) as u32,
                y as u32,
                clip,
            );
            y += ROW_H;
        }

        if snapshot.health.is_empty() {
            self.draw_text(
                "No health data",
                FONT_MD,
                400,
                TEXT_DIM,
                buffer,
                (panel_x + PAD) as u32,
                y as u32,
                clip,
            );
        }
    }

    /// Draw coloured bounding-box outlines for every node in the widget tree.
    pub fn paint_layout_bounds(&self, root: &WidgetNode, buffer: &mut PixelBuffer, scale: f32) {
        let bw = buffer.width as i32;
        let bh = buffer.height as i32;
        let full = ClipRect {
            x: 0,
            y: 0,
            width: bw,
            height: bh,
        };
        paint_bounds_recursive(root, buffer, scale, full, 0);
    }

    fn draw_text(
        &self,
        text: &str,
        size: f32,
        weight: u16,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
        clip: ClipRect,
    ) {
        self.text.render_clipped(
            text,
            FONT,
            size,
            weight,
            1.3,
            mesh_ui::style::TextAlign::Left,
            color,
            buffer,
            x,
            y,
            (
                clip.x.max(0) as u32,
                clip.y.max(0) as u32,
                clip.width.max(0) as u32,
                clip.height.max(0) as u32,
            ),
            None,
        );
    }

    fn draw_text_clipped(
        &self,
        text: &str,
        size: f32,
        weight: u16,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
        clip: ClipRect,
        max_width: Option<f32>,
    ) {
        self.text.render_clipped(
            text,
            FONT,
            size,
            weight,
            1.3,
            mesh_ui::style::TextAlign::Left,
            color,
            buffer,
            x,
            y,
            (
                clip.x.max(0) as u32,
                clip.y.max(0) as u32,
                clip.width.max(0) as u32,
                clip.height.max(0) as u32,
            ),
            max_width,
        );
    }
}

impl Default for DebugOverlay {
    fn default() -> Self {
        Self::new()
    }
}

fn paint_bounds_recursive(
    node: &WidgetNode,
    buffer: &mut PixelBuffer,
    scale: f32,
    clip: ClipRect,
    depth: usize,
) {
    use mesh_ui::style::Display;
    if node.computed_style.display == Display::None {
        return;
    }

    let color = BOUNDS_COLORS[depth % BOUNDS_COLORS.len()];
    let x = (node.layout.x * scale) as i32;
    let y = (node.layout.y * scale) as i32;
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
        // Top edge
        fill_rect_clipped(
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
        // Bottom edge
        fill_rect_clipped(
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
        // Left edge
        fill_rect_clipped(
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
        // Right edge
        fill_rect_clipped(
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

    for child in &node.children {
        paint_bounds_recursive(child, buffer, scale, clip, depth + 1);
    }
}

fn intersect(a: ClipRect, b: ClipRect) -> ClipRect {
    let x1 = a.x.max(b.x);
    let y1 = a.y.max(b.y);
    let x2 = (a.x + a.width).min(b.x + b.width);
    let y2 = (a.y + a.height).min(b.y + b.height);
    ClipRect {
        x: x1,
        y: y1,
        width: (x2 - x1).max(0),
        height: (y2 - y1).max(0),
    }
}

fn state_color(state: &str) -> Color {
    match state {
        "running" => TEXT_OK,
        "errored" => TEXT_ERROR,
        "suspended" => TEXT_WARN,
        _ => TEXT_DIM,
    }
}

fn health_color(status: &str) -> Color {
    if status == "healthy" {
        TEXT_OK
    } else if status.starts_with("error") {
        TEXT_ERROR
    } else {
        TEXT_WARN
    }
}
