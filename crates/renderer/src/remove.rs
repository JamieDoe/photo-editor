//! Remove (ADR 0066): paint over something (a wire, a person, a sign) to have it filled
//! in from the rest of the photo.
//!
//! The painted area, the hole, is filled by example-based inpainting: Wexler, Shechtman
//! and Irani's space-time completion, with Barnes et al.'s PatchMatch to find the
//! examples. Every 7 × 7 patch touching the hole is matched to the most similar whole
//! patch outside it, and each hole pixel becomes the weighted vote of the matched
//! patches covering it. Matching and voting alternate, coarse to fine, so structure is
//! settled at low resolution and detail at full.
//!
//! - **Only a window around the hole** is worked on: the hole's bounds plus a context
//!   band a few times its thickness. A thin wire across the frame costs a thin strip.
//! - **Matching is perceptual:** it compares the square root of linear light, so dark
//!   areas count as much as light ones. The result goes back to linear light.
//! - **Deterministic:** random choices come from fixed seeds, and parallel work is
//!   split into fixed bands of rows. The fill is the same on any machine, with any
//!   number of cores.
//! - **Placement:** strokes are in the source photo's own coordinates, like spots, so a
//!   removal stays on what it covers whatever the crop. The fill is computed at the
//!   resolution rendered (a preview level or the full image): previews and exports
//!   agree in structure, if not pixel for pixel.

use image_core::{Cancellation, LinearImage};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::RenderError;
use crate::masks::brush::{self, Stroke, Window};

/// One removal: an area painted over, filled as one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Removal {
    /// The painted area: brush strokes over the source photo, in fractions of its width
    /// and height (the brush size a fraction of its diagonal).
    pub strokes: Vec<Stroke>,
}

impl Removal {
    pub fn sanitized(&self) -> Self {
        Self {
            strokes: self
                .strokes
                .iter()
                .map(Stroke::sanitized)
                .filter(|s| !s.points.is_empty())
                .collect(),
        }
    }

    /// Paints nothing.
    pub fn is_noop(&self) -> bool {
        !self
            .strokes
            .iter()
            .any(|s| !s.erase && s.flow > 0.0 && !s.points.is_empty())
    }
}

/// Patch half-size: patches are 7 × 7.
const HALF: i32 = 3;
const PATCH: usize = 2 * HALF as usize + 1;
/// Coverage above which a pixel is part of the hole (below, a soft edge left as is).
const HOLE_MIN: f32 = 1.0 / 255.0;
/// The context band around the hole: this many times its half-thickness, at least
/// `MIN_CONTEXT` pixels.
const CONTEXT: f32 = 4.0;
const MIN_CONTEXT: usize = 4 * PATCH;
/// Rows per band of parallel work. Fixed, so results do not depend on the core count.
const BAND: usize = 16;

/// `image` with each of `removals` filled in, in order (each sees the fills before it).
pub fn remove(
    image: &LinearImage,
    removals: &[Removal],
    cancel: &dyn Cancellation,
) -> Result<LinearImage, RenderError> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let mut data = image.data().to_vec();
    for removal in removals.iter().map(Removal::sanitized) {
        if !removal.is_noop() {
            fill(&mut data, w, h, &removal.strokes, cancel)?;
        }
    }
    Ok(LinearImage::new(image.width(), image.height(), data).expect("same size as the source"))
}

/// Fills the hole `strokes` paint in `data` (linear RGB, `w` × `h`).
fn fill(
    data: &mut [u16],
    w: usize,
    h: usize,
    strokes: &[Stroke],
    cancel: &dyn Cancellation,
) -> Result<(), RenderError> {
    let Some(reach) = brush::reach(strokes, (w, h)) else {
        return Ok(());
    };
    // The hole's thickness sets the context band and the pyramid's depth.
    let reach_cover = brush::rasterize_window(strokes, (w, h), reach);
    let reach_hole: Vec<bool> = reach_cover.iter().map(|&c| c > HOLE_MIN).collect();
    if !reach_hole.contains(&true) {
        return Ok(());
    }
    let half_thickness = half_thickness(&reach_hole, reach.width, reach.height);
    let margin = ((CONTEXT * half_thickness).ceil() as usize).max(MIN_CONTEXT);
    let region = Window {
        x: reach.x.saturating_sub(margin),
        y: reach.y.saturating_sub(margin),
        width: 0,
        height: 0,
    };
    let region = Window {
        width: (reach.x + reach.width + margin).min(w) - region.x,
        height: (reach.y + reach.height + margin).min(h) - region.y,
        ..region
    };
    let (rw, rh) = (region.width, region.height);
    let cover = brush::rasterize_window(strokes, (w, h), region);
    let hole: Vec<bool> = cover.iter().map(|&c| c > HOLE_MIN).collect();
    let pixel = |x: usize, y: usize| ((region.y + y) * w + region.x + x) * 3;
    let image: Vec<[f32; 3]> = (0..rw * rh)
        .map(|i| {
            let at = pixel(i % rw, i / rw);
            [0, 1, 2].map(|c| (f32::from(data[at + c]) / 65535.0).sqrt())
        })
        .collect();

    let filled = inpaint(image, hole, rw, rh, half_thickness, cancel)?;

    // Back into the photo: the fill where painted, blended by the brush's soft edge.
    for (i, (&a, value)) in cover.iter().zip(&filled).enumerate() {
        if a <= HOLE_MIN {
            continue;
        }
        let at = pixel(i % rw, i / rw);
        for c in 0..3 {
            let original = f32::from(data[at + c]);
            let fill = value[c].max(0.0).powi(2) * 65535.0;
            data[at + c] = (original + (fill - original) * a.min(1.0))
                .round()
                .clamp(0.0, 65535.0) as u16;
        }
    }
    Ok(())
}

