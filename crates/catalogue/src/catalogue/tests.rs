use std::path::{Path, PathBuf};

use super::*;

struct Library {
    _dir: fixtures::TempDir,
    root: PathBuf,
    cat: Catalogue,
    folder: FolderId,
}

fn library(label: &str) -> Library {
    let dir = fixtures::TempDir::new(label);
    let root = dir
        .path()
        .join("Photos")
        .canonicalize()
        .unwrap_or_else(|_| {
            std::fs::create_dir_all(dir.path().join("Photos")).unwrap();
            dir.path().join("Photos").canonicalize().unwrap()
        });
    let cat = Catalogue::open_in_memory().unwrap();
    let folder = cat.add_folder(&root).unwrap();
    Library {
        _dir: dir,
        root,
        cat,
        folder,
    }
}

fn write(path: &Path, content: &[u8]) -> SourceIdentity {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).unwrap();
    }
    std::fs::write(path, content).unwrap();
    SourceIdentity::from_path(path).unwrap()
}

fn photo_bytes(seed: u8) -> Vec<u8> {
    (0..200_000u32)
        .map(|i| (i as u8).wrapping_mul(seed).wrapping_add(seed))
        .collect()
}

#[test]
fn new_then_unchanged_on_rescan() {
    let l = library("cat-new");
    let id = write(&l.root.join("a.nef"), &photo_bytes(1));
    let s1 = l.cat.begin_scan().unwrap();
    let (p1, o1) = l.cat.record_file(l.folder, &id, s1).unwrap();
    assert_eq!(o1, RecordOutcome::New);
    let s2 = l.cat.begin_scan().unwrap();
    assert_ne!(s1, s2);
    let (p2, o2) = l.cat.record_file(l.folder, &id, s2).unwrap();
    assert_eq!((p2, o2), (p1, RecordOutcome::Unchanged));
    assert_eq!(l.cat.photo_count().unwrap(), 1);
}

#[test]
fn content_change_in_place_keeps_the_photo() {
    let l = library("cat-changed");
    let path = l.root.join("a.dng");
    let (p1, _) = l
        .cat
        .record_file(l.folder, &write(&path, &photo_bytes(1)), ScanId(1))
        .unwrap();
    let (p2, o) = l
        .cat
        .record_file(l.folder, &write(&path, &photo_bytes(2)), ScanId(2))
        .unwrap();
    assert_eq!((p2, o), (p1, RecordOutcome::Changed));
}

#[test]
fn moved_or_renamed_file_keeps_its_photo() {
    let l = library("cat-moved");
    let old = l.root.join("2024/a.nef");
    let (p1, _) = l
        .cat
        .record_file(l.folder, &write(&old, &photo_bytes(3)), ScanId(1))
        .unwrap();
    let new = l.root.join("Best/renamed.nef");
    std::fs::create_dir_all(new.parent().unwrap()).unwrap();
    std::fs::rename(&old, &new).unwrap();
    let (p2, o) = l
        .cat
        .record_file(
            l.folder,
            &SourceIdentity::from_path(&new).unwrap(),
            ScanId(2),
        )
        .unwrap();
    assert_eq!(p2, p1);
    assert_eq!(o, RecordOutcome::Moved { from: old.clone() });
    let files = l.cat.files_in(&l.root, true).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, new.canonicalize().unwrap());
    assert_eq!(l.cat.photo_count().unwrap(), 1);
}

#[test]
fn copies_are_separate_photos() {
    let l = library("cat-copy");
    let a = l.root.join("a.nef");
    let b = l.root.join("copy of a.nef");
    let (p1, _) = l
        .cat
        .record_file(l.folder, &write(&a, &photo_bytes(4)), ScanId(1))
        .unwrap();
    let (p2, o) = l
        .cat
        .record_file(l.folder, &write(&b, &photo_bytes(4)), ScanId(1))
        .unwrap();
    assert_eq!(
        o,
        RecordOutcome::New,
        "the original still exists, so this is a copy"
    );
    assert_ne!(p1, p2);
}

