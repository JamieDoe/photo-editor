//! Multi-resolution image pyramid used for previews.

use std::sync::Arc;

use rayon::prelude::*;

use crate::LinearImage;

/// A set of progressively halved copies of one source image.
///
/// `levels()[0]` is the largest level; each following level halves both dimensions.
/// Levels are reference counted so renders can hold a level without holding a lock.
#[derive(Debug, Clone)]
pub struct Pyramid {
    levels: Vec<Arc<LinearImage>>,
}

impl Pyramid {
    /// Builds levels by repeated 2x box downsampling, stopping before a level's long
    /// edge would drop below `min_long_edge`.
    pub fn build(base: LinearImage, min_long_edge: u32) -> Self {
        let mut levels = vec![Arc::new(base)];
        loop {
            let last = levels.last().expect("pyramid has a base level");
            if last.long_edge() / 2 < min_long_edge.max(1) || last.width() < 2 || last.height() < 2
            {
                break;
            }
            let next = downsample_2x(last);
            levels.push(Arc::new(next));
        }
        Self { levels }
    }

    pub fn levels(&self) -> &[Arc<LinearImage>] {
        &self.levels
    }

    pub fn base(&self) -> &Arc<LinearImage> {
        &self.levels[0]
    }

    /// Index of the smallest level whose long edge is at least `target_long_edge`,
    /// or the largest level if none is big enough.
    pub fn select_index(&self, target_long_edge: u32) -> usize {
        self.levels
            .iter()
            .rposition(|l| l.long_edge() >= target_long_edge)
            .unwrap_or(0)
    }

    pub fn byte_size(&self) -> usize {
        self.levels.iter().map(|l| l.byte_size()).sum()
    }
}

/// Halves both dimensions with a 2x2 box filter (averaging in linear light).
/// Odd trailing rows/columns are dropped.
pub fn downsample_2x(src: &LinearImage) -> LinearImage {
    let dw = (src.width() / 2).max(1);
    let dh = (src.height() / 2).max(1);
    let src_stride = src.width() as usize * 3;
    let sdata = src.data();
    let dst_stride = dw as usize * 3;
    let mut out = vec![0u16; dst_stride * dh as usize];

    out.par_chunks_mut(dst_stride)
        .enumerate()
        .for_each(|(y, drow)| {
            let sy0 = (y * 2).min(src.height() as usize - 1);
            let sy1 = (y * 2 + 1).min(src.height() as usize - 1);
            let r0 = &sdata[sy0 * src_stride..(sy0 + 1) * src_stride];
            let r1 = &sdata[sy1 * src_stride..(sy1 + 1) * src_stride];
            for x in 0..dw as usize {
                let sx0 = (x * 2).min(src.width() as usize - 1) * 3;
                let sx1 = (x * 2 + 1).min(src.width() as usize - 1) * 3;
                for c in 0..3 {
                    let sum = u32::from(r0[sx0 + c])
                        + u32::from(r0[sx1 + c])
                        + u32::from(r1[sx0 + c])
                        + u32::from(r1[sx1 + c]);
                    drow[x * 3 + c] = ((sum + 2) / 4) as u16;
                }
            }
        });

    LinearImage::new(dw, dh, out).expect("downsample produces consistent dimensions")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, v: u16) -> LinearImage {
        LinearImage::new(w, h, vec![v; (w * h * 3) as usize]).unwrap()
    }

    #[test]
    fn downsample_averages_blocks() {
        // 2x2 image, one pixel per value; averages to a single pixel.
        let data = vec![0, 0, 0, 100, 100, 100, 200, 200, 200, 300, 300, 300];
        let img = LinearImage::new(2, 2, data).unwrap();
        let d = downsample_2x(&img);
        assert_eq!((d.width(), d.height()), (1, 1));
        assert_eq!(d.data(), &[150, 150, 150]);
    }

    #[test]
    fn downsample_drops_odd_edges() {
        let d = downsample_2x(&solid(5, 3, 7));
        assert_eq!((d.width(), d.height()), (2, 1));
        assert!(d.data().iter().all(|&v| v == 7));
    }

    #[test]
    fn pyramid_levels_halve_until_minimum() {
        let p = Pyramid::build(solid(1000, 600, 1), 128);
        let dims: Vec<_> = p.levels().iter().map(|l| (l.width(), l.height())).collect();
        // 125 would be below the 128 minimum, so it is not built.
        assert_eq!(dims, vec![(1000, 600), (500, 300), (250, 150)]);
    }

    #[test]
    fn pyramid_select_prefers_smallest_sufficient_level() {
        let p = Pyramid::build(solid(1000, 600, 1), 128);
        assert_eq!(p.select_index(100), 2);
        assert_eq!(p.select_index(250), 2);
        assert_eq!(p.select_index(251), 1);
        assert_eq!(p.select_index(5000), 0);
    }

    #[test]
    fn pyramid_of_tiny_image_has_single_level() {
        let p = Pyramid::build(solid(1, 1, 1), 128);
        assert_eq!(p.levels().len(), 1);
    }
}
