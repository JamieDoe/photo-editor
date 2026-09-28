//! Application state shared by all commands.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use app_core::{CancelToken, Catalogue, CatalogueError, Engine, EngineConfig};
use folders::FolderAccess;
use settings::{BackgroundIntensity, LoadOutcome, Settings, SettingsStore};

pub struct AppState {
    pub engine: Engine,
    pub settings: SettingsStore,
    /// Folders the user granted; the only places the webview may browse or open from.
    pub folders: FolderAccess,
    /// The library catalogue (SQLite). See ADR 0012.
    pub catalogue: Arc<Catalogue>,
    /// Shown once in the Library if the catalogue had to be reset or is read-only.
    pub catalogue_notice: Option<String>,
    /// Cancel tokens of indexing passes still running, by root folder.
    pub indexing: Mutex<HashMap<String, CancelToken>>,
    /// Settings the engine was built with; changing these needs a restart.
    pub startup_background_intensity: BackgroundIntensity,
    /// Where an unreadable settings file was moved at start-up, if that happened.
    pub settings_recovered_from: Option<PathBuf>,
    /// Set when launched with `PE_SELF_TEST=<image path>` (see docs/PERFORMANCE.md).
    pub self_test: Option<PathBuf>,
    /// Export ids are allocated here (not by the job system) so progress events can
    /// carry the id from the very first event.
    pub next_export_id: AtomicU64,
    /// Cancel tokens of exports still running, by export id (see `lifecycle`).
    pub exports: Mutex<HashMap<u64, CancelToken>>,
    /// Set once the user confirmed quitting, so the resulting exit is not intercepted.
    pub quitting: AtomicBool,
}

impl AppState {
    /// Loads settings, opens the catalogue and builds the engine.
    pub fn new(
        settings_path: PathBuf,
        catalogue_path: PathBuf,
        self_test: Option<PathBuf>,
    ) -> Self {
        // Self-test runs must never touch the user's library.
        let (catalogue, catalogue_notice) = if self_test.is_some() {
            (
                Catalogue::open_in_memory().expect("in-memory SQLite always opens"),
                None,
            )
        } else {
            open_catalogue(&catalogue_path)
        };
        let (settings, outcome) = SettingsStore::load(settings_path);
        match &outcome {
            LoadOutcome::Fresh => log::info!("no settings file yet; using defaults"),
            LoadOutcome::Loaded => log::info!("settings loaded from {}", settings.path().display()),
            LoadOutcome::Recovered { backup } => {
                log::warn!(
                    "settings were unreadable; moved to {} and reset to defaults",
                    backup.display()
                );
            }
        }
        let current = settings.get();
        // Folders remembered in settings were granted through the folder dialog earlier.
        let folders = FolderAccess::new();
        folders.grant_remembered(current.library.default_folder.iter().map(String::as_str));
        folders.grant_remembered(current.library.recent_folders.iter().map(String::as_str));
        Self {
            folders,
            catalogue: Arc::new(catalogue),
            catalogue_notice,
            indexing: Mutex::new(HashMap::new()),
            engine: Engine::new(engine_config(&current)),
            startup_background_intensity: current.performance.background_intensity,
            settings_recovered_from: match outcome {
                LoadOutcome::Recovered { backup } => Some(backup),
                _ => None,
            },
            settings,
            self_test,
            next_export_id: AtomicU64::new(1),
            exports: Mutex::new(HashMap::new()),
            quitting: AtomicBool::new(false),
        }
    }

    pub fn restart_required(&self) -> bool {
        self.settings.get().performance.background_intensity != self.startup_background_intensity
    }
}

/// Opens the catalogue, recovering from damage. It is rebuildable from the library
/// folders, so a corrupt file is moved aside and a fresh one started. A catalogue
/// from a newer app version is left untouched and an in-memory one used instead.
fn open_catalogue(path: &std::path::Path) -> (Catalogue, Option<String>) {
    match Catalogue::open(path) {
        Ok(c) => {
            log::info!("catalogue opened at {}", path.display());
            (c, None)
        }
        Err(CatalogueError::Corrupt(detail)) => {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let backup = path.with_extension(format!("corrupt-{stamp}.sqlite"));
            log::warn!(
                "catalogue corrupt ({detail}); moving it to {} and rebuilding",
                backup.display()
            );
            let _ = std::fs::rename(path, &backup);
            for suffix in ["-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
            }
            let notice = format!(
                "The library database was damaged and has been rebuilt. Your photos are untouched; folders are re-indexed when you open them. The old file was kept at {}.",
                backup.display()
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

/// Engine configuration derived from user settings.
pub fn engine_config(settings: &Settings) -> EngineConfig {
    let cpu_threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut config = EngineConfig {
        preview_cache_bytes: settings.performance.preview_cache_mb as usize * 1024 * 1024,
        ..EngineConfig::default()
    };
    config.jobs.background.compute_threads = Some(
        settings
            .performance
            .background_intensity
            .threads(cpu_threads),
    );
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_config_follows_settings() {
        let mut s = Settings::default();
        s.performance.preview_cache_mb = 512;
        s.performance.background_intensity = BackgroundIntensity::Low;
        let c = engine_config(&s);
        assert_eq!(c.preview_cache_bytes, 512 * 1024 * 1024);
        let cpus = std::thread::available_parallelism().map_or(4, |n| n.get());
        assert_eq!(
            c.jobs.background.compute_threads,
            Some(BackgroundIntensity::Low.threads(cpus))
        );
    }
}
