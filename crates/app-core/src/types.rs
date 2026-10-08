use std::path::PathBuf;
use std::sync::Arc;

use export::ExportFormat;
use image_core::OutputImage;
use raw::SourceKind;
use renderer::adjustments::{AdjustmentSpec, MixerSpec};
use renderer::{EditRecipe, PreviewQuality};

/// Session-scoped handle to an open image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageId(pub u64);

/// Result of opening an image.
#[derive(Debug, Clone)]
pub struct ImageSummary {
    pub id: ImageId,
    /// Canonical path of the opened file.
    pub path: std::path::PathBuf,
    pub file_name: String,
    pub decoder: &'static str,
    pub kind: SourceKind,
    pub camera: String,
    pub iso: Option<f32>,
    pub aperture: Option<f32>,
    pub shutter_seconds: Option<f32>,
    pub focal_length_mm: Option<f32>,
    /// For showing Temperature in kelvin; `None` if the as-shot light is unknown.
    pub temperature_scale: Option<renderer::TemperatureScale>,
    pub full_width: u32,
    pub full_height: u32,
    /// Dimensions of each preview pyramid level, largest first.
    pub levels: Vec<(u32, u32)>,
    pub pyramid_bytes: usize,
    pub identity_ms: f64,
    pub decode_ms: f64,
    pub pyramid_ms: f64,
    /// Time to extract the embedded preview, if the file had one.
    pub embedded_preview_ms: Option<f64>,
}

/// Camera-rendered preview extracted from the file before decoding (RGBA8, sRGB).
/// Display-only placeholder; never cached as a render or used for editing.
#[derive(Debug, Clone)]
pub struct EmbeddedFrame {
    pub image: Arc<OutputImage>,
    pub extract_ms: f64,
}

/// Which view a preview is for. Each has its own render slot: a new request cancels
/// only the previous one for the same view, so the before image of a comparison
/// (ADR 0045) and the live edit do not cancel each other.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PreviewSlot {
    /// The edit being worked on.
    #[default]
    Viewer,
    /// The photo before editing, shown beside it; rendered after the edit's frames.
    Compare,
    /// The preset strip's previews (ADR 0046), rendered one after another behind the
    /// viewer's frames.
    Presets,
}

#[derive(Debug, Clone)]
pub struct PreviewRequest {
    pub image: ImageId,
    pub recipe: EditRecipe,
    pub quality: PreviewQuality,
    /// Long edge of the viewport in device pixels.
    pub target_long_edge: u32,
}

/// A rendered preview (RGBA8, sRGB).
#[derive(Debug, Clone)]
pub struct PreviewFrame {
    pub image: Arc<OutputImage>,
    pub level: usize,
    pub cache_hit: bool,
    pub render_ms: f64,
    /// The size this recipe's output has at full resolution (after crop), so the
    /// viewer keeps one exact shape for all of its renders.
    pub full_size: (u32, u32),
    /// The frame's histogram (ADR 0036), for viewer frames; `None` for thumbnails.
    pub histogram: Option<Arc<renderer::Histogram>>,
}

#[derive(Debug, Clone)]
pub struct ExportRequest {
    pub image: ImageId,
    pub recipe: EditRecipe,
    pub destination: PathBuf,
    pub format: ExportFormat,
    /// Output sharpening (ADR 0059), after any resize.
    pub sharpening: export::sharpen::OutputSharpening,
    /// The colour space written (ADR 0062).
    pub colour_space: export::colour::ExportColourSpace,
    /// The capture facts written with it (ADR 0063).
    pub metadata: export::metadata::MetadataChoice,
    /// The photographer's marks, written with the metadata as XMP (ADR 0067).
    pub judgements: export::metadata::Judgements,
    /// A watermark to lay on it (ADR 0069), shared by every photo of a batch.
    pub watermark: Option<std::sync::Arc<export::watermark::Watermark>>,
}

/// Exporting a photo straight from its file (ADR 0050), as the export queue does: it
/// need not be open in the editor.
#[derive(Debug, Clone)]
pub struct FileExport {
    pub source: PathBuf,
    pub recipe: EditRecipe,
    pub destination: PathBuf,
    pub format: ExportFormat,
    /// The output's long edge at most this many pixels; the full size when `None`.
    pub long_edge: Option<u32>,
    /// Output sharpening (ADR 0059), after any resize.
    pub sharpening: export::sharpen::OutputSharpening,
    /// The colour space written (ADR 0062).
    pub colour_space: export::colour::ExportColourSpace,
    /// The capture facts written with it (ADR 0063).
    pub metadata: export::metadata::MetadataChoice,
    /// The photographer's marks, written with the metadata as XMP (ADR 0067).
    pub judgements: export::metadata::Judgements,
    /// A watermark to lay on it (ADR 0069), shared by every photo of a batch.
    pub watermark: Option<std::sync::Arc<export::watermark::Watermark>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportStage {
    Decoding,
    Rendering,
    Encoding,
    Writing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExportProgress {
    pub stage: ExportStage,
    /// Overall completion, 0..1 (coarse: by stage).
    pub fraction: f32,
}

/// An export's estimated size (ADR 0068), before it is made.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExportEstimate {
    /// The file's estimated size in bytes.
    pub bytes: u64,
    /// The size it will be written at (after crop and any long-edge limit).
    pub width: u32,
    pub height: u32,
    /// The sample it was estimated from: its pixels and encoded bytes.
    pub sample_pixels: u64,
    pub sample_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ExportSummary {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
    pub decode_ms: f64,
    pub render_ms: f64,
    pub encode_ms: f64,
    pub write_ms: f64,
    pub total_ms: f64,
}

#[derive(Debug, Clone)]
pub struct EngineInfo {
    pub renderer_version: u32,
    pub recipe_version: u32,
    pub decoders: Vec<&'static str>,
    pub extensions: Vec<&'static str>,
    pub libraw_version: Option<String>,
    pub render_backend: &'static str,
    /// JPEG encoder used for export (libjpeg-turbo or the pure-Rust fallback).
    pub jpeg_encoder: &'static str,
    /// JPEG decoder used for embedded RAW previews.
    pub embedded_jpeg_decoder: &'static str,
    pub adjustments: Vec<AdjustmentSpec>,
    pub mixer: MixerSpec,
    pub straighten: AdjustmentSpec,
    pub perspective: Vec<AdjustmentSpec>,
    pub mask: Vec<AdjustmentSpec>,
    pub mask_feather: AdjustmentSpec,
    pub mask_density: AdjustmentSpec,
    pub setting_groups: Vec<renderer::settings::SettingGroup>,
    pub curve_regions: Vec<AdjustmentSpec>,
    pub grading: Vec<AdjustmentSpec>,
    pub calibration: Vec<AdjustmentSpec>,
}
