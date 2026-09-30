use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cache::{CacheStats, RenderKey};
use image_core::{OutputImage, PixelFormat, Pyramid};
use jobs::{CancelToken, JobHandle, JobSpec, JobSystem, Lane, Priority};
use raw::{DecodeOptions, DecodeScale, DecoderRegistry};
use renderer::{
    CpuRenderer, PreviewQuality, RECIPE_VERSION, RENDERER_VERSION, RenderBackend, RenderPlan,
    TemperatureScale,
};

use crate::previews::PreviewCache;
use crate::session::{OpenImages, OpenedImage};
use crate::{
    EmbeddedFrame, EngineConfig, EngineError, EngineInfo, ExportProgress, ExportRequest,
    ExportStage, ExportSummary, ImageId, ImageSummary, PreviewFrame, PreviewRequest,
    SourceIdentity,
};

/// Supersede key for the main viewer's preview renders: a new request cancels the
/// previous one whether it is interactive or detail quality.
const VIEWER_PREVIEW_KEY: &str = "viewer-preview";
const OPEN_KEY: &str = "open";
const RGBA_FORMAT_TAG: u8 = 0;
const INTERACTIVE_UNDERSAMPLE_PERCENT: u32 = 85;

/// The application engine. Cheap to share behind an `Arc`; all methods take `&self`.
pub struct Engine {
    pub(crate) shared: Arc<Shared>,
    // Owned outside `Shared` so job closures (which hold `Arc<Shared>`) never keep the
    // job system alive; dropping the engine stops the workers.
    pub(crate) jobs: JobSystem,
}

pub(crate) struct Shared {
    config: EngineConfig,
    pub(crate) decoders: DecoderRegistry,
    pub(crate) renderer: CpuRenderer,
    pub(crate) thumbnails: Option<cache::DiskCache>,
    /// Cancel tokens of thumbnail pre-generation batches, by batch key (library root).
    pub(crate) thumbnail_batches: Mutex<std::collections::HashMap<String, CancelToken>>,
    images: Mutex<OpenImages>,
    previews: Mutex<PreviewCache>,
    next_image_id: AtomicU64,
}

impl Engine {
    pub fn new(config: EngineConfig) -> Self {
        Self::with_decoders(config, DecoderRegistry::with_defaults())
    }

    pub fn with_decoders(config: EngineConfig, decoders: DecoderRegistry) -> Self {
        let jobs = JobSystem::new(config.jobs.clone());
        let shared = Arc::new(Shared {
            images: Mutex::new(OpenImages::new(config.max_open_images)),
            previews: Mutex::new(PreviewCache::new(config.preview_cache_bytes)),
            thumbnails: config
                .thumbnail_cache_dir
                .clone()
                .map(|dir| cache::DiskCache::new(dir, "jpg", config.thumbnail_cache_bytes)),
            thumbnail_batches: Mutex::new(std::collections::HashMap::new()),
            config,
            decoders,
            renderer: CpuRenderer,
            next_image_id: AtomicU64::new(1),
        });
        Self { shared, jobs }
    }

    pub fn info(&self) -> EngineInfo {
        #[cfg(feature = "libraw")]
        let libraw_version = Some(raw::LibRawDecoder::libraw_version());
        #[cfg(not(feature = "libraw"))]
        let libraw_version = None;
        EngineInfo {
            renderer_version: RENDERER_VERSION,
            recipe_version: RECIPE_VERSION,
            decoders: self.shared.decoders.names(),
            extensions: self.shared.decoders.extensions(),
            libraw_version,
            render_backend: self.shared.renderer.name(),
            jpeg_encoder: export::JpegEncoder::preferred().name(),
            embedded_jpeg_decoder: raw::embedded_jpeg_decoder(),
            adjustments: renderer::adjustments::specs(),
            mixer: renderer::adjustments::mixer_spec(),
            straighten: renderer::adjustments::STRAIGHTEN,
            perspective: renderer::adjustments::PERSPECTIVE.to_vec(),
            mask: renderer::adjustments::mask_specs(),
            mask_feather: renderer::adjustments::MASK_FEATHER,
            mask_density: renderer::adjustments::MASK_DENSITY,
        }
    }

