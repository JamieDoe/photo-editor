//! Albums (ADR 0055): names as the photographer types them, tidied.

use crate::error::{EngineError, ErrorKind};

/// The longest album name kept.
pub const MAX_ALBUM_NAME: usize = 60;

/// `name` with runs of spaces closed up and trimmed, at most [`MAX_ALBUM_NAME`]
/// characters; an error if nothing is left.
pub fn clean_album_name(name: &str) -> Result<String, EngineError> {
    let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(EngineError::new(
            ErrorKind::InvalidInput,
            "Give the album a name.",
            "empty album name",
        ));
    }
    Ok(name.chars().take(MAX_ALBUM_NAME).collect())
}

/// The album no longer exists (deleted elsewhere, or a stale view).
pub fn album_not_found() -> EngineError {
    EngineError::new(
        ErrorKind::NotFound,
        "That album no longer exists.",
        "album not found",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_tidied() {
        assert_eq!(clean_album_name("  To   print ").unwrap(), "To print");
        assert!(clean_album_name(" \t ").is_err());
        assert_eq!(
            clean_album_name(&"x".repeat(100)).unwrap().len(),
            MAX_ALBUM_NAME
        );
    }
}
