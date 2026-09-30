use super::*;

fn contains_text(node: &WidgetNode, content: &str) -> bool {
    node.attributes
        .get("content")
        .is_some_and(|value| value == content)
        || node
            .children
            .iter()
            .any(|child| contains_text(child, content))
}

/// `navigation_bar_catalog` has no `mesh.wm` provider, so the bar's start
/// slot components cannot create a runtime. That cannot change while the
/// component keeps its catalog; each frame used to rebuild and discard a whole
/// Luau context per instance before showing the same placeholder.
#[test]
fn unavailable_interface_runtime_failure_is_not_retried_every_frame() {
    let theme = default_theme();
    let mut navigation =
        real_frontend_module_component("@mesh/navigation-bar", navigation_bar_catalog());
    navigation.visible = true;
    let mut buffer = PixelBuffer::new(960, 80);
    navigation
        .paint(&theme, SurfaceExtent::unpadded(960, 80), &mut buffer, 1.0)
        .unwrap();

    let failed = navigation
        .failed_runtime_creations
        .borrow()
        .keys()
        .map(|key| key.to_string())
        .collect::<Vec<_>>();
    assert!(
        !failed.is_empty() && failed.iter().all(|key| key.contains("slot:start")),
        "only the mesh.wm start-slot instances should be remembered as failed: {failed:?}"
    );
    for key in &failed {
        assert!(
            !navigation
                .runtimes
                .lock()
                .unwrap()
                .contains_key(key.as_str())
        );
    }

    // A retry would record and render a freshly formatted message; a
    // remembered failure renders exactly what was stored.
    for message in navigation
        .failed_runtime_creations
        .borrow_mut()
        .values_mut()
    {
        *message = "remembered failure".into();
    }
    navigation.invalidate(ComponentDirtyFlags::TREE_REBUILD);
    navigation
        .paint(&theme, SurfaceExtent::unpadded(960, 80), &mut buffer, 1.0)
        .unwrap();
    assert!(contains_text(
        navigation.last_tree.as_ref().unwrap(),
        "remembered failure"
    ));

    // Rebuilding the runtime map forgets the failures.
    navigation.unmount_runtimes().unwrap();
    assert!(navigation.failed_runtime_creations.borrow().is_empty());
}
