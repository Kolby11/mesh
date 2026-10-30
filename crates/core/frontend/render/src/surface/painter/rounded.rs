//! Clip-invariant rasterization of rounded fills, borders and clipping masks.
//!
//! Skia's anti-aliased rounded-rect coverage depends on the clip it is drawn
//! under: a partial repaint whose clip cuts an arc, or even one that only cuts
//! a straight edge far from the arc, produces different edge pixels than a
//! full paint (up to ~56/255). Partial repaints must reproduce full paints, so
//! each shape's coverage is computed once without any clip and cached:
//!
//! - fully covered pixels (body, integer bands, corner interiors) form one
//!   region drawn as a solid fill;
//! - partially covered corner pixels are packed into one A8 atlas drawn with
//!   integer sprite offsets. Full coverage stays out of the atlas: Skia rounds
//!   a tinted A8 blit differently from a solid fill, which would leave a
//!   1/255 seam around every corner;
//! - a straight edge row or column of partial coverage (a fractional border
//!   width or a phased edge) is a solid fill at scaled alpha.
//!
//! Each corner is rasterized from a symmetric shape twice the corner region's
//! size whose four corners all repeat it, so no clip ever cuts its arc. Shapes
//! whose corner regions overlap (pills, small circles) are rasterized whole.
//! A fractional device translation is part of the cached coverage (its
//! phase), so a shape scrolled by whole pixels keeps its pixels. Any other
//! canvas matrix, or an overlapping shape too large to rasterize whole,
//! returns `false` so the caller can draw it directly. Clipping masks instead record a
//! complete shape as a picture shader and keep its evaluation on the runtime
//! pipeline, independently of the destination damage rectangle.

use std::cell::RefCell;
use std::sync::Arc;

use mesh_core_elements::lru::ByteLruCache;
use mesh_core_elements::style::{Color, CornerShape, CornerShapes, Corners, Edges};
use skia_safe::{
    AlphaType, BlendMode, Canvas, ClipOp, ColorType, Data, FilterMode, IRect, ImageInfo, Matrix,
    MipmapMode, Paint, PaintStyle, Path, PathBuilder, PictureRecorder, Point, RRect, RSXform,
    Rect, Region, SamplingOptions, Shader, TileMode, Vector, images, region::RegionOp, surfaces,
};

use super::geometry::{ClipRect, intersect_clip};

const COVERAGE_CACHE_ENTRIES: usize = 512;
const COVERAGE_CACHE_BYTES: usize = 4 * 1024 * 1024;
const CLIP_CACHE_ENTRIES: usize = 128;
const CLIP_CACHE_BYTES: usize = 32 * 1024 * 1024;
/// Larger overlapping-corner shapes (big circles) are drawn directly rather
/// than rasterized whole on every cache miss.
const MAX_WHOLE_SHAPE_PIXELS: i64 = 256 * 256;

thread_local! {
    static COVERAGE: RefCell<ByteLruCache<ShapeKey, ShapeCoverage>> =
        RefCell::new(ByteLruCache::new(COVERAGE_CACHE_ENTRIES, COVERAGE_CACHE_BYTES));
    static CLIP_SHADERS: RefCell<ByteLruCache<ClipKey, Shader>> =
        RefCell::new(ByteLruCache::new(CLIP_CACHE_ENTRIES, CLIP_CACHE_BYTES));
    // Runtime evaluation keeps mask composition on Skia's high-precision
    // pipeline; the bitmap fast path rounds differently for narrow spans.
    static MASK_EFFECT: skia_safe::RuntimeEffect = skia_safe::RuntimeEffect::make_for_shader(
        "uniform shader mask; half4 main(float2 p) { return half4(mask.eval(p).a); }", None
    ).expect("valid rounded coverage shader");
}