/// Half the hole's thickness in pixels: the largest distance from inside it to its
/// edge (3-4 chamfer). Outside the window counts as outside the hole.
fn half_thickness(hole: &[bool], w: usize, h: usize) -> f32 {
    const FAR: u32 = u32::MAX / 2;
    let mut d: Vec<u32> = hole
        .iter()
        .map(|&in_hole| if in_hole { FAR } else { 0 })
        .collect();
    let at = |d: &[u32], x: isize, y: isize| -> u32 {
        if x < 0 || y < 0 || x >= w as isize || y >= h as isize {
            0
        } else {
            d[y as usize * w + x as usize]
        }
    };
    for y in 0..h as isize {
        for x in 0..w as isize {
            let i = y as usize * w + x as usize;
            if d[i] == 0 {
                continue;
            }
            let best = [
                at(&d, x - 1, y) + 3,
                at(&d, x, y - 1) + 3,
                at(&d, x - 1, y - 1) + 4,
                at(&d, x + 1, y - 1) + 4,
            ]
            .into_iter()
            .min()
            .unwrap_or(FAR);
            d[i] = d[i].min(best);
        }
    }
    for y in (0..h as isize).rev() {
        for x in (0..w as isize).rev() {
            let i = y as usize * w + x as usize;
            if d[i] == 0 {
                continue;
            }
            let best = [
                at(&d, x + 1, y) + 3,
                at(&d, x, y + 1) + 3,
                at(&d, x + 1, y + 1) + 4,
                at(&d, x - 1, y + 1) + 4,
            ]
            .into_iter()
            .min()
            .unwrap_or(FAR);
            d[i] = d[i].min(best);
        }
    }
    d.iter().copied().max().unwrap_or(0) as f32 / 3.0
}

/// One level of the pyramid: the image (perceptual values; hole pixels meaningless)
/// and which pixels are hole.
struct Level {
    w: usize,
    h: usize,
    image: Vec<[f32; 3]>,
    hole: Vec<bool>,
}

impl Level {
    /// Half the size: a pixel is hole if any pixel it covers is; known pixels average
    /// the known pixels they cover.
    fn half(&self) -> Self {
        let (w, h) = (self.w.div_ceil(2), self.h.div_ceil(2));
        let mut image = vec![[0.0; 3]; w * h];
        let mut hole = vec![false; w * h];
        for y in 0..h {
            for x in 0..w {
                let (mut sum, mut n, mut any_hole) = ([0.0f32; 3], 0, false);
                for (cx, cy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (fx, fy) = (2 * x + cx, 2 * y + cy);
                    if fx >= self.w || fy >= self.h {
                        continue;
                    }
                    let i = fy * self.w + fx;
                    if self.hole[i] {
                        any_hole = true;
                    } else {
                        for (s, v) in sum.iter_mut().zip(self.image[i]) {
                            *s += v;
                        }
                        n += 1;
                    }
                }
                let i = y * w + x;
                hole[i] = any_hole;
                if n > 0 {
                    image[i] = sum.map(|s| s / n as f32);
                }
            }
        }
        Self { w, h, image, hole }
    }
}

/// Where each patch is matched from, and how well.
#[derive(Debug, Clone, Copy)]
struct Match {
    /// The source patch's centre, as `y * w + x`; `NONE` for no match.
    source: u32,
    distance: f32,
}

const NONE: u32 = u32::MAX;
const UNMATCHED: Match = Match {
    source: NONE,
    distance: f32::INFINITY,
};

