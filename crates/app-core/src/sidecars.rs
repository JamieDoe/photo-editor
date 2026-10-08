//! XMP sidecars beside RAW files (ADR 0067, part 2): when the setting is on, a RAW's
//! rating and label are kept in `NAME.xmp` beside it, where Lightroom, Bridge and
//! Capture One read them. The RAW itself is never written to; other files (JPEGs) carry
//! their marks inside and get none.

use std::path::{Path, PathBuf};

use export::metadata::Judgements;
use export::sidecar::{SidecarChange, update};

/// Sidecars larger than this are not read (none written by a photo app comes close).
const MAX_SIDECAR_BYTES: u64 = 4 * 1024 * 1024;

/// What happened to one sidecar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Synced {
    Written,
    Deleted,
    Unchanged,
}

/// Where the sidecar of `raw` is: its name with `.xmp` in place of its extension.
pub fn path_for(raw: &Path) -> PathBuf {
    raw.with_extension("xmp")
}

/// Whether `path` is a RAW file, by its extension, among `raw_extensions` (lower case).
pub fn is_raw(path: &Path, raw_extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| raw_extensions.contains(&e.to_ascii_lowercase().as_str()))
}

/// The marks in `raw`'s sidecar (ADR 0067, part 3), if it has one with any: for a
/// photo joining the library with another app's culling beside it.
pub fn read(raw: &Path) -> Option<catalogue::Marks> {
    let sidecar = path_for(raw);
    let meta = std::fs::metadata(&sidecar).ok()?;
    if !meta.is_file() || meta.len() > MAX_SIDECAR_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(&sidecar).ok()?;
    let j = export::sidecar::read_marks(&text)?;
    use export::metadata::LabelName;
    Some(catalogue::Marks {
        rating: catalogue::Rating::new(j.rating).unwrap_or_default(),
        flag: if j.rejected {
            catalogue::Flag::Reject
        } else {
            catalogue::Flag::None
        },
        label: match j.label {
            None => catalogue::ColourLabel::None,
            Some(LabelName::Red) => catalogue::ColourLabel::Red,
            Some(LabelName::Yellow) => catalogue::ColourLabel::Yellow,
            Some(LabelName::Green) => catalogue::ColourLabel::Green,
            Some(LabelName::Blue) => catalogue::ColourLabel::Blue,
            Some(LabelName::Purple) => catalogue::ColourLabel::Purple,
        },
    })
}

/// Brings `raw`'s sidecar in line with `judgements`.
pub fn sync(raw: &Path, judgements: &Judgements) -> Result<Synced, String> {
    let sidecar = path_for(raw);
    let existing = match std::fs::metadata(&sidecar) {
        Ok(meta) if meta.len() > MAX_SIDECAR_BYTES => {
            return Err(format!("{} is too large to update", sidecar.display()));
        }
        Ok(_) => Some(std::fs::read_to_string(&sidecar).map_err(|e| e.to_string())?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    match update(existing.as_deref(), judgements).map_err(|e| e.to_string())? {
        SidecarChange::Write(text) => {
            export::write_atomic(&sidecar, text.as_bytes()).map_err(|e| e.to_string())?;
            Ok(Synced::Written)
        }
        SidecarChange::Delete => {
            std::fs::remove_file(&sidecar).map_err(|e| e.to_string())?;
            Ok(Synced::Deleted)
        }
        SidecarChange::Leave => Ok(Synced::Unchanged),
    }
}

/// Syncs each of `files` (the RAWs among them) with its marks in `catalogue`, logging
/// failures: a sidecar that can't be written never fails the change that caused it.
pub fn sync_files(catalogue: &catalogue::Catalogue, files: &[PathBuf], raw_extensions: &[&str]) {
    for file in files.iter().filter(|f| is_raw(f, raw_extensions)) {
        let marks = catalogue
            .photo_at(file)
            .and_then(|photo| photo.map(|p| catalogue.marks(p)).transpose());
        let judgements = match marks {
            Ok(m) => m.map(|m| crate::judgements(&m)).unwrap_or_default(),
            Err(e) => {
                log::warn!("sidecar: no marks for {}: {e}", file.display());
                continue;
            }
        };
        if let Err(e) = sync(file, &judgements) {
            log::warn!("sidecar for {} not updated: {e}", file.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use export::metadata::LabelName;

    #[test]
    fn sidecars_follow_the_marks_and_never_touch_the_raw() {
        let dir = fixtures::TempDir::new("sidecars");
        let raw = dir.path().join("DSC_0012.NEF");
        std::fs::write(&raw, b"raw bytes").unwrap();
        let rated = Judgements {
            rating: 4,
            rejected: false,
            label: Some(LabelName::Green),
        };
        assert_eq!(sync(&raw, &rated).unwrap(), Synced::Written);
        let side = dir.path().join("DSC_0012.xmp");
        let text = std::fs::read_to_string(&side).unwrap();
        assert!(text.contains(r#"xmp:Rating="4""#) && text.contains(r#"xmp:Label="Green""#));
        assert_eq!(sync(&raw, &rated).unwrap(), Synced::Unchanged);
        assert_eq!(sync(&raw, &Judgements::default()).unwrap(), Synced::Deleted);
        assert!(!side.exists());
        assert_eq!(std::fs::read(&raw).unwrap(), b"raw bytes");
    }

    #[test]
    fn another_apps_sidecar_is_updated_not_replaced() {
        let dir = fixtures::TempDir::new("sidecars-other");
        let raw = dir.path().join("IMG_1.CR3");
        std::fs::write(&raw, b"raw").unwrap();
        let theirs = "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\" xmlns:crs=\"http://ns.adobe.com/camera-raw-settings/1.0/\" xmp:Rating=\"1\" crs:Exposure2012=\"+1.00\"/></rdf:RDF></x:xmpmeta>";
        std::fs::write(path_for(&raw), theirs).unwrap();
        let five = Judgements {
            rating: 5,
            ..Default::default()
        };
        assert_eq!(sync(&raw, &five).unwrap(), Synced::Written);
        let text = std::fs::read_to_string(path_for(&raw)).unwrap();
        assert!(text.contains(r#"xmp:Rating="5""#) && text.contains(r#"crs:Exposure2012="+1.00""#));
        // Cleared: the marks go, their file stays.
        assert_eq!(sync(&raw, &Judgements::default()).unwrap(), Synced::Written);
        assert!(path_for(&raw).exists());
    }

    #[test]
    fn an_unreadable_sidecar_is_left_alone() {
        let dir = fixtures::TempDir::new("sidecars-odd");
        let raw = dir.path().join("a.arw");
        std::fs::write(&raw, b"raw").unwrap();
        std::fs::write(path_for(&raw), "something else entirely").unwrap();
        let rated = Judgements {
            rating: 2,
            ..Default::default()
        };
        assert!(sync(&raw, &rated).is_err());
        assert_eq!(
            std::fs::read_to_string(path_for(&raw)).unwrap(),
            "something else entirely"
        );
    }

    #[test]
    fn only_raws_get_sidecars() {
        let raws = ["nef", "cr3", "arw"];
        assert!(is_raw(Path::new("/p/DSC_1.NEF"), &raws));
        assert!(!is_raw(Path::new("/p/DSC_1.JPG"), &raws));
        assert!(!is_raw(Path::new("/p/notes"), &raws));
        assert_eq!(
            path_for(Path::new("/p/DSC_1.NEF")),
            Path::new("/p/DSC_1.xmp")
        );
    }
}
