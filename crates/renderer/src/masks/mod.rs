//! Masks (ADR 0040): adjusting part of the photo.
//!
//! A mask is a shape and the adjustments it makes where it covers the photo. Shapes
//! are in the frame the crop is in (the photo turned, straightened and perspective
//! corrected, before cropping), as fractions of its width and height, so cropping
//! does not move a mask over the picture. The renderer only asks a shape how much it
//! covers each pixel (0..1); how a mask was made does not matter to it.
//!
//! A mask can combine shapes (ADR 0043): further shapes are added to it, subtracted
//! from it or intersected with it, in order, and its Density scales the result.
//!
//! The adjustments are applied where the global ones are: Exposure and Warmth as
//! scene-linear gains after the white balance and exposure, Clarity in the detail
//! stage.
//!
//! Generated masks (ADR 0074: Subject, People, Sky) are the exception to frame
//! coordinates: they are made from the photo as decoded, so their coverage is in its
//! coordinates, and the frame is mapped back to it (as the crop's resampling does).
//! Their coverage is stored outside the recipe and given to the renderer with the
//! plan ([`GeneratedMasks`]); the recipe only names it.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::geometry::{CropRect, Geometry, Mapping};

pub mod brush;
pub use brush::Stroke;

/// A mask: where, and what it changes there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Mask {
    /// Tells masks apart while they are edited; not rendered.
    pub id: u32,
    pub shape: MaskShape,
    /// Further shapes combined with `shape`, in order (ADR 0043).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<MaskPart>>"))]
    pub parts: Vec<MaskPart>,
    /// Adjust outside the shapes instead of inside.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<bool>"))]
    pub invert: bool,
    /// Kept but not applied: the photographer switched it off to compare.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<bool>"))]
    pub hidden: bool,
    /// How strongly the mask applies, 0..100 (ADR 0043): its coverage is scaled by it.
    #[serde(default = "full_density", skip_serializing_if = "is_full_density")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<f32>"))]
    pub density: f32,
    #[serde(default)]
    pub adjustments: LocalAdjustments,
}

fn full_density() -> f32 {
    100.0
}

fn is_full_density(d: &f32) -> bool {
    *d == 100.0
}

/// A further shape of a mask and how it changes the mask (ADR 0043).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MaskPart {
    pub mode: Combine,
    pub shape: MaskShape,
}

/// How a shape changes the mask so far, as coverages combine: as independent layers
/// (the brush composes its strokes the same way), so overlapping soft edges blend
/// smoothly instead of meeting in a crease.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Combine {
    /// Covers where either covers: `c + p - c·p`.
    Add,
    /// Takes the shape away: `c·(1 - p)`.
    Subtract,
    /// Covers only where both cover: `c·p`.
    Intersect,
}

impl Combine {
    /// Coverage `c` so far, changed by a shape covering `p`.
    #[inline]
    pub fn apply(self, c: f32, p: f32) -> f32 {
        match self {
            Self::Add => c + p - c * p,
            Self::Subtract => c * (1.0 - p),
            Self::Intersect => c * p,
        }
    }
}

/// Where a mask covers the photo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Painted strokes (ADR 0042), in the order painted; see [`brush`].
    Brush { strokes: Vec<Stroke> },
    /// A mask made from the photo (ADR 0074): `mask` names its stored coverage (a
    /// content hash), made by the AI subsystem for `of`. Its coverage covers the photo
    /// as decoded, so it stays on what it covers whatever the geometry.
    Generated { of: GeneratedKind, mask: String },
}

/// What a generated mask covers (ADR 0074).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum GeneratedKind {
    Subject,
    People,
    Sky,
}

/// The stored coverage of a render's generated masks, by name (ADR 0074), in the
/// photo's coordinates as decoded. A name missing here covers nothing.
pub type GeneratedMasks = HashMap<String, Arc<brush::CoverageMap>>;

