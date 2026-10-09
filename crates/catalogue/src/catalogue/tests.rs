use std::path::{Path, PathBuf};

use super::*;
use crate::{Collection, ColourLabel, Flag, MarkChange, Marks};

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

#[test]
fn details_are_read_once_and_again_after_content_changes() {
    let l = library("cat-details");
    let path = l.root.join("a.nef");
    let (photo, _) = l
        .cat
        .record_file(l.folder, &write(&path, &photo_bytes(50)), ScanId(1))
        .unwrap();
    assert_eq!(
        l.cat.photos_needing_details(l.folder, 10).unwrap(),
        [(photo, path.canonicalize().unwrap())]
    );

    let details = crate::PhotoDetails {
        camera_make: Some("Nikon".into()),
        camera_model: Some("Z 6".into()),
        captured_at: Some("2026-09-24T06:41:12".into()),
        iso: Some(100),
        aperture: Some(8.0),
        width: Some(6048),
        height: Some(4024),
        rotation: 90,
        gps: Some((54.52, -3.01)),
        ..Default::default()
    };
    l.cat
        .set_details(&[(photo, Some(details.clone()))])
        .unwrap();
    assert!(
        l.cat
            .photos_needing_details(l.folder, 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(l.cat.details(photo).unwrap(), Some(details.clone()));
    let in_dir = l.cat.details_in_dir(&l.root).unwrap();
    assert_eq!(in_dir, [(path.canonicalize().unwrap(), details)]);

    // Unchanged rescan keeps them; changed content queues a re-read.
    l.cat
        .record_file(
            l.folder,
            &SourceIdentity::from_path(&path).unwrap(),
            ScanId(2),
        )
        .unwrap();
    assert!(
        l.cat
            .photos_needing_details(l.folder, 10)
            .unwrap()
            .is_empty()
    );
    l.cat
        .record_file(l.folder, &write(&path, &photo_bytes(51)), ScanId(3))
        .unwrap();
    assert_eq!(l.cat.photos_needing_details(l.folder, 10).unwrap().len(), 1);
}

#[test]
fn unreadable_details_are_not_retried_and_missing_files_are_skipped() {
    let l = library("cat-details-none");
    let a = write(&l.root.join("a.nef"), &photo_bytes(52));
    let b = write(&l.root.join("b.nef"), &photo_bytes(53));
    let out = l
        .cat
        .record_files(l.folder, &[a.clone(), b.clone()], ScanId(1))
        .unwrap();
    // `a` could not be read: stored as empty, not queued again.
    l.cat.set_details(&[(out[0].0, None)]).unwrap();
    // `b` disappears: marked missing, so not queued either.
    std::fs::remove_file(&b.canonical_path).unwrap();
    let s2 = l.cat.begin_scan().unwrap();
    l.cat
        .touch_unchanged(
            &[(a.canonical_path.clone(), a.size, a.modified_unix_ns)],
            s2,
        )
        .unwrap();
    l.cat.finish_scan(l.folder, s2, None).unwrap();
    assert!(
        l.cat
            .photos_needing_details(l.folder, 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        l.cat.details(out[0].0).unwrap(),
        Some(crate::PhotoDetails::default())
    );
}

#[test]
fn many_identical_copies_stay_fast() {
    // 2,000 identical copies in one scan: each must be recorded as a new photo without
    // checking all earlier copies on disk (that was quadratic). Real small copies, not
    // hard links: NTFS allows at most 1,023 links per file.
    let l = library("cat-copies");
    let content = &photo_bytes(60)[..8 * 1024];
    write(&l.root.join("orig.nef"), content);
    let ids: Vec<SourceIdentity> = (0..2_000)
        .map(|i| write(&l.root.join(format!("copies/c{i}.nef")), content))
        .collect();
    let t = std::time::Instant::now();
    let out = l.cat.record_files(l.folder, &ids, ScanId(1)).unwrap();
    let elapsed = t.elapsed();
    assert!(out.iter().all(|(_, o)| *o == RecordOutcome::New));
    assert!(elapsed.as_secs_f64() < 2.0, "2,000 copies took {elapsed:?}");

    // A real move among the copies is still detected in the next scan.
    let moved_from = ids[5].canonical_path.clone();
    let moved_to = l.root.join("moved.nef");
    std::fs::rename(&moved_from, &moved_to).unwrap();
    let (_, o) = l
        .cat
        .record_file(
            l.folder,
            &SourceIdentity::from_path(&moved_to).unwrap(),
            ScanId(2),
        )
        .unwrap();
    assert!(matches!(o, RecordOutcome::Moved { .. }), "{o:?}");
}

fn rate(n: u8) -> MarkChange {
    MarkChange::Rating(crate::Rating::new(n).unwrap())
}

#[test]
fn marks_are_set_read_and_listed_per_folder() {
    let l = library("cat-marks");
    let a = l.root.join("a.nef");
    let b = l.root.join("b.nef");
    let c = l.root.join("sub/c.nef");
    let ids: Vec<PhotoId> = [(&a, 1), (&b, 2), (&c, 3)]
        .into_iter()
        .map(|(p, seed)| {
            l.cat
                .record_file(l.folder, &write(p, &photo_bytes(seed)), ScanId(1))
                .unwrap()
                .0
        })
        .collect();
    assert_eq!(
        l.cat.photo_at(&a.canonicalize().unwrap()).unwrap(),
        Some(ids[0])
    );
    assert_eq!(l.cat.photo_at(&l.root.join("nope.nef")).unwrap(), None);

    l.cat.set_marks(&ids[..2], rate(4)).unwrap();
    l.cat
        .set_marks(&ids[1..2], MarkChange::Flag(Flag::Pick))
        .unwrap();
    l.cat
        .set_marks(&ids[2..], MarkChange::Flag(Flag::Reject))
        .unwrap();
    assert_eq!(l.cat.marks(ids[0]).unwrap().rating.stars(), 4);
    assert_eq!(
        l.cat.marks(ids[1]).unwrap(),
        Marks {
            rating: crate::Rating::new(4).unwrap(),
            flag: Flag::Pick,
            label: ColourLabel::None,
        }
    );
    // A rating change leaves the flag alone, and 0 clears the rating.
    l.cat.set_marks(&ids[1..2], rate(0)).unwrap();
    assert_eq!(l.cat.marks(ids[1]).unwrap().flag, Flag::Pick);
    assert_eq!(l.cat.marks(ids[1]).unwrap().rating.stars(), 0);

    let mut here = l.cat.marks_in_dir(&l.root).unwrap();
    here.sort_by(|x, y| x.0.cmp(&y.0));
    let names: Vec<_> = here
        .iter()
        .map(|(p, m)| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                m.rating.stars(),
                m.flag,
            )
        })
        .collect();
    assert_eq!(
        names,
        [
            ("a.nef".to_owned(), 4, Flag::None),
            ("b.nef".to_owned(), 0, Flag::Pick)
        ]
    );
}

#[test]
fn colour_labels_are_a_third_independent_mark() {
    let l = library("cat-labels");
    let ids: Vec<PhotoId> = ["a.nef", "b.nef"]
        .iter()
        .enumerate()
        .map(|(i, name)| {
            l.cat
                .record_file(
                    l.folder,
                    &write(&l.root.join(name), &photo_bytes(i as u8 + 1)),
                    ScanId(1),
                )
                .unwrap()
                .0
        })
        .collect();
    l.cat.set_marks(&ids, rate(3)).unwrap();
    l.cat
        .set_marks(&ids[..1], MarkChange::Label(ColourLabel::Purple))
        .unwrap();
    // Setting a label leaves rating and flag alone, and the reverse.
    l.cat
        .set_marks(&ids[..1], MarkChange::Flag(Flag::Pick))
        .unwrap();
    assert_eq!(
        l.cat.marks(ids[0]).unwrap(),
        Marks {
            rating: crate::Rating::new(3).unwrap(),
            flag: Flag::Pick,
            label: ColourLabel::Purple,
        }
    );
    assert_eq!(l.cat.marks(ids[1]).unwrap().label, ColourLabel::None);
    // A photo with only a label is listed with the folder's marks.
    l.cat.set_marks(&ids, rate(0)).unwrap();
    l.cat.set_marks(&ids, MarkChange::Flag(Flag::None)).unwrap();
    let here = l.cat.marks_in_dir(&l.root).unwrap();
    assert_eq!(here.len(), 1);
    assert_eq!(here[0].1.label, ColourLabel::Purple);
    // Collections and searches carry it.
    l.cat
        .set_marks(&ids[..1], MarkChange::Flag(Flag::Pick))
        .unwrap();
    let picks = l.cat.collection(Collection::Picks).unwrap();
    assert_eq!(picks[0].marks.label, ColourLabel::Purple);
    // Every marked photo, for writing sidecars: the labelled pick, not the other.
    let marked = l.cat.marked().unwrap();
    assert_eq!(marked.len(), 1);
    assert_eq!(marked[0].photo, ids[0]);
    // Marks read from sidecars only go to photos without marks of their own.
    let imported = Marks {
        rating: crate::Rating::new(5).unwrap(),
        flag: Flag::Reject,
        label: ColourLabel::Blue,
    };
    assert_eq!(
        l.cat
            .import_marks(&[(ids[0], imported), (ids[1], imported)])
            .unwrap(),
        1
    );
    assert_eq!(
        l.cat.marks(ids[0]).unwrap().label,
        ColourLabel::Purple,
        "kept its own"
    );
    assert_eq!(l.cat.marks(ids[1]).unwrap(), imported);
    l.cat.set_marks(&ids[1..2], rate(0)).unwrap();
    l.cat
        .set_marks(&ids[1..2], MarkChange::Flag(Flag::None))
        .unwrap();
    l.cat
        .set_marks(&ids[1..2], MarkChange::Label(ColourLabel::None))
        .unwrap();
    // Clearing it.
    l.cat
        .set_marks(&ids[..1], MarkChange::Label(ColourLabel::None))
        .unwrap();
    assert_eq!(l.cat.marks(ids[0]).unwrap().label, ColourLabel::None);
    l.cat
        .set_marks(&ids[..1], MarkChange::Flag(Flag::None))
        .unwrap();
    assert!(l.cat.marked().unwrap().is_empty());
}

#[test]
fn marks_follow_a_moved_photo_and_collections_skip_missing_files() {
    let l = library("cat-marks-move");
    let old = l.root.join("keeper.nef");
    let (photo, _) = l
        .cat
        .record_file(l.folder, &write(&old, &photo_bytes(9)), ScanId(1))
        .unwrap();
    l.cat.set_marks(&[photo], rate(5)).unwrap();
    l.cat
        .set_marks(&[photo], MarkChange::Flag(Flag::Pick))
        .unwrap();
    let other = l.root.join("gone.nef");
    let (gone, _) = l
        .cat
        .record_file(l.folder, &write(&other, &photo_bytes(10)), ScanId(1))
        .unwrap();
    l.cat
        .set_marks(&[gone], MarkChange::Flag(Flag::Reject))
        .unwrap();

    // Move one file, delete the other, rescan.
    let new = l.root.join("Best/keeper.nef");
    std::fs::create_dir_all(new.parent().unwrap()).unwrap();
    std::fs::rename(&old, &new).unwrap();
    std::fs::remove_file(&other).unwrap();
    let (moved, o) = l
        .cat
        .record_file(
            l.folder,
            &SourceIdentity::from_path(&new).unwrap(),
            ScanId(2),
        )
        .unwrap();
    assert_eq!(moved, photo);
    assert!(matches!(o, RecordOutcome::Moved { .. }));
    l.cat.finish_scan(l.folder, ScanId(2), None).unwrap();

    let picks = l.cat.collection(Collection::Picks).unwrap();
    assert_eq!(picks.len(), 1);
    assert_eq!(picks[0].path, new.canonicalize().unwrap());
    assert_eq!(picks[0].marks.rating.stars(), 5);
    assert_eq!(l.cat.collection(Collection::Rated).unwrap().len(), 1);
    // Five stars: a favourite (ADR 0081).
    assert_eq!(
        l.cat.collection(Collection::Favourites).unwrap()[0].photo,
        photo
    );
    // The rejected photo's file is missing: not listed, not counted.
    assert!(l.cat.collection(Collection::Rejected).unwrap().is_empty());
    // All photos: the present one only.
    let all = l.cat.collection(Collection::All).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].path, new.canonicalize().unwrap());
    assert_eq!(
        l.cat.collection_counts().unwrap(),
        crate::CollectionCounts {
            all: 1,
            picks: 1,
            rated: 1,
            rejected: 0,
            // The pick was rated five stars.
            favourites: 1,
            // Both were indexed just now; the missing one is not counted.
            recent: 1,
            edited: 0,
        }
    );
}