    /// Decodes a preview-resolution copy of `path` and builds its pyramid.
    /// Supersedes any open still in progress. The desktop editor uses this: it shows
    /// only renders of the RAW data, never the camera's embedded JPEG (ADR 0020).
    pub fn open(&self, path: impl Into<PathBuf>) -> JobHandle<ImageSummary, EngineError> {
        let shared = Arc::clone(&self.shared);
        let path = path.into();
        let spec =
            JobSpec::new(Lane::Interactive, Priority::Interactive, "open").superseding(OPEN_KEY);
        self.jobs.submit(spec, move |token| {
            shared.open(&path, token, None::<fn(EmbeddedFrame)>)
        })
    }

    /// Like [`Engine::open`], but first extracts the file's embedded preview (if any)
    /// and passes it to `on_preview` from the job thread, typically within tens of
    /// milliseconds, while the real decode continues. Best effort: a missing or
    /// broken embedded preview never fails the open.
    pub fn open_with_preview(
        &self,
        path: impl Into<PathBuf>,
        on_preview: impl FnOnce(EmbeddedFrame) + Send + 'static,
    ) -> JobHandle<ImageSummary, EngineError> {
        let shared = Arc::clone(&self.shared);
        let path = path.into();
        let spec =
            JobSpec::new(Lane::Interactive, Priority::Interactive, "open").superseding(OPEN_KEY);
        self.jobs.submit(spec, move |token| {
            shared.open(&path, token, Some(on_preview))
        })
    }

    /// Auto level (ADR 0033): the straighten angle that levels the open photo, or `None`
    /// without a clear horizon or vertical. Measured on a preview level of about
    /// 1000 px, on the interactive lane (tens of milliseconds).
    pub fn auto_level(&self, image: ImageId) -> JobHandle<Option<f32>, EngineError> {
        let Some(image) = self.shared.images.lock().expect("images lock").get(image) else {
            return JobHandle::ready(
                self.jobs.next_id(),
                Err(jobs::JobError::Failed(EngineError::image_not_open())),
            );
        };
        let level = Arc::clone(&image.pyramid.levels()[image.pyramid.select_index(1000)]);
        let spec = JobSpec::new(Lane::Interactive, Priority::Interactive, "auto-level")
            .superseding("auto-level");
        self.jobs.submit(spec, move |_token| {
            Ok(renderer::geometry::auto_level(&level))
        })
    }

    /// Remove chromatic aberration (ADR 0035): the correction measured on the open
    /// photo, or `None` when it has too few clean edges to measure reliably. Measured on
    /// the level nearest 2000 px or above (about a quarter of a second), on the
    /// interactive lane: the user is waiting for the toggle.
    pub fn measure_chromatic_aberration(
        &self,
        image: ImageId,
    ) -> JobHandle<Option<renderer::ChromaticAberration>, EngineError> {
        let Some(image) = self.shared.images.lock().expect("images lock").get(image) else {
            return JobHandle::ready(
                self.jobs.next_id(),
                Err(jobs::JobError::Failed(EngineError::image_not_open())),
            );
        };
        let level = Arc::clone(&image.pyramid.levels()[image.pyramid.select_index(2000)]);
        let spec = JobSpec::new(
            Lane::Interactive,
            Priority::Interactive,
            "chromatic-aberration",
        )
        .superseding("chromatic-aberration");
        self.jobs.submit(spec, move |_token| {
            Ok(renderer::chromatic::estimate(&level))
        })
    }