/// The longest a generated mask's name may be (a SHA-256 in hex).
pub const MASK_NAME_LEN: usize = 64;

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
    /// Covers nothing: a brush mask with nothing painted yet.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Brush { strokes } if strokes.iter().all(|s| s.points.is_empty() || s.erase))
    }

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
            Self::Brush { strokes } => Self::Brush {
                strokes: strokes
                    .iter()
                    .map(Stroke::sanitized)
                    .filter(|s| !s.points.is_empty())
                    .collect(),
            },
            // The name becomes a file name in the store: hexadecimal digits only.
            Self::Generated { of, mask } => Self::Generated {
                of,
                mask: mask
                    .chars()
                    .filter(char::is_ascii_hexdigit)
                    .map(|c| c.to_ascii_lowercase())
                    .take(MASK_NAME_LEN)
                    .collect(),
            },
        }
    }

    /// The stored mask it names, if it is a generated one.
    pub fn generated(&self) -> Option<&str> {
        match self {
            Self::Generated { mask, .. } => Some(mask),
            _ => None,
        }
    }
}

/// A radial gradient's radii, as fractions of the frame's diagonal.
const MIN_RADIUS: f32 = 0.002;
const MAX_RADIUS: f32 = 3.0;

impl Mask {
    /// A mask of one shape, at full density.
    pub fn new(id: u32, shape: MaskShape, adjustments: LocalAdjustments) -> Self {
        Self {
            id,
            shape,
            parts: Vec::new(),
            invert: false,
            hidden: false,
            density: 100.0,
            adjustments,
        }
    }

    pub fn sanitized(&self) -> Self {
        Self {
            id: self.id,
            shape: self.shape.clone().sanitized(),
            parts: self
                .parts
                .iter()
                .map(|p| MaskPart {
                    mode: p.mode,
                    shape: p.shape.clone().sanitized(),
                })
                .collect(),
            invert: self.invert,
            hidden: self.hidden,
            density: if self.density.is_finite() {
                self.density.clamp(0.0, 100.0)
            } else {
                100.0
            },
            adjustments: self.adjustments.sanitized(),
        }
    }

    /// Its shapes cover nothing before inverting: an empty brush, or shapes combined
    /// down to nothing (only an added shape can cover more; intersecting with an empty
    /// one leaves nothing).
    pub fn covers_nothing(&self) -> bool {
        self.parts
            .iter()
            .fold(self.shape.is_empty(), |empty, p| match p.mode {
                Combine::Add => empty && p.shape.is_empty(),
                Combine::Subtract => empty,
                Combine::Intersect => empty || p.shape.is_empty(),
            })
    }

    /// Changes nothing: hidden, at no density, without adjustments, or covering
    /// nothing (unless inverted: then everything).
    pub fn is_noop(&self) -> bool {
        self.hidden
            || self.density <= 0.0
            || self.adjustments.is_identity()
            || (!self.invert && self.covers_nothing())
    }
}

/// Where the rendered image sits in the frame masks are drawn in: the crop (fractions
/// of the frame) and the frame's size in source pixels (for the aspect ratio).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub crop: CropRect,
    pub width: f32,
    pub height: f32,
    /// How the frame came from the source, for generated masks (ADR 0074): the
    /// geometry and the source's size. `None` when the frame is the source.
    pub from_source: Option<(Geometry, f32, f32)>,
}

impl Frame {
    /// The whole of a `w` x `h` image.
    pub fn whole(w: u32, h: u32) -> Self {
        Self {
            crop: CropRect::FULL,
            width: w as f32,
            height: h as f32,
            from_source: None,
        }
    }
}

/// A mask's shape in the frame's pixels, ready to evaluate per pixel.
#[derive(Debug, Clone)]
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
    /// Coverage read from the strokes' map: `scale` takes frame pixels to map pixels.
    Brush {
        map: Arc<brush::CoverageMap>,
        scale: [f32; 2],
    },
    /// Coverage read from a generated mask in the source's coordinates (ADR 0074):
    /// frame pixels are mapped to the source (`to_source`, the source's size) and then
    /// to the map. `None` when the mask is missing: it covers nothing.
    Generated {
        map: Option<Arc<brush::CoverageMap>>,
        frame: [f32; 2],
        to_source: Option<(Mapping, [f32; 2])>,
    },
}