/// What a level knows about its patches.
struct Patches {
    /// Centres whose patch touches the hole: the ones matched.
    target: Vec<bool>,
    /// Centres of whole patches entirely outside the hole: the ones matched to.
    valid: Vec<bool>,
    /// The valid centres, for random choices.
    sources: Vec<u32>,
}

impl Patches {
    fn new(level: &Level) -> Self {
        let (w, h) = (level.w, level.h);
        // Hole grown by the patch's half-size: a centre there has hole in its patch.
        let near_hole = dilate(&level.hole, w, h, HALF as usize);
        let half = HALF as usize;
        let valid: Vec<bool> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                x >= half && y >= half && x + half < w && y + half < h && !near_hole[i]
            })
            .collect();
        let sources = (0..w * h).filter(|&i| valid[i]).map(|i| i as u32).collect();
        Self {
            target: near_hole,
            valid,
            sources,
        }
    }
}

/// `mask` grown by `r` pixels in every direction (a square neighbourhood).
fn dilate(mask: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    let rows: Vec<bool> = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            (x.saturating_sub(r)..(x + r + 1).min(w)).any(|xx| mask[y * w + xx])
        })
        .collect();
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            (y.saturating_sub(r)..(y + r + 1).min(h)).any(|yy| rows[yy * w + x])
        })
        .collect()
}

/// The filled image (perceptual values) for `image` with `hole`, `w` × `h`.
fn inpaint(
    image: Vec<[f32; 3]>,
    hole: Vec<bool>,
    w: usize,
    h: usize,
    half_thickness: f32,
    cancel: &dyn Cancellation,
) -> Result<Vec<[f32; 3]>, RenderError> {
    // Coarse enough that the hole is about a patch across, while the level still holds
    // a few patches.
    let mut levels = vec![Level { w, h, image, hole }];
    loop {
        let last = levels.last().expect("at least one level");
        let scale = (1 << (levels.len() - 1)) as f32;
        let thin_enough = half_thickness / scale <= 2.0 * HALF as f32;
        let too_small = last.w.min(last.h) / 2 < 3 * PATCH;
        if thin_enough || too_small || levels.len() >= 10 {
            break;
        }
        let next = last.half();
        levels.push(next);
    }

    let coarsest = levels.len() - 1;
    let mut estimate: Vec<[f32; 3]> = Vec::new();
    let mut matches: Vec<Match> = Vec::new();
    for depth in (0..levels.len()).rev() {
        if cancel.is_cancelled() {
            return Err(RenderError::Cancelled);
        }
        let level = &levels[depth];
        let patches = Patches::new(level);
        let seed = 0x5EED_0000 + depth as u64;
        if patches.sources.is_empty() {
            // Nothing whole to copy from (the hole fills the level): smooth it in.
            estimate = if depth == coarsest {
                peel(level)
            } else {
                upsample_pixels(&estimate, &levels[depth + 1], level)
            };
            matches = vec![UNMATCHED; level.w * level.h];
            continue;
        }
        if depth == coarsest {
            (estimate, matches) = fill_inward(level, &patches, seed);
        } else {
            matches = upsample_matches(&matches, &levels[depth + 1], level, &patches, seed);
            let previous = upsample_pixels(&estimate, &levels[depth + 1], level);
            estimate = vote(level, &patches, &matches, &previous);
        }
        // More rounds where they are cheap; detail needs fewer. A thin hole (a wire) is
        // already coarse enough at full size: its one level gets the middle count.
        let rounds = match (depth == coarsest, depth == 0) {
            (true, false) => 10,
            (true, true) | (false, false) => 5,
            (false, true) => 3,
        };
        for round in 0..rounds {
            if cancel.is_cancelled() {
                return Err(RenderError::Cancelled);
            }
            let sweeps = if depth == coarsest && round == 0 {
                4
            } else {
                2
            };
            patch_match(
                level,
                &patches,
                &estimate,
                &mut matches,
                sweeps,
                seed ^ ((round as u64) << 16),
            );
            estimate = vote(level, &patches, &matches, &estimate);
        }
    }
    Ok(estimate)
}

/// A first guess for the hole: filled from its edge inward, each ring the average of
/// the pixels already known or filled around it ("onion peeling").
fn peel(level: &Level) -> Vec<[f32; 3]> {
    let (w, h) = (level.w, level.h);
    let mut image = level.image.clone();
    let mut done: Vec<bool> = level.hole.iter().map(|&in_hole| !in_hole).collect();
    loop {
        let ring: Vec<(usize, [f32; 3])> = (0..w * h)
            .into_par_iter()
            .filter(|&i| !done[i])
            .filter_map(|i| {
                let (x, y) = ((i % w) as isize, (i / w) as isize);
                let (mut sum, mut n) = ([0.0f32; 3], 0);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (nx, ny) = (x + dx, y + dy);
                        if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
                            continue;
                        }
                        let j = ny as usize * w + nx as usize;
                        if done[j] {
                            for c in 0..3 {
                                sum[c] += image[j][c];
                            }
                            n += 1;
                        }
                    }
                }
                (n > 0).then(|| (i, sum.map(|s| s / n as f32)))
            })
            .collect();
        if ring.is_empty() {
            break;
        }
        for (i, v) in ring {
            image[i] = v;
            done[i] = true;
        }
    }
    image
}