#[test]
fn recently_edited_lists_the_latest_edits_first() {
    let l = library("cat-recently-edited");
    let photo = |name: &str| {
        l.cat
            .record_file(
                l.folder,
                &write(&l.root.join(name), &photo_bytes(name.len() as u8)),
                ScanId(1),
            )
            .unwrap()
            .0
    };
    let (a, b) = (photo("a.nef"), photo("bb.nef"));
    // Never edited.
    photo("ccc.nef");
    let old = photo("dddd.nef");
    for p in [a, b, old] {
        l.cat.set_edit(p, Some((30, "{}"))).unwrap();
    }
    // When each was last edited: b before a; `old` six weeks ago.
    let now = crate::catalogue::now_ms();
    let day = 24 * 60 * 60 * 1000;
    for (p, at) in [(a, now - 1000), (b, now - day), (old, now - 42 * day)] {
        l.cat
            .conn()
            .execute(
                "UPDATE edits SET updated_at_ms = ?2 WHERE photo_id = ?1",
                params![p.0, at],
            )
            .unwrap();
    }
    let edited: Vec<PhotoId> = l
        .cat
        .collection(Collection::RecentlyEdited)
        .unwrap()
        .iter()
        .map(|e| e.photo)
        .collect();
    // The latest first; the photo never edited and the one edited long ago are not in it.
    assert_eq!(edited, [a, b]);
    assert_eq!(l.cat.collection_counts().unwrap().edited, 2);
    // Removing an edit takes the photo out.
    l.cat.set_edit(a, None).unwrap();
    assert_eq!(
        l.cat.collection(Collection::RecentlyEdited).unwrap().len(),
        1
    );
}

