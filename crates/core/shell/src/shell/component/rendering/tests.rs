use super::layer_surface_request_size;
use mesh_core_wayland::Edge;

#[test]
fn floating_side_surface_keeps_its_measured_height() {
    assert_eq!(
        layer_surface_request_size(Edge::Right, 0, (380, 220)),
        (380, 220)
    );
}