#[test]
fn unseen_files_become_missing_and_come_back() {
    let l = library("cat-missing");
    let keep = write(&l.root.join("keep.nef"), &photo_bytes(5));
    let gone = write(&l.root.join("sub/gone.nef"), &photo_bytes(6));
    let s1 = l.cat.begin_scan().unwrap();
    l.cat
        .record_files(l.folder, &[keep.clone(), gone.clone()], s1)
        .unwrap();
    let s2 = l.cat.begin_scan().unwrap();
    l.cat.record_file(l.folder, &keep, s2).unwrap();
    assert_eq!(l.cat.finish_scan(l.folder, s2, None).unwrap(), 1);
    let status = |cat: &Catalogue| {
        cat.files_in(&l.root, true)
            .unwrap()
            .into_iter()
            .map(|f| (f.path.file_name().unwrap().to_owned(), f.status))
            .collect::<Vec<_>>()
    };
    assert!(status(&l.cat).contains(&("gone.nef".into(), FileStatus::Missing)));
    // Reappears (e.g. drive reconnected): present again, same photo count.
    let s3 = l.cat.begin_scan().unwrap();
    l.cat.record_files(l.folder, &[keep, gone], s3).unwrap();
    assert_eq!(l.cat.finish_scan(l.folder, s3, None).unwrap(), 0);
    assert!(
        status(&l.cat)
            .iter()
            .all(|(_, s)| *s == FileStatus::Present)
    );
    assert_eq!(l.cat.photo_count().unwrap(), 2);
}

#[test]
fn partial_rescan_only_affects_its_subtree() {
    let l = library("cat-partial");
    let a = write(&l.root.join("A/1.nef"), &photo_bytes(7));
    let b = write(&l.root.join("B/2.nef"), &photo_bytes(8));
    l.cat.record_files(l.folder, &[a, b], ScanId(1)).unwrap();
    // Rescan only A, finding nothing: only A's file goes missing.
    let n = l
        .cat
        .finish_scan(l.folder, ScanId(2), Some(&l.root.join("A")))
        .unwrap();
    assert_eq!(n, 1);
    let b_status = l.cat.files_in(&l.root.join("B"), false).unwrap()[0].status;
    assert_eq!(b_status, FileStatus::Present);
}

#[test]
fn files_in_lists_direct_children_or_the_whole_tree() {
    let l = library("cat-list");
    let ids: Vec<_> = ["x.nef", "d/y.nef", "d/e/z.nef", "dd/w.nef"]
        .iter()
        .enumerate()
        .map(|(i, p)| write(&l.root.join(p), &photo_bytes(10 + i as u8)))
        .collect();
    l.cat.record_files(l.folder, &ids, ScanId(1)).unwrap();
    let names = |v: Vec<FileRecord>| {
        v.into_iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(l.cat.files_in(&l.root.join("d"), false).unwrap()),
        ["y.nef"]
    );
    // "dd" must not match the "d" prefix.
    assert_eq!(
        names(l.cat.files_in(&l.root.join("d"), true).unwrap()),
        ["z.nef", "y.nef"]
    );
    assert_eq!(l.cat.files_in(&l.root, true).unwrap().len(), 4);
}

#[test]
fn removing_a_folder_removes_its_photos_but_not_files_on_disk() {
    let l = library("cat-remove");
    let path = l.root.join("a.nef");
    l.cat
        .record_file(l.folder, &write(&path, &photo_bytes(20)), ScanId(1))
        .unwrap();
    l.cat.remove_folder(l.folder).unwrap();
    assert_eq!(l.cat.photo_count().unwrap(), 0);
    assert!(l.cat.folders().unwrap().is_empty());
    assert!(path.exists());
}

#[test]
fn add_folder_is_idempotent() {
    let l = library("cat-folders");
    assert_eq!(l.cat.add_folder(&l.root).unwrap(), l.folder);
    assert_eq!(l.cat.folders().unwrap().len(), 1);
}

