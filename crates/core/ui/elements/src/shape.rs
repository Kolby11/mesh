//! Geometry of the `<shape>` element: one description that the painter turns
//! into a path and hit-testing turns into an inside/outside decision, so the
//! clickable area is exactly the painted area.
//!
//! Angles are degrees clockwise from twelve o'clock. A sector or arc is centred
//! in the element's box and its outer radius is half the shorter side. Polygon
//! and polyline vertices are percentages of the box.

use crate::{Prop, tree::WidgetNode};
use kurbo::{Affine, BezPath, ParamCurveNearest, PathEl, Point, Shape};
use std::{cell::RefCell, sync::Arc};
use svgtypes::{
    NumberListParser, PathParser, PathSegment, SimplePathSegment, SimplifyingPathParser,
};

const MAX_PATH_BYTES: usize = 16_384;
const MAX_PATH_SEGMENTS: usize = 2_048;
const MAX_PATH_COORD: f64 = 1_000_000.0;
const MIN_PATH_EXTENT: f64 = 0.000_001;

/// Largest angular step, in degrees, of the line segments approximating an arc.
const ARC_STEP_DEGREES: f32 = 2.0;
/// Narrowest band, in pixels, that still counts as a hit on an unfilled line.
const MIN_LINE_HIT_WIDTH: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShapeOp {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ShapeFillRule {
    #[default]
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShapeKind {
    /// A filled annular sector; an inner fraction of zero gives a pie slice.
    Sector {
        inner: f32,
        start: f32,
        sweep: f32,
    },
    /// A stroked circular arc with no fill.
    Arc {
        start: f32,
        sweep: f32,
    },
    Polygon(Vec<(f32, f32)>),
    Polyline(Vec<(f32, f32)>),
    Path(ShapePath),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapePath {
    path: Arc<ParsedPath>,
    to_box: Affine,
    fill: bool,
    rule: ShapeFillRule,
}

#[derive(Debug, PartialEq)]
struct ParsedPath {
    outline: BezPath,
    fill: BezPath,
}

thread_local! {
    static PATHS: RefCell<crate::lru::LruCache<String, Option<Arc<ParsedPath>>>> =
        RefCell::new(crate::lru::LruCache::new(64));
}

impl ShapePath {
    fn from_node(node: &WidgetNode, width: f32, height: f32) -> Option<Self> {
        Self::from_attributes(node, node.attributes.get(Prop::D)?, width, height)
    }

    /// Read a text baseline path in the element's local box coordinates.
    pub fn for_text(node: &WidgetNode) -> Option<Self> {
        Self::from_attributes(
            node,
            node.attributes.get(Prop::TextPath)?,
            node.layout.width,
            node.layout.height,
        )
    }

    fn from_attributes(node: &WidgetNode, data: &str, width: f32, height: f32) -> Option<Self> {
        if data.len() > MAX_PATH_BYTES {
            return None;
        }
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        let [x, y, w, h] = match node.attributes.get(Prop::ViewBox) {
            Some(value) => parse_view_box(value)?,
            None => [0.0, 0.0, 100.0, 100.0],
        };
        let rule = match node
            .attributes
            .get(Prop::FillRule)
            .map(|value| value.trim())
        {
            None | Some("nonzero") => ShapeFillRule::NonZero,
            Some("evenodd") => ShapeFillRule::EvenOdd,
            Some(_) => return None,
        };
        let fill = match node
            .attributes
            .get(Prop::PathFill)
            .map(|value| value.trim())
        {
            None | Some("auto") => true,
            Some("none") => false,
            Some(_) => return None,
        };
        let path = PATHS.with_borrow_mut(|cache| {
            if let Some(path) = cache.get(data) {
                return path.clone();
            }
            let path = parse_path(data).map(Arc::new);
            cache.insert(data.to_owned(), path.clone());
            path
        })?;
        let to_box = Affine::scale_non_uniform(f64::from(width) / w, f64::from(height) / h)
            * Affine::translate((-x, -y));
        Some(Self {
            path,
            to_box,
            fill,
            rule,
        })
    }

    pub fn outline(&self) -> Vec<ShapeOp> {
        let project = |point: Point| {
            let local = self.to_box * point;
            (local.x as f32, local.y as f32)
        };
        self.path
            .outline
            .iter()
            .map(|element| match element {
                PathEl::MoveTo(p) => {
                    let (x, y) = project(p);
                    ShapeOp::Move(x, y)
                }
                PathEl::LineTo(p) => {
                    let (x, y) = project(p);
                    ShapeOp::Line(x, y)
                }
                PathEl::QuadTo(p1, p) => {
                    let (x1, y1) = project(p1);
                    let (x, y) = project(p);
                    ShapeOp::Quad(x1, y1, x, y)
                }
                PathEl::CurveTo(p1, p2, p) => {
                    let (x1, y1) = project(p1);
                    let (x2, y2) = project(p2);
                    let (x, y) = project(p);
                    ShapeOp::Cubic(x1, y1, x2, y2, x, y)
                }
                PathEl::ClosePath => ShapeOp::Close,
            })
            .collect()
    }

    fn contains(&self, x: f32, y: f32, stroke: f32) -> bool {
        let local = Point::new(f64::from(x), f64::from(y));
        if self.fill {
            let winding = self.path.fill.winding(self.to_box.inverse() * local);
            let inside = match self.rule {
                ShapeFillRule::NonZero => winding != 0,
                ShapeFillRule::EvenOdd => winding % 2 != 0,
            };
            if inside {
                return true;
            }
        }
        stroke > 0.0
            && self.path.outline.segments().any(|segment| {
                let reach_squared = f64::from(stroke * 0.5).powi(2);
                (self.to_box * segment).nearest(local, 0.01).distance_sq <= reach_squared
            })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShapeGeometry {
    kind: ShapeKind,
    width: f32,
    height: f32,
    /// Stroke width in pixels; an arc is inset by half of it so it stays inside
    /// the box.
    stroke: f32,
}

impl ShapeGeometry {
    /// Reads a `shape` node's attributes. `None` when the node is not a shape
    /// or its attributes describe nothing drawable.
    pub fn from_node(node: &WidgetNode) -> Option<Self> {
        if node.tag != "shape" {
            return None;
        }
        let number = |prop| {
            node.attributes
                .get(prop)
                .and_then(|v| v.trim().parse::<f32>().ok())
        };
        let points = || parse_points(node.attributes.get(Prop::Points)?);
        let width = node.layout.width;
        let height = node.layout.height;
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        let start = number(Prop::Start).unwrap_or(0.0);
        let end = number(Prop::End).unwrap_or(start + 360.0);
        let sweep = (end - start).clamp(-360.0, 360.0);
        let kind = match node.attributes.get(Prop::Type).map(|v| v.trim()) {
            None | Some("sector") => ShapeKind::Sector {
                inner: number(Prop::Inner).unwrap_or(0.0).clamp(0.0, 1.0),
                start,
                sweep,
            },
            Some("arc") => ShapeKind::Arc { start, sweep },
            Some("polygon") => ShapeKind::Polygon(points()?),
            Some("polyline") => ShapeKind::Polyline(points()?),
            Some("path") => ShapeKind::Path(ShapePath::from_node(node, width, height)?),
            Some(_) => return None,
        };
        Some(Self {
            kind,
            width,
            height,
            stroke: node.computed_style.border_width.top.max(0.0),
        })
    }

    pub fn kind(&self) -> &ShapeKind {
        &self.kind
    }

    /// Whether the painter fills the outline; unfilled shapes only stroke.
    pub fn is_filled(&self) -> bool {
        match &self.kind {
            ShapeKind::Sector { .. } | ShapeKind::Polygon(_) => true,
            ShapeKind::Path(path) => path.fill,
            _ => false,
        }
    }

    pub fn fill_rule(&self) -> ShapeFillRule {
        match &self.kind {
            ShapeKind::Polygon(_) => ShapeFillRule::EvenOdd,
            ShapeKind::Path(path) => path.rule,
            _ => ShapeFillRule::NonZero,
        }
    }

    /// Whether this shape clips its retained child content.
    pub fn clips_children(node: &WidgetNode) -> bool {
        node.tag == "shape"
            && node
                .attributes
                .get(Prop::ClipChildren)
                .is_some_and(|value| matches!(value.trim(), "" | "true" | "1"))
    }

    /// Membership of the filled geometry without the stroke or input slop.
    pub fn contains_fill(&self, x: f32, y: f32) -> bool {
        match &self.kind {
            ShapeKind::Sector {
                inner,
                start,
                sweep,
            } => {
                let (cx, cy) = self.centre();
                let (dx, dy) = (x - cx, y - cy);
                let distance = dx.hypot(dy);
                let radius = self.outer_radius();
                distance <= radius
                    && distance >= radius * inner
                    && angle_within(dx, dy, *start, *sweep)
            }
            ShapeKind::Polygon(points) => {
                let vertices: Vec<_> = points.iter().map(|p| self.vertex(*p)).collect();
                polygon_contains(&vertices, x, y)
            }
            ShapeKind::Path(path) => path.contains(x, y, 0.0),
            ShapeKind::Polyline(_) | ShapeKind::Arc { .. } => false,
        }
    }

    fn centre(&self) -> (f32, f32) {
        (self.width / 2.0, self.height / 2.0)
    }

    fn outer_radius(&self) -> f32 {
        let radius = self.width.min(self.height) / 2.0;
        match self.kind {
            ShapeKind::Arc { .. } => (radius - self.stroke / 2.0).max(0.0),
            ShapeKind::Sector { .. } => (radius - self.stroke / 2.0).max(0.0),
            _ => radius,
        }
    }

    fn vertex(&self, point: (f32, f32)) -> (f32, f32) {
        (point.0 / 100.0 * self.width, point.1 / 100.0 * self.height)
    }

    /// The outline in the element's local coordinates, origin at the box's
    /// top-left corner.
    pub fn outline(&self) -> Vec<ShapeOp> {
        let (cx, cy) = self.centre();
        let outer = self.outer_radius();
        match &self.kind {
            ShapeKind::Sector {
                inner,
                start,
                sweep,
            } => {
                let inner = outer * inner;
                let mut ops = arc_ops(cx, cy, outer, *start, *sweep, true);
                if inner > 0.0 {
                    ops.extend(arc_ops(cx, cy, inner, *start + *sweep, -*sweep, false));
                } else if sweep.abs() < 360.0 {
                    ops.push(ShapeOp::Line(cx, cy));
                }
                ops.push(ShapeOp::Close);
                ops
            }
            ShapeKind::Path(path) => path.outline(),
            ShapeKind::Arc { start, sweep } => arc_ops(cx, cy, outer, *start, *sweep, true),
            ShapeKind::Polygon(points) | ShapeKind::Polyline(points) => {
                let mut ops = Vec::with_capacity(points.len() + 1);
                for (index, point) in points.iter().enumerate() {
                    let (x, y) = self.vertex(*point);
                    ops.push(if index == 0 {
                        ShapeOp::Move(x, y)
                    } else {
                        ShapeOp::Line(x, y)
                    });
                }
                if matches!(self.kind, ShapeKind::Polygon(_)) {
                    ops.push(ShapeOp::Close);
                }
                ops
            }
        }
    }

    /// Whether the local point lies on the shape: inside a filled outline, or
    /// within reach of an unfilled line.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let (cx, cy) = self.centre();
        let outer = self.outer_radius();
        let reach = (self.stroke / 2.0).max(MIN_LINE_HIT_WIDTH / 2.0);
        match &self.kind {
            ShapeKind::Sector {
                inner,
                start,
                sweep,
            } => {
                let (dx, dy) = (x - cx, y - cy);
                let distance = dx.hypot(dy);
                distance <= outer + self.stroke / 2.0
                    && distance >= outer * inner - self.stroke / 2.0
                    && angle_within(dx, dy, *start, *sweep)
            }
            ShapeKind::Arc { start, sweep } => {
                let (dx, dy) = (x - cx, y - cy);
                (dx.hypot(dy) - outer).abs() <= reach && angle_within(dx, dy, *start, *sweep)
            }
            ShapeKind::Polygon(points) => {
                let vertices: Vec<_> = points.iter().map(|p| self.vertex(*p)).collect();
                polygon_contains(&vertices, x, y)
            }
            ShapeKind::Polyline(points) => {
                let vertices: Vec<_> = points.iter().map(|p| self.vertex(*p)).collect();
                vertices
                    .windows(2)
                    .any(|pair| segment_distance(pair[0], pair[1], x, y) <= reach)
            }
            ShapeKind::Path(path) => path.contains(x, y, self.stroke),
        }
    }
}

fn parse_view_box(source: &str) -> Option<[f64; 4]> {
    let mut stream = NumberListParser::from(source);
    let values = [
        stream.next()?.ok()?,
        stream.next()?.ok()?,
        stream.next()?.ok()?,
        stream.next()?.ok()?,
    ];
    (stream.next().is_none()
        && valid_coordinates(values)
        && values[2] >= MIN_PATH_EXTENT
        && values[3] >= MIN_PATH_EXTENT)
        .then_some(values)
}

fn valid_coordinates(values: impl IntoIterator<Item = f64>) -> bool {
    values
        .into_iter()
        .all(|value| value.abs() <= MAX_PATH_COORD)
}

fn valid_path_segment(segment: PathSegment) -> bool {
    match segment {
        PathSegment::MoveTo { x, y, .. }
        | PathSegment::LineTo { x, y, .. }
        | PathSegment::SmoothQuadratic { x, y, .. } => valid_coordinates([x, y]),
        PathSegment::HorizontalLineTo { x, .. } => valid_coordinates([x]),
        PathSegment::VerticalLineTo { y, .. } => valid_coordinates([y]),
        PathSegment::CurveTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
            ..
        } => valid_coordinates([x1, y1, x2, y2, x, y]),
        PathSegment::Quadratic { x1, y1, x, y, .. } => valid_coordinates([x1, y1, x, y]),
        PathSegment::SmoothCurveTo { x2, y2, x, y, .. } => valid_coordinates([x2, y2, x, y]),
        PathSegment::EllipticalArc {
            rx,
            ry,
            x_axis_rotation,
            x,
            y,
            ..
        } => {
            valid_coordinates([rx, ry, x_axis_rotation, x, y])
                && [rx, ry]
                    .into_iter()
                    .all(|radius| radius == 0.0 || radius.abs() >= MIN_PATH_EXTENT)
        }
        PathSegment::ClosePath { .. } => true,
    }
}

fn parse_path(source: &str) -> Option<ParsedPath> {
    // Validating numeric inputs first bounds the SVG parser's expansion of
    // elliptical arcs into cubic segments.
    if !PathParser::from(source).all(|segment| segment.is_ok_and(valid_path_segment)) {
        return None;
    }
    let mut outline = BezPath::new();
    let mut fill = BezPath::new();
    let mut open = false;
    let mut drawable = false;
    for (index, segment) in SimplifyingPathParser::from(source).enumerate() {
        if index == MAX_PATH_SEGMENTS {
            return None;
        }
        let element = match segment.ok()? {
            SimplePathSegment::MoveTo { x, y } => {
                if open {
                    fill.close_path();
                }
                open = true;
                PathEl::MoveTo(Point::new(x, y))
            }
            SimplePathSegment::ClosePath => {
                open = false;
                PathEl::ClosePath
            }
            SimplePathSegment::LineTo { x, y } => {
                drawable = true;
                PathEl::LineTo(Point::new(x, y))
            }
            SimplePathSegment::Quadratic { x1, y1, x, y } => {
                drawable = true;
                PathEl::QuadTo(Point::new(x1, y1), Point::new(x, y))
            }
            SimplePathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                drawable = true;
                PathEl::CurveTo(Point::new(x1, y1), Point::new(x2, y2), Point::new(x, y))
            }
        };
        outline.push(element);
        fill.push(element);
    }
    if open {
        fill.close_path();
    }
    (drawable && outline.is_finite()).then_some(ParsedPath { outline, fill })
}

fn parse_points(source: &str) -> Option<Vec<(f32, f32)>> {
    let mut points = Vec::new();
    for pair in source.split_whitespace() {
        let (x, y) = pair.split_once(',')?;
        points.push((x.parse().ok()?, y.parse().ok()?));
    }
    (points.len() >= 2).then_some(points)
}

fn point_at(cx: f32, cy: f32, radius: f32, degrees: f32) -> (f32, f32) {
    let radians = degrees.to_radians();
    (cx + radius * radians.sin(), cy - radius * radians.cos())
}

/// Line segments along a circular arc, beginning with a move or a line to its
/// first point.
fn arc_ops(cx: f32, cy: f32, radius: f32, start: f32, sweep: f32, begin: bool) -> Vec<ShapeOp> {
    let steps = (sweep.abs() / ARC_STEP_DEGREES).ceil().max(1.0) as usize;
    (0..=steps)
        .map(|step| {
            let (x, y) = point_at(cx, cy, radius, start + sweep * step as f32 / steps as f32);
            if step == 0 && begin {
                ShapeOp::Move(x, y)
            } else {
                ShapeOp::Line(x, y)
            }
        })
        .collect()
}

fn angle_within(dx: f32, dy: f32, start: f32, sweep: f32) -> bool {
    if sweep.abs() >= 360.0 {
        return true;
    }
    if dx == 0.0 && dy == 0.0 {
        return true;
    }
    let angle = dx.atan2(-dy).to_degrees();
    let (from, span) = if sweep >= 0.0 {
        (start, sweep)
    } else {
        (start + sweep, -sweep)
    };
    (angle - from).rem_euclid(360.0) <= span
}

fn polygon_contains(vertices: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut previous = match vertices.last() {
        Some(point) => *point,
        None => return false,
    };
    for &(vx, vy) in vertices {
        if (vy > y) != (previous.1 > y) && x < (previous.0 - vx) * (y - vy) / (previous.1 - vy) + vx
        {
            inside = !inside;
        }
        previous = (vx, vy);
    }
    inside
}

fn segment_distance(a: (f32, f32), b: (f32, f32), x: f32, y: f32) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let length_squared = abx * abx + aby * aby;
    let t = if length_squared == 0.0 {
        0.0
    } else {
        (((x - a.0) * abx + (y - a.1) * aby) / length_squared).clamp(0.0, 1.0)
    };
    (x - (a.0 + t * abx)).hypot(y - (a.1 + t * aby))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(kind: ShapeKind) -> ShapeGeometry {
        ShapeGeometry {
            kind,
            width: 200.0,
            height: 200.0,
            stroke: 0.0,
        }
    }

    #[test]
    fn sector_hits_only_its_wedge_and_ring() {
        let wedge = geometry(ShapeKind::Sector {
            inner: 0.5,
            start: -45.0,
            sweep: 90.0,
        });
        assert!(wedge.contains(100.0, 30.0), "top, between the radii");
        assert!(!wedge.contains(100.0, 95.0), "inside the hub hole");
        assert!(!wedge.contains(170.0, 100.0), "right, outside the wedge");
        assert!(!wedge.contains(100.0, 170.0), "bottom, outside the wedge");
    }

    #[test]
    fn sector_that_crosses_the_top_wraps_around_north() {
        let wedge = geometry(ShapeKind::Sector {
            inner: 0.0,
            start: 315.0,
            sweep: 90.0,
        });
        assert!(wedge.contains(100.0, 20.0));
        assert!(wedge.contains(80.0, 40.0));
        assert!(!wedge.contains(180.0, 100.0));
    }

    #[test]
    fn negative_sweep_covers_the_same_wedge() {
        let wedge = geometry(ShapeKind::Sector {
            inner: 0.0,
            start: 45.0,
            sweep: -90.0,
        });
        assert!(wedge.contains(100.0, 20.0));
        assert!(!wedge.contains(100.0, 180.0));
    }

    #[test]
    fn polygon_uses_even_odd_containment() {
        let triangle = geometry(ShapeKind::Polygon(vec![
            (50.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
        ]));
        assert!(triangle.contains(100.0, 150.0));
        assert!(!triangle.contains(10.0, 10.0));
    }

    #[test]
    fn polyline_hits_within_reach_of_a_segment() {
        let line = geometry(ShapeKind::Polyline(vec![(0.0, 50.0), (100.0, 50.0)]));
        assert!(line.contains(100.0, 102.0));
        assert!(!line.contains(100.0, 120.0));
    }

    #[test]
    fn outline_of_a_ring_sector_is_closed() {
        let wedge = geometry(ShapeKind::Sector {
            inner: 0.4,
            start: 0.0,
            sweep: 90.0,
        });
        let ops = wedge.outline();
        assert!(matches!(ops.first(), Some(ShapeOp::Move(..))));
        assert_eq!(ops.last(), Some(&ShapeOp::Close));
    }

    #[test]
    fn points_need_two_valid_pairs() {
        assert_eq!(
            parse_points("0,0 50,50"),
            Some(vec![(0.0, 0.0), (50.0, 50.0)])
        );
        assert_eq!(parse_points("0,0"), None);
        assert_eq!(parse_points("0,0 a,b"), None);
    }

    fn path_node(data: &str, rule: &str) -> WidgetNode {
        let mut node = WidgetNode::new("shape");
        node.layout.width = 200.0;
        node.layout.height = 100.0;
        node.attributes.insert(Prop::Type, "path".into());
        node.attributes.insert(Prop::D, data.into());
        node.attributes.insert(Prop::FillRule, rule.into());
        node
    }

    #[test]
    fn svg_paths_use_view_box_and_winding_rules() {
        let mut node = path_node(
            "M 10 20 h 100 v 100 H 10 Z M 35 45 h 50 v 50 H 35 Z",
            "evenodd",
        );
        node.attributes
            .insert(Prop::ViewBox, "10,20,100,100".into());
        let shape = ShapeGeometry::from_node(&node).unwrap();
        assert!(shape.contains(15.0, 15.0));
        assert!(!shape.contains(100.0, 50.0));
        node.attributes.insert(Prop::FillRule, "nonzero".into());
        assert!(
            ShapeGeometry::from_node(&node)
                .unwrap()
                .contains(100.0, 50.0)
        );
    }

    #[test]
    fn svg_paths_keep_curves_and_hit_only_a_stroke_when_requested() {
        let mut node = path_node("M10,50 Q45,50 50,50 A40,40 0 0 1 90,90", "nonzero");
        node.attributes.insert(Prop::PathFill, "none".into());
        node.computed_style.border_width.top = 4.0;
        let shape = ShapeGeometry::from_node(&node).unwrap();
        assert!(matches!(shape.outline()[1], ShapeOp::Quad(..)));
        assert!(matches!(shape.outline()[2], ShapeOp::Cubic(..)));
        assert!(shape.contains(60.0, 51.0));
        assert!(!shape.contains(60.0, 53.0));
    }

    #[test]
    fn invalid_paths_and_view_boxes_have_no_geometry() {
        for path in [
            "M 10 20",
            "M10 20 A1e99 1 0 0 1 1 1",
            "L20 20",
            "M10 10 Z bad",
        ] {
            assert!(ShapeGeometry::from_node(&path_node(path, "nonzero")).is_none());
        }
        let mut node = path_node("M10 20 L30 40", "nonzero");
        for view_box in ["0 0 0 1", "0 0 100 100 extra", "0 0 100 1e-99"] {
            node.attributes.insert(Prop::ViewBox, view_box.into());
            assert!(ShapeGeometry::from_node(&node).is_none());
        }
    }
}