/// Intersect with coverage rasterized independently of the damage clip.
/// Corners that `shapes` marks as scooped curve inward instead of outward.
pub(super) fn clip(canvas: &Canvas, rect: ClipRect, radii: Corners, shapes: CornerShapes) {
    let shape = RoundedShape {
        rect,
        outer: super::backend::normalize_corners(rect.width as f32, rect.height as f32, radii),
        ring: None,
    };
    let scooped = shapes.scoops(shape.outer).then(|| {
        scooped_path(
            Rect::from_wh(rect.width as f32, rect.height as f32),
            shape.outer,
            shapes,
        )
    });
    let key = ClipKey {
        shape: shape.key(Phase::ZERO),
        shapes: if scooped.is_some() {
            shapes
        } else {
            CornerShapes::default()
        },
    };
    let shader = CLIP_SHADERS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(shader) = cache.get(&key) {
            return Some(shader.clone());
        }
        if rect.width <= 0 || rect.height <= 0 {
            return None;
        }
        let bounds = Rect::from_wh(rect.width as f32, rect.height as f32);
        let mut recorder = PictureRecorder::new();
        let recording = recorder.begin_recording(bounds, false);
        let mut paint = solid_paint(Color::WHITE, 1.0);
        paint.set_anti_alias(true);
        // A picture shader rasterizes its complete intrinsic bounds, so the
        // destination damage never cuts the shape during coverage generation.
        match &scooped {
            Some(path) => {
                recording.draw_path(path, &paint);
            }
            None => draw_shape(recording, bounds, shape.outer, None, &paint),
        }
        let picture = recorder.finish_recording_as_picture(None)?;
        let shader = picture.to_shader(
            (TileMode::Decal, TileMode::Decal),
            FilterMode::Nearest,
            None,
            &bounds,
        );
        let shader = MASK_EFFECT.with(|effect| {
            effect.make_shader(
                Data::new_empty(),
                &[skia_safe::runtime_effect::ChildPtr::Shader(shader)],
                None,
            )
        })?;
        // Picture shaders may retain a full RGBA raster, in addition to the
        // recorded shape.
        let bytes = (rect.width as usize)
            .saturating_mul(rect.height as usize)
            .saturating_mul(4)
            .saturating_add(picture.approximate_bytes_used());
        cache.insert(key, shader.clone(), bytes.max(1));
        Some(shader)
    });
    if let Some(shader) = shader {
        let matrix = Matrix::translate((rect.x as f32, rect.y as f32));
        canvas.clip_shader(shader.with_local_matrix(&matrix), None);
    } else if let Some(path) = scooped {
        canvas.clip_path(
            &path.make_offset((rect.x as f32, rect.y as f32)),
            None,
            true,
        );
    } else {
        canvas.clip_rrect(
            rrect(
                Rect::from_xywh(
                    rect.x as f32,
                    rect.y as f32,
                    rect.width as f32,
                    rect.height as f32,
                ),
                shape.outer,
            ),
            None,
            true,
        );
    }
}

/// Remove the scooped corners' quarter discs from everything drawn after it,
/// leaving the rest of the canvas, including outside `rect`, unclipped.
/// Outer shadows of a scooped box use this so they do not fill the scoops.
pub(super) fn clip_out_scoops(canvas: &Canvas, rect: ClipRect, radii: Corners, shapes: CornerShapes) {
    let radii = super::backend::normalize_corners(rect.width as f32, rect.height as f32, radii);
    let bounds = Rect::from_xywh(
        rect.x as f32,
        rect.y as f32,
        rect.width as f32,
        rect.height as f32,
    );
    let corners = [
        (shapes.top_left, radii.top_left, (bounds.left, bounds.top)),
        (shapes.top_right, radii.top_right, (bounds.right, bounds.top)),
        (shapes.bottom_right, radii.bottom_right, (bounds.right, bounds.bottom)),
        (shapes.bottom_left, radii.bottom_left, (bounds.left, bounds.bottom)),
    ];
    for (shape, radius, (x, y)) in corners {
        if shape == CornerShape::Scoop && radius > 0.0 {
            let disc = Rect::from_ltrb(x - radius, y - radius, x + radius, y + radius);
            canvas.clip_rrect(RRect::new_oval(disc), ClipOp::Difference, true);
        }
    }
}

