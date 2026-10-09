//! Sky masks without a model (ADR 0074, phase 2, option b): the sky found by what it
//! looks like and where it is, its edges then refined against the photo.
//!
//! 1. **A coarse grid** of about [`COARSE_EDGE`] cells on the long edge: each cell's
//!    mean colour, its brightness in the scene, and how much its pixels vary (sky is
//!    smooth; foliage, branches and walls are not).
//! 2. **Candidates:** cells bright enough to be sky, neither green nor warm, and smooth.
//!    With the camera's exposure ([`Picture::scene_ev`]), "bright" is the scene's own
//!    brightness: sky is several stops brighter than anything indoors, which tells a
//!    white wall or a studio backdrop from an overcast sky.
//! 3. **The sky** grows from candidates on the top edge, across neighbours of nearly
//!    the same colour and brightness, so it stops at a horizon even when the sea
//!    below is as blue.
//!    Cells whose bright pixels are sky-coloured and much brighter than their dark ones
//!    are sky through branches. What grew must be blue, or else among the brightest
//!    things in the photo (a grey wall in daylight is not).
//! 4. **Holes** the sky encloses (clouds, a bird), not reaching the bottom of the
//!    photo, are sky too unless green or warm.
//! 5. **Edges:** the cells, as a mask at up to [`FINE_EDGE`] px, refined with a guided
//!    filter on the photo's luminance, so the mask follows tree lines and rooftops.
//!
//! 5b. Through branches, each pixel's share of sky comes from its brightness between
//!    its cell's dark and bright pixels, so the mask runs between the twigs.
//!
//! Night skies (too dark) and sunsets (warm) are not found yet.

mod guided;

use crate::{AiError, Coverage, MaskKind, Picture, Segmenter};

/// What made a sky mask, and its version: a change to how the sky is found changes it.
pub const GENERATOR: &str = "photo-editor/sky/1";

/// The long edge the mask is made at (about; a whole fraction of the picture).
const FINE_EDGE: usize = 1536;
/// The long edge of the grid the sky is found on (about).
const COARSE_EDGE: usize = 384;
/// The scene brightness sky reaches (EV at ISO 100): daylight sky is 11–15, a pale sky
/// at dusk about 10; indoor scenes, even their white walls, stay below about 8.
const SKY_EV: f32 = 9.0;
/// Without the camera's exposure: the linear luminance sky reaches in the picture.
const SKY_LUMINANCE: f32 = 0.3;
/// Less than this share of the grid is no sky.
const MIN_SHARE: f32 = 0.015;
/// Enclosed holes larger than this share of the grid are left alone.
const MAX_HOLE_SHARE: f32 = 0.3;
/// Sky between branches: a cell's bright quarter at least this many stops brighter
/// than its dark quarter (dark twigs against sky).
const BRANCH_CONTRAST: f32 = 0.25;
/// The step in brightness (stops) the sky crosses between neighbouring cells, and
/// darker into a branch cell.
const STEP: f32 = 0.33;
const BRANCH_STEP: f32 = 0.6;
/// A sky this much bluer than red (sRGB) is blue; a sky that isn't must be at least
/// this bright against the photo's brightest (its 97th percentile).
const BLUE: f32 = 0.04;
const GREY_SKY_BRIGHTNESS: f32 = 0.55;
/// Dark parts this much bluer than red (sRGB) are water, not twigs: a branch cell's
/// dark quarter must be less blue.
const WATER_BLUE: f32 = 0.1;

/// Finds the sky (ADR 0074): the only kind it makes.
pub struct SkyFinder;

impl Segmenter for SkyFinder {
    fn supports(&self, kind: MaskKind) -> bool {
        kind == MaskKind::Sky
    }

    fn generator(&self, _kind: MaskKind) -> String {
        GENERATOR.to_owned()
    }

