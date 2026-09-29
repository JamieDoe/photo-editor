//! Dehaze (ADR 0028): removes (or adds) atmospheric haze.
//!
//! Haze is modelled the standard way (Koschmieder): what the camera saw is the scene
//! `J` dimmed by the air's transmission `t` plus the airlight `A` scattered in,
//! `I = J·t + A·(1 − t)`. Dehazing solves for `J`.
//!
//! Everything is estimated on the scene map (ADR 0023), so it is cheap and does not
//! depend on render size:
//!
//! - **Airlight** `A`: the average colour of the haziest cells (the top 0.1 % of the
//!   dark channel, He et al.): usually bright sky or distant haze.
//! - **Transmission**: the dark channel prior. Haze-free areas have some colour
//!   channel near black in every patch, so `t = 1 − ω · min over patch and channels
//!   of I/A`.
//! - The transmission is refined with a guided filter whose guide is luminance, so a
//!   full-resolution pixel evaluates it with its own brightness: the map's coarse
//!   patches don't leave halos along horizons or branches.
//!
//! At `+s` the transmission is lowered only by `s` of the estimated haze
//! (`t' = 1 − s·(1 − t)`, at least [`MIN_TRANSMISSION`]); at `−s` airlight is blended
//! in, more where the scene is already hazy.

use super::scene::{GuidedMap, SceneMap, luma};

/// Dark-channel patch radius in map cells (about 6 % of the picture).
const PATCH_RADIUS: usize = 7;
/// Share of the estimated haze removed at +100 (He et al. keep a little, so distant
/// things still look distant).
const OMEGA: f32 = 0.9;
/// Lower bound of the transmission: limits how far hazy areas are stretched.
pub const MIN_TRANSMISSION: f32 = 0.2;
/// Guided filter refining the transmission.
const GUIDE_RADIUS: usize = 13;
const GUIDE_EPS: f32 = 1.0e-3;
/// Share of the haziest cells that define the airlight.
const AIRLIGHT_SHARE: f64 = 0.001;

#[derive(Debug, Clone)]
pub struct DehazeModel {
    /// Slider / 100, -1..1.
    amount: f32,
    airlight: [f32; 3],
    airlight_y: f32,
    inv_airlight_y: f32,
    /// Transmission model; guide is luminance / airlight luminance.
    transmission: GuidedMap,
}

impl DehazeModel {
    /// Estimates haze on `scene` for a Dehaze slider value (-100..100).
    pub fn build(scene: &SceneMap, amount: f32) -> Self {
        let (w, h) = (scene.width, scene.height);
        let raw_dark: Vec<f32> = scene.rgb.iter().map(|c| c[0].min(c[1]).min(c[2])).collect();
        let raw_dark = min_filter(&raw_dark, w, h, PATCH_RADIUS);

        // Airlight: mean colour of the haziest cells.
        // Partial selection, not a full sort: only the top `n` are needed.
        let mut order: Vec<usize> = (0..w * h).collect();
        let n = (((w * h) as f64 * AIRLIGHT_SHARE).ceil() as usize).clamp(1, w * h);
        order.select_nth_unstable_by(n - 1, |&a, &b| raw_dark[b].total_cmp(&raw_dark[a]));
        let mut sum = [0.0f64; 3];
        for &k in &order[..n] {
            for (s, v) in sum.iter_mut().zip(scene.rgb[k]) {
                *s += f64::from(v);
            }
        }
        let airlight = sum.map(|s| ((s / n as f64) as f32).max(1.0e-4));
        let airlight_y = luma(airlight).max(1.0e-4);

        // Dark channel of the scene normalised by the airlight, then transmission.
        let dark: Vec<f32> = scene
            .rgb
            .iter()
            .map(|c| {
                (0..3)
                    .map(|i| c[i] / airlight[i])
                    .fold(f32::INFINITY, f32::min)
            })
            .collect();
        let dark = min_filter(&dark, w, h, PATCH_RADIUS);
        let t: Vec<f32> = dark
            .iter()
            .map(|d| (1.0 - OMEGA * d).clamp(0.0, 1.0))
            .collect();
        let guide: Vec<f32> = scene.rgb.iter().map(|&c| luma(c) / airlight_y).collect();
        Self {
            amount: (amount / 100.0).clamp(-1.0, 1.0),
            airlight,
            airlight_y,
            inv_airlight_y: 1.0 / airlight_y,
            transmission: GuidedMap::new(&guide, &t, w, h, GUIDE_RADIUS, GUIDE_EPS),
        }
    }