/// The outline of `rect` with normalized `radii`, each corner curving outward
/// (`round`) or inward (`scoop`). A scoop's quarter arc is centred on the
/// corner point; a round one on the point `radius` inside it.
pub(super) fn scooped_path(rect: Rect, radii: Corners, shapes: CornerShapes) -> Path {
    let (l, t, r, b) = (rect.left, rect.top, rect.right, rect.bottom);
    let mut builder = PathBuilder::new();
    builder.move_to((l + radii.top_left, t));
    let corners = [
        ((r, t), radii.top_right, shapes.top_right, (-1.0, 0.0), (0.0, 1.0)),
        ((r, b), radii.bottom_right, shapes.bottom_right, (0.0, -1.0), (-1.0, 0.0)),
        ((l, b), radii.bottom_left, shapes.bottom_left, (1.0, 0.0), (0.0, -1.0)),
        ((l, t), radii.top_left, shapes.top_left, (0.0, 1.0), (1.0, 0.0)),
    ];
    for ((cx, cy), radius, shape, into, out) in corners {
        let start = Point::new(cx + into.0 * radius, cy + into.1 * radius);
        builder.line_to(start);
        if radius > 0.0 {
            let end = Point::new(cx + out.0 * radius, cy + out.1 * radius);
            let control = match shape {
                CornerShape::Round => Point::new(cx, cy),
                CornerShape::Scoop => Point::new(start.x + end.x - cx, start.y + end.y - cy),
            };
            builder.conic_to(control, end, std::f32::consts::FRAC_1_SQRT_2);
        }
    }
    builder.close();
    builder.detach()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ClipKey {
    shape: ShapeKey,
    shapes: CornerShapes,
}

/// A rounded rectangle at integer local bounds, optionally hollowed into a
/// border ring.
#[derive(Debug, Clone, Copy)]
pub(super) struct RoundedShape {
    pub rect: ClipRect,
    /// Outer radii, already normalized to fit `rect`.
    pub outer: Corners,
    pub ring: Option<Ring>,
}

/// The hollow of a border ring: side widths and inner radii normalized to the
/// inner rectangle. The inner rectangle must be non-empty.
#[derive(Debug, Clone, Copy)]
pub(super) struct Ring {
    pub widths: Edges,
    pub inner: Corners,
}

/// The fractional part of a shape's device origin. Content scrolled to a
/// fractional offset draws every shape at a phase; coverage computed for it
/// is identical at every whole-pixel translation, so pixels shifted in place
/// match a fresh paint. A shape with a phase covers one more device row or
/// column than its size.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Phase {
    x: f32,
    y: f32,
}

impl Phase {
    const ZERO: Self = Self { x: 0.0, y: 0.0 };

    fn extra(self) -> (i32, i32) {
        (i32::from(self.x > 0.0), i32::from(self.y > 0.0))
    }
}

/// Coverage of one shape relative to its grid origin.
#[derive(Clone)]
struct ShapeCoverage {
    full: Region,
    /// Partially covered pixels of every masked part, stacked vertically.
    atlas: Option<skia_safe::Image>,
    /// `(source rect in the atlas, destination offset)` per masked part.
    sprites: Arc<[(Rect, (f32, f32))]>,
    /// Rows or columns of one partial coverage along straight edges: phased
    /// edges and fractional border widths.
    bands: Arc<[(IRect, f32)]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ShapeKey {
    width: i32,
    height: i32,
    outer: [u32; 4],
    widths: Option<[u32; 4]>,
    inner: [u32; 4],
    phase: [u32; 2],
}

/// Corner order shared by every per-corner array in this module.
const TL: usize = 0;
const TR: usize = 1;
const BR: usize = 2;
const BL: usize = 3;

fn corners_array(corners: Corners) -> [f32; 4] {
    [
        corners.top_left,
        corners.top_right,
        corners.bottom_right,
        corners.bottom_left,
    ]
}

fn bits(values: [f32; 4]) -> [u32; 4] {
    values.map(f32::to_bits)
}

impl RoundedShape {
    fn widths(&self) -> Option<Edges> {
        self.ring.map(|ring| ring.widths)
    }

