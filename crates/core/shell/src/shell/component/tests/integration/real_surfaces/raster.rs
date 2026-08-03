use super::*;

/// End-to-end proof that the icon pipeline lands pixels on a real module
/// surface: compile the shipped navigation bar, paint it, locate the volume
/// `<icon>` node, and assert its bounding box contains rasterized pixels.
/// This exercises the full chain — template `<icon>` → WidgetNode →
/// `DisplayPaintContent::Icon` → `render_display_icon_node` → registry/XDG
/// resolution → SVG/PNG raster (or the built-in missing-icon fallback) → blit.
/// The missing-icon fallback always rasterizes, so this is deterministic even
/// without a system icon theme installed.
#[test]
fn shipped_navigation_icon_rasterizes_pixels_on_real_surface() {
    // Provide every interface the navigation bar consumes (audio, network,
    // power, brightness, hyprland, media). A missing interface makes the
    // affected component render an unbounded error-string placeholder instead
    // of its real content — and three ~700px error strings (workspaces, window
    // title, battery) inflate the bar far past its intrinsic width and shove
    // the right-aligned control cluster off-buffer. With the real content the
    // bar fits a normal panel width, exactly as the shipped shell paints it.
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component
        .handle_service_event(&ServiceEvent::Updated {
            service: "mesh.audio".into(),
            source_module: "@mesh/pipewire-audio".into(),
            payload: serde_json::json!({
                "available": true,
                "percent": 50,
                "muted": false
            }),
        })
        .unwrap();
    component.visible = true;

    // Paint at a realistic laptop panel width — narrower than the bar's content
    // overflowed to in the icon-fix follow-up note (x≈1978 on a 960px paint).
    // With real component content the whole bar, including the right cluster,
    // stays on-buffer here.
    let theme = default_theme();
    let width = 1280;
    let height = 80;
    let mut buffer = PixelBuffer::new(width, height);
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();

    let tree = component
        .last_tree
        .as_ref()
        .expect("rendered navigation bar");

    // The right-aligned control cluster must fit entirely within the surface so
    // its buttons (volume, theme, language, battery, settings) are visible and
    // hittable — this is the invariant the follow-up note flagged.
    let cluster = first_node_by_class(tree, "right-cluster").expect("control cluster node");
    let cluster_right = cluster.layout.x + cluster.layout.width;
    assert!(
        cluster.layout.height <= 40.0,
        "right control cluster should shrink-wrap its 40px controls, got {:?}",
        cluster.layout
    );
    assert!(
        cluster.layout.x >= 0.0 && cluster_right <= width as f32,
        "right control cluster bounds [x={}, right={cluster_right}] should fall inside the \
         {width}px surface so all of its controls stay visible",
        cluster.layout.x
    );

    let button = first_node_with_click_handler(
        tree,
        "__mesh_embed__::@mesh/navigation-bar/local:VolumeButton::onAudioToggle",
    )
    .expect("volume button");
    let icon = first_node_by_tag(button, "icon").expect("volume icon node");
    assert!(
        icon.attributes.get("name").is_some() || icon.attributes.get("src").is_some(),
        "volume icon should declare a name or src to resolve"
    );
    let icon_key = icon.mesh_key().expect("icon mesh key").to_owned();
    let (left, top, right, bottom) =
        find_node_bounds_by_key(tree, &icon_key, 0.0, 0.0).expect("icon bounds");
    assert!(
        right > left && bottom > top,
        "icon should have a non-empty layout box, got {left},{top},{right},{bottom}"
    );
    assert!(
        right <= width as f32 && bottom <= height as f32,
        "volume icon bounds [{left},{top},{right},{bottom}] should fall inside the painted \
         {width}x{height} surface so it is actually visible"
    );

    let painted = opaque_pixels_in_bounds(&buffer, left, top, right, bottom);
    assert!(
        painted > 0,
        "the volume icon should rasterize visible pixels onto the real navigation surface \
         (themed icon or built-in missing-icon fallback), but its bounds \
         [{left},{top},{right},{bottom}] were fully transparent"
    );
}

#[test]
fn shipped_navigation_blur_props_control_radius_background_and_enablement() {
    let mut component =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    component.visible = true;
    component.settings_json = serde_json::json!({
        "props": {
            "global": {
                "blur_enabled": true,
                "blur_radius": "7px",
                "blur_background": "rgba(1, 2, 3, 0.5)"
            }
        }
    });
    component.runtimes.lock().unwrap().clear();
    component.init_root_runtime().unwrap();

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(1280, 80);
    component.paint(&theme, 1280, 80, &mut buffer, 1.0).unwrap();
    let nav = first_node_by_class(component.last_tree.as_ref().unwrap(), "nav-shell").unwrap();
    assert_eq!(nav.computed_style.backdrop_filter.blur_radius, 7.0);
    assert_eq!(nav.computed_style.background_color.r, 1);
    assert_eq!(nav.computed_style.background_color.g, 2);
    assert_eq!(nav.computed_style.background_color.b, 3);
    assert!((i16::from(nav.computed_style.background_color.a) - 128).abs() <= 1);

    let mut disabled =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    disabled.visible = true;
    disabled.settings_json = serde_json::json!({
        "props": { "global": { "blur_enabled": false } }
    });
    disabled.runtimes.lock().unwrap().clear();
    disabled.init_root_runtime().unwrap();
    disabled.paint(&theme, 1280, 80, &mut buffer, 1.0).unwrap();
    let nav = first_node_by_class(disabled.last_tree.as_ref().unwrap(), "nav-shell").unwrap();
    assert_eq!(nav.computed_style.backdrop_filter.blur_radius, 0.0);
}