/// Random patches tried for each hole pixel by [`fill_inward`].
const INWARD_SAMPLES: usize = 96;

/// The coarsest level's first fill, from the hole's edge inward (as Newson et al.
/// initialise): ring by ring, each hole pixel takes the centre of the whole patch that
/// best matches the pixels already known around it (the photo's, or earlier rings').
/// Matching only what is known keeps the first guess from being a bland average that
/// flat patches elsewhere would then match best.
fn fill_inward(level: &Level, patches: &Patches, seed: u64) -> (Vec<[f32; 3]>, Vec<Match>) {
    let (w, h) = (level.w, level.h);
    let mut image = level.image.clone();
    let mut known: Vec<bool> = level.hole.iter().map(|&in_hole| !in_hole).collect();
    // Centres matched here: the hole's pixels (the rest of the targets start random).
    let mut matches = random_matches(level, patches, seed);
    let mut filled = vec![false; w * h];
    loop {
        let ring: Vec<usize> = (0..w * h)
            .filter(|&i| !known[i])
            .filter(|&i| neighbours(i, w, h).any(|j| known[j]))
            .collect();
        if ring.is_empty() {
            break;
        }
        let chosen: Vec<(usize, u32)> = ring
            .par_iter()
            .map(|&p| {
                let mut rng = Rng::new(seed ^ (p as u64).wrapping_mul(0xD1B5_4A32_D192_ED03));
                let score = |s: usize| known_distance(level, &image, &known, p, s);
                let mut best = (NONE, f32::INFINITY);
                let try_source = |s: usize, best: &mut (u32, f32)| {
                    if patches.valid[s] {
                        let d = score(s);
                        if d < best.1 {
                            *best = (s as u32, d);
                        }
                    }
                };
                // Continue what the neighbours filled before took, shifted.
                for n in neighbours(p, w, h).filter(|&n| filled[n]) {
                    let s = matches[n].source as usize;
                    let (sx, sy) = ((s % w) as isize, (s / w) as isize);
                    let (cx, cy) = (
                        sx + (p % w) as isize - (n % w) as isize,
                        sy + (p / w) as isize - (n / w) as isize,
                    );
                    if cx >= 0 && cy >= 0 && cx < w as isize && cy < h as isize {
                        try_source(cy as usize * w + cx as usize, &mut best);
                    }
                }
                for _ in 0..INWARD_SAMPLES {
                    try_source(
                        patches.sources[rng.below(patches.sources.len())] as usize,
                        &mut best,
                    );
                }
                // Refine around the best, halving the radius.
                let mut radius = (w.max(h) / 4) as isize;
                while radius >= 1 {
                    let (bx, by) = (
                        (best.0 as usize % w) as isize,
                        (best.0 as usize / w) as isize,
                    );
                    let cx = (bx + rng.within(radius)).clamp(0, w as isize - 1);
                    let cy = (by + rng.within(radius)).clamp(0, h as isize - 1);
                    try_source(cy as usize * w + cx as usize, &mut best);
                    radius /= 2;
                }
                (p, best.0)
            })
            .collect();
        for (p, s) in chosen {
            image[p] = level.image[s as usize];
            known[p] = true;
            filled[p] = true;
            matches[p] = Match {
                source: s,
                distance: f32::INFINITY,
            };
        }
    }
    (image, matches)
}

/// The 8 neighbours of pixel `i` inside a `w` × `h` level.
fn neighbours(i: usize, w: usize, h: usize) -> impl Iterator<Item = usize> {
    let (x, y) = ((i % w) as isize, (i / w) as isize);
    (-1..=1)
        .flat_map(move |dy| (-1..=1).map(move |dx| (x + dx, y + dy)))
        .filter(move |&(nx, ny)| {
            (nx, ny) != (x, y) && nx >= 0 && ny >= 0 && nx < w as isize && ny < h as isize
        })
        .map(move |(nx, ny)| ny as usize * w + nx as usize)
}

