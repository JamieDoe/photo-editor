//! Smart collections (ADR 0084): names tidied as album names are, and rules checked.

use crate::error::{EngineError, ErrorKind};

/// The longest words kept in a rule.
pub const MAX_SMART_WORDS: usize = 200;

/// `name` tidied as an album name is ([`crate::clean_album_name`]); an error if nothing
/// is left.
pub fn clean_smart_collection_name(name: &str) -> Result<String, EngineError> {
    crate::albums::tidy(name).ok_or_else(|| {
        EngineError::new(
            ErrorKind::InvalidInput,
            "Give the smart collection a name.",
            "empty smart collection name",
        )
    })
}

/// `rules` with their words tidied; an error if they match every photo (that is All
/// photos, not a smart collection).
pub fn checked_rules(
    mut rules: catalogue::SmartRules,
) -> Result<catalogue::SmartRules, EngineError> {
    rules.words = rules
        .words
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_SMART_WORDS)
        .collect();
    if rules.is_empty() {
        return Err(EngineError::new(
            ErrorKind::InvalidInput,
            "Choose at least one rule.",
            "smart collection without rules",
        ));
    }
    Ok(rules)
}

/// The smart collection no longer exists (deleted elsewhere, or a stale view).
pub fn smart_collection_not_found() -> EngineError {
    EngineError::new(
        ErrorKind::NotFound,
        "That smart collection no longer exists.",
        "smart collection not found",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_tidied_and_rules_are_needed() {
        assert_eq!(
            clean_smart_collection_name("  Best   of 2026 ").unwrap(),
            "Best of 2026"
        );
        assert!(clean_smart_collection_name("  ").is_err());
        assert!(checked_rules(catalogue::SmartRules::default()).is_err());
        let words = checked_rules(catalogue::SmartRules {
            words: "  iceland   2026 ".into(),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(words.words, "iceland 2026");
        assert!(
            checked_rules(catalogue::SmartRules {
                edited: Some(true),
                ..Default::default()
            })
            .is_ok()
        );
    }
}