    /// Renders a preview. Cache hits complete immediately; otherwise the render runs on
    /// the interactive lane and supersedes the previous viewer render.
    pub fn render_preview(&self, req: PreviewRequest) -> JobHandle<PreviewFrame, EngineError> {
        let Some(image) = self
            .shared
            .images
            .lock()
            .expect("images lock")
            .get(req.image)
        else {
            return JobHandle::ready(
                self.jobs.next_id(),
                Err(jobs::JobError::Failed(EngineError::image_not_open())),
            );
        };
        let target = req
            .quality
            .target_long_edge(req.target_long_edge, self.shared.config.limits);
        // Pyramid levels are powers of two apart; while dragging, accept a level slightly
        // below target rather than jumping to one with ~4x the pixels.
        let min_edge = match req.quality {
            PreviewQuality::Interactive => target * INTERACTIVE_UNDERSAMPLE_PERCENT / 100,
            PreviewQuality::Thumbnail | PreviewQuality::Detail => target,
        };
        // A crop shows part of each level: pick the level by what the crop keeps.
        let recipe = req.recipe.sanitized();
        let (fw, fh) = image.full_size;
        let full_output = recipe.geometry.map_or((fw, fh), |g| g.output_size(fw, fh));
        let kept = full_output.0.max(full_output.1) as f64 / fw.max(fh).max(1) as f64;
        let min_edge = (f64::from(min_edge) / kept.max(1e-3)).ceil() as u32;
        let level_index = image.pyramid.select_index(min_edge);
        let level = Arc::clone(&image.pyramid.levels()[level_index]);
        let as_shot_white = image.as_shot_white;
        let key = RenderKey::new(
            image.source_id,
            &recipe.canonical_bytes(),
            level.width(),
            level.height(),
            RGBA_FORMAT_TAG,
            RENDERER_VERSION,
        );

        let (supersede_key, priority) = match req.quality {
            PreviewQuality::Thumbnail => (
                format!("thumbnail-{}", req.image.0),
                Priority::VisibleThumbnail,
            ),
            PreviewQuality::Interactive | PreviewQuality::Detail => {
                (VIEWER_PREVIEW_KEY.to_owned(), Priority::Interactive)
            }
        };

        if let Some(hit) = self
            .shared
            .previews
            .lock()
            .expect("preview cache lock")
            .get(&key)
        {
            // The request is satisfied; anything still rendering for this slot is obsolete.
            self.jobs.cancel_key(&supersede_key);
            let histogram = viewer_histogram(req.quality, &hit);
            let frame = PreviewFrame {
                histogram,
                image: hit,
                level: level_index,
                cache_hit: true,
                render_ms: 0.0,
                full_size: full_output,
            };
            return JobHandle::ready(self.jobs.next_id(), Ok(frame));
        }

        let shared = Arc::clone(&self.shared);
        let quality = req.quality;
        let spec = JobSpec::new(Lane::Interactive, priority, "preview").superseding(supersede_key);
        self.jobs.submit(spec, move |token| {
            let t0 = Instant::now();
            let plan = RenderPlan::from_recipe(&recipe, as_shot_white);
            let out = shared
                .renderer
                .render(&plan, &level, PixelFormat::Rgba8, token)?;
            let render_ms = ms(t0);
            let out = Arc::new(out);
            let histogram = viewer_histogram(quality, &out);
            shared.previews.lock().expect("preview cache lock").insert(
                key,
                Arc::clone(&out),
                quality,
            );
            Ok(PreviewFrame {
                image: out,
                level: level_index,
                cache_hit: false,
                render_ms,
                full_size: full_output,
                histogram,
            })
        })
    }

