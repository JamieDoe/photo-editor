//! Library thumbnails: small JPEGs made from the camera's embedded preview (or, if a
//! file has none, a reduced decode and default render), kept in a bounded disk cache.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, UNIX_EPOCH};

use cache::{DiskCache, Fnv64};
use export::ExportFormat;
use image_core::resize::fit_long_edge;
use image_core::{OutputImage, PixelFormat};
use jobs::{CancelToken, JobError, JobHandle, JobSpec, Lane, Priority};
use raw::{DecodeError, DecodeOptions, DecodeScale};
use renderer::{EditRecipe, RENDERER_VERSION, RenderBackend, RenderPlan};

use crate::engine::Shared;
use crate::{Engine, EngineError, ErrorKind};

/// Long edge of library thumbnails, in pixels. Sharp on a high-DPI screen for grid
/// tiles up to ~256 CSS pixels.
pub const THUMBNAIL_LONG_EDGE: u32 = 512;
/// Bump whenever thumbnail output changes, so cached thumbnails are regenerated.
pub const THUMBNAIL_VERSION: u64 = 1;
const JPEG_QUALITY: u8 = 80;
/// Pre-generation stops at the number of thumbnails that fit the cache budget at this
/// (generous) average size, so a very large library never churns the cache.
const PREGEN_BYTES_PER_THUMBNAIL: u64 = 64 * 1024;

/// How a thumbnail was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailSource {
    /// Read from the disk cache.
    Cache,
    /// Made from the camera's embedded preview (or a reduced JPEG decode).
    Embedded,
    /// The file had no usable embedded preview: decoded and rendered with default
    /// settings.
    Rendered,
}

#[derive(Debug, Clone)]
pub struct Thumbnail {
    /// A JPEG, long edge at most [`THUMBNAIL_LONG_EDGE`].
    pub jpeg: Vec<u8>,
    pub source: ThumbnailSource,
    /// Time to produce it (zero for cache hits).
    pub ms: f64,
}

impl Engine {
    /// The thumbnail of `path` (a canonical path), showing `recipe` if the photo is
    /// edited. A disk-cache hit completes immediately, after a small blocking read, so
    /// call this off the UI thread. Otherwise it is generated on the browse lane.
    /// Requesting the same path again supersedes the earlier request, and
    /// [`Engine::cancel_thumbnail`] cancels it.
    pub fn thumbnail(
        &self,
        path: PathBuf,
        recipe: Option<EditRecipe>,
    ) -> JobHandle<Thumbnail, EngineError> {
        let recipe = recipe.filter(|r| !r.is_identity());
        let key = match thumbnail_key(&path, recipe.as_ref()) {
            Ok(key) => key,
            Err(e) => return JobHandle::ready(self.jobs.next_id(), Err(JobError::Failed(e))),
        };
        if let Some(hit) = self.shared.cached_thumbnail(key) {
            return JobHandle::ready(self.jobs.next_id(), Ok(hit));
        }
        let shared = Arc::clone(&self.shared);
        let spec = JobSpec::new(Lane::Browse, Priority::VisibleThumbnail, "thumbnail")
            .superseding(supersede_key(&path));
        self.jobs.submit(spec, move |token| {
            shared.make_thumbnail(&path, key, recipe.as_ref(), token)
        })
    }

    /// Cancels a pending thumbnail request for `path` (it scrolled out of view).
    pub fn cancel_thumbnail(&self, path: &Path) {
        self.jobs.cancel_key(&supersede_key(path));
    }

    /// Makes missing thumbnails for `paths` in the background, so browsing later finds
    /// them cached (PRODUCT.md workflow D). One small job per photo on the background
    /// lane at idle priority: exports, indexing and on-screen thumbnails (their own
    /// lane) always go first, and cancelling costs at most one thumbnail. A new batch
    /// with the same `key` cancels the previous one. Without a cache this does nothing.
    pub fn pregenerate_thumbnails(
        &self,
        key: &str,
        mut paths: Vec<(PathBuf, Option<EditRecipe>)>,
    ) -> ThumbnailBatch {
        let token = CancelToken::new();
        if let Some(previous) = self
            .shared
            .thumbnail_batches
            .lock()
            .expect("batches lock")
            .insert(key.to_owned(), token.clone())
        {
            previous.cancel();
        }
        let Some(cache) = &self.shared.thumbnails else {
            return ThumbnailBatch {
                token,
                handles: Vec::new(),
            };
        };
        let limit = (cache.budget_bytes() / PREGEN_BYTES_PER_THUMBNAIL) as usize;
        if paths.len() > limit {
            log::info!(
                "pre-generating the first {limit} of {} thumbnails (cache budget)",
                paths.len()
            );
            paths.truncate(limit);
        }
        let handles = paths
            .into_iter()
            .map(|(path, recipe)| {
                let shared = Arc::clone(&self.shared);
                let batch = token.clone();
                let recipe = recipe.filter(|r| !r.is_identity());
                let spec = JobSpec::new(Lane::Background, Priority::Idle, "thumbnail-pregen");
                self.jobs.submit(spec, move |_| {
                    shared.pregenerate_one(&path, recipe.as_ref(), &batch)
                })
            })
            .collect();
        ThumbnailBatch { token, handles }
    }

