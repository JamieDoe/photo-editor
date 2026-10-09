use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cache::{CacheStats, RenderKey};
use image_core::{OutputImage, PixelFormat, Pyramid};
use jobs::{CancelToken, JobHandle, JobSpec, JobSystem, Lane, Priority};
use raw::{DecodeOptions, DecodeScale, DecoderRegistry};
use renderer::geometry::Turn;
use renderer::{
    CpuRenderer, PreviewQuality, RECIPE_VERSION, RENDERER_VERSION, RenderBackend, RenderPlan,
    TemperatureScale,
};

use crate::previews::PreviewCache;
use crate::session::{OpenImages, OpenedImage};
use crate::{
    EditRecipe, EmbeddedFrame, EngineConfig, EngineError, EngineInfo, ExportEstimate, ExportFormat,
    ExportProgress, ExportRequest, ExportStage, ExportSummary, FileExport, GeneratedMask, ImageId,
    ImageSummary, PreviewFrame, PreviewRequest, PreviewSlot, SourceIdentity,
};

/// Supersede key for the main viewer's preview renders: a new request cancels the
/// previous one whether it is interactive or detail quality.
const VIEWER_PREVIEW_KEY: &str = "viewer-preview";
/// Supersede key for the comparison's before renders (ADR 0045).
const COMPARE_PREVIEW_KEY: &str = "compare-preview";
/// Supersede key for the preset strip's previews (ADR 0046).
const PRESET_PREVIEW_KEY: &str = "preset-previews";
const OPEN_KEY: &str = "open";
const RGBA_FORMAT_TAG: u8 = 0;
/// Window renders (ADR 0070) are keyed apart from whole previews.
const RGBA_WINDOW_FORMAT_TAG: u8 = 7;
const INTERACTIVE_UNDERSAMPLE_PERCENT: u32 = 85;
/// Generated masks (ADR 0074) are made from a picture about this many pixels long:
/// Vision's models work at a lower size anyway, and soft edges are upscaled smoothly.
pub const SEGMENT_LONG_EDGE: u32 = 1536;
/// Auto tone's sample (ADR 0071): about this many pixels on its long side.
pub const AUTO_TONE_SAMPLE_EDGE: u32 = 512;

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
    /// The platform's mask generator (ADR 0074), if it has one.
    segmenter: Box<dyn ai::Segmenter>,
    /// Generated masks' coverage (ADR 0074).
    masks: crate::mask_store::MaskStore,
}

impl Engine {
    pub fn new(config: EngineConfig) -> Self {
        Self::with_decoders(config, DecoderRegistry::with_defaults())
    }

