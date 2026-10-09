use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cache::SourceId;
use image_core::Pyramid;

use crate::ImageId;

/// A decoded image held in memory for previewing.
#[derive(Debug)]
pub(crate) struct OpenedImage {
    pub id: ImageId,
    pub path: PathBuf,
    pub source_id: SourceId,
    /// The file's content fingerprint (ADR 0012): generated masks belong to it (ADR 0074).
    pub fingerprint: u64,
    /// How bright the scene was, from the camera's exposure (`ai::scene_ev`): the sky
    /// finder tells sky from bright walls with it (ADR 0074).
    pub scene_ev: Option<f32>,
    /// The lens corrections the file records (ADR 0075), and the lens's name.
    pub lens: Option<(renderer::lens::LensCorrection, String)>,
    pub pyramid: Pyramid,
    /// The as-shot light, which white balance adjustments are relative to.
    pub as_shot_white: Option<image_core::Chromaticity>,
    /// The photo's full size (after orientation), which crops are measured against.
    pub full_size: (u32, u32),
    /// The photo at full resolution, decoded on demand for viewing at 100 % (ADR 0070)
    /// and dropped with the image.
    pub full: Mutex<Option<Arc<image_core::LinearImage>>>,
    /// Held while the full resolution decodes, so two jobs never decode it twice.
    pub full_decode: Mutex<()>,
    /// The removals' fill made at full resolution (ADR 0070), for the removals it was
    /// made for: every view of the photo shows it, scaled.
    pub fill: Mutex<Option<(Vec<renderer::remove::Removal>, Arc<renderer::remove::Fill>)>>,
}

impl OpenedImage {
    /// The full-resolution source, if it has been decoded.
    pub fn full(&self) -> Option<Arc<image_core::LinearImage>> {
        self.full.lock().expect("full source lock").clone()
    }

    /// The full-resolution fill of `removals`, if it has been made.
    pub fn fill_for(
        &self,
        removals: &[renderer::remove::Removal],
    ) -> Option<Arc<renderer::remove::Fill>> {
        let fill = self.fill.lock().expect("fill lock");
        fill.as_ref()
            .filter(|(made_for, _)| made_for.as_slice() == removals)
            .map(|(_, fill)| Arc::clone(fill))
    }

    fn byte_size(&self) -> usize {
        let fill = self.fill.lock().expect("fill lock");
        self.pyramid.byte_size()
            + self.full().map_or(0, |f| f.byte_size())
            + fill.as_ref().map_or(0, |(_, f)| f.byte_size())
    }
}

/// Bounded most-recently-used set of open images.
#[derive(Debug)]
pub(crate) struct OpenImages {
    capacity: usize,
    /// Most recently used last.
    images: Vec<Arc<OpenedImage>>,
}

impl OpenImages {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            images: Vec::new(),
        }
    }

    /// Inserts an image, returning any evicted images.
    pub fn insert(&mut self, image: Arc<OpenedImage>) -> Vec<Arc<OpenedImage>> {
        self.images.push(image);
        let excess = self.images.len().saturating_sub(self.capacity);
        self.images.drain(..excess).collect()
    }

    pub fn get(&mut self, id: ImageId) -> Option<Arc<OpenedImage>> {
        let pos = self.images.iter().position(|i| i.id == id)?;
        let image = self.images.remove(pos);
        self.images.push(Arc::clone(&image));
        Some(image)
    }

    /// The open image read from `path`, if any (without making it the most recent).
    pub fn by_path(&self, path: &std::path::Path) -> Option<Arc<OpenedImage>> {
        self.images.iter().find(|i| i.path == path).cloned()
    }

    pub fn remove(&mut self, id: ImageId) -> Option<Arc<OpenedImage>> {
        let pos = self.images.iter().position(|i| i.id == id)?;
        Some(self.images.remove(pos))
    }

    pub fn bytes(&self) -> usize {
        self.images.iter().map(|i| i.byte_size()).sum()
    }
}