    fn segment(&self, picture: Picture<'_>, kind: MaskKind) -> Result<Option<Coverage>, AiError> {
        if kind != MaskKind::Sky {
            return Err(AiError::Unsupported(kind));
        }
        picture.check()?;
        Ok(find_sky(&picture))
    }
}

/// An image as sRGB-encoded planes (0..1), and its luminance (from the encoded values:
/// the guide the edges follow).
struct Planes {
    width: usize,
    height: usize,
    rgb: [Vec<f32>; 3],
    luma: Vec<f32>,
}

impl Planes {
    /// `picture` averaged over `factor` × `factor` blocks.
    fn of(picture: &Picture<'_>, factor: usize) -> Self {
        let (pw, ph) = (picture.width as usize, picture.height as usize);
        let (width, height) = (pw.div_ceil(factor), ph.div_ceil(factor));
        let mut rgb = [
            vec![0.0; width * height],
            vec![0.0; width * height],
            vec![0.0; width * height],
        ];
        for y in 0..height {
            for x in 0..width {
                let mut sum = [0u32; 3];
                let mut n = 0;
                for yy in y * factor..((y + 1) * factor).min(ph) {
                    for xx in x * factor..((x + 1) * factor).min(pw) {
                        let i = (yy * pw + xx) * 4;
                        for (c, s) in sum.iter_mut().enumerate() {
                            *s += u32::from(picture.rgba[i + c]);
                        }
                        n += 1;
                    }
                }
                for c in 0..3 {
                    rgb[c][y * width + x] = sum[c] as f32 / (255.0 * n as f32);
                }
            }
        }
        let luma = (0..width * height)
            .map(|k| luma_of(rgb[0][k], rgb[1][k], rgb[2][k]))
            .collect();
        Self {
            width,
            height,
            rgb,
            luma,
        }
    }
}

fn luma_of(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// One cell of the coarse grid.
#[derive(Debug, Clone, Copy)]
struct Cell {
    /// Mean sRGB-encoded colour.
    rgb: [f32; 3],
    /// Mean linear luminance.
    light: f32,
    /// Standard deviation of the luminance of its pixels.
    texture: f32,
    /// Its brightest quarter of pixels: their mean colour and linear luminance, and
    /// their mean encoded luminance. Through branches, that is the sky.
    bright: [f32; 3],
    bright_light: f32,
    bright_luma: f32,
    /// The mean colour, linear and encoded luminance of its darkest quarter.
    dark: [f32; 3],
    dark_light: f32,
    dark_luma: f32,
}

/// What a cell is to the sky.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    No,
    /// Open sky: the whole cell.
    Open,
    /// Sky between branches or leaves: its bright pixels.
    Branches,
}

impl Cell {
    /// The sky's colour and linear luminance in the cell, as `kind` sees it.
    fn tone(&self, kind: Kind) -> ([f32; 3], f32) {
        match kind {
            Kind::Branches => (self.bright, self.bright_light),
            _ => (self.rgb, self.light),
        }
    }
}

/// How green a colour is: green over the larger of red and blue.
fn green(rgb: [f32; 3]) -> f32 {
    rgb[1] - rgb[0].max(rgb[2])
}

/// How warm a colour is: red over blue.
fn warm(rgb: [f32; 3]) -> f32 {
    rgb[0] - rgb[2]
}

/// A colour without its brightness, for telling neighbours apart.
fn chroma([r, g, b]: [f32; 3]) -> [f32; 2] {
    [r - b, g - (r + b) / 2.0]
}

/// The coarse grid over `fine`, `size` × `size` fine pixels a cell.
struct Grid {
    width: usize,
    height: usize,
    size: usize,
    cells: Vec<Cell>,
}

