//! The export queue (ADR 0050): photos exported one after another in the background.
//!
//! A full-resolution export holds the decoded photo (tens of bytes a pixel), so the
//! queue runs one at a time rather than filling memory with several. Photos added
//! while it runs join the same run. Each running export's cancel token is registered
//! in `AppState::exports`, so the quit guard sees it; cancelling (from the UI, or a
//! confirmed quit) stops the current export and drops the rest.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use app_core::{CancelToken, EditRecipe, ExportFormat, FileExport, JobError};
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;
use crate::ipc::{EXPORT_QUEUE_EVENT, ExportQueueEvent, ExportedFileDto, FileFailureDto};

/// A photo waiting to be exported.
#[derive(Debug, Clone)]
pub struct QueuedExport {
    pub source: PathBuf,
    pub recipe: EditRecipe,
    pub folder: PathBuf,
    pub long_edge: Option<u32>,
    pub format: ExportFormat,
    pub sharpening: app_core::OutputSharpening,
    pub colour_space: app_core::ExportColourSpace,
    pub metadata: app_core::MetadataChoice,
    /// The photo's marks, written as XMP with the metadata (ADR 0067).
    pub judgements: app_core::Judgements,
}

#[derive(Default)]
struct Run {
    pending: VecDeque<QueuedExport>,
    /// Photos in this run, done so far, and those that failed.
    total: u32,
    done: u32,
    failed: Vec<FileFailureDto>,
    outputs: Vec<ExportedFileDto>,
    /// The folder the last photo went to (for "Exported 12 photos to …").
    folder: Option<PathBuf>,
    /// Names given out in this run, so two photos never get the same one.
    reserved: HashSet<PathBuf>,
    running: bool,
    cancelled: bool,
    /// The running export's token.
    current: Option<CancelToken>,
}

#[derive(Default)]
pub struct ExportQueue {
    run: Mutex<Run>,
}

impl ExportQueue {
    /// Adds photos (and any that could not be queued, as failures) and starts the
    /// runner if it is not running. Returns the photos now in the run.
    pub fn add(
        &self,
        app: &AppHandle,
        items: Vec<QueuedExport>,
        refused: Vec<FileFailureDto>,
    ) -> u32 {
        let start = {
            let mut run = self.run.lock().expect("export queue lock");
            if !run.running {
                *run = Run::default();
            }
            run.total += (items.len() + refused.len()) as u32;
            run.done += refused.len() as u32;
            run.failed.extend(refused);
            run.pending.extend(items);
            let start = !run.running;
            run.running = true;
            start
        };
        if start {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { run_queue(app).await });
        }
        self.run.lock().expect("export queue lock").total
    }

    /// Stops the run: cancels the photo exporting now and drops the rest.
    pub fn cancel(&self) {
        let mut run = self.run.lock().expect("export queue lock");
        if !run.running {
            return;
        }
        run.cancelled = true;
        run.pending.clear();
        if let Some(token) = &run.current {
            token.cancel();
        }
    }

    /// Photos still to export (for the quit guard).
    pub fn pending(&self) -> usize {
        self.run.lock().map_or(0, |r| r.pending.len())
    }
}

/// The file a photo exports to in `folder`: its own name with `extension`, numbered
/// when that name is taken on disk or earlier in the run ("DSC_0012-2.jpg"). Never an
/// existing file, so an export never replaces anything.
pub fn destination_for(
    source: &Path,
    folder: &Path,
    extension: &str,
    reserved: &HashSet<PathBuf>,
) -> PathBuf {
    let stem = source
        .file_stem()
        .map_or_else(|| "Photo".into(), |s| s.to_string_lossy().into_owned());
    let taken = |p: &Path| p.exists() || reserved.contains(p);
    let first = folder.join(format!("{stem}.{extension}"));
    if !taken(&first) {
        return first;
    }
    (2..)
        .map(|n| folder.join(format!("{stem}-{n}.{extension}")))
        .find(|p| !taken(p))
        .expect("an unused name exists")
}