    /// Exports at full resolution on the background lane. `progress` is called from
    /// the job's thread.
    pub fn export(
        &self,
        req: ExportRequest,
        progress: impl Fn(ExportProgress) + Send + 'static,
    ) -> JobHandle<ExportSummary, EngineError> {
        let Some(image) = self
            .shared
            .images
            .lock()
            .expect("images lock")
            .get(req.image)
        else {
            return JobHandle::ready(
                self.jobs.next_id(),
                Err(jobs::JobError::Failed(EngineError::image_not_open())),
            );
        };
        if let Err(e) = export::validate_destination(&req.destination, &image.path, req.format) {
            return JobHandle::ready(self.jobs.next_id(), Err(jobs::JobError::Failed(e.into())));
        }
        let shared = Arc::clone(&self.shared);
        let spec = JobSpec::new(Lane::Background, Priority::Export, "export");
        self.jobs.submit(spec, move |token| {
            shared.export(&image.path, &req, token, &progress)
        })
    }

    /// Releases an image and its cached previews.
    pub fn close(&self, id: ImageId) {
        let removed = self.shared.images.lock().expect("images lock").remove(id);
        if let Some(image) = removed {
            self.shared
                .previews
                .lock()
                .expect("preview cache lock")
                .retain(|k| k.source != image.source_id);
        }
    }

    /// Changes the preview cache budget at runtime (settings), evicting to fit.
    pub fn set_preview_cache_budget(&self, bytes: usize) {
        self.shared
            .previews
            .lock()
            .expect("preview cache lock")
            .set_budget(bytes);
    }

    pub fn preview_cache_stats(&self) -> CacheStats {
        self.shared
            .previews
            .lock()
            .expect("preview cache lock")
            .stats()
    }

    /// Bytes held by open images' pyramids.
    pub fn open_image_bytes(&self) -> usize {
        self.shared.images.lock().expect("images lock").bytes()
    }
}

impl Shared {
    fn open(
        &self,
        path: &Path,
        token: &CancelToken,
        on_preview: Option<impl FnOnce(EmbeddedFrame)>,
    ) -> Result<ImageSummary, EngineError> {
        let t0 = Instant::now();
        let identity = SourceIdentity::from_path(path).map_err(raw::DecodeError::from)?;
        let identity_ms = ms(t0);

        let t_embedded = Instant::now();
        // Only extracted when someone will show it (not the editor; ADR 0020).
        let embedded = match on_preview {
            Some(callback) => self
                .decoders
                .embedded_preview(
                    &identity.canonical_path,
                    self.config.embedded_preview_min_edge,
                    token,
                )
                .map(|p| p.map(|p| (p, callback))),
            None => Ok(None),
        };
        let embedded_preview_ms = match embedded {
            Ok(Some((preview, on_preview))) => {
                let extract_ms = ms(t_embedded);
                on_preview(EmbeddedFrame {
                    image: Arc::new(preview.image),
                    extract_ms,
                });
                Some(extract_ms)
            }
            Ok(None) => None,
            Err(e) => {
                // The real decode below reports actionable errors; this is advisory.
                log::warn!("embedded preview unavailable for {}: {e}", path.display());
                None
            }
        };
        if token.is_cancelled() {
            return Err(EngineError::cancelled());
        }

        let t1 = Instant::now();
        let options = DecodeOptions::new(DecodeScale::AtLeast(self.config.preview_source_min_edge))
            .with_max_threads(rayon::current_num_threads());
        let decoded = self
            .decoders
            .decode(&identity.canonical_path, options, token)?;
        let decode_ms = ms(t1);
        if token.is_cancelled() {
            return Err(EngineError::cancelled());
        }

        let t2 = Instant::now();
        let pyramid = Pyramid::build(decoded.image, self.config.limits.thumbnail_long_edge);
        let pyramid_ms = ms(t2);

        let id = ImageId(self.next_image_id.fetch_add(1, Ordering::Relaxed));
        let info = decoded.info;
        let summary = ImageSummary {
            id,
            path: identity.canonical_path.clone(),
            file_name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            decoder: info.decoder,
            kind: info.kind,
            camera: catalogue::camera_name(Some(&info.make), Some(&info.model)).unwrap_or_default(),
            iso: info.iso,
            aperture: info.aperture,
            shutter_seconds: info.shutter_seconds,
            focal_length_mm: info.focal_length_mm,
            temperature_scale: TemperatureScale::for_source(info.as_shot_white),
            full_width: info.full_width,
            full_height: info.full_height,
            levels: pyramid
                .levels()
                .iter()
                .map(|l| (l.width(), l.height()))
                .collect(),
            pyramid_bytes: pyramid.byte_size(),
            identity_ms,
            decode_ms,
            pyramid_ms,
            embedded_preview_ms,
        };
        let opened = Arc::new(OpenedImage {
            id,
            path: identity.canonical_path.clone(),
            source_id: identity.source_id(),
            pyramid,
            as_shot_white: info.as_shot_white,
            full_size: (info.full_width, info.full_height),
        });
        let evicted = self.images.lock().expect("images lock").insert(opened);
        if !evicted.is_empty() {
            let mut previews = self.previews.lock().expect("preview cache lock");
            for e in evicted {
                previews.retain(|k| k.source != e.source_id);
            }
        }
        log::info!(
            "opened {} ({}, {}x{}): decode {:.0} ms, pyramid {:.1} ms, embedded preview {}",
            path.display(),
            summary.decoder,
            summary.full_width,
            summary.full_height,
            summary.decode_ms,
            summary.pyramid_ms,
            summary
                .embedded_preview_ms
                .map_or("none".into(), |ms| format!("{ms:.0} ms")),
        );
        Ok(summary)
    }