#[test]
fn collections_include_details_once_indexed() {
    let l = library("cat-marks-details");
    let (photo, _) = l
        .cat
        .record_file(
            l.folder,
            &write(&l.root.join("x.nef"), &photo_bytes(4)),
            ScanId(1),
        )
        .unwrap();
    l.cat.set_marks(&[photo], rate(2)).unwrap();
    assert_eq!(
        l.cat.collection(Collection::Rated).unwrap()[0].details,
        None
    );
    let details = crate::PhotoDetails {
        camera_model: Some("Z 6".into()),
        captured_at: Some("2026-09-24T06:41:12".into()),
        ..Default::default()
    };
    l.cat
        .set_details(&[(photo, Some(details.clone()))])
        .unwrap();
    assert_eq!(
        l.cat.collection(Collection::Rated).unwrap()[0].details,
        Some(details)
    );
}

#[test]
fn edits_are_stored_per_photo_follow_moves_and_can_be_cleared() {
    let l = library("cat-edits");
    let old = l.root.join("a.nef");
    let (photo, _) = l
        .cat
        .record_file(l.folder, &write(&old, &photo_bytes(21)), ScanId(1))
        .unwrap();
    let plain = l.root.join("b.nef");
    l.cat
        .record_file(l.folder, &write(&plain, &photo_bytes(22)), ScanId(1))
        .unwrap();
    assert_eq!(l.cat.edit_of(photo).unwrap(), None);

    l.cat
        .set_edit(photo, Some((1, r#"{"exposure":0.5}"#)))
        .unwrap();
    l.cat
        .set_edit(photo, Some((1, r#"{"exposure":1.0}"#)))
        .unwrap(); // replaces
    let stored = l.cat.edit_of(photo).unwrap().unwrap();
    assert_eq!(
        (stored.recipe_version, stored.json.as_str()),
        (1, r#"{"exposure":1.0}"#)
    );
    let here = l.cat.edits_in(&l.root, false).unwrap();
    assert_eq!(here.len(), 1, "only the edited photo is listed");
    assert_eq!(here[0].0, old.canonicalize().unwrap());

    // Moved into a subfolder: the edit goes with it.
    let new = l.root.join("sub/a.nef");
    std::fs::create_dir_all(new.parent().unwrap()).unwrap();
    std::fs::rename(&old, &new).unwrap();
    l.cat
        .record_file(
            l.folder,
            &SourceIdentity::from_path(&new).unwrap(),
            ScanId(2),
        )
        .unwrap();
    assert!(l.cat.edits_in(&l.root, false).unwrap().is_empty());
    assert_eq!(l.cat.edits_in(&l.root, true).unwrap().len(), 1);
    let at_new = l.cat.edit_at(&new.canonicalize().unwrap()).unwrap();
    assert_eq!(
        at_new.map(|e| e.json),
        Some(r#"{"exposure":1.0}"#.to_owned())
    );
    assert_eq!(l.cat.edit_at(&old).unwrap(), None);

    l.cat.set_edit(photo, None).unwrap();
    assert_eq!(l.cat.edit_of(photo).unwrap(), None);
}

#[test]
fn collections_say_which_photos_are_edited() {
    let l = library("cat-edits-collection");
    let (photo, _) = l
        .cat
        .record_file(
            l.folder,
            &write(&l.root.join("x.nef"), &photo_bytes(23)),
            ScanId(1),
        )
        .unwrap();
    l.cat.set_marks(&[photo], rate(3)).unwrap();
    assert!(!l.cat.collection(Collection::Rated).unwrap()[0].edited);
    l.cat.set_edit(photo, Some((1, "{}"))).unwrap();
    assert!(l.cat.collection(Collection::Rated).unwrap()[0].edited);
}

#[test]
fn albums_hold_photos_follow_moves_and_skip_missing_files() {
    let l = library("cat-albums");
    let rec = |name: &str, seed: u8, scan: i64| {
        l.cat
            .record_file(
                l.folder,
                &write(&l.root.join(name), &photo_bytes(seed)),
                ScanId(scan),
            )
            .unwrap()
            .0
    };
    let (a, b, c) = (
        rec("a.nef", 11, 1),
        rec("b.nef", 12, 1),
        rec("c.nef", 13, 1),
    );
    let portfolio = l.cat.add_album("Portfolio").unwrap();
    let print = l.cat.add_album("to print").unwrap();
    assert_eq!(l.cat.add_to_album(portfolio, &[a, b]).unwrap(), 2);
    // Already in it: not added twice.
    assert_eq!(l.cat.add_to_album(portfolio, &[b, c]).unwrap(), 1);
    l.cat.add_to_album(print, &[a]).unwrap();
    // Listed by name, ignoring case, with counts and a cover.
    let albums = l.cat.albums().unwrap();
    let names: Vec<_> = albums.iter().map(|a| (a.name.as_str(), a.count)).collect();
    assert_eq!(names, [("Portfolio", 3), ("to print", 1)]);
    assert!(albums[0].cover.is_some());

    // Move a, delete c, rescan: the album follows a and leaves out c.
    let new = l.root.join("Moved/a.nef");
    std::fs::create_dir_all(new.parent().unwrap()).unwrap();
    std::fs::rename(l.root.join("a.nef"), &new).unwrap();
    std::fs::remove_file(l.root.join("c.nef")).unwrap();
    for name in ["Moved/a.nef", "b.nef"] {
        l.cat
            .record_file(
                l.folder,
                &SourceIdentity::from_path(&l.root.join(name)).unwrap(),
                ScanId(2),
            )
            .unwrap();
    }
    l.cat.finish_scan(l.folder, ScanId(2), None).unwrap();
    let photos = l.cat.album_photos(portfolio).unwrap();
    let paths: Vec<_> = photos.iter().map(|p| p.path.clone()).collect();
    assert_eq!(paths.len(), 2);
    assert!(paths.contains(&new.canonicalize().unwrap()));
    assert_eq!(l.cat.album(portfolio).unwrap().unwrap().count, 2);

    // Removing from one album leaves the other; renaming; deleting keeps the photos.
    assert_eq!(l.cat.remove_from_album(portfolio, &[a, a]).unwrap(), 1);
    assert_eq!(l.cat.album(print).unwrap().unwrap().count, 1);
    assert!(l.cat.rename_album(print, "Prints").unwrap());
    assert!(l.cat.delete_album(portfolio).unwrap());
    assert!(!l.cat.delete_album(portfolio).unwrap());
    assert_eq!(
        l.cat.add_to_album(portfolio, &[b]).unwrap(),
        0,
        "no such album"
    );
    assert!(l.cat.album(portfolio).unwrap().is_none());
    assert_eq!(l.cat.albums().unwrap()[0].name, "Prints");
    // Deleting an album deletes no photos (the missing one is kept for when it returns).
    assert_eq!(l.cat.photo_count().unwrap(), 3);
}

#[test]
fn search_finds_photos_by_folder_camera_lens_and_date() {
    let l = library("cat-search");
    let rec = |name: &str, seed: u8| {
        l.cat
            .record_file(
                l.folder,
                &write(&l.root.join(name), &photo_bytes(seed)),
                ScanId(1),
            )
            .unwrap()
            .0
    };
    let skye = rec("Isle of Skye/DSC_0001.NEF", 31);
    let lake = rec("Lake District/IMG_0420.JPG", 32);
    let undated = rec("Lake District/scan_50%.tif", 33);
    let details = |camera: &str, lens: &str, at: &str| crate::PhotoDetails {
        camera_make: Some("NIKON CORPORATION".into()),
        camera_model: Some(camera.into()),
        lens: Some(lens.into()),
        captured_at: Some(at.into()),
        ..Default::default()
    };
    l.cat
        .set_details(&[
            (
                skye,
                Some(details(
                    "Z 6",
                    "NIKKOR Z 24-70mm f/4 S",
                    "2025-06-24T21:10:00",
                )),
            ),
            (
                lake,
                Some(details("Z f", "NIKKOR Z 40mm f/2", "2026-09-03T08:00:00")),
            ),
        ])
        .unwrap();
    let found = |q: &str| {
        let mut names: Vec<String> = l
            .cat
            .search(q)
            .unwrap()
            .into_iter()
            .map(|e| e.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    // A place, from the folder; case does not matter.
    assert_eq!(found("skye"), ["DSC_0001.NEF"]);
    assert_eq!(found("LAKE district"), ["IMG_0420.JPG", "scan_50%.tif"]);
    // Camera and lens.
    assert_eq!(found("nikon 40mm"), ["IMG_0420.JPG"]);
    // Dates: a year, a month, a day, an ISO prefix; together, all must match.
    assert_eq!(found("2025"), ["DSC_0001.NEF"]);
    assert_eq!(found("sept"), ["IMG_0420.JPG"]);
    assert_eq!(found("june 24"), ["DSC_0001.NEF"]);
    assert_eq!(found("2026-09"), ["IMG_0420.JPG"]);
    assert!(found("skye 2026").is_empty());
    // A % is a character, not a wildcard; nothing for nothing.
    assert_eq!(found("50%"), ["scan_50%.tif"]);
    assert!(found("  ").is_empty());
    // The folders above the library never match: every photo is under them.
    let above = l
        .root
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(found(&above).is_empty(), "{above}");
    // The library folder's own name does.
    assert_eq!(found("photos").len(), 3);
    let _ = undated;
}
