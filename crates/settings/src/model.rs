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

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LibrarySettings {
    /// Folder the library opens at start-up when the last place can't be reopened.
    pub default_folder: Option<String>,
    /// Most recently opened folders, newest first.
    pub recent_folders: Vec<String>,
    /// How the Library shows photos, remembered between launches (ADR 0065).
    pub view: LibraryViewSettings,
    /// Where the Library was left, reopened at the next launch (ADR 0065). Recorded by
    /// Rust (a folder only once its access is checked), never by a settings update.
    pub last_place: Option<LibraryPlace>,
    /// Whether ratings and labels are also written to `.xmp` sidecars beside RAW files,
    /// for other photo apps (ADR 0067). Off by default: it writes into photo folders.
    pub write_sidecars: bool,
}

impl<'de> Deserialize<'de> for LibrarySettings {
    /// Field by field, like the view: one unreadable value (say, a place kind from a
    /// newer version) is that field's default, not a reset of the whole file.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        Ok(Self {
            default_folder: lenient(&value, "defaultFolder"),
            recent_folders: lenient(&value, "recentFolders"),
            view: lenient(&value, "view"),
            last_place: lenient(&value, "lastPlace"),
            write_sidecars: lenient(&value, "writeSidecars"),
        })
    }
}

/// A place the Library shows (ADR 0065): a folder, a library-wide collection, or an
/// album.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum LibraryPlace {
    Folder {
        path: String,
    },
    Collection {
        collection: LibraryCollection,
    },
    Album {
        // Album ids are small; a JS number holds them exactly.
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        id: i64,
    },
}

/// The library-wide collections, as the sidebar lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum LibraryCollection {
    All,
    Recent,
    /// Recently edited (ADR 0079).
    Edited,
    /// Five stars (ADR 0081).
    Favourites,
    Picks,
    Rated,
    Rejected,
}

/// The export watermark (ADR 0069): off by default; the text is the photographer's own.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct WatermarkSettings {
    pub enabled: bool,
    /// What it says, such as "© 2026 Jamie". Nothing is laid on while it is empty.
    pub text: String,
    pub position: WatermarkPosition,
    pub size: WatermarkSize,
}

impl WatermarkSettings {
    /// Longer text is cut to this many characters.
    pub const TEXT_MAX: usize = 120;
}

impl<'de> Deserialize<'de> for WatermarkSettings {
    /// Field by field, like the Library's view: an unknown value is that field's default.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        let text: String = lenient(&value, "text");
        Ok(Self {
            enabled: lenient(&value, "enabled"),
            text: text.chars().take(Self::TEXT_MAX).collect(),
            position: lenient(&value, "position"),
            size: lenient(&value, "size"),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum WatermarkPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
    Centre,
    /// Repeated across the photo, on a slant.
    Repeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum WatermarkSize {
    Small,
    #[default]
    Medium,
    Large,
}

/// The Library's view choices (ADR 0065). Each reads leniently: a value this version
/// does not know (written by a newer one) falls back to its default rather than
/// failing the settings file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LibraryViewSettings {
    pub layout: LibraryLayout,
    pub filter: LibraryFilter,
    /// The one colour label shown, if chosen (ADR 0064).
    pub label: Option<LibraryLabel>,
    pub sort: LibrarySort,
}

impl<'de> Deserialize<'de> for LibraryViewSettings {
    /// Field by field: a missing or unknown value is that field's default.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        Ok(Self {
            layout: lenient(&value, "layout"),
            filter: lenient(&value, "filter"),
            label: lenient(&value, "label"),
            sort: lenient(&value, "sort"),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum LibraryLayout {
    #[default]
    Grid,
    List,
}

/// The Library header's filter: everything, picks, or three stars and up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum LibraryFilter {
    #[default]
    All,
    Picks,
    Rated3,
}

/// A colour label to show alone (ADR 0064).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum LibraryLabel {
    Red,
    Yellow,
    Green,
    Blue,
    Purple,
}

/// The Library's order (ADR 0064): capture time first, as in other photo tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum LibrarySort {
    #[default]
    Captured,
    Newest,
    Name,
    Rating,
}

