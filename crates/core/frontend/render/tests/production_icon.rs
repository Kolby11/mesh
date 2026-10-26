use mesh_core_elements::Prop;
use mesh_core_elements::style::Color;
use mesh_core_elements::{LayoutRect, WidgetNode};
use mesh_core_icon::{
    FontAsset, FrontendIconBindings, IconMapping, IconPackBindings, ResolvedTarget, SupportedAxes,
};
use mesh_core_render::{PixelBuffer, paint_frontend_tree_at_for_module};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

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
        vocabularies: HashMap::new(),
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
    let _guard = test_guard();
    let mut pack = material_bindings();
    pack.mappings.insert(
        "prepared-glyph.svg".into(),
        IconMapping {
            target: "ms/volume_up".into(),
            multicolor: false,
        },
    );
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

    let prepared = std::thread::spawn(|| {
        mesh_core_icon::resolve_icon_for_module_on_worker(
            "@mesh/navigation-bar",
            "prepared-glyph.svg",
            18,
        )
    })
    .join()
    .unwrap();
    assert!(matches!(
        prepared,
        mesh_core_icon::IconResolution::Found {
            target: ResolvedTarget::Glyph { .. },
            ..
        }
    ));
    assert_eq!(
        mesh_core_icon::prepared_icon_for_module("@mesh/navigation-bar", "prepared-glyph.svg", 18),
        Some(prepared),
        "large shared fonts must not make a resolved name repeat asynchronous lookups"
    );
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
        .insert(Prop::Name, "audio-volume-high".into());
    icon.attributes.insert(Prop::Size, "18".into());
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
        .insert(Prop::Name, "definitely-missing-icon".into());
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

fn paint_missing_icon(consumer: &mesh_core_render::ResourcePaintConsumer) {
    let mut icon = WidgetNode::new("icon");
    icon.set_module_id("@mesh/never-bound");
    icon.layout = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 16.0,
        height: 16.0,
    };
    icon.computed_style.background_color = Color::TRANSPARENT;
    icon.attributes
        .insert(Prop::Name, "missing-repaint-loop-icon".into());
    icon.attributes.insert(Prop::Size, "16".into());
    let _scope = consumer.enter();
    let mut buffer = PixelBuffer::new(16, 16);
    paint_frontend_tree_at_for_module(&icon, &mut buffer, 1.0, 0.0, 0.0, None, None);
}

