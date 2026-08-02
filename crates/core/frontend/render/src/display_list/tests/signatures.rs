use super::super::paint_node::*;
use super::super::*;
use super::common::*;
use crate::RenderObjectDirtySummary;
use mesh_core_elements::WidgetNode;
use mesh_core_elements::style::{BackgroundPaint, Color, StyleLinearGradient};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[test]
fn checkbox_and_radio_emit_checkmark_content_only_when_checked() {
    let mut checkbox = node(1, "checkbox", 0.0, 0.0, 18.0, 18.0);
    checkbox.attributes.insert("checked".into(), "true".into());
    assert_eq!(
        build_paint_content(&checkbox),
        DisplayPaintContent::Checkmark(DisplayCheckmarkPaint {
            kind: CheckmarkKind::Check,
        })
    );

    let mut radio = node(2, "radio", 0.0, 0.0, 18.0, 18.0);
    radio.attributes.insert("checked".into(), "checked".into());
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
    falsey.attributes.insert("checked".into(), "false".into());
    assert_eq!(build_paint_content(&falsey), DisplayPaintContent::None);
}

#[test]
fn primitive_signature_ignores_irrelevant_payload_attrs_for_generic_nodes() {
    let mut base = node(1, "box", 0.0, 0.0, 20.0, 20.0);
    let original = primitive_signature(&base, DisplayPrimitiveSlot::Generic);

    base.attributes.insert("content".into(), "ignored".into());
    base.attributes.insert("value".into(), "ignored".into());
    base.attributes.insert("src".into(), "ignored.png".into());

    assert_eq!(
        primitive_signature(&base, DisplayPrimitiveSlot::Generic),
        original
    );
}

#[test]
fn primitive_signature_tracks_relevant_paint_payload_attrs() {
    let mut text = node(1, "text", 0.0, 0.0, 20.0, 20.0);
    let original_text = primitive_signature(&text, DisplayPrimitiveSlot::Generic);
    text.attributes.insert("content".into(), "changed".into());
    assert_ne!(
        primitive_signature(&text, DisplayPrimitiveSlot::Generic),
        original_text
    );

    let mut checkbox = node(2, "checkbox", 0.0, 0.0, 20.0, 20.0);
    let original_checkbox = primitive_signature(&checkbox, DisplayPrimitiveSlot::Generic);
    checkbox.attributes.insert("checked".into(), "true".into());
    assert_ne!(
        primitive_signature(&checkbox, DisplayPrimitiveSlot::Generic),
        original_checkbox
    );
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
fn display_entries_skip_batch_signature_for_barriers() {
    let mut text = node(1, "text", 0.0, 0.0, 20.0, 20.0);
    text.computed_style.background_color.a = 0;
    text.attributes.insert("content".into(), "barrier".into());
    let mut out = Vec::new();
    let mut next = HashMap::new();

    collect_display_entries(&text, 0.0, 0.0, Some(&mut out), None, &mut next);

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0.slot, DisplayPrimitiveSlot::Text);
    assert_eq!(out[0].1.barrier, Some(DisplayBatchBarrier::Text));
    assert_eq!(out[0].1.batch_signature, 0);
}

