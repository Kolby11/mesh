use super::*;

mod common;

mod integration;
mod interaction;
mod invalidation;
mod restyle;

#[test]
fn generated_error_placeholder_is_bounded_after_restyle_constraints() {
    let message = "missing interface ".repeat(100);
    let mut node = runtime::bounded_error_widget(&message);

    // Simulate arbitrary host CSS winning during the normal restyle pass.
    node.computed_style.max_width = None;
    node.children[0].computed_style.max_width = None;
    rendering::constrain_error_placeholders(&mut node);

    for constrained in [&node, &node.children[0]] {
        assert_eq!(
            constrained.computed_style.max_width,
            Some(ERROR_PLACEHOLDER_MAX_WIDTH)
        );
        assert_eq!(constrained.computed_style.min_width, Some(0.0));
        assert_eq!(
            constrained.computed_style.overflow_x,
            mesh_core_elements::style::Overflow::Hidden
        );
        assert_eq!(
            constrained.computed_style.white_space,
            mesh_core_elements::style::WhiteSpace::Nowrap
        );
        assert_eq!(
            constrained.computed_style.text_overflow,
            mesh_core_elements::style::TextOverflow::Ellipsis
        );
    }
    assert_eq!(node.attributes.get("content"), Some(&message));
}
