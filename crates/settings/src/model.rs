use serde::{Deserialize, Serialize};

/// Current settings schema version.
pub const SETTINGS_VERSION: u32 = 1;

/// Most recent folders remembered by the library.
pub const MAX_RECENT_FOLDERS: usize = 10;

/// All settings. Missing fields take defaults and unknown fields are ignored, so files
/// written by older or newer versions still load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Settings {
    pub version: u32,
    pub general: GeneralSettings,
    pub performance: PerformanceSettings,
    pub library: LibrarySettings,
    pub export: ExportSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            general: GeneralSettings::default(),
            performance: PerformanceSettings::default(),
            library: LibrarySettings::default(),
            export: ExportSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Theme {
    /// Follow the operating system's light/dark appearance.
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GeneralSettings {
    pub theme: Theme,
}

/// How much of the machine background work (export, later indexing) may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum BackgroundIntensity {
    /// A quarter of the CPU threads: quietest, slowest exports.
    Low,
    /// Half of the CPU threads.
    #[default]
    Balanced,
    /// All but one CPU thread: fastest exports, editing may feel slower meanwhile.
    High,
}

impl BackgroundIntensity {
    /// Compute threads for background work on a machine with `cpu_threads`.
    pub fn threads(self, cpu_threads: usize) -> usize {
        let n = cpu_threads.max(1);
        match self {
            Self::Low => n / 4,
            Self::Balanced => n / 2,
            Self::High => n - 1,
        }
        .max(1)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PerformanceSettings {
    /// Memory budget for rendered previews, in MiB.
    pub preview_cache_mb: u32,
    pub background_intensity: BackgroundIntensity,
}

impl PerformanceSettings {
    pub const PREVIEW_CACHE_MIN_MB: u32 = 64;
    pub const PREVIEW_CACHE_MAX_MB: u32 = 4096;
}

impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            preview_cache_mb: 256,
            background_intensity: BackgroundIntensity::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LibrarySettings {
    /// Folder the library opens at start-up, if set.
    pub default_folder: Option<String>,
    /// Most recently opened folders, newest first.
    pub recent_folders: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ExportSettings {
    /// JPEG quality, 1-100.
    pub jpeg_quality: u8,
}

impl ExportSettings {
    pub const JPEG_QUALITY_MIN: u8 = 50;
    pub const JPEG_QUALITY_MAX: u8 = 100;
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self { jpeg_quality: 92 }
    }
}

impl Settings {
    /// Clamps every value into range and normalises lists. Applied on load and on every
    /// update, so the rest of the app never sees invalid settings.
    pub fn sanitized(mut self) -> Self {
        self.version = SETTINGS_VERSION;
        let p = &mut self.performance;
        p.preview_cache_mb = p.preview_cache_mb.clamp(
            PerformanceSettings::PREVIEW_CACHE_MIN_MB,
            PerformanceSettings::PREVIEW_CACHE_MAX_MB,
        );
        self.export.jpeg_quality = self.export.jpeg_quality.clamp(
            ExportSettings::JPEG_QUALITY_MIN,
            ExportSettings::JPEG_QUALITY_MAX,
        );
        let lib = &mut self.library;
        lib.default_folder = lib.default_folder.take().filter(|f| !f.trim().is_empty());
        let mut seen = std::collections::HashSet::new();
        lib.recent_folders
            .retain(|f| !f.trim().is_empty() && seen.insert(f.clone()));
        lib.recent_folders.truncate(MAX_RECENT_FOLDERS);
        self
    }

    /// Records `folder` as the most recently used (moves it to the front).
    pub fn with_recent_folder(mut self, folder: &str) -> Self {
        self.library.recent_folders.retain(|f| f != folder);
        self.library.recent_folders.insert(0, folder.to_owned());
        self.sanitized()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert_eq!(Settings::default().sanitized(), Settings::default());
    }

    #[test]
    fn sanitize_clamps_and_dedupes() {
        let mut s = Settings::default();
        s.performance.preview_cache_mb = 1;
        s.export.jpeg_quality = 250;
        s.library.default_folder = Some("  ".into());
        s.library.recent_folders = vec!["/a".into(), "".into(), "/a".into(), "/b".into()];
        let s = s.sanitized();
        assert_eq!(s.performance.preview_cache_mb, 64);
        assert_eq!(s.export.jpeg_quality, 100);
        assert_eq!(s.library.default_folder, None);
        assert_eq!(s.library.recent_folders, vec!["/a", "/b"]);
    }

    #[test]
    fn recent_folders_are_mru_and_bounded() {
        let mut s = Settings::default();
        for i in 0..15 {
            s = s.with_recent_folder(&format!("/f{i}"));
        }
        s = s.with_recent_folder("/f3");
        assert_eq!(s.library.recent_folders.len(), MAX_RECENT_FOLDERS);
        assert_eq!(s.library.recent_folders[0], "/f3");
        assert_eq!(s.library.recent_folders[1], "/f14");
    }

    #[test]
    fn older_and_newer_files_still_load() {
        // Missing sections (older file) take defaults.
        let s: Settings =
            serde_json::from_str(r#"{"version":1,"general":{"theme":"dark"}}"#).unwrap();
        assert_eq!(s.general.theme, Theme::Dark);
        assert_eq!(s.export, ExportSettings::default());
        // Unknown fields (newer file) are ignored.
        let s: Settings = serde_json::from_str(
            r#"{"version":7,"future":{"x":1},"export":{"jpegQuality":80,"format":"avif"}}"#,
        )
        .unwrap();
        assert_eq!(s.export.jpeg_quality, 80);
    }

    #[test]
    fn background_threads_scale_with_machine() {
        assert_eq!(BackgroundIntensity::Low.threads(10), 2);
        assert_eq!(BackgroundIntensity::Balanced.threads(10), 5);
        assert_eq!(BackgroundIntensity::High.threads(10), 9);
        for i in [
            BackgroundIntensity::Low,
            BackgroundIntensity::Balanced,
            BackgroundIntensity::High,
        ] {
            assert_eq!(i.threads(1), 1, "{i:?} on a single-core machine");
        }
    }
}