    /// Horizontal and vertical side widths meeting at `corner`; zero for a
    /// fill.
    fn corner_widths(&self, corner: usize) -> (f32, f32) {
        let Some(widths) = self.widths() else {
            return (0.0, 0.0);
        };
        match corner {
            TL => (widths.left, widths.top),
            TR => (widths.right, widths.top),
            BR => (widths.right, widths.bottom),
            _ => (widths.left, widths.bottom),
        }
    }

    /// The device pixels the shape touches at `phase`.
    fn grid(&self, phase: Phase) -> (i32, i32) {
        let (extra_x, extra_y) = phase.extra();
        (self.rect.width + extra_x, self.rect.height + extra_y)
    }

    /// Size of each corner's region: it covers the arc and, for a ring, the
    /// square where the two sides meet, so the straight parts left over are
    /// plain bands.
    fn corner_regions(&self, phase: Phase) -> [(i32, i32); 4] {
        let outer = corners_array(self.outer);
        let (extra_x, extra_y) = phase.extra();
        std::array::from_fn(|corner| {
            let arc = outer[corner].ceil() as i32;
            let (side_x, side_y) = self.corner_widths(corner);
            (
                arc.max(side_x.ceil() as i32) + extra_x,
                arc.max(side_y.ceil() as i32) + extra_y,
            )
        })
    }

    /// Origin of `corner`'s region relative to the grid origin.
    fn corner_origin(
        corner: usize,
        (width, height): (i32, i32),
        (grid_width, grid_height): (i32, i32),
    ) -> (i32, i32) {
        let x = match corner {
            TL | BL => 0,
            _ => grid_width - width,
        };
        let y = match corner {
            TL | TR => 0,
            _ => grid_height - height,
        };
        (x, y)
    }

    fn overlapping(regions: &[(i32, i32); 4], (width, height): (i32, i32)) -> bool {
        regions[TL].0 + regions[TR].0 > width
            || regions[BL].0 + regions[BR].0 > width
            || regions[TL].1 + regions[BL].1 > height
            || regions[TR].1 + regions[BR].1 > height
    }

