use super::super::paint_node::*;
use super::super::*;
use super::common::*;
use crate::{RenderObjectDirtySummary, RenderObjectFingerprint};
use mesh_core_elements::Prop;
use mesh_core_elements::style::{Color, FontStyle};

#[test]
fn checkbox_and_radio_emit_checkmark_content_only_when_checked() {
    let mut checkbox = node(1, "checkbox", 0.0, 0.0, 18.0, 18.0);
    checkbox.attributes.insert(Prop::Checked, "true".into());
    assert_eq!(
        build_paint_content(&checkbox),
        DisplayPaintContent::Checkmark(DisplayCheckmarkPaint {
            kind: CheckmarkKind::Check,
        })
    );

    let mut radio = node(2, "radio", 0.0, 0.0, 18.0, 18.0);
    radio.attributes.insert(Prop::Checked, "checked".into());
    assert_eq!(
        build_paint_content(&radio),
        DisplayPaintContent::Checkmark(DisplayCheckmarkPaint {
            kind: CheckmarkKind::Dot,
        })
    );

    // Unchecked controls paint no mark.
    let unchecked = node(3, "checkbox", 0.0, 0.0, 18.0, 18.0);
    assert_eq!(build_paint_content(&unchecked), DisplayPaintContent::None);

    let mut falsey = node(4, "checkbox", 0.0, 0.0, 18.0, 18.0);
    falsey.attributes.insert(Prop::Checked, "false".into());
    assert_eq!(build_paint_content(&falsey), DisplayPaintContent::None);
}

#[test]
fn primitive_signature_ignores_irrelevant_payload_attrs_for_generic_nodes() {
    let mut base = node(1, "box", 0.0, 0.0, 20.0, 20.0);
    let original = primitive_signature(&base, DisplayPrimitiveSlot::Generic);

    base.attributes.insert(Prop::Content, "ignored".into());
    base.attributes.insert(Prop::Text, "ignored".into());
    base.attributes.insert(Prop::Value, "ignored".into());
    base.attributes.insert(Prop::Src, "ignored.png".into());

    assert_eq!(
        primitive_signature(&base, DisplayPrimitiveSlot::Generic),
        original
    );
}

#[test]
fn primitive_signature_tracks_relevant_paint_payload_attrs() {
    let mut text = node(1, "text", 0.0, 0.0, 20.0, 20.0);
    let original_text = primitive_signature(&text, DisplayPrimitiveSlot::Generic);
    text.attributes.insert(Prop::Content, "changed".into());
    assert_ne!(
        primitive_signature(&text, DisplayPrimitiveSlot::Generic),
        original_text
    );

    let mut checkbox = node(2, "checkbox", 0.0, 0.0, 20.0, 20.0);
    let original_checkbox = primitive_signature(&checkbox, DisplayPrimitiveSlot::Generic);
    checkbox.attributes.insert(Prop::Checked, "true".into());
    assert_ne!(
        primitive_signature(&checkbox, DisplayPrimitiveSlot::Generic),
        original_checkbox
    );
}

#[test]
fn retained_dirty_contract_matches_shared_paint_signature_inputs() {
    let mut checkbox = node(1, "checkbox", 0.0, 0.0, 20.0, 20.0);
    let checkbox_before = RenderObjectFingerprint::for_node(&checkbox, None);
    let checkbox_signature = primitive_signature(&checkbox, DisplayPrimitiveSlot::Generic);
    checkbox.attributes.insert(Prop::Checked, "true".into());
    let checkbox_after = RenderObjectFingerprint::for_node(&checkbox, Some(&checkbox_before));
    let mut dirty = RenderObjectDirtySummary::default();
    assert!(dirty.add_fingerprint_diff(&checkbox_before, &checkbox_after));
    assert_eq!(dirty.primitive, 1);
    assert_ne!(
        primitive_signature(&checkbox, DisplayPrimitiveSlot::Generic),
        checkbox_signature
    );

    let mut text = node(2, "text", 0.0, 0.0, 20.0, 20.0);
    text.attributes.insert(Prop::Content, "before".into());
    let text_before = RenderObjectFingerprint::for_node(&text, None);
    let text_signature = primitive_signature(&text, DisplayPrimitiveSlot::Text);
    text.attributes.insert(Prop::Text, "after".into());
    text.computed_style.letter_spacing = 1.5;
    let text_after = RenderObjectFingerprint::for_node(&text, Some(&text_before));
    let mut dirty = RenderObjectDirtySummary::default();
    assert!(dirty.add_fingerprint_diff(&text_before, &text_after));
    assert_eq!(dirty.text, 1);
    assert_ne!(
        primitive_signature(&text, DisplayPrimitiveSlot::Text),
        text_signature
    );

    let mut icon = node(3, "icon", 0.0, 0.0, 20.0, 20.0);
    let icon_before = RenderObjectFingerprint::for_node(&icon, None);
    let icon_signature = primitive_signature(&icon, DisplayPrimitiveSlot::Icon);
    icon.computed_style.icon_fill = Some(1.0);
    let icon_after = RenderObjectFingerprint::for_node(&icon, Some(&icon_before));
    let mut dirty = RenderObjectDirtySummary::default();
    assert!(dirty.add_fingerprint_diff(&icon_before, &icon_after));
    assert_eq!(dirty.primitive, 1);
    assert_ne!(
        primitive_signature(&icon, DisplayPrimitiveSlot::Icon),
        icon_signature
    );
}

