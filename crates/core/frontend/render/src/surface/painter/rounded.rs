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
//! - a fractional border edge is one row or column of solid fill at scaled
//!   alpha.
//!
//! Each corner is rasterized from a symmetric shape twice the corner region's
//! size whose four corners all repeat it, so no clip ever cuts its arc. Shapes
//! whose corner regions overlap (pills, small circles) are rasterized whole.
//! The decomposition needs an integer device translation; any other canvas
//! matrix, or an overlapping shape too large to rasterize whole, returns
//! `false` so the caller can draw it directly. Clipping masks instead record a
//! complete shape as a picture shader and keep its evaluation on the runtime
//! pipeline, independently of the destination damage rectangle.

use std::cell::RefCell;
use std::sync::Arc;

use mesh_core_elements::lru::ByteLruCache;
use mesh_core_elements::style::{Color, Corners, Edges};
use skia_safe::{
    AlphaType, BlendMode, Canvas, ColorType, Data, FilterMode, IRect, ImageInfo, Matrix,
    MipmapMode, Paint, PaintStyle, PictureRecorder, RRect, RSXform, Rect, Region, SamplingOptions,
    Shader, TileMode, Vector, images, region::RegionOp, surfaces,
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
    static CLIP_SHADERS: RefCell<ByteLruCache<ShapeKey, Shader>> =
        RefCell::new(ByteLruCache::new(CLIP_CACHE_ENTRIES, CLIP_CACHE_BYTES));
    // Runtime evaluation keeps mask composition on Skia's high-precision
    // pipeline; the bitmap fast path rounds differently for narrow spans.
    static MASK_EFFECT: skia_safe::RuntimeEffect = skia_safe::RuntimeEffect::make_for_shader(
        "uniform shader mask; half4 main(float2 p) { return half4(mask.eval(p).a); }", None
    ).expect("valid rounded coverage shader");
}

