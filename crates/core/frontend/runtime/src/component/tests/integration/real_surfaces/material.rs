use super::*;
use crate::component::testing::glass::{GlassCatalog, root};
use mesh_core_theme::ThemeSourceHandle;

fn material_theme(name: &str, stylesheet: &str) -> mesh_core_theme::Theme {
    mesh_core_theme::load_theme_from_source(
        &ThemeSourceHandle::new(root().join("modules/themes").join(name), stylesheet).unwrap(),
        name,
        name,
    )
    .unwrap()
}

fn paint_desktop(
    component: &mut FrontendSurfaceComponent,
    theme: &mesh_core_theme::Theme,
    width: u32,
    height: u32,
) {
    let mut buffer = PixelBuffer::new(width, height);
    component.surface_size_changed(width, height);
    component
        .paint_raster(
            theme,
            SurfaceExtent::unpadded(width, height),
            &mut buffer,
            1.0,
        )
        .unwrap();
    let text = rendered_text(component);
    assert!(
        !text
            .iter()
            .any(|text| text.contains("script error") || text.contains("frontend runtime")),
        "{text:?}"
    );
    let tree = component.last_tree.as_ref().unwrap();
    let canvas = first_node_by_class(tree, "canvas").unwrap();
    assert!(
        canvas.layout.width > 0.0 && canvas.layout.height > 0.0,
        "{width}x{height}: {:?}",
        canvas.layout
    );
    let grid = first_node_by_class(tree, "widget-grid").unwrap();
    for widget in grid.children.iter().filter(|node| {
        node.attributes
            .get(Prop::Class)
            .is_some_and(|class| class.split_whitespace().any(|class| class == "widget"))
    }) {
        assert!(
            widget.layout.width <= canvas.layout.width,
            "widget exceeds its canvas: {:?} / {:?}",
            widget.layout,
            canvas.layout
        );
    }
}

