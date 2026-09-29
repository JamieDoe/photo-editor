//! Highlights, Shadows, Whites and Blacks (ADR 0023).
//!
//! All four brighten or darken a pixel by a number of stops, applied equally to R, G
//! and B so hues do not shift. They differ in what decides the amount:
//!
//! - **Shadows / Highlights** are *local*: the amount depends on the brightness of the
//!   pixel's surroundings (the edge-aware [`ToneBase`]), not on the pixel itself. Dark
//!   areas are lifted as a whole while the texture inside them keeps its contrast, and
//!   strong edges do not glow, because the base map follows them.
//! - **Whites / Blacks** set the ends of the tonal range: the amount depends on the
//!   pixel's own brightness, near white or near black.
//!
//! Brightness is measured in stops below sensor white (`d = -log2(Y)`, so mid grey
//! 0.18 is 2.47 stops down). Every weight is a smoothstep, so there are no tonal steps.

use image_core::LinearImage;
use image_core::color::REC709_LUMA;

use super::dehaze::DehazeModel;
use super::scene::{GuidedMap, MAP_LONG_EDGE, SceneMap};

/// Maximum lift of Shadows +100 in the darkest areas, in stops.
pub const SHADOWS_STOPS: f32 = 2.0;
/// Maximum change of Highlights ±100 in the brightest areas, in stops.
pub const HIGHLIGHTS_STOPS: f32 = 1.5;
/// Maximum change of Whites ±100 at white, in stops.
pub const WHITES_STOPS: f32 = 1.0;
/// Maximum change of Blacks ±100 in the deepest shadows, in stops.
pub const BLACKS_STOPS: f32 = 1.5;

/// Luminance floor for the logarithm (about 20 stops below white).
const FLOOR: f32 = 1.0e-6;

/// Slider values (-100..100) of the tone controls.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ToneParams {
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
}

impl ToneParams {
    pub fn is_identity(&self) -> bool {
        self.highlights == 0.0 && self.shadows == 0.0 && self.whites == 0.0 && self.blacks == 0.0
    }