impl CompiledShape {
    pub fn new(shape: &MaskShape, frame: &Frame, generated: &GeneratedMasks) -> Self {
        match shape.clone().sanitized() {
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
            MaskShape::Brush { strokes } => {
                let map = brush::coverage(&strokes, frame.width, frame.height);
                let (mw, mh) = map.size();
                Self::Brush {
                    map,
                    scale: [mw as f32 / frame.width, mh as f32 / frame.height],
                }
            }
            MaskShape::Generated { mask, .. } => Self::Generated {
                map: generated.get(&mask).cloned(),
                frame: [frame.width, frame.height],
                to_source: frame
                    .from_source
                    .map(|(g, w, h)| (Mapping::new(&g, w, h), [w, h])),
            },
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
            Self::Brush { ref map, scale } => map.sample(x * scale[0], y * scale[1]),
            Self::Generated {
                ref map,
                frame,
                ref to_source,
            } => {
                let Some(map) = map else { return 0.0 };
                // The point as fractions of the source, then of the map (which covers
                // the whole source).
                let (u, v) = (x / frame[0], y / frame[1]);
                let (u, v) = match to_source {
                    None => (u, v),
                    Some((mapping, [w, h])) => {
                        let (sx, sy) = mapping.source(u, v);
                        (sx / w, sy / h)
                    }
                };
                if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                    return 0.0;
                }
                let (mw, mh) = map.size();
                map.sample(u * mw as f32, v * mh as f32)
            }
        }
    }
}

/// A mask as the renderer applies it: its shape, and its adjustments resolved to
/// stops and gains.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalMask {
    pub shape: MaskShape,
    /// Further shapes combined with it, in order.
    pub parts: Vec<MaskPart>,
    /// Adjusts outside the shapes instead.
    pub invert: bool,
    /// Coverage scale, 0..1.
    pub density: f32,
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
    masks: Vec<(CompiledMask, LocalMask)>,
    frame: Frame,
}

/// A mask's shapes, compiled: the first, then the others with how each combines.
#[derive(Debug, Clone)]
struct CompiledMask {
    first: CompiledShape,
    parts: Vec<(Combine, CompiledShape)>,
}

impl LocalField {
    /// How much mask `m` covers frame point (`fx`, `fy`): its shapes combined,
    /// inverted if it is, at its density.
    #[inline]
    fn cover(shapes: &CompiledMask, m: &LocalMask, fx: f32, fy: f32) -> f32 {
        let c = shapes
            .parts
            .iter()
            .fold(shapes.first.coverage(fx, fy), |c, (mode, shape)| {
                // Nothing left to intersect with or take away from.
                if c == 0.0 && *mode != Combine::Add {
                    c
                } else {
                    mode.apply(c, shape.coverage(fx, fy))
                }
            });
        let c = if m.invert { 1.0 - c } else { c };
        c * m.density
    }