/// `object[field]` read as `T`, or `T`'s default when it is missing or not a value this
/// version knows.
fn lenient<T: serde::de::DeserializeOwned + Default>(object: &serde_json::Value, field: &str) -> T {
    object
        .get(field)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
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
    /// What exports are sharpened for (ADR 0059).
    pub sharpen: OutputSharpening,
    /// The colour space exports are written in (ADR 0062).
    pub colour_space: ExportColourSpace,
    /// Whether exports carry the photo's capture facts: camera, lens, exposure, capture
    /// time (ADR 0063). The design's "Keep metadata", on by default.
    pub keep_metadata: bool,
    /// Whether the location is left out of them. The design's "Strip location", off by
    /// default.
    pub strip_location: bool,
    /// A line of text in a corner of each export (ADR 0069): the design's "Watermark".
    pub watermark: WatermarkSettings,
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

/// What an export is sharpened for (ADR 0059): nothing, a screen, matte or glossy paper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum OutputSharpening {
    None,
    #[default]
    Screen,
    Matte,
    Glossy,
}

impl<'de> Deserialize<'de> for OutputSharpening {
    /// A choice this version does not know (written by a newer one) reads as Screen.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match serde_json::Value::deserialize(d)?.as_str() {
            Some("none") => Self::None,
            Some("matte") => Self::Matte,
            Some("glossy") => Self::Glossy,
            _ => Self::Screen,
        })
    }
}

/// The colour space an export is written in (ADR 0062).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ExportColourSpace {
    #[default]
    Srgb,
    DisplayP3,
    AdobeRgb,
}

