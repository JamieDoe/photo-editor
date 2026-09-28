//! Library indexing: walks a folder tree and records its photos in the catalogue.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use catalogue::{Catalogue, CatalogueError, RecordOutcome, SourceIdentity};
use jobs::{CancelToken, JobHandle, JobSpec, Lane, Priority};
use rayon::prelude::*;

use crate::{Engine, EngineError, ErrorKind};

/// Files handled per catalogue transaction and progress report.
const BATCH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexProgress {
    /// Photos found by the folder walk.
    pub found: usize,
    /// Photos recorded so far.
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
    pub walk_ms: f64,
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
    /// on the background lane. Re-indexing the same folder supersedes a pass still
    /// running. A cancelled pass marks nothing missing.
    pub fn index_folder(
        &self,
        catalogue: Arc<Catalogue>,
        root: PathBuf,
        progress: impl Fn(IndexProgress) + Send + 'static,
    ) -> JobHandle<IndexSummary, EngineError> {
        let extensions = self.info().extensions;
        let spec = JobSpec::new(Lane::Background, Priority::Indexing, "index")
            .superseding(format!("index:{}", root.display()));
        self.jobs.submit(spec, move |token| {
            index(&catalogue, &root, &extensions, token, &progress)
        })
    }
}

fn index(
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
        found,
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
        walk_ms,
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
        for (_, outcome) in catalogue.record_files(folder, &identities, scan)? {
            match outcome {
                RecordOutcome::New => summary.new += 1,
                RecordOutcome::Changed => summary.changed += 1,
                RecordOutcome::Moved { .. } => summary.moved += 1,
                RecordOutcome::Unchanged => summary.unchanged += 1,
            }
        }
        progress(IndexProgress {
            found,
            processed: ((i + 1) * BATCH).min(found),
        });
    }

    summary.missing = catalogue.finish_scan(folder, scan, None)?;
    summary.total_ms = ms(start);
    log::info!(
        "indexed {} in {:.0} ms: {} found, {} new, {} changed, {} moved, {} unchanged, {} missing, {} skipped",
        root.display(),
        summary.total_ms,
        summary.found,
        summary.new,
        summary.changed,
        summary.moved,
        summary.unchanged,
        summary.missing,
        summary.skipped
    );
    Ok(summary)
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}
