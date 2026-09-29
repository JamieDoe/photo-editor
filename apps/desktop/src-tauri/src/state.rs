//! Application state shared by all commands.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64};

use app_core::{CancelToken, Engine, EngineConfig};
use folders::FolderAccess;
use settings::{BackgroundIntensity, LoadOutcome, Settings, SettingsStore};

pub struct AppState {
    pub engine: Engine,
    pub settings: SettingsStore,
    /// Folders the user granted; the only places the webview may browse or open from.
    pub folders: FolderAccess,
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
    /// Loads settings from `settings_path` and builds the engine from them.
    pub fn new(settings_path: PathBuf, self_test: Option<PathBuf>) -> Self {
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
