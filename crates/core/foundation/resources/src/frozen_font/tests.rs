use super::*;

fn face(source: fontdb::Source, family: &str, index: u32) -> fontdb::FaceInfo {
    fontdb::FaceInfo {
        id: fontdb::ID::dummy(),
        source,
        index,
        families: vec![(family.into(), fontdb::Language::English_UnitedStates)],
        post_script_name: format!("{family}-Regular"),
        style: fontdb::Style::Normal,
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        monospaced: false,
    }
}

#[test]
fn files_are_owned_and_collection_sources_share_one_reservation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("collection.font");
    std::fs::write(&path, b"font bytes").unwrap();
    let mut original = fontdb::Database::new();
    let first = original.push_face_info(face(fontdb::Source::File(path.clone()), "First", 0));
    let second = original.push_face_info(face(fontdb::Source::File(path.clone()), "Second", 1));
    original.set_sans_serif_family("Second");
    original.set_serif_family("First");
    let budget = ResourceByteBudget::new(10);
    let frozen =
        FrozenFontDatabase::prepare(&original, &budget, &ResourcePreparationToken::new()).unwrap();
    assert_eq!(frozen.bytes(), 10);
    assert_eq!(budget.used_bytes(), 10);
    std::fs::remove_file(path).unwrap();
    let clone = frozen.database();
    assert_eq!(clone.family_name(&fontdb::Family::SansSerif), "Second");
    assert_eq!(clone.family_name(&fontdb::Family::Serif), "First");
    for (old, index) in [(first, 0), (second, 1)] {
        let id = frozen.face_id(old).unwrap();
        assert!(matches!(
            clone.face(id).unwrap().source,
            fontdb::Source::Binary(_)
        ));
        assert_eq!(
            clone.with_face_data(id, |bytes, actual_index| (bytes.to_vec(), actual_index)),
            Some((b"font bytes".to_vec(), index))
        );
    }
    drop(frozen);
    assert_eq!(budget.used_bytes(), 10);
    drop(clone);
    assert_eq!(budget.used_bytes(), 0);
}

#[test]
fn binary_sources_are_copied_and_remapped_without_reparsing_metadata() {
    let bytes: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(b"memory font".to_vec());
    let mut original = fontdb::Database::new();
    let removed = original.push_face_info(face(
        fontdb::Source::Binary(Arc::clone(&bytes)),
        "Removed",
        0,
    ));
    original.remove_face(removed);
    let old_id =
        original.push_face_info(face(fontdb::Source::Binary(Arc::clone(&bytes)), "Alias", 7));
    let budget = ResourceByteBudget::new(11);
    let frozen =
        FrozenFontDatabase::prepare(&original, &budget, &ResourcePreparationToken::new()).unwrap();
    assert_eq!(frozen.face_id(removed), None);
    let database = frozen.database();
    let id = frozen.face_id(old_id).unwrap();
    assert_eq!(database.face(id).unwrap().families[0].0, "Alias");
    database
        .with_face_data(id, |copy, index| {
            assert_eq!(copy, bytes.as_ref().as_ref());
            assert_ne!(copy.as_ptr(), bytes.as_ref().as_ref().as_ptr());
            assert_eq!(index, 7);
        })
        .unwrap();
    drop(frozen);
    drop(database);
    assert_eq!(budget.used_bytes(), 0);
}

#[test]
fn failure_and_cancellation_release_partial_reservations() {
    let mut original = fontdb::Database::new();
    original.push_face_info(face(
        fontdb::Source::Binary(Arc::new(vec![0; 4])),
        "First",
        0,
    ));
    original.push_face_info(face(
        fontdb::Source::Binary(Arc::new(vec![0; 8])),
        "Second",
        0,
    ));
    let budget = ResourceByteBudget::new(8);
    assert!(matches!(
        FrozenFontDatabase::prepare(&original, &budget, &ResourcePreparationToken::new()),
        Err(FrozenFontError::Budget)
    ));
    assert_eq!(budget.used_bytes(), 0);
    let cancelled = ResourcePreparationToken::new();
    cancelled.cancel();
    assert!(matches!(
        FrozenFontDatabase::prepare(&original, &budget, &cancelled),
        Err(FrozenFontError::Cancelled)
    ));
    assert!(matches!(
        FrozenFontDatabase::prepare(&fontdb::Database::new(), &budget, &cancelled),
        Err(FrozenFontError::Cancelled)
    ));
    assert_eq!(budget.used_bytes(), 0);
}

#[test]
fn non_regular_font_sources_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let mut original = fontdb::Database::new();
    original.push_face_info(face(
        fontdb::Source::File(directory.path().to_path_buf()),
        "Directory",
        0,
    ));
    let budget = ResourceByteBudget::new(1024);
    assert!(matches!(
        FrozenFontDatabase::prepare(&original, &budget, &ResourcePreparationToken::new()),
        Err(FrozenFontError::Asset(
            ResourceAssetError::NotRegularFile { .. }
        ))
    ));
    assert_eq!(budget.used_bytes(), 0);
}

#[test]
fn snapshot_clones_hold_shared_budget_across_threads() {
    let mut original = fontdb::Database::new();
    let old_id = original.push_face_info(face(
        fontdb::Source::Binary(Arc::new(vec![7; 8])),
        "Worker",
        0,
    ));
    let budget = ResourceByteBudget::new(8);
    let token = ResourcePreparationToken::new();
    let frozen = FrozenFontDatabase::prepare(&original, &budget, &token).unwrap();
    let worker_database = frozen.database();
    let id = frozen.face_id(old_id).unwrap();
    drop(frozen);
    assert!(matches!(
        FrozenFontDatabase::prepare(&original, &budget, &token),
        Err(FrozenFontError::Budget)
    ));
    std::thread::spawn(move || {
        assert_eq!(
            worker_database.with_face_data(id, |bytes, _| bytes.to_vec()),
            Some(vec![7; 8])
        );
    })
    .join()
    .unwrap();
    assert_eq!(budget.used_bytes(), 0);
    assert!(FrozenFontDatabase::prepare(&original, &budget, &token).is_ok());
    assert_eq!(budget.used_bytes(), 0);
}
