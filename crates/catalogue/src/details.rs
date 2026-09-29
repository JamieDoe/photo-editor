//! Photo details (metadata read from file headers) stored on photos.

use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, Row, params};

use crate::catalogue::{Catalogue, Result};
use crate::{FolderId, PhotoId};

/// Version of the metadata extraction. Photos with an older `metadata_version` are
/// (re-)read by the indexer, so improving extraction only needs a bump here.
pub const METADATA_VERSION: i64 = 1;

/// What the catalogue knows about a photograph from its file headers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PhotoDetails {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    /// Camera wall-clock time, ISO 8601 without zone ("2026-09-24T06:41:12").
    pub captured_at: Option<String>,
    pub iso: Option<u32>,
    pub aperture: Option<f32>,
    pub shutter_seconds: Option<f32>,
    pub focal_length_mm: Option<f32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub rotation: u16,
    pub gps: Option<(f64, f64)>,
}

impl PhotoDetails {
    /// Display name of the camera: "Nikon Z 6", without repeating the make when the
    /// model already includes it ("Canon EOS R5", not "Canon Canon EOS R5").
    pub fn camera(&self) -> Option<String> {
        camera_name(self.camera_make.as_deref(), self.camera_model.as_deref())
    }
}

/// See [`PhotoDetails::camera`].
pub fn camera_name(make: Option<&str>, model: Option<&str>) -> Option<String> {
    match (
        make.map(str::trim).filter(|s| !s.is_empty()),
        model.map(str::trim).filter(|s| !s.is_empty()),
    ) {
        (Some(make), Some(model)) if model.to_lowercase().starts_with(&make.to_lowercase()) => {
            Some(model.to_owned())
        }
        (Some(make), Some(model)) => Some(format!("{make} {model}")),
        (make, model) => make.or(model).map(str::to_owned),
    }
}

pub(crate) const COLUMNS: &str = "p.camera_make, p.camera_model, p.lens, p.captured_at, p.iso, p.aperture, p.shutter, \
                       p.focal_length, p.width, p.height, p.rotation, p.latitude, p.longitude";

pub(crate) fn from_row(r: &Row<'_>, first: usize) -> rusqlite::Result<PhotoDetails> {
    let lat: Option<f64> = r.get(first + 11)?;
    let lon: Option<f64> = r.get(first + 12)?;
    Ok(PhotoDetails {
        camera_make: r.get(first)?,
        camera_model: r.get(first + 1)?,
        lens: r.get(first + 2)?,
        captured_at: r.get(first + 3)?,
        iso: r.get::<_, Option<i64>>(first + 4)?.map(|v| v as u32),
        aperture: r.get::<_, Option<f64>>(first + 5)?.map(|v| v as f32),
        shutter_seconds: r.get::<_, Option<f64>>(first + 6)?.map(|v| v as f32),
        focal_length_mm: r.get::<_, Option<f64>>(first + 7)?.map(|v| v as f32),
        width: r.get::<_, Option<i64>>(first + 8)?.map(|v| v as u32),
        height: r.get::<_, Option<i64>>(first + 9)?.map(|v| v as u32),
        rotation: r.get::<_, i64>(first + 10)? as u16,
        gps: lat.zip(lon),
    })
}

impl Catalogue {
    /// Present photos in `folder` whose details are missing or out of date, with the
    /// path to read them from. At most `limit`.
    pub fn photos_needing_details(
        &self,
        folder: FolderId,
        limit: usize,
    ) -> Result<Vec<(PhotoId, PathBuf)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT p.id, f.path FROM photos p JOIN files f ON f.photo_id = p.id
             WHERE f.folder_id = ?1 AND f.missing = 0 AND p.metadata_version < ?2
             ORDER BY p.id LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![folder.0, METADATA_VERSION, limit as i64], |r| {
            Ok((PhotoId(r.get(0)?), PathBuf::from(r.get::<_, String>(1)?)))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Stores details. `None` records that the file could not be read, so it is not
    /// retried until the file changes.
    pub fn set_details(&self, items: &[(PhotoId, Option<PhotoDetails>)]) -> Result<()> {
        self.with_tx(|tx| {
            let mut stmt = tx.prepare_cached(
                "UPDATE photos SET metadata_version = ?2, camera_make = ?3, camera_model = ?4, lens = ?5,
                 captured_at = ?6, iso = ?7, aperture = ?8, shutter = ?9, focal_length = ?10, width = ?11,
                 height = ?12, rotation = ?13, latitude = ?14, longitude = ?15 WHERE id = ?1",
            )?;
            for (photo, details) in items {
                let d = details.clone().unwrap_or_default();
                stmt.execute(params![
                    photo.0,
                    METADATA_VERSION,
                    d.camera_make,
                    d.camera_model,
                    d.lens,
                    d.captured_at,
                    d.iso.map(i64::from),
                    d.aperture.map(f64::from),
                    d.shutter_seconds.map(f64::from),
                    d.focal_length_mm.map(f64::from),
                    d.width.map(i64::from),
                    d.height.map(i64::from),
                    i64::from(d.rotation),
                    d.gps.map(|g| g.0),
                    d.gps.map(|g| g.1),
                ])?;
            }
            Ok(())
        })
    }

    /// Details of one photo, if they have been read.
    pub fn details(&self, photo: PhotoId) -> Result<Option<PhotoDetails>> {
        let conn = self.conn();
        let sql =
            format!("SELECT {COLUMNS} FROM photos p WHERE p.id = ?1 AND p.metadata_version > 0");
        Ok(conn
            .query_row(&sql, [photo.0], |r| from_row(r, 0))
            .optional()?)
    }

    /// Details of every read photo directly in `dir`, by file path.
    pub fn details_in_dir(&self, dir: &Path) -> Result<Vec<(PathBuf, PhotoDetails)>> {
        let dir = crate::catalogue::text(dir)?
            .trim_end_matches(std::path::MAIN_SEPARATOR)
            .to_owned();
        let conn = self.conn();
        let sql = format!(
            "SELECT f.path, {COLUMNS} FROM files f JOIN photos p ON p.id = f.photo_id
             WHERE f.dir = ?1 AND p.metadata_version > 0"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([dir], |r| {
            Ok((PathBuf::from(r.get::<_, String>(0)?), from_row(r, 1)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::camera_name;

    #[test]
    fn camera_names_do_not_repeat_the_make() {
        assert_eq!(
            camera_name(Some("Nikon"), Some("Z 6")).as_deref(),
            Some("Nikon Z 6")
        );
        assert_eq!(
            camera_name(Some("Canon"), Some("Canon EOS R5")).as_deref(),
            Some("Canon EOS R5")
        );
        assert_eq!(
            camera_name(Some("  "), Some("X-T3")).as_deref(),
            Some("X-T3")
        );
        assert_eq!(camera_name(None, None), None);
    }
}
