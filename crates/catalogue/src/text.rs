//! A photo's words (ADR 0089): its title and caption, and its own creator and copyright
//! where they differ from the photographer's usual ones (kept in the settings and
//! written into every export). Stored on the photo, so they follow it across moves and
//! renames; like marks and keywords, they can't be rebuilt from the files.

use rusqlite::params;

use crate::PhotoId;
use crate::catalogue::{Catalogue, Result};

/// One of a photo's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextField {
    Title,
    Caption,
    Creator,
    Copyright,
}

impl TextField {
    pub const ALL: [Self; 4] = [Self::Title, Self::Caption, Self::Creator, Self::Copyright];

    fn column(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Caption => "caption",
            Self::Creator => "creator",
            Self::Copyright => "copyright",
        }
    }

    /// The longest value kept, in characters.
    pub fn max_chars(self) -> usize {
        match self {
            Self::Caption => 2000,
            Self::Title | Self::Creator | Self::Copyright => 200,
        }
    }

    /// `text` as this field keeps it: trimmed, a caption keeping its line breaks (at
    /// most one blank line in a row), the others on one line; cut to
    /// [`max_chars`](Self::max_chars). `None` when that leaves nothing.
    pub fn tidy(self, text: &str) -> Option<String> {
        let one_line = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        let tidied = match self {
            Self::Caption => {
                let mut lines: Vec<String> = Vec::new();
                for line in text.lines().map(one_line) {
                    if !(line.is_empty() && lines.last().is_some_and(String::is_empty)) {
                        lines.push(line);
                    }
                }
                lines.join("\n").trim().to_owned()
            }
            _ => one_line(text),
        };
        let cut: String = tidied.chars().take(self.max_chars()).collect();
        let cut = cut.trim_end().to_owned();
        (!cut.is_empty()).then_some(cut)
    }
}

/// A photo's words; `None` where it has none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhotoText {
    pub title: Option<String>,
    pub caption: Option<String>,
    pub creator: Option<String>,
    pub copyright: Option<String>,
}

impl PhotoText {
    pub fn get(&self, field: TextField) -> Option<&str> {
        match field {
            TextField::Title => self.title.as_deref(),
            TextField::Caption => self.caption.as_deref(),
            TextField::Creator => self.creator.as_deref(),
            TextField::Copyright => self.copyright.as_deref(),
        }
    }
}

impl Catalogue {
    /// The words of each of `photos`, in order (a photo not in the catalogue has none).
    pub fn photo_text(&self, photos: &[PhotoId]) -> Result<Vec<PhotoText>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT title, caption, creator, copyright FROM photos WHERE id = ?1",
        )?;
        photos
            .iter()
            .map(|p| {
                let mut rows = stmt.query([p.0])?;
                Ok(match rows.next()? {
                    Some(r) => PhotoText {
                        title: r.get(0)?,
                        caption: r.get(1)?,
                        creator: r.get(2)?,
                        copyright: r.get(3)?,
                    },
                    None => PhotoText::default(),
                })
            })
            .collect()
    }

    /// Sets `field` of every photo in `photos` to `text` (tidied with
    /// [`TextField::tidy`]; nothing left clears it), in one transaction.
    pub fn set_photo_text(&self, photos: &[PhotoId], field: TextField, text: &str) -> Result<()> {
        let value = field.tidy(text);
        self.with_tx(|tx| {
            let mut stmt = tx.prepare_cached(&format!(
                "UPDATE photos SET {} = ?2 WHERE id = ?1",
                field.column()
            ))?;
            for photo in photos {
                stmt.execute(params![photo.0, value])?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_tidied_per_field() {
        assert_eq!(
            TextField::Title.tidy("  Heron   at\tdawn\n").as_deref(),
            Some("Heron at dawn")
        );
        assert_eq!(TextField::Copyright.tidy(" \n "), None);
        assert_eq!(
            TextField::Caption
                .tidy("  First  line \n\n\n\nSecond line  \n")
                .as_deref(),
            Some("First line\n\nSecond line")
        );
        let long = "x".repeat(300);
        assert_eq!(TextField::Creator.tidy(&long).map(|t| t.len()), Some(200));
        assert_eq!(
            TextField::Caption
                .tidy(&"y ".repeat(1500))
                .map(|t| t.chars().count()),
            Some(1999),
            "cut, then the trailing space trimmed"
        );
    }
}