    /// The transmission model (guide: [`DehazeModel::guide`]).
    pub fn map(&self) -> &GuidedMap {
        &self.transmission
    }

    pub fn airlight(&self) -> [f32; 3] {
        self.airlight
    }

    /// The guide value of a pixel with linear luminance `y`.
    #[inline]
    pub fn guide(&self, y: f32) -> f32 {
        y * self.inv_airlight_y
    }

    /// `(scale, offset)` per channel such that the output is `input * scale + offset
    /// [c]`, for a pixel whose estimated transmission (the model's value) is `t`.
    #[inline]
    pub fn affine(&self, t: f32) -> (f32, [f32; 3]) {
        let t = t.clamp(0.0, 1.0);
        if self.amount >= 0.0 {
            // J = (I - A) / t' + A
            let t_eff = (1.0 - self.amount * (1.0 - t)).max(MIN_TRANSMISSION);
            let scale = 1.0 / t_eff;
            (scale, self.airlight.map(|a| a * (1.0 - scale)))
        } else {
            // I' = I (1 - h) + A h: more haze where there is already some.
            let haze = -self.amount * (0.1 + 0.3 * (1.0 - t));
            (1.0 - haze, self.airlight.map(|a| a * haze))
        }
    }

    /// Dehazes one pixel whose transmission is `t`.
    #[inline]
    pub fn apply(&self, rgb: [f32; 3], t: f32) -> [f32; 3] {
        let (scale, offset) = self.affine(t);
        [0, 1, 2].map(|c| (rgb[c] * scale + offset[c]).max(0.0))
    }

    /// Dehazes a linear luminance whose transmission is `t` (luminance is linear in
    /// the channels, so this matches [`DehazeModel::apply`]).
    ///
    /// Computed directly (the airlight's luminance stands in for its colour), as it
    /// runs for every pixel the detail stage measures.
    #[inline]
    pub fn apply_luminance(&self, y: f32, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let a = self.airlight_y;
        let out = if self.amount >= 0.0 {
            let t_eff = (1.0 - self.amount * (1.0 - t)).max(MIN_TRANSMISSION);
            (y - a) / t_eff + a
        } else {
            let haze = -self.amount * (0.1 + 0.3 * (1.0 - t));
            y + (a - y) * haze
        };
        out.max(0.0)
    }

    /// Dehazes map-cell luminances in place (for maps built after this stage).
    pub fn apply_to_map_luminance(&self, luminance: &mut [f32]) {
        for (k, y) in luminance.iter_mut().enumerate() {
            let t = self.transmission.cell(k, self.guide(*y));
            *y = self.apply_luminance(*y, t);
        }
    }

    /// Transmission at pixel (`x`, `y`) of a `w` x `h` image with linear luminance
    /// `lum` (reference path).
    pub fn transmission_at(&self, x: usize, y: usize, w: usize, h: usize, lum: f32) -> f32 {
        self.transmission.value_at(x, y, w, h, self.guide(lum))
    }
}