/// The mean squared difference between the known pixels of the patch around `t` and
/// the same pixels of the patch around source `s`.
fn known_distance(level: &Level, image: &[[f32; 3]], known: &[bool], t: usize, s: usize) -> f32 {
    let w = level.w as isize;
    let (tx, ty) = ((t % level.w) as isize, (t / level.w) as isize);
    let (sx, sy) = ((s % level.w) as isize, (s / level.w) as isize);
    let (mut sum, mut n) = (0.0f32, 0u32);
    for dy in -HALF as isize..=HALF as isize {
        let y = ty + dy;
        if y < 0 || y >= level.h as isize {
            continue;
        }
        for dx in -HALF as isize..=HALF as isize {
            let x = tx + dx;
            if x < 0 || x >= w || !known[(y * w + x) as usize] {
                continue;
            }
            let a = image[(y * w + x) as usize];
            let b = level.image[((sy + dy) * w + sx + dx) as usize];
            let (d0, d1, d2) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
            sum += d0 * d0 + d1 * d1 + d2 * d2;
            n += 1;
        }
    }
    if n == 0 {
        f32::INFINITY
    } else {
        sum / n as f32
    }
}

/// Every target matched to a random valid patch.
fn random_matches(level: &Level, patches: &Patches, seed: u64) -> Vec<Match> {
    (0..level.w * level.h)
        .map(|i| {
            if patches.target[i] {
                let mut rng = Rng::new(seed ^ (i as u64).wrapping_mul(0x9E37_79B9));
                Match {
                    source: patches.sources[rng.below(patches.sources.len())],
                    distance: f32::INFINITY,
                }
            } else {
                UNMATCHED
            }
        })
        .collect()
}

/// A coarser level's matches carried to `level`: each target takes its coarse
/// parent's match, scaled, at the same offset within it.
fn upsample_matches(
    coarse: &[Match],
    coarse_level: &Level,
    level: &Level,
    patches: &Patches,
    seed: u64,
) -> Vec<Match> {
    let (w, h) = (level.w, level.h);
    let cw = coarse_level.w;
    (0..w * h)
        .into_par_iter()
        .map(|i| {
            if !patches.target[i] {
                return UNMATCHED;
            }
            let (x, y) = (i % w, i / w);
            let parent = coarse[(y / 2).min(coarse_level.h - 1) * cw + (x / 2).min(cw - 1)];
            if parent.source != NONE {
                let (px, py) = (parent.source as usize % cw, parent.source as usize / cw);
                let (sx, sy) = ((2 * px + x % 2).min(w - 1), (2 * py + y % 2).min(h - 1));
                let s = sy * w + sx;
                if patches.valid[s] {
                    return Match {
                        source: s as u32,
                        distance: f32::INFINITY,
                    };
                }
            }
            let mut rng = Rng::new(seed ^ (i as u64).wrapping_mul(0x9E37_79B9));
            Match {
                source: patches.sources[rng.below(patches.sources.len())],
                distance: f32::INFINITY,
            }
        })
        .collect()
}

/// A coarser level's estimate carried to `level` (nearest pixel), with the level's
/// own known pixels.
fn upsample_pixels(coarse: &[[f32; 3]], coarse_level: &Level, level: &Level) -> Vec<[f32; 3]> {
    let (w, cw, ch) = (level.w, coarse_level.w, coarse_level.h);
    (0..level.w * level.h)
        .map(|i| {
            if !level.hole[i] {
                return level.image[i];
            }
            let (x, y) = (i % w, i / w);
            coarse[(y / 2).min(ch - 1) * cw + (x / 2).min(cw - 1)]
        })
        .collect()
}

/// The squared difference between the patch around target `t` in `estimate` and the
/// patch around source `s` in the level's image, giving up past `bound`. Target pixels
/// outside the level are skipped (sources are always whole).
#[inline]
fn distance(level: &Level, estimate: &[[f32; 3]], t: usize, s: usize, bound: f32) -> f32 {
    let w = level.w as isize;
    let (tx, ty) = ((t % level.w) as isize, (t / level.w) as isize);
    let (sx, sy) = ((s % level.w) as isize, (s / level.w) as isize);
    let mut sum = 0.0f32;
    for dy in -HALF as isize..=HALF as isize {
        let (y, yy) = (ty + dy, sy + dy);
        if y < 0 || y >= level.h as isize {
            continue;
        }
        for dx in -HALF as isize..=HALF as isize {
            let x = tx + dx;
            if x < 0 || x >= w {
                continue;
            }
            let a = estimate[(y * w + x) as usize];
            let b = level.image[(yy * w + sx + dx) as usize];
            let (d0, d1, d2) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
            sum += d0 * d0 + d1 * d1 + d2 * d2;
        }
        if sum > bound {
            return sum;
        }
    }
    sum
}

