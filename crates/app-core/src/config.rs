use jobs::JobSystemConfig;
use renderer::QualityLimits;

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Previews are decoded at reduced resolution when the result still has at least
    /// this long edge. Full resolution is only decoded for export.
    pub preview_source_min_edge: u32,
    /// Embedded (camera) previews shown while decoding are picked/downscaled to at
    /// least this long edge.
    pub embedded_preview_min_edge: u32,
    pub limits: QualityLimits,
    /// Byte budget of the rendered-preview cache.
    pub preview_cache_bytes: usize,
    /// Decoded images kept in memory (each holds its preview pyramid).
    pub max_open_images: usize,
    pub jobs: JobSystemConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            preview_source_min_edge: 1600,
            embedded_preview_min_edge: 1024,
            limits: QualityLimits::default(),
            preview_cache_bytes: 256 * 1024 * 1024,
            max_open_images: 2,
            jobs: JobSystemConfig::default(),
        }
    }
}
