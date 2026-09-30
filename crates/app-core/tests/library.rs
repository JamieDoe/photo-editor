//! Indexing integration tests: real files on disk, real catalogue, engine job system.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

use app_core::{Catalogue, Engine, EngineConfig, IndexProgress, IndexStage, JobError};

fn photo(path: &Path, seed: u16) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, fixtures::chart_jpeg(64 + seed, 48, 90)).unwrap();
}

struct Setup {
    _dir: fixtures::TempDir,
    root: PathBuf,
    engine: Engine,
    catalogue: Arc<Catalogue>,
}

fn setup(label: &str) -> Setup {
    let dir = fixtures::TempDir::new(label);
    let root = dir.path().join("Photos");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    Setup {
        root,
        engine: Engine::new(EngineConfig::default()),
        catalogue: Arc::new(Catalogue::open_in_memory().unwrap()),
        _dir: dir,
    }
}

impl Setup {
    fn index(&self) -> app_core::IndexSummary {
        self.engine
            .index_folder(Arc::clone(&self.catalogue), self.root.clone(), |_| {})
            .wait()
            .unwrap()
    }
}

#[test]
fn indexes_then_rescans_moves_and_missing() {
    let s = setup("index-flow");
    for i in 0..30u16 {
        photo(&s.root.join(format!("day{}/IMG_{i:03}.jpg", i % 3)), i);
    }
    std::fs::write(s.root.join("notes.txt"), b"not a photo").unwrap();

    let first = s.index();
    assert_eq!(
        (first.found, first.new, first.unchanged, first.missing),
        (30, 30, 0, 0)
    );
    assert_eq!(s.catalogue.photo_count().unwrap(), 30);

    let again = s.index();
    assert_eq!(
        (again.new, again.unchanged),
        (0, 30),
        "rescan uses the fast path"
    );

    // Move one file, delete another, change a third in place.
    std::fs::rename(
        s.root.join("day0/IMG_000.jpg"),
        s.root.join("day1/moved.jpg"),
    )
    .unwrap();
    std::fs::remove_file(s.root.join("day2/IMG_002.jpg")).unwrap();
    photo(&s.root.join("day1/IMG_001.jpg"), 999);

    let third = s.index();
    assert_eq!(third.moved, 1);
    assert_eq!(third.missing, 1);
    assert_eq!(third.changed, 1);
    assert_eq!(third.new, 0);
    assert_eq!(third.unchanged, 27);
    assert_eq!(
        s.catalogue.photo_count().unwrap(),
        30,
        "moved photo kept, missing photo kept"
    );
}

#[test]
fn reports_progress_to_completion() {
    let s = setup("index-progress");
    for i in 0..600u16 {
        photo(&s.root.join(format!("IMG_{i:04}.jpg")), i % 50);
    }
    let seen = Arc::new(Mutex::new(Vec::<IndexProgress>::new()));
    let sink = Arc::clone(&seen);
    let summary = s
        .engine
        .index_folder(Arc::clone(&s.catalogue), s.root.clone(), move |p| {
            sink.lock().unwrap().push(p)
        })
        .wait()
        .unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(summary.found, 600);
    for stage in [IndexStage::Recording, IndexStage::ReadingDetails] {
        let events: Vec<_> = seen.iter().filter(|p| p.stage == stage).collect();
        assert_eq!(events.first().unwrap().processed, 0, "{stage:?}");
        assert_eq!(events.last().unwrap().processed, 600, "{stage:?}");
        assert!(events.windows(2).all(|w| w[0].processed <= w[1].processed));
    }
    // Recording finishes before details start.
    let first_details = seen
        .iter()
        .position(|p| p.stage == IndexStage::ReadingDetails)
        .unwrap();
    assert!(
        seen[..first_details]
            .iter()
            .all(|p| p.stage == IndexStage::Recording)
    );
}

#[test]
fn details_are_read_during_indexing_and_not_again_on_rescan() {
    let s = setup("index-details");
    photo(&s.root.join("a.jpg"), 1);
    photo(&s.root.join("sub/b.jpg"), 2);
    let first = s.index();
    assert_eq!(first.details_read, 2);
    let files = s.catalogue.files_in(&s.root, true).unwrap();
    let details = s
        .catalogue
        .details(files[0].photo)
        .unwrap()
        .expect("details stored");
    assert_eq!((details.width, details.height), (Some(65), Some(48)));
    assert_eq!(
        s.index().details_read,
        0,
        "unchanged photos keep their details"
    );
    photo(&s.root.join("a.jpg"), 40); // new content
    assert_eq!(s.index().details_read, 1);
}

#[test]
fn cancelled_scan_marks_nothing_missing() {
    let s = setup("index-cancel");
    for i in 0..10u16 {
        photo(&s.root.join(format!("IMG_{i}.jpg")), i);
    }
    s.index();
    // Cancel from inside the scan, once it has begun recording: a rescan of ten
    // unchanged files can finish before a cancel from out here arrives. The progress
    // callback waits for the job's token, so the cancel always lands mid-scan.
    let slot: Arc<(Mutex<Option<jobs::CancelToken>>, Condvar)> = Arc::default();
    let in_scan = Arc::clone(&slot);
    let handle = s
        .engine
        .index_folder(Arc::clone(&s.catalogue), s.root.clone(), move |p| {
            if p.stage == IndexStage::Recording && p.processed == 0 {
                let (token, ready) = &*in_scan;
                let token = ready
                    .wait_while(token.lock().unwrap(), |t| t.is_none())
                    .unwrap();
                token.as_ref().unwrap().cancel();
            }
        });
    {
        let (token, ready) = &*slot;
        *token.lock().unwrap() = Some(handle.token().clone());
        ready.notify_all();
    }
    assert_eq!(handle.wait().unwrap_err(), JobError::Cancelled);
    let missing = s
        .catalogue
        .files_in(&s.root, true)
        .unwrap()
        .iter()
        .filter(|f| f.status == catalogue::FileStatus::Missing)
        .count();
    assert_eq!(missing, 0);
}

#[test]
fn unreadable_folder_is_a_user_facing_error() {
    let s = setup("index-missing-root");
    let gone = s.root.join("not-there");
    let err = s
        .engine
        .index_folder(Arc::clone(&s.catalogue), gone, |_| {})
        .wait()
        .unwrap_err();
    assert!(
        matches!(err, JobError::Failed(ref e) if e.message == "This folder could not be read."),
        "{err:?}"
    );
}
