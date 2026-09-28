use std::fmt;

#[derive(Debug)]
pub enum CatalogueError {
    /// The database file failed its integrity check. The caller should move it aside
    /// and re-index (the catalogue is rebuildable).
    Corrupt(String),
    /// Written by a newer version of the app.
    TooNew {
        found: i64,
        supported: i64,
    },
    /// A path that is not valid Unicode cannot be stored (SQLite text is UTF-8).
    NonUnicodePath(std::path::PathBuf),
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
}

impl fmt::Display for CatalogueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Corrupt(m) => write!(f, "catalogue is corrupt: {m}"),
            Self::TooNew { found, supported } => {
                write!(
                    f,
                    "catalogue schema {found} is newer than supported ({supported})"
                )
            }
            Self::NonUnicodePath(p) => write!(f, "path is not valid Unicode: {}", p.display()),
            Self::Sqlite(e) => write!(f, "catalogue database error: {e}"),
            Self::Io(e) => write!(f, "catalogue I/O error: {e}"),
        }
    }
}

impl std::error::Error for CatalogueError {}

impl From<rusqlite::Error> for CatalogueError {
    fn from(e: rusqlite::Error) -> Self {
        // SQLITE_CORRUPT / SQLITE_NOTADB surface as errors on any statement.
        if let rusqlite::Error::SqliteFailure(f, msg) = &e
            && matches!(
                f.code,
                rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
            )
        {
            return Self::Corrupt(msg.clone().unwrap_or_else(|| f.to_string()));
        }
        Self::Sqlite(e)
    }
}

impl From<std::io::Error> for CatalogueError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
