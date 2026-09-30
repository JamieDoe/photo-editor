//! Masks (ADR 0040): adjusting part of the photo.
//!
//! A mask is a shape and the adjustments it makes where it covers the photo. Shapes
//! are in the frame the crop is in (the photo turned, straightened and perspective
//! corrected, before cropping), as fractions of its width and height, so cropping
//! does not move a mask over the picture. The renderer only asks a shape how much it
//! covers each pixel (0..1); how a mask was made does not matter to it.
//!
//! The adjustments are applied where the global ones are: Exposure and Warmth as
//! scene-linear gains after the white balance and exposure, Clarity in the detail
//! stage.

use serde::{Deserialize, Serialize};

use crate::geometry::CropRect;

/// A mask: where, and what it changes there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Mask {
    /// Tells masks apart while they are edited; not rendered.
    pub id: u32,
    pub shape: MaskShape,
    #[serde(default)]
    pub adjustments: LocalAdjustments,
}

/// Where a mask covers the photo.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum MaskShape {
    /// A linear gradient: fully on the `start` side of the line through `start`,
    /// fading to nothing at the line through `end` (both lines perpendicular to
    /// `start` → `end`). Points are fractions of the frame's width and height; they
    /// may lie outside it.
    Linear { start: [f32; 2], end: [f32; 2] },
}

/// What a mask changes where it covers the photo, as the global controls do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LocalAdjustments {
    /// Stops, -2..2 (as in the design).
    pub exposure: f32,
    /// -100 (cooler) .. 100 (warmer), on the Temperature scale.
    pub warmth: f32,
    /// -100..100, as the global Clarity.
    pub clarity: f32,
}

impl LocalAdjustments {
    pub fn is_identity(&self) -> bool {
        self.exposure == 0.0 && self.warmth == 0.0 && self.clarity == 0.0
    }

    pub fn sanitized(self) -> Self {
        use crate::adjustments::{MASK_CLARITY, MASK_EXPOSURE, MASK_WARMTH};
        Self {
            exposure: MASK_EXPOSURE.clamp(self.exposure),
            warmth: MASK_WARMTH.clamp(self.warmth),
            clarity: MASK_CLARITY.clamp(self.clarity),
        }
    }
}

/// How far outside the frame a shape's points may be, in frame fractions.
const POINT_RANGE: f32 = 2.0;

impl MaskShape {
    pub fn sanitized(self) -> Self {
        let clean = |p: [f32; 2]| {
            p.map(|v| {
                let v = if v.is_finite() {
                    v.clamp(-POINT_RANGE, 1.0 + POINT_RANGE)
                } else {
                    0.5
                };
                if v == 0.0 { 0.0 } else { v }
            })
        };
        match self {
            Self::Linear { start, end } => {
                let (start, mut end) = (clean(start), clean(end));
                // A gradient needs some length to fade over.
                if (end[0] - start[0]).abs() + (end[1] - start[1]).abs() < 1e-4 {
                    end[1] = start[1] + 0.01;
                }
                Self::Linear { start, end }
            }
        }
    }
}

impl Mask {
    pub fn sanitized(&self) -> Self {
        Self {
            id: self.id,
            shape: self.shape.sanitized(),
            adjustments: self.adjustments.sanitized(),
        }
    }
}

/// Where the rendered image sits in the frame masks are drawn in: the crop (fractions
/// of the frame) and the frame's size in source pixels (for the aspect ratio).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub crop: CropRect,
    pub width: f32,
    pub height: f32,
}

impl Frame {
    /// The whole of a `w` x `h` image.
    pub fn whole(w: u32, h: u32) -> Self {
        Self {
            crop: CropRect::FULL,
            width: w as f32,
            height: h as f32,
        }
    }
}

/// A mask's shape in the frame's pixels, ready to evaluate per pixel.
#[derive(Debug, Clone, Copy)]
pub enum CompiledShape {
    /// Coverage `1 - smoothstep(t)` with `t = (p - start) · along`, where `along` is
    /// start → end divided by its squared length.
    Linear { start: [f32; 2], along: [f32; 2] },
}

