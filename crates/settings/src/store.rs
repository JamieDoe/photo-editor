use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::Settings;

/// What happened when settings were loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadOutcome {
    /// No settings file yet: defaults are in use.
    Fresh,
    Loaded,
    /// The file could not be read or parsed. It was moved aside to `backup` and
    /// defaults are in use.
    Recovered {
        backup: PathBuf,
    },
}

/// In-memory settings backed by a JSON file, written atomically on every update.
pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// Loads settings from `path`. Never fails: problems fall back to defaults and are
    /// reported through the outcome and the log.
    pub fn load(path: impl Into<PathBuf>) -> (Self, LoadOutcome) {
        let path = path.into();
        let (settings, outcome) = match std::fs::read(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (Settings::default(), LoadOutcome::Fresh)
            }
            Err(e) => {
                log::warn!("settings unreadable at {}: {e}", path.display());
                (Settings::default(), recover(&path))
            }
            Ok(bytes) => match serde_json::from_slice::<Settings>(&bytes) {
                Ok(s) => (s.sanitized(), LoadOutcome::Loaded),
                Err(e) => {
                    log::warn!("settings corrupt at {}: {e}", path.display());
                    (Settings::default(), recover(&path))
                }
            },
        };
        (
            Self {
                path,
                current: Mutex::new(settings),
            },
            outcome,
        )
    }

    pub fn get(&self) -> Settings {
        self.current.lock().expect("settings lock").clone()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Validates, persists and adopts `settings`. Returns the sanitised settings actually
    /// stored. On a write error the in-memory settings are left unchanged.
    pub fn update(&self, settings: Settings) -> std::io::Result<Settings> {
        let settings = settings.sanitized();
        let json = serde_json::to_vec_pretty(&settings).expect("settings serialise");
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        platform::fs::write_atomic(&self.path, &json)?;
        *self.current.lock().expect("settings lock") = settings.clone();
        Ok(settings)
    }

    /// Applies `change` to the current settings and persists the result.
    pub fn modify(&self, change: impl FnOnce(Settings) -> Settings) -> std::io::Result<Settings> {
        self.update(change(self.get()))
    }
}

/// Moves an unreadable settings file aside so the user's data is not lost and the app
/// can start with defaults.
fn recover(path: &Path) -> LoadOutcome {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let backup = path.with_extension(format!("corrupt-{stamp}.json"));
    match std::fs::rename(path, &backup) {
        Ok(()) => LoadOutcome::Recovered { backup },
        Err(e) => {
            log::warn!("could not back up unreadable settings: {e}");
            LoadOutcome::Recovered {
                backup: path.to_path_buf(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Theme;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = fixtures::TempDir::new("settings-fresh");
        let (store, outcome) = SettingsStore::load(dir.path().join("settings.json"));
        assert_eq!(outcome, LoadOutcome::Fresh);
        assert_eq!(store.get(), Settings::default());
    }

    #[test]
    fn update_persists_and_reloads() {
        let dir = fixtures::TempDir::new("settings-roundtrip");
        let path = dir.path().join("nested/settings.json");
        let (store, _) = SettingsStore::load(&path);
        let saved = store
            .modify(|mut s| {
                s.general.theme = Theme::Light;
                s.export.jpeg_quality = 10; // clamped
                s
            })
            .unwrap();
        assert_eq!(saved.export.jpeg_quality, 50);
        let (reloaded, outcome) = SettingsStore::load(&path);
        assert_eq!(outcome, LoadOutcome::Loaded);
        assert_eq!(reloaded.get(), saved);
    }

    #[test]
    fn corrupt_file_is_backed_up_and_defaults_used() {
        let dir = fixtures::TempDir::new("settings-corrupt");
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{ not json").unwrap();
        let (store, outcome) = SettingsStore::load(&path);
        assert_eq!(store.get(), Settings::default());
        let LoadOutcome::Recovered { backup } = outcome else {
            panic!("expected recovery")
        };
        assert_eq!(
            std::fs::read(&backup).unwrap(),
            b"{ not json",
            "user data preserved"
        );
        assert!(!path.exists());
        // The app can save normally afterwards.
        store.update(Settings::default()).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn failed_write_keeps_previous_settings() {
        let dir = fixtures::TempDir::new("settings-unwritable");
        // A regular file where the settings directory should be makes the write fail.
        let blocker = dir.path().join("config");
        std::fs::write(&blocker, b"not a directory").unwrap();
        let path = blocker.join("settings.json");
        let (store, _) = SettingsStore::load(&path);
        let before = store.get();
        let mut next = before.clone();
        next.general.theme = Theme::Dark;
        assert!(store.update(next).is_err());
        assert_eq!(store.get(), before);
    }
}