    fn key(&self, phase: Phase) -> ShapeKey {
        ShapeKey {
            width: self.rect.width,
            height: self.rect.height,
            outer: bits(corners_array(self.outer)),
            widths: self
                .widths()
                .map(|widths| bits([widths.top, widths.right, widths.bottom, widths.left])),
            inner: bits(self.ring.map_or([0.0; 4], |ring| corners_array(ring.inner))),
            phase: [phase.x.to_bits(), phase.y.to_bits()],
        }
    }
}

/// Draw `shape` in `color` under `clip` with clip-invariant pixels. Returns
/// `false`, having drawn nothing, when the shape must be drawn directly.
pub(super) fn draw(canvas: &Canvas, shape: &RoundedShape, color: Color, clip: ClipRect) -> bool {
    let matrix = canvas.local_to_device_as_3x3();
    if !matrix.is_translate() {
        return false;
    }
    let phase = Phase {
        x: matrix.translate_x() - matrix.translate_x().floor(),
        y: matrix.translate_y() - matrix.translate_y().floor(),
    };
    let rect = shape.rect;
    let clipped = intersect_clip(rect, clip);
    if clipped.width <= 0 || clipped.height <= 0 || color.a == 0 {
        return true;
    }
    let grid = shape.grid(phase);
    let regions = shape.corner_regions(phase);
    let overlapping = RoundedShape::overlapping(&regions, grid);
    if overlapping && i64::from(grid.0) * i64::from(grid.1) > MAX_WHOLE_SHAPE_PIXELS {
        return false;
    }
    let Some(coverage) = coverage(shape, phase, &regions, overlapping) else {
        return false;
    };

    let paint = solid_paint(color, 1.0);
    let save_count = canvas.save();
    // The coverage already holds the phase; draw it on whole device pixels.
    // `floor` makes the subtraction exact, so the translation is integral.
    canvas.translate((-phase.x, -phase.y));
    canvas.clip_rect(
        Rect::from_xywh(
            clipped.x as f32 + phase.x,
            clipped.y as f32 + phase.y,
            clipped.width as f32,
            clipped.height as f32,
        ),
        None,
        false,
    );
    let mut full = coverage.full.clone();
    full.translate((rect.x, rect.y));
    canvas.draw_region(&full, &paint);
    if let Some(atlas) = &coverage.atlas {
        crate::prepared_scene::retain_image(atlas);
        let transforms: Vec<_> = coverage
            .sprites
            .iter()
            .map(|&(_, (x, y))| RSXform::new(1.0, 0.0, (rect.x as f32 + x, rect.y as f32 + y)))
            .collect();
        let sources: Vec<_> = coverage.sprites.iter().map(|&(source, _)| source).collect();
        canvas.draw_atlas(
            atlas,
            &transforms,
            &sources,
            None,
            BlendMode::Modulate,
            SamplingOptions::new(FilterMode::Nearest, MipmapMode::None),
            None,
            &paint,
        );
    }
    for &(band, band_coverage) in coverage.bands.iter() {
        canvas.draw_irect(
            band.with_offset((rect.x, rect.y)),
            &solid_paint(color, band_coverage),
        );
    }
    canvas.restore_to_count(save_count);
    true
}

/// Add the straight run `start..end` of the interval `from..to` across it,
/// one row (`horizontal`) or column per device pixel it touches: whole pixels
/// to the full region, partial ones as bands.
fn add_straight(
    builder: &mut CoverageBuilder,
    (start, end): (i32, i32),
    (from, to): (f32, f32),
    horizontal: bool,
) {
    if end <= start || to <= from {
        return;
    }
    for pixel in from.floor() as i32..to.ceil() as i32 {
        let covered = to.min(pixel as f32 + 1.0) - from.max(pixel as f32);
        let line = if horizontal {
            IRect::from_xywh(start, pixel, end - start, 1)
        } else {
            IRect::from_xywh(pixel, start, 1, end - start)
        };
        if covered >= 1.0 {
            builder.full.push(line);
        } else if covered > 0.0 {
            builder.bands.push((line, covered));
        }
    }
}

fn solid_paint(color: Color, coverage: f32) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(false);
    paint.set_style(PaintStyle::Fill);
    paint.set_color(crate::surface::buffer::skia_color(color));
    if coverage < 1.0 {
        paint.set_alpha_f(f32::from(color.a) / 255.0 * coverage);
    }
    paint
}

fn coverage(
    shape: &RoundedShape,
    phase: Phase,
    regions: &[(i32, i32); 4],
    overlapping: bool,
) -> Option<ShapeCoverage> {
    let key = shape.key(phase);
    COVERAGE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(coverage) = cache.get(&key) {
            return Some(coverage.clone());
        }
        let (coverage, bytes) = if overlapping {
            rasterize_whole(shape, phase)?
        } else {
            rasterize_parts(shape, phase, regions)?
        };
        cache.insert(key, coverage.clone(), bytes.max(1));
        Some(coverage)
    })
}

/// Collects full-coverage runs into a region and partial pixels into stacked
/// atlas rows.
#[derive(Default)]
struct CoverageBuilder {
    full: Vec<IRect>,
    atlas_rows: Vec<Vec<u8>>,
    atlas_width: i32,
    sprites: Vec<(Rect, (f32, f32))>,
    bands: Vec<(IRect, f32)>,
}