/// Intersect with coverage rasterized independently of the damage clip.
pub(super) fn clip(canvas: &Canvas, rect: ClipRect, radii: Corners) {
    let shape = RoundedShape {
        rect,
        outer: super::backend::normalize_corners(rect.width as f32, rect.height as f32, radii),
        ring: None,
    };
    let key = shape.key();
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
        draw_shape(
            recording,
            rect.width,
            rect.height,
            shape.outer,
            None,
            &paint,
        );
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

/// A rounded rectangle at integer device bounds, optionally hollowed into a
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

/// Coverage of one shape relative to its origin.
#[derive(Clone)]
struct ShapeCoverage {
    full: Region,
    /// Partially covered pixels of every masked part, stacked vertically.
    atlas: Option<skia_safe::Image>,
    /// `(source rect in the atlas, destination offset)` per masked part.
    sprites: Arc<[(Rect, (f32, f32))]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ShapeKey {
    width: i32,
    height: i32,
    outer: [u32; 4],
    widths: Option<[u32; 4]>,
    inner: [u32; 4],
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

    /// Size of each corner's region: it covers the arc and, for a ring, the
    /// square where the two sides meet, so the straight parts left over are
    /// plain bands.
    fn corner_regions(&self) -> [(i32, i32); 4] {
        let outer = corners_array(self.outer);
        std::array::from_fn(|corner| {
            let arc = outer[corner].ceil() as i32;
            let (side_x, side_y) = self.corner_widths(corner);
            (arc.max(side_x.ceil() as i32), arc.max(side_y.ceil() as i32))
        })
    }

    /// Origin of `corner`'s region relative to the shape origin.
    fn corner_origin(&self, corner: usize, (width, height): (i32, i32)) -> (i32, i32) {
        let x = match corner {
            TL | BL => 0,
            _ => self.rect.width - width,
        };
        let y = match corner {
            TL | TR => 0,
            _ => self.rect.height - height,
        };
        (x, y)
    }

    fn overlapping(&self, regions: &[(i32, i32); 4]) -> bool {
        regions[TL].0 + regions[TR].0 > self.rect.width
            || regions[BL].0 + regions[BR].0 > self.rect.width
            || regions[TL].1 + regions[BL].1 > self.rect.height
            || regions[TR].1 + regions[BR].1 > self.rect.height
    }

    fn key(&self) -> ShapeKey {
        ShapeKey {
            width: self.rect.width,
            height: self.rect.height,
            outer: bits(corners_array(self.outer)),
            widths: self
                .widths()
                .map(|widths| bits([widths.top, widths.right, widths.bottom, widths.left])),
            inner: bits(self.ring.map_or([0.0; 4], |ring| corners_array(ring.inner))),
        }
    }
}

/// Draw `shape` in `color` under `clip` with clip-invariant pixels. Returns
/// `false`, having drawn nothing, when the shape must be drawn directly.
pub(super) fn draw(canvas: &Canvas, shape: &RoundedShape, color: Color, clip: ClipRect) -> bool {
    let matrix = canvas.local_to_device_as_3x3();
    if !matrix.is_translate()
        || matrix.translate_x().fract() != 0.0
        || matrix.translate_y().fract() != 0.0
    {
        return false;
    }
    let rect = shape.rect;
    let clipped = intersect_clip(rect, clip);
    if clipped.width <= 0 || clipped.height <= 0 || color.a == 0 {
        return true;
    }
    let regions = shape.corner_regions();
    let overlapping = shape.overlapping(&regions);
    if overlapping && i64::from(rect.width) * i64::from(rect.height) > MAX_WHOLE_SHAPE_PIXELS {
        return false;
    }
    let Some(coverage) = coverage(shape, &regions, overlapping) else {
        return false;
    };

    let paint = solid_paint(color, 1.0);
    let save_count = canvas.save();
    canvas.clip_irect(irect(clipped), None);
    let mut full = coverage.full.clone();
    full.translate((rect.x, rect.y));
    canvas.draw_region(&full, &paint);
    if let Some(atlas) = &coverage.atlas {
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
    if !overlapping && let Some(widths) = shape.widths() {
        draw_fractional_edges(canvas, rect, &regions, widths, color, clipped);
    }
    canvas.restore_to_count(save_count);
    true
}

/// The straight sides of a ring between its corner regions, as
/// `(span start, span end, outer edge, thickness, horizontal, inward sign)`
/// relative to the shape origin.
fn ring_sides(
    rect: ClipRect,
    regions: &[(i32, i32); 4],
    widths: Edges,
) -> [(i32, i32, i32, f32, bool, i32); 4] {
    let (width, height) = (rect.width, rect.height);
    [
        (regions[TL].0, width - regions[TR].0, 0, widths.top, true, 1),
        (
            regions[BL].0,
            width - regions[BR].0,
            height,
            widths.bottom,
            true,
            -1,
        ),
        (
            regions[TL].1,
            height - regions[BL].1,
            0,
            widths.left,
            false,
            1,
        ),
        (
            regions[TR].1,
            height - regions[BR].1,
            width,
            widths.right,
            false,
            -1,
        ),
    ]
}

/// The band `offset..offset + depth` pixels inward from a side's outer edge.
fn side_band(
    (start, end, edge, _, horizontal, inward): (i32, i32, i32, f32, bool, i32),
    offset: i32,
    depth: i32,
) -> IRect {
    let near = if inward > 0 {
        edge + offset
    } else {
        edge - offset - depth
    };
    if horizontal {
        IRect::from_xywh(start, near, end - start, depth)
    } else {
        IRect::from_xywh(near, start, depth, end - start)
    }
}

fn draw_fractional_edges(
    canvas: &Canvas,
    rect: ClipRect,
    regions: &[(i32, i32); 4],
    widths: Edges,
    color: Color,
    clipped: ClipRect,
) {
    for side in ring_sides(rect, regions, widths) {
        let (start, end, _, thickness, _, _) = side;
        let whole = thickness.floor() as i32;
        let fraction = thickness - whole as f32;
        if end <= start || fraction <= 0.0 {
            continue;
        }
        let band = side_band(side, whole, 1).with_offset((rect.x, rect.y));
        let visible = intersect_clip(
            ClipRect {
                x: band.left,
                y: band.top,
                width: band.width(),
                height: band.height(),
            },
            clipped,
        );
        if visible.width > 0 && visible.height > 0 {
            canvas.draw_irect(band, &solid_paint(color, fraction));
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

fn irect(rect: ClipRect) -> IRect {
    IRect::from_xywh(rect.x, rect.y, rect.width, rect.height)
}

fn coverage(
    shape: &RoundedShape,
    regions: &[(i32, i32); 4],
    overlapping: bool,
) -> Option<ShapeCoverage> {
    let key = shape.key();
    COVERAGE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(coverage) = cache.get(&key) {
            return Some(coverage.clone());
        }
        let (coverage, bytes) = if overlapping {
            rasterize_whole(shape)?
        } else {
            rasterize_parts(shape, regions)?
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
        let bytes = (self.atlas_width * atlas_height) as usize + self.full.len() * 16;
        Some((
            ShapeCoverage {
                full,
                atlas,
                sprites: self.sprites.into(),
            },
            bytes,
        ))
    }
}

fn rasterize_whole(shape: &RoundedShape) -> Option<(ShapeCoverage, usize)> {
    let (width, height) = (shape.rect.width, shape.rect.height);
    let pixels = rasterize(width, height, |canvas, paint| {
        draw_shape(canvas, width, height, shape.outer, shape.ring, paint);
    })?;
    let mut builder = CoverageBuilder::default();
    builder.add(&pixels, width, IRect::from_wh(width, height), (0, 0));
    builder.finish()
}

fn rasterize_parts(
    shape: &RoundedShape,
    regions: &[(i32, i32); 4],
) -> Option<(ShapeCoverage, usize)> {
    let mut builder = CoverageBuilder::default();
    for (corner, &size) in regions.iter().enumerate() {
        if size.0 <= 0 || size.1 <= 0 {
            continue;
        }
        let (pixels, region) = rasterize_corner(shape, corner, size)?;
        builder.add(
            &pixels,
            size.0 * 2,
            region,
            shape.corner_origin(corner, size),
        );
    }
    let (width, height) = (shape.rect.width, shape.rect.height);
    match shape.widths() {
        None => {
            let mut body = Region::from_rect(IRect::from_wh(width, height));
            for (corner, &size) in regions.iter().enumerate() {
                let (x, y) = shape.corner_origin(corner, size);
                body.op_rect(IRect::from_xywh(x, y, size.0, size.1), RegionOp::Difference);
            }
            builder.full.extend(skia_safe::region::Iterator::new(&body));
        }
        Some(widths) => {
            for side in ring_sides(shape.rect, regions, widths) {
                let (start, end, _, thickness, _, _) = side;
                let whole = thickness.floor() as i32;
                if end > start && whole > 0 {
                    builder.full.push(side_band(side, 0, whole));
                }
            }
        }
    }
    builder.finish()
}

/// Rasterize a corner from a symmetric shape twice the region's size whose
/// four corners all repeat this one, so the quadrant holds exactly this
/// corner's local geometry. Returns the pixels and the quadrant.
fn rasterize_corner(
    shape: &RoundedShape,
    corner: usize,
    (width, height): (i32, i32),
) -> Option<(Vec<u8>, IRect)> {
    let (mini_width, mini_height) = (width * 2, height * 2);
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
        draw_shape(
            canvas,
            mini_width,
            mini_height,
            Corners::all(radius),
            ring,
            paint,
        );
    })?;
    let x = if matches!(corner, TL | BL) { 0 } else { width };
    let y = if matches!(corner, TL | TR) { 0 } else { height };
    Some((pixels, IRect::from_xywh(x, y, width, height)))
}

fn draw_shape(
    canvas: &Canvas,
    width: i32,
    height: i32,
    outer: Corners,
    ring: Option<Ring>,
    paint: &Paint,
) {
    let outer_rect = Rect::from_wh(width as f32, height as f32);
    let outer = rrect(outer_rect, outer);
    let inner = ring.and_then(|ring| {
        let widths = ring.widths;
        let inner_rect = Rect::from_ltrb(
            widths.left,
            widths.top,
            width as f32 - widths.right,
            height as f32 - widths.bottom,
        );
        (inner_rect.width() > 0.0 && inner_rect.height() > 0.0)
            .then(|| rrect(inner_rect, ring.inner))
    });
    match inner {
        Some(inner) => canvas.draw_drrect(outer, inner, paint),
        None => canvas.draw_rrect(outer, paint),
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
