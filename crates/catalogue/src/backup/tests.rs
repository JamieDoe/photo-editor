use super::*;
use crate::{Flag, MarkChange, ScanId, SourceIdentity};

struct Lib {
    dir: fixtures::TempDir,
    cat: Catalogue,
    photo: crate::PhotoId,
}

/// A file-backed catalogue with one picked photo.
fn library(label: &str) -> Lib {
    let dir = fixtures::TempDir::new(label);
    let photos = dir.path().join("Photos");
    std::fs::create_dir_all(&photos).unwrap();
    let file = photos.join("a.nef");
    std::fs::write(&file, vec![7u8; 4096]).unwrap();
    let cat = Catalogue::open(&dir.path().join("catalogue.sqlite")).unwrap();
    let folder = cat.add_folder(&photos.canonicalize().unwrap()).unwrap();
    let (photo, _) = cat
        .record_file(
            folder,
            &SourceIdentity::from_path(&file).unwrap(),
            ScanId(1),
        )
        .unwrap();
    cat.set_marks(&[photo], MarkChange::Flag(Flag::Pick))
        .unwrap();
    Lib { dir, cat, photo }
}

fn info(ms: i64, kind: BackupKind) -> BackupInfo {
    BackupInfo {
        path: PathBuf::from(format!("/b/{ms}-{}", kind.tag())),
        taken_at_ms: ms,
        kind,
        bytes: 1,
    }
}