impl CoverageBuilder {
    /// Add the `region` of a rasterized `pixels` buffer `stride` wide, placed
    /// at `origin` in the shape.
    fn add(&mut self, pixels: &[u8], stride: i32, region: IRect, origin: (i32, i32)) {
        let mut rows = Vec::with_capacity(region.height() as usize);
        let mut partial = false;
        for row in region.top..region.bottom {
            let start = (row * stride + region.left) as usize;
            let line = &pixels[start..start + region.width() as usize];
            let y = origin.1 + row - region.top;
            let mut run_start = None;
            for (column, &coverage) in line.iter().enumerate() {
                let column = column as i32;
                if coverage == 255 {
                    run_start.get_or_insert(column);
                } else if let Some(run) = run_start.take() {
                    self.full.push(IRect::from_ltrb(
                        origin.0 + run,
                        y,
                        origin.0 + column,
                        y + 1,
                    ));
                }
                partial |= coverage != 0 && coverage != 255;
            }
            if let Some(run) = run_start {
                self.full.push(IRect::from_ltrb(
                    origin.0 + run,
                    y,
                    origin.0 + region.width(),
                    y + 1,
                ));
            }
            rows.push(
                line.iter()
                    .map(|&coverage| if coverage == 255 { 0 } else { coverage })
                    .collect(),
            );
        }
        if !partial {
            return;
        }
        let top = self.atlas_rows.len() as f32;
        self.sprites.push((
            Rect::from_xywh(0.0, top, region.width() as f32, region.height() as f32),
            (origin.0 as f32, origin.1 as f32),
        ));
        self.atlas_width = self.atlas_width.max(region.width());
        self.atlas_rows.extend(rows);
    }

    fn finish(self) -> Option<(ShapeCoverage, usize)> {
        let mut full = Region::new();
        full.set_rects(&self.full);
        let atlas_height = self.atlas_rows.len() as i32;
        let atlas = if self.sprites.is_empty() {
            None
        } else {
            let width = self.atlas_width as usize;
            let mut bytes = vec![0u8; width * atlas_height as usize];
            for (index, row) in self.atlas_rows.iter().enumerate() {
                bytes[index * width..index * width + row.len()].copy_from_slice(row);
            }
            let info = ImageInfo::new(
                (self.atlas_width, atlas_height),
                ColorType::Alpha8,
                AlphaType::Premul,
                None,
            );
            Some(images::raster_from_data(
                &info,
                Data::new_copy(&bytes),
                width,
            )?)
        };
        let bytes = (self.atlas_width * atlas_height) as usize
            + self.full.len() * 16
            + self.bands.len() * 20;
        Some((
            ShapeCoverage {
                full,
                atlas,
                sprites: self.sprites.into(),
                bands: self.bands.into(),
            },
            bytes,
        ))
    }
}

fn rasterize_whole(shape: &RoundedShape, phase: Phase) -> Option<(ShapeCoverage, usize)> {
    let (width, height) = shape.grid(phase);
    let outer_rect = Rect::from_xywh(
        phase.x,
        phase.y,
        shape.rect.width as f32,
        shape.rect.height as f32,
    );
    let pixels = rasterize(width, height, |canvas, paint| {
        draw_shape(canvas, outer_rect, shape.outer, shape.ring, paint);
    })?;
    let mut builder = CoverageBuilder::default();
    builder.add(&pixels, width, IRect::from_wh(width, height), (0, 0));
    builder.finish()
}

