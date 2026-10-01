//! Finding sensor dust (ADR 0058): the soft, round, slightly dark spots that dust on a
//! camera's sensor leaves, most visible in skies and other smooth areas.
//!
//! Dust darkens what is behind it by a similar fraction in every channel, so it is
//! looked for in log luminance:
//! 1. Each pixel is compared with its surroundings: a lightly smoothed image less a
//!    heavily blurred background. Dust is a small patch clearly below its background.
//! 2. The patches below the threshold are gathered into blobs.
//! 3. A blob counts as dust when it is the right size and round enough, darker by a
//!    few percent but not black, about the same in every channel (no colour of its
//!    own), and set in smooth surroundings, which leaves out texture, edges, birds and
//!    coloured things.
//!
//! The best are returned, at most [`MAX_FOUND`], strongest first.

use image_core::LinearImage;
use image_core::color::REC709_LUMA;
use rayon::prelude::*;

use crate::retouch::{Spot, SpotKind, find_source};

/// The most spots reported.
pub const MAX_FOUND: usize = 40;

/// A dust spot found: its centre (fractions of the width and height), its radius (a
/// fraction of the long edge, its soft edge included) and how much darker it is
/// (0.05 is 5%).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dust {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub darkening: f32,
}

/// Blobs need to be at least this much darker than their background (natural log:
/// about 2.5%) to be considered.
const THRESHOLD: f32 = 0.025;
/// Darker than this (about 55%) is something in the scene, not dust.
const MAX_DEPTH: f32 = 0.8;
/// The surroundings' unevenness (standard deviation of log luminance) above which a
/// spot is not looked for: texture hides dust and imitates it.
const MAX_SURROUND_SPREAD: f32 = 0.02;
/// How far one channel's darkening may stray from green's: a fraction of it, plus a
/// little for noise. Dust is close to neutral; a grey cloud on blue sky is not.
const NEUTRAL_TOLERANCE: (f32, f32) = (0.2, 0.01);
/// How unlike its mirror image (through its centre) a spot may be: the summed
/// differences over the summed darkening. Dust is round and even; an edge, or a
/// fragment of cloud, lopsided.
const MAX_LOPSIDED: f32 = 0.35;

/// Dust spots in `image`, strongest first.
pub fn find_dust(image: &LinearImage) -> Vec<Dust> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let long = w.max(h) as f32;
    if w < 64 || h < 64 {
        return Vec::new();
    }
    let log = log_luminance(image);
    // Smoothed against noise; the background at several times the largest dust.
    let fine = blur(&log, w, h, ((long / 1000.0).round() as usize).max(1));
    let background = blur(&log, w, h, ((long / 50.0).round() as usize).max(8));
    let diff: Vec<f32> = fine.iter().zip(&background).map(|(f, b)| f - b).collect();
    let (min_r, max_r) = ((long * 0.0012).max(1.2), long * 0.02);

    let mut seen = vec![false; w * h];
    let mut found = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if seen[start] || diff[start] > -THRESHOLD {
            continue;
        }
        // The blob: connected pixels below the threshold.
        let mut pixels = Vec::new();
        seen[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            pixels.push(i);
            let (x, y) = (i % w, i / w);
            let mut visit = |j: usize| {
                if !seen[j] && diff[j] <= -THRESHOLD {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
            // Far too big for dust: give up on it early.
            if pixels.len() as f32 > std::f32::consts::PI * max_r * max_r * 4.0 {
                stack.clear();
            }
        }
        if let Some(d) = judge(&pixels, &diff, &log, image, w, h, (min_r, max_r)) {
            found.push(d);
        }
    }
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    found.truncate(MAX_FOUND);
    found.into_iter().map(|(d, _)| d).collect()
}