    fn export(
        &self,
        source: &Path,
        req: &ExportRequest,
        token: &CancelToken,
        progress: &dyn Fn(ExportProgress),
    ) -> Result<ExportSummary, EngineError> {
        let start = Instant::now();
        progress(ExportProgress {
            stage: ExportStage::Decoding,
            fraction: 0.0,
        });
        let decoded = self.decoders.decode(
            source,
            // Inside the background lane this is its bounded pool size, so LibRaw's
            // internal threads cannot take the cores interactive renders need.
            DecodeOptions::new(DecodeScale::Full).with_max_threads(rayon::current_num_threads()),
            token,
        )?;
        let decode_ms = ms(start);

        progress(ExportProgress {
            stage: ExportStage::Rendering,
            fraction: 0.6,
        });
        let t = Instant::now();
        let plan = RenderPlan::from_recipe(&req.recipe, decoded.info.as_shot_white);
        let rendered = self
            .renderer
            .render(&plan, &decoded.image, PixelFormat::Rgb8, token)?;
        drop(decoded);
        let render_ms = ms(t);

        progress(ExportProgress {
            stage: ExportStage::Encoding,
            fraction: 0.8,
        });
        let t = Instant::now();
        let bytes = export::encode(&rendered, req.format)?;
        let encode_ms = ms(t);
        if token.is_cancelled() {
            return Err(EngineError::cancelled());
        }

        progress(ExportProgress {
            stage: ExportStage::Writing,
            fraction: 0.95,
        });
        let t = Instant::now();
        export::write_atomic(&req.destination, &bytes)?;
        let write_ms = ms(t);

        let total_ms = ms(start);
        log::info!(
            "exported {} ({}x{}, {} bytes) in {total_ms:.0} ms",
            req.destination.display(),
            rendered.width(),
            rendered.height(),
            bytes.len()
        );
        Ok(ExportSummary {
            path: req.destination.clone(),
            width: rendered.width(),
            height: rendered.height(),
            bytes: bytes.len(),
            decode_ms,
            render_ms,
            encode_ms,
            write_ms,
            total_ms,
        })
    }
}

/// The histogram the editor's graph shows, for viewer frames only (thumbnails have no
/// use for one). One parallel pass over the frame.
fn viewer_histogram(
    quality: PreviewQuality,
    image: &OutputImage,
) -> Option<Arc<renderer::Histogram>> {
    (quality != PreviewQuality::Thumbnail).then(|| Arc::new(renderer::Histogram::of(image)))
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}