    /// Entries and bytes in the thumbnail cache (scans its directory).
    pub fn thumbnail_cache_stats(&self) -> Option<cache::DiskCacheStats> {
        self.shared.thumbnails.as_ref().map(DiskCache::stats)
    }
}

/// A running thumbnail pre-generation batch. Dropping it does not stop the work.
pub struct ThumbnailBatch {
    token: CancelToken,
    handles: Vec<JobHandle<Pregenerated, EngineError>>,
}

/// What pre-generation did for one photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pregenerated {
    Made,
    AlreadyCached,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BatchSummary {
    pub made: usize,
    pub already_cached: usize,
    pub failed: usize,
    pub cancelled: usize,
}

impl ThumbnailBatch {
    /// Photos submitted (after the cache-budget cap).
    pub fn len(&self) -> usize {
        self.handles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    /// Skips every photo not yet started.
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// Waits for every photo (tests and benchmarks).
    pub fn wait(self) -> BatchSummary {
        let mut s = BatchSummary::default();
        for h in self.handles {
            match h.wait() {
                Ok(Pregenerated::Made) => s.made += 1,
                Ok(Pregenerated::AlreadyCached) => s.already_cached += 1,
                Err(JobError::Cancelled) => s.cancelled += 1,
                Err(JobError::Failed(e)) if e.kind == ErrorKind::Cancelled => s.cancelled += 1,
                Err(_) => s.failed += 1,
            }
        }
        s
    }
}

impl Shared {
    fn pregenerate_one(
        &self,
        path: &Path,
        recipe: Option<&EditRecipe>,
        batch: &CancelToken,
    ) -> Result<Pregenerated, EngineError> {
        if batch.is_cancelled() {
            return Err(EngineError::cancelled());
        }
        let key = thumbnail_key(path, recipe)?;
        if self.thumbnails.as_ref().is_some_and(|c| c.contains(key)) {
            return Ok(Pregenerated::AlreadyCached);
        }
        self.make_thumbnail(path, key, recipe, batch)?;
        Ok(Pregenerated::Made)
    }

    fn cached_thumbnail(&self, key: u64) -> Option<Thumbnail> {
        let cache = self.thumbnails.as_ref()?;
        let jpeg = cache.get(key)?;
        if !is_complete_jpeg(&jpeg) {
            // Left half-written by a crash or power loss: regenerate.
            cache.remove(key);
            return None;
        }
        Some(Thumbnail {
            jpeg,
            source: ThumbnailSource::Cache,
            ms: 0.0,
        })
    }

    fn make_thumbnail(
        &self,
        path: &Path,
        key: u64,
        recipe: Option<&EditRecipe>,
        token: &CancelToken,
    ) -> Result<Thumbnail, EngineError> {
        // An earlier request may have produced it while this one was queued.
        if let Some(hit) = self.cached_thumbnail(key) {
            return Ok(hit);
        }
        let start = Instant::now();
        let preview = match recipe {
            // Edited: the camera's preview doesn't show the edit, so render it.
            Some(_) => Ok(None),
            None => self
                .decoders
                .display_preview(path, THUMBNAIL_LONG_EDGE, token),
        };
        let (image, source) = match preview {
            Ok(Some(p)) if p.image.width().max(p.image.height()) >= THUMBNAIL_LONG_EDGE => {
                (p.image, ThumbnailSource::Embedded)
            }
            Err(DecodeError::Cancelled) => return Err(EngineError::cancelled()),
            // Missing, too small or unreadable: the real decode below decides.
            other => {
                if let Err(e) = other {
                    log::debug!("no display preview for {}: {e}", path.display());
                }
                let recipe = recipe.copied().unwrap_or_default();
                (
                    self.render_thumbnail(path, &recipe, token)?,
                    ThumbnailSource::Rendered,
                )
            }
        };
        if token.is_cancelled() {
            return Err(EngineError::cancelled());
        }
        let image = fit_long_edge(image, THUMBNAIL_LONG_EDGE);
        let jpeg = export::encode(
            &image,
            ExportFormat::Jpeg {
                quality: JPEG_QUALITY,
            },
        )
        .map_err(|e| {
            EngineError::new(
                ErrorKind::Internal,
                "The thumbnail could not be created.",
                e.to_string(),
            )
        })?;
        if let Some(cache) = &self.thumbnails
            && let Err(e) = cache.put(key, &jpeg)
        {
            // Still usable this time; it is regenerated next time.
            log::warn!("thumbnail cache write failed: {e}");
        }
        Ok(Thumbnail {
            jpeg,
            source,
            ms: start.elapsed().as_secs_f64() * 1000.0,
        })
    }

