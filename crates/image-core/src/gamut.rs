//! Soft gamut compression (ADR 0060): bringing colours that lie outside sRGB inside it
//! smoothly, instead of clipping each channel at zero (which flattens them and shifts
//! their hue).
//!
//! The curve is the ACES reference gamut compression, per channel: a channel's
//! distance from the brightest channel, `(max - c) / max`, is 0 for a grey and 1 on
//! the gamut's edge (a channel at zero); beyond 1 lies outside. Distances below a
//! threshold are left alone; above it they are eased so that a chosen limit lands
//! exactly on the edge. Greys, and colours well inside, never change.
//!
//! Used where colour enters (raw files decoded in Rec.2020, then compressed into the
//! sRGB working space) and where it leaves (edits that push colours past sRGB, before
//! they are encoded).

/// Linear Rec.2020 to linear sRGB (both D65).
pub const REC2020_TO_SRGB: [[f32; 3]; 3] = [
    [1.660_491, -0.587_641, -0.072_85],
    [-0.124_55, 1.132_9, -0.008_349],
    [-0.018_151, -0.100_579, 1.118_73],
];

/// How strongly to compress: from what distance, and which distance (per channel:
/// red, green, blue) lands on the edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Compression {
    pub threshold: f32,
    pub limits: [f32; 3],
    pub power: f32,
}

/// Raw files' colour (Rec.2020) into sRGB, sized for what cameras record: colours up
/// to 20% beyond sRGB's edge (the fixtures' reach beyond sRGB averages 3-6%) are
/// brought inside, easing in from 97% of the way to the edge, so in-gamut colours keep
/// their values. The few beyond 20% (Rec.2020's far corners, which cameras rarely
/// reach) still clip as before. Gentler and later than ACES's defaults on purpose:
/// measured against a hard clip, it changes about 2% of a photo's pixels by about 4
/// levels where it changes them (ADR 0060).
pub const FROM_REC2020: Compression = Compression {
    threshold: 0.97,
    limits: [1.2, 1.2, 1.2],
    power: 1.2,
};

/// Edits' colour on the way out: colours pushed up to 30% beyond sRGB's edge are
/// brought back smoothly, easing in from 97% of the way to the edge, so in-gamut
/// colours (and existing edits' look) stay as they were.
pub const ON_OUTPUT: Compression = Compression {
    threshold: 0.97,
    limits: [1.3, 1.3, 1.3],
    power: 1.2,
};

impl Compression {
    /// The curve's scale for a channel's limit (so that `limit` maps to 1).
    fn scale(&self, limit: f32) -> f32 {
        let (t, p) = (self.threshold, self.power);
        (limit - t) / (((1.0 - t) / (limit - t)).powf(-p) - 1.0).powf(1.0 / p)
    }

    fn compress_distance(&self, d: f32, scale: f32) -> f32 {
        let (t, p) = (self.threshold, self.power);
        if d < t {
            return d;
        }
        let x = (d - t) / scale;
        t + (d - t) / (1.0 + x.powf(p)).powf(1.0 / p)
    }
}

/// `rgb` (linear) with any channel far below the brightest brought in smoothly, as
/// `c` sets. Unchanged when every channel is within the threshold (greys always).
#[inline]
pub fn compress(rgb: [f32; 3], c: &Compression) -> [f32; 3] {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    if max <= 0.0 {
        return rgb;
    }
    // Most pixels: nothing near the edge, so nothing to do.
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    if (max - min) / max < c.threshold {
        return rgb;
    }
    std::array::from_fn(|i| {
        let d = (max - rgb[i]) / max;
        let cd = c.compress_distance(d, c.scale(c.limits[i]));
        max - cd * max
    })
}