/// PatchMatch: improves every target's match by trying its neighbours' matches,
/// shifted (propagation), and random patches ever nearer its own (random search).
/// Bands of rows run in parallel; each reads matches outside itself from before the
/// sweep, so the result does not depend on scheduling.
fn patch_match(
    level: &Level,
    patches: &Patches,
    estimate: &[[f32; 3]],
    matches: &mut [Match],
    sweeps: usize,
    seed: u64,
) {
    let (w, h) = (level.w, level.h);
    // The estimate changed since the distances were measured.
    matches.par_iter_mut().enumerate().for_each(|(t, m)| {
        if m.source != NONE {
            m.distance = distance(level, estimate, t, m.source as usize, f32::INFINITY);
        }
    });
    for sweep in 0..sweeps {
        let forward = sweep % 2 == 0;
        let before = matches.to_vec();
        matches
            .par_chunks_mut(BAND * w)
            .enumerate()
            .for_each(|(band, rows)| {
                let first_row = band * BAND;
                let row_count = rows.len() / w;
                let mut rng = Rng::new(seed ^ ((sweep as u64) << 40) ^ ((band as u64) << 20));
                let step: isize = if forward { 1 } else { -1 };
                for r in 0..row_count {
                    let ry = if forward { r } else { row_count - 1 - r };
                    let y = first_row + ry;
                    for c in 0..w {
                        let x = if forward { c } else { w - 1 - c };
                        let t = y * w + x;
                        if !patches.target[t] {
                            continue;
                        }
                        let mut best = rows[ry * w + x];
                        let try_source = |s: usize, best: &mut Match| {
                            if s as u32 == best.source || !patches.valid[s] {
                                return;
                            }
                            let d = distance(level, estimate, t, s, best.distance);
                            if d < best.distance {
                                *best = Match {
                                    source: s as u32,
                                    distance: d,
                                };
                            }
                        };
                        // Propagation: the neighbour before (in scan order) along the
                        // row and the column, its match shifted by one.
                        let neighbours = [
                            (x as isize - step, y as isize),
                            (x as isize, y as isize - step),
                        ];
                        for (nx, ny) in neighbours {
                            if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
                                continue;
                            }
                            let n = ny as usize * w + nx as usize;
                            let in_band =
                                (ny as usize) >= first_row && (ny as usize) < first_row + row_count;
                            let m = if in_band {
                                rows[n - first_row * w]
                            } else {
                                before[n]
                            };
                            if m.source == NONE {
                                continue;
                            }
                            let (sx, sy) = (
                                (m.source as usize % w) as isize,
                                (m.source as usize / w) as isize,
                            );
                            let (cx, cy) = (sx + (x as isize - nx), sy + (y as isize - ny));
                            if cx >= 0 && cy >= 0 && cx < w as isize && cy < h as isize {
                                try_source(cy as usize * w + cx as usize, &mut best);
                            }
                        }
                        // Random search, around the best so far, halving the radius.
                        let mut radius = w.max(h) as isize;
                        while radius >= 1 {
                            if best.source != NONE {
                                let (bx, by) = (
                                    (best.source as usize % w) as isize,
                                    (best.source as usize / w) as isize,
                                );
                                let cx = (bx + rng.within(radius)).clamp(0, w as isize - 1);
                                let cy = (by + rng.within(radius)).clamp(0, h as isize - 1);
                                try_source(cy as usize * w + cx as usize, &mut best);
                            }
                            radius /= 2;
                        }
                        rows[ry * w + x] = best;
                    }
                }
            });
    }
}

/// Each hole pixel as the weighted average of what the matched patches covering it
/// say it should be; patches that match well count for more. Known pixels stay.
fn vote(
    level: &Level,
    patches: &Patches,
    matches: &[Match],
    previous: &[[f32; 3]],
) -> Vec<[f32; 3]> {
    let (w, h) = (level.w, level.h);
    // Weights fall off with a patch's distance, relative to a typical one (the 75th
    // percentile), as in Wexler et al.
    let mut distances: Vec<f32> = matches
        .iter()
        .filter(|m| m.source != NONE && m.distance.is_finite())
        .map(|m| m.distance)
        .collect();
    let typical = if distances.is_empty() {
        1.0
    } else {
        let k = (distances.len() * 3 / 4).min(distances.len() - 1);
        let (_, v, _) = distances.select_nth_unstable_by(k, f32::total_cmp);
        v.max(1e-6)
    };
    (0..w * h)
        .into_par_iter()
        .map(|i| {
            if !level.hole[i] {
                return level.image[i];
            }
            let (x, y) = ((i % w) as isize, (i / w) as isize);
            let (mut sum, mut total) = ([0.0f32; 3], 0.0f32);
            for dy in -HALF as isize..=HALF as isize {
                for dx in -HALF as isize..=HALF as isize {
                    // The patch centred at (x - dx, y - dy) covers this pixel at (dx, dy).
                    let (cx, cy) = (x - dx, y - dy);
                    if cx < 0 || cy < 0 || cx >= w as isize || cy >= h as isize {
                        continue;
                    }
                    let c = cy as usize * w + cx as usize;
                    let m = matches[c];
                    if !patches.target[c] || m.source == NONE {
                        continue;
                    }
                    let (sx, sy) = (
                        (m.source as usize % w) as isize + dx,
                        (m.source as usize / w) as isize + dy,
                    );
                    let weight = if m.distance.is_finite() {
                        (-m.distance / (2.0 * typical)).exp().max(1e-8)
                    } else {
                        1e-8
                    };
                    let v = level.image[(sy * w as isize + sx) as usize];
                    for k in 0..3 {
                        sum[k] += weight * v[k];
                    }
                    total += weight;
                }
            }
            if total > 0.0 {
                sum.map(|s| s / total)
            } else {
                previous[i]
            }
        })
        .collect()
}