#[test]
fn display_entry_collection_can_patch_only_selected_nodes() {
    let mut root = node(1, "row", 0.0, 0.0, 100.0, 20.0);
    let mut first = node(2, "text", 0.0, 0.0, 40.0, 20.0);
    first.attributes.insert("content".into(), "first".into());
    let mut second = node(3, "text", 40.0, 0.0, 40.0, 20.0);
    second.attributes.insert("content".into(), "second".into());
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

// cargo test -p mesh-core-render --release -- display_primitive_hashing_beats_byte_fallback --ignored --nocapture
#[test]
#[ignore = "release-only display signature primitive hashing microbenchmark"]
fn display_primitive_hashing_beats_byte_fallback() {
    #[derive(Default)]
    struct ByteOnlyHasher(u64);

    impl Hasher for ByteOnlyHasher {
        fn finish(&self) -> u64 {
            self.0
        }

        fn write(&mut self, bytes: &[u8]) {
            for byte in bytes {
                self.0 ^= u64::from(*byte);
                self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }

    fn hash_fields(hasher: &mut impl Hasher) {
        4_u8.hash(hasher);
        0x1234_u16.hash(hasher);
        0x1234_5678_u32.hash(hasher);
        0x1234_5678_9abc_def0_u64.hash(hasher);
        0x1234_5678_9abc_def0_1234_5678_9abc_def0_u128.hash(hasher);
        1920_usize.hash(hasher);
        (-42_i32).hash(hasher);
        (-9001_i64).hash(hasher);
    }

    let iterations = 5_000_000;
    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0_u64;
    for _ in 0..iterations {
        let mut hasher = ByteOnlyHasher(0xcbf2_9ce4_8422_2325);
        hash_fields(&mut hasher);
        old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(hasher.finish()));
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0_u64;
    for _ in 0..iterations {
        let mut hasher = DisplaySignatureHasher::default();
        hash_fields(&mut hasher);
        new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(hasher.finish()));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "display primitive hashing: byte fallback {old_time:?}; word-at-a-time {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_ne!(old_accumulator, 0);
    assert_ne!(new_accumulator, 0);
    assert!(new_time * 5 < old_time * 4);
}

// cargo test -p mesh-core-render --release -- retained_subtree_handle_beats_fieldwise_clone --ignored --nocapture
#[test]
#[ignore = "release-only retained paint-subtree clone microbenchmark"]
fn retained_subtree_handle_beats_fieldwise_clone() {
    let subtree = RetainedPaintSubtree {
        generation: 1,
        filter_layer: false,
        commands: vec![DisplayPaintCommand {
            node: Arc::new(build_paint_node(
                &node(1, "box", 0.0, 0.0, 20.0, 20.0),
                0.0,
                0.0,
            )),
            clip: DisplayListClip {
                x: 0,
                y: 0,
                width: 20,
                height: 20,
            },
            kind: DisplayPaintCommandKind::Node,
        }]
        .into(),
        kinds: vec![DisplayPaintCommandKind::Node].into(),
        effect_overflow_count: 0,
        pruning: PruningMetrics::default(),
        command_span: Some(RetainedSubtreeSpan {
            bounds: DamageRect {
                x: 0,
                y: 0,
                width: 20,
                height: 20,
            },
            local_bounds: DamageRect {
                x: 0,
                y: 0,
                width: 20,
                height: 20,
            },
            command_count: 1,
            includes_scrollbars: false,
        }),
        child_order: Some(vec![0, 1, 2, 3].into()),
    };
    let retained = Arc::new(subtree.clone());
    let iterations = 10_000_000;

    let old_started = std::time::Instant::now();
    for _ in 0..iterations {
        std::hint::black_box(std::hint::black_box(&subtree).clone());
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    for _ in 0..iterations {
        std::hint::black_box(Arc::clone(std::hint::black_box(&retained)));
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "retained subtree reuse clone: fieldwise {old_time:?}; whole-subtree handle {new_time:?}; ratio {:.1}x",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert!(new_time * 5 < old_time * 4);
}

// cargo test -p mesh-core-render --release -- arc_paint_command_node_beats_owned_command_clones --ignored --nocapture
#[test]
#[ignore = "release-only display paint command node clone microbenchmark"]
fn arc_paint_command_node_beats_owned_command_clones() {
    #[derive(Clone)]
    struct OldDisplayPaintCommand {
        node: DisplayPaintNode,
        clip: DisplayListClip,
        kind: DisplayPaintCommandKind,
    }

    let mut widget = node(1, "text", 0.0, 0.0, 180.0, 24.0);
    widget.attributes.insert(
        "content".into(),
        "The same paint node is copied through retained command buffers".into(),
    );
    let paint_node = build_paint_node(&widget, 0.0, 0.0);
    let clip = DisplayListClip {
        x: 0,
        y: 0,
        width: 180,
        height: 24,
    };
    let old_commands = vec![
        OldDisplayPaintCommand {
            node: paint_node.clone(),
            clip,
            kind: DisplayPaintCommandKind::Node,
        },
        OldDisplayPaintCommand {
            node: paint_node.clone(),
            clip,
            kind: DisplayPaintCommandKind::Scrollbars,
        },
    ];
    let shared_node = Arc::new(paint_node);
    let new_commands = vec![
        DisplayPaintCommand {
            node: Arc::clone(&shared_node),
            clip,
            kind: DisplayPaintCommandKind::Node,
        },
        DisplayPaintCommand {
            node: shared_node,
            clip,
            kind: DisplayPaintCommandKind::Scrollbars,
        },
    ];
    let iterations = 2_000_000;

    let old_started = std::time::Instant::now();
    let mut old_total = 0usize;
    for _ in 0..iterations {
        let cloned = old_commands.clone();
        old_total = old_total.wrapping_add(
            cloned
                .iter()
                .map(|command| {
                    command.node.id as usize
                        + command.clip.width as usize
                        + usize::from(command.kind == DisplayPaintCommandKind::Scrollbars)
                })
                .sum::<usize>(),
        );
        std::hint::black_box(cloned);
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_total = 0usize;
    for _ in 0..iterations {
        let cloned = new_commands.clone();
        new_total = new_total.wrapping_add(
            cloned
                .iter()
                .map(|command| {
                    command.node.id as usize
                        + command.clip.width as usize
                        + usize::from(command.kind == DisplayPaintCommandKind::Scrollbars)
                })
                .sum::<usize>(),
        );
        std::hint::black_box(cloned);
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "display paint command node clone: owned node {old_time:?}; arc node {new_time:?}; ratio {:.1}x; totals={old_total}/{new_total}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_eq!(old_total, new_total);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-render --release -- unchanged_display_list_update_beats_flat_rebuild --ignored --nocapture
#[test]
#[ignore = "release-only unchanged display-list update microbenchmark"]
fn unchanged_display_list_update_beats_flat_rebuild() {
    let root = display_entry_benchmark_tree(24, 24);
    let iterations = 1_000;

    let mut retained = RetainedDisplayList::default();
    retained.update(&root, 1200, 800, false, true);

    let no_op_started = std::time::Instant::now();
    let mut no_op_accumulator = 0u64;
    for _ in 0..iterations {
        let metrics = retained.update(&root, 1200, 800, false, true);
        no_op_accumulator =
            no_op_accumulator.wrapping_add(std::hint::black_box(metrics.entries_reused));
    }
    let no_op_time = no_op_started.elapsed();

    let rebuild_started = std::time::Instant::now();
    let mut rebuild_accumulator = 0u64;
    for _ in 0..iterations {
        let mut rebuilt = RetainedDisplayList::default();
        let metrics = rebuilt.update(&root, 1200, 800, false, true);
        rebuild_accumulator =
            rebuild_accumulator.wrapping_add(std::hint::black_box(metrics.entries_rebuilt));
    }
    let rebuild_time = rebuild_started.elapsed();

    eprintln!(
        "unchanged display-list update: no-op {no_op_time:?}; fresh flat rebuild {rebuild_time:?}; ratio {:.1}x; accumulators={no_op_accumulator}/{rebuild_accumulator}",
        rebuild_time.as_secs_f64() / no_op_time.as_secs_f64()
    );
    assert_ne!(no_op_accumulator, 0);
    assert_ne!(rebuild_accumulator, 0);
    assert!(no_op_time < rebuild_time);
}

// cargo test -p mesh-core-render --release -- retained_generation_shortcut_beats_non_clean_entry_scan --ignored --nocapture
#[test]
#[ignore = "release-only retained-generation display-list microbenchmark"]
fn retained_generation_shortcut_beats_non_clean_entry_scan() {
    let root = display_entry_benchmark_tree(120, 20);
    let iterations = 2_000;
    let empty_dirty = HashSet::new();

    let mut scanned = RetainedDisplayList::default();
    scanned.update_with_dirty_nodes(
        &root,
        RenderObjectDirtySummary::default(),
        &empty_dirty,
        1200,
        800,
        false,
        true,
    );
    let scan_started = std::time::Instant::now();
    let mut scan_total = 0u64;
    for _ in 0..iterations {
        let metrics = scanned.update_with_dirty_nodes(
            &root,
            RenderObjectDirtySummary::default(),
            &empty_dirty,
            1200,
            800,
            false,
            true,
        );
        scan_total = scan_total.wrapping_add(std::hint::black_box(metrics.entries_reused));
    }
    let scan_time = scan_started.elapsed();

    let mut generation_gated = RetainedDisplayList::default();
    generation_gated.update_for_retained_generation(
        &root,
        1,
        RenderObjectDirtySummary::default(),
        &empty_dirty,
        1200,
        800,
        false,
        true,
    );
    let gated_started = std::time::Instant::now();
    let mut gated_total = 0u64;
    for _ in 0..iterations {
        let metrics = generation_gated.update_for_retained_generation(
            &root,
            1,
            RenderObjectDirtySummary::default(),
            &empty_dirty,
            1200,
            800,
            false,
            true,
        );
        gated_total = gated_total.wrapping_add(std::hint::black_box(metrics.entries_reused));
    }
    let gated_time = gated_started.elapsed();

    assert_eq!(scan_total, gated_total);
    eprintln!(
        "unchanged non-clean display-list sync: entry scan {scan_time:?}; retained-generation gate {gated_time:?}; ratio {:.1}x",
        scan_time.as_secs_f64() / gated_time.as_secs_f64()
    );
    assert!(gated_time * 2 < scan_time);
}

// cargo test -p mesh-core-render --release -- sparse_display_entry_patch_beats_full_signature_collection --ignored --nocapture
#[test]
#[ignore = "release-only sparse display-entry collection microbenchmark"]
fn sparse_display_entry_patch_beats_full_signature_collection() {
    let root = display_entry_benchmark_tree(120, 20);
    let iterations = 2_000;
    let selected_ids = HashSet::from([1_200_u64]);
    let mut retained = HashMap::new();
    collect_display_entries(&root, 0.0, 0.0, None, None, &mut retained);

    let full_started = std::time::Instant::now();
    let mut full = HashMap::new();
    let mut full_total = 0usize;
    for _ in 0..iterations {
        full.clear();
        collect_display_entries(&root, 0.0, 0.0, None, None, &mut full);
        full_total = full_total.wrapping_add(std::hint::black_box(full.len()));
    }
    let full_time = full_started.elapsed();

    let copied_started = std::time::Instant::now();
    let mut copied = HashMap::new();
    let mut copied_total = 0usize;
    for _ in 0..iterations {
        copied.clear();
        copied.extend(retained.iter().map(|(key, entry)| (*key, *entry)));
        for node_id in &selected_ids {
            for slot in DISPLAY_PRIMITIVE_SLOTS {
                copied.remove(&DisplayListKey {
                    node_id: *node_id,
                    slot,
                });
            }
        }
        collect_display_entries(&root, 0.0, 0.0, None, Some(&selected_ids), &mut copied);
        copied_total = copied_total.wrapping_add(std::hint::black_box(copied.len()));
    }
    let copied_time = copied_started.elapsed();

    let in_place_started = std::time::Instant::now();
    let mut in_place = retained.clone();
    let mut replacements = HashMap::new();
    let mut in_place_total = 0usize;
    for _ in 0..iterations {
        replacements.clear();
        collect_display_entries(
            &root,
            0.0,
            0.0,
            None,
            Some(&selected_ids),
            &mut replacements,
        );
        for node_id in &selected_ids {
            for slot in DISPLAY_PRIMITIVE_SLOTS {
                let key = DisplayListKey {
                    node_id: *node_id,
                    slot,
                };
                if let Some(entry) = replacements.remove(&key) {
                    in_place.insert(key, entry);
                } else {
                    in_place.remove(&key);
                }
            }
        }
        in_place_total = in_place_total.wrapping_add(std::hint::black_box(in_place.len()));
    }
    let in_place_time = in_place_started.elapsed();

    assert_eq!(full_total, copied_total);
    assert_eq!(copied_total, in_place_total);
    assert_eq!(full, copied);
    assert_eq!(copied, in_place);
    eprintln!(
        "sparse display entries: full signatures {full_time:?}; copied-map patch {copied_time:?}; in-place patch {in_place_time:?}; copy elimination ratio {:.1}x",
        copied_time.as_secs_f64() / in_place_time.as_secs_f64()
    );
    assert!(
        in_place_time * 5 < copied_time * 4,
        "in-place sparse patching should beat copied-map patching by at least 20%"
    );
}

// cargo test -p mesh-core-render --release -- tag_aware_payload_signature_skips_irrelevant_attr_hashes --ignored --nocapture
#[test]
#[ignore = "release-only display signature payload hashing microbenchmark"]
fn tag_aware_payload_signature_skips_irrelevant_attr_hashes() {
    fn old_hash_all_payload_attrs(node: &WidgetNode, slot: DisplayPrimitiveSlot) -> u64 {
        let mut hasher = DisplaySignatureHasher::default();
        slot.hash(&mut hasher);
        node.tag.hash(&mut hasher);
        hash_attribute(node, "content", &mut hasher);
        hash_attribute(node, "text", &mut hasher);
        hash_attribute(node, "name", &mut hasher);
        hash_attribute(node, "value", &mut hasher);
        hash_attribute(node, "placeholder", &mut hasher);
        hash_attribute(node, "type", &mut hasher);
        hash_attribute(node, "min", &mut hasher);
        hash_attribute(node, "max", &mut hasher);
        hash_attribute(node, "orient", &mut hasher);
        hash_attribute(node, "src", &mut hasher);
        hash_attribute(node, "size", &mut hasher);
        hasher.finish()
    }

    let mut nodes = Vec::new();
    for index in 0..512_u64 {
        let tag = if index % 8 == 0 { "text" } else { "box" };
        let mut item = node(index + 1, tag, 0.0, 0.0, 20.0, 20.0);
        item.attributes
            .insert("content".into(), format!("row {index}"));
        item.attributes.insert("value".into(), index.to_string());
        item.attributes.insert("src".into(), "icon.png".into());
        nodes.push(item);
    }
    let iterations = 20_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0u64;
    for _ in 0..iterations {
        for item in &nodes {
            old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(
                old_hash_all_payload_attrs(item, DisplayPrimitiveSlot::Generic),
            ));
        }
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0u64;
    for _ in 0..iterations {
        for item in &nodes {
            let mut hasher = DisplaySignatureHasher::default();
            DisplayPrimitiveSlot::Generic.hash(&mut hasher);
            item.tag.hash(&mut hasher);
            hash_paint_content_attributes(item, &mut hasher);
            new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(hasher.finish()));
        }
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "display payload signature attrs: all attrs {old_time:?}; tag-aware {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_ne!(old_accumulator, 0);
    assert_ne!(new_accumulator, 0);
    assert!(new_time < old_time);
}

// cargo test -p mesh-core-render --release -- slot_aware_batch_signature_skips_irrelevant_material_hashes --ignored --nocapture
#[test]
#[ignore = "release-only display batch signature material hashing microbenchmark"]
fn slot_aware_batch_signature_skips_irrelevant_material_hashes() {
    fn old_batch_signature(node: &WidgetNode, slot: DisplayPrimitiveSlot) -> u64 {
        let mut hasher = DisplaySignatureHasher::default();
        slot.hash(&mut hasher);
        hash_color(node.computed_style.background_color, &mut hasher);
        hash_color(node.computed_style.border_color, &mut hasher);
        hash_color(node.computed_style.color, &mut hasher);
        node.computed_style.font_family.hash(&mut hasher);
        node.computed_style.font_size.to_bits().hash(&mut hasher);
        hash_box_shadow(node.computed_style.box_shadow, &mut hasher);
        hash_background_paint(&node.computed_style.background_paint, &mut hasher);
        node.computed_style
            .filter
            .blur_radius
            .to_bits()
            .hash(&mut hasher);
        node.computed_style
            .backdrop_filter
            .blur_radius
            .to_bits()
            .hash(&mut hasher);
        hasher.finish()
    }

    let mut nodes = Vec::new();
    for index in 0..512_u64 {
        let mut item = node(index + 1, "box", 0.0, 0.0, 20.0, 20.0);
        item.computed_style.background_color = Color {
            r: (index % 251) as u8,
            g: ((index * 3) % 251) as u8,
            b: ((index * 7) % 251) as u8,
            a: 255,
        };
        item.computed_style.border_color = Color {
            r: ((index * 11) % 251) as u8,
            g: ((index * 13) % 251) as u8,
            b: ((index * 17) % 251) as u8,
            a: 255,
        };
        item.computed_style.color = Color {
            r: ((index * 19) % 251) as u8,
            g: ((index * 23) % 251) as u8,
            b: ((index * 29) % 251) as u8,
            a: 255,
        };
        if index % 4 == 0 {
            item.computed_style.background_paint =
                BackgroundPaint::LinearGradient(StyleLinearGradient {
                    from: Color::from_hex("#112233").unwrap(),
                    to: Color::from_hex("#445566").unwrap(),
                });
        }
        nodes.push(item);
    }
    let iterations = 50_000;

    let old_started = std::time::Instant::now();
    let mut old_accumulator = 0u64;
    for _ in 0..iterations {
        for item in &nodes {
            old_accumulator = old_accumulator.wrapping_add(std::hint::black_box(
                old_batch_signature(item, DisplayPrimitiveSlot::Background),
            ));
        }
    }
    let old_time = old_started.elapsed();

    let new_started = std::time::Instant::now();
    let mut new_accumulator = 0u64;
    for _ in 0..iterations {
        for item in &nodes {
            new_accumulator = new_accumulator.wrapping_add(std::hint::black_box(batch_signature(
                item,
                DisplayPrimitiveSlot::Background,
            )));
        }
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "display batch signature material: broad {old_time:?}; slot-aware {new_time:?}; ratio {:.1}x; accumulators={old_accumulator:x}/{new_accumulator:x}",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert_ne!(old_accumulator, 0);
    assert_ne!(new_accumulator, 0);
    assert!(new_time < old_time);
}
