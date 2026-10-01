//! IPC payload types. TypeScript bindings are generated from these with ts-rs
//! (`cargo test -p desktop`), so the UI never hand-maintains copies.

use app_core::{EngineError, EngineInfo, ErrorKind, ExportStage, ExportSummary, ImageSummary};
use renderer::adjustments::{AdjustmentSpec, MixerSpec};
use renderer::{EditRecipe, PreviewQuality, TemperatureScale};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Name of the Tauri event carrying [`ExportEvent`]s.
pub const EXPORT_EVENT: &str = "export://event";
/// Name of the Tauri event carrying [`ExportQueueEvent`]s (ADR 0050).
pub const EXPORT_QUEUE_EVENT: &str = "export://queue";

/// Photos to export, and how (ADR 0050).
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportBatchDto {
    pub items: Vec<ExportItemDto>,
    /// The long edge in pixels; the full size when left out.
    #[ts(optional)]
    pub long_edge: Option<u32>,
    /// JPEG quality, 50-100.
    pub quality: u8,
    /// What to write (ADR 0057); JPEG when left out.
    #[serde(default)]
    #[ts(optional)]
    pub format: Option<settings::ExportFileFormat>,
    /// What to sharpen for (ADR 0059); Screen when left out.
    #[serde(default)]
    #[ts(optional)]
    pub sharpen: Option<settings::OutputSharpening>,
    /// The folder to export to, for the self-test only; otherwise the one chosen in
    /// the folder dialog (settings).
    #[ts(optional)]
    pub folder: Option<String>,
}

/// A file an export wrote.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportedFileDto {
    pub path: String,
    pub width: u32,
    pub height: u32,
    /// The file's size in bytes.
    #[ts(type = "number")]
    pub bytes: u64,
}

/// One photo to export: the photo open in the editor (by its image id, with its edit
/// as it is now), or a library photo (by path, with its saved edit).
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportItemDto {
    #[ts(optional, type = "number")]
    pub image_id: Option<u64>,
    #[ts(optional)]
    pub path: Option<String>,
    #[ts(optional)]
    pub recipe: Option<EditRecipe>,
}

/// The export queue's progress and outcome.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ExportQueueEvent {
    /// `done` of `total` photos finished; `current` is exporting, `fraction` of the way.
    Progress {
        done: u32,
        total: u32,
        current: String,
        fraction: f32,
    },
    /// The queue ran out (or was cancelled).
    Finished {
        exported: u32,
        /// The files written, in order.
        outputs: Vec<ExportedFileDto>,
        failed: Vec<FileFailureDto>,
        /// The folder the photos went to.
        folder: Option<String>,
        cancelled: bool,
    },
}

/// Preview frames are returned as raw bytes (not JSON) to avoid base64/array encoding
/// of multi-megabyte buffers. Layout, all little-endian:
///
/// | offset | type | field                         |
/// |--------|------|-------------------------------|
/// | 0      | u32  | width                         |
/// | 4      | u32  | height                        |
/// | 8      | u32  | pyramid level                 |
/// | 12     | u32  | flags (see `FRAME_FLAG_*`)    |
/// | 16     | f32  | render / extract time in ms   |
/// | 20     | u32  | full-resolution output width  |
/// | 24     | u32  | full-resolution output height |
/// | 28     | u32[]| histogram, if `FRAME_FLAG_HISTOGRAM`: red, green, blue, luma counts, 256 each |
/// | …      | u8[] | RGBA8 pixels, width*height*4  |
///
/// The full-resolution size is the recipe's output (after crop) at the photo's full
/// size: the exact shape of the picture, which preview levels only approximate.
/// Mirrored in `src/ipc/frame.ts`.
pub const FRAME_HEADER_BYTES: usize = 28;
/// The frame was served from the preview cache.
pub const FRAME_FLAG_CACHE_HIT: u32 = 1;
/// A histogram (`renderer::histogram::ENCODED_BYTES`) follows the header (ADR 0036).
pub const FRAME_FLAG_HISTOGRAM: u32 = 2;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EngineInfoDto {
    pub renderer_version: u32,
    pub recipe_version: u32,
    pub decoders: Vec<String>,
    pub extensions: Vec<String>,
    pub libraw_version: Option<String>,
    pub render_backend: String,
    pub jpeg_encoder: String,
    pub embedded_jpeg_decoder: String,
    pub cpu_threads: u32,
    pub adjustments: Vec<AdjustmentSpec>,
    /// The colour mixer's bands and per-band controls (ADR 0025).
    pub mixer: MixerSpec,
    /// The Geometry section's Straighten slider (ADR 0032).
    pub straighten: AdjustmentSpec,
    /// The Geometry section's Vertical and Horizontal sliders (ADR 0034).
    pub perspective: Vec<AdjustmentSpec>,
    /// A mask's Exposure, Warmth and Clarity (ADR 0040).
    pub mask: Vec<AdjustmentSpec>,
    /// A radial mask's Feather (ADR 0041).
    pub mask_feather: AdjustmentSpec,
    /// A mask's Density (ADR 0043).
    pub mask_density: AdjustmentSpec,
    /// The recipe's settings by panel section, for copying edits (ADR 0048).
    pub setting_groups: Vec<renderer::settings::SettingGroup>,
    /// The parametric tone curve's region sliders (ADR 0051).
    pub curve_regions: Vec<AdjustmentSpec>,
    /// Colour grading's Luminance, Blending and Balance (ADR 0052).
    pub grading: Vec<AdjustmentSpec>,
    /// Calibration's sliders (ADR 0053).
    pub calibration: Vec<AdjustmentSpec>,
}