    pub fn with_decoders(config: EngineConfig, decoders: DecoderRegistry) -> Self {
        let jobs = JobSystem::new(config.jobs.clone());
        let masks = crate::mask_store::MaskStore::new(config.mask_dir.clone());
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
            segmenter: ai::platform_segmenter(),
            masks,
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
            setting_groups: renderer::settings::setting_groups(),
            curve_regions: renderer::adjustments::CURVE_REGIONS.to_vec(),
            grading: renderer::adjustments::GRADING.to_vec(),
            calibration: renderer::adjustments::CALIBRATION.to_vec(),
            mask_kinds: self.mask_kinds().into_iter().map(generated_kind).collect(),
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

    /// A new heal or clone spot (ADR 0054) on the open photo, its source found nearby;
    /// `None` when no source fits. Searched on a preview level of about 1500 px, on the
    /// interactive lane (a few milliseconds).
    pub fn new_spot(
        &self,
        image: ImageId,
        kind: renderer::retouch::SpotKind,
        at: [f32; 2],
        radius: f32,
        avoid: Vec<renderer::retouch::Spot>,
    ) -> JobHandle<Option<renderer::retouch::Spot>, EngineError> {
        let Some(image) = self.shared.images.lock().expect("images lock").get(image) else {
            return JobHandle::ready(
                self.jobs.next_id(),
                Err(jobs::JobError::Failed(EngineError::image_not_open())),
            );
        };
        let level = Arc::clone(&image.pyramid.levels()[image.pyramid.select_index(1500)]);
        let spec = JobSpec::new(Lane::Interactive, Priority::Interactive, "new-spot");
        self.jobs.submit(spec, move |_token| {
            Ok(renderer::retouch::new_spot(
                &level, kind, at[0], at[1], radius, &avoid,
            ))
        })
    }

    /// Sensor dust on the open photo (ADR 0058), as heal spots not already covered by
    /// `existing`, each with a source. Looked for on a preview level of about 2000 px,
    /// on the interactive lane (tens of milliseconds).
    pub fn find_dust(
        &self,
        image: ImageId,
        existing: Vec<renderer::retouch::Spot>,
    ) -> JobHandle<Vec<renderer::retouch::Spot>, EngineError> {
        let Some(image) = self.shared.images.lock().expect("images lock").get(image) else {
            return JobHandle::ready(
                self.jobs.next_id(),
                Err(jobs::JobError::Failed(EngineError::image_not_open())),
            );
        };
        let level = Arc::clone(&image.pyramid.levels()[image.pyramid.select_index(2000)]);
        let spec = JobSpec::new(Lane::Interactive, Priority::Interactive, "find-dust")
            .superseding("find-dust");
        self.jobs.submit(spec, move |_token| {
            Ok(renderer::dust::dust_spots(&level, &existing))
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
        self.render_preview_in(req, PreviewSlot::Viewer)
    }

    /// Renders a preview for `slot`: as [`Engine::render_preview`], superseding only the
    /// previous render for the same slot. Compare renders yield to the viewer's.
    pub fn render_preview_in(
        &self,
        req: PreviewRequest,
        slot: PreviewSlot,
    ) -> JobHandle<PreviewFrame, EngineError> {
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
        if let Some(window) = req.window {
            return self.render_window(
                image,
                req.recipe,
                req.quality,
                window,
                req.target_long_edge,
                slot,
            );
        }
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
        let fill = PlanExtras::of(&self.shared, &image, &recipe);
        let key = RenderKey::new(
            image.source_id,
            &fill.keyed(recipe.canonical_bytes()),
            level.width(),
            level.height(),
            RGBA_FORMAT_TAG,
            RENDERER_VERSION,
        );

        let (supersede_key, priority) = match (slot, req.quality) {
            (PreviewSlot::Viewer, PreviewQuality::Thumbnail) => (
                format!("thumbnail-{}", req.image.0),
                Priority::VisibleThumbnail,
            ),
            (PreviewSlot::Viewer, PreviewQuality::Interactive | PreviewQuality::Detail) => {
                (VIEWER_PREVIEW_KEY.to_owned(), Priority::Interactive)
            }
            (PreviewSlot::Compare, _) => (COMPARE_PREVIEW_KEY.to_owned(), Priority::VisiblePreview),
            (PreviewSlot::Presets, _) => {
                (PRESET_PREVIEW_KEY.to_owned(), Priority::VisibleThumbnail)
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
                window: None,
                fill_pending: fill.pending(),
            };
            return JobHandle::ready(self.jobs.next_id(), Ok(frame));
        }

        let shared = Arc::clone(&self.shared);
        let quality = req.quality;
        let spec = JobSpec::new(Lane::Interactive, priority, "preview").superseding(supersede_key);
        self.jobs.submit(spec, move |token| {
            let t0 = Instant::now();
            let plan = fill.plan(&recipe, as_shot_white);
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
                window: None,
                fill_pending: fill.pending(),
            })
        })
    }

    /// A window of the open photo's output (ADR 0070, zoom), for the viewer, from the
    /// smallest source at least as sharp as the zoom (`zoomed_long_edge`, the whole
    /// output's long edge at it): a preview level, or past the largest one the full
    /// resolution once it is decoded ([`Engine::prepare_full`]). Interactive renders
    /// (while dragging) and those before it is ready take the largest level. The
    /// window is scaled to the source; its pixels are what a whole render of that
    /// source gives there (`CpuRenderer::render_window`).
    fn render_window(
        &self,
        image: Arc<OpenedImage>,
        recipe: EditRecipe,
        quality: PreviewQuality,
        window: (u32, u32, u32, u32),
        zoomed_long_edge: u32,
        slot: PreviewSlot,
    ) -> JobHandle<PreviewFrame, EngineError> {
        let recipe = recipe.sanitized();
        let (fw, fh) = image.full_size;
        let full_output = recipe.geometry.map_or((fw, fh), |g| g.output_size(fw, fh));
        // The smallest source at least as sharp as the zoom shows: the source's long
        // edge must be the photo's times the zoom (output long edge over the full
        // output's). While dragging, a little less will do, as for whole previews.
        let zoom = f64::from(zoomed_long_edge) / f64::from(full_output.0.max(full_output.1).max(1));
        let allowance = match quality {
            PreviewQuality::Interactive => f64::from(INTERACTIVE_UNDERSAMPLE_PERCENT) / 100.0,
            PreviewQuality::Thumbnail | PreviewQuality::Detail => 1.0,
        };
        let needed = (zoom * allowance * f64::from(fw.max(fh))).ceil() as u32;
        let base = image.pyramid.base();
        let full = (quality != PreviewQuality::Interactive && needed > base.long_edge())
            .then(|| image.full())
            .flatten();
        let source = full.unwrap_or_else(|| {
            Arc::clone(&image.pyramid.levels()[image.pyramid.select_index(needed)])
        });
        // The source's output is the full output scaled by the source's size.
        let s = f64::from(source.width()) / f64::from(fw.max(1));
        let (x, y, w, h) = window;
        let scaled = (
            (f64::from(x) * s).floor() as u32,
            (f64::from(y) * s).floor() as u32,
            ((f64::from(x + w) * s).ceil() as u32)
                .saturating_sub((f64::from(x) * s).floor() as u32)
                .max(1),
            ((f64::from(y + h) * s).ceil() as u32)
                .saturating_sub((f64::from(y) * s).floor() as u32)
                .max(1),
        );
        let fill = PlanExtras::of(&self.shared, &image, &recipe);
        let mut key_bytes = fill.keyed(recipe.canonical_bytes());
        for v in [scaled.0, scaled.1, scaled.2, scaled.3] {
            key_bytes.extend(v.to_le_bytes());
        }
        let key = RenderKey::new(
            image.source_id,
            &key_bytes,
            source.width(),
            source.height(),
            RGBA_WINDOW_FORMAT_TAG,
            RENDERER_VERSION,
        );
        let covers = move |out: &OutputImage| {
            [
                f64::from(scaled.0) / s,
                f64::from(scaled.1) / s,
                f64::from(out.width()) / s,
                f64::from(out.height()) / s,
            ]
        };
        let supersede_key = match slot {
            PreviewSlot::Compare => COMPARE_PREVIEW_KEY.to_owned(),
            _ => VIEWER_PREVIEW_KEY.to_owned(),
        };
        if let Some(hit) = self
            .shared
            .previews
            .lock()
            .expect("preview cache lock")
            .get(&key)
        {
            self.jobs.cancel_key(&supersede_key);
            let window = Some(covers(&hit));
            return JobHandle::ready(
                self.jobs.next_id(),
                Ok(PreviewFrame {
                    image: hit,
                    level: 0,
                    cache_hit: true,
                    render_ms: 0.0,
                    full_size: full_output,
                    histogram: None,
                    window,
                    fill_pending: fill.pending(),
                }),
            );
        }
        let shared = Arc::clone(&self.shared);
        let as_shot_white = image.as_shot_white;
        let spec = JobSpec::new(Lane::Interactive, Priority::Interactive, "preview-window")
            .superseding(supersede_key);
        self.jobs.submit(spec, move |token| {
            let t0 = Instant::now();
            let plan = fill.plan(&recipe, as_shot_white);
            let out =
                shared
                    .renderer
                    .render_window(&plan, &source, PixelFormat::Rgba8, scaled, token)?;
            let render_ms = ms(t0);
            let out = Arc::new(out);
            shared.previews.lock().expect("preview cache lock").insert(
                key,
                Arc::clone(&out),
                quality,
            );
            let window = Some(covers(&out));
            Ok(PreviewFrame {
                image: out,
                level: 0,
                cache_hit: false,
                render_ms,
                full_size: full_output,
                histogram: None,
                window,
                fill_pending: fill.pending(),
            })
        })
    }

    /// Decodes the open photo at full resolution (ADR 0070), for viewing at 100 %:
    /// once per photo, kept while it is open. On the background lane; resolves when
    /// window renders can use it.
    pub fn prepare_full(&self, image: ImageId) -> JobHandle<(), EngineError> {
        let Some(open) = self.shared.images.lock().expect("images lock").get(image) else {
            return JobHandle::ready(
                self.jobs.next_id(),
                Err(jobs::JobError::Failed(EngineError::image_not_open())),
            );
        };
        if open.full().is_some() {
            return JobHandle::ready(self.jobs.next_id(), Ok(()));
        }
        let shared = Arc::clone(&self.shared);
        let spec = JobSpec::new(
            Lane::Background,
            Priority::VisiblePreview,
            "full-resolution",
        )
        .superseding(format!("full-resolution-{}", image.0));
        self.jobs.submit(spec, move |token| {
            shared.full_source(&open, token).map(|_| ())
        })
    }

    /// Fills `removals` on the open photo at full resolution (ADR 0070), decoding it
    /// first if need be, so that every view shows the same fill; resolves when renders
    /// use it. Until then, renders fill at their own size and say so
    /// ([`PreviewFrame::fill_pending`]). A newer call for the photo replaces one still
    /// running.
    pub fn prepare_fill(
        &self,
        image: ImageId,
        removals: Vec<renderer::remove::Removal>,
    ) -> JobHandle<(), EngineError> {
        let Some(open) = self.shared.images.lock().expect("images lock").get(image) else {
            return JobHandle::ready(
                self.jobs.next_id(),
                Err(jobs::JobError::Failed(EngineError::image_not_open())),
            );
        };
        let removals = painting(&removals);
        if removals.is_empty() || open.fill_for(&removals).is_some() {
            return JobHandle::ready(self.jobs.next_id(), Ok(()));
        }
        let shared = Arc::clone(&self.shared);
        let spec = JobSpec::new(Lane::Background, Priority::VisiblePreview, "removal-fill")
            .superseding(format!("removal-fill-{}", image.0));
        self.jobs.submit(spec, move |token| {
            let full = shared.full_source(&open, token)?;
            let t0 = Instant::now();
            let fill = renderer::remove::Fill::new(&full, &removals, token)?;
            log::info!(
                "removals of {} filled at full resolution in {:.0} ms",
                open.path.display(),
                ms(t0)
            );
            *open.fill.lock().expect("fill lock") = Some((removals, Arc::new(fill)));
            Ok(())
        })
    }

    /// The kinds of mask this computer can make from a photo (ADR 0074).
    pub fn mask_kinds(&self) -> Vec<ai::MaskKind> {
        [
            ai::MaskKind::Subject,
            ai::MaskKind::Sky,
            ai::MaskKind::People,
        ]
        .into_iter()
        .filter(|&k| self.shared.segmenter.supports(k))
        .collect()
    }

    /// Stored mask `name` as it covers the shown part of the photo (ADR 0074): `width`
    /// × `height` samples (0..255, row by row) over `crop` of the frame that
    /// `geometry` (and the lens's corrections, with `profile_corrections`; ADR 0075)
    /// makes of the open photo, mapped as the renderer maps it. For drawing a mask's
    /// tint; `None` when the mask isn't stored.
    pub fn mask_view(
        &self,
        image: ImageId,
        geometry: Option<renderer::Geometry>,
        profile_corrections: bool,
        name: &str,
        crop: renderer::CropRect,
        size: (u32, u32),
    ) -> JobHandle<Option<Vec<u8>>, EngineError> {
        let Some(open) = self.shared.images.lock().expect("images lock").get(image) else {
            return JobHandle::ready(
                self.jobs.next_id(),
                Err(jobs::JobError::Failed(EngineError::image_not_open())),
            );
        };
        let shared = Arc::clone(&self.shared);
        let name = name.to_owned();
        let spec = JobSpec::new(Lane::Interactive, Priority::VisiblePreview, "mask-view");
        self.jobs.submit(spec, move |_token| {
            use renderer::masks::{CompiledShape, Frame, GeneratedKind, MaskShape, SourceFrame};
            if !crate::mask_store::belongs(&name, open.fingerprint) {
                return Ok(None);
            }
            let Some(map) = shared.masks.get(&name) else {
                return Ok(None);
            };
            let (sw, sh) = (open.full_size.0 as f32, open.full_size.1 as f32);
            let lens = open
                .lens
                .as_ref()
                .map(|(l, _)| *l)
                .filter(|_| profile_corrections);
            let frame = match geometry.filter(|g| !g.is_identity()) {
                None if lens.is_none() => Frame::whole(open.full_size.0, open.full_size.1),
                g => {
                    let g = g.unwrap_or_default();
                    let (fw, fh) = g.oriented_size(sw, sh);
                    Frame {
                        crop: renderer::CropRect::FULL,
                        width: fw,
                        height: fh,
                        from_source: Some(SourceFrame {
                            geometry: g,
                            width: sw,
                            height: sh,
                            lens,
                        }),
                    }
                }
            };
            let shape = CompiledShape::new(
                &MaskShape::Generated {
                    of: GeneratedKind::Subject,
                    mask: name.clone(),
                },
                &frame,
                &[(name, map)].into_iter().collect(),
            );
            let (w, h) = (size.0.clamp(1, 4096), size.1.clamp(1, 4096));
            let mut out = Vec::with_capacity((w * h) as usize);
            for j in 0..h {
                let fy = (crop.y + (j as f32 + 0.5) / h as f32 * crop.h) * frame.height;
                for i in 0..w {
                    let fx = (crop.x + (i as f32 + 0.5) / w as f32 * crop.w) * frame.width;
                    out.push((shape.coverage(fx, fy) * 255.0).round() as u8);
                }
            }
            Ok(Some(out))
        })
    }

    /// The generated masks `recipe` names that open photo `image` can't use (ADR 0074):
    /// made on another computer or their files removed, or made from another photo (a
    /// pasted edit, a preset). They cover nothing until made again. All of them when the
    /// photo isn't open.
    pub fn missing_masks(&self, image: ImageId, recipe: &EditRecipe) -> Vec<String> {
        let photo = self
            .shared
            .images
            .lock()
            .expect("images lock")
            .get(image)
            .map(|o| o.fingerprint);
        generated_in(recipe)
            .into_iter()
            .map(|(name, _)| name)
            .filter(|name| photo.is_none_or(|p| self.shared.usable_mask(name, p).is_none()))
            .collect()
    }

    /// A `kind` mask of the open photo (ADR 0074), kept in the mask store under the
    /// name it returns, or `None` when the photo has nothing of the kind. Made from the photo as decoded (oriented, no edit, no crop) at about
    /// [`SEGMENT_LONG_EDGE`] px, on the background lane: the system's model takes a
    /// second or so. A newer request for the photo replaces one still running.
    pub fn segment(
        &self,
        image: ImageId,
        kind: ai::MaskKind,
    ) -> JobHandle<Option<GeneratedMask>, EngineError> {
        let Some(open) = self.shared.images.lock().expect("images lock").get(image) else {
            return JobHandle::ready(
                self.jobs.next_id(),
                Err(jobs::JobError::Failed(EngineError::image_not_open())),
            );
        };
        if !self.shared.segmenter.supports(kind) {
            return JobHandle::ready(
                self.jobs.next_id(),
                Err(jobs::JobError::Failed(
                    ai::AiError::Unsupported(kind).into(),
                )),
            );
        }
        let level =
            Arc::clone(&open.pyramid.levels()[open.pyramid.select_index(SEGMENT_LONG_EDGE)]);
        let shared = Arc::clone(&self.shared);
        let spec = JobSpec::new(Lane::Background, Priority::VisiblePreview, "segment")
            .superseding(format!("segment-{}-{kind:?}", image.0));
        self.jobs.submit(spec, move |token| {
            let source = MaskSource {
                image: &level,
                as_shot_white: open.as_shot_white,
                scene_ev: open.scene_ev,
                photo: open.fingerprint,
            };
            shared.make_mask(&source, kind, token)
        })
    }

    /// Auto tone for the open photo as `recipe` edits it (ADR 0071): its tone sliders
    /// found by rendering a sample of about [`AUTO_TONE_SAMPLE_EDGE`] px through the
    /// pipeline until it meets each target (`renderer::auto_tone`). On the interactive
    /// lane, tens of renders of the small sample; a newer request replaces one still
    /// running.
    pub fn auto_tone(
        &self,
        image: ImageId,
        recipe: &EditRecipe,
    ) -> JobHandle<renderer::auto_tone::AutoTone, EngineError> {
        self.auto_job(image, recipe, |r, measure| {
            renderer::auto_tone::auto_tone(r, measure)
        })
    }

    /// Auto for one setting (ADR 0071; Shift-double-click on its slider): as
    /// [`Engine::auto_tone`], for `setting` alone, the rest of `recipe` as it is.
    pub fn auto_setting(
        &self,
        image: ImageId,
        recipe: &EditRecipe,
        setting: renderer::auto_tone::ToneSetting,
    ) -> JobHandle<f32, EngineError> {
        self.auto_job(image, recipe, move |r, measure| {
            renderer::auto_tone::auto_setting(r, setting, measure)
        })
    }

    /// Runs `find` on the open photo's Auto sample: `recipe` sanitised, and a measure
    /// that renders a recipe on the sample and reads its tones.
    fn auto_job<T: Send + 'static>(
        &self,
        image: ImageId,
        recipe: &EditRecipe,
        find: impl FnOnce(
            &EditRecipe,
            &mut dyn FnMut(
                &EditRecipe,
            )
                -> Result<renderer::auto_tone::ToneStats, renderer::RenderError>,
        ) -> Result<T, renderer::RenderError>
        + Send
        + 'static,
    ) -> JobHandle<T, EngineError> {
        let Some(open) = self.shared.images.lock().expect("images lock").get(image) else {
            return JobHandle::ready(
                self.jobs.next_id(),
                Err(jobs::JobError::Failed(EngineError::image_not_open())),
            );
        };
        let recipe = recipe.sanitized();
        let (fw, fh) = open.full_size;
        let full_output = recipe.geometry.map_or((fw, fh), |g| g.output_size(fw, fh));
        // What the crop keeps decides the level, as for previews.
        let kept = full_output.0.max(full_output.1) as f64 / fw.max(fh).max(1) as f64;
        let min_edge = (f64::from(AUTO_TONE_SAMPLE_EDGE) / kept.max(1e-3)).ceil() as u32;
        let level = Arc::clone(&open.pyramid.levels()[open.pyramid.select_index(min_edge)]);
        let as_shot_white = open.as_shot_white;
        let fill = PlanExtras::of(&self.shared, &open, &recipe);
        let shared = Arc::clone(&self.shared);
        let spec = JobSpec::new(Lane::Interactive, Priority::VisiblePreview, "auto-tone")
            .superseding("auto-tone");
        self.jobs.submit(spec, move |token| {
            let t0 = Instant::now();
            let mut renders = 0;
            let mut measure = |r: &EditRecipe| {
                renders += 1;
                let plan = fill.plan(r, as_shot_white);
                shared
                    .renderer
                    .render(&plan, &level, PixelFormat::Rgb8, token)
                    .map(|out| renderer::auto_tone::ToneStats::of(&out))
            };
            let found = find(&recipe, &mut measure)?;
            log::info!(
                "auto from a {}x{} sample: {renders} renders in {:.0} ms",
                level.width(),
                level.height(),
                ms(t0)
            );
            Ok(found)
        })
    }

