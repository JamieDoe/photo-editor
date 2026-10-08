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
    pub pyramid: Pyramid,
    /// The as-shot light, which white balance adjustments are relative to.
    pub as_shot_white: Option<image_core::Chromaticity>,
    /// The photo's full size (after orientation), which crops are measured against.
    pub full_size: (u32, u32),
    /// The photo at full resolution, decoded on demand for viewing at 100 % (ADR 0070)
    /// and dropped with the image.
    pub full: Mutex<Option<Arc<image_core::LinearImage>>>,
}

impl OpenedImage {
    /// The full-resolution source, if it has been decoded.
    pub fn full(&self) -> Option<Arc<image_core::LinearImage>> {
        self.full.lock().expect("full source lock").clone()
    }

    fn byte_size(&self) -> usize {
        self.pyramid.byte_size() + self.full().map_or(0, |f| f.byte_size())
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

    pub fn remove(&mut self, id: ImageId) -> Option<Arc<OpenedImage>> {
        let pos = self.images.iter().position(|i| i.id == id)?;
        Some(self.images.remove(pos))
    }

    pub fn bytes(&self) -> usize {
        self.images.iter().map(|i| i.byte_size()).sum()
    }
}