#[test]
fn persists_across_reopen() {
    let dir = fixtures::TempDir::new("cat-persist");
    let db = dir.path().join("cat/catalogue.sqlite");
    let root = dir.path().canonicalize().unwrap();
    let id = write(&root.join("a.nef"), &photo_bytes(30));
    {
        let cat = Catalogue::open(&db).unwrap();
        let f = cat.add_folder(&root).unwrap();
        cat.record_file(f, &id, ScanId(1)).unwrap();
    }
    let cat = Catalogue::open(&db).unwrap();
    assert_eq!(cat.photo_count().unwrap(), 1);
    assert_eq!(cat.folders().unwrap()[0].path, root);
}

#[test]
fn corrupt_database_is_detected() {
    let dir = fixtures::TempDir::new("cat-corrupt");
    let db = dir.path().join("catalogue.sqlite");
    std::fs::write(&db, vec![0x5au8; 64 * 1024]).unwrap();
    match Catalogue::open(&db) {
        Err(CatalogueError::Corrupt(_)) => {}
        Err(e) => panic!("expected Corrupt, got {e}"),
        Ok(_) => panic!("garbage opened as a catalogue"),
    }
}

#[test]
fn batch_recording_scales() {
    let l = library("cat-batch");
    // Identities need real files for move detection only; synthesise them here.
    let ids: Vec<SourceIdentity> = (0..5_000u64)
        .map(|i| SourceIdentity {
            canonical_path: l.root.join(format!("d{}/IMG_{i:05}.NEF", i / 500)),
            size: 20_000_000 + i,
            modified_unix_ns: 1_700_000_000_000_000_000 + u128::from(i),
            fingerprint: i.wrapping_mul(0x9E37_79B9_7F4A_7C15),
        })
        .collect();
    let t = std::time::Instant::now();
    let out = l.cat.record_files(l.folder, &ids, ScanId(1)).unwrap();
    let first = t.elapsed();
    assert!(out.iter().all(|(_, o)| *o == RecordOutcome::New));
    let t = std::time::Instant::now();
    let again = l.cat.record_files(l.folder, &ids, ScanId(2)).unwrap();
    assert!(again.iter().all(|(_, o)| *o == RecordOutcome::Unchanged));
    eprintln!(
        "5000 files: first index {first:?}, rescan {:?}",
        t.elapsed()
    );
    assert!(first.as_secs() < 10, "indexing 5000 files took {first:?}");
}

#[test]
fn touch_unchanged_marks_known_files_without_fingerprinting() {
    let l = library("cat-touch");
    let a = write(&l.root.join("a.nef"), &photo_bytes(40));
    l.cat.record_file(l.folder, &a, ScanId(1)).unwrap();
    let unknown = l.root.join("b.nef");
    let seen = l
        .cat
        .touch_unchanged(
            &[
                (a.canonical_path.clone(), a.size, a.modified_unix_ns),
                (a.canonical_path.clone(), a.size + 1, a.modified_unix_ns), // size changed
                (unknown, 1, 1),
            ],
            ScanId(2),
        )
        .unwrap();
    assert_eq!(seen, [true, false, false]);
    // Touched in scan 2, so finishing scan 2 marks nothing missing.
    assert_eq!(l.cat.finish_scan(l.folder, ScanId(2), None).unwrap(), 0);
}

#[test]
fn from_known_matches_from_path() {
    let dir = fixtures::TempDir::new("cat-from-known");
    let path = dir.path().join("x.nef");
    std::fs::write(&path, photo_bytes(41)).unwrap();
    let full = SourceIdentity::from_path(&path).unwrap();
    let (size, modified) = SourceIdentity::stat(&full.canonical_path).unwrap();
    assert_eq!(
        SourceIdentity::from_known(full.canonical_path.clone(), size, modified).unwrap(),
        full
    );
}
