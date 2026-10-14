//! Range masks (ADR 0085): coverage by a pixel's lightness (Luminance) or colour
//! (Colour), as Lightroom's range masks give.
//!
//! Both read the photo as it enters the local adjustments: the image being rendered
//! after white balance and exposure ([`Reference`]), in Oklab, so a mask's own
//! Exposure never moves it and "lightness" and "near a colour" are perceptual.
//! Lightness above white is taken as white.
//!
//! A render's range coverage is worked out once, over the whole image it renders
//! ([`coverage_map`]), and read like a brush's map; the renderer keeps the latest maps
//! while other controls change.

use std::collections::HashMap;
use std::sync::Arc;

use image_core::LinearImage;
use rayon::prelude::*;

use super::MaskShape;
use super::brush::CoverageMap;
use crate::ops::colour_grading::oklab;

/// The most colours a colour range may have.
pub const MAX_COLOURS: usize = 5;
/// A new luminance range's Feather.
pub const DEFAULT_FEATHER: f32 = 50.0;
/// A new colour range's Range.
pub const DEFAULT_RANGE: f32 = 50.0;
/// The lightness a luminance range fades over at Feather 100, beyond each end.
const MAX_FADE: f32 = 25.0;
/// How far from a picked colour (Oklab distance, lightness counted half) coverage
/// ends at Range 0 and 100.
const NEAREST: f32 = 0.02;
const FURTHEST: f32 = 0.26;
/// Coverage is full up to this share of that distance.
const CORE: f32 = 0.4;

/// Range masks' coverage over the image a render works on, by [`key`].
pub type RangeMaps = HashMap<String, Arc<CoverageMap>>;

/// A range shape's name among a render's [`RangeMaps`]: its sanitised form.
pub fn key(shape: &MaskShape) -> String {
    serde_json::to_string(&shape.clone().sanitized()).unwrap_or_default()
}

/// The image range masks read, and the gains the plan applies to it before the local
/// adjustments (white balance and exposure).
pub struct Reference<'a> {
    pub image: &'a LinearImage,
    pub gains: [f32; 3],
}

/// Oklab colour of a pixel (linear sRGB, 0..65535) after `gains`; lightness 0..1.
#[inline]
pub fn lab_of(px: [u16; 3], gains: [f32; 3]) -> [f32; 3] {
    let rgb = [0, 1, 2].map(|c| f32::from(px[c]) / 65535.0 * gains[c]);
    let [l, a, b] = oklab(rgb.map(|v| v.max(0.0)));
    [l.clamp(0.0, 1.0), a, b]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A range shape ready to test pixels.
#[derive(Debug, Clone)]
pub enum RangeTest {
    /// Lightness (0..100) between `low` and `high`, fading over `fade` beyond each.
    Luminance { low: f32, high: f32, fade: f32 },
    /// Near any of `colours`: full within `inner`, nothing beyond `outer`.
    Colour {
        colours: Vec<[f32; 3]>,
        inner: f32,
        outer: f32,
    },
}

impl RangeTest {
    /// The test for `shape`; `None` for shapes of place.
    pub fn new(shape: &MaskShape) -> Option<Self> {
        match shape.clone().sanitized() {
            MaskShape::Luminance { low, high, feather } => Some(Self::Luminance {
                low,
                high,
                fade: feather / 100.0 * MAX_FADE,
            }),
            MaskShape::Colour { colours, range } => {
                let outer = NEAREST + (FURTHEST - NEAREST) * range / 100.0;
                Some(Self::Colour {
                    colours,
                    inner: outer * CORE,
                    outer,
                })
            }
            _ => None,
        }
    }

    /// Coverage (0..1) of a pixel of Oklab colour `lab`.
    #[inline]
    pub fn coverage(&self, lab: [f32; 3]) -> f32 {
        match self {
            Self::Luminance { low, high, fade } => {
                let l = lab[0] * 100.0;
                if *fade <= 0.0 {
                    return if (*low..=*high).contains(&l) {
                        1.0
                    } else {
                        0.0
                    };
                }
                smoothstep(low - fade, *low, l) * (1.0 - smoothstep(*high, high + fade, l))
            }
            Self::Colour {
                colours,
                inner,
                outer,
            } => colours
                .iter()
                .map(|c| {
                    let d = (0.5 * (lab[0] - c[0]))
                        .hypot(lab[1] - c[1])
                        .hypot(lab[2] - c[2]);
                    1.0 - smoothstep(*inner, *outer, d)
                })
                .fold(0.0, f32::max),
        }
    }
}

/// `test`'s coverage of every pixel of the reference image.
pub fn coverage_map(reference: &Reference<'_>, test: &RangeTest) -> CoverageMap {
    let image = reference.image;
    let (w, h) = (image.width() as usize, image.height() as usize);
    let mut data = vec![0u16; w * h];
    data.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let src = image.row(y as u32);
        for (x, out) in row.iter_mut().enumerate() {
            let px = [src[x * 3], src[x * 3 + 1], src[x * 3 + 2]];
            *out = (test.coverage(lab_of(px, reference.gains)) * 65535.0).round() as u16;
        }
    });
    CoverageMap::from_u16(w, h, data).expect("a map of the image's size")
}

