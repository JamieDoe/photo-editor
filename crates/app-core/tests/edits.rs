//! Saving and loading edit recipes through the catalogue.

use app_core::{Catalogue, EditRecipe, SavedEdit, SourceIdentity, load_edit, save_edit};

fn photo(label: &str) -> (fixtures::TempDir, Catalogue, app_core::PhotoId) {
    let dir = fixtures::TempDir::new(label);
    let path = dir.path().join("a.jpg");
    std::fs::write(&path, fixtures::chart_jpeg(64, 48, 90)).unwrap();
    let cat = Catalogue::open_in_memory().unwrap();
    let folder = cat.add_folder(&dir.path().canonicalize().unwrap()).unwrap();
    let scan = cat.begin_scan().unwrap();
    let (id, _) = cat
        .record_file(folder, &SourceIdentity::from_path(&path).unwrap(), scan)
        .unwrap();
    (dir, cat, id)
}

#[test]
fn saved_recipes_load_back_and_identity_clears_them() {
    let (_dir, cat, id) = photo("edits-roundtrip");
    assert_eq!(load_edit(&cat, id).unwrap(), SavedEdit::None);
    let recipe = EditRecipe {
        exposure: 0.75,
        saturation: -20.0,
        ..EditRecipe::default()
    };
    assert!(save_edit(&cat, id, &recipe).unwrap());
    assert_eq!(
        load_edit(&cat, id).unwrap(),
        SavedEdit::Recipe(Box::new(recipe))
    );
    // Out-of-range values are stored sanitised.
    let wild = EditRecipe {
        exposure: 99.0,
        ..EditRecipe::default()
    };
    save_edit(&cat, id, &wild).unwrap();
    assert_eq!(
        load_edit(&cat, id).unwrap().recipe(),
        Some(wild.sanitized())
    );
    // Back to the original: no edit stored at all.
    assert!(!save_edit(&cat, id, &EditRecipe::default()).unwrap());
    assert_eq!(cat.edit_of(id).unwrap(), None);
}

#[test]
fn an_edit_from_a_newer_version_is_never_overwritten() {
    let (_dir, cat, id) = photo("edits-newer");
    cat.set_edit(
        id,
        Some((app_core::RECIPE_VERSION + 1, r#"{"futureSlider":3}"#)),
    )
    .unwrap();
    assert!(matches!(
        load_edit(&cat, id).unwrap(),
        SavedEdit::TooNew { .. }
    ));
    let err = save_edit(&cat, id, &EditRecipe::default()).unwrap_err();
    assert_eq!(err.kind, app_core::ErrorKind::Unsupported);
    assert_eq!(
        cat.edit_of(id).unwrap().map(|e| e.json),
        Some(r#"{"futureSlider":3}"#.to_owned())
    );
}