#[test]
fn a_backup_is_a_complete_consistent_copy_including_recent_writes() {
    let l = library("backup-copy");
    // Just written, still in the WAL (not checkpointed into the main file).
    l.cat
        .set_edit(l.photo, Some((1, r#"{"exposure":1.0}"#)))
        .unwrap();
    let store = BackupStore::new(l.dir.path().join("backups"));
    let b = store.back_up(&l.cat, BackupKind::Manual).unwrap();
    assert!(b.bytes > 0);
    assert_eq!(b.kind, BackupKind::Manual);

    let copy = Catalogue::open(&b.path).unwrap();
    assert_eq!(copy.photo_count().unwrap(), 1);
    assert_eq!(copy.marks(l.photo).unwrap().flag, Flag::Pick);
    assert_eq!(
        copy.edit_of(l.photo).unwrap().map(|e| e.json),
        Some(r#"{"exposure":1.0}"#.to_owned())
    );
    assert!(
        std::fs::read_dir(store.dir())
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().ends_with(".tmp")),
        "no temporary files left"
    );
}

#[test]
fn backups_are_listed_newest_first_and_foreign_files_ignored() {
    let l = library("backup-list");
    let store = BackupStore::new(l.dir.path().join("backups"));
    assert!(store.list().unwrap().is_empty(), "no folder yet: empty");
    let first = store.back_up(&l.cat, BackupKind::Auto).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let second = store.back_up(&l.cat, BackupKind::PreUpgrade(4)).unwrap();
    std::fs::write(store.dir().join("notes.txt"), b"x").unwrap();
    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].path, second.path);
    assert_eq!(listed[0].kind, BackupKind::PreUpgrade(4));
    assert_eq!(listed[1].path, first.path);
}

#[test]
fn a_damaged_catalogue_is_restored_from_the_newest_good_backup() {
    let l = library("backup-restore");
    let store = BackupStore::new(l.dir.path().join("backups"));
    store.back_up(&l.cat, BackupKind::Auto).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    // A newer backup that is itself damaged must be skipped.
    let bad = store.back_up(&l.cat, BackupKind::Auto).unwrap();
    std::fs::write(&bad.path, b"not a database").unwrap();
    let path = l.cat.path().unwrap().to_path_buf();
    drop(l.cat);
    std::fs::write(&path, vec![0x42; 8192]).unwrap();
    assert!(Catalogue::open(&path).is_err());

    let good = store.latest_valid().expect("a valid backup");
    assert_ne!(good.path, bad.path);
    store.restore(&good, &path).unwrap();
    let cat = Catalogue::open(&path).unwrap();
    assert_eq!(cat.marks(l.photo).unwrap().flag, Flag::Pick);
}

#[test]
fn schema_version_is_read_without_opening_for_writes() {
    let dir = fixtures::TempDir::new("backup-version");
    let path = dir.path().join("c.sqlite");
    assert_eq!(schema_version_of(&path).unwrap(), None);
    drop(Catalogue::open(&path).unwrap());
    assert_eq!(
        schema_version_of(&path).unwrap(),
        Some(crate::SCHEMA_VERSION)
    );
}

#[test]
fn in_memory_catalogues_cannot_be_backed_up() {
    let dir = fixtures::TempDir::new("backup-mem");
    let store = BackupStore::new(dir.path());
    assert!(
        store
            .back_up(&Catalogue::open_in_memory().unwrap(), BackupKind::Auto)
            .is_err()
    );
}

#[test]
fn retention_keeps_recent_daily_and_weekly_backups() {
    let hour = DAY_MS / 24;
    // Midday, so a backup an hour earlier falls on the same calendar day.
    let now = 100 * DAY_MS + 12 * hour;
    let mut all = vec![
        // Today: five hourly backups; the newest three are kept.
        info(now - hour, BackupKind::Auto),
        info(now - 2 * hour, BackupKind::Manual),
        info(now - 3 * hour, BackupKind::Auto),
        info(now - 4 * hour, BackupKind::Auto),
        info(now - 5 * hour, BackupKind::Auto),
    ];
    // Days 2..=6 ago, two per day: the newest of each day is kept.
    for d in 2..=6 {
        all.push(info(now - d * DAY_MS, BackupKind::Auto));
        all.push(info(now - d * DAY_MS - hour, BackupKind::Auto));
    }
    // 8..=34 days ago, daily: one per week is kept.
    for d in 8..=34 {
        all.push(info(now - d * DAY_MS, BackupKind::Auto));
    }
    // Very old: gone.
    all.push(info(now - 60 * DAY_MS, BackupKind::Auto));
    // Pre-upgrade: the newest two, whatever their age.
    all.push(info(now - 90 * DAY_MS, BackupKind::PreUpgrade(3)));
    all.push(info(now - 50 * DAY_MS, BackupKind::PreUpgrade(4)));
    all.push(info(now - 10 * DAY_MS, BackupKind::PreUpgrade(5)));

    let pruned = to_prune(&all, now);
    let kept: Vec<&BackupInfo> = all.iter().filter(|b| !pruned.contains(b)).collect();
    let age = |b: &BackupInfo| (now - b.taken_at_ms) as f64 / DAY_MS as f64;

    // Newest three.
    for h in 1..=3 {
        assert!(kept.iter().any(|b| b.taken_at_ms == now - h * hour));
    }
    // One per recent day, and the older same-day backup dropped.
    for d in 2..=6 {
        assert!(kept.iter().any(|b| b.taken_at_ms == now - d * DAY_MS));
        assert!(
            !kept
                .iter()
                .any(|b| b.taken_at_ms == now - d * DAY_MS - hour)
        );
    }
    let weekly = kept
        .iter()
        .filter(|b| b.kind == BackupKind::Auto && age(b) >= 7.0)
        .count();
    assert!((4..=5).contains(&weekly), "about one per week: {weekly}");
    assert!(!kept.iter().any(|b| b.taken_at_ms == now - 60 * DAY_MS));
    let upgrades: Vec<_> = kept
        .iter()
        .filter(|b| matches!(b.kind, BackupKind::PreUpgrade(_)))
        .map(|b| b.kind)
        .collect();
    assert_eq!(
        upgrades,
        [BackupKind::PreUpgrade(4), BackupKind::PreUpgrade(5)]
    );
    assert!(kept.len() <= 16, "bounded: {}", kept.len());
    // Idempotent: pruning what is kept removes nothing more.
    let kept_owned: Vec<BackupInfo> = kept.into_iter().cloned().collect();
    assert!(to_prune(&kept_owned, now).is_empty());
}

#[test]
fn pruning_deletes_files_on_disk() {
    let l = library("backup-prune");
    let store = BackupStore::new(l.dir.path().join("backups"));
    for _ in 0..5 {
        store.back_up(&l.cat, BackupKind::Auto).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(3));
    }
    // All five were taken "today": only the newest three survive the "newest 3" rule
    // plus the day bucket (which is one of them).
    assert_eq!(store.prune(now_ms()).unwrap(), 2);
    assert_eq!(store.list().unwrap().len(), 3);
}