/// A frame point's colour as range masks see it, read without rendering (for a range
/// mask's tint and for picking a colour): from the photo, or a pyramid level of it,
/// through the frame's mapping, with the lens's vignetting lifted and the plan's gains
/// before its masks ([`crate::RenderPlan::gains_before_masks`]).
pub struct FrameSampler<'a> {
    image: &'a LinearImage,
    /// The full-size source the frame was made from, and how it was.
    full: (f32, f32),
    mapping: Option<crate::geometry::Mapping>,
    vignetting: Option<crate::lens::LensAt>,
    gains: [f32; 3],
}

impl<'a> FrameSampler<'a> {
    /// `image` is the photo (any pyramid level of it) whose full size is `full`;
    /// `from_source` how the frame came from it (`None`: the frame is the photo).
    pub fn new(
        image: &'a LinearImage,
        full: (u32, u32),
        from_source: Option<&super::SourceFrame>,
        gains: [f32; 3],
    ) -> Self {
        let full = (full.0.max(1) as f32, full.1.max(1) as f32);
        Self {
            image,
            full,
            mapping: from_source.map(super::SourceFrame::mapping),
            vignetting: from_source
                .and_then(|s| s.lens)
                .filter(crate::lens::LensCorrection::has_vignetting)
                .map(|l| l.at(full.0, full.1)),
            gains,
        }
    }

    /// The Oklab colour at frame point (`u`, `v`) (fractions of the frame).
    pub fn lab(&self, u: f32, v: f32) -> [f32; 3] {
        let (sx, sy) = match &self.mapping {
            Some(m) => m.source(u, v),
            None => (u * self.full.0, v * self.full.1),
        };
        let lift = self
            .vignetting
            .as_ref()
            .map_or(1.0, |l| l.vignetting_gain(sx, sy));
        let (w, h) = (self.image.width() as f32, self.image.height() as f32);
        let rgb = bilinear(
            self.image,
            sx / self.full.0 * w - 0.5,
            sy / self.full.1 * h - 0.5,
        );
        lab_of(rgb, self.gains.map(|g| g * lift))
    }

    /// The average colour around frame point (`u`, `v`), over `radius` of the frame's
    /// width (a picked colour, steadier than one pixel's).
    pub fn lab_around(&self, u: f32, v: f32, radius: f32) -> [f32; 3] {
        let offsets = [-1.0, -0.5, 0.0, 0.5, 1.0];
        let mut sum = [0.0f32; 3];
        for dy in offsets {
            for dx in offsets {
                let lab = self.lab(u + dx * radius, v + dy * radius);
                (0..3).for_each(|c| sum[c] += lab[c]);
            }
        }
        sum.map(|s| s / 25.0)
    }
}

