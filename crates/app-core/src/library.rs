//! Library indexing: walks a folder tree, records its photos in the catalogue and
//! reads their details (camera, lens, capture time) from file headers.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use catalogue::{
    Catalogue, CatalogueError, FolderId, PhotoDetails, PhotoId, RecordOutcome, SourceIdentity,
};
use jobs::{CancelToken, JobHandle, JobSpec, Lane, Priority};
use rayon::prelude::*;

use crate::engine::Shared;
use crate::{Engine, EngineError, ErrorKind};

/// Files handled per catalogue transaction and progress report.
const BATCH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexStage {
    /// Recording files in the catalogue.
    Recording,
    /// Reading camera, lens and capture details of new or changed photos.
    ReadingDetails,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexProgress {
    pub stage: IndexStage,
    /// Items in this stage: photos found by the walk, or photos needing details.
    pub total: usize,
    /// Items handled so far in this stage.
    pub processed: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndexSummary {
    pub root: PathBuf,
    pub found: usize,
    pub new: usize,
    pub changed: usize,
    pub moved: usize,
    pub unchanged: usize,
    /// Previously indexed files not found this time (now marked missing).
    pub missing: usize,
    /// Files or folders that could not be read.
    pub skipped: usize,
    /// Photos whose details were read in this pass.
    pub details_read: usize,
    /// New photos that took their marks from another app's sidecar (ADR 0067).
    pub marks_from_sidecars: usize,
    pub walk_ms: f64,
    pub details_ms: f64,
    pub total_ms: f64,
}

impl From<CatalogueError> for EngineError {
    fn from(e: CatalogueError) -> Self {
        let message = match e {
            CatalogueError::Corrupt(_) => {
                "The library database is damaged. Restart the app to rebuild it from your folders."
            }
            _ => "The library could not be updated.",
        };
        Self::new(ErrorKind::Internal, message, e.to_string())
    }
}

impl Engine {
    /// Indexes `root` (a canonical, user-granted folder) and everything beneath it,
    /// on the background lane, then reads details of new or changed photos.
    /// Re-indexing the same folder supersedes a pass still running. A cancelled pass
    /// marks nothing missing, and details it did not reach stay queued.
    pub fn index_folder(
        &self,
        catalogue: Arc<Catalogue>,
        root: PathBuf,
        progress: impl Fn(IndexProgress) + Send + 'static,
    ) -> JobHandle<IndexSummary, EngineError> {
        let extensions = self.info().extensions;
        let shared = Arc::clone(&self.shared);
        let spec = JobSpec::new(Lane::Background, Priority::Indexing, "index")
            .superseding(format!("index:{}", root.display()));
        self.jobs.submit(spec, move |token| {
            index(&shared, &catalogue, &root, &extensions, token, &progress)
        })
    }
}

fn index(
    shared: &Shared,
    catalogue: &Catalogue,
    root: &Path,
    extensions: &[&str],
    token: &CancelToken,
    progress: &dyn Fn(IndexProgress),
) -> Result<IndexSummary, EngineError> {
    let start = Instant::now();
    let mut paths = Vec::new();
    let walk = folders::walk_photos(root, extensions, |p| {
        if token.is_cancelled() {
            return ControlFlow::Break(());
        }
        paths.push(p);
        ControlFlow::Continue(())
    })
    .map_err(|e| {
        EngineError::new(
            ErrorKind::NotFound,
            "This folder could not be read.",
            e.to_string(),
        )
    })?;
    if walk.stopped {
        return Err(EngineError::cancelled());
    }
    let walk_ms = ms(start);
    let found = paths.len();
    progress(IndexProgress {
        stage: IndexStage::Recording,
        total: found,
        processed: 0,
    });

    let folder = catalogue.add_folder(root)?;
    let scan = catalogue.begin_scan()?;
    let mut summary = IndexSummary {
        root: root.to_path_buf(),
        found,
        new: 0,
        changed: 0,
        moved: 0,
        unchanged: 0,
        missing: 0,
        skipped: walk.skipped,
        details_read: 0,
        marks_from_sidecars: 0,
        walk_ms,
        details_ms: 0.0,
        total_ms: 0.0,
    };

    for (i, batch) in paths.chunks(BATCH).enumerate() {
        if token.is_cancelled() {
            return Err(EngineError::cancelled());
        }
        // Stat (cheap), then skip fingerprinting for files the catalogue already
        // knows at the same path, size and modification time.
        let stats: Vec<(PathBuf, u64, u128)> = batch
            .par_iter()
            .filter_map(|p| {
                SourceIdentity::stat(p)
                    .ok()
                    .map(|(size, modified)| (p.clone(), size, modified))
            })
            .collect();
        summary.skipped += batch.len() - stats.len();
        let known = catalogue.touch_unchanged(&stats, scan)?;
        summary.unchanged += known.iter().filter(|k| **k).count();

        let to_fingerprint: Vec<_> = stats
            .into_iter()
            .zip(known)
            .filter(|(_, k)| !k)
            .map(|(s, _)| s)
            .collect();
        let identities: Vec<SourceIdentity> = to_fingerprint
            .into_par_iter()
            .filter_map(|(p, size, modified)| SourceIdentity::from_known(p, size, modified).ok())
            .collect();
        let recorded = catalogue.record_files(folder, &identities, scan)?;
        for (_, outcome) in &recorded {
            match outcome {
                RecordOutcome::New => summary.new += 1,
                RecordOutcome::Changed => summary.changed += 1,
                RecordOutcome::Moved { .. } => summary.moved += 1,
                RecordOutcome::Unchanged => summary.unchanged += 1,
            }
        }
        // Photos new to the library take the marks another app left in their sidecars
        // (ADR 0067), unless they have their own. Rendered images (JPEG, PNG, TIFF)
        // carry theirs inside, and a `NAME.xmp` beside one belongs to a RAW of the same
        // name.
        let from_sidecars: Vec<(PhotoId, catalogue::Marks)> = recorded
            .par_iter()
            .zip(identities.par_iter())
            .filter(|((_, outcome), id)| {
                matches!(outcome, RecordOutcome::New) && !is_rendered(&id.canonical_path)
            })
            .filter_map(|((photo, _), id)| {
                crate::sidecars::read(&id.canonical_path).map(|m| (*photo, m))
            })
            .collect();
        if !from_sidecars.is_empty() {
            summary.marks_from_sidecars += catalogue.import_marks(&from_sidecars)?;
        }
        progress(IndexProgress {
            stage: IndexStage::Recording,
            total: found,
            processed: ((i + 1) * BATCH).min(found),
        });
    }
    summary.missing = catalogue.finish_scan(folder, scan, None)?;

    let details_start = Instant::now();
    summary.details_read = read_details(shared, catalogue, folder, token, progress)?;
    summary.details_ms = ms(details_start);
    summary.total_ms = ms(start);
    log::info!(
        "indexed {} in {:.0} ms: {} found, {} new, {} changed, {} moved, {} unchanged, {} missing, {} skipped; \
         details read for {} in {:.0} ms; marks from {} sidecars",
        root.display(),
        summary.total_ms,
        summary.found,
        summary.new,
        summary.changed,
        summary.moved,
        summary.unchanged,
        summary.missing,
        summary.skipped,
        summary.details_read,
        summary.details_ms,
        summary.marks_from_sidecars
    );
    Ok(summary)
}

/// Reads details for every present photo in `folder` that needs them, in parallel
/// batches of [`BATCH`], each stored in one transaction. Resumable: whatever a
/// cancelled pass did not reach stays queued in the catalogue.
fn read_details(
    shared: &Shared,
    catalogue: &Catalogue,
    folder: FolderId,
    token: &CancelToken,
    progress: &dyn Fn(IndexProgress),
) -> Result<usize, EngineError> {
    let pending = catalogue.photos_needing_details(folder, usize::MAX)?;
    let total = pending.len();
    if total == 0 {
        return Ok(0);
    }
    progress(IndexProgress {
        stage: IndexStage::ReadingDetails,
        total,
        processed: 0,
    });
    let mut done = 0;
    for batch in pending.chunks(BATCH) {
        if token.is_cancelled() {
            return Err(EngineError::cancelled());
        }
        let details: Vec<(PhotoId, Option<PhotoDetails>)> = batch
            .par_iter()
            .map(|(photo, path)| {
                (
                    *photo,
                    shared.decoders.read_metadata(path).ok().map(to_details),
                )
            })
            .collect();
        catalogue.set_details(&details)?;
        done += details.len();
        progress(IndexProgress {
            stage: IndexStage::ReadingDetails,
            total,
            processed: done,
        });
    }
    Ok(done)
}

fn is_rendered(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(raw::is_rendered_extension)
}

fn to_details(m: raw::PhotoMetadata) -> PhotoDetails {
    PhotoDetails {
        camera_make: m.camera_make,
        camera_model: m.camera_model,
        lens: m.lens,
        captured_at: m.captured_at,
        iso: m.iso,
        aperture: m.aperture,
        shutter_seconds: m.shutter_seconds,
        focal_length_mm: m.focal_length_mm,
        width: m.width,
        height: m.height,
        rotation: m.rotation,
        gps: m.gps,
    }
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}
