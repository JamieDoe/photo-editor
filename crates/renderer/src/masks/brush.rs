//! Brush masks (ADR 0042): painted strokes, rasterised into a coverage map.
//!
//! A stroke is a path the photographer painted, with the brush's size, feather and
//! flow at the time, and whether it painted or erased. Along its path a stroke is
//! even: its strength is its flow, however slowly or quickly it was drawn. Strokes
//! build up in order as paint does (two 50 % strokes over each other give 75 %), and
//! an erasing stroke takes away only what was painted before it.
//!
//! Coverage is rasterised once per set of strokes and size, at the frame's own size
//! (between [`RASTER_LONG_EDGE`] and [`MAX_RASTER_LONG_EDGE`] pixels on its long
//! side), and cached: renders read it with bilinear sampling, so dragging other
//! controls never repaints it. Preview levels up to [`RASTER_LONG_EDGE`] share one map;
//! larger renders (100 %, exports) get their own, so mask edges stay sharp there.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// Coverage map resolution, at least: pixels on the frame's long side.
pub const RASTER_LONG_EDGE: f32 = 2048.0;
/// Coverage map resolution, at most: a full-resolution render of up to about 8K
/// rasterises at its own size (ADR 0070), a larger one at this.
pub const MAX_RASTER_LONG_EDGE: f32 = 8192.0;
/// Brush radii, as fractions of the frame's diagonal.
pub const MIN_SIZE: f32 = 0.0005;
pub const MAX_SIZE: f32 = 0.5;
/// Coverage maps kept: a few masks' worth, and at most this many bytes (full-size
/// maps are large).
const CACHED_MAPS: usize = 6;
const CACHED_BYTES: usize = 192 << 20;

/// One painted (or erased) stroke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Stroke {
    /// Takes coverage away instead of adding it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<bool>"))]
    pub erase: bool,
    /// The brush's radius, as a fraction of the frame's diagonal (a quarter turn leaves
    /// it alone).
    pub size: f32,
    /// 0..100: the share of the radius the soft edge takes.
    pub feather: f32,
    /// 0..100: how much the stroke paints (or erases).
    pub flow: f32,
    /// The path, in frame fractions.
    pub points: Vec<[f32; 2]>,
}

impl Stroke {
    /// In range, with unusable points dropped and coordinates rounded to 1/10000.
    pub fn sanitized(&self) -> Self {
        let finite = |v: f32, or: f32| if v.is_finite() { v } else { or };
        let round = |v: f32| {
            let v = (v.clamp(-2.0, 3.0) * 10_000.0).round() / 10_000.0;
            if v == 0.0 { 0.0 } else { v }
        };
        Self {
            erase: self.erase,
            size: finite(self.size, 0.04).clamp(MIN_SIZE, MAX_SIZE),
            feather: finite(self.feather, 50.0).clamp(0.0, 100.0),
            flow: finite(self.flow, 100.0).clamp(0.0, 100.0),
            points: self
                .points
                .iter()
                .filter(|p| p[0].is_finite() && p[1].is_finite())
                .map(|p| [round(p[0]), round(p[1])])
                .collect(),
        }
    }
}

/// Rasterised coverage (0..1 as 0..65535), `width` x `height` over the frame (or, for
/// a generated mask, over the source: ADR 0074).
#[derive(Debug, PartialEq)]
pub struct CoverageMap {
    width: usize,
    height: usize,
    data: Vec<u16>,
}