/// Minimum over a (2r+1)² window, clamped at the borders.
fn min_filter(data: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut rows = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let (x0, x1) = (x.saturating_sub(r), (x + r).min(w - 1));
            rows[y * w + x] = data[y * w + x0..=y * w + x1]
                .iter()
                .copied()
                .fold(f32::INFINITY, f32::min);
        }
    }
    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        let (y0, y1) = (y.saturating_sub(r), (y + r).min(h - 1));
        for x in 0..w {
            out[y * w + x] = (y0..=y1)
                .map(|yy| rows[yy * w + x])
                .fold(f32::INFINITY, f32::min);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scene: a clear foreground (saturated colours, deep shadows) at the bottom and
    /// a hazy band (everything pulled towards a bluish-grey airlight) at the top.
    fn hazy_scene() -> SceneMap {
        let (w, h) = (128, 80);
        let air = [0.8, 0.85, 0.95];
        let rgb = (0..w * h)
            .map(|k| {
                let (x, y) = (k % w, k / w);
                let clear = [
                    0.05 + 0.4 * ((x / 8) % 2) as f32,
                    0.1 + 0.3 * ((x / 5) % 2) as f32,
                    0.02 + 0.2 * ((y / 6) % 2) as f32,
                ];
                // Transmission falls from 1 (bottom) to 0.3 (top).
                let t = 0.3 + 0.7 * y as f32 / (h - 1) as f32;
                [0, 1, 2].map(|c| clear[c] * t + air[c] * (1.0 - t))
            })
            .collect();
        SceneMap {
            width: w,
            height: h,
            rgb,
        }
    }

    fn contrast(scene: &SceneMap, m: &DehazeModel, rows: std::ops::Range<usize>) -> f32 {
        let w = scene.width;
        let v: Vec<f32> = rows
            .flat_map(|y| (0..w).map(move |x| y * w + x))
            .map(|k| {
                let y = luma(scene.rgb[k]);
                m.apply_luminance(y, m.map().cell(k, m.guide(y)))
            })
            .collect();
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
    }

    #[test]
    fn airlight_is_the_haze_colour() {
        let m = DehazeModel::build(&hazy_scene(), 100.0);
        let a = m.airlight();
        // Bluish and bright, as the haze is.
        assert!(a[2] > a[0] && a[0] > 0.4, "{a:?}");
    }

    #[test]
    fn zero_changes_nothing() {
        let m = DehazeModel::build(&hazy_scene(), 0.0);
        for t in [0.0, 0.3, 1.0] {
            assert_eq!(m.apply([0.3, 0.2, 0.1], t), [0.3, 0.2, 0.1]);
        }
    }

    #[test]
    fn dehaze_restores_contrast_where_it_was_hazy() {
        let scene = hazy_scene();
        let none = DehazeModel::build(&scene, 0.0);
        let full = DehazeModel::build(&scene, 100.0);
        let add = DehazeModel::build(&scene, -100.0);
        let top = 0..20; // hazy
        let bottom = 70..80; // clear
        assert!(contrast(&scene, &full, top.clone()) > 1.6 * contrast(&scene, &none, top.clone()));
        // The clear foreground changes much less.
        let clear_gain =
            contrast(&scene, &full, bottom.clone()) / contrast(&scene, &none, bottom.clone());
        assert!(clear_gain < 1.3, "{clear_gain}");
        // Negative adds haze: less contrast.
        assert!(contrast(&scene, &add, top.clone()) < 0.8 * contrast(&scene, &none, top));
    }

    #[test]
    fn luminance_form_matches_the_colour_form() {
        let m = DehazeModel::build(&hazy_scene(), 70.0);
        let rgb = [0.4, 0.5, 0.6];
        for t in [0.25, 0.6, 0.95] {
            let a = luma(m.apply(rgb, t));
            let b = m.apply_luminance(luma(rgb), t);
            assert!((a - b).abs() < 1e-5, "{a} vs {b}");
        }
    }

    #[test]
    fn stretch_is_bounded() {
        let m = DehazeModel::build(&hazy_scene(), 100.0);
        let (scale, _) = m.affine(0.0);
        assert!(scale <= 1.0 / MIN_TRANSMISSION + 1e-5);
    }
}