fn settle_icon_resolution_jobs() {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while mesh_core_render::icon_resolution_jobs_pending() {
        mesh_core_render::poll_icon_resolution_jobs();
        assert!(
            std::time::Instant::now() < deadline,
            "icon resolution job never settled"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// A missing icon must settle: its resolution job finding nothing changes no
/// pixels, so it neither wakes the consumer nor is resubmitted by the repaint.
#[test]
fn missing_icon_resolution_does_not_request_repaints() {
    let _guard = test_guard();
    mesh_core_icon::replace_default_registry(Arc::new(std::sync::Mutex::new(
        mesh_core_icon::IconRegistry::empty(),
    )));
    let consumer = mesh_core_render::ResourcePaintConsumer::default();
    for _ in 0..3 {
        paint_missing_icon(&consumer);
        settle_icon_resolution_jobs();
        assert!(
            !consumer.take_ready(),
            "a missing icon's resolution job requested a repaint"
        );
    }
}

fn named_icon(name: &str) -> WidgetNode {
    let mut node = WidgetNode::new("icon");
    node.set_module_id("@mesh/async-icon-test");
    node.layout = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: 16.0,
        height: 16.0,
    };
    node.computed_style.background_color = Color::TRANSPARENT;
    node.computed_style.color = Color::WHITE;
    node.attributes.insert(Prop::Name, name.into());
    node.attributes.insert(Prop::Size, "16".into());
    node
}

fn paint_icon(
    node: &WidgetNode,
    consumer: &mesh_core_render::ResourcePaintConsumer,
) -> PixelBuffer {
    let _scope = consumer.enter();
    let mut buffer = PixelBuffer::new(16, 16);
    paint_frontend_tree_at_for_module(node, &mut buffer, 1.0, 0.0, 0.0, None, None);
    buffer
}

fn wait_for_icon(consumer: &mesh_core_render::ResourcePaintConsumer) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !consumer.is_ready() {
        mesh_core_render::poll_icon_resolution_jobs();
        mesh_core_render::poll_icon_raster_jobs();
        assert!(
            std::time::Instant::now() < deadline,
            "icon completion never woke its consumer"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn install_theme(root: &std::path::Path, color: &str) {
    let theme = root.join("AsyncIconTest");
    std::fs::create_dir_all(theme.join("scalable")).unwrap();
    std::fs::write(theme.join("index.theme"), "[Icon Theme]\nName=AsyncIconTest\nDirectories=scalable\n\n[scalable]\nSize=16\nType=Scalable\nMinSize=1\nMaxSize=512\n").unwrap();
    std::fs::write(theme.join("scalable/cold-test.svg"), format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="{color}"/></svg>"#)).unwrap();
    let mut registry = mesh_core_icon::IconRegistry::from_catalog(Arc::new(
        mesh_core_resources::SystemResourceCatalog::from_icon_dirs(vec![root.into()]),
    ))
    .unwrap();
    registry.set_shell_default_pack(Some("AsyncIconTest".into()));
    mesh_core_icon::replace_default_registry(Arc::new(std::sync::Mutex::new(registry)));
}

#[test]
fn cold_icon_lookup_repaints_only_consumers_and_rejects_old_catalogs() {
    let _guard = test_guard();
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let consumer = mesh_core_render::ResourcePaintConsumer::default();
    let other = mesh_core_render::ResourcePaintConsumer::default();
    let unrelated = mesh_core_render::ResourcePaintConsumer::default();
    let icon = named_icon("cold-test");
    for (root, color, channel) in [(first.path(), "#ff0000", 2), (second.path(), "#0000ff", 0)] {
        install_theme(root, color);
        consumer.take_ready();
        other.take_ready();
        let initial = paint_icon(&icon, &consumer);
        assert!(
            initial.data().iter().all(|byte| *byte == 0),
            "pending lookup must not paint a false missing marker"
        );
        let other_initial = paint_icon(&icon, &other);
        let other_painted = other_initial
            .data()
            .chunks_exact(4)
            .any(|pixel| pixel[3] != 0);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            wait_for_icon(&consumer);
            consumer.take_ready();
            let pixels = paint_icon(&icon, &consumer);
            if pixels.data().chunks_exact(4).any(|pixel| pixel[3] > 0) {
                assert!(
                    pixels
                        .data()
                        .chunks_exact(4)
                        .all(|pixel| pixel[channel] == 255 && pixel[3] == 255)
                );
                break;
            }
            assert!(std::time::Instant::now() < deadline);
        }
        assert!(other_painted || other.is_ready());
        assert!(!unrelated.is_ready());
    }
}

#[test]
fn painting_does_not_wait_for_the_committed_registry_lock() {
    let _guard = test_guard();
    let registry = Arc::new(std::sync::Mutex::new(mesh_core_icon::IconRegistry::empty()));
    mesh_core_icon::replace_default_registry(Arc::clone(&registry));
    let (locked_send, locked_recv) = std::sync::mpsc::channel();
    let (release_send, release_recv) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _locked = registry.lock().unwrap();
        locked_send.send(()).unwrap();
        release_recv
            .recv_timeout(std::time::Duration::from_secs(2))
            .is_ok()
    });
    locked_recv.recv().unwrap();
    let consumer = mesh_core_render::ResourcePaintConsumer::default();
    let icon = named_icon("unknown-while-locked");
    let initial = paint_icon(&icon, &consumer);
    let other = mesh_core_render::ResourcePaintConsumer::default();
    let other_initial = paint_icon(&icon, &other);
    release_send.send(()).ok();
    assert!(
        holder.join().unwrap(),
        "painting waited for a blocking registry lock"
    );
    assert!(initial.data().iter().all(|byte| *byte == 0));
    assert!(other_initial.data().iter().all(|byte| *byte == 0));
    wait_for_icon(&consumer);
    assert!(
        other.is_ready(),
        "all consumers of a pending lookup must be notified"
    );
    consumer.take_ready();
    let missing = paint_icon(&icon, &consumer);
    assert!(missing.data().chunks_exact(4).any(|pixel| pixel[3] != 0));
    settle_icon_resolution_jobs();
    let _ = paint_icon(&icon, &consumer);
    settle_icon_resolution_jobs();
    assert!(
        !consumer.take_ready(),
        "terminal missing icons must settle after one repaint"
    );
}

#[test]
fn relative_file_names_resolve_on_the_worker_without_false_missing_markers() {
    let _guard = test_guard();
    struct RestoreDirectory(PathBuf);
    impl Drop for RestoreDirectory {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.0).unwrap();
        }
    }
    let _restore = RestoreDirectory(std::env::current_dir().unwrap());
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("local.svg"), r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="white"/></svg>"#).unwrap();
    std::env::set_current_dir(temp.path()).unwrap();
    mesh_core_icon::replace_default_registry(Arc::new(std::sync::Mutex::new(
        mesh_core_icon::IconRegistry::empty(),
    )));
    let consumer = mesh_core_render::ResourcePaintConsumer::default();
    let icon = named_icon("local.svg");
    assert!(
        paint_icon(&icon, &consumer)
            .data()
            .iter()
            .all(|byte| *byte == 0)
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        wait_for_icon(&consumer);
        consumer.take_ready();
        let pixels = paint_icon(&icon, &consumer);
        if pixels.data().chunks_exact(4).all(|pixel| pixel[3] == 255) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "relative file icon did not become ready"
        );
    }
}