#[test]
fn resource_revision_invalidates_retained_font_paint_signatures() {
    if crate::isolate_resource_state_test(
        "display_list::tests::signatures::resource_revision_invalidates_retained_font_paint_signatures",
    ) {
        return;
    }
    let text = node(4, "text", 0.0, 0.0, 20.0, 20.0);
    let _resources = crate::resource_state_test_lock();
    let before = primitive_signature(&text, DisplayPrimitiveSlot::Text);
    mesh_core_resources::advance_resource_revision();
    let after = primitive_signature(&text, DisplayPrimitiveSlot::Text);
    assert_ne!(before, after);
}

#[test]
fn batch_signature_uses_only_slot_material() {
    let mut background = node(1, "box", 0.0, 0.0, 20.0, 20.0);
    let original_background = batch_signature(&background, DisplayPrimitiveSlot::Background);

    background.computed_style.color = Color::from_hex("#ff00ff").unwrap();
    background.computed_style.font_size = 48.0;
    background.computed_style.border_color = Color::from_hex("#00ffff").unwrap();
    assert_eq!(
        batch_signature(&background, DisplayPrimitiveSlot::Background),
        original_background
    );

    background.computed_style.background_color = Color::from_hex("#123456").unwrap();
    assert_ne!(
        batch_signature(&background, DisplayPrimitiveSlot::Background),
        original_background
    );
}

#[test]
fn batch_signature_tracks_generic_content_material() {
    let mut slider = node(1, "slider", 0.0, 0.0, 20.0, 20.0);
    slider.computed_style.background_color.a = 0;
    let original = batch_signature(&slider, DisplayPrimitiveSlot::Generic);

    slider.computed_style.font_size = 42.0;
    assert_eq!(
        batch_signature(&slider, DisplayPrimitiveSlot::Generic),
        original
    );

    slider.computed_style.color = Color::from_hex("#336699").unwrap();
    assert_ne!(
        batch_signature(&slider, DisplayPrimitiveSlot::Generic),
        original
    );
}

#[test]
fn batch_signature_separates_text_font_styles() {
    let mut text = node(1, "text", 0.0, 0.0, 20.0, 20.0);
    text.computed_style.background_color.a = 0;
    let normal = batch_signature(&text, DisplayPrimitiveSlot::Text);

    text.computed_style.font_style = FontStyle::Italic;
    assert_ne!(batch_signature(&text, DisplayPrimitiveSlot::Text), normal);
}

#[test]
fn display_entries_skip_batch_signature_for_barriers() {
    let mut text = node(1, "text", 0.0, 0.0, 20.0, 20.0);
    text.computed_style.background_color.a = 0;
    text.attributes.insert(Prop::Content, "barrier".into());
    let mut out = Vec::new();
    let mut next = HashMap::new();

    collect_display_entries(&text, 0.0, 0.0, Some(&mut out), None, &mut next);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].barrier, Some(DisplayBatchBarrier::Text));
    assert_eq!(out[0].batch_signature, 0);
}