impl CoverageMap {
    /// A map from 8-bit coverage (0..255), row by row: a generated mask's (ADR 0074).
    /// `None` when the pixels don't match the size.
    pub fn from_u8(width: usize, height: usize, data: &[u8]) -> Option<Self> {
        (width > 0 && height > 0 && data.len() == width * height).then(|| Self {
            width,
            height,
            data: data.iter().map(|&v| u16::from(v) * 257).collect(),
        })
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Coverage at map point (`x`, `y`), in map pixels (pixel `i` spans `i..i + 1`),
    /// bilinear, clamped at the edges.
    #[inline]
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        let (w, h) = (self.width, self.height);
        let x = (x - 0.5).clamp(0.0, (w - 1) as f32);
        let y = (y - 0.5).clamp(0.0, (h - 1) as f32);
        let (x0, y0) = (x as usize, y as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (x - x0 as f32, y - y0 as f32);
        let at = |xx: usize, yy: usize| f32::from(self.data[yy * w + xx]);
        let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
        let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
        (top + (bottom - top) * ty) / 65535.0
    }
}

/// The map's size for a frame of `w` x `h`: its shape, as long as the frame between
/// [`RASTER_LONG_EDGE`] and [`MAX_RASTER_LONG_EDGE`].
pub fn raster_size(w: f32, h: f32) -> (usize, usize) {
    let long = w.max(h).max(1.0);
    let s = long.clamp(RASTER_LONG_EDGE, MAX_RASTER_LONG_EDGE) / long;
    (
        ((w * s).round() as usize).max(1),
        ((h * s).round() as usize).max(1),
    )
}

/// The strokes' coverage over a frame of `w` x `h`, from the cache when the same
/// strokes were rasterised for a frame of the same shape. While a stroke is being
/// painted only it is rasterised, over the cached map of the strokes before it.
pub fn coverage(strokes: &[Stroke], w: f32, h: f32) -> Arc<CoverageMap> {
    static CACHE: Mutex<Vec<(u64, Arc<CoverageMap>)>> = Mutex::new(Vec::new());
    let size = raster_size(w, h);
    let lookup = |key: u64| {
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let i = cache.iter().position(|(k, _)| *k == key)?;
        // Most recently used last.
        let entry = cache.remove(i);
        let map = Arc::clone(&entry.1);
        cache.push(entry);
        Some(map)
    };
    let key = cache_key(strokes, size);
    if let Some(map) = lookup(key) {
        return map;
    }
    let before = strokes.len().saturating_sub(1);
    let map = match lookup(cache_key(&strokes[..before], size)) {
        Some(base) if before > 0 => Arc::new(rasterize_onto(Some(&base), &strokes[before..], size)),
        _ => Arc::new(rasterize_onto(None, strokes, size)),
    };
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.push((key, Arc::clone(&map)));
    let bytes = |cache: &[(u64, Arc<CoverageMap>)]| -> usize {
        cache.iter().map(|(_, m)| m.data.len() * 2).sum()
    };
    // The least recently used go first; the newest map always stays.
    while cache.len() > 1 && (cache.len() > CACHED_MAPS || bytes(&cache) > CACHED_BYTES) {
        cache.remove(0);
    }
    map
}

fn cache_key(strokes: &[Stroke], size: (usize, usize)) -> u64 {
    let mut h = DefaultHasher::new();
    size.hash(&mut h);
    for s in strokes {
        (
            s.erase,
            s.size.to_bits(),
            s.feather.to_bits(),
            s.flow.to_bits(),
        )
            .hash(&mut h);
        for p in &s.points {
            (p[0].to_bits(), p[1].to_bits()).hash(&mut h);
        }
    }
    h.finish()
}

/// Paints `strokes` into a map of `size`.
pub fn rasterize(strokes: &[Stroke], size: (usize, usize)) -> CoverageMap {
    rasterize_onto(None, strokes, size)
}

/// Paints `strokes` over `base` (or an empty map). Coverage is rounded to the map's
/// precision after each stroke, so painting in steps and all at once agree exactly.
fn rasterize_onto(
    base: Option<&CoverageMap>,
    strokes: &[Stroke],
    (mw, mh): (usize, usize),
) -> CoverageMap {
    let mut cover: Vec<f32> = match base {
        Some(b) if b.size() == (mw, mh) => b.data.iter().map(|&v| f32::from(v) / 65535.0).collect(),
        _ => vec![0.0f32; mw * mh],
    };
    let whole = Window {
        x: 0,
        y: 0,
        width: mw,
        height: mh,
    };
    paint(&mut cover, whole, strokes, (mw, mh), true);
    CoverageMap {
        width: mw,
        height: mh,
        data: cover
            .iter()
            .map(|&c| (c * 65535.0).round() as u16)
            .collect(),
    }
}

/// A rectangle of a map, in map pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

/// The pixels of a `size` map that `strokes` can reach (their brush's radius around
/// their paths), or `None` when they reach none.
pub fn reach(strokes: &[Stroke], (mw, mh): (usize, usize)) -> Option<Window> {
    let diagonal = (mw as f32).hypot(mh as f32);
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for s in strokes.iter().map(Stroke::sanitized) {
        if s.points.is_empty() || s.flow == 0.0 || s.erase {
            continue;
        }
        let r = (s.size * diagonal).max(0.5);
        for p in &s.points {
            let (px, py) = (p[0] * mw as f32, p[1] * mh as f32);
            x0 = x0.min(px - r);
            y0 = y0.min(py - r);
            x1 = x1.max(px + r);
            y1 = y1.max(py + r);
        }
    }
    let clampx = |v: f32| (v.max(0.0) as usize).min(mw);
    let clampy = |v: f32| (v.max(0.0) as usize).min(mh);
    let (wx0, wy0) = (clampx(x0.floor()), clampy(y0.floor()));
    let (wx1, wy1) = (clampx(x1.ceil() + 1.0), clampy(y1.ceil() + 1.0));
    // Lazily: with nothing painted the bounds are inverted, and the sizes would
    // underflow.
    (x0 <= x1 && wx0 < wx1 && wy0 < wy1).then(|| Window {
        x: wx0,
        y: wy0,
        width: wx1 - wx0,
        height: wy1 - wy0,
    })
}

/// The strokes' coverage (0..1) over `window` of a `size` map, unrounded: for the
/// Remove tool (ADR 0066), which needs a hole at full resolution but only around it.
pub fn rasterize_window(strokes: &[Stroke], size: (usize, usize), window: Window) -> Vec<f32> {
    let mut cover = vec![0.0f32; window.width * window.height];
    paint(&mut cover, window, strokes, size, false);
    cover
}

/// Paints `strokes` into `cover`, which holds `window` of a `(mw, mh)` map. With
/// `quantize`, coverage is rounded to the map's 16-bit precision after each stroke.
fn paint(
    cover: &mut [f32],
    window: Window,
    strokes: &[Stroke],
    (mw, mh): (usize, usize),
    quantize: bool,
) {
    let diagonal = (mw as f32).hypot(mh as f32);
    let (wx0, wy0) = (window.x, window.y);
    let (wx1, wy1) = (window.x + window.width, window.y + window.height);
    for s in strokes.iter().map(Stroke::sanitized) {
        if s.points.is_empty() || s.flow == 0.0 {
            continue;
        }
        let r = (s.size * diagonal).max(0.5);
        let inner = r * (1.0 - s.feather / 100.0);
        let pts: Vec<[f32; 2]> = s
            .points
            .iter()
            .map(|p| [p[0] * mw as f32, p[1] * mh as f32])
            .collect();
        let pts = simplify(&pts, 0.25);
        // Segments (or the single point), each with the rows it can reach.
        let segments: Vec<Segment> = if pts.len() == 1 {
            vec![Segment::new(pts[0], pts[0], r)]
        } else {
            pts.windows(2)
                .map(|w| Segment::new(w[0], w[1], r))
                .collect()
        };
        let y0 = segments.iter().map(|g| g.y0).fold(f32::MAX, f32::min);
        let y1 = segments.iter().map(|g| g.y1).fold(f32::MIN, f32::max);
        let x0 = segments.iter().map(|g| g.x0).fold(f32::MAX, f32::min);
        let x1 = segments.iter().map(|g| g.x1).fold(f32::MIN, f32::max);
        let clampx = |v: f32| (v.max(0.0) as usize).clamp(wx0, wx1);
        let clampy = |v: f32| (v.max(0.0) as usize).clamp(wy0, wy1);
        let (by0, by1) = (clampy(y0.floor()), clampy(y1.ceil() + 1.0));
        let (bx0, bx1) = (clampx(x0.floor()), clampx(x1.ceil() + 1.0));
        if by0 >= by1 || bx0 >= bx1 {
            continue;
        }
        let flow = s.flow / 100.0;
        let erase = s.erase;
        let ww = window.width;
        cover[(by0 - wy0) * ww..(by1 - wy0) * ww]
            .par_chunks_mut(ww)
            .enumerate()
            .for_each_init(Vec::new, |row_cover, (r_off, row)| {
                let y = by0 + r_off;
                let py = y as f32 + 0.5;
                // This row's own stroke coverage: the brush's profile, the most at each
                // pixel over the segments that reach it.
                row_cover.clear();
                row_cover.resize(bx1 - bx0, 0.0f32);
                for g in segments.iter().filter(|g| py >= g.y0 && py <= g.y1) {
                    let sx0 = clampx(g.x0.floor()).max(bx0);
                    let sx1 = clampx(g.x1.ceil() + 1.0).min(bx1);
                    for x in sx0..sx1 {
                        let c = profile(g.distance(x as f32 + 0.5, py), inner, r);
                        let v = &mut row_cover[x - bx0];
                        if c > *v {
                            *v = c;
                        }
                    }
                }
                for (c, v) in row[bx0 - wx0..bx1 - wx0].iter_mut().zip(row_cover.iter()) {
                    if *v == 0.0 {
                        continue;
                    }
                    let a = flow * v;
                    let next = if erase {
                        *c * (1.0 - a)
                    } else {
                        *c + (1.0 - *c) * a
                    };
                    *c = if quantize {
                        (next.clamp(0.0, 1.0) * 65535.0).round() / 65535.0
                    } else {
                        next.clamp(0.0, 1.0)
                    };
                }
            });
    }
}

/// A stroke segment in map pixels, with its reach (the rows and columns within the
/// brush's radius of it).
struct Segment {
    a: [f32; 2],
    d: [f32; 2],
    len2: f32,
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
}

impl Segment {
    fn new(a: [f32; 2], b: [f32; 2], r: f32) -> Self {
        let d = [b[0] - a[0], b[1] - a[1]];
        Self {
            a,
            d,
            len2: d[0] * d[0] + d[1] * d[1],
            x0: a[0].min(b[0]) - r,
            x1: a[0].max(b[0]) + r,
            y0: a[1].min(b[1]) - r,
            y1: a[1].max(b[1]) + r,
        }
    }