impl From<EngineInfo> for EngineInfoDto {
    fn from(i: EngineInfo) -> Self {
        Self {
            renderer_version: i.renderer_version,
            recipe_version: i.recipe_version,
            decoders: i.decoders.into_iter().map(str::to_owned).collect(),
            extensions: i.extensions.into_iter().map(str::to_owned).collect(),
            libraw_version: i.libraw_version,
            render_backend: i.render_backend.to_owned(),
            jpeg_encoder: i.jpeg_encoder.to_owned(),
            embedded_jpeg_decoder: i.embedded_jpeg_decoder.to_owned(),
            cpu_threads: std::thread::available_parallelism().map_or(1, |n| n.get() as u32),
            adjustments: i.adjustments,
            mixer: i.mixer,
            straighten: i.straighten,
            perspective: i.perspective,
            mask: i.mask,
            mask_feather: i.mask_feather,
            mask_density: i.mask_density,
            setting_groups: i.setting_groups,
            curve_regions: i.curve_regions,
            grading: i.grading,
            calibration: i.calibration,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImageSummaryDto {
    #[ts(type = "number")]
    pub id: u64,
    /// Canonical path of the opened file.
    pub path: String,
    pub file_name: String,
    pub decoder: String,
    pub camera_raw: bool,
    pub camera: String,
    pub iso: Option<f32>,
    pub aperture: Option<f32>,
    pub shutter_seconds: Option<f32>,
    pub focal_length_mm: Option<f32>,
    /// For showing Temperature in kelvin (camera RAW with a known as-shot light).
    pub temperature_scale: Option<TemperatureScale>,
    pub full_width: u32,
    pub full_height: u32,
    pub levels: Vec<(u32, u32)>,
    #[ts(type = "number")]
    pub pyramid_bytes: u64,
    pub identity_ms: f64,
    pub decode_ms: f64,
    pub pyramid_ms: f64,
    pub embedded_preview_ms: Option<f64>,
    /// The photo's saved edit, applied from the first render. Null if unedited.
    pub saved_recipe: Option<EditRecipe>,
    /// Whether edits to this photo are saved.
    pub edit_saving: EditSavingDto,
}

/// Whether edits to an open photo are saved (ADR 0019).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum EditSavingDto {
    /// In a library folder: edits are saved automatically.
    Library,
    /// Opened from outside the library: edits last until another photo is opened.
    #[default]
    NotInLibrary,
    /// Edited in a newer version of the app: shown unedited, and not overwritten.
    NewerVersion,
}

/// A preset (ADR 0046): a built-in look or one the photographer saved. `id` is
/// `builtin:<name>` or `user:<number>`; only saved presets can be changed.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PresetDto {
    pub id: String,
    pub name: String,
    pub built_in: bool,
    /// The look alone: applying it keeps the photo's exposure, geometry, lens
    /// corrections and masks.
    pub recipe: EditRecipe,
}

impl From<app_core::Preset> for PresetDto {
    fn from(p: app_core::Preset) -> Self {
        let (id, built_in) = match &p.id {
            app_core::PresetRef::BuiltIn(name) => (format!("builtin:{name}"), true),
            app_core::PresetRef::User(id) => (format!("user:{}", id.0), false),
        };
        Self {
            id,
            name: p.name,
            built_in,
            recipe: p.recipe,
        }
    }
}

/// What importing preset files did (ADR 0047).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PresetImportDto {
    pub imported: Vec<ImportedPresetDto>,
    /// Files that could not be imported, and why.
    pub failed: Vec<FileFailureDto>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportedPresetDto {
    pub preset: PresetDto,
    pub from_lightroom: bool,
    /// Lightroom settings this app has no counterpart for, by Lightroom's names.
    pub left_out: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileFailureDto {
    /// The file's name (a preset file, or a photo).
    pub file: String,
    pub message: String,
}

/// The preset a `PresetDto::id` names.
pub fn preset_ref(id: &str) -> Option<app_core::PresetRef> {
    if let Some(name) = id.strip_prefix("builtin:") {
        return Some(app_core::PresetRef::BuiltIn(name.to_owned()));
    }
    let n = id.strip_prefix("user:")?.parse().ok()?;
    Some(app_core::PresetRef::User(app_core::PresetId(n)))
}

/// What pasting or syncing edits onto several photos did (ADR 0049).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PastedEditsDto {
    pub applied: Vec<PastedPhotoDto>,
    /// Photos left as they were, and why.
    pub failed: Vec<FileFailureDto>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PastedPhotoDto {
    pub path: String,
    /// The photo now differs from its original.
    pub edited: bool,
}

/// Result of saving an edit.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EditSavedDto {
    /// The photo now differs from the original (false after a reset).
    pub edited: bool,
}

impl From<ImageSummary> for ImageSummaryDto {
    fn from(s: ImageSummary) -> Self {
        Self {
            id: s.id.0,
            path: s.path.display().to_string(),
            file_name: s.file_name,
            decoder: s.decoder.to_owned(),
            camera_raw: s.kind == app_core::SourceKind::CameraRaw,
            camera: s.camera,
            iso: s.iso,
            aperture: s.aperture,
            shutter_seconds: s.shutter_seconds,
            focal_length_mm: s.focal_length_mm,
            temperature_scale: s.temperature_scale,
            full_width: s.full_width,
            full_height: s.full_height,
            levels: s.levels,
            pyramid_bytes: s.pyramid_bytes as u64,
            identity_ms: s.identity_ms,
            decode_ms: s.decode_ms,
            pyramid_ms: s.pyramid_ms,
            embedded_preview_ms: s.embedded_preview_ms,
            saved_recipe: None,
            edit_saving: EditSavingDto::NotInLibrary,
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PreviewRequestDto {
    #[ts(type = "number")]
    pub image_id: u64,
    pub recipe: EditRecipe,
    pub quality: PreviewQuality,
    /// Long edge of the viewport in device pixels.
    pub target_long_edge: u32,
    /// Which view it is for; the viewer when left out (ADR 0045).
    #[serde(default)]
    #[ts(optional, as = "Option<PreviewSlotDto>")]
    pub slot: PreviewSlotDto,
}

/// Which view a preview is for: each cancels only its own earlier renders.
#[derive(Debug, Clone, Copy, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PreviewSlotDto {
    #[default]
    Viewer,
    /// The photo before editing, for the before/after comparison.
    Compare,
    /// The preset strip's previews (ADR 0046).
    Presets,
}

impl From<PreviewSlotDto> for app_core::PreviewSlot {
    fn from(s: PreviewSlotDto) -> Self {
        match s {
            PreviewSlotDto::Viewer => Self::Viewer,
            PreviewSlotDto::Compare => Self::Compare,
            PreviewSlotDto::Presets => Self::Presets,
        }
    }
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportRequestDto {
    #[ts(type = "number")]
    pub image_id: u64,
    pub recipe: EditRecipe,
    /// Only honoured in self-test mode; otherwise a save dialog is shown.
    pub destination: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportStartedDto {
    #[ts(type = "number")]
    pub job_id: u64,
    pub path: String,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ExportStageDto {
    Decoding,
    Rendering,
    Encoding,
    Writing,
}

impl From<ExportStage> for ExportStageDto {
    fn from(s: ExportStage) -> Self {
        match s {
            ExportStage::Decoding => Self::Decoding,
            ExportStage::Rendering => Self::Rendering,
            ExportStage::Encoding => Self::Encoding,
            ExportStage::Writing => Self::Writing,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ExportEvent {
    Progress {
        #[ts(type = "number")]
        job_id: u64,
        stage: ExportStageDto,
        fraction: f32,
    },
    Finished {
        #[ts(type = "number")]
        job_id: u64,
        path: String,
        width: u32,
        height: u32,
        #[ts(type = "number")]
        bytes: u64,
        decode_ms: f64,
        render_ms: f64,
        encode_ms: f64,
        total_ms: f64,
    },
    Failed {
        #[ts(type = "number")]
        job_id: u64,
        error: IpcError,
    },
}

impl ExportEvent {
    pub fn finished(job_id: u64, s: &ExportSummary) -> Self {
        Self::Finished {
            job_id,
            path: s.path.display().to_string(),
            width: s.width,
            height: s.height,
            bytes: s.bytes as u64,
            decode_ms: s.decode_ms,
            render_ms: s.render_ms,
            encode_ms: s.encode_ms,
            total_ms: s.total_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum IpcErrorKind {
    NotFound,
    Unsupported,
    DecodeFailed,
    ImageNotOpen,
    InvalidDestination,
    ExportFailed,
    InvalidInput,
    /// Superseded by a newer request; the UI should ignore it silently.
    Cancelled,
    Internal,
}

/// Error returned to the UI: a category and a photographer-facing message.
/// Technical detail is logged on the Rust side, never shown; `reference` links the
/// two (it appears in the log line and in the UI's "Copy details").
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IpcError {
    pub kind: IpcErrorKind,
    pub message: String,
    pub reference: Option<String>,
}

impl IpcError {
    /// An internal failure in the shell itself; the detail is logged, the user sees a
    /// generic message.
    pub fn internal(detail: impl std::fmt::Display) -> Self {
        let reference = crate::logging::new_reference();
        log::error!("[{reference}] internal: {detail}");
        Self {
            kind: IpcErrorKind::Internal,
            message: "Something went wrong. Please try again.".into(),
            reference: Some(reference),
        }
    }
}

impl From<EngineError> for IpcError {
    fn from(e: EngineError) -> Self {
        let reference = (e.kind != ErrorKind::Cancelled).then(|| {
            let reference = crate::logging::new_reference();
            // Expected failures (unsupported file, bad destination) are warnings; the
            // rest are errors.
            match e.kind {
                ErrorKind::Internal | ErrorKind::ExportFailed => {
                    log::error!("[{reference}] {:?}: {}", e.kind, e.detail);
                }
                _ => log::warn!("[{reference}] {:?}: {}", e.kind, e.detail),
            }
            reference
        });
        let kind = match e.kind {
            ErrorKind::NotFound => IpcErrorKind::NotFound,
            ErrorKind::Unsupported => IpcErrorKind::Unsupported,
            ErrorKind::DecodeFailed => IpcErrorKind::DecodeFailed,
            ErrorKind::ImageNotOpen => IpcErrorKind::ImageNotOpen,
            ErrorKind::InvalidDestination => IpcErrorKind::InvalidDestination,
            ErrorKind::ExportFailed => IpcErrorKind::ExportFailed,
            ErrorKind::InvalidInput => IpcErrorKind::InvalidInput,
            ErrorKind::Cancelled => IpcErrorKind::Cancelled,
            ErrorKind::Internal => IpcErrorKind::Internal,
        };
        Self {
            kind,
            message: e.message,
            reference,
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SelfTestConfigDto {
    pub image_path: String,
    pub export_path: String,
}

/// Where a UI-side error came from.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ClientErrorSource {
    /// React render error caught by the error boundary.
    Render,
    /// `window.onerror`.
    Uncaught,
    /// Unhandled promise rejection.
    UnhandledRejection,
}

/// An error that happened in the webview, reported for the local log.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClientErrorReport {
    pub source: ClientErrorSource,
    pub message: String,
    pub stack: Option<String>,
}

/// Facts included in "Copy details" and useful in bug reports. Local only.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiagnosticsDto {
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub cpu_threads: u32,
    pub renderer_version: u32,
    pub libraw_version: Option<String>,
    pub jpeg_encoder: String,
    pub embedded_jpeg_decoder: String,
    pub log_dir: Option<String>,
}

/// Settings as shown in the Settings screen.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SettingsViewDto {
    pub settings: settings::Settings,
    /// A changed setting only takes effect after the app restarts.
    pub restart_required: bool,
    /// Set if the settings file was unreadable at start-up and was moved to this path.
    pub recovered_from: Option<String>,
}

/// One step in the path from the granted folder down to the listed folder.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FolderCrumbDto {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhotoEntryDto {
    pub name: String,
    pub path: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    #[ts(type = "number")]
    pub modified_ms: u64,
    /// Camera RAW (as opposed to an already-rendered format such as JPEG).
    pub raw: bool,
    /// From the catalogue, once the folder has been indexed.
    pub details: Option<PhotoDetailsDto>,
    /// Rating and flag (defaults until set).
    pub marks: MarksDto,
    /// The photo has a saved edit.
    pub edited: bool,
}

/// Pick/reject flag.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FlagDto {
    #[default]
    None,
    Pick,
    Reject,
}

impl From<app_core::Flag> for FlagDto {
    fn from(f: app_core::Flag) -> Self {
        match f {
            app_core::Flag::None => Self::None,
            app_core::Flag::Pick => Self::Pick,
            app_core::Flag::Reject => Self::Reject,
        }
    }
}

impl From<FlagDto> for app_core::Flag {
    fn from(f: FlagDto) -> Self {
        match f {
            FlagDto::None => Self::None,
            FlagDto::Pick => Self::Pick,
            FlagDto::Reject => Self::Reject,
        }
    }
}

/// The photographer's marks on a photo (ADR 0018).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MarksDto {
    /// 0 (unrated) to 5.
    pub rating: u8,
    pub flag: FlagDto,
}

impl From<app_core::Marks> for MarksDto {
    fn from(m: app_core::Marks) -> Self {
        Self {
            rating: m.rating.stars(),
            flag: m.flag.into(),
        }
    }
}

/// One change applied to one or more photos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum MarkChangeDto {
    Rating { stars: u8 },
    Flag { flag: FlagDto },
}

/// Library-wide collections: built from marks, or the recently imported photos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CollectionKindDto {
    Picks,
    Rated,
    Rejected,
    /// First indexed within the last 30 days (ADR 0056).
    Recent,
}

impl From<CollectionKindDto> for app_core::Collection {
    fn from(k: CollectionKindDto) -> Self {
        match k {
            CollectionKindDto::Picks => Self::Picks,
            CollectionKindDto::Rated => Self::Rated,
            CollectionKindDto::Rejected => Self::Rejected,
            CollectionKindDto::Recent => Self::RecentlyImported,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CollectionCountsDto {
    pub picks: u32,
    pub rated: u32,
    pub rejected: u32,
    pub recent: u32,
}

impl From<app_core::CollectionCounts> for CollectionCountsDto {
    fn from(c: app_core::CollectionCounts) -> Self {
        Self {
            picks: c.picks as u32,
            rated: c.rated as u32,
            rejected: c.rejected as u32,
            recent: c.recent as u32,
        }
    }
}

/// The photos matching a search of the library (ADR 0056).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchResultsDto {
    pub query: String,
    pub photos: Vec<PhotoEntryDto>,
}

/// Photos of a library-wide collection, from the catalogue.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CollectionListingDto {
    pub kind: CollectionKindDto,
    pub photos: Vec<PhotoEntryDto>,
}

/// An album (ADR 0055) as the library shows it.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AlbumDto {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    /// Its photos whose files are present.
    pub count: usize,
    /// Its first photo's file, for a cover; null when empty or not granted.
    pub cover: Option<String>,
}

/// An album and its photos.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AlbumListingDto {
    pub album: AlbumDto,
    pub photos: Vec<PhotoEntryDto>,
}

/// Photo details read from file headers during indexing.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PhotoDetailsDto {
    pub camera: Option<String>,
    pub lens: Option<String>,
    /// Camera wall-clock time, ISO 8601 without zone.
    pub captured_at: Option<String>,
    pub iso: Option<u32>,
    pub aperture: Option<f32>,
    pub shutter_seconds: Option<f32>,
    pub focal_length_mm: Option<f32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl From<&app_core::PhotoDetails> for PhotoDetailsDto {
    fn from(d: &app_core::PhotoDetails) -> Self {
        Self {
            camera: d.camera(),
            lens: d.lens.clone(),
            captured_at: d.captured_at.clone(),
            iso: d.iso,
            aperture: d.aperture,
            shutter_seconds: d.shutter_seconds,
            focal_length_mm: d.focal_length_mm,
            width: d.width,
            height: d.height,
        }
    }
}

/// A one-level folder listing for the Library.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FolderListingDto {
    pub path: String,
    pub name: String,
    /// From the granted folder (first) to this folder (last).
    pub breadcrumbs: Vec<FolderCrumbDto>,
    pub folders: Vec<FolderCrumbDto>,
    pub photos: Vec<PhotoEntryDto>,
    /// Entries that could not be read.
    pub skipped: u32,
}

/// Name of the Tauri event carrying [`IndexEvent`]s.
pub const INDEX_EVENT: &str = "library://index";

/// Background indexing of a library folder.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum IndexEvent {
    Progress {
        root: String,
        stage: IndexStageDto,
        total: u32,
        processed: u32,
    },
    Finished {
        root: String,
        found: u32,
        new: u32,
        changed: u32,
        moved: u32,
        missing: u32,
        skipped: u32,
        details_read: u32,
        total_ms: f64,
    },
    Failed {
        root: String,
        error: IpcError,
    },
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum IndexStageDto {
    Recording,
    ReadingDetails,
}

impl From<app_core::IndexStage> for IndexStageDto {
    fn from(s: app_core::IndexStage) -> Self {
        match s {
            app_core::IndexStage::Recording => Self::Recording,
            app_core::IndexStage::ReadingDetails => Self::ReadingDetails,
        }
    }
}

/// Library backups, for Settings (ADR 0021).
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupStatusDto {
    /// False when the library is temporary (in memory) and cannot be backed up.
    pub enabled: bool,
    pub count: u32,
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number | null")]
    pub latest_at_ms: Option<i64>,
    pub folder: String,
    /// Copies on another drive; null when not set up.
    pub copy: Option<BackupCopyDto>,
}

/// Backups also copied to a chosen folder, typically on another drive.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackupCopyDto {
    /// The chosen folder (copies go into a subfolder of it).
    pub folder: String,
    /// False when the folder is missing, typically because its drive is unplugged.
    pub connected: bool,
    pub count: u32,
    #[ts(type = "number | null")]
    pub latest_at_ms: Option<i64>,
}

/// Library-wide facts for the Library screen.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LibraryStatusDto {
    #[ts(type = "number")]
    pub photos: u64,
    pub folders: Vec<String>,
    /// Set if the catalogue was reset or could not be opened (shown once).
    pub notice: Option<String>,
    pub collections: CollectionCountsDto,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_ids_round_trip() {
        let saved = app_core::Preset {
            id: app_core::PresetRef::User(app_core::PresetId(12)),
            name: "Mine".into(),
            recipe: EditRecipe::default(),
        };
        let dto = PresetDto::from(saved.clone());
        assert_eq!((dto.id.as_str(), dto.built_in), ("user:12", false));
        assert_eq!(preset_ref(&dto.id), Some(saved.id));
        assert_eq!(
            preset_ref("builtin:mono"),
            Some(app_core::PresetRef::BuiltIn("mono".into()))
        );
        for bad in ["", "user:", "user:x", "12", "other:1"] {
            assert_eq!(preset_ref(bad), None, "{bad}");
        }
    }
}
