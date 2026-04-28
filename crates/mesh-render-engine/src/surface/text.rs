//! Text measurement and rendering for the frontend render engine.

use super::PixelBuffer;
use cosmic_text::{
    Align, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style as CosmicStyle, SwashCache,
    Weight, Wrap,
};
use mesh_ui::Color;
use mesh_ui::style::TextAlign;
use std::cell::RefCell;
use std::sync::Mutex;

pub struct TextRenderer {
    engine: Mutex<TextEngine>,
}

struct TextEngine {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

thread_local! {
    static RENDERER: RefCell<TextRenderer> = RefCell::new(TextRenderer::new());
}

pub struct SharedTextMeasurer;

impl TextRenderer {
    pub fn new() -> Self {
        Self {
            engine: Mutex::new(TextEngine {
                font_system: FontSystem::new(),
                swash_cache: SwashCache::new(),
            }),
        }
    }

    pub fn measure(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        self.measure_styled(text, font_family, font_size, 400, 1.0, max_width)
    }

    pub fn render(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
    ) {
        let clip = (0, 0, buffer.width, buffer.height);
        self.render_clipped(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            TextAlign::Left,
            color,
            buffer,
            x,
            y,
            clip,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_clipped(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        align: TextAlign,
        color: Color,
        buffer: &mut PixelBuffer,
        x: u32,
        y: u32,
        clip: (u32, u32, u32, u32),
        max_width: Option<f32>,
    ) {
        let mut engine = self.engine.lock().unwrap();
        let (attrs, metrics, width, text_align) = text_config(
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            align,
        );

        let mut cosmic = Buffer::new(&mut engine.font_system, metrics);
        {
            let mut cosmic_borrow = cosmic.borrow_with(&mut engine.font_system);
            cosmic_borrow.set_wrap(wrap_for(max_width));
            cosmic_borrow.set_size(width, None);
            cosmic_borrow.set_text(text, &attrs, Shaping::Advanced, Some(text_align));
        }
        drop(engine);

        let mut engine = self.engine.lock().unwrap();

        let base_x = x as i32;
        let base_y = y as i32;
        let (clip_x, clip_y, clip_w, clip_h) = clip;
        let clip_right = clip_x.saturating_add(clip_w);
        let clip_bottom = clip_y.saturating_add(clip_h);

        {
            let TextEngine {
                font_system,
                swash_cache,
            } = &mut *engine;
            let mut cosmic_borrow = cosmic.borrow_with(font_system);
            cosmic_borrow.draw(
                swash_cache,
                cosmic_color(color),
                |glyph_x, glyph_y, glyph_w, glyph_h, glyph_color| {
                    let draw_x = base_x + glyph_x;
                    let draw_y = base_y + glyph_y;

                    let (r, g, b, a) = glyph_color.as_rgba_tuple();
                    let draw_color = Color { r, g, b, a };

                    for off_y in 0..glyph_h {
                        for off_x in 0..glyph_w {
                            let px = draw_x + off_x as i32;
                            let py = draw_y + off_y as i32;
                            if px < clip_x as i32
                                || py < clip_y as i32
                                || px >= clip_right as i32
                                || py >= clip_bottom as i32
                            {
                                continue;
                            }
                            buffer.blend_pixel(px as u32, py as u32, draw_color, 255);
                        }
                    }
                },
            );
        }
    }

    pub fn measure_styled(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        let mut engine = self.engine.lock().unwrap();
        let (attrs, metrics, width, _) = text_config(
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
            TextAlign::Left,
        );

        let mut cosmic = Buffer::new(&mut engine.font_system, metrics);
        let mut cosmic = cosmic.borrow_with(&mut engine.font_system);
        cosmic.set_wrap(wrap_for(max_width));
        cosmic.set_size(width, None);
        cosmic.set_text(text, &attrs, Shaping::Advanced, Some(Align::Left));

        let mut measured_width = 0.0f32;
        let mut measured_height = 0.0f32;
        for run in cosmic.layout_runs() {
            measured_width = measured_width.max(run.line_w);
            measured_height = measured_height.max(run.line_top + run.line_height);
        }

        if measured_height <= 0.0 {
            measured_height = metrics.line_height;
        }

        (measured_width, measured_height)
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl mesh_ui::TextMeasurer for TextRenderer {
    fn measure_text(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        self.measure_styled(
            text,
            font_family,
            font_size,
            font_weight,
            line_height,
            max_width,
        )
    }
}

impl mesh_ui::TextMeasurer for SharedTextMeasurer {
    fn measure_text(
        &self,
        text: &str,
        font_family: &str,
        font_size: f32,
        font_weight: u16,
        line_height: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        RENDERER.with(|renderer| {
            renderer.borrow().measure_styled(
                text,
                font_family,
                font_size,
                font_weight,
                line_height,
                max_width,
            )
        })
    }
}

fn text_config(
    font_family: &str,
    font_size: f32,
    font_weight: u16,
    line_height: f32,
    max_width: Option<f32>,
    align: TextAlign,
) -> (Attrs<'_>, Metrics, Option<f32>, Align) {
    let family = primary_family(font_family);
    let attrs = Attrs::new()
        .family(family)
        .style(CosmicStyle::Normal)
        .weight(Weight(font_weight.max(100)));
    let metrics = Metrics::new(
        font_size.max(1.0),
        (font_size * line_height.max(1.0)).max(1.0),
    );
    let width = max_width.filter(|value| *value > 0.0);
    let align = match align {
        TextAlign::Left => Align::Left,
        TextAlign::Center => Align::Center,
        TextAlign::Right => Align::Right,
    };
    (attrs, metrics, width, align)
}

fn primary_family(font_family: &str) -> Family<'_> {
    let family = font_family
        .split(',')
        .map(|part| part.trim().trim_matches('"').trim_matches('\''))
        .find(|part| !part.is_empty())
        .unwrap_or("sans-serif");

    match family.to_ascii_lowercase().as_str() {
        "serif" => Family::Serif,
        "sans-serif" | "sans" | "system-ui" => Family::SansSerif,
        "monospace" | "mono" => Family::Monospace,
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        _ => Family::Name(family),
    }
}

fn wrap_for(max_width: Option<f32>) -> Wrap {
    if max_width.is_some() {
        Wrap::Word
    } else {
        Wrap::None
    }
}

fn cosmic_color(color: Color) -> cosmic_text::Color {
    cosmic_text::Color::rgba(color.r, color.g, color.b, color.a)
}
