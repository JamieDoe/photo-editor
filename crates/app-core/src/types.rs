use std::path::PathBuf;
use std::sync::Arc;

use export::ExportFormat;
use image_core::OutputImage;
use raw::SourceKind;
use renderer::adjustments::AdjustmentSpec;
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

#[derive(Debug, Clone, Copy)]
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
}

#[derive(Debug, Clone)]
pub struct ExportRequest {
    pub image: ImageId,
    pub recipe: EditRecipe,
    pub destination: PathBuf,
    pub format: ExportFormat,
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
}
