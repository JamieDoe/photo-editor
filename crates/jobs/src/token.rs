use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use image_core::Cancellation;

/// Shared cancellation flag for one job.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Whether both tokens belong to the same job.
    pub fn same_as(&self, other: &CancelToken) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Cancellation for CancelToken {
    fn is_cancelled(&self) -> bool {
        CancelToken::is_cancelled(self)
    }
}
