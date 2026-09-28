use std::sync::atomic::{AtomicBool, Ordering};

/// Cooperative cancellation signal polled by long-running decode/render work.
///
/// Kept as a trait so the decoder and renderer do not depend on the job system.
pub trait Cancellation: Sync {
    fn is_cancelled(&self) -> bool;
}

/// A cancellation source that never fires (benchmarks, tests, one-shot tools).
#[derive(Debug, Clone, Copy, Default)]
pub struct NeverCancel;

impl Cancellation for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

impl Cancellation for AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::Relaxed)
    }
}