async fn run_queue(app: AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let queue = &state.export_queue;
    loop {
        let next = {
            let mut run = queue.run.lock().expect("export queue lock");
            match run.pending.pop_front() {
                Some(item) if !run.cancelled => {
                    let extension = item.format.extensions()[0];
                    let dest =
                        destination_for(&item.source, &item.folder, extension, &run.reserved);
                    run.reserved.insert(dest.clone());
                    Some((item, dest, run.done, run.total))
                }
                _ => None,
            }
        };
        let Some((item, dest, done, total)) = next else {
            break;
        };
        let name = file_name(&item.source);
        let id = state.next_export_id.fetch_add(1, Ordering::Relaxed);
        let progress_app = app.clone();
        let current = name.clone();
        let handle = state.engine.export_file(
            FileExport {
                source: item.source.clone(),
                recipe: item.recipe,
                destination: dest,
                format: item.format,
                long_edge: item.long_edge,
                sharpening: item.sharpening,
                colour_space: item.colour_space,
                metadata: item.metadata,
                judgements: item.judgements,
            },
            move |p| {
                let _ = progress_app.emit(
                    EXPORT_QUEUE_EVENT,
                    ExportQueueEvent::Progress {
                        done,
                        total,
                        current: current.clone(),
                        fraction: p.fraction,
                    },
                );
            },
        );
        let token = handle.token().clone();
        state
            .exports
            .lock()
            .expect("exports lock")
            .insert(id, token.clone());
        queue.run.lock().expect("export queue lock").current = Some(token);
        let result = tauri::async_runtime::spawn_blocking(move || handle.wait()).await;
        state.exports.lock().expect("exports lock").remove(&id);

        let mut run = queue.run.lock().expect("export queue lock");
        run.current = None;
        run.done += 1;
        match result {
            Ok(Ok(summary)) => {
                run.folder = summary.path.parent().map(Path::to_path_buf);
                run.outputs.push(ExportedFileDto {
                    path: summary.path.display().to_string(),
                    width: summary.width,
                    height: summary.height,
                    bytes: summary.bytes as u64,
                });
            }
            Ok(Err(JobError::Cancelled)) => run.cancelled = true,
            Ok(Err(JobError::Failed(e))) => {
                log::warn!("export of {} failed: {}", item.source.display(), e.detail);
                run.failed.push(FileFailureDto {
                    file: name,
                    message: e.message,
                });
            }
            Ok(Err(e)) => {
                log::warn!("export of {} failed: {e:?}", item.source.display());
                run.failed.push(FileFailureDto {
                    file: name,
                    message: "Something went wrong exporting it.".to_owned(),
                });
            }
            Err(e) => log::error!("export worker failed: {e}"),
        }
        let _ = app.emit(
            EXPORT_QUEUE_EVENT,
            ExportQueueEvent::Progress {
                done: run.done,
                total: run.total,
                current: String::new(),
                fraction: 0.0,
            },
        );
    }
    let mut run = queue.run.lock().expect("export queue lock");
    let event = ExportQueueEvent::Finished {
        exported: run.outputs.len() as u32,
        outputs: std::mem::take(&mut run.outputs),
        failed: std::mem::take(&mut run.failed),
        folder: run.folder.as_ref().map(|f| f.display().to_string()),
        cancelled: run.cancelled,
    };
    run.running = false;
    drop(run);
    let _ = app.emit(EXPORT_QUEUE_EVENT, event);
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_never_replace_a_file() {
        let dir = std::env::temp_dir().join(format!("export-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = Path::new("/photos/DSC_0012.NEF");
        let mut reserved = HashSet::new();
        let first = destination_for(source, &dir, "jpg", &reserved);
        assert_eq!(first, dir.join("DSC_0012.jpg"));
        // Taken earlier in the run, then on disk.
        reserved.insert(first.clone());
        assert_eq!(
            destination_for(source, &dir, "jpg", &reserved),
            dir.join("DSC_0012-2.jpg")
        );
        std::fs::write(dir.join("DSC_0012-2.jpg"), b"x").unwrap();
        assert_eq!(
            destination_for(source, &dir, "jpg", &reserved),
            dir.join("DSC_0012-3.jpg")
        );
        // A JPEG exported next to itself gets a new name too.
        std::fs::write(dir.join("IMG_1.jpg"), b"x").unwrap();
        assert_eq!(
            destination_for(&dir.join("IMG_1.jpg"), &dir, "jpg", &HashSet::new()),
            dir.join("IMG_1-2.jpg")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