#[test]
fn display_entry_collection_can_patch_only_selected_nodes() {
    let mut root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
    let mut first = node(2, "text", 0.0, 0.0, 40.0, 20.0);
    first.attributes.insert(Prop::Content, "first".into());
    let mut second = node(3, "text", 40.0, 0.0, 40.0, 20.0);
    second.attributes.insert(Prop::Content, "second".into());
    root.children.extend([first, second]);

    let mut full = HashMap::new();
    collect_display_entries(&root, 0.0, 0.0, None, None, &mut full);
    let mut selected = HashMap::new();
    collect_display_entries(
        &root,
        0.0,
        0.0,
        None,
        Some(&HashSet::from([3])),
        &mut selected,
    );

    assert!(!selected.is_empty());
    assert!(selected.keys().all(|key| key.node_id == 3));
    assert_eq!(
        selected.get(&DisplayListKey {
            node_id: 3,
            slot: DisplayPrimitiveSlot::Text,
        }),
        full.get(&DisplayListKey {
            node_id: 3,
            slot: DisplayPrimitiveSlot::Text,
        })
    );
}

#[test]
fn retained_fingerprints_match_fresh_display_entries_and_reject_stale_resources() {
    let mut tree = node(1, "column", 0.0, 0.0, 120.0, 80.0);
    let mut text = node(2, "text", 0.0, 0.0, 120.0, 40.0);
    text.attributes
        .insert(Prop::Content, "shared paint inputs".into());
    tree.children.push(text);
    tree.children
        .push(node(3, "checkbox", 0.0, 40.0, 20.0, 20.0));
    let mut fingerprints: HashMap<_, _> = std::iter::once(&tree)
        .chain(tree.children.iter())
        .map(|node| (node.id, RenderObjectFingerprint::for_node(node, None)))
        .collect();
    for stale in [false, true] {
        if stale {
            for fingerprint in fingerprints.values_mut() {
                fingerprint.paint.resource_revision = u64::MAX;
                fingerprint.paint.material = 123;
            }
        }
        let mut expected = HashMap::new();
        collect_display_entries(&tree, 0.0, 0.0, None, None, &mut expected);
        let mut actual = HashMap::new();
        collect_display_entries_with_fingerprints(
            &tree,
            0.0,
            0.0,
            None,
            None,
            &mut actual,
            Some(&|id| fingerprints.get(&id)),
        );
        assert_eq!(actual, expected, "stale resources: {stale}");
        let mut index = batch_index::BatchIndex::default();
        index.rebuild(&tree, 0.0, 0.0, &expected);
        actual.clear();
        assert!(index.collect_dirty_entries_with_fingerprints(
            &tree,
            &HashSet::from([2]),
            &mut actual,
            Some(&|id| fingerprints.get(&id))
        ));
        assert!(
            actual
                .iter()
                .all(|(key, entry)| expected.get(key) == Some(entry))
        );
    }
}

#[test]
#[ignore = "release-only shared retained/display fingerprint benchmark"]
fn shared_retained_display_fingerprint_benchmark() {
    let mut tree = node(1, "column", 0.0, 0.0, 240.0, 20480.0);
    for index in 0..1024 {
        let mut text = node(index + 2, "text", 0.0, index as f32 * 20.0, 240.0, 20.0);
        text.attributes
            .insert(Prop::Content, format!("Label {index}"));
        tree.children.push(text);
    }
    let fingerprints: HashMap<_, _> = std::iter::once(&tree)
        .chain(tree.children.iter())
        .map(|node| (node.id, RenderObjectFingerprint::for_node(node, None)))
        .collect();
    let mut entries = HashMap::new();
    let start = std::time::Instant::now();
    for _ in 0..100 {
        entries.clear();
        collect_display_entries(&tree, 0.0, 0.0, None, None, &mut entries);
        std::hint::black_box(&entries);
    }
    let fresh = start.elapsed();
    let start = std::time::Instant::now();
    for _ in 0..100 {
        entries.clear();
        collect_display_entries_with_fingerprints(
            &tree,
            0.0,
            0.0,
            None,
            None,
            &mut entries,
            Some(&|id| fingerprints.get(&id)),
        );
        std::hint::black_box(&entries);
    }
    let shared = start.elapsed();
    eprintln!("display entries: 1025 nodes, 100 collections: fresh {fresh:?}, shared {shared:?}");
}