/// A small, fast, seedable generator (SplitMix64): the same seed, the same fill.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (n > 0).
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// Uniform in `-r..=r`.
    fn within(&mut self, r: isize) -> isize {
        self.below(2 * r as usize + 1) as isize - r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image_core::NeverCancel;

    /// A `w` × `h` image from linear values.
    fn image(w: u32, h: u32, f: impl Fn(u32, u32) -> [f32; 3]) -> LinearImage {
        let data = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| f(x, y).map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16))
            .collect();
        LinearImage::new(w, h, data).unwrap()
    }

    /// A removal painted as one dab at (`x`, `y`) (pixels) of radius `r` pixels.
    fn dab(img: &LinearImage, x: f32, y: f32, r: f32) -> Removal {
        stroke(img, &[[x, y]], r)
    }

    fn stroke(img: &LinearImage, points: &[[f32; 2]], r: f32) -> Removal {
        let (w, h) = (img.width() as f32, img.height() as f32);
        Removal {
            strokes: vec![Stroke {
                erase: false,
                size: r / w.hypot(h),
                feather: 0.0,
                flow: 100.0,
                points: points.iter().map(|p| [p[0] / w, p[1] / h]).collect(),
            }],
        }
    }

    fn pixel(img: &LinearImage, x: u32, y: u32) -> [f32; 3] {
        let i = ((y * img.width() + x) * 3) as usize;
        [0, 1, 2].map(|c| f32::from(img.data()[i + c]) / 65535.0)
    }

    /// Mean absolute difference (perceptual: square roots) over pixels within `r` of
    /// (`cx`, `cy`).
    fn error_in(a: &LinearImage, b: &LinearImage, cx: f32, cy: f32, r: f32) -> f32 {
        let (mut sum, mut n) = (0.0, 0);
        for y in 0..a.height() {
            for x in 0..a.width() {
                if (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy) <= r {
                    let (p, q) = (pixel(a, x, y), pixel(b, x, y));
                    for c in 0..3 {
                        sum += (p[c].sqrt() - q[c].sqrt()).abs();
                    }
                    n += 3;
                }
            }
        }
        sum / n as f32
    }

    #[test]
    fn nothing_painted_changes_nothing() {
        let img = image(64, 48, |x, y| [x as f32 / 64.0, y as f32 / 48.0, 0.3]);
        let out = remove(&img, &[Removal { strokes: vec![] }], &NeverCancel).unwrap();
        assert_eq!(out.data(), img.data());
        assert!(Removal { strokes: vec![] }.is_noop());
    }

    #[test]
    fn a_flat_area_fills_flat_and_nothing_else_changes() {
        // A dark object on a grey wall.
        let img = image(160, 120, |x, y| {
            if (x as f32 - 80.0).hypot(y as f32 - 60.0) < 10.0 {
                [0.02, 0.01, 0.01]
            } else {
                [0.3, 0.3, 0.3]
            }
        });
        let out = remove(&img, &[dab(&img, 80.0, 60.0, 14.0)], &NeverCancel).unwrap();
        for y in 0..120 {
            for x in 0..160 {
                let d = (x as f32 + 0.5 - 80.0).hypot(y as f32 + 0.5 - 60.0);
                if d < 14.0 {
                    for v in pixel(&out, x, y) {
                        assert!((v - 0.3).abs() < 0.002, "({x}, {y}): {v}");
                    }
                } else if d > 16.0 {
                    assert_eq!(pixel(&out, x, y), pixel(&img, x, y), "({x}, {y}) changed");
                }
            }
        }
    }

    #[test]
    fn stripes_continue_across_the_hole() {
        // Horizontal stripes 8 px apart, with a blob over them: the fill should carry
        // the stripes through, as a smooth fill cannot.
        let stripes = |_x: u32, y: u32| {
            if (y / 4).is_multiple_of(2) {
                [0.6, 0.5, 0.4]
            } else {
                [0.05, 0.08, 0.1]
            }
        };
        let truth = image(200, 150, stripes);
        let img = image(200, 150, |x, y| {
            if (x as f32 - 100.0).hypot(y as f32 - 75.0) < 12.0 {
                [0.9, 0.0, 0.0]
            } else {
                stripes(x, y)
            }
        });
        let out = remove(&img, &[dab(&img, 100.0, 75.0, 15.0)], &NeverCancel).unwrap();
        let error = error_in(&out, &truth, 100.0, 75.0, 14.0);
        // What a smooth fill would leave: the stripes' average, about half their
        // difference (0.4 in square roots) everywhere.
        let smooth = error_in(
            &image(200, 150, |_, _| [0.2, 0.2, 0.2]),
            &truth,
            100.0,
            75.0,
            14.0,
        );
        assert!(error < 0.2 * smooth, "error {error}, smooth fill {smooth}");
    }

    #[test]
    fn a_wire_across_the_sky_disappears() {
        // A sky darkening upwards, with a thin wire across it.
        let sky = |_x: u32, y: u32| {
            let t = y as f32 / 300.0;
            [0.2 + 0.3 * t, 0.35 + 0.3 * t, 0.7 + 0.2 * t]
        };
        let truth = image(400, 300, sky);
        let wire = |x: f32| 120.0 + x * 0.1;
        let img = image(400, 300, |x, y| {
            if (y as f32 - wire(x as f32)).abs() < 1.5 {
                [0.02, 0.02, 0.02]
            } else {
                sky(x, y)
            }
        });
        let removal = stroke(&img, &[[0.0, wire(0.0)], [400.0, wire(400.0)]], 4.0);
        let out = remove(&img, &[removal], &NeverCancel).unwrap();
        for x in (10..390).step_by(20) {
            let y = wire(x as f32).round() as u32;
            let (got, want) = (pixel(&out, x, y), pixel(&truth, x, y));
            for c in 0..3 {
                assert!(
                    (got[c].sqrt() - want[c].sqrt()).abs() < 0.03,
                    "({x}, {y}): {got:?} vs {want:?}"
                );
            }
        }
    }

    #[test]
    fn a_hole_at_the_corner_fills() {
        let img = image(96, 64, |x, y| {
            if x < 8 && y < 8 {
                [1.0, 1.0, 1.0]
            } else {
                [0.4, 0.2, 0.1]
            }
        });
        let out = remove(&img, &[dab(&img, 2.0, 2.0, 12.0)], &NeverCancel).unwrap();
        for y in 0..8 {
            for x in 0..8 {
                let v = pixel(&out, x, y);
                assert!(
                    (v[0] - 0.4).abs() < 0.01 && (v[2] - 0.1).abs() < 0.01,
                    "({x}, {y}): {v:?}"
                );
            }
        }
    }

    #[test]
    fn the_same_on_any_number_of_cores() {
        let img = image(240, 180, |x, y| {
            let n = ((x * 7919 + y * 104_729) % 1000) as f32 / 1000.0;
            [
                0.2 + 0.3 * n,
                0.3 + 0.2 * ((x / 6) % 2) as f32,
                0.4 * (y as f32 / 180.0),
            ]
        });
        let removals = [dab(&img, 120.0, 90.0, 20.0), dab(&img, 60.0, 40.0, 8.0)];
        let run = |threads: usize| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap()
                .install(|| remove(&img, &removals, &NeverCancel).unwrap())
        };
        let (one, four) = (run(1), run(4));
        assert_eq!(one.data(), four.data());
        assert_eq!(run(4).data(), four.data(), "and on a second run");
        assert_ne!(one.data(), img.data(), "it did fill something");
    }

    #[test]
    fn stops_when_cancelled() {
        let img = image(120, 90, |_, _| [0.5, 0.5, 0.5]);
        let cancelled = std::sync::atomic::AtomicBool::new(true);
        assert!(matches!(
            remove(&img, &[dab(&img, 60.0, 45.0, 10.0)], &cancelled),
            Err(RenderError::Cancelled)
        ));
    }

    #[test]
    fn the_thickness_is_measured_inside_the_hole() {
        // A 9-pixel-wide band: half its thickness is about 4.5 pixels.
        let (w, h) = (40, 30);
        let hole: Vec<bool> = (0..w * h).map(|i| (10..19).contains(&(i / w))).collect();
        let t = half_thickness(&hole, w, h);
        assert!((4.0..=5.5).contains(&t), "{t}");
    }
}