impl Grid {
    fn of(fine: &Planes, size: usize) -> Self {
        let (width, height) = (fine.width.div_ceil(size), fine.height.div_ceil(size));
        let mut cells = Vec::with_capacity(width * height);
        let mut pixels: Vec<usize> = Vec::with_capacity(size * size);
        for y in 0..height {
            for x in 0..width {
                pixels.clear();
                for yy in y * size..((y + 1) * size).min(fine.height) {
                    pixels.extend(
                        (x * size..((x + 1) * size).min(fine.width)).map(|xx| yy * fine.width + xx),
                    );
                }
                pixels.sort_by(|&a, &b| fine.luma[a].total_cmp(&fine.luma[b]));
                let mean_of = |ks: &[usize]| {
                    let n = ks.len() as f32;
                    let rgb =
                        [0, 1, 2].map(|c| ks.iter().map(|&k| fine.rgb[c][k]).sum::<f32>() / n);
                    let luma = ks.iter().map(|&k| fine.luma[k]).sum::<f32>() / n;
                    (rgb, luma)
                };
                let light_of =
                    |rgb: [f32; 3]| luma_of(linear(rgb[0]), linear(rgb[1]), linear(rgb[2]));
                let quarter = pixels.len().div_ceil(4);
                let (rgb, mean) = mean_of(&pixels);
                let (bright, bright_luma) = mean_of(&pixels[pixels.len() - quarter..]);
                let (dark, dark_luma) = mean_of(&pixels[..quarter]);
                let squares = pixels
                    .iter()
                    .map(|&k| fine.luma[k] * fine.luma[k])
                    .sum::<f32>()
                    / pixels.len() as f32;
                cells.push(Cell {
                    rgb,
                    light: light_of(rgb),
                    texture: (squares - mean * mean).max(0.0).sqrt(),
                    bright,
                    bright_light: light_of(bright),
                    bright_luma,
                    dark,
                    dark_light: light_of(dark),
                    dark_luma,
                });
            }
        }
        Self {
            width,
            height,
            size,
            cells,
        }
    }
}

/// Whether a colour of linear luminance `light` could be sky: bright enough, and
/// neither green nor warm.
fn sky_coloured(rgb: [f32; 3], light: f32, scene_ev: Option<f32>) -> bool {
    let bright = match scene_ev {
        Some(ev) => ev + (light.max(1e-6) / 0.18).log2() >= SKY_EV,
        None => light >= SKY_LUMINANCE,
    };
    // Pink (green under red), as an overexposed sandstone wall is, isn't sky either.
    bright && green(rgb) < 0.02 && warm(rgb) < 0.04 && rgb[1] - rgb[0] > -0.04
}

/// What `cell` could be: open sky (sky-coloured and smooth), sky through branches (its
/// bright pixels sky-coloured and brighter than its dark ones, which are not blue: twigs,
/// not waves), or not sky.
fn classify(cell: &Cell, scene_ev: Option<f32>) -> Kind {
    let smooth = cell.texture <= 0.01 + 0.03 * luma_of(cell.rgb[0], cell.rgb[1], cell.rgb[2]);
    if smooth && sky_coloured(cell.rgb, cell.light, scene_ev) {
        Kind::Open
    } else if sky_coloured(cell.bright, cell.bright_light, scene_ev)
        && (cell.bright_light / cell.dark_light.max(1e-6)).log2() >= BRANCH_CONTRAST
        && cell.dark[2] - cell.dark[0] < WATER_BLUE
    {
        Kind::Branches
    } else {
        Kind::No
    }
}

/// Whether the sky can spread from cell `a` to its neighbour `b`: the sky in each
/// nearly the same colour and brightness (a third of a stop; into a branch cell, whose
/// bright quarter twigs darken, `b` may be up to [`BRANCH_STEP`] darker).
fn continues(a: ([f32; 3], f32), b: ([f32; 3], f32), into_branches: bool) -> bool {
    let stops = (b.1.max(1e-6) / a.1.max(1e-6)).log2();
    let darker = if into_branches { BRANCH_STEP } else { STEP };
    let [ca, cb] = [chroma(a.0), chroma(b.0)];
    (-darker..=STEP).contains(&stops) && (ca[0] - cb[0]).abs() + (ca[1] - cb[1]).abs() <= 0.05
}