fn rasterize_parts(
    shape: &RoundedShape,
    phase: Phase,
    regions: &[(i32, i32); 4],
) -> Option<(ShapeCoverage, usize)> {
    let grid = shape.grid(phase);
    let mut builder = CoverageBuilder::default();
    for (corner, &size) in regions.iter().enumerate() {
        if size.0 <= 0 || size.1 <= 0 {
            continue;
        }
        let (pixels, region) = rasterize_corner(shape, phase, corner, size)?;
        builder.add(
            &pixels,
            size.0 * 2,
            region,
            RoundedShape::corner_origin(corner, size, grid),
        );
    }
    let (width, height) = (shape.rect.width as f32, shape.rect.height as f32);
    let (grid_width, grid_height) = grid;
    let top = (regions[TL].0, grid_width - regions[TR].0);
    let bottom = (regions[BL].0, grid_width - regions[BR].0);
    let left = (regions[TL].1, grid_height - regions[BL].1);
    let right = (regions[TR].1, grid_height - regions[BR].1);
    match shape.widths() {
        None => {
            // Phased outer edges are partial rows and columns; whatever the
            // corners and those leave is fully covered.
            let mut body = Region::from_rect(IRect::from_wh(grid_width, grid_height));
            for (corner, &size) in regions.iter().enumerate() {
                let (x, y) = RoundedShape::corner_origin(corner, size, grid);
                body.op_rect(IRect::from_xywh(x, y, size.0, size.1), RegionOp::Difference);
            }
            if phase.y > 0.0 {
                add_straight(&mut builder, top, (phase.y, 1.0), true);
                add_straight(&mut builder, bottom, (height, phase.y + height), true);
                body.op_rect(IRect::from_xywh(0, 0, grid_width, 1), RegionOp::Difference);
                body.op_rect(
                    IRect::from_xywh(0, grid_height - 1, grid_width, 1),
                    RegionOp::Difference,
                );
            }
            if phase.x > 0.0 {
                add_straight(&mut builder, left, (phase.x, 1.0), false);
                add_straight(&mut builder, right, (width, phase.x + width), false);
                body.op_rect(IRect::from_xywh(0, 0, 1, grid_height), RegionOp::Difference);
                body.op_rect(
                    IRect::from_xywh(grid_width - 1, 0, 1, grid_height),
                    RegionOp::Difference,
                );
            }
            builder.full.extend(skia_safe::region::Iterator::new(&body));
        }
        Some(widths) => {
            let (x, y) = (phase.x, phase.y);
            add_straight(&mut builder, top, (y, y + widths.top), true);
            add_straight(
                &mut builder,
                bottom,
                (y + height - widths.bottom, y + height),
                true,
            );
            add_straight(&mut builder, left, (x, x + widths.left), false);
            add_straight(
                &mut builder,
                right,
                (x + width - widths.right, x + width),
                false,
            );
        }
    }
    builder.finish()
}

/// Rasterize a corner from a symmetric shape about twice the region's size
/// whose four corners all repeat this one, so the quadrant holds exactly this
/// corner's local geometry at `phase`. Returns the pixels and the quadrant.
fn rasterize_corner(
    shape: &RoundedShape,
    phase: Phase,
    corner: usize,
    (width, height): (i32, i32),
) -> Option<(Vec<u8>, IRect)> {
    let (mini_width, mini_height) = (width * 2, height * 2);
    let (extra_x, extra_y) = phase.extra();
    // The shape's far edge sits where the real shape's does relative to its
    // corner region, which ends one pixel past a phased shape.
    let outer_rect = Rect::from_xywh(
        phase.x,
        phase.y,
        (mini_width - extra_x) as f32,
        (mini_height - extra_y) as f32,
    );
    let radius = corners_array(shape.outer)[corner];
    let ring = shape.ring.map(|ring| {
        let (side_x, side_y) = shape.corner_widths(corner);
        Ring {
            widths: Edges {
                top: side_y,
                right: side_x,
                bottom: side_y,
                left: side_x,
            },
            inner: Corners::all(corners_array(ring.inner)[corner]),
        }
    });
    let pixels = rasterize(mini_width, mini_height, |canvas, paint| {
        draw_shape(canvas, outer_rect, Corners::all(radius), ring, paint);
    })?;
    let x = if matches!(corner, TL | BL) { 0 } else { width };
    let y = if matches!(corner, TL | TR) { 0 } else { height };
    Some((pixels, IRect::from_xywh(x, y, width, height)))
}

fn draw_shape(
    canvas: &Canvas,
    outer_rect: Rect,
    outer: Corners,
    ring: Option<Ring>,
    paint: &Paint,
) {
    let outer_rrect = rrect(outer_rect, outer);
    let inner = ring.and_then(|ring| {
        let widths = ring.widths;
        let inner_rect = Rect::from_ltrb(
            outer_rect.left + widths.left,
            outer_rect.top + widths.top,
            outer_rect.right - widths.right,
            outer_rect.bottom - widths.bottom,
        );
        (inner_rect.width() > 0.0 && inner_rect.height() > 0.0)
            .then(|| rrect(inner_rect, ring.inner))
    });
    match inner {
        Some(inner) => canvas.draw_drrect(outer_rrect, inner, paint),
        None => canvas.draw_rrect(outer_rrect, paint),
    };
}