    /// Distance from point (`px`, `py`) to the segment.
    #[inline]
    fn distance(&self, px: f32, py: f32) -> f32 {
        let t = if self.len2 > 0.0 {
            (((px - self.a[0]) * self.d[0] + (py - self.a[1]) * self.d[1]) / self.len2)
                .clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (qx, qy) = (
            self.a[0] + t * self.d[0] - px,
            self.a[1] + t * self.d[1] - py,
        );
        (qx * qx + qy * qy).sqrt()
    }
}

/// `pts` with points that lie within `tolerance` of the line through their
/// neighbours dropped (Douglas–Peucker), so a smooth stroke is a few segments.
fn simplify(pts: &[[f32; 2]], tolerance: f32) -> Vec<[f32; 2]> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((first, last)) = stack.pop() {
        let seg = Segment::new(pts[first], pts[last], 0.0);
        let (mut far, mut far_d) = (0, tolerance);
        for (i, p) in pts.iter().enumerate().take(last).skip(first + 1) {
            let d = seg.distance(p[0], p[1]);
            if d > far_d {
                far = i;
                far_d = d;
            }
        }
        if far > 0 {
            keep[far] = true;
            stack.push((first, far));
            stack.push((far, last));
        }
    }
    pts.iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(p, _)| *p)
        .collect()
}