/// The 4-connected neighbours of cell `k` in a `width` × `height` grid.
fn neighbours(k: usize, width: usize, height: usize) -> impl Iterator<Item = usize> {
    let (x, y) = (k % width, k / width);
    [
        (x > 0).then(|| k - 1),
        (x + 1 < width).then_some(k + 1),
        (y > 0).then(|| k - width),
        (y + 1 < height).then_some(k + width),
    ]
    .into_iter()
    .flatten()
}

/// What each cell is to the sky: grown from the top edge, then with the holes it
/// encloses. `None` when what grew can't be sky.
fn sky_cells(grid: &Grid, scene_ev: Option<f32>) -> Option<Vec<Kind>> {
    let (w, h) = (grid.width, grid.height);
    let cells = &grid.cells;
    let could_be: Vec<Kind> = cells.iter().map(|c| classify(c, scene_ev)).collect();
    let mut sky = vec![Kind::No; w * h];
    let mut queue: Vec<usize> = (0..w).filter(|&k| could_be[k] != Kind::No).collect();
    for &k in &queue {
        sky[k] = could_be[k];
    }
    while let Some(k) = queue.pop() {
        for n in neighbours(k, w, h) {
            if sky[n] == Kind::No
                && could_be[n] != Kind::No
                && continues(
                    cells[k].tone(sky[k]),
                    cells[n].tone(could_be[n]),
                    could_be[n] == Kind::Branches,
                )
            {
                sky[n] = could_be[n];
                queue.push(n);
            }
        }
    }
    if !plausible(grid, &sky, scene_ev.is_some()) {
        return None;
    }
    fill_holes(grid, &mut sky);
    Some(sky)
}

/// Whether what grew from the top can be sky: enough of it, and blue, or else (a grey
/// or white sky) among the brightest things in the photo; a grey wall in daylight is
/// not. Without the camera's exposure (`exposure_known`) only a blue sky is: a grey
/// one can't be told from a wall.
fn plausible(grid: &Grid, sky: &[Kind], exposure_known: bool) -> bool {
    let tones: Vec<([f32; 3], f32)> = sky
        .iter()
        .zip(&grid.cells)
        .filter(|(k, _)| **k != Kind::No)
        .map(|(k, c)| c.tone(*k))
        .collect();
    if (tones.len() as f32) < MIN_SHARE * sky.len() as f32 {
        return false;
    }
    let n = tones.len() as f32;
    let [r, _, b] = [0, 1, 2].map(|c| tones.iter().map(|t| t.0[c]).sum::<f32>() / n);
    if b - r >= BLUE {
        return true;
    }
    if !exposure_known {
        return false;
    }
    let mut lights: Vec<f32> = tones.iter().map(|t| t.1).collect();
    let median = percentile(&mut lights, 0.5);
    let mut all: Vec<f32> = grid.cells.iter().map(|c| c.light).collect();
    median >= GREY_SKY_BRIGHTNESS * percentile(&mut all, 0.97)
}

/// The `p` quantile (0..1) of `values` (reordered).
fn percentile(values: &mut [f32], p: f32) -> f32 {
    let i = ((values.len() - 1) as f32 * p).round() as usize;
    *values.select_nth_unstable_by(i, f32::total_cmp).1
}