fn rrect(rect: Rect, corners: Corners) -> RRect {
    let radius = |value: f32| Vector::new(value, value);
    RRect::new_rect_radii(
        rect,
        &[
            radius(corners.top_left),
            radius(corners.top_right),
            radius(corners.bottom_right),
            radius(corners.bottom_left),
        ],
    )
}

fn rasterize(width: i32, height: i32, draw: impl FnOnce(&Canvas, &Paint)) -> Option<Vec<u8>> {
    let info = ImageInfo::new((width, height), ColorType::Alpha8, AlphaType::Premul, None);
    let mut surface = surfaces::raster(&info, None, None)?;
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    draw(surface.canvas(), &paint);
    let mut pixels = vec![0u8; (width * height) as usize];
    surface
        .read_pixels(&info, &mut pixels, width as usize, (0, 0))
        .then_some(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: i32 = 64;

    /// Paint `shape` with the canvas translated by `(0, offset)` under
    /// `clip`, returning the RGBA pixels.
    fn paint(shape: &RoundedShape, offset: f32, clip: ClipRect) -> Vec<u8> {
        let info = ImageInfo::new((SIZE, SIZE), ColorType::RGBA8888, AlphaType::Premul, None);
        let mut surface = surfaces::raster(&info, None, None).unwrap();
        let canvas = surface.canvas();
        canvas.translate((0.0, offset));
        assert!(draw(
            canvas,
            shape,
            Color {
                r: 224,
                g: 175,
                b: 104,
                a: 41
            },
            clip,
        ));
        let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
        assert!(surface.read_pixels(&info, &mut pixels, SIZE as usize * 4, (0, 0)));
        pixels
    }

    fn row(pixels: &[u8], y: i32) -> &[u8] {
        let stride = SIZE as usize * 4;
        &pixels[y as usize * stride..(y as usize + 1) * stride]
    }

    fn shapes() -> [RoundedShape; 3] {
        let card = ClipRect {
            x: 4,
            y: 2,
            width: 50,
            height: 30,
        };
        let pill = ClipRect {
            x: 9,
            y: 6,
            width: 43,
            height: 19,
        };
        [
            RoundedShape {
                rect: card,
                outer: Corners::all(8.0),
                ring: None,
            },
            RoundedShape {
                rect: pill,
                outer: Corners::all(9.5),
                ring: None,
            },
            RoundedShape {
                rect: card,
                outer: Corners::all(8.0),
                ring: Some(Ring {
                    widths: Edges::all(1.5),
                    inner: Corners::all(6.5),
                }),
            },
        ]
    }

    #[test]
    fn phased_shapes_keep_their_pixels_across_whole_pixel_shifts_and_clips() {
        let everything = ClipRect {
            x: -SIZE,
            y: -SIZE,
            width: SIZE * 3,
            height: SIZE * 3,
        };
        for shape in shapes() {
            let near = paint(&shape, 3.5, everything);
            let far = paint(&shape, 7.5, everything);
            for y in 0..SIZE - 4 {
                assert_eq!(row(&near, y), row(&far, y + 4), "row {y} of {shape:?}");
            }
            // A clip cutting through the arc keeps the pixels it leaves.
            let clip = ClipRect {
                x: 0,
                y: shape.rect.y + shape.rect.height - 5,
                width: shape.rect.x + 6,
                height: 4,
            };
            let clipped = paint(&shape, 3.5, clip);
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let index = ((y * SIZE + x) * 4) as usize;
                    let pixel = &clipped[index..index + 4];
                    if pixel != [0, 0, 0, 0] {
                        assert_eq!(pixel, &near[index..index + 4], "({x}, {y}) of {shape:?}");
                    }
                }
            }
        }
    }
}