impl<'de> Deserialize<'de> for ExportColourSpace {
    /// A space this version does not know (written by a newer one) reads as sRGB.
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match serde_json::Value::deserialize(d)?.as_str() {
            Some("displayP3") => Self::DisplayP3,
            Some("adobeRgb") => Self::AdobeRgb,
            _ => Self::Srgb,
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
            sharpen: OutputSharpening::Screen,
            colour_space: ExportColourSpace::Srgb,
            keep_metadata: true,
            strip_location: false,
            watermark: WatermarkSettings::default(),
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
        let s: Settings =
            serde_json::from_str(r#"{"version":7,"export":{"sharpen":"glossy"}}"#).unwrap();
        assert_eq!(s.export.sharpen, OutputSharpening::Glossy);
        let s: Settings =
            serde_json::from_str(r#"{"version":7,"export":{"sharpen":"canvas"}}"#).unwrap();
        assert_eq!(
            s.export.sharpen,
            OutputSharpening::Screen,
            "unknown: the default"
        );
        let s: Settings =
            serde_json::from_str(r#"{"version":7,"export":{"colourSpace":"adobeRgb"}}"#).unwrap();
        assert_eq!(s.export.colour_space, ExportColourSpace::AdobeRgb);
        let s: Settings =
            serde_json::from_str(r#"{"version":7,"export":{"colourSpace":"rec2020"}}"#).unwrap();
        assert_eq!(
            s.export.colour_space,
            ExportColourSpace::Srgb,
            "unknown: the default"
        );
        // Metadata switches (ADR 0063): kept, location kept, unless stored otherwise.
        let s: Settings = serde_json::from_str(r#"{"version":7,"export":{}}"#).unwrap();
        assert!(s.export.keep_metadata && !s.export.strip_location);
        let s: Settings = serde_json::from_str(
            r#"{"version":7,"export":{"keepMetadata":false,"stripLocation":true}}"#,
        )
        .unwrap();
        assert!(!s.export.keep_metadata && s.export.strip_location);
    }

    #[test]
    fn library_view_is_remembered_and_read_leniently() {
        // Defaults: grid, everything, no label, capture time.
        let s: Settings = serde_json::from_str(r#"{"version":7}"#).unwrap();
        assert_eq!(s.library.view, LibraryViewSettings::default());
        assert_eq!(s.library.view.sort, LibrarySort::Captured);
        // A stored view loads.
        let s: Settings = serde_json::from_str(
            r#"{"version":7,"library":{"view":{"layout":"list","filter":"rated3","label":"purple","sort":"name"}}}"#,
        )
        .unwrap();
        assert_eq!(
            s.library.view,
            LibraryViewSettings {
                layout: LibraryLayout::List,
                filter: LibraryFilter::Rated3,
                label: Some(LibraryLabel::Purple),
                sort: LibrarySort::Name,
            }
        );
        // Values from a newer version fall back, one by one; the rest still load.
        let s: Settings = serde_json::from_str(
            r#"{"version":7,"library":{"defaultFolder":"/p","view":{"layout":"filmstrip","filter":"rated3","label":"orange","sort":"size"}}}"#,
        )
        .unwrap();
        assert_eq!(s.library.default_folder.as_deref(), Some("/p"));
        assert_eq!(s.library.view.layout, LibraryLayout::Grid);
        assert_eq!(s.library.view.filter, LibraryFilter::Rated3);
        assert_eq!(s.library.view.label, None);
        assert_eq!(s.library.view.sort, LibrarySort::Captured);
        // And it round-trips as the UI writes it.
        let json = serde_json::to_string(&s.library.view).unwrap();
        assert_eq!(
            json,
            r#"{"layout":"grid","filter":"rated3","label":null,"sort":"captured"}"#
        );
    }

    #[test]
    fn the_last_place_is_remembered_and_read_leniently() {
        for (json, want) in [
            (
                r#"{"kind":"folder","path":"/p/2026"}"#,
                Some(LibraryPlace::Folder {
                    path: "/p/2026".into(),
                }),
            ),
            (
                r#"{"kind":"collection","collection":"all"}"#,
                Some(LibraryPlace::Collection {
                    collection: LibraryCollection::All,
                }),
            ),
            (
                r#"{"kind":"album","id":7}"#,
                Some(LibraryPlace::Album { id: 7 }),
            ),
            // From a newer version, or damaged: no place, and the rest still loads.
            (r#"{"kind":"smartCollection","id":3}"#, None),
            (r#"{"kind":"collection","collection":"flagged"}"#, None),
            (r#""somewhere""#, None),
        ] {
            let s: Settings = serde_json::from_str(&format!(
                r#"{{"version":7,"library":{{"defaultFolder":"/p","lastPlace":{json},"view":{{"sort":"name"}}}}}}"#
            ))
            .unwrap();
            assert_eq!(s.library.last_place, want, "{json}");
            assert_eq!(s.library.default_folder.as_deref(), Some("/p"), "{json}");
            assert_eq!(s.library.view.sort, LibrarySort::Name, "{json}");
        }
        let place = LibraryPlace::Collection {
            collection: LibraryCollection::Picks,
        };
        assert_eq!(
            serde_json::to_string(&place).unwrap(),
            r#"{"kind":"collection","collection":"picks"}"#
        );
    }

    #[test]
    fn the_watermark_is_off_by_default_and_read_leniently() {
        let s: Settings = serde_json::from_str(r#"{"version":7}"#).unwrap();
        assert_eq!(s.export.watermark, WatermarkSettings::default());
        assert!(!s.export.watermark.enabled);
        let s: Settings = serde_json::from_str(
            r#"{"version":7,"export":{"watermark":{"enabled":true,"text":"© 2026 Jamie","position":"topLeft","size":"large"}}}"#,
        )
        .unwrap();
        assert_eq!(
            s.export.watermark,
            WatermarkSettings {
                enabled: true,
                text: "© 2026 Jamie".into(),
                position: WatermarkPosition::TopLeft,
                size: WatermarkSize::Large,
            }
        );
        // Unknown values fall back one by one; very long text is cut.
        let long = "x".repeat(500);
        let s: Settings = serde_json::from_str(&format!(
            r#"{{"version":7,"export":{{"jpegQuality":90,"watermark":{{"enabled":true,"text":"{long}","position":"middle","size":"huge"}}}}}}"#
        ))
        .unwrap();
        assert_eq!(s.export.jpeg_quality, 90);
        assert!(s.export.watermark.enabled);
        assert_eq!(
            s.export.watermark.text.chars().count(),
            WatermarkSettings::TEXT_MAX
        );
        assert_eq!(s.export.watermark.position, WatermarkPosition::BottomRight);
        assert_eq!(s.export.watermark.size, WatermarkSize::Medium);
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
