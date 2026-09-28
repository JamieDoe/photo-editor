//! Background job system.
//!
//! Work is submitted to a [`Lane`]. Each lane has its own worker threads and priority
//! queue, so long background work (export, indexing) can never occupy the worker that
//! interactive renders need. Within a lane, jobs run in [`Priority`] order, FIFO for
//! equal priorities.
//!
//! Jobs submitted with a *supersede key* cancel any earlier job with the same key.
//! Cancellation is cooperative: queued jobs are skipped, running jobs observe their
//! [`CancelToken`] and return early.

mod token;

use std::any::Any;
use std::cmp::Ordering as CmpOrdering;
use std::collections::{BinaryHeap, HashMap};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

pub use token::CancelToken;

/// Execution lane. Lanes have independent workers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lane {
    /// Latency-sensitive work the user is waiting on (open, interactive render).
    Interactive,
    /// Throughput work that must not starve interactive work (export, thumbnails).
    Background,
}

/// Priority within a lane. Lower variants run first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    Interactive = 0,
    VisiblePreview = 1,
    VisibleThumbnail = 2,
    Indexing = 3,
    Export = 4,
    Idle = 5,
}

/// How a job should be scheduled.
#[derive(Debug, Clone)]
pub struct JobSpec {
    pub lane: Lane,
    pub priority: Priority,
    /// Submitting a job with a key cancels the previous job with the same key.
    pub supersede_key: Option<String>,
    /// Human-readable label for diagnostics.
    pub label: &'static str,
}

impl JobSpec {
    pub fn new(lane: Lane, priority: Priority, label: &'static str) -> Self {
        Self {
            lane,
            priority,
            supersede_key: None,
            label,
        }
    }

    pub fn superseding(mut self, key: impl Into<String>) -> Self {
        self.supersede_key = Some(key.into());
        self
    }
}

/// Why a job did not produce a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobError<E> {
    /// Cancelled explicitly or superseded by a newer job.
    Cancelled,
    /// The job ran and returned an error.
    Failed(E),
    /// The job panicked. This is a bug; the worker thread survives.
    Panicked(String),
}

impl<E: fmt::Display> fmt::Display for JobError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "job cancelled"),
            Self::Failed(e) => write!(f, "{e}"),
            Self::Panicked(msg) => write!(f, "job panicked: {msg}"),
        }
    }
}

/// Unique job identifier, monotonically increasing per [`JobSystem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(pub u64);

/// Handle to a submitted job's result.
pub struct JobHandle<T, E> {
    id: JobId,
    token: CancelToken,
    rx: mpsc::Receiver<Result<T, JobError<E>>>,
}

impl<T, E> JobHandle<T, E> {
    /// A handle that is already complete (e.g. a cache hit that needed no work).
    pub fn ready(id: JobId, result: Result<T, JobError<E>>) -> Self {
        let (tx, rx) = mpsc::sync_channel(1);
        // Cannot fail: the receiver is alive and the channel has capacity.
        let _ = tx.send(result);
        Self {
            id,
            token: CancelToken::new(),
            rx,
        }
    }

    pub fn id(&self) -> JobId {
        self.id
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }

    pub fn token(&self) -> &CancelToken {
        &self.token
    }

    /// Blocks until the job finishes. Do not call on a UI thread.
    pub fn wait(self) -> Result<T, JobError<E>> {
        // A dropped sender means the job system shut down before running the job.
        self.rx.recv().unwrap_or(Err(JobError::Cancelled))
    }
}

/// Worker configuration for one lane.
#[derive(Debug, Clone)]
pub struct LaneConfig {
    /// Number of worker threads pulling from this lane's queue.
    pub workers: usize,
    /// Threads for data-parallel work inside jobs on this lane. `None` uses rayon's
    /// global pool (all cores). A smaller pool bounds how much CPU the lane can take.
    pub compute_threads: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct JobSystemConfig {
    pub interactive: LaneConfig,
    pub background: LaneConfig,
}

impl Default for JobSystemConfig {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
        Self {
            interactive: LaneConfig {
                workers: 1,
                compute_threads: None,
            },
            background: LaneConfig {
                workers: 1,
                // Leave at least half the machine for interactive rendering.
                compute_threads: Some((cores / 2).max(1)),
            },
        }
    }
}

type Task = Box<dyn FnOnce(bool) + Send>;

struct QueuedJob {
    priority: Priority,
    seq: u64,
    token: CancelToken,
    task: Task,
}