#[test]
fn material_desktop_edits_grid_and_paints_across_sizes_and_themes() {
    let catalog = GlassCatalog::new();
    let mut component = catalog.mount("material-workspace");
    for (name, stylesheet) in [
        ("material", "dark.css"),
        ("catppuccin-mocha", "theme.css"),
        ("liquid-glass", "theme.css"),
    ] {
        let theme = material_theme(name, stylesheet);
        for (width, height) in [(1920, 1080), (1366, 768), (640, 800)] {
            paint_desktop(&mut component, &theme, width, height);
            assert_eq!(
                runtime_value(&component, "widgets")
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .len(),
                4
            );
        }
    }
    component.call_namespaced_handler("onEdit", &[]).unwrap();
    component
        .call_namespaced_handler("onAdd", &[serde_json::json!("clock")])
        .unwrap();
    assert_eq!(
        runtime_value(&component, "widgets")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        5
    );
    component
        .call_namespaced_handler(
            "onResize",
            &[serde_json::json!("clock-1"), serde_json::json!(40)],
        )
        .unwrap();
    assert_eq!(
        runtime_value(&component, "widgets").unwrap()[0]["width"],
        320
    );
    component
        .call_namespaced_handler(
            "onMove",
            &[serde_json::json!("clock-1"), serde_json::json!(1)],
        )
        .unwrap();
    assert_eq!(
        runtime_value(&component, "widgets").unwrap()[1]["id"],
        "clock-1"
    );
    component
        .call_namespaced_handler("onRemove", &[serde_json::json!("clock-1")])
        .unwrap();
    assert_eq!(
        runtime_value(&component, "widgets")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        4
    );
    component
        .call_namespaced_handler("onAdd", &[serde_json::json!("unknown")])
        .unwrap();
    assert_eq!(
        runtime_value(&component, "widgets")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn material_desktop_remount_restores_edits_and_other_instances_stay_independent() {
    let catalog = GlassCatalog::new();
    let mut component = catalog.mount("material-workspace");
    component
        .call_namespaced_handler(
            "onResize",
            &[serde_json::json!("clock-1"), serde_json::json!(80)],
        )
        .unwrap();
    component
        .call_namespaced_handler("onRemove", &[serde_json::json!("media-4")])
        .unwrap();
    let expected = runtime_value(&component, "widgets").unwrap();
    component.unmount().unwrap();
    component
        .mount(ComponentContext {
            component_id: "@mesh/material-workspace".into(),
            surface_id: "@mesh/material-workspace".into(),
            diagnostics: Diagnostics::new("@mesh/material-workspace"),
        })
        .unwrap();
    let theme = material_theme("material", "dark.css");
    paint_desktop(&mut component, &theme, 1366, 768);
    assert_eq!(runtime_value(&component, "widgets").unwrap(), expected);
    let mut other = catalog.mount_instance("material-workspace", "@mesh/material-workspace#other");
    paint_desktop(&mut other, &theme, 1366, 768);
    let other_widgets = runtime_value(&other, "widgets").unwrap();
    assert_eq!(other_widgets.as_array().unwrap().len(), 4);
    assert_eq!(other_widgets[0]["width"], 280);
}

#[test]
fn material_desktop_keyboard_can_open_picker_and_move_widgets() {
    let mut component = GlassCatalog::new().mount("material-workspace");
    let theme = material_theme("material", "dark.css");
    paint_desktop(&mut component, &theme, 1366, 768);
    let tree = component.last_tree.as_ref().unwrap();
    let edit = first_node_with_attr(tree, "aria-label", "Edit desktop")
        .unwrap()
        .mesh_key()
        .unwrap()
        .to_owned();
    assert!(collect_focus_traversal(tree).contains(&edit));
    component.focused_key = Some(edit);
    component
        .handle_input(
            &theme,
            1366,
            768,
            ComponentInput::KeyPressed {
                key: "Enter".into(),
                modifiers: Default::default(),
            },
        )
        .unwrap();
    component
        .handle_input(
            &theme,
            1366,
            768,
            ComponentInput::KeyReleased {
                key: "Enter".into(),
                modifiers: Default::default(),
            },
        )
        .unwrap();
    paint_desktop(&mut component, &theme, 1366, 768);
    assert_eq!(
        runtime_value(&component, "editing"),
        Some(serde_json::json!(true))
    );
    let tree = component.last_tree.as_ref().unwrap();
    let handle = first_node_with_attr(tree, "aria-label", "Move widget")
        .unwrap()
        .mesh_key()
        .unwrap()
        .to_owned();
    assert!(collect_focus_traversal(tree).contains(&handle));
    component.focused_key = Some(handle);
    component
        .handle_input(
            &theme,
            1366,
            768,
            ComponentInput::KeyPressed {
                key: "ArrowRight".into(),
                modifiers: Default::default(),
            },
        )
        .unwrap();
    assert_eq!(
        runtime_value(&component, "widgets").unwrap()[1]["id"],
        "clock-1"
    );
    component.call_namespaced_handler("onPicker", &[]).unwrap();
    paint_desktop(&mut component, &theme, 1366, 768);
    let tree = component.last_tree.as_ref().unwrap();
    let search = first_node_with_attr(tree, "aria-label", "Search widgets")
        .unwrap()
        .mesh_key()
        .unwrap()
        .to_owned();
    assert!(collect_focus_traversal(tree).contains(&search));
    component.focused_key = Some(search);
    component
        .handle_input(
            &theme,
            1366,
            768,
            ComponentInput::TextInput {
                text: "weather".into(),
            },
        )
        .unwrap();
    assert_eq!(
        runtime_value(&component, "query"),
        Some(serde_json::json!("weather"))
    );
}

#[test]
fn shell_size_change_reflows_controls_and_preserves_hit_testing() {
    let mut component = test_frontend_component(
        r#"
<template><column><button class="scale-proof" onclick={activate}>Accessible</button></column></template>
<script lang="luau">activated = false function activate() activated = true end</script>
<style>.scale-proof { width: 100px; height: 40px; font-size: 14px; padding: 4px; }</style>
"#,
    );
    let mut theme = default_theme();
    let mut buffer = PixelBuffer::new(600, 300);
    let extent = SurfaceExtent::unpadded(300, 150);
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let before = first_node_by_class(component.last_tree.as_ref().unwrap(), "scale-proof").unwrap();
    assert_eq!(
        (before.layout.width, before.computed_style.font_size),
        (100.0, 14.0)
    );
    theme.set_ui_scale(1.5);
    component.theme_changed().unwrap();
    component
        .paint_raster(&theme, extent, &mut buffer, 1.0)
        .unwrap();
    let after = first_node_by_class(component.last_tree.as_ref().unwrap(), "scale-proof").unwrap();
    assert_eq!(
        (after.layout.width, after.computed_style.font_size),
        (150.0, 21.0)
    );
    let (x, y) = (
        after.layout.x + after.layout.width - 10.0,
        after.layout.y + after.layout.height / 2.0,
    );
    for pressed in [true, false] {
        component
            .handle_input(
                &theme,
                300,
                150,
                ComponentInput::PointerButton {
                    x,
                    y,
                    button: 0x110,
                    pressed,
                },
            )
            .unwrap();
    }
    assert_eq!(
        runtime_value(&component, "activated"),
        Some(serde_json::json!(true))
    );
    theme.set_ui_scale(1.0);
    component.theme_changed().unwrap();
    component
        .paint_raster(&theme, extent, &mut buffer, 2.0)
        .unwrap();
    let restored =
        first_node_by_class(component.last_tree.as_ref().unwrap(), "scale-proof").unwrap();
    assert_eq!(
        (restored.layout.width, restored.computed_style.font_size),
        (100.0, 14.0)
    );
}
