//! Library backups (ADR 0021): opening the catalogue safely (a backup before each
//! upgrade, restore from the newest good backup if it is damaged) and taking backups
//! while the app runs.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use app_core::{BackupInfo, BackupKind, BackupStore, Catalogue, CatalogueError};
use tauri::{AppHandle, Manager};

use crate::AppState;

/// First check after start-up: late enough never to slow the launch.
const FIRST_CHECK: Duration = Duration::from_secs(30);
const CHECK_EVERY: Duration = Duration::from_secs(15 * 60);
/// A backup at start-up if the newest is older than this.
const STALE_AFTER_MS: i64 = 12 * 60 * 60 * 1000;
/// While running: at most hourly, and only if something changed.
const MIN_INTERVAL_MS: i64 = 60 * 60 * 1000;

/// Opens the catalogue at `path`, protecting what cannot be rebuilt:
/// - before upgrading an existing catalogue to a new schema, it is backed up;
/// - a damaged catalogue is moved aside and replaced by the newest good backup (or,
///   with no backup, rebuilt empty: folders are re-indexed when opened);
/// - a catalogue from a newer app version is left untouched and an in-memory one used.
///
/// Returns the catalogue and a notice to show once, if something happened.
pub fn open_catalogue(path: &Path, store: &BackupStore) -> (Catalogue, Option<String>) {
    if let Ok(Some(version)) = app_core::schema_version_of(path)
        && version > 0
        && version < app_core::SCHEMA_VERSION
    {
        match store.back_up_file(path, BackupKind::PreUpgrade(version as u32)) {
            Ok(b) => log::info!(
                "backed up the catalogue before upgrading: {}",
                b.path.display()
            ),
            // Upgrades are append-only and transactional; proceed, but say so in the log.
            Err(e) => log::warn!("pre-upgrade backup failed ({e}); upgrading anyway"),
        }
    }
    match Catalogue::open(path) {
        Ok(c) => {
            log::info!("catalogue opened at {}", path.display());
            (c, None)
        }
        Err(CatalogueError::Corrupt(detail)) => {
            let kept = path.with_extension(format!("corrupt-{}.sqlite", now_ms() / 1000));
            log::warn!(
                "catalogue corrupt ({detail}); moving it to {}",
                kept.display()
            );
            let _ = std::fs::rename(path, &kept);
            for suffix in ["-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
            }
            if let Some(backup) = store.latest_valid() {
                match store
                    .restore(&backup, path)
                    .and_then(|()| Catalogue::open(path))
                {
                    Ok(c) => {
                        log::warn!("catalogue restored from {}", backup.path.display());
                        let notice = format!(
                            "The library database was damaged, so it was restored from the backup taken {}. Ratings, flags and edits made after that are lost; your photos are untouched. The damaged file was kept at {}.",
                            ago(now_ms() - backup.taken_at_ms),
                            kept.display()
                        );
                        return (c, Some(notice));
                    }
                    Err(e) => log::error!("restoring {} failed: {e}", backup.path.display()),
                }
            }
            let notice = format!(
                "The library database was damaged and no backup could be restored, so it has been rebuilt. Your photos are untouched; folders are re-indexed when you open them, but ratings, flags and edits are lost. The old file was kept at {}.",
                kept.display()
            );
            match Catalogue::open(path) {
                Ok(c) => (c, Some(notice)),
                Err(e) => in_memory_fallback(&e),
            }
        }
        Err(e) => in_memory_fallback(&e),
    }
}

fn in_memory_fallback(e: &CatalogueError) -> (Catalogue, Option<String>) {
    log::error!("catalogue unavailable ({e}); using a temporary in-memory catalogue");
    let catalogue = Catalogue::open_in_memory().expect("in-memory SQLite always opens");
    let notice = "The library database couldn’t be opened (it may belong to a newer version of the app). Changes this session won’t be saved.";
    (catalogue, Some(notice.to_owned()))
}

/// Starts the background schedule: a backup soon after start-up if the newest is stale,
/// then at most hourly while anything changes.
pub fn start(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("backups".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK);
            let mut first = true;
            loop {
                let Some(state) = app.try_state::<AppState>() else {
                    return;
                };
                if state.quitting.load(Ordering::Relaxed) {
                    return;
                }
                if let Err(e) = tick(&state, first) {
                    log::warn!("scheduled backup failed: {e}");
                }
                first = false;
                std::thread::sleep(CHECK_EVERY);
            }
        });
    if let Err(e) = spawned {
        log::error!("backup schedule not started: {e}");
    }
}

fn tick(state: &AppState, at_start_up: bool) -> Result<(), CatalogueError> {
    if state.catalogue.path().is_none() {
        return Ok(()); // in-memory (self-test, or a catalogue that could not be opened)
    }
    let newest = state.backups.list()?.into_iter().next();
    let age_ms = newest.map(|b| now_ms() - b.taken_at_ms);
    let changed = state.catalogue.change_count() > state.backup_changes.load(Ordering::Relaxed);
    if backup_due(age_ms, changed, at_start_up) {
        back_up(state, BackupKind::Auto)?;
    }
    Ok(())
}

