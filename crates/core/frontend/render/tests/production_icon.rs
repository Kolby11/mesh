use mesh_core_elements::style::Color;
use mesh_core_elements::{LayoutRect, WidgetNode};
use mesh_core_icon::{
    FontAsset, FrontendIconBindings, IconMapping, IconPackBindings, ResolvedTarget, SupportedAxes,
};
use mesh_core_render::{PixelBuffer, paint_frontend_tree_at_for_module};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

fn material_bindings() -> IconPackBindings {
    let module_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../modules/icon-packs/material-symbols");
    let font_path = module_dir.join("assets/MaterialSymbolsRounded.ttf");
    let font_bytes: Arc<[u8]> = std::fs::read(&font_path).unwrap().into();
    let glyphs = HashMap::from([("volume_up".to_string(), 0xe050)]);

    IconPackBindings {
        pack_id: "material-rounded".into(),
        module_id: "@mesh/icons-material-symbols".into(),
        mappings: HashMap::from([(
            "audio-volume-high".into(),
            IconMapping {
                target: "ms/volume_up".into(),
                multicolor: false,
            },
        )]),
        axes: SupportedAxes {
            fill: true,
            weight: true,
            grade: true,
            optical_size: true,
        },
        font_aliases: HashMap::from([(
            "ms".into(),
            FontAsset {
                family: "Material Symbols Rounded".into(),
                glyph_map_path: None,
                resolved_font_path: Some(font_path.clone()),
                prepared_font: Some(font_bytes),
                font_fingerprint: mesh_core_resources::resource_fingerprint(&font_path),
                prepared_glyphs: Some(Arc::new(glyphs)),
            },
        )]),
    }
}

#[test]
fn production_module_icon_path_renders_prepared_material_glyph() {
    let pack = material_bindings();
    mesh_core_icon::replace_default_bindings(
        vec![pack],
        vec![(
            "@mesh/navigation-bar".into(),
            FrontendIconBindings {
                declared_pack_chain: vec!["@mesh/icons-material-symbols".into()],
                ..FrontendIconBindings::default()
            },
        )],
        None,
    )
    .unwrap();

    let resolution =
        mesh_core_icon::resolve_icon_for_module("@mesh/navigation-bar", "audio-volume-high", 18);
    assert!(matches!(
        resolution,
        mesh_core_icon::IconResolution::Found {
            target: ResolvedTarget::Glyph { .. },
            ..
        }
    ));

    let mut root = WidgetNode::new("box");
    root.set_module_id("@mesh/desk");
    root.layout = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 32.0,
        height: 32.0,
    };
    root.computed_style.background_color = Color::TRANSPARENT;

    let mut icon = WidgetNode::new("icon");
    icon.set_module_id("@mesh/navigation-bar");
    icon.layout = LayoutRect {
        x: 4.0,
        y: 4.0,
        width: 24.0,
        height: 24.0,
    };
    icon.computed_style.background_color = Color::TRANSPARENT;
    icon.computed_style.color = Color::WHITE;
    icon.computed_style.icon_fill = Some(0.0);
    icon.computed_style.icon_weight = Some(400.0);
    icon.attributes
        .insert("name".into(), "audio-volume-high".into());
    icon.attributes.insert("size".into(), "18".into());
    root.children.push(icon);

    let mut buffer = PixelBuffer::new(32, 32);
    paint_frontend_tree_at_for_module(&root, &mut buffer, 1.0, 0.0, 0.0, None, Some("@mesh/desk"));

    let opaque = buffer
        .data()
        .chunks_exact(4)
        .filter(|pixel| pixel[3] != 0)
        .count();
    assert!(
        opaque > 8,
        "prepared font glyph should paint more than a marker: {opaque}"
    );

    let mut missing_root = root.clone();
    missing_root.children[0]
        .attributes
        .insert("name".into(), "definitely-missing-icon".into());
    let mut missing_buffer = PixelBuffer::new(32, 32);
    paint_frontend_tree_at_for_module(
        &missing_root,
        &mut missing_buffer,
        1.0,
        0.0,
        0.0,
        None,
        Some("@mesh/desk"),
    );
    assert_ne!(
        buffer.data(),
        missing_buffer.data(),
        "a resolved Material glyph must not paint the same crossed-box fallback as a missing icon"
    );
}