    pub fn new(masks: &[LocalMask], frame: Frame, generated: &GeneratedMasks) -> Self {
        Self {
            masks: masks
                .iter()
                .map(|m| {
                    let shapes = CompiledMask {
                        first: CompiledShape::new(&m.shape, &frame, generated),
                        parts: m
                            .parts
                            .iter()
                            .map(|p| (p.mode, CompiledShape::new(&p.shape, &frame, generated)))
                            .collect(),
                    };
                    (shapes, m.clone())
                })
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
        let s = CompiledShape::new(
            &linear([0.5, 0.0], [0.5, 0.5]),
            &frame,
            &GeneratedMasks::new(),
        );
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
        let s = CompiledShape::new(
            &linear([0.0, 0.0], [1.0, 1.0]),
            &frame,
            &GeneratedMasks::new(),
        );
        // start → end is (400, 100) px; (-100, 400) px is perpendicular to it.
        assert!((s.coverage(-100.0, 400.0) - 1.0).abs() < 1e-6);
        assert!((s.coverage(200.0, 50.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_field_follows_the_crop() {
        let masks = [LocalMask {
            shape: linear([0.5, 0.0], [0.5, 0.5]),
            parts: Vec::new(),
            invert: false,
            density: 1.0,
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
                from_source: None,
            },
            &GeneratedMasks::new(),
        );
        assert_eq!(field.stops(10, 0, 400, 100), 0.0);
        let whole = LocalField::new(&masks, Frame::whole(400, 200), &GeneratedMasks::new());
        assert!((whole.stops(10, 0, 400, 200) - 1.0).abs() < 1e-3);
        assert!((whole.clarity(10, 0, 400, 200) - 20.0).abs() < 0.01);
        // Rendered at another size, the same place gets the same value.
        let small = LocalField::new(&masks, Frame::whole(100, 50), &GeneratedMasks::new());
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
        let s = CompiledShape::new(
            &radial([0.5, 0.5], [0.2, 0.1], 0.0, 50.0),
            &frame,
            &GeneratedMasks::new(),
        );
        assert_eq!(s.coverage(150.0, 200.0), 1.0);
        // Inside the unfeathered half of the radius: full; past the edge: none.
        assert_eq!(s.coverage(150.0 + 49.0, 200.0), 1.0);
        assert_eq!(s.coverage(150.0 + 101.0, 200.0), 0.0);
        assert_eq!(s.coverage(150.0, 200.0 + 51.0), 0.0);
        // Half way through the fade (three quarters of the radius).
        assert!((s.coverage(150.0 + 75.0, 200.0) - 0.5).abs() < 1e-5);
        assert!((s.coverage(150.0, 200.0 + 37.5) - 0.5).abs() < 1e-5);
        // Turned a quarter: the long axis is vertical.
        let turned = CompiledShape::new(
            &radial([0.5, 0.5], [0.2, 0.1], 90.0, 50.0),
            &frame,
            &GeneratedMasks::new(),
        );
        assert!(turned.coverage(150.0, 200.0 + 90.0) > 0.0);
        assert_eq!(turned.coverage(150.0 + 60.0, 200.0), 0.0);
        // No feather: a hard edge.
        let hard = CompiledShape::new(
            &radial([0.5, 0.5], [0.2, 0.2], 0.0, 0.0),
            &frame,
            &GeneratedMasks::new(),
        );
        assert_eq!(hard.coverage(150.0 + 99.0, 200.0), 1.0);
        assert_eq!(hard.coverage(150.0 + 101.0, 200.0), 0.0);
    }

    #[test]
    fn inverted_masks_adjust_outside() {
        let mask = |invert| LocalMask {
            shape: radial([0.5, 0.5], [0.1, 0.1], 0.0, 0.0),
            parts: Vec::new(),
            invert,
            density: 1.0,
            stops: 1.0,
            warmth: [0.0; 3],
            clarity: 0.0,
        };
        let inside = LocalField::new(
            &[mask(false)],
            Frame::whole(300, 400),
            &GeneratedMasks::new(),
        );
        let outside = LocalField::new(
            &[mask(true)],
            Frame::whole(300, 400),
            &GeneratedMasks::new(),
        );
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
        let m = Mask::new(
            3,
            linear([0.5, 0.1], [0.5, 0.6]),
            LocalAdjustments {
                exposure: -0.5,
                ..Default::default()
            },
        );
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(
            json,
            r#"{"id":3,"shape":{"kind":"linear","start":[0.5,0.1],"end":[0.5,0.6]},"adjustments":{"exposure":-0.5,"warmth":0.0,"clarity":0.0}}"#
        );
        assert_eq!(serde_json::from_str::<Mask>(&json).unwrap(), m);
        // Inverted or hidden masks say so; others leave the flags out.
        let flagged = Mask {
            invert: true,
            hidden: true,
            ..m.clone()
        };
        let json = serde_json::to_string(&flagged).unwrap();
        assert!(json.contains(r#""invert":true,"hidden":true"#), "{json}");
        assert_eq!(serde_json::from_str::<Mask>(&json).unwrap(), flagged);
    }

    /// A mask's coverage at frame point (`x`, `y`) of a 400 x 400 frame.
    fn cover_at(mask: &Mask, x: f32, y: f32) -> f32 {
        let m = LocalMask {
            shape: mask.shape.clone(),
            parts: mask.parts.clone(),
            invert: mask.invert,
            density: mask.density / 100.0,
            stops: 1.0,
            warmth: [0.0; 3],
            clarity: 0.0,
        };
        let field = LocalField::new(&[m], Frame::whole(400, 400), &GeneratedMasks::new());
        let (shapes, m) = &field.masks[0];
        LocalField::cover(shapes, m, x, y)
    }

    fn part(mode: Combine, shape: MaskShape) -> MaskPart {
        MaskPart { mode, shape }
    }

    #[test]
    fn shapes_combine_in_order() {
        // Two hard discs of radius 40 px (diagonal 565.7 px), 60 px apart.
        let r = 40.0 / 400.0 / std::f32::consts::SQRT_2;
        let left = radial([0.4, 0.5], [r, r], 0.0, 0.0);
        let right = radial([0.55, 0.5], [r, r], 0.0, 0.0);
        let exposure = LocalAdjustments {
            exposure: 1.0,
            ..Default::default()
        };
        let with = |mode| Mask {
            parts: vec![part(mode, right.clone())],
            ..Mask::new(1, left.clone(), exposure)
        };
        // Left only, the overlap, right only.
        let (l, both, rt) = ((140.0, 200.0), (190.0, 200.0), (240.0, 200.0));
        let at = |m: &Mask| [l, both, rt].map(|(x, y)| cover_at(m, x, y));
        assert_eq!(at(&with(Combine::Add)), [1.0, 1.0, 1.0]);
        assert_eq!(at(&with(Combine::Subtract)), [1.0, 0.0, 0.0]);
        assert_eq!(at(&with(Combine::Intersect)), [0.0, 1.0, 0.0]);
        // Inverting takes the complement of the combination; density scales it.
        let inverted = Mask {
            invert: true,
            ..with(Combine::Subtract)
        };
        assert_eq!(at(&inverted), [0.0, 1.0, 1.0]);
        let half = Mask {
            density: 50.0,
            ..with(Combine::Add)
        };
        assert_eq!(at(&half), [0.5, 0.5, 0.5]);
        assert_eq!(cover_at(&half, 5.0, 5.0), 0.0);
    }

    #[test]
    fn soft_edges_combine_as_layers() {
        let c = 0.6;
        let p = 0.5;
        assert!((Combine::Add.apply(c, p) - 0.8).abs() < 1e-6);
        assert!((Combine::Subtract.apply(c, p) - 0.3).abs() < 1e-6);
        assert!((Combine::Intersect.apply(c, p) - 0.3).abs() < 1e-6);
        // Subtracting is intersecting with the complement.
        assert_eq!(
            Combine::Subtract.apply(c, p),
            Combine::Intersect.apply(c, 1.0 - p)
        );
    }

    #[test]
    fn a_mask_knows_when_it_covers_nothing() {
        let empty = MaskShape::Brush {
            strokes: Vec::new(),
        };
        let disc = radial([0.5, 0.5], [0.1, 0.1], 0.0, 50.0);
        let exposure = LocalAdjustments {
            exposure: 1.0,
            ..Default::default()
        };
        let mask = |shape: &MaskShape, parts| Mask {
            parts,
            ..Mask::new(1, shape.clone(), exposure)
        };
        assert!(mask(&empty, vec![]).covers_nothing());
        assert!(!mask(&empty, vec![part(Combine::Add, disc.clone())]).covers_nothing());
        assert!(mask(&empty, vec![part(Combine::Subtract, disc.clone())]).covers_nothing());
        assert!(mask(&disc, vec![part(Combine::Intersect, empty.clone())]).covers_nothing());
        assert!(!mask(&disc, vec![part(Combine::Subtract, empty.clone())]).covers_nothing());
        // An inverted empty mask covers everything; no density changes nothing.
        let inverted = Mask {
            invert: true,
            ..mask(&empty, vec![])
        };
        assert!(!inverted.is_noop());
        let faded = Mask {
            density: 0.0,
            ..mask(&disc, vec![])
        };
        assert!(faded.is_noop());
    }

    #[test]
    fn older_masks_read_as_one_shape_at_full_density() {
        let json = r#"{"id":1,"shape":{"kind":"radial","centre":[0.5,0.5],"radius":[0.1,0.1],"angle":0.0,"feather":50.0},"adjustments":{"exposure":1.0}}"#;
        let m: Mask = serde_json::from_str(json).unwrap();
        assert!(m.parts.is_empty());
        assert_eq!(m.density, 100.0);
        // And written the same way while they have no parts at full density.
        let combined = Mask {
            parts: vec![part(Combine::Subtract, linear([0.5, 0.0], [0.5, 0.2]))],
            density: 60.0,
            ..m.clone()
        };
        let out = serde_json::to_string(&combined).unwrap();
        assert!(
            out.contains(r#""parts":[{"mode":"subtract","shape":{"kind":"linear""#),
            "{out}"
        );
        assert!(out.contains(r#""density":60.0"#), "{out}");
        assert_eq!(serde_json::from_str::<Mask>(&out).unwrap(), combined);
        assert!(!serde_json::to_string(&m).unwrap().contains("density"));
    }

    #[test]
    fn density_is_kept_in_range() {
        let disc = radial([0.5, 0.5], [0.1, 0.1], 0.0, 50.0);
        let d = |density| {
            Mask {
                density,
                ..Mask::new(1, disc.clone(), LocalAdjustments::default())
            }
            .sanitized()
            .density
        };
        assert_eq!((d(150.0), d(-5.0), d(f32::NAN)), (100.0, 0.0, 100.0));
    }

    /// A generated mask covering the source's left half, stored as `name`.
    fn left_half(name: &str) -> GeneratedMasks {
        let (w, h) = (40usize, 20usize);
        let data: Vec<u8> = (0..w * h)
            .map(|i| if i % w < w / 2 { 255 } else { 0 })
            .collect();
        GeneratedMasks::from([(
            name.to_owned(),
            Arc::new(brush::CoverageMap::from_u8(w, h, &data).unwrap()),
        )])
    }

    fn generated(name: &str) -> MaskShape {
        MaskShape::Generated {
            of: GeneratedKind::Subject,
            mask: name.to_owned(),
        }
    }

    #[test]
    fn a_generated_mask_covers_what_it_was_made_on() {
        let masks = left_half("ab12");
        let frame = Frame::whole(400, 200);
        let s = CompiledShape::new(&generated("ab12"), &frame, &masks);
        assert_eq!(s.coverage(50.0, 100.0), 1.0);
        assert_eq!(s.coverage(350.0, 100.0), 0.0);
        // Missing from the store: it covers nothing.
        let missing = CompiledShape::new(&generated("cd34"), &frame, &masks);
        assert_eq!(missing.coverage(50.0, 100.0), 0.0);
    }

    #[test]
    fn a_generated_mask_stays_on_the_photo_when_it_is_turned() {
        // A quarter turn clockwise: the source's left half becomes the frame's top.
        let masks = left_half("ab12");
        let g = Geometry {
            rotation: 1,
            ..Geometry::default()
        };
        let frame = Frame {
            crop: CropRect::FULL,
            width: 200.0,
            height: 400.0,
            from_source: Some((g, 400.0, 200.0)),
        };
        let s = CompiledShape::new(&generated("ab12"), &frame, &masks);
        assert!(
            s.coverage(100.0, 50.0) > 0.99,
            "{}",
            s.coverage(100.0, 50.0)
        );
        assert!(
            s.coverage(100.0, 350.0) < 0.01,
            "{}",
            s.coverage(100.0, 350.0)
        );
    }

    #[test]
    fn a_generated_masks_name_is_only_ever_hexadecimal() {
        let MaskShape::Generated { mask, .. } = generated("../../etc/PASSWD-AB12").sanitized()
        else {
            unreachable!()
        };
        assert_eq!(mask, "ecadab12", "no slashes or dots, only its hex digits");
        let long = "f".repeat(100);
        let MaskShape::Generated { mask, .. } = generated(&long).sanitized() else {
            unreachable!()
        };
        assert_eq!(mask.len(), MASK_NAME_LEN);
    }
}
