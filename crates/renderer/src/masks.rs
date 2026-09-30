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
    /// Adjust outside the shape instead of inside.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<bool>"))]
    pub invert: bool,
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
    /// A radial gradient (ADR 0041): an ellipse, fully on inside, fading to nothing at
    /// its edge. `centre` is in frame fractions; `radius` (along the ellipse's own two
    /// axes) in fractions of the frame's diagonal, which a quarter turn leaves alone;
    /// `angle` turns the first axis clockwise from horizontal, in degrees. `feather`
    /// (0..100) is the share of the radius the fade takes: 0 is a hard edge, 100 fades
    /// from the centre.
    Radial {
        centre: [f32; 2],
        radius: [f32; 2],
        angle: f32,
        feather: f32,
    },
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
            Self::Radial {
                centre,
                radius,
                angle,
                feather,
            } => {
                let finite = |v: f32, or: f32| if v.is_finite() { v } else { or };
                let angle = finite(angle, 0.0).rem_euclid(360.0);
                let angle = if angle > 180.0 { angle - 360.0 } else { angle };
                Self::Radial {
                    centre: clean(centre),
                    radius: radius.map(|r| finite(r, 0.1).clamp(MIN_RADIUS, MAX_RADIUS)),
                    angle: if angle == 0.0 { 0.0 } else { angle },
                    feather: finite(feather, 50.0).clamp(0.0, 100.0),
                }
            }
        }
    }
}

/// A radial gradient's radii, as fractions of the frame's diagonal.
const MIN_RADIUS: f32 = 0.002;
const MAX_RADIUS: f32 = 3.0;

impl Mask {
    pub fn sanitized(&self) -> Self {
        Self {
            id: self.id,
            shape: self.shape.sanitized(),
            invert: self.invert,
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
    /// Coverage `1 - smoothstep(inner, 1, d)`, where `d` is the elliptical distance
    /// from `centre` (1 on the edge): `axes` are the ellipse's axis directions divided
    /// by their radii.
    Radial {
        centre: [f32; 2],
        axes: [[f32; 2]; 2],
        inner: f32,
    },
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
            MaskShape::Radial {
                centre,
                radius,
                angle,
                feather,
            } => {
                let diagonal = frame.width.hypot(frame.height);
                let (sin, cos) = angle.to_radians().sin_cos();
                let (a, b) = (radius[0] * diagonal, radius[1] * diagonal);
                Self::Radial {
                    centre: [centre[0] * frame.width, centre[1] * frame.height],
                    axes: [[cos / a, sin / a], [-sin / b, cos / b]],
                    inner: 1.0 - feather / 100.0,
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
            Self::Radial {
                centre,
                axes,
                inner,
            } => {
                let (dx, dy) = (x - centre[0], y - centre[1]);
                let u = dx * axes[0][0] + dy * axes[0][1];
                let v = dx * axes[1][0] + dy * axes[1][1];
                let d = (u * u + v * v).sqrt();
                if inner >= 1.0 {
                    // No feather: a hard edge.
                    return if d <= 1.0 { 1.0 } else { 0.0 };
                }
                let t = ((d - inner) / (1.0 - inner)).clamp(0.0, 1.0);
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
    /// Adjusts outside the shape instead.
    pub invert: bool,
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
    /// How much mask `m` covers frame point (`fx`, `fy`), its inversion included.
    #[inline]
    fn cover(shape: &CompiledShape, m: &LocalMask, fx: f32, fy: f32) -> f32 {
        let c = shape.coverage(fx, fy);
        if m.invert { 1.0 - c } else { c }
    }

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
            let c = Self::cover(shape, m, fx, fy);
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
            .map(|(shape, m)| Self::cover(shape, m, fx, fy) * m.stops)
            .sum()
    }

    /// Clarity added at pixel (`x`, `y`).
    #[inline]
    pub fn clarity(&self, x: usize, y: usize, w: usize, h: usize) -> f32 {
        let (fx, fy) = self.frame_point(x, y, w, h);
        self.masks
            .iter()
            .filter(|(_, m)| m.clarity != 0.0)
            .map(|(shape, m)| Self::cover(shape, m, fx, fy) * m.clarity)
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
            invert: false,
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
        let MaskShape::Linear { start, end } = s else {
            unreachable!()
        };
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

    fn radial(centre: [f32; 2], radius: [f32; 2], angle: f32, feather: f32) -> MaskShape {
        MaskShape::Radial {
            centre,
            radius,
            angle,
            feather,
        }
    }

    #[test]
    fn a_radial_gradient_covers_its_ellipse_and_fades_at_the_edge() {
        // 300 x 400 frame: diagonal 500 px. Radii 100 px and 50 px.
        let frame = Frame::whole(300, 400);
        let s = CompiledShape::new(&radial([0.5, 0.5], [0.2, 0.1], 0.0, 50.0), &frame);
        assert_eq!(s.coverage(150.0, 200.0), 1.0);
        // Inside the unfeathered half of the radius: full; past the edge: none.
        assert_eq!(s.coverage(150.0 + 49.0, 200.0), 1.0);
        assert_eq!(s.coverage(150.0 + 101.0, 200.0), 0.0);
        assert_eq!(s.coverage(150.0, 200.0 + 51.0), 0.0);
        // Half way through the fade (three quarters of the radius).
        assert!((s.coverage(150.0 + 75.0, 200.0) - 0.5).abs() < 1e-5);
        assert!((s.coverage(150.0, 200.0 + 37.5) - 0.5).abs() < 1e-5);
        // Turned a quarter: the long axis is vertical.
        let turned = CompiledShape::new(&radial([0.5, 0.5], [0.2, 0.1], 90.0, 50.0), &frame);
        assert!(turned.coverage(150.0, 200.0 + 90.0) > 0.0);
        assert_eq!(turned.coverage(150.0 + 60.0, 200.0), 0.0);
        // No feather: a hard edge.
        let hard = CompiledShape::new(&radial([0.5, 0.5], [0.2, 0.2], 0.0, 0.0), &frame);
        assert_eq!(hard.coverage(150.0 + 99.0, 200.0), 1.0);
        assert_eq!(hard.coverage(150.0 + 101.0, 200.0), 0.0);
    }

    #[test]
    fn inverted_masks_adjust_outside() {
        let mask = |invert| LocalMask {
            shape: radial([0.5, 0.5], [0.1, 0.1], 0.0, 0.0),
            invert,
            stops: 1.0,
            warmth: [0.0; 3],
            clarity: 0.0,
        };
        let inside = LocalField::new(&[mask(false)], Frame::whole(300, 400));
        let outside = LocalField::new(&[mask(true)], Frame::whole(300, 400));
        assert_eq!(
            (
                inside.stops(150, 200, 300, 400),
                outside.stops(150, 200, 300, 400)
            ),
            (1.0, 0.0)
        );
        assert_eq!(
            (inside.stops(5, 5, 300, 400), outside.stops(5, 5, 300, 400)),
            (0.0, 1.0)
        );
    }

    #[test]
    fn radial_sanitising_keeps_it_drawable() {
        let MaskShape::Radial {
            radius,
            angle,
            feather,
            ..
        } = radial([0.5, 0.5], [0.0, f32::NAN], 540.0, 400.0).sanitized()
        else {
            unreachable!()
        };
        assert_eq!(radius, [MIN_RADIUS, 0.1]);
        assert_eq!((angle, feather), (180.0, 100.0));
    }

    #[test]
    fn serialises_with_its_kind() {
        let m = Mask {
            id: 3,
            shape: linear([0.5, 0.1], [0.5, 0.6]),
            invert: false,
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
