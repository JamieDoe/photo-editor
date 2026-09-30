//! Application state shared by all commands.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};

use app_core::BackupStore;
use std::sync::{Arc, Mutex};

use app_core::{CancelToken, Catalogue, Engine, EngineConfig};
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
    /// Library backups (ADR 0021).
    pub backups: BackupStore,
    /// `Catalogue::change_count` at the last backup: later changes make one due.
    pub backup_changes: AtomicU64,
    /// Serialises backups (scheduled and "Back up now").
    pub backup_lock: Mutex<()>,
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
    /// Photos waiting to be exported, one after another (ADR 0050).
    pub export_queue: crate::export_queue::ExportQueue,
    /// Set once the user confirmed quitting, so the resulting exit is not intercepted.
    pub quitting: AtomicBool,
}

impl AppState {
    /// Loads settings, opens the catalogue and builds the engine.
    pub fn new(
        settings_path: PathBuf,
        catalogue_path: PathBuf,
        thumbnail_dir: PathBuf,
        backups_dir: PathBuf,
        self_test: Option<PathBuf>,
    ) -> Self {
        let backups = BackupStore::new(backups_dir);
        let (settings, outcome) = SettingsStore::load(settings_path);
        // Self-test runs must never touch the user's library.
        let (catalogue, catalogue_notice) = if self_test.is_some() {
            (
                Catalogue::open_in_memory().expect("in-memory SQLite always opens"),
                None,
            )
        } else {
            let copy =
                match crate::backups::copy_target(settings.get().backups.copy_folder.as_deref()) {
                    crate::backups::CopyTarget::Ready(store) => Some(store),
                    _ => None,
                };
            crate::backups::open_catalogue(&catalogue_path, &backups, copy.as_ref())
        };
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
            backups,
            backup_changes: AtomicU64::new(0),
            backup_lock: Mutex::new(()),
            indexing: Mutex::new(HashMap::new()),
            engine: Engine::new(EngineConfig {
                thumbnail_cache_dir: Some(thumbnail_dir),
                ..engine_config(&current)
            }),
            startup_background_intensity: current.performance.background_intensity,
            settings_recovered_from: match outcome {
                LoadOutcome::Recovered { backup } => Some(backup),
                _ => None,
            },
            settings,
            self_test,
            next_export_id: AtomicU64::new(1),
            exports: Mutex::new(HashMap::new()),
            export_queue: Default::default(),
            quitting: AtomicBool::new(false),
        }
    }

    pub fn restart_required(&self) -> bool {
        self.settings.get().performance.background_intensity != self.startup_background_intensity
    }
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
