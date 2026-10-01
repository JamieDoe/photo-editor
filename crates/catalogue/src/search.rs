//! Searching the library (ADR 0056): the photos whose file (name or folders), camera,
//! lens or capture date match every word of a query.
//!
//! Folder names are where places usually are ("2026 Iceland/Day 1"), so a place finds
//! its photos without location data. Dates are found by year ("2026"), month name
//! ("september", "sept") or day ("24"), each of which also matches as text, so a
//! number in a file or folder name still counts.

use crate::catalogue::{Catalogue, Result};
use crate::marks::CollectionEntry;

/// The most words of a query used.
const MAX_TERMS: usize = 8;

/// One word of a query, as matched.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Term {
    /// In the file's path, the camera or the lens.
    Text(String),
    /// A capture year, or text.
    Year(u16, String),
    /// A capture month (1–12), or text.
    Month(u8, String),
    /// A capture day of the month, or text.
    Day(u8, String),
    /// The start of an ISO capture date: "2026-09", "2026-09-24".
    DatePrefix(String),
}

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

fn terms(query: &str) -> Vec<Term> {
    query
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| matches!(c, ',' | '.' | '"' | '\''))
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .take(MAX_TERMS)
        .map(|w| {
            let digits = w.chars().all(|c| c.is_ascii_digit());
            if digits
                && w.len() == 4
                && let Ok(y) = w.parse::<u16>()
                && (1900..=2200).contains(&y)
            {
                return Term::Year(y, w);
            }
            if digits
                && w.len() <= 2
                && let Ok(d) = w.parse::<u8>()
                && (1..=31).contains(&d)
            {
                return Term::Day(d, w);
            }
            if w.len() >= 7
                && w.as_bytes()[4] == b'-'
                && w.chars().all(|c| c.is_ascii_digit() || c == '-')
            {
                return Term::DatePrefix(w);
            }
            // At least three letters: "mar" is March, "may" May, "sept" September.
            if w.len() >= 3
                && !digits
                && let Some(m) = MONTHS.iter().position(|m| m.starts_with(&w))
            {
                return Term::Month(m as u8 + 1, w);
            }
            Term::Text(w)
        })
        .collect()
}

/// The file's path from its library folder's own name down ("2026 Iceland/Day 1/
/// DSC_0001.NEF"): the folders above the library (the home folder, Pictures) would
/// match every photo. `rtrim(root, <root without separators>)` is the root's parent
/// with its trailing separator.
const BELOW_LIBRARY: &str = "(SELECT substr(f.path, length(rtrim(fo.path, \
     replace(replace(fo.path, '/', ''), '\\', ''))) + 1) FROM folders fo WHERE fo.id = f.folder_id)";

/// `w` as a LIKE pattern matching it anywhere (its own % and _ taken literally).
fn anywhere(w: &str) -> String {
    let escaped: String = w
        .chars()
        .flat_map(|c| match c {
            '%' | '_' | '\\' => vec!['\\', c],
            c => vec![c],
        })
        .collect();
    format!("%{escaped}%")
}

/// The SQL condition (over `p` and `f`) for `terms`, all of which must match, and its
/// parameters in order.
fn condition(terms: &[Term]) -> (String, Vec<String>) {
    let mut params = Vec::new();
    let text = |w: &str, params: &mut Vec<String>| {
        params.push(anywhere(w));
        let n = params.len();
        format!(
            "({BELOW_LIBRARY} LIKE ?{n} ESCAPE '\\' OR p.camera_make LIKE ?{n} ESCAPE '\\' \
             OR p.camera_model LIKE ?{n} ESCAPE '\\' OR p.lens LIKE ?{n} ESCAPE '\\')"
        )
    };
    let parts: Vec<String> = terms
        .iter()
        .map(|t| match t {
            Term::Text(w) => text(w, &mut params),
            Term::Year(y, w) => format!(
                "(substr(p.captured_at, 1, 4) = '{y:04}' OR {})",
                text(w, &mut params)
            ),
            Term::Month(m, w) => format!(
                "(substr(p.captured_at, 6, 2) = '{m:02}' OR {})",
                text(w, &mut params)
            ),
            Term::Day(d, w) => format!(
                "(substr(p.captured_at, 9, 2) = '{d:02}' OR {})",
                text(w, &mut params)
            ),
            Term::DatePrefix(w) => {
                params.push(format!("{}%", w));
                format!("p.captured_at LIKE ?{}", params.len())
            }
        })
        .collect();
    (parts.join(" AND "), params)
}

impl Catalogue {
    /// Present photos matching every word of `query`, oldest capture first; none for an
    /// empty query.
    pub fn search(&self, query: &str) -> Result<Vec<CollectionEntry>> {
        let terms = terms(query);
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let (condition, params) = condition(&terms);
        self.entries(&condition, rusqlite::params_from_iter(params))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_read_as_dates_or_text() {
        assert_eq!(
            terms("Iceland  2026, Sept 24 nikon 2026-09"),
            [
                Term::Text("iceland".into()),
                Term::Year(2026, "2026".into()),
                Term::Month(9, "sept".into()),
                Term::Day(24, "24".into()),
                Term::Text("nikon".into()),
                Term::DatePrefix("2026-09".into()),
            ]
        );
        // Too short for a month, out of range for a day or year: text.
        assert_eq!(
            terms("ma 45 1234"),
            [
                Term::Text("ma".into()),
                Term::Text("45".into()),
                Term::Text("1234".into())
            ]
        );
        assert!(terms("   ").is_empty());
        assert_eq!(terms(&"a ".repeat(20)).len(), MAX_TERMS);
    }

    #[test]
    fn like_patterns_take_wildcards_literally() {
        assert_eq!(anywhere("50%_off"), r"%50\%\_off%");
    }
}
