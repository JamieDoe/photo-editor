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
    pub backups: BackupSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            general: GeneralSettings::default(),
            performance: PerformanceSettings::default(),
            library: LibrarySettings::default(),
            export: ExportSettings::default(),
            backups: BackupSettings::default(),
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

/// Library backups (ADR 0021).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BackupSettings {
    /// A folder (typically on another drive) that also receives every backup. Set only
    /// through the native folder dialog.
    pub copy_folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ExportSettings {
    /// What exports are written as (ADR 0057).
    pub format: ExportFileFormat,
    /// JPEG quality, 1-100.
    pub jpeg_quality: u8,
    /// The folder exports are saved to (ADR 0050). Set only through the native folder
    /// dialog.
    pub folder: Option<String>,
    /// The exported photos' long edge in pixels; their full size when `None`.
    pub long_edge: Option<u32>,
    /// The export preset last chosen ("web", "social", "full"), if the settings still
    /// match it.
    pub preset: Option<String>,
}

/// An export's file format (ADR 0057): JPEG for sharing, TIFF (16-bit) for printing and
/// further editing, PNG (lossless 8-bit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ExportFileFormat {
    #[default]
    Jpeg,
    Tiff,
    Png,
}

impl<'de> Deserialize<'de> for ExportFileFormat {
    /// A format this version does not know (written by a newer one) reads as JPEG,
    /// rather than failing the whole settings file.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match serde_json::Value::deserialize(d)?.as_str() {
            Some("tiff") => Self::Tiff,
            Some("png") => Self::Png,
            _ => Self::Jpeg,
        })
    }
}

impl ExportSettings {
    pub const JPEG_QUALITY_MIN: u8 = 50;
    pub const JPEG_QUALITY_MAX: u8 = 100;
    /// Long edges below this are not photos any more.
    pub const LONG_EDGE_MIN: u32 = 256;
    pub const LONG_EDGE_MAX: u32 = 16_384;
}

impl Default for ExportSettings {
    /// As the design's Web preset: 2048 px, quality 85.
    fn default() -> Self {
        Self {
            format: ExportFileFormat::Jpeg,
            jpeg_quality: 85,
            folder: None,
            long_edge: Some(2048),
            preset: Some("web".to_owned()),
        }
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
        self.export.long_edge = self
            .export
            .long_edge
            .map(|e| e.clamp(ExportSettings::LONG_EDGE_MIN, ExportSettings::LONG_EDGE_MAX));
        self.export.folder = self.export.folder.take().filter(|f| !f.trim().is_empty());
        let lib = &mut self.library;
        lib.default_folder = lib.default_folder.take().filter(|f| !f.trim().is_empty());
        let mut seen = std::collections::HashSet::new();
        lib.recent_folders
            .retain(|f| !f.trim().is_empty() && seen.insert(f.clone()));
        lib.recent_folders.truncate(MAX_RECENT_FOLDERS);
        self.backups.copy_folder = self
            .backups
            .copy_folder
            .take()
            .filter(|f| !f.trim().is_empty());
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
        assert_eq!(
            s.export.format,
            ExportFileFormat::Jpeg,
            "unknown format: the default"
        );
        let s: Settings =
            serde_json::from_str(r#"{"version":7,"export":{"format":"tiff"}}"#).unwrap();
        assert_eq!(s.export.format, ExportFileFormat::Tiff);
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
