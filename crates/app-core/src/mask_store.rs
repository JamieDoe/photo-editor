//! Generated masks' coverage (ADR 0074), kept outside recipes: a recipe names a mask,
//! and its pixels are here, one greyscale PNG per mask, named by a hash of its
//! content. Like Lightroom's AI mask data, a mask can always be made again (Update
//! masks), so the store is not the only copy of anything a photographer made; but it
//! is not a cache either, as an edit renders with it.
//!
//! A mask's name starts with the fingerprint of the photo it was made from, so a
//! recipe copied to another photo (pasted, a preset, a synced edit) can't use the first
//! photo's mask: [`belongs`] tells, and a mask that doesn't belong is remade.
//!
//! The most recently used masks are also held decoded, so renders don't read the disk.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use renderer::masks::brush::CoverageMap;

/// Decoded masks kept in memory (a few photos' worth; each is about 1 to 6 MB).
const HELD: usize = 8;

pub(crate) struct MaskStore {
    /// Where masks are written; `None` keeps them in memory only (tests, benchmarks).
    dir: Option<PathBuf>,
    /// Most recently used last; in memory only, everything put.
    held: Mutex<Vec<(String, Arc<CoverageMap>)>>,
}

impl MaskStore {
    pub(crate) fn new(dir: Option<PathBuf>) -> Self {
        Self {
            dir,
            held: Mutex::new(Vec::new()),
        }
    }

    /// Keeps `coverage` (made by `generator` from the photo whose fingerprint is
    /// `photo`) and gives its name: [`NAME_LEN`] hexadecimal digits, the same for the
    /// same mask.
    pub(crate) fn put(
        &self,
        coverage: &ai::Coverage,
        generator: &str,
        photo: u64,
    ) -> std::io::Result<String> {
        let name = name_of(coverage, generator, photo);
        let map = CoverageMap::from_u8(
            coverage.width as usize,
            coverage.height as usize,
            &coverage.data,
        )
        .ok_or_else(|| std::io::Error::other("a mask's pixels don't match its size"))?;
        if let Some(dir) = &self.dir {
            let path = path_in(dir, &name);
            if !path.exists() {
                std::fs::create_dir_all(dir)?;
                platform::fs::write_atomic(&path, &encode(coverage)?)?;
            }
        }
        self.hold(name.clone(), Arc::new(map));
        Ok(name)
    }

    /// The mask named `name`, if it is here.
    pub(crate) fn get(&self, name: &str) -> Option<Arc<CoverageMap>> {
        {
            let mut held = self.held.lock().expect("mask store lock");
            if let Some(i) = held.iter().position(|(n, _)| n == name) {
                let entry = held.remove(i);
                let map = Arc::clone(&entry.1);
                held.push(entry);
                return Some(map);
            }
        }
        let dir = self.dir.as_ref()?;
        // Names come from recipes, sanitised to hexadecimal; anything else is no name.
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let map = Arc::new(decode(&std::fs::read(path_in(dir, name)).ok()?)?);
        self.hold(name.to_owned(), Arc::clone(&map));
        Some(map)
    }

    fn hold(&self, name: String, map: Arc<CoverageMap>) {
        let mut held = self.held.lock().expect("mask store lock");
        held.retain(|(n, _)| *n != name);
        held.push((name, map));
        if held.len() > HELD {
            held.remove(0);
        }
    }
}

/// The length of a mask's name: the photo's fingerprint, then the mask's hash.
pub(crate) const NAME_LEN: usize = 48;

/// Whether mask `name` was made from the photo whose fingerprint is `photo`.
pub(crate) fn belongs(name: &str, photo: u64) -> bool {
    name.len() == NAME_LEN && name.starts_with(&format!("{photo:016x}"))
}

fn path_in(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.png"))
}

/// A mask's name: the photo's fingerprint, then two independent 64-bit hashes of what
/// made it and its pixels.
fn name_of(coverage: &ai::Coverage, generator: &str, photo: u64) -> String {
    let hash = |seed: u64| {
        let mut h = DefaultHasher::new();
        seed.hash(&mut h);
        generator.hash(&mut h);
        (coverage.width, coverage.height).hash(&mut h);
        coverage.data.hash(&mut h);
        h.finish()
    };
    format!(
        "{photo:016x}{:016x}{:016x}",
        hash(0x6d61_736b),
        hash(0x7374_6f72)
    )
}

/// A mask as an 8-bit greyscale PNG.
fn encode(coverage: &ai::Coverage) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, coverage.width, coverage.height);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    writer
        .write_image_data(&coverage.data)
        .map_err(std::io::Error::other)?;
    writer.finish().map_err(std::io::Error::other)?;
    Ok(out)
}

/// A stored mask, if it is an 8-bit greyscale PNG.
fn decode(bytes: &[u8]) -> Option<CoverageMap> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .ok()?;
    let mut data = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut data).ok()?;
    if info.color_type != png::ColorType::Grayscale || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    data.truncate(info.buffer_size());
    CoverageMap::from_u8(info.width as usize, info.height as usize, &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage() -> ai::Coverage {
        ai::Coverage {
            width: 4,
            height: 2,
            data: vec![0, 64, 128, 255, 255, 128, 64, 0],
        }
    }

    #[test]
    fn a_mask_is_kept_by_its_content_and_read_back() {
        let dir = fixtures::TempDir::new("mask-store");
        let store = MaskStore::new(Some(dir.path().to_path_buf()));
        let name = store.put(&coverage(), "test/subject", 7).unwrap();
        assert_eq!(name.len(), NAME_LEN);
        assert_eq!(
            store.put(&coverage(), "test/subject", 7).unwrap(),
            name,
            "same mask, same name"
        );
        assert_ne!(
            store.put(&coverage(), "test/people", 7).unwrap(),
            name,
            "another model"
        );
        // It belongs to the photo it was made from, and no other.
        assert!(belongs(&name, 7));
        assert!(!belongs(&name, 8));
        assert!(!belongs(&name[..32], 7));
        // A fresh store (a new launch) reads it from disk.
        let fresh = MaskStore::new(Some(dir.path().to_path_buf()));
        let map = fresh.get(&name).expect("stored");
        let expected = CoverageMap::from_u8(4, 2, &coverage().data).unwrap();
        assert_eq!(*map, expected);
        // Missing, and names that aren't names.
        assert!(fresh.get("0123456789abcdef0123456789abcdef").is_none());
        assert!(fresh.get("../../secrets").is_none());
        assert!(fresh.get("").is_none());
    }

    #[test]
    fn without_a_folder_masks_live_in_memory() {
        let store = MaskStore::new(None);
        let name = store.put(&coverage(), "test/subject", 1).unwrap();
        assert!(store.get(&name).is_some());
    }
}