/// Adds to `sky` the regions it encloses (not reaching the bottom of the photo, and
/// not too large) that are neither green nor warm: clouds, mostly.
fn fill_holes(grid: &Grid, sky: &mut [Kind]) {
    let (w, h) = (grid.width, grid.height);
    let mut seen = vec![false; w * h];
    let max = (MAX_HOLE_SHARE * (w * h) as f32) as usize;
    for start in 0..w * h {
        if sky[start] != Kind::No || seen[start] {
            continue;
        }
        let mut region = vec![start];
        seen[start] = true;
        let mut i = 0;
        let mut reaches_bottom = false;
        while i < region.len() {
            let k = region[i];
            i += 1;
            reaches_bottom |= k / w == h - 1;
            for n in neighbours(k, w, h) {
                if sky[n] == Kind::No && !seen[n] {
                    seen[n] = true;
                    region.push(n);
                }
            }
        }
        if reaches_bottom || region.len() > max {
            continue;
        }
        let n = region.len() as f32;
        let mean = [0, 1, 2].map(|c| region.iter().map(|&k| grid.cells[k].rgb[c]).sum::<f32>() / n);
        if green(mean) < 0.04 && warm(mean) < 0.1 {
            for k in region {
                sky[k] = Kind::Open;
            }
        }
    }
}

/// The sky cells as a mask over the fine picture: open sky blended between cell
/// centres, and in branch cells each pixel by its brightness between the cell's dark
/// and bright quarters.
fn prior(fine: &Planes, grid: &Grid, sky: &[Kind]) -> Vec<f32> {
    let size = grid.size as f32;
    let (gw, gh) = (grid.width, grid.height);
    let open = |xx: usize, yy: usize| f32::from(u8::from(sky[yy * gw + xx] == Kind::Open));
    (0..fine.width * fine.height)
        .map(|k| {
            let (x, y) = (k % fine.width, k / fine.width);
            let gx = ((x as f32 + 0.5) / size - 0.5).clamp(0.0, (gw - 1) as f32);
            let gy = ((y as f32 + 0.5) / size - 0.5).clamp(0.0, (gh - 1) as f32);
            let (x0, y0) = (gx as usize, gy as usize);
            let (x1, y1) = ((x0 + 1).min(gw - 1), (y0 + 1).min(gh - 1));
            let (tx, ty) = (gx - x0 as f32, gy - y0 as f32);
            let top = open(x0, y0) + (open(x1, y0) - open(x0, y0)) * tx;
            let bottom = open(x0, y1) + (open(x1, y1) - open(x0, y1)) * tx;
            let blended = top + (bottom - top) * ty;
            let c = (y / grid.size) * gw + x / grid.size;
            if sky[c] != Kind::Branches {
                return blended;
            }
            let cell = &grid.cells[c];
            let t = ((fine.luma[k] - cell.dark_luma)
                / (cell.bright_luma - cell.dark_luma).max(1e-3)
                - 0.3)
                / 0.5;
            let t = t.clamp(0.0, 1.0);
            blended.max(t * t * (3.0 - 2.0 * t))
        })
        .collect()
}

fn find_sky(picture: &Picture<'_>) -> Option<Coverage> {
    let long = picture.width.max(picture.height) as usize;
    let factor = ((long as f32 / FINE_EDGE as f32).round() as usize).max(1);
    let fine = Planes::of(picture, factor);
    let cell = ((fine.width.max(fine.height) as f32 / COARSE_EDGE as f32).round() as usize).max(1);
    let grid = Grid::of(&fine, cell);
    let sky = sky_cells(&grid, picture.scene_ev)?;
    // The cells' mask, its edges then moved onto the photo's.
    let prior = prior(&fine, &grid, &sky);
    let radius = (fine.width.max(fine.height) / 192).max(2);
    let refined = guided::guided_filter(&fine.luma, &prior, fine.width, fine.height, radius, 2e-3);
    let data: Vec<u8> = refined
        .iter()
        .map(|&v| {
            let t = ((v - 0.1) / 0.8).clamp(0.0, 1.0);
            (t * t * (3.0 - 2.0 * t) * 255.0).round() as u8
        })
        .collect();
    let coverage = Coverage {
        width: fine.width as u32,
        height: fine.height as u32,
        data,
    };
    (coverage.share() >= MIN_SHARE).then_some(coverage)
}

#[cfg(test)]
mod tests;