    /// Whether the local (neighbourhood) part is needed.
    pub fn is_local(&self) -> bool {
        self.highlights != 0.0 || self.shadows != 0.0
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Stops below white of a linear luminance.
pub fn stops_below_white(y: f32) -> f32 {
    -(y.max(FLOOR)).log2()
}

/// Local part, from the surroundings' brightness `d` (stops below white): shadows act
/// from about mid grey downwards, highlights within about 3 stops of white.
pub fn local_stops(d: f32, p: &ToneParams) -> f32 {
    let shadows = smoothstep(2.0, 5.5, d);
    let highlights = 1.0 - smoothstep(0.25, 3.0, d);
    p.shadows / 100.0 * SHADOWS_STOPS * shadows
        + p.highlights / 100.0 * HIGHLIGHTS_STOPS * highlights
}

/// End-point part, from the pixel's own brightness `d`: whites act on the top 1.5
/// stops, blacks on the deepest tones.
pub fn endpoint_stops(d: f32, p: &ToneParams) -> f32 {
    let whites = 1.0 - smoothstep(0.0, 1.5, d);
    let blacks = smoothstep(4.0, 8.0, d);
    p.whites / 100.0 * WHITES_STOPS * whites + p.blacks / 100.0 * BLACKS_STOPS * blacks
}

/// Reference implementation for one pixel: `base_d` is the surroundings' brightness
/// from [`ToneBase::stops_at`] (ignored when there is no local part).
pub fn apply(rgb: [f32; 3], base_d: f32, p: &ToneParams) -> [f32; 3] {
    let [wr, wg, wb] = REC709_LUMA;
    let d = stops_below_white(rgb[0] * wr + rgb[1] * wg + rgb[2] * wb);
    let gain = (local_stops(base_d, p) + endpoint_stops(d, p)).exp2();
    rgb.map(|c| c * gain)
}

/// Long edge of the base map. Fixed (not a fraction of the render size), so a preview
/// and the full-resolution export see the same surroundings and look the same.
pub const BASE_LONG_EDGE: u32 = MAP_LONG_EDGE;
/// Guided filter window radius on the base map: about a tenth of the picture.
const RADIUS: usize = 13;
/// Guided filter regularisation, in stops²: brightness steps well above about half a
/// stop count as edges and are kept.
const EPSILON: f32 = 0.25;

/// The surroundings' brightness of every point of an image: a fast guided filter
/// (He & Sun) of log luminance, self-guided, computed on the scene map. For a pixel
/// with log luminance `i`, the smoothed value is `a·i + b`, with `a` and `b`
/// interpolated from the map; flat areas get their average, strong edges are kept.
#[derive(Debug, Clone)]
pub struct ToneBase(GuidedMap);

impl ToneBase {
    /// Builds the map for `source` after per-channel `gains` (the white balance and
    /// exposure applied before the tone stage).
    pub fn build(source: &LinearImage, gains: [f32; 3]) -> Self {
        Self::from_scene(&SceneMap::build(source, gains), None)
    }

    /// Builds the map from the scene map, after dehaze if it runs before the tone
    /// stage (ADR 0028).
    pub fn from_scene(scene: &SceneMap, dehaze: Option<&DehazeModel>) -> Self {
        let mut luminance = scene.luminance();
        if let Some(d) = dehaze {
            d.apply_to_map_luminance(&mut luminance);
        }
        let i: Vec<f32> = luminance.iter().map(|&l| l.max(FLOOR).log2()).collect();
        Self::from_log_luminance(scene.width, scene.height, &i)
    }

    /// The guided filter on a `w` x `h` map of log2 luminance.
    pub fn from_log_luminance(w: usize, h: usize, i: &[f32]) -> Self {
        Self(GuidedMap::new(i, i, w, h, RADIUS, EPSILON))
    }

    /// Surroundings' brightness, in stops below white, at pixel (`x`, `y`) of an image
    /// `image_w` x `image_h` whose own log2 luminance there is `log_y`.
    pub fn stops_at(&self, x: usize, y: usize, image_w: usize, image_h: usize, log_y: f32) -> f32 {
        -self.0.value_at(x, y, image_w, image_h, log_y)
    }

    pub fn size(&self) -> (usize, usize) {
        self.0.size()
    }

    /// The underlying model map (`a·log_y + b` is the surroundings' log2 luminance).
    pub fn map(&self) -> &GuidedMap {
        &self.0
    }

    /// See [`GuidedMap::columns`].
    pub fn columns(&self, image_w: usize) -> Vec<(u32, u32, f32)> {
        self.0.columns(image_w)
    }

    /// See [`GuidedMap::row`].
    pub fn row(&self, y: usize, image_h: usize, a_row: &mut Vec<f32>, b_row: &mut Vec<f32>) {
        self.0.row(y, image_h, a_row, b_row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(highlights: f32, shadows: f32, whites: f32, blacks: f32) -> ToneParams {
        ToneParams {
            highlights,
            shadows,
            whites,
            blacks,
        }
    }

    #[test]
    fn neutral_sliders_change_nothing() {
        let p = ToneParams::default();
        assert!(p.is_identity());
        for d in [0.0, 1.0, 2.5, 5.0, 12.0] {
            assert_eq!(local_stops(d, &p), 0.0);
            assert_eq!(endpoint_stops(d, &p), 0.0);
        }
        assert_eq!(apply([0.1, 0.2, 0.3], 3.0, &p), [0.1, 0.2, 0.3]);
    }

    #[test]
    fn shadows_act_on_dark_surroundings_and_highlights_on_bright_ones() {
        let s = params(0.0, 100.0, 0.0, 0.0);
        assert!(
            (local_stops(8.0, &s) - SHADOWS_STOPS).abs() < 1e-6,
            "deep shadow"
        );
        assert!(local_stops(2.47, &s) <= 0.15, "mid grey barely moves");
        assert_eq!(local_stops(0.5, &s), 0.0, "highlights untouched");
        let h = params(-100.0, 0.0, 0.0, 0.0);
        assert!(
            (local_stops(0.0, &h) + HIGHLIGHTS_STOPS).abs() < 1e-6,
            "white recovered"
        );
        assert!(local_stops(2.47, &h).abs() <= 0.15, "mid grey barely moves");
        assert_eq!(local_stops(6.0, &h), 0.0, "shadows untouched");
    }

    #[test]
    fn whites_and_blacks_move_the_ends_of_the_range() {
        let w = params(0.0, 0.0, 100.0, 0.0);
        assert!((endpoint_stops(0.0, &w) - WHITES_STOPS).abs() < 1e-6);
        assert_eq!(endpoint_stops(3.0, &w), 0.0);
        let b = params(0.0, 0.0, 0.0, -100.0);
        assert!((endpoint_stops(10.0, &b) + BLACKS_STOPS).abs() < 1e-6);
        assert_eq!(endpoint_stops(2.47, &b), 0.0, "mid grey untouched");
    }

    #[test]
    fn gains_keep_colour_ratios() {
        let out = apply([0.02, 0.01, 0.005], 6.0, &params(0.0, 60.0, 0.0, 0.0));
        assert!(out[0] > 0.02);
        assert!((out[0] / out[1] - 2.0).abs() < 1e-4 && (out[1] / out[2] - 2.0).abs() < 1e-4);
    }

    #[test]
    fn a_flat_area_is_its_own_surroundings() {
        let (w, h) = (40, 30);
        let i = vec![-4.0f32; w * h];
        let base = ToneBase::from_log_luminance(w, h, &i);
        for (x, y) in [(0, 0), (20, 15), (39, 29)] {
            assert!((base.stops_at(x, y, w, h, -4.0) - 4.0).abs() < 1e-4);
        }
    }

    #[test]
    fn strong_edges_are_kept_so_they_do_not_glow() {
        // Left half 6 stops down, right half 1 stop down: a hard edge in the middle.
        let (w, h) = (64, 32);
        let i: Vec<f32> = (0..w * h)
            .map(|k| if k % w < w / 2 { -6.0 } else { -1.0 })
            .collect();
        let base = ToneBase::from_log_luminance(w, h, &i);
        // Right next to the edge, each side's surroundings stay close to its own level,
        // where a plain blur would average them (−3.5).
        let dark = base.stops_at(w / 2 - 2, h / 2, w, h, -6.0);
        let bright = base.stops_at(w / 2 + 1, h / 2, w, h, -1.0);
        assert!((dark - 6.0).abs() < 0.6, "dark side {dark}");
        assert!((bright - 1.0).abs() < 0.6, "bright side {bright}");
    }

    #[test]
    fn the_map_does_not_depend_on_render_size() {
        // The same scene at two sizes: preview and export must see the same surroundings.
        let scene = |w: u32, h: u32| {
            let data: Vec<u16> = (0..w * h)
                .flat_map(|k| {
                    let (x, y) = (k % w, k / w);
                    let v = if (x * 4 / w + y * 3 / h).is_multiple_of(2) {
                        3000
                    } else {
                        40000
                    };
                    [v; 3]
                })
                .collect();
            LinearImage::new(w, h, data).unwrap()
        };
        let small = ToneBase::build(&scene(600, 400), [1.0; 3]);
        let large = ToneBase::build(&scene(2400, 1600), [1.0; 3]);
        assert_eq!(small.size(), large.size());
        for (fx, fy) in [(0.1, 0.1), (0.5, 0.5), (0.8, 0.3)] {
            let at = |b: &ToneBase, w: usize, h: usize| {
                let (x, y) = ((fx * w as f32) as usize, (fy * h as f32) as usize);
                b.stops_at(x, y, w, h, -3.0)
            };
            assert!((at(&small, 600, 400) - at(&large, 2400, 1600)).abs() < 0.05);
        }
    }
}
