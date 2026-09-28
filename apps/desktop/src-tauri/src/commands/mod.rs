//! Tauri command handlers: thin adapters from IPC payloads to the engine.
//!
//! Handlers never do heavy work themselves. They submit engine jobs and await the
//! result on Tauri's blocking pool, so the main (UI) thread is never blocked.

pub mod export;
pub mod images;
pub mod library;
pub mod selftest;
pub mod settings;
pub mod system;

use app_core::{EngineError, JobHandle};

use crate::ipc::IpcError;

pub type IpcResult<T> = Result<T, IpcError>;

/// Waits for a job off the async runtime's worker threads.
pub async fn wait<T: Send + 'static>(handle: JobHandle<T, EngineError>) -> IpcResult<T> {
    tauri::async_runtime::spawn_blocking(move || handle.wait())
        .await
        .map_err(|e| IpcError::internal(format!("worker join failed: {e}")))?
        .map_err(|e| EngineError::from_job(e).into())
}