impl CompiledShape {
    pub fn new(shape: &MaskShape, frame: &Frame) -> Self {
        match shape.sanitized() {
            MaskShape::Linear { start, end } => {
                let s = [start[0] * frame.width, start[1] * frame.height];
                let e = [end[0] * frame.width, end[1] * frame.height];
                let d = [e[0] - s[0], e[1] - s[1]];
                let len2 = (d[0] * d[0] + d[1] * d[1]).max(1e-6);
                Self::Linear {
                    start: s,
                    along: [d[0] / len2, d[1] / len2],
                }
            }
        }
    }

    /// Coverage (0..1) at point (`x`, `y`) of the frame, in its pixels.
    #[inline]
    pub fn coverage(&self, x: f32, y: f32) -> f32 {
        match *self {
            Self::Linear { start, along } => {
                let t = ((x - start[0]) * along[0] + (y - start[1]) * along[1]).clamp(0.0, 1.0);
                1.0 - t * t * (3.0 - 2.0 * t)
            }
        }
    }
}

/// A mask as the renderer applies it: its shape, and its adjustments resolved to
/// stops and gains.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalMask {
    pub shape: MaskShape,
    /// Exposure, in stops.
    pub stops: f32,
    /// Warmth as log2 channel gains.
    pub warmth: [f32; 3],
    pub clarity: f32,
}

impl LocalMask {
    /// Whether it changes the gains (Exposure or Warmth).
    pub fn has_gains(&self) -> bool {
        self.stops != 0.0 || self.warmth != [0.0; 3]
    }
}

/// The masks of a render, compiled for its frame: coverage and the resulting
/// adjustments at any pixel of the rendered image.
#[derive(Debug, Clone)]
pub struct LocalField {
    masks: Vec<(CompiledShape, LocalMask)>,
    frame: Frame,
}

impl LocalField {
    pub fn new(masks: &[LocalMask], frame: Frame) -> Self {
        Self {
            masks: masks
                .iter()
                .map(|m| (CompiledShape::new(&m.shape, &frame), m.clone()))
                .collect(),
            frame,
        }
    }

    pub fn has_gains(&self) -> bool {
        self.masks.iter().any(|(_, m)| m.has_gains())
    }

    pub fn has_exposure(&self) -> bool {
        self.masks.iter().any(|(_, m)| m.stops != 0.0)
    }

    pub fn has_clarity(&self) -> bool {
        self.masks.iter().any(|(_, m)| m.clarity != 0.0)
    }

    /// The frame point (in its pixels) at the centre of pixel (`x`, `y`) of the
    /// rendered `w` x `h` image.
    #[inline]
    pub fn frame_point(&self, x: usize, y: usize, w: usize, h: usize) -> (f32, f32) {
        let c = &self.frame.crop;
        (
            (c.x + (x as f32 + 0.5) / w as f32 * c.w) * self.frame.width,
            (c.y + (y as f32 + 0.5) / h as f32 * c.h) * self.frame.height,
        )
    }

    /// Log2 channel gains (Exposure and Warmth) at pixel (`x`, `y`).
    #[inline]
    pub fn log2_gains(&self, x: usize, y: usize, w: usize, h: usize) -> [f32; 3] {
        let (fx, fy) = self.frame_point(x, y, w, h);
        let mut g = [0.0f32; 3];
        for (shape, m) in &self.masks {
            if !m.has_gains() {
                continue;
            }
            let c = shape.coverage(fx, fy);
            for (gc, wc) in g.iter_mut().zip(m.warmth) {
                *gc += c * (m.stops + wc);
            }
        }
        g
    }

    /// Exposure in stops at pixel (`x`, `y`) (for the tone stage's surroundings).
    #[inline]
    pub fn stops(&self, x: usize, y: usize, w: usize, h: usize) -> f32 {
        let (fx, fy) = self.frame_point(x, y, w, h);
        self.masks
            .iter()
            .filter(|(_, m)| m.stops != 0.0)
            .map(|(shape, m)| shape.coverage(fx, fy) * m.stops)
            .sum()
    }