    /// A reduced-resolution decode rendered with `recipe`: for edited photos, and for
    /// files without a usable embedded preview (with the default recipe).
    fn render_thumbnail(
        &self,
        path: &Path,
        recipe: &EditRecipe,
        token: &CancelToken,
    ) -> Result<OutputImage, EngineError> {
        let options = DecodeOptions::new(DecodeScale::AtLeast(THUMBNAIL_LONG_EDGE))
            .with_max_threads(rayon::current_num_threads());
        let decoded = self.decoders.decode(path, options, token)?;
        let plan = RenderPlan::from_recipe(recipe);
        Ok(self
            .renderer
            .render(&plan, &decoded.image, PixelFormat::Rgb8, token)?)
    }
}

/// Cache key: the source's identity (path, size, modification time), the edit recipe
/// (if any), the thumbnail size and the versions of everything that shapes the output.
fn thumbnail_key(path: &Path, recipe: Option<&EditRecipe>) -> Result<u64, EngineError> {
    let meta = std::fs::metadata(path).map_err(DecodeError::from)?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    let mut h = Fnv64::new();
    h.write(path.as_os_str().as_encoded_bytes())
        .write_u64(meta.len())
        .write(&modified.to_le_bytes())
        .write_u64(u64::from(THUMBNAIL_LONG_EDGE))
        .write_u64(THUMBNAIL_VERSION)
        .write_u64(u64::from(RENDERER_VERSION));
    if let Some(recipe) = recipe {
        h.write(&recipe.canonical_bytes());
    }
    Ok(h.finish())
}

fn supersede_key(path: &Path) -> String {
    format!("library-thumbnail:{}", path.display())
}

/// Starts with a JPEG start-of-image marker and ends with end-of-image.
fn is_complete_jpeg(bytes: &[u8]) -> bool {
    bytes.len() > 4 && bytes.starts_with(&[0xFF, 0xD8]) && bytes.ends_with(&[0xFF, 0xD9])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_changes_with_content_and_path() {
        let dir = fixtures::TempDir::new("thumb-key");
        let a = dir.path().join("a.jpg");
        let b = dir.path().join("b.jpg");
        std::fs::write(&a, b"one").unwrap();
        std::fs::write(&b, b"one").unwrap();
        let ka = thumbnail_key(&a, None).unwrap();
        assert_eq!(ka, thumbnail_key(&a, None).unwrap());
        assert_ne!(ka, thumbnail_key(&b, None).unwrap());
        std::fs::write(&a, b"longer").unwrap();
        assert_ne!(ka, thumbnail_key(&a, None).unwrap());
        assert!(thumbnail_key(&dir.path().join("missing.jpg"), None).is_err());
    }

    /// Occupies the browse lane's only worker until the returned sender is dropped.
    fn block_browse_lane(engine: &Engine) -> (std::sync::mpsc::Sender<()>, JobHandle<(), ()>) {
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        let handle = engine.jobs.submit(
            JobSpec::new(Lane::Browse, Priority::Interactive, "blocker"),
            move |_| {
                started_tx.send(()).unwrap();
                let _ = release_rx.recv();
                Ok(())
            },
        );
        started_rx.recv().unwrap();
        (release_tx, handle)
    }

    fn one_browse_worker() -> Engine {
        let mut config = crate::EngineConfig::default();
        config.jobs.browse.workers = 1;
        Engine::new(config)
    }

    #[test]
    fn cancelled_and_superseded_requests_do_no_work() {
        let dir = fixtures::TempDir::new("thumb-cancel");
        let path = dir.path().join("a.jpg");
        std::fs::write(&path, fixtures::chart_jpeg(1024, 768, 90)).unwrap();
        let engine = one_browse_worker();
        let (release, blocker) = block_browse_lane(&engine);

        let scrolled_away = engine.thumbnail(path.clone(), None);
        engine.cancel_thumbnail(&path);
        let superseded = engine.thumbnail(path.clone(), None);
        let current = engine.thumbnail(path.clone(), None);
        drop(release);
        blocker.wait().unwrap();

        assert!(matches!(scrolled_away.wait(), Err(JobError::Cancelled)));
        assert!(matches!(superseded.wait(), Err(JobError::Cancelled)));
        assert_eq!(current.wait().unwrap().source, ThumbnailSource::Embedded);
        assert_eq!(engine.jobs.keyed_jobs(), 0, "no supersede keys left behind");
    }

    #[test]
    fn truncated_jpegs_are_rejected() {
        assert!(is_complete_jpeg(&[0xFF, 0xD8, 0, 0, 0xFF, 0xD9]));
        assert!(!is_complete_jpeg(&[0xFF, 0xD8, 0, 0, 0, 0]));
        assert!(!is_complete_jpeg(&[]));
    }
}
