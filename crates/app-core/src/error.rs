use std::fmt;

use jobs::JobError;
use raw::DecodeError;

/// Error category, stable across IPC so the UI can react (e.g. ignore `Cancelled`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    NotFound,
    Unsupported,
    DecodeFailed,
    ImageNotOpen,
    InvalidDestination,
    ExportFailed,
    /// The request itself was not acceptable (such as a preset with no name).
    InvalidInput,
    /// Cancelled or superseded by a newer request. Not a failure from the user's view.
    Cancelled,
    Internal,
}

/// Engine error with a photographer-facing `message` and technical `detail` for logs.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineError {
    pub kind: ErrorKind,
    pub message: String,
    pub detail: String,
}

impl EngineError {
    pub fn new(kind: ErrorKind, message: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            detail: detail.into(),
        }
    }

    pub fn cancelled() -> Self {
        Self::new(
            ErrorKind::Cancelled,
            "Superseded by a newer request.",
            "cancelled",
        )
    }

    pub fn image_not_open() -> Self {
        Self::new(
            ErrorKind::ImageNotOpen,
            "This photograph is no longer open.",
            "unknown image id",
        )
    }

    /// Flattens a job outcome into an engine error.
    pub fn from_job(e: JobError<EngineError>) -> Self {
        match e {
            JobError::Cancelled => Self::cancelled(),
            JobError::Failed(e) => e,
            JobError::Panicked(msg) => Self::new(
                ErrorKind::Internal,
                "Something went wrong. Please try again.",
                msg,
            ),
        }
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {} ({})", self.kind, self.message, self.detail)
    }
}

impl std::error::Error for EngineError {}

impl From<DecodeError> for EngineError {
    fn from(e: DecodeError) -> Self {
        let kind = match e {
            DecodeError::NotFound(_) => ErrorKind::NotFound,
            DecodeError::Unsupported(_) => ErrorKind::Unsupported,
            DecodeError::Cancelled => ErrorKind::Cancelled,
            DecodeError::Io(_) | DecodeError::Corrupt(_) | DecodeError::OutOfMemory => {
                ErrorKind::DecodeFailed
            }
            DecodeError::Internal(_) => ErrorKind::Internal,
        };
        Self::new(kind, e.user_message(), e.to_string())
    }
}

impl From<export::ExportError> for EngineError {
    fn from(e: export::ExportError) -> Self {
        let kind = match e {
            export::ExportError::InvalidDestination(_) => ErrorKind::InvalidDestination,
            _ => ErrorKind::ExportFailed,
        };
        Self::new(kind, e.user_message(), e.to_string())
    }
}

impl From<renderer::RenderError> for EngineError {
    fn from(e: renderer::RenderError) -> Self {
        match e {
            renderer::RenderError::Cancelled => Self::cancelled(),
            renderer::RenderError::Backend(m) => Self::new(
                ErrorKind::Internal,
                "The photograph could not be rendered.",
                m,
            ),
        }
    }
}