/// Whether a scheduled backup is due, given the age of the newest backup (`None` if
/// there is none) and whether the library changed since the last one.
fn backup_due(newest_age_ms: Option<i64>, changed: bool, at_start_up: bool) -> bool {
    match newest_age_ms {
        None => true,
        Some(age) if at_start_up => age > STALE_AFTER_MS,
        Some(age) => changed && age >= MIN_INTERVAL_MS,
    }
}

/// Takes a backup now and prunes old ones. Serialised: a scheduled backup and
/// "Back up now" never run at the same time.
pub fn back_up(state: &AppState, kind: BackupKind) -> Result<BackupInfo, CatalogueError> {
    let _guard = state.backup_lock.lock().expect("backup lock");
    let changes = state.catalogue.change_count();
    let info = state.backups.back_up(&state.catalogue, kind)?;
    state.backup_changes.store(changes, Ordering::Relaxed);
    let removed = state.backups.prune(now_ms())?;
    log::info!(
        "catalogue backed up to {} ({} KB); {removed} old backup(s) removed",
        info.path.display(),
        info.bytes / 1024
    );
    Ok(info)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// "3 hours ago", for notices.
fn ago(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    match minutes {
        0 => "just now".into(),
        1 => "a minute ago".into(),
        m if m < 60 => format!("{m} minutes ago"),
        m if m < 120 => "an hour ago".into(),
        m if m < 48 * 60 => format!("{} hours ago", m / 60),
        m => format!("{} days ago", m / (24 * 60)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(dir: &Path) -> (std::path::PathBuf, BackupStore) {
        let path = dir.join("catalogue.sqlite");
        let photos = dir.join("Photos");
        std::fs::create_dir_all(&photos).unwrap();
        let cat = Catalogue::open(&path).unwrap();
        cat.add_folder(&photos.canonicalize().unwrap()).unwrap();
        let store = BackupStore::new(dir.join("backups"));
        store.back_up(&cat, BackupKind::Auto).unwrap();
        (path, store)
    }

    #[test]
    fn a_damaged_catalogue_is_restored_and_the_user_told() {
        let dir = fixtures::TempDir::new("app-restore");
        let (path, store) = library(dir.path());
        std::fs::write(&path, vec![0x42; 8192]).unwrap();
        let (cat, notice) = open_catalogue(&path, &store);
        assert_eq!(cat.folders().unwrap().len(), 1, "restored, not rebuilt");
        let notice = notice.expect("a notice");
        assert!(notice.contains("restored from the backup"), "{notice}");
        assert!(
            std::fs::read_dir(dir.path())
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().contains("corrupt-")),
            "the damaged file is kept"
        );
    }

    #[test]
    fn without_a_backup_a_damaged_catalogue_is_rebuilt() {
        let dir = fixtures::TempDir::new("app-rebuild");
        let path = dir.path().join("catalogue.sqlite");
        std::fs::write(&path, vec![0x42; 8192]).unwrap();
        let store = BackupStore::new(dir.path().join("backups"));
        let (cat, notice) = open_catalogue(&path, &store);
        assert!(cat.folders().unwrap().is_empty());
        assert!(notice.unwrap().contains("rebuilt"));
    }

    #[test]
    fn an_up_to_date_catalogue_opens_without_a_backup_or_notice() {
        let dir = fixtures::TempDir::new("app-open");
        let (path, store) = library(dir.path());
        let before = store.list().unwrap().len();
        let (_cat, notice) = open_catalogue(&path, &store);
        assert_eq!(notice, None);
        assert_eq!(
            store.list().unwrap().len(),
            before,
            "no pre-upgrade backup needed"
        );
    }

    #[test]
    fn scheduled_backups_are_due_when_stale_or_after_an_hour_of_changes() {
        let hour = 3_600_000;
        assert!(backup_due(None, false, true), "never backed up");
        assert!(backup_due(None, false, false));
        assert!(
            backup_due(Some(13 * hour), false, true),
            "stale at start-up"
        );
        assert!(
            !backup_due(Some(2 * hour), true, true),
            "recent enough at start-up"
        );
        assert!(backup_due(Some(hour), true, false), "changed, an hour on");
        assert!(!backup_due(Some(hour / 2), true, false), "at most hourly");
        assert!(!backup_due(Some(5 * hour), false, false), "nothing changed");
    }

    #[test]
    fn times_read_naturally() {
        assert_eq!(ago(30_000), "just now");
        assert_eq!(ago(5 * 60_000), "5 minutes ago");
        assert_eq!(ago(3 * 3_600_000), "3 hours ago");
        assert_eq!(ago(3 * 86_400_000), "3 days ago");
    }
}