/// `image` at pixel point (`x`, `y`) (pixel centres at whole numbers), bilinear,
/// clamped at the edges.
fn bilinear(image: &LinearImage, x: f32, y: f32) -> [u16; 3] {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (x - x0 as f32, y - y0 as f32);
    let d = image.data();
    let at = |xx: usize, yy: usize, c: usize| f32::from(d[(yy * w + xx) * 3 + c]);
    [0, 1, 2].map(|c| {
        let top = at(x0, y0, c) + (at(x1, y0, c) - at(x0, y0, c)) * tx;
        let bottom = at(x0, y1, c) + (at(x1, y1, c) - at(x0, y1, c)) * tx;
        (top + (bottom - top) * ty).round() as u16
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grey(v: f32) -> [f32; 3] {
        lab_of([(v * 65535.0) as u16; 3], [1.0; 3])
    }

    #[test]
    fn a_luminance_range_covers_its_band_and_fades_beyond() {
        let shadows = RangeTest::new(&MaskShape::Luminance {
            low: 0.0,
            high: 40.0,
            feather: 40.0,
        })
        .unwrap();
        // Oklab lightness of linear grey: 0.01 is 21.5, 0.18 is 56.6, 0.9 is 96.5.
        assert_eq!(shadows.coverage(grey(0.01)), 1.0);
        assert_eq!(shadows.coverage(grey(0.9)), 0.0);
        let mid = shadows.coverage(grey(0.08));
        assert!(mid > 0.0 && mid < 1.0, "{mid}");
        // No feather: a hard edge.
        let hard = RangeTest::new(&MaskShape::Luminance {
            low: 50.0,
            high: 100.0,
            feather: 0.0,
        })
        .unwrap();
        assert_eq!(hard.coverage(grey(0.18)), 1.0);
        assert_eq!(hard.coverage(grey(0.08)), 0.0);
        // Brighter than white is white.
        assert_eq!(hard.coverage(lab_of([65535; 3], [4.0; 3])), 1.0);
    }

    #[test]
    fn a_colour_range_covers_near_colours_and_widens_with_range() {
        let sky = lab_of([9000, 20000, 45000], [1.0; 3]);
        let paler_sky = lab_of([12000, 23000, 45000], [1.0; 3]);
        let grass = lab_of([8000, 25000, 4000], [1.0; 3]);
        let test = |range: f32| {
            RangeTest::new(&MaskShape::Colour {
                colours: vec![sky],
                range,
            })
            .unwrap()
        };
        assert_eq!(test(50.0).coverage(sky), 1.0);
        assert_eq!(test(50.0).coverage(grass), 0.0);
        assert!(test(0.0).coverage(paler_sky) < test(100.0).coverage(paler_sky));
        // The nearest of several colours counts.
        let both = RangeTest::new(&MaskShape::Colour {
            colours: vec![sky, grass],
            range: 50.0,
        })
        .unwrap();
        assert_eq!(both.coverage(grass), 1.0);
        // Gains move a pixel's lightness: the same pixel twice as bright is lighter.
        assert!(lab_of([9000, 20000, 45000], [1.4; 3])[0] > sky[0]);
    }

    #[test]
    fn shapes_are_sanitised_and_named_by_their_settings() {
        let s = MaskShape::Luminance {
            low: 80.0,
            high: f32::NAN,
            feather: 300.0,
        }
        .sanitized();
        assert_eq!(
            s,
            MaskShape::Luminance {
                low: 80.0,
                high: 100.0,
                feather: 100.0
            }
        );
        let swapped = MaskShape::Luminance {
            low: 70.0,
            high: 20.0,
            feather: 10.0,
        }
        .sanitized();
        assert!(matches!(
            swapped,
            MaskShape::Luminance {
                low: 20.0,
                high: 70.0,
                ..
            }
        ));
        let many = MaskShape::Colour {
            colours: vec![[0.5, 0.0, 0.0]; 9],
            range: 50.0,
        }
        .sanitized();
        assert!(matches!(&many, MaskShape::Colour { colours, .. } if colours.len() == MAX_COLOURS));
        assert!(
            MaskShape::Colour {
                colours: vec![],
                range: 50.0
            }
            .is_empty()
        );
        assert_ne!(key(&swapped), key(&s));
        assert_eq!(key(&swapped), key(&swapped.clone().sanitized()));
    }

    #[test]
    fn the_sampler_reads_what_a_render_would() {
        // Left half dark, right half bright, at a quarter of the full size.
        let data: Vec<u16> = (0..8 * 4)
            .flat_map(|i| [if i % 8 < 4 { 600 } else { 50000 }; 3])
            .collect();
        let level = LinearImage::new(8, 4, data).unwrap();
        let sampler = FrameSampler::new(&level, (32, 16), None, [1.0; 3]);
        assert_eq!(sampler.lab(0.1, 0.5), lab_of([600; 3], [1.0; 3]));
        assert_eq!(sampler.lab(0.9, 0.5), lab_of([50000; 3], [1.0; 3]));
        // Gains as the plan's.
        let brighter = FrameSampler::new(&level, (32, 16), None, [2.0; 3]);
        assert!(brighter.lab(0.1, 0.5)[0] > sampler.lab(0.1, 0.5)[0]);
        // Turned a half turn, the frame's left is the photo's right.
        let turned = crate::masks::SourceFrame {
            geometry: crate::geometry::Geometry {
                rotation: 2,
                ..Default::default()
            },
            width: 32.0,
            height: 16.0,
            lens: None,
        };
        let half = FrameSampler::new(&level, (32, 16), Some(&turned), [1.0; 3]);
        assert_eq!(half.lab(0.1, 0.5), lab_of([50000; 3], [1.0; 3]));
        // Averaged around a point inside one half, the same.
        let around = sampler.lab_around(0.85, 0.5, 0.05);
        assert!((around[0] - sampler.lab(0.85, 0.5)[0]).abs() < 1e-5);
    }

    #[test]
    fn the_map_covers_the_reference_pixel_by_pixel() {
        // Left half dark, right half bright.
        let data: Vec<u16> = (0..8 * 2)
            .flat_map(|i| [if i % 8 < 4 { 600 } else { 50000 }; 3])
            .collect();
        let image = LinearImage::new(8, 2, data).unwrap();
        let highlights = RangeTest::new(&MaskShape::Luminance {
            low: 70.0,
            high: 100.0,
            feather: 0.0,
        })
        .unwrap();
        let map = coverage_map(
            &Reference {
                image: &image,
                gains: [1.0; 3],
            },
            &highlights,
        );
        assert_eq!(map.size(), (8, 2));
        assert_eq!(map.sample(1.5, 0.5), 0.0);
        assert_eq!(map.sample(6.5, 1.5), 1.0);
        // Exposure lifting the dark half into the range covers it too.
        let lifted = coverage_map(
            &Reference {
                image: &image,
                gains: [60.0; 3],
            },
            &highlights,
        );
        assert_eq!(lifted.sample(1.5, 0.5), 1.0);
    }
}