    /// The size an export of the open photo will have (ADR 0068): a sample of about
    /// [`export::estimate::SAMPLE_LONG_EDGE`] px is rendered with `recipe`, sharpened,
    /// converted and encoded as the export would be, and scaled to the export's pixel
    /// count. Tens of milliseconds, on the interactive lane; a newer estimate replaces
    /// one still running.
    pub fn estimate_export(
        &self,
        image: ImageId,
        recipe: &EditRecipe,
        format: ExportFormat,
        long_edge: Option<u32>,
        sharpening: export::sharpen::OutputSharpening,
        colour_space: export::colour::ExportColourSpace,
    ) -> JobHandle<ExportEstimate, EngineError> {
        let Some(open) = self.shared.images.lock().expect("images lock").get(image) else {
            return JobHandle::ready(
                self.jobs.next_id(),
                Err(jobs::JobError::Failed(EngineError::image_not_open())),
            );
        };
        let recipe = recipe.sanitized();
        let (fw, fh) = open.full_size;
        let full_output = recipe.geometry.map_or((fw, fh), |g| g.output_size(fw, fh));
        // What the crop keeps decides the level, as for previews.
        let kept = full_output.0.max(full_output.1) as f64 / fw.max(fh).max(1) as f64;
        let min_edge =
            (f64::from(export::estimate::SAMPLE_LONG_EDGE) / kept.max(1e-3)).ceil() as u32;
        let level = Arc::clone(&open.pyramid.levels()[open.pyramid.select_index(min_edge)]);
        let as_shot_white = open.as_shot_white;
        let fill = PlanExtras::of(&self.shared, &open, &recipe);
        let shared = Arc::clone(&self.shared);
        let spec = JobSpec::new(
            Lane::Interactive,
            Priority::VisiblePreview,
            "export-estimate",
        )
        .superseding("export-estimate");
        self.jobs.submit(spec, move |token| {
            let plan = fill.plan(&recipe, as_shot_white);
            let render_format = if colour_space == export::colour::ExportColourSpace::Srgb {
                format.pixel_format()
            } else {
                image_core::PixelFormat::Rgb16
            };
            let sample = shared
                .renderer
                .render(&plan, &level, render_format, token)?;
            let sample = export::sharpen::sharpen(&sample, sharpening);
            let sample = export::colour::convert(&sample, colour_space, format.pixel_format());
            let sample_bytes = export::encode_in(&sample, format, colour_space, None)?.len() as u64;
            let sample_pixels = u64::from(sample.width()) * u64::from(sample.height());
            let long = full_output.0.max(full_output.1).max(1);
            let s = long_edge.map_or(1.0, |e| (f64::from(e) / f64::from(long)).min(1.0));
            let width = (f64::from(full_output.0) * s).round().max(1.0) as u32;
            let height = (f64::from(full_output.1) * s).round().max(1.0) as u32;
            let target_pixels = u64::from(width) * u64::from(height);
            Ok(ExportEstimate {
                bytes: export::estimate::scale(sample_bytes, sample_pixels, target_pixels, format),
                width,
                height,
                sample_pixels,
                sample_bytes,
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
        self.export_file(
            FileExport {
                source: image.path.clone(),
                recipe: req.recipe,
                destination: req.destination,
                format: req.format,
                sharpening: req.sharpening,
                colour_space: req.colour_space,
                metadata: req.metadata,
                judgements: req.judgements,
                watermark: req.watermark.clone(),
                long_edge: None,
            },
            progress,
        )
    }

    /// Exports a photo from its file (ADR 0050), at full resolution or fitted to a long
    /// edge, on the background lane. The photo need not be open.
    pub fn export_file(
        &self,
        req: FileExport,
        progress: impl Fn(ExportProgress) + Send + 'static,
    ) -> JobHandle<ExportSummary, EngineError> {
        if let Err(e) = export::validate_destination(&req.destination, &req.source, req.format) {
            return JobHandle::ready(self.jobs.next_id(), Err(jobs::JobError::Failed(e.into())));
        }
        let shared = Arc::clone(&self.shared);
        let spec = JobSpec::new(Lane::Background, Priority::Export, "export");
        self.jobs
            .submit(spec, move |token| shared.export(&req, token, &progress))
    }

    /// The file an open image was read from (for exporting it by path).
    pub fn image_path(&self, id: ImageId) -> Option<PathBuf> {
        self.shared
            .images
            .lock()
            .expect("images lock")
            .get(id)
            .map(|image| image.path.clone())
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
    /// Stored mask `name`, if it was made from the photo whose fingerprint is `photo`.
    fn usable_mask(
        &self,
        name: &str,
        photo: u64,
    ) -> Option<Arc<renderer::masks::brush::CoverageMap>> {
        crate::mask_store::belongs(name, photo)
            .then(|| self.masks.get(name))
            .flatten()
    }

    /// The coverage of the generated masks `recipe` names that the store has for the
    /// photo whose fingerprint is `photo` (ADR 0074), and how many it names that it
    /// doesn't.
    fn generated_masks(
        &self,
        recipe: &EditRecipe,
        photo: u64,
    ) -> (Arc<renderer::masks::GeneratedMasks>, usize) {
        let mut missing = 0;
        let found = generated_in(recipe)
            .into_iter()
            .filter_map(|(name, _)| {
                let map = self.usable_mask(&name, photo);
                missing += usize::from(map.is_none());
                map.map(|m| (name, m))
            })
            .collect();
        (Arc::new(found), missing)
    }

    /// A `kind` mask of `source`, kept in the mask store; `None` when the photo has
    /// nothing of the kind.
    fn make_mask(
        &self,
        source: &MaskSource<'_>,
        kind: ai::MaskKind,
        token: &CancelToken,
    ) -> Result<Option<GeneratedMask>, EngineError> {
        let segmenter = &self.segmenter;
        let t0 = Instant::now();
        // The photo as it looks unedited: what the model was made for.
        let plan = RenderPlan::from_recipe(&EditRecipe::default(), source.as_shot_white);
        let picture = self
            .renderer
            .render(&plan, source.image, PixelFormat::Rgba8, token)?;
        if token.is_cancelled() {
            return Err(EngineError::cancelled());
        }
        let coverage = segmenter.segment(
            ai::Picture {
                width: picture.width(),
                height: picture.height(),
                rgba: picture.data(),
                scene_ev: source.scene_ev,
            },
            kind,
        )?;
        let ms = ms(t0);
        log::info!(
            "{kind:?} mask of a {}x{} picture in {ms:.0} ms: {}",
            picture.width(),
            picture.height(),
            coverage.as_ref().map_or("none".to_owned(), |c| format!(
                "{:.1} % covered",
                c.share() * 100.0
            ))
        );
        let Some(coverage) = coverage else {
            return Ok(None);
        };
        // Kept, so recipes can name it and every render can find it.
        let generator = segmenter.generator(kind);
        let name = self
            .masks
            .put(&coverage, &generator, source.photo)
            .map_err(|e| {
                EngineError::new(
                    crate::ErrorKind::Internal,
                    "The mask could not be kept.",
                    e.to_string(),
                )
            })?;
        Ok(Some(GeneratedMask {
            kind,
            name,
            generator,
            coverage: Arc::new(coverage),
            ms,
        }))
    }

    /// `generated` completed for an export of `image` (ADR 0074): each mask `recipe`
    /// names that it lacks is made again from the photo being exported, under the
    /// recipe's name for it. Masks this computer can't make stay missing (they cover
    /// nothing), with a warning.
    fn remake_masks(
        &self,
        recipe: &EditRecipe,
        source: &MaskSource<'_>,
        generated: &mut renderer::masks::GeneratedMasks,
        token: &CancelToken,
    ) -> Result<(), EngineError> {
        let mut reduced_image = None;
        for (name, of) in generated_in(recipe) {
            if generated.contains_key(&name) {
                continue;
            }
            let kind = mask_kind(of);
            if !self.segmenter.supports(kind) {
                log::warn!("export: no {kind:?} masks on this computer; mask {name} left out");
                continue;
            }
            let image =
                reduced_image.get_or_insert_with(|| reduced(source.image, SEGMENT_LONG_EDGE));
            let made = self.make_mask(&MaskSource { image, ..*source }, kind, token)?;
            if let Some(map) = made.and_then(|m| self.masks.get(&m.name)) {
                generated.insert(name, map);
            }
        }
        Ok(())
    }

    /// The open photo at full resolution, decoded now (and kept) if it hasn't been.
    fn full_source(
        &self,
        open: &OpenedImage,
        token: &CancelToken,
    ) -> Result<Arc<image_core::LinearImage>, EngineError> {
        let _decoding = open.full_decode.lock().expect("full decode lock");
        if let Some(full) = open.full() {
            return Ok(full);
        }
        let t0 = Instant::now();
        let decoded = self.decoders.decode(
            &open.path,
            DecodeOptions::new(DecodeScale::Full).with_max_threads(rayon::current_num_threads()),
            token,
        )?;
        if token.is_cancelled() {
            return Err(EngineError::cancelled());
        }
        log::info!(
            "full resolution of {} ({}x{}) decoded in {:.0} ms",
            open.path.display(),
            decoded.image.width(),
            decoded.image.height(),
            ms(t0)
        );
        let full = Arc::new(decoded.image);
        *open.full.lock().expect("full source lock") = Some(Arc::clone(&full));
        Ok(full)
    }

    fn open(
        &self,
        path: &Path,
        token: &CancelToken,
        on_preview: Option<impl FnOnce(EmbeddedFrame)>,
    ) -> Result<ImageSummary, EngineError> {
        let t0 = Instant::now();
        let identity = SourceIdentity::from_path(path).map_err(raw::DecodeError::from)?;
        let lens = crate::lens::of(&identity.canonical_path);
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
            lens_profile: lens.as_ref().map(|(_, name)| name.clone()),
            upright: Turn::from_exif(info.orientation),
        };
        let opened = Arc::new(OpenedImage {
            id,
            path: identity.canonical_path.clone(),
            source_id: identity.source_id(),
            fingerprint: identity.fingerprint,
            scene_ev: ai::scene_ev(info.iso, info.aperture, info.shutter_seconds),
            lens,
            upright: Turn::from_exif(info.orientation),
            pyramid,
            as_shot_white: info.as_shot_white,
            full_size: (info.full_width, info.full_height),
            full: Mutex::new(None),
            full_decode: Mutex::new(()),
            fill: Mutex::new(None),
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
        req: &FileExport,
        token: &CancelToken,
        progress: &dyn Fn(ExportProgress),
    ) -> Result<ExportSummary, EngineError> {
        let start = Instant::now();
        progress(ExportProgress {
            stage: ExportStage::Decoding,
            fraction: 0.0,
        });
        // A sized export decodes at the smallest scale that still fills it. The crop
        // keeps only part of the frame; its long edge is at least the smaller of its two
        // fractions times the frame's, so the frame must be larger by that much.
        let scale = match req.long_edge {
            None => DecodeScale::Full,
            Some(edge) => {
                let kept = req
                    .recipe
                    .geometry
                    .map_or(1.0, |g| f64::from(g.crop.w.min(g.crop.h)).clamp(0.01, 1.0));
                DecodeScale::AtLeast((f64::from(edge) / kept).ceil() as u32)
            }
        };
        let decoded = self.decoders.decode(
            &req.source,
            // Inside the background lane this is its bounded pool size, so LibRaw's
            // internal threads cannot take the cores interactive renders need.
            DecodeOptions::new(scale).with_max_threads(rayon::current_num_threads()),
            token,
        )?;
        let decode_ms = ms(start);

        progress(ExportProgress {
            stage: ExportStage::Rendering,
            fraction: 0.6,
        });
        let t = Instant::now();
        // An edit made on the file as stored, adapted to it upright (ADR 0078).
        let recipe = req
            .recipe
            .on_upright(Turn::from_exif(decoded.info.orientation));
        let lens = crate::lens::applied(&recipe, crate::lens::of(&req.source).map(|(l, _)| l));
        let mut plan = RenderPlan::from_recipe(&recipe, decoded.info.as_shot_white).with_lens(lens);
        if !recipe.masks.is_empty() {
            let photo = SourceIdentity::from_path(&req.source)
                .map_err(raw::DecodeError::from)?
                .fingerprint;
            let (found, missing) = self.generated_masks(&recipe, photo);
            let mut found = Arc::unwrap_or_clone(found);
            if missing > 0 {
                // An edit pasted onto a photo not yet opened: its masks are made now.
                let info = &decoded.info;
                let source = MaskSource {
                    image: &decoded.image,
                    as_shot_white: info.as_shot_white,
                    scene_ev: ai::scene_ev(info.iso, info.aperture, info.shutter_seconds),
                    photo,
                };
                self.remake_masks(&recipe, &source, &mut found, token)?;
            }
            plan.generated_masks = Arc::new(found);
        }
        // The open photo's full-resolution fill, once made: a sized export shows the
        // removals as the viewer does (ADR 0070), not filled again at its own size.
        if !plan.removals.is_empty() {
            let open = self
                .images
                .lock()
                .expect("images lock")
                .by_path(&req.source);
            plan.removal_fill = open.and_then(|o| o.fill_for(&plan.removals));
        }
        // Another colour space is converted from a 16-bit render, so the file is
        // rounded once, at its own depth (ADR 0062).
        let render_format = if req.colour_space == export::colour::ExportColourSpace::Srgb {
            req.format.pixel_format()
        } else {
            image_core::PixelFormat::Rgb16
        };
        let rendered = self
            .renderer
            .render(&plan, &decoded.image, render_format, token)?;
        drop(decoded);
        let rendered = match req.long_edge {
            Some(edge) => export::resize::fit_long_edge(&rendered, edge),
            None => rendered,
        };
        // Sharpened for its medium at the size it is written (ADR 0059).
        let rendered = export::sharpen::sharpen(&rendered, req.sharpening);
        // The watermark (ADR 0069) at the size written, after sharpening so its edges
        // stay as drawn.
        let rendered = match &req.watermark {
            Some(w) => export::watermark::apply(&rendered, w),
            None => rendered,
        };
        // In the chosen colour space (ADR 0062), rounded to the file's depth once.
        let rendered =
            export::colour::convert(&rendered, req.colour_space, req.format.pixel_format());
        let render_ms = ms(t);

        progress(ExportProgress {
            stage: ExportStage::Encoding,
            fraction: 0.8,
        });
        let t = Instant::now();
        // The source's capture facts (ADR 0063), read from its header: a few
        // milliseconds. A file whose header cannot be read still exports, without them.
        let entries = (req.metadata != export::metadata::MetadataChoice::None)
            .then(|| {
                let facts = self
                    .decoders
                    .read_metadata(&req.source)
                    .map(|m| capture_facts(&m))
                    .unwrap_or_else(|e| {
                        log::warn!("no metadata for {}: {e}", req.source.display());
                        export::metadata::CaptureFacts::default()
                    });
                export::metadata::entries(
                    &facts,
                    &req.judgements,
                    req.metadata,
                    rendered.width(),
                    rendered.height(),
                    req.colour_space,
                )
            })
            .flatten();
        let bytes = export::encode_in(&rendered, req.format, req.colour_space, entries.as_ref())?;
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

/// The photographer's marks as an export writes them (ADR 0067). Picks have no common
/// XMP property; a reject is written in place of the rating.
pub fn judgements(marks: &catalogue::Marks) -> export::metadata::Judgements {
    use export::metadata::LabelName;
    export::metadata::Judgements {
        rating: marks.rating.stars(),
        rejected: marks.flag == catalogue::Flag::Reject,
        label: match marks.label {
            catalogue::ColourLabel::None => None,
            catalogue::ColourLabel::Red => Some(LabelName::Red),
            catalogue::ColourLabel::Yellow => Some(LabelName::Yellow),
            catalogue::ColourLabel::Green => Some(LabelName::Green),
            catalogue::ColourLabel::Blue => Some(LabelName::Blue),
            catalogue::ColourLabel::Purple => Some(LabelName::Purple),
        },
    }
}

/// What an export copies from the photo's metadata (ADR 0063).
fn capture_facts(m: &raw::PhotoMetadata) -> export::metadata::CaptureFacts {
    export::metadata::CaptureFacts {
        camera_make: m.camera_make.clone(),
        camera_model: m.camera_model.clone(),
        lens: m.lens.clone(),
        captured_at: m.captured_at.clone(),
        iso: m.iso,
        aperture: m.aperture,
        shutter_seconds: m.shutter_seconds,
        focal_length_mm: m.focal_length_mm,
        gps: m.gps,
    }
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

/// The removals of `removals` that paint something, as render plans keep them.
fn painting(removals: &[renderer::remove::Removal]) -> Vec<renderer::remove::Removal> {
    removals
        .iter()
        .map(renderer::remove::Removal::sanitized)
        .filter(|r| !r.is_noop())
        .collect()
}

/// Where a render's removals come from (ADR 0070).
enum FillState {
    /// No removals.
    None,
    /// The full-resolution fill isn't made yet: filled at the rendered size.
    Pending,
    /// The full-resolution fill, scaled.
    Ready(Arc<renderer::remove::Fill>),
}

/// What a render of an open photo needs beyond its recipe and source: where its
/// removals' fill comes from (ADR 0070), and its generated masks' coverage (ADR 0074).
/// What a generated mask is made from: the photo as decoded (at some size), its as-shot
/// light, how bright the scene was, and its fingerprint.
#[derive(Clone, Copy)]
struct MaskSource<'a> {
    image: &'a image_core::LinearImage,
    as_shot_white: Option<image_core::Chromaticity>,
    scene_ev: Option<f32>,
    photo: u64,
}

struct PlanExtras {
    fill: FillState,
    /// How the photo was turned upright (ADR 0078).
    upright: Turn,
    /// The photo's lens corrections, when the recipe applies them (ADR 0075).
    lens: Option<renderer::lens::LensCorrection>,
    generated: Arc<renderer::masks::GeneratedMasks>,
    /// Generated masks the recipe names that the store doesn't have.
    missing: usize,
}

impl PlanExtras {
    fn of(shared: &Shared, open: &OpenedImage, recipe: &EditRecipe) -> Self {
        let removals = painting(&recipe.removals);
        let fill = if removals.is_empty() {
            FillState::None
        } else {
            open.fill_for(&removals)
                .map_or(FillState::Pending, FillState::Ready)
        };
        let (generated, missing) = shared.generated_masks(recipe, open.fingerprint);
        Self {
            fill,
            upright: open.upright,
            lens: crate::lens::applied(recipe, open.lens.as_ref().map(|(l, _)| *l)),
            generated,
            missing,
        }
    }

    fn pending(&self) -> bool {
        matches!(self.fill, FillState::Pending)
    }

    /// `bytes` (a render's cache key) marked with where its removals come from and how
    /// many generated masks were missing, so a render with the full-resolution fill or
    /// a found mask never answers for one without.
    fn keyed(&self, mut bytes: Vec<u8>) -> Vec<u8> {
        bytes.push(match self.fill {
            FillState::None => 0,
            FillState::Pending => 1,
            FillState::Ready(_) => 2,
        });
        bytes.extend((self.missing as u32).to_le_bytes());
        bytes
    }

    fn plan(&self, recipe: &EditRecipe, white: Option<image_core::Chromaticity>) -> RenderPlan {
        // An edit made on the file as stored, adapted to it upright (ADR 0078); the
        // editor has it adapted already.
        let adapted;
        let recipe = if recipe.unoriented {
            adapted = recipe.on_upright(self.upright);
            &adapted
        } else {
            recipe
        };
        let mut plan = RenderPlan::from_recipe(recipe, white).with_lens(self.lens);
        if let FillState::Ready(fill) = &self.fill {
            plan.removal_fill = Some(Arc::clone(fill));
        }
        plan.generated_masks = Arc::clone(&self.generated);
        plan
    }
}

/// The recipe's name for a kind of generated mask (ADR 0074).
pub fn generated_kind(kind: ai::MaskKind) -> renderer::masks::GeneratedKind {
    match kind {
        ai::MaskKind::Subject => renderer::masks::GeneratedKind::Subject,
        ai::MaskKind::People => renderer::masks::GeneratedKind::People,
        ai::MaskKind::Sky => renderer::masks::GeneratedKind::Sky,
    }
}

/// The AI subsystem's name for a kind of generated mask (ADR 0074).
pub fn mask_kind(kind: renderer::masks::GeneratedKind) -> ai::MaskKind {
    match kind {
        renderer::masks::GeneratedKind::Subject => ai::MaskKind::Subject,
        renderer::masks::GeneratedKind::People => ai::MaskKind::People,
        renderer::masks::GeneratedKind::Sky => ai::MaskKind::Sky,
    }
}

/// The generated masks `recipe` names (ADR 0074), and what each covers.
fn generated_in(recipe: &EditRecipe) -> Vec<(String, renderer::masks::GeneratedKind)> {
    use renderer::masks::MaskShape;
    let mut names: Vec<_> = recipe
        .masks
        .iter()
        .flat_map(|m| std::iter::once(&m.shape).chain(m.parts.iter().map(|p| &p.shape)))
        .filter_map(|shape| match shape.clone().sanitized() {
            MaskShape::Generated { of, mask } if !mask.is_empty() => Some((mask, of)),
            _ => None,
        })
        .collect();
    names.sort_by(|a, b| a.0.cmp(&b.0));
    names.dedup_by(|a, b| a.0 == b.0);
    names
}

/// `image` halved while it stays at least `long_edge` px on its long edge.
fn reduced(image: &image_core::LinearImage, long_edge: u32) -> image_core::LinearImage {
    let mut out = image_core::pyramid::downsample_2x(image);
    if out.long_edge() < long_edge {
        return image.clone();
    }
    while out.long_edge() / 2 >= long_edge {
        out = image_core::pyramid::downsample_2x(&out);
    }
    out
}