    /// Clarity added at pixel (`x`, `y`).
    #[inline]
    pub fn clarity(&self, x: usize, y: usize, w: usize, h: usize) -> f32 {
        let (fx, fy) = self.frame_point(x, y, w, h);
        self.masks
            .iter()
            .filter(|(_, m)| m.clarity != 0.0)
            .map(|(shape, m)| shape.coverage(fx, fy) * m.clarity)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear(start: [f32; 2], end: [f32; 2]) -> MaskShape {
        MaskShape::Linear { start, end }
    }

    #[test]
    fn a_linear_gradient_fades_from_start_to_end() {
        let frame = Frame::whole(400, 200);
        // Full at the top, nothing from the middle down.
        let s = CompiledShape::new(&linear([0.5, 0.0], [0.5, 0.5]), &frame);
        assert_eq!(s.coverage(10.0, 0.0), 1.0);
        assert_eq!(s.coverage(390.0, 150.0), 0.0);
        assert!((s.coverage(200.0, 50.0) - 0.5).abs() < 1e-6);
        // Above the start line it stays full; along a line parallel to it, constant.
        assert_eq!(s.coverage(200.0, -30.0), 1.0);
        assert_eq!(s.coverage(0.0, 25.0), s.coverage(399.0, 25.0));
    }

    #[test]
    fn gradients_are_measured_in_pixels_not_fractions() {
        // A diagonal in fractions of a wide frame: the lines are perpendicular in
        // pixels, so a point on the start line's pixel perpendicular is fully on.
        let frame = Frame::whole(400, 100);
        let s = CompiledShape::new(&linear([0.0, 0.0], [1.0, 1.0]), &frame);
        // start → end is (400, 100) px; (-100, 400) px is perpendicular to it.
        assert!((s.coverage(-100.0, 400.0) - 1.0).abs() < 1e-6);
        assert!((s.coverage(200.0, 50.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_field_follows_the_crop() {
        let masks = [LocalMask {
            shape: linear([0.5, 0.0], [0.5, 0.5]),
            stops: 1.0,
            warmth: [0.0; 3],
            clarity: 20.0,
        }];
        // The bottom half of the frame: the gradient has ended there.
        let crop = CropRect {
            x: 0.0,
            y: 0.5,
            w: 1.0,
            h: 0.5,
        };
        let field = LocalField::new(
            &masks,
            Frame {
                crop,
                width: 400.0,
                height: 200.0,
            },
        );
        assert_eq!(field.stops(10, 0, 400, 100), 0.0);
        let whole = LocalField::new(&masks, Frame::whole(400, 200));
        assert!((whole.stops(10, 0, 400, 200) - 1.0).abs() < 1e-3);
        assert!((whole.clarity(10, 0, 400, 200) - 20.0).abs() < 0.01);
        // Rendered at another size, the same place gets the same value.
        let small = LocalField::new(&masks, Frame::whole(100, 50));
        // Pixel centres: 41.5 / 200 and 10.5 / 50 of the height, 0.0025 apart (half a
        // large pixel; the gradient changes about 0.01 stops per large pixel there).
        assert!((whole.stops(202, 41, 400, 200) - small.stops(50, 10, 100, 50)).abs() < 0.01);
    }

    #[test]
    fn sanitising_keeps_shapes_usable() {
        let s = linear([f32::NAN, 9.0], [f32::NAN, 9.0]).sanitized();
        let MaskShape::Linear { start, end } = s;
        assert_eq!(start, [0.5, 3.0]);
        assert!(end[1] > start[1]);
        let adj = LocalAdjustments {
            exposure: 7.0,
            warmth: f32::NAN,
            clarity: -300.0,
        }
        .sanitized();
        assert_eq!((adj.exposure, adj.warmth, adj.clarity), (2.0, 0.0, -100.0));
    }

    #[test]
    fn serialises_with_its_kind() {
        let m = Mask {
            id: 3,
            shape: linear([0.5, 0.1], [0.5, 0.6]),
            adjustments: LocalAdjustments {
                exposure: -0.5,
                ..Default::default()
            },
        };
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(
            json,
            r#"{"id":3,"shape":{"kind":"linear","start":[0.5,0.1],"end":[0.5,0.6]},"adjustments":{"exposure":-0.5,"warmth":0.0,"clarity":0.0}}"#
        );
        assert_eq!(serde_json::from_str::<Mask>(&json).unwrap(), m);
    }
}
