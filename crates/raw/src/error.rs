use std::fmt;

/// Decoder failure. [`DecodeError::user_message`] gives photographer-facing text;
/// `Display` gives the technical detail for logs.
#[derive(Debug)]
pub enum DecodeError {
    NotFound(String),
    Io(std::io::Error),
    /// No decoder for this type, or the decoder does not support this camera/variant.
    Unsupported(String),
    /// The file is damaged or not what its extension claims.
    Corrupt(String),
    OutOfMemory,
    Cancelled,
    Internal(String),
}

impl DecodeError {
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::NotFound(_) => {
                "The photograph could not be found. It may have been moved or deleted."
            }
            Self::Io(_) => "The photograph could not be read from disk.",
            Self::Unsupported(_) => "This file type or camera format is not supported yet.",
            Self::Corrupt(_) => {
                "This photograph could not be decoded. The file may be damaged or this camera format may not yet be supported."
            }
            Self::OutOfMemory => "There is not enough memory to open this photograph.",
            Self::Cancelled => "Opening the photograph was cancelled.",
            Self::Internal(_) => "Something went wrong while opening this photograph.",
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(p) => write!(f, "file not found: {p}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Unsupported(m) => write!(f, "unsupported: {m}"),
            Self::Corrupt(m) => write!(f, "corrupt or undecodable: {m}"),
            Self::OutOfMemory => write!(f, "out of memory"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Internal(m) => write!(f, "internal decoder error: {m}"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl From<std::io::Error> for DecodeError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            Self::NotFound(e.to_string())
        } else {
            Self::Io(e)
        }
    }
}