impl PartialEq for QueuedJob {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.seq == other.seq
    }
}
impl Eq for QueuedJob {}
impl PartialOrd for QueuedJob {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}
impl Ord for QueuedJob {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // BinaryHeap is a max-heap: "greater" pops first. Higher priority = lower enum
        // value; earlier submission = lower seq.
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

#[derive(Default)]
struct QueueState {
    heap: BinaryHeap<QueuedJob>,
    shutdown: bool,
}

#[derive(Default)]
struct LaneQueue {
    state: Mutex<QueueState>,
    available: Condvar,
}

/// The job system. Dropping it stops workers after their current job.
pub struct JobSystem {
    interactive: Arc<LaneQueue>,
    background: Arc<LaneQueue>,
    superseded: Mutex<HashMap<String, CancelToken>>,
    next_id: AtomicU64,
    workers: Vec<JoinHandle<()>>,
}

impl JobSystem {
    pub fn new(config: JobSystemConfig) -> Self {
        let interactive = Arc::new(LaneQueue::default());
        let background = Arc::new(LaneQueue::default());
        let mut workers = Vec::new();
        for (lane, queue, cfg) in [
            (Lane::Interactive, &interactive, &config.interactive),
            (Lane::Background, &background, &config.background),
        ] {
            let pool = cfg.compute_threads.map(|n| {
                Arc::new(
                    rayon::ThreadPoolBuilder::new()
                        .num_threads(n)
                        .thread_name(move |i| format!("jobs-{lane:?}-compute-{i}"))
                        .build()
                        .expect("failed to create compute thread pool"),
                )
            });
            for i in 0..cfg.workers.max(1) {
                let queue = Arc::clone(queue);
                let pool = pool.clone();
                let handle = std::thread::Builder::new()
                    .name(format!("jobs-{lane:?}-{i}"))
                    .spawn(move || worker_loop(&queue, pool.as_deref()))
                    .expect("failed to spawn job worker");
                workers.push(handle);
            }
        }
        Self {
            interactive,
            background,
            superseded: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            workers,
        }
    }

    /// Submits a job. `work` receives the job's cancel token and should poll it.
    pub fn submit<T, E, F>(&self, spec: JobSpec, work: F) -> JobHandle<T, E>
    where
        T: Send + 'static,
        E: Send + 'static,
        F: FnOnce(&CancelToken) -> Result<T, E> + Send + 'static,
    {
        let id = self.next_id();
        let token = CancelToken::new();
        if let Some(key) = &spec.supersede_key {
            self.replace_superseded(key, Some(token.clone()));
        }

        let (tx, rx) = mpsc::sync_channel(1);
        let job_token = token.clone();
        let task: Task = Box::new(move |skip| {
            let result = if skip || job_token.is_cancelled() {
                Err(JobError::Cancelled)
            } else {
                match catch_unwind(AssertUnwindSafe(|| work(&job_token))) {
                    // Work that noticed cancellation may still return Ok/Err; report
                    // cancellation so callers never act on a superseded result.
                    _ if job_token.is_cancelled() => Err(JobError::Cancelled),
                    Ok(Ok(v)) => Ok(v),
                    Ok(Err(e)) => Err(JobError::Failed(e)),
                    Err(panic) => Err(JobError::Panicked(panic_message(panic.as_ref()))),
                }
            };
            // The receiver may have been dropped (caller lost interest); that is fine.
            let _ = tx.send(result);
        });

        let queue = self.queue(spec.lane);
        let mut state = queue.state.lock().expect("job queue poisoned");
        state.heap.push(QueuedJob {
            priority: spec.priority,
            seq: id.0,
            token: token.clone(),
            task,
        });
        drop(state);
        queue.available.notify_one();

        JobHandle { id, token, rx }
    }

    /// Cancels the current job registered under `key`, if any.
    pub fn cancel_key(&self, key: &str) {
        self.replace_superseded(key, None);
    }

    /// Allocates a job id without submitting work (for [`JobHandle::ready`]).
    pub fn next_id(&self) -> JobId {
        JobId(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Number of jobs waiting (not running) in a lane.
    pub fn queued(&self, lane: Lane) -> usize {
        self.queue(lane)
            .state
            .lock()
            .map(|s| s.heap.len())
            .unwrap_or(0)
    }

    fn replace_superseded(&self, key: &str, token: Option<CancelToken>) {
        let mut map = self.superseded.lock().expect("supersede map poisoned");
        let previous = match token {
            Some(t) => map.insert(key.to_owned(), t),
            None => map.remove(key),
        };
        if let Some(previous) = previous {
            previous.cancel();
        }
    }

    fn queue(&self, lane: Lane) -> &LaneQueue {
        match lane {
            Lane::Interactive => &self.interactive,
            Lane::Background => &self.background,
        }
    }
}

impl Default for JobSystem {
    fn default() -> Self {
        Self::new(JobSystemConfig::default())
    }
}

impl Drop for JobSystem {
    fn drop(&mut self) {
        for queue in [&self.interactive, &self.background] {
            if let Ok(mut state) = queue.state.lock() {
                state.shutdown = true;
                // Dropping queued tasks drops their senders; waiters see Cancelled.
                state.heap.clear();
            }
            queue.available.notify_all();
        }
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}

fn worker_loop(queue: &LaneQueue, pool: Option<&rayon::ThreadPool>) {
    loop {
        let job = {
            let mut state = queue.state.lock().expect("job queue poisoned");
            loop {
                if state.shutdown {
                    return;
                }
                if let Some(job) = state.heap.pop() {
                    break job;
                }
                state = queue.available.wait(state).expect("job queue poisoned");
            }
        };
        let skip = job.token.is_cancelled();
        match pool {
            Some(pool) => pool.install(|| (job.task)(skip)),
            None => (job.task)(skip),
        }
    }
}

fn panic_message(panic: &(dyn Any + Send)) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_owned()
    }
}

#[cfg(test)]
mod tests;