/// The brush's strength at distance `d` from its path: full within `inner`, fading
/// (smoothstep) to nothing at `r`.
#[inline]
fn profile(d: f32, inner: f32, r: f32) -> f32 {
    if d <= inner {
        return 1.0;
    }
    if d >= r {
        return 0.0;
    }
    let t = (d - inner) / (r - inner);
    1.0 - t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_paints_what_the_whole_map_paints_there() {
        let strokes = vec![
            stroke(
                &[[0.2, 0.3], [0.5, 0.45], [0.7, 0.4]],
                0.03,
                40.0,
                80.0,
                false,
            ),
            stroke(&[[0.5, 0.45]], 0.01, 0.0, 100.0, true),
        ];
        let size = (300, 200);
        let whole = rasterize(&strokes, size);
        let window = reach(&strokes, size).expect("the strokes reach the map");
        assert!(window.width < 300 && window.height < 200, "{window:?}");
        let part = rasterize_window(&strokes, size, window);
        for y in 0..window.height {
            for x in 0..window.width {
                let full = f32::from(whole.data[(window.y + y) * 300 + window.x + x]) / 65535.0;
                assert!(
                    (part[y * window.width + x] - full).abs() < 1e-3,
                    "({x}, {y})"
                );
            }
        }
        // Nothing painted outside the reach.
        let total: f32 = whole.data.iter().map(|&v| f32::from(v)).sum();
        let inside: f32 = part.iter().map(|&v| v * 65535.0).sum();
        assert!((total - inside).abs() / total.max(1.0) < 1e-3);
        assert_eq!(reach(&[], size), None);
    }

    fn stroke(points: &[[f32; 2]], size: f32, feather: f32, flow: f32, erase: bool) -> Stroke {
        Stroke {
            erase,
            size,
            feather,
            flow,
            points: points.to_vec(),
        }
    }

    /// Coverage at frame fraction (`x`, `y`) of a map for a frame of this shape.
    fn at(map: &CoverageMap, x: f32, y: f32) -> f32 {
        let (w, h) = map.size();
        map.sample(x * w as f32, y * h as f32)
    }

    #[test]
    fn a_stroke_covers_its_path_and_fades_at_its_edge() {
        // A 3:2 preview level: the map is 2048 x 1365, diagonal about 2461 px.
        let size = raster_size(1500.0, 1000.0);
        assert_eq!(size, (2048, 1365));
        let s = stroke(&[[0.2, 0.5], [0.8, 0.5]], 0.05, 50.0, 100.0, false);
        let map = rasterize(&[s], size);
        assert!((at(&map, 0.5, 0.5) - 1.0).abs() < 1e-3);
        // Radius about 123 px of 1365: 0.09 of the height. Beyond it, nothing.
        assert_eq!(at(&map, 0.5, 0.5 + 0.1), 0.0);
        assert_eq!(at(&map, 0.9, 0.5), 0.0);
        // Half way through the soft edge (three quarters of the radius): about half.
        let r = 0.05 * 2048f32.hypot(1365.0) / 1365.0;
        assert!(
            (at(&map, 0.5, 0.5 + 0.75 * r) - 0.5).abs() < 0.03,
            "{}",
            at(&map, 0.5, 0.5 + 0.75 * r)
        );
        // Even along the path.
        assert!((at(&map, 0.3, 0.5) - at(&map, 0.7, 0.5)).abs() < 1e-3);
    }

    #[test]
    fn maps_follow_the_frame_between_their_limits() {
        assert_eq!(raster_size(400.0, 300.0), (2048, 1536));
        assert_eq!(raster_size(3000.0, 2000.0), (3000, 2000));
        assert_eq!(raster_size(6048.0, 4024.0), (6048, 4024));
        assert_eq!(raster_size(12000.0, 8000.0), (8192, 5461));
    }

    #[test]
    fn strokes_build_up_and_erase_only_what_came_before() {
        let size = raster_size(400.0, 400.0);
        let half = |erase| stroke(&[[0.2, 0.5], [0.8, 0.5]], 0.05, 0.0, 50.0, erase);
        let one = rasterize(&[half(false)], size);
        assert!((at(&one, 0.5, 0.5) - 0.5).abs() < 1e-3);
        let two = rasterize(&[half(false), half(false)], size);
        assert!((at(&two, 0.5, 0.5) - 0.75).abs() < 1e-3);
        // Erasing half of it, then painting across again: the new paint stays.
        let full = stroke(&[[0.2, 0.5], [0.8, 0.5]], 0.05, 0.0, 100.0, false);
        let erase = stroke(&[[0.5, 0.2], [0.5, 0.8]], 0.05, 0.0, 100.0, true);
        let erased = rasterize(&[full.clone(), erase.clone()], size);
        assert_eq!(at(&erased, 0.5, 0.5), 0.0);
        assert!((at(&erased, 0.3, 0.5) - 1.0).abs() < 1e-3);
        let repainted = rasterize(&[full.clone(), erase, full], size);
        assert!((at(&repainted, 0.5, 0.5) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn a_single_point_is_a_dab() {
        let size = raster_size(400.0, 400.0);
        let map = rasterize(&[stroke(&[[0.5, 0.5]], 0.05, 0.0, 100.0, false)], size);
        assert!((at(&map, 0.5, 0.5) - 1.0).abs() < 1e-3);
        assert_eq!(at(&map, 0.6, 0.5), 0.0);
    }

    #[test]
    fn maps_are_cached_by_their_strokes_and_shape() {
        let s = vec![stroke(&[[0.1, 0.1], [0.4, 0.3]], 0.02, 30.0, 80.0, false)];
        let a = coverage(&s, 1500.0, 1000.0);
        // Another preview level of the same shape: the same map.
        let b = coverage(&s, 750.0, 500.0);
        assert!(Arc::ptr_eq(&a, &b));
        let mut moved = s.clone();
        moved[0].points[1][0] = 0.41;
        assert!(!Arc::ptr_eq(&a, &coverage(&moved, 1500.0, 1000.0)));
        // Full resolution: a map of its own, at its size.
        let full = coverage(&s, 6000.0, 4000.0);
        assert_eq!(full.size(), (6000, 4000));
        assert!(!Arc::ptr_eq(&a, &full));
    }

    #[test]
    fn painting_in_steps_matches_painting_at_once() {
        let strokes = vec![
            stroke(
                &[[0.1, 0.2], [0.5, 0.5], [0.9, 0.3]],
                0.05,
                60.0,
                70.0,
                false,
            ),
            stroke(&[[0.5, 0.1], [0.5, 0.9]], 0.03, 20.0, 100.0, true),
            stroke(&[[0.2, 0.8], [0.8, 0.7]], 0.04, 50.0, 60.0, false),
        ];
        let size = raster_size(900.0, 600.0);
        let at_once = rasterize(&strokes, size);
        let base = rasterize(&strokes[..2], size);
        let stepped = rasterize_onto(Some(&base), &strokes[2..], size);
        assert_eq!(at_once.data, stepped.data);
    }

    #[test]
    fn erasing_while_painting_takes_paint_away() {
        // As the editor paints: the paint stroke, then an erase stroke growing point
        // by point over the cached map of the paint.
        let paint = stroke(&[[0.1, 0.5], [0.9, 0.5]], 0.05, 0.0, 100.0, false);
        let before = coverage(std::slice::from_ref(&paint), 900.0, 600.0);
        assert!((at(&before, 0.5, 0.5) - 1.0).abs() < 1e-3);
        let mut erase = stroke(&[[0.5, 0.3]], 0.05, 0.0, 100.0, true);
        for y in [0.4, 0.5, 0.6, 0.7] {
            erase.points.push([0.5, y]);
            let map = coverage(&[paint.clone(), erase.clone()], 900.0, 600.0);
            if y >= 0.5 {
                assert_eq!(at(&map, 0.5, 0.5), 0.0, "erased at {y}");
            }
            assert!((at(&map, 0.2, 0.5) - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn a_painting_session_matches_rasterising_from_scratch() {
        // Several strokes, erases among them, each painted point by point through the
        // cache as the editor does; the result must equal a fresh rasterisation.
        let plan: [(bool, [f32; 2], [f32; 2]); 5] = [
            (false, [0.1, 0.3], [0.9, 0.35]),
            (false, [0.2, 0.6], [0.8, 0.55]),
            (true, [0.5, 0.1], [0.5, 0.9]),
            (false, [0.45, 0.2], [0.55, 0.7]),
            (true, [0.1, 0.5], [0.9, 0.5]),
        ];
        let mut strokes: Vec<Stroke> = Vec::new();
        for (erase, a, b) in plan {
            strokes.push(stroke(&[a], 0.03, 50.0, 90.0, erase));
            for k in 1..=12 {
                let t = k as f32 / 12.0;
                let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                strokes.last_mut().unwrap().points.push(p);
                let _ = coverage(&strokes, 1200.0, 800.0);
            }
        }
        let cached = coverage(&strokes, 1200.0, 800.0);
        let fresh = rasterize(&strokes, raster_size(1200.0, 800.0));
        assert_eq!(cached.data, fresh.data);
    }

    #[test]
    fn simplifying_keeps_the_shape() {
        // Many points on a line, one corner: three points remain.
        let mut pts: Vec<[f32; 2]> = (0..50).map(|i| [i as f32, 0.0]).collect();
        pts.extend((1..50).map(|i| [49.0, i as f32]));
        assert_eq!(
            simplify(&pts, 0.25),
            vec![[0.0, 0.0], [49.0, 0.0], [49.0, 49.0]]
        );
    }

    #[test]
    fn sanitising_keeps_strokes_paintable() {
        let s = stroke(
            &[[f32::NAN, 0.2], [0.123456, 9.0]],
            7.0,
            -3.0,
            f32::NAN,
            false,
        )
        .sanitized();
        assert_eq!(s.points, vec![[0.1235, 3.0]]);
        assert_eq!((s.size, s.feather, s.flow), (MAX_SIZE, 0.0, 100.0));
    }
}
