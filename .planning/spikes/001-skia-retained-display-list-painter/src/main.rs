use skia_safe::{
    Color, Color4f, EncodedImageFormat, Font, Paint, PaintStyle, PathBuilder, Point, RRect, Rect,
    surfaces,
};

#[derive(Debug, Clone)]
enum Command {
    FillRect {
        rect: Rect,
        color: Color,
    },
    FillRoundedRect {
        rect: Rect,
        radius: f32,
        color: Color,
    },
    StrokePath {
        points: Vec<Point>,
        width: f32,
        color: Color,
    },
    Text {
        origin: Point,
        text: String,
        size: f32,
        color: Color,
    },
}

fn main() {
    let width = 360;
    let height = 120;
    let mut surface = surfaces::raster_n32_premul((width, height)).expect("create raster surface");
    let canvas = surface.canvas();
    canvas.clear(Color::from_argb(255, 18, 20, 24));

    let commands = vec![
        Command::FillRoundedRect {
            rect: Rect::from_xywh(16.0, 16.0, 328.0, 88.0),
            radius: 10.0,
            color: Color::from_argb(255, 36, 43, 56),
        },
        Command::FillRect {
            rect: Rect::from_xywh(32.0, 34.0, 52.0, 52.0),
            color: Color::from_argb(255, 86, 168, 255),
        },
        Command::StrokePath {
            points: vec![
                Point::new(108.0, 78.0),
                Point::new(144.0, 42.0),
                Point::new(180.0, 70.0),
                Point::new(220.0, 36.0),
            ],
            width: 4.0,
            color: Color::from_argb(255, 151, 214, 141),
        },
        Command::Text {
            origin: Point::new(108.0, 94.0),
            text: "Skia retained painter spike".to_string(),
            size: 18.0,
            color: Color::WHITE,
        },
    ];

    for command in &commands {
        draw_command(canvas, command);
    }

    let image = surface.image_snapshot();
    let png = image
        .encode(None, EncodedImageFormat::PNG, None)
        .expect("encode png");
    let output_path =
        ".planning/spikes/001-skia-retained-display-list-painter/skia-spike-output.png";
    std::fs::write(output_path, png.as_bytes()).expect("write png");

    let pixmap = surface.peek_pixels().expect("peek pixels");
    let center = pixmap.get_color((width / 2, height / 2));
    println!(
        "rendered {} commands to {output_path}; center=#{:02x}{:02x}{:02x}{:02x}",
        commands.len(),
        center.a(),
        center.r(),
        center.g(),
        center.b()
    );
}

fn draw_command(canvas: &skia_safe::Canvas, command: &Command) {
    match command {
        Command::FillRect { rect, color } => {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(*color);
            canvas.draw_rect(*rect, &paint);
        }
        Command::FillRoundedRect {
            rect,
            radius,
            color,
        } => {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(*color);
            let rr = RRect::new_rect_xy(*rect, *radius, *radius);
            canvas.draw_rrect(rr, &paint);
        }
        Command::StrokePath {
            points,
            width,
            color,
        } => {
            if points.len() < 2 {
                return;
            }
            let mut path = PathBuilder::new();
            path.move_to(points[0]);
            for point in &points[1..] {
                path.line_to(*point);
            }
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color4f(Color4f::from(*color), None);
            paint.set_stroke_width(*width);
            paint.set_style(PaintStyle::Stroke);
            canvas.draw_path(&path.snapshot(), &paint);
        }
        Command::Text {
            origin,
            text,
            size,
            color,
        } => {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(*color);
            let mut font = Font::default();
            font.set_size(*size);
            canvas.draw_str(text, *origin, &font, &paint);
        }
    }
}