/// Whether `pixels` (a blob of `diff` below the threshold) is dust, and its score.
fn judge(
    pixels: &[usize],
    diff: &[f32],
    log: &[f32],
    image: &LinearImage,
    w: usize,
    h: usize,
    (min_r, max_r): (f32, f32),
) -> Option<(Dust, f32)> {
    let area = pixels.len() as f32;
    let core_r = (area / std::f32::consts::PI).sqrt();
    if core_r < min_r * 0.5 || core_r > max_r {
        return None;
    }
    // Centre, weighted by how much darker each pixel is; extent and depth.
    let (mut sx, mut sy, mut sw, mut depth) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for &i in pixels {
        let (x, y) = (i % w, i / w);
        let wt = -diff[i];
        sx += (x as f32 + 0.5) * wt;
        sy += (y as f32 + 0.5) * wt;
        sw += wt;
        depth = depth.max(-diff[i]);
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    let (bw, bh) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
    // Round: fills most of its box, and the box is not long and thin.
    if area / (bw * bh) < 0.5 || bw.max(bh) / bw.min(bh) > 2.0 || depth > MAX_DEPTH {
        return None;
    }
    let (cx, cy) = (sx / sw, sy / sw);
    // The soft edge reaches past the threshold's outline.
    let radius = (bw.max(bh) / 2.0).max(core_r) * 1.6 + 1.0;
    if radius < min_r || radius > max_r * 1.6 + 1.0 {
        return None;
    }
    // The surroundings: a ring from 1.5 to 2.5 radii, which must be smooth.
    let ring = ring_pixels(cx, cy, radius * 1.5, radius * 2.5, w, h);
    if ring.len() < 12 {
        return None;
    }
    let mean = ring.iter().map(|&i| log[i]).sum::<f32>() / ring.len() as f32;
    let spread =
        (ring.iter().map(|&i| (log[i] - mean).powi(2)).sum::<f32>() / ring.len() as f32).sqrt();
    if spread > MAX_SURROUND_SPREAD || depth < spread * 3.0 {
        return None;
    }
    // Neutral: each channel darkened by about the same fraction.
    let core: Vec<usize> = pixels.to_vec();
    let channel_drop =
        |c: usize| mean_log_channel(image, &core, c) - mean_log_channel(image, &ring, c);
    let drops = [channel_drop(0), channel_drop(1), channel_drop(2)];
    let mid = drops[1];
    let (fraction, plus) = NEUTRAL_TOLERANCE;
    if mid >= 0.0
        || drops
            .iter()
            .any(|d| (d - mid).abs() > fraction * mid.abs() + plus)
    {
        return None;
    }
    // Even: each pixel within the radius darkened about as much as the one opposite
    // it through the centre. Dust is round and soft all round; an edge, or anything
    // dark on one side only, is not.
    let (mut differ, mut total) = (0.0f32, 0.0f32);
    for i in ring_pixels(cx, cy, 0.0, radius, w, h) {
        let (x, y) = ((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
        let (mx, my) = (2.0 * cx - x, 2.0 * cy - y);
        if mx < 0.0 || my < 0.0 || mx >= w as f32 || my >= h as f32 {
            continue;
        }
        let m = my as usize * w + mx as usize;
        let (a, b) = (diff[i].min(0.0), diff[m].min(0.0));
        differ += (a - b).abs();
        total += a.abs() + b.abs();
    }
    if total <= 0.0 || differ / total > MAX_LOPSIDED {
        return None;
    }
    let long = w.max(h) as f32;
    let dust = Dust {
        x: cx / w as f32,
        y: cy / h as f32,
        radius: radius / long,
        darkening: 1.0 - (-depth).exp(),
    };
    Some((dust, depth / (spread + 0.005)))
}

fn log_luminance(image: &LinearImage) -> Vec<f32> {
    let [wr, wg, wb] = REC709_LUMA;
    image
        .data()
        .as_chunks::<3>()
        .0
        .par_iter()
        .map(|p| {
            let y = (f32::from(p[0]) * wr + f32::from(p[1]) * wg + f32::from(p[2]) * wb) / 65535.0;
            y.max(1e-4).ln()
        })
        .collect()
}

fn mean_log_channel(image: &LinearImage, pixels: &[usize], c: usize) -> f32 {
    let data = image.data();
    pixels
        .iter()
        .map(|&i| (f32::from(data[i * 3 + c]) / 65535.0).max(1e-4).ln())
        .sum::<f32>()
        / pixels.len().max(1) as f32
}

/// Pixel indices between radii `inner` and `outer` of (`cx`, `cy`), inside the image.
fn ring_pixels(cx: f32, cy: f32, inner: f32, outer: f32, w: usize, h: usize) -> Vec<usize> {
    let x0 = (cx - outer).floor().max(0.0) as usize;
    let y0 = (cy - outer).floor().max(0.0) as usize;
    let x1 = ((cx + outer).ceil() as usize).min(w);
    let y1 = ((cy + outer).ceil() as usize).min(h);
    let mut out = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let d = (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy);
            if d >= inner && d <= outer {
                out.push(y * w + x);
            }
        }
    }
    out
}

/// Three box blurs of radius `r` (close to a Gaussian), rows then columns each time,
/// in parallel.
fn blur(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut a = src.to_vec();
    let mut b = vec![0.0f32; a.len()];
    for _ in 0..3 {
        box_rows(&a, &mut b, w, r);
        box_cols(&b, &mut a, w, h, r);
    }
    a
}

/// Running-sum box blur along each row, edges clamped.
fn box_rows(src: &[f32], dst: &mut [f32], w: usize, r: usize) {
    let n = (2 * r + 1) as f32;
    dst.par_chunks_mut(w)
        .zip(src.par_chunks(w))
        .for_each(|(out, row)| {
            let at = |x: isize| row[x.clamp(0, w as isize - 1) as usize];
            let mut sum: f32 = (-(r as isize)..=r as isize).map(at).sum();
            for (x, o) in out.iter_mut().enumerate() {
                *o = sum / n;
                sum += at(x as isize + r as isize + 1) - at(x as isize - r as isize);
            }
        });
}

/// Box blur down the columns, edges clamped: a running sum of whole rows, so memory
/// is read in order; bands of rows in parallel, each starting its own sum.
fn box_cols(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    const BAND: usize = 64;
    let n = (2 * r + 1) as f32;
    let row = |y: isize| &src[y.clamp(0, h as isize - 1) as usize * w..][..w];
    dst.par_chunks_mut(w * BAND)
        .enumerate()
        .for_each(|(band, out)| {
            let y0 = (band * BAND) as isize;
            let mut acc = vec![0.0f32; w];
            for y in y0 - r as isize..=y0 + r as isize {
                for (a, v) in acc.iter_mut().zip(row(y)) {
                    *a += v;
                }
            }
            for (k, line) in out.chunks_mut(w).enumerate() {
                let y = y0 + k as isize;
                for (o, a) in line.iter_mut().zip(&acc) {
                    *o = a / n;
                }
                let (add, sub) = (row(y + r as isize + 1), row(y - r as isize));
                for ((a, p), m) in acc.iter_mut().zip(add).zip(sub) {
                    *a += p - m;
                }
            }
        });
}

/// Heal spots for the dust in `image` that `existing` spots do not already cover, each
/// with a source found as for a clicked spot (clear of the others).
pub fn dust_spots(image: &LinearImage, existing: &[Spot]) -> Vec<Spot> {
    let (w, h) = (image.width() as f32, image.height() as f32);
    let long = w.max(h);
    let covered = |d: &Dust| {
        existing.iter().any(|s| {
            let s = s.sanitized();
            ((s.x - d.x) * w).hypot((s.y - d.y) * h) < s.radius * long
        })
    };
    let mut spots: Vec<Spot> = Vec::new();
    for d in find_dust(image).iter().filter(|d| !covered(d)) {
        let avoid: Vec<Spot> = existing.iter().chain(&spots).copied().collect();
        if let Some([source_x, source_y]) = find_source(image, d.x, d.y, d.radius, &avoid) {
            spots.push(
                Spot {
                    kind: SpotKind::Heal,
                    x: d.x,
                    y: d.y,
                    source_x,
                    source_y,
                    radius: d.radius,
                    ..Default::default()
                }
                .sanitized(),
            );
        }
    }
    spots
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1600 x 1000 "sky": a smooth gradient with slight noise, as linear RGB.
    fn sky() -> Vec<[f32; 3]> {
        let (w, h) = (1600usize, 1000usize);
        (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
                // Deterministic noise, about 0.5%.
                let n =
                    (((i as u64).wrapping_mul(2_654_435_761) >> 16) % 1000) as f32 / 1000.0 - 0.5;
                let base = 0.25 + 0.2 * (1.0 - y) + 0.05 * x;
                let v = 1.0 + n * 0.01;
                [base * 0.7 * v, base * 0.85 * v, base * 1.1 * v]
            })
            .collect()
    }

    /// Darkens a soft disc (Gaussian, `sigma` px) by `fraction` times `tint`.
    fn spot(img: &mut [[f32; 3]], cx: f32, cy: f32, sigma: f32, fraction: f32, tint: [f32; 3]) {
        let w = 1600usize;
        let r = (sigma * 4.0) as isize;
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (cx as isize + dx, cy as isize + dy);
                if x < 0 || y < 0 || x >= 1600 || y >= 1000 {
                    continue;
                }
                let g = (-((dx * dx + dy * dy) as f32) / (2.0 * sigma * sigma)).exp();
                let p = &mut img[y as usize * w + x as usize];
                for c in 0..3 {
                    p[c] *= 1.0 - fraction * g * tint[c];
                }
            }
        }
    }

    fn image(px: &[[f32; 3]]) -> LinearImage {
        let data = px
            .iter()
            .flat_map(|p| p.map(|v| (v.clamp(0.0, 1.0) * 65535.0) as u16))
            .collect();
        LinearImage::new(1600, 1000, data).unwrap()
    }

    fn near(found: &[Dust], x: f32, y: f32) -> bool {
        found
            .iter()
            .any(|d| (d.x * 1600.0 - x).hypot(d.y * 1000.0 - y) < 4.0)
    }

    #[test]
    fn finds_soft_dust_in_a_sky_and_nothing_else() {
        let mut px = sky();
        let neutral = [1.0; 3];
        // Dust of a few sizes and strengths.
        let dust = [
            (300.0, 200.0, 3.0, 0.12),
            (900.0, 300.0, 6.0, 0.08),
            (1300.0, 700.0, 4.0, 0.2),
            (600.0, 800.0, 2.5, 0.1),
        ];
        for (x, y, s, f) in dust {
            spot(&mut px, x, y, s, f, neutral);
        }
        // Not dust: a coloured blob, something black, a long thin streak (a wire),
        // and a small grey cloud (darkening blue more than red, as grey does on blue
        // sky).
        spot(&mut px, 200.0, 600.0, 5.0, 0.4, [0.1, 1.0, 1.0]);
        spot(&mut px, 1450.0, 250.0, 5.0, 0.15, [0.55, 1.0, 1.45]);
        spot(&mut px, 1100.0, 150.0, 5.0, 0.95, neutral);
        for x in 400..1000 {
            spot(&mut px, x as f32, 500.0, 1.5, 0.06, neutral);
        }
        let found = find_dust(&image(&px));
        for (x, y, _, _) in dust {
            assert!(near(&found, x, y), "missed dust at {x},{y}: {found:?}");
        }
        assert_eq!(found.len(), dust.len(), "{found:?}");
        // Sizes cover the soft edge; darkening about as made.
        let big = found
            .iter()
            .find(|d| (d.x * 1600.0 - 900.0).abs() < 4.0)
            .unwrap();
        assert!(
            big.radius * 1600.0 > 6.0 && big.radius * 1600.0 < 30.0,
            "{big:?}"
        );
        assert!(big.darkening > 0.04 && big.darkening < 0.12, "{big:?}");
    }

    #[test]
    fn texture_hides_nothing_and_finds_nothing() {
        // Busy texture (foliage-like): plenty of small dark bits, none of them dust.
        let w = 1600usize;
        let px: Vec<[f32; 3]> = (0..w * 1000)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let v = 0.2
                    + 0.15 * (((x * 7 + y * 13) % 17) as f32 / 17.0)
                    + 0.1 * ((x / 5 + y / 3) % 2) as f32;
                [v * 0.6, v, v * 0.5]
            })
            .collect();
        assert!(find_dust(&image(&px)).is_empty());
        // A plain sky has none.
        assert!(find_dust(&image(&sky())).is_empty());
    }

    #[test]
    fn spots_heal_what_is_not_already_covered() {
        let mut px = sky();
        spot(&mut px, 300.0, 200.0, 3.0, 0.12, [1.0; 3]);
        spot(&mut px, 900.0, 300.0, 6.0, 0.08, [1.0; 3]);
        let img = image(&px);
        let all = dust_spots(&img, &[]);
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|s| s.kind == SpotKind::Heal && !s.is_noop()));
        // One already healed by hand: only the other is offered.
        let by_hand = Spot {
            x: 300.0 / 1600.0,
            y: 0.2,
            radius: 0.01,
            source_x: 0.3,
            ..Default::default()
        };
        let rest = dust_spots(&img, &[by_hand]);
        assert_eq!(rest.len(), 1);
        assert!((rest[0].x * 1600.0 - 900.0).abs() < 4.0);
        // Healing them removes the dust.
        let healed = crate::retouch::retouch(&img, &all);
        assert!(find_dust(&healed).is_empty());
    }

    #[test]
    fn the_fast_blur_is_a_box_blur() {
        // Against a direct three-pass box blur with clamped edges.
        let (w, h, r) = (37usize, 150usize, 3usize);
        let src: Vec<f32> = (0..w * h).map(|i| ((i * 7919) % 101) as f32).collect();
        let direct = |s: &[f32]| {
            let mut a = s.to_vec();
            for _ in 0..3 {
                let at = |a: &[f32], x: isize, y: isize| {
                    a[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize]
                };
                let rows: Vec<f32> = (0..w * h)
                    .map(|i| {
                        (-(r as isize)..=r as isize)
                            .map(|d| at(&a, (i % w) as isize + d, (i / w) as isize))
                            .sum::<f32>()
                            / (2 * r + 1) as f32
                    })
                    .collect();
                a = (0..w * h)
                    .map(|i| {
                        (-(r as isize)..=r as isize)
                            .map(|d| at(&rows, (i % w) as isize, (i / w) as isize + d))
                            .sum::<f32>()
                            / (2 * r + 1) as f32
                    })
                    .collect();
            }
            a
        };
        for (a, b) in blur(&src, w, h, r).iter().zip(direct(&src)) {
            assert!((a - b).abs() < 1e-2, "{a} vs {b}");
        }
    }
}