/// A linear Rec.2020 colour in linear sRGB, compressed into its gamut.
#[inline]
pub fn rec2020_to_srgb(rgb: [f32; 3]) -> [f32; 3] {
    let m = &REC2020_TO_SRGB;
    let s = [0, 1, 2].map(|r| m[r][0] * rgb[0] + m[r][1] * rgb[1] + m[r][2] * rgb[2]);
    compress(s, &FROM_REC2020)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_srgb(rgb: [f32; 3]) -> [f32; 3] {
        let m = &REC2020_TO_SRGB;
        [0, 1, 2].map(|r| m[r][0] * rgb[0] + m[r][1] * rgb[1] + m[r][2] * rgb[2])
    }

    /// Points on Rec.2020's edge (one channel 0, one 1, the third anywhere).
    fn rec2020_edge() -> Vec<[f32; 3]> {
        let mut v = Vec::new();
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            for p in [
                [1.0, t, 0.0],
                [t, 1.0, 0.0],
                [0.0, 1.0, t],
                [0.0, t, 1.0],
                [t, 0.0, 1.0],
                [1.0, 0.0, t],
            ] {
                v.push(p);
            }
        }
        v
    }

    #[test]
    fn colours_up_to_the_limit_land_inside_srgb_and_beyond_it_clip() {
        // Over Rec.2020's edge: every colour within the limits is brought inside sRGB;
        // the far corners beyond them are left for the decoder's clip, as before.
        let (mut inside, mut beyond) = (0, 0);
        for p in rec2020_edge() {
            let s = to_srgb(p);
            let max = s[0].max(s[1]).max(s[2]);
            let within = (0..3).all(|c| (max - s[c]) / max <= FROM_REC2020.limits[c]);
            if within {
                inside += 1;
                let out = rec2020_to_srgb(p);
                assert!(out.iter().all(|c| *c >= -1e-3), "{p:?} -> {out:?}");
            } else {
                beyond += 1;
            }
        }
        assert!(inside > 0 && beyond > 0, "{inside} {beyond}");
    }

    #[test]
    fn real_camera_colour_lands_inside_near_the_edge() {
        // About 6% beyond sRGB, as the fixtures' most saturated colours: brought
        // inside, still near the edge (not pulled far in), the brightest untouched.
        let s = compress([0.6, 0.3, -0.036], &FROM_REC2020);
        assert!(s[2] >= 0.0 && s[2] < 0.02, "{s:?}");
        assert_eq!(s[0], 0.6);
    }

    #[test]
    fn colours_well_inside_and_greys_are_untouched() {
        for p in [
            [0.5, 0.5, 0.5],
            [0.4, 0.2, 0.1],
            [0.1, 0.3, 0.6],
            [0.03, 0.3, 0.6],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
        ] {
            assert_eq!(compress(p, &FROM_REC2020), p);
            assert_eq!(compress(p, &ON_OUTPUT), p);
        }
        // Saturated colours just inside the edge (98% of the way) move only a little.
        let near = compress([0.012, 0.3, 0.6], &FROM_REC2020);
        assert!(
            (near[0] - 0.012).abs() < 0.01 && near[1] == 0.3 && near[2] == 0.6,
            "{near:?}"
        );
        // Rec.2020 greys stay grey in sRGB.
        let g = rec2020_to_srgb([0.3, 0.3, 0.3]);
        assert!(
            (g[0] - 0.3).abs() < 1e-4 && (g[1] - 0.3).abs() < 1e-4 && (g[2] - 0.3).abs() < 1e-4
        );
    }

    #[test]
    fn compression_is_smooth_and_keeps_the_brightest_channel() {
        // Pushing blue below zero step by step: the result keeps falling, never jumps,
        // and red (the brightest) is never touched.
        let mut last = f32::MAX;
        for i in 0..=60 {
            let b = 0.2 - i as f32 * 0.005;
            let out = compress([0.8, 0.4, b], &ON_OUTPUT);
            assert_eq!(out[0], 0.8);
            assert!(out[2] <= last + 1e-6, "not monotone at {b}");
            assert!(
                last == f32::MAX || (last - out[2]).abs() < 0.01,
                "a jump at {b}"
            );
            last = out[2];
        }
        // Up to the limit, inside the gamut.
        assert!(compress([0.8, 0.4, 0.8 - 1.3 * 0.8], &ON_OUTPUT)[2] >= -1e-4);
    }
}
