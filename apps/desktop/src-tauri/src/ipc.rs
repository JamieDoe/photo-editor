//! IPC payload types. TypeScript bindings are generated from these with ts-rs
//! (`cargo test -p desktop`), so the UI never hand-maintains copies.

use app_core::{EngineError, EngineInfo, ErrorKind, ExportStage, ExportSummary, ImageSummary};
use renderer::adjustments::AdjustmentSpec;
use renderer::{EditRecipe, PreviewQuality};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Name of the Tauri event carrying [`ExportEvent`]s.
pub const EXPORT_EVENT: &str = "export://event";

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
/// | 20     | u8[] | RGBA8 pixels, width*height*4  |
///
/// Mirrored in `src/ipc/frame.ts`.
pub const FRAME_HEADER_BYTES: usize = 20;
/// The frame was served from the preview cache.
pub const FRAME_FLAG_CACHE_HIT: u32 = 1;
/// The frame is the file's embedded camera preview, not a render of the recipe.
pub const FRAME_FLAG_EMBEDDED: u32 = 2;

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
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImageSummaryDto {
    #[ts(type = "number")]
    pub id: u64,
    pub file_name: String,
    pub decoder: String,
    pub camera_raw: bool,
    pub camera: String,
    pub full_width: u32,
    pub full_height: u32,
    pub levels: Vec<(u32, u32)>,
    #[ts(type = "number")]
    pub pyramid_bytes: u64,
    pub identity_ms: f64,
    pub decode_ms: f64,
    pub pyramid_ms: f64,
    pub embedded_preview_ms: Option<f64>,
}

impl From<ImageSummary> for ImageSummaryDto {
    fn from(s: ImageSummary) -> Self {
        Self {
            id: s.id.0,
            file_name: s.file_name,
            decoder: s.decoder.to_owned(),
            camera_raw: s.kind == app_core::SourceKind::CameraRaw,
            camera: s.camera,
            full_width: s.full_width,
            full_height: s.full_height,
            levels: s.levels,
            pyramid_bytes: s.pyramid_bytes as u64,
            identity_ms: s.identity_ms,
            decode_ms: s.decode_ms,
            pyramid_ms: s.pyramid_ms,
            embedded_preview_ms: s.embedded_preview_ms,
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
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExportRequestDto {
    #[ts(type = "number")]
    pub image_id: u64,
    pub recipe: EditRecipe,
    /// JPEG quality 1-100.
    pub quality: u8,
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
    /// Superseded by a newer request; the UI should ignore it silently.
    Cancelled,
    Internal,
}

/// Error returned to the UI: a category and a photographer-facing message.
/// Technical detail is logged on the Rust side, never shown.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IpcError {
    pub kind: IpcErrorKind,
    pub message: String,
}

impl IpcError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: IpcErrorKind::Internal,
            message: message.into(),
        }
    }
}

impl From<EngineError> for IpcError {
    fn from(e: EngineError) -> Self {
        if e.kind != ErrorKind::Cancelled {
            eprintln!("[engine] {e}");
        }
        let kind = match e.kind {
            ErrorKind::NotFound => IpcErrorKind::NotFound,
            ErrorKind::Unsupported => IpcErrorKind::Unsupported,
            ErrorKind::DecodeFailed => IpcErrorKind::DecodeFailed,
            ErrorKind::ImageNotOpen => IpcErrorKind::ImageNotOpen,
            ErrorKind::InvalidDestination => IpcErrorKind::InvalidDestination,
            ErrorKind::ExportFailed => IpcErrorKind::ExportFailed,
            ErrorKind::Cancelled => IpcErrorKind::Cancelled,
            ErrorKind::Internal => IpcErrorKind::Internal,
        };
        Self {
            kind,
            message: e.message,
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
