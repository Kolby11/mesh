use super::*;
use mesh_core_wayland::Edge;

/// Layer-shell arranges higher layers first, so their exclusive zones push the
/// surfaces of lower layers inward.
fn layer_rank(layer: Option<Layer>) -> u8 {
    match layer.unwrap_or(Layer::Top) {
        Layer::Background => 0,
        Layer::Bottom => 1,
        Layer::Top => 2,
        Layer::Overlay => 3,
    }
}

/// The parts of a surface record that decide its position on the output.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Placement {
    visible: bool,
    width: u32,
    height: u32,
    edge: Option<Edge>,
    layer: Option<Layer>,
    exclusive_zone: i32,
    margin_top: i32,
    margin_right: i32,
    margin_bottom: i32,
    margin_left: i32,
}

impl From<&StubSurface> for Placement {
    fn from(surface: &StubSurface) -> Self {
        Self {
            visible: surface.visible,
            width: surface.width,
            height: surface.height,
            edge: surface.edge,
            layer: surface.layer,
            exclusive_zone: surface.exclusive_zone,
            margin_top: surface.margin_top,
            margin_right: surface.margin_right,
            margin_bottom: surface.margin_bottom,
            margin_left: surface.margin_left,
        }
    }
}

/// Where a layer surface sits on its output, predicted the way a wlroots-style
/// compositor arranges it: anchor edge, margins, centering on an unanchored or
/// narrower axis, and the exclusive zones MESH's own surfaces reserve before
/// it. Layer-shell never tells a client its position, and foreign surfaces'
/// zones are invisible to MESH, so a frame is exact only when MESH owns every
/// exclusive zone on that edge (spec 04 §11).
pub(super) fn predicted_origin(
    surface: Placement,
    others: impl Iterator<Item = Placement>,
    output: (f32, f32),
) -> (f32, f32) {
    let (out_w, out_h) = output;
    let w = if surface.width == 0 {
        out_w
    } else {
        surface.width as f32
    };
    let h = if surface.height == 0 {
        out_h
    } else {
        surface.height as f32
    };
    let zone: f32 = if surface.exclusive_zone < 0 {
        0.0
    } else {
        others
            .filter(|other| {
                other.visible
                    && other.edge == surface.edge
                    && other.exclusive_zone > 0
                    && layer_rank(other.layer) > layer_rank(surface.layer)
            })
            .map(|other| other.exclusive_zone as f32)
            .sum()
    };
    let (ml, mr, mt, mb) = (
        surface.margin_left as f32,
        surface.margin_right as f32,
        surface.margin_top as f32,
        surface.margin_bottom as f32,
    );
    let centered_x = (out_w - w) / 2.0 + (ml - mr) / 2.0;
    let centered_y = (out_h - h) / 2.0 + (mt - mb) / 2.0;
    let span_x = if surface.width == 0 { ml } else { centered_x };
    let span_y = if surface.height == 0 { mt } else { centered_y };
    match surface.edge {
        Some(Edge::Top) => (span_x, zone + mt),
        Some(Edge::Bottom) => (span_x, out_h - zone - mb - h),
        Some(Edge::Left) => (zone + ml, span_y),
        Some(Edge::Right) => (out_w - zone - mr - w, span_y),
        None => (centered_x, centered_y),
    }
}

impl Shell {
    pub(super) fn backdrop_frame(&self, surface_id: &str) -> mesh_core_render::BackdropFrame {
        let Some(surface) = self.surfaces.get(surface_id) else {
            return mesh_core_render::BackdropFrame::default();
        };
        let Some((out_w, out_h)) = self.presentation_engine.output_logical_size(surface_id) else {
            return mesh_core_render::BackdropFrame::default();
        };
        let output = (out_w as f32, out_h as f32);
        let others = self
            .surfaces
            .iter()
            .filter(|(id, _)| id.as_str() != surface_id)
            .map(|(_, other)| Placement::from(other));
        mesh_core_render::BackdropFrame {
            origin: predicted_origin(Placement::from(surface), others, output),
            output,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer_surface(edge: Edge, layer: Layer, size: (u32, u32), zone: i32) -> Placement {
        Placement {
            visible: true,
            width: size.0,
            height: size.1,
            edge: Some(edge),
            layer: Some(layer),
            exclusive_zone: zone,
            ..Placement::default()
        }
    }

    #[test]
    fn origins_follow_anchor_margins_and_higher_layer_zones() {
        let output = (2880.0, 1800.0);
        let mut bar = layer_surface(Edge::Top, Layer::Top, (2846, 40), 52);
        bar.margin_top = 8;
        assert_eq!(
            predicted_origin(bar, std::iter::empty(), output),
            (17.0, 8.0)
        );

        let mut cards = layer_surface(Edge::Top, Layer::Bottom, (0, 600), 0);
        cards.margin_top = 490;
        assert_eq!(
            predicted_origin(cards, std::iter::once(bar), output),
            (0.0, 542.0),
            "a zone-respecting surface moves below a higher layer's exclusive zone"
        );

        let wallpaper = layer_surface(Edge::Top, Layer::Background, (0, 1800), -1);
        assert_eq!(
            predicted_origin(wallpaper, std::iter::once(bar), output),
            (0.0, 0.0)
        );

        let mut dock = layer_surface(Edge::Bottom, Layer::Top, (700, 106), 117);
        dock.margin_bottom = 11;
        assert_eq!(
            predicted_origin(dock, std::iter::empty(), output),
            (1090.0, 1683.0)
        );
    }
}
