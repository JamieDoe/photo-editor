use std::sync::{Arc, Mutex};

use image_core::LinearImage;
use image_core::color::REC709_LUMA;

use super::lut::CurveLut;
use crate::ops::colour_mixer::{self, MixerTable};
use crate::ops::dehaze::DehazeModel;
use crate::ops::detail::{self, DetailParams};
use crate::ops::scene::{self, RowModel, SceneMap};
use crate::ops::tone::{self, ToneBase, ToneParams};
use crate::ops::{contrast, look, vibrance};
use crate::{RenderPlan, Stage};

/// Where a chunk of pixels sits in the image (needed by stages that look at
/// neighbourhoods).
#[derive(Debug, Clone, Copy)]
pub(super) struct RowSpan<'a> {
    pub first_row: usize,
    pub width: usize,
    pub height: usize,
    /// The whole source, for stages that read rows outside the chunk.
    pub source: &'a LinearImage,
}

/// A compiled, fused CPU operation over interleaved RGB `f32` samples.
pub(super) enum Kernel {
    Gain([f32; 3]),
    Curve(Box<CurveLut>),
    Saturation(f32),
    Vibrance(f32),
    Mixer(Box<MixerTable>),
    /// Highlights/shadows (with the surroundings map) and whites/blacks, with the
    /// gains as lookup tables over "stops below white".
    Tone(Box<ToneKernel>),
    /// Texture and clarity: reads source rows around the chunk.
    Detail(Box<DetailKernel>),
    Dehaze(Box<DehazeKernel>),
}

/// The surroundings map with each pixel column's map columns and blend weight.
type BaseWithColumns = (std::sync::Arc<ToneBase>, Vec<(u32, u32, f32)>);

pub(super) struct ToneKernel {
    base: Option<BaseWithColumns>,
    local: StopsLut,
    endpoints: StopsLut,
}

/// `2^f(d)` for d in stops below white, tabulated every 1/64 stop over 0..24.
struct StopsLut {
    table: Vec<f32>,
}

impl StopsLut {
    const STEPS_PER_STOP: f32 = 64.0;
    const MAX_STOPS: f32 = 24.0;

    fn build(f: impl Fn(f32) -> f32) -> Self {
        let n = (Self::MAX_STOPS * Self::STEPS_PER_STOP) as usize;
        Self {
            table: (0..=n)
                .map(|i| f(i as f32 / Self::STEPS_PER_STOP).exp2())
                .collect(),
        }
    }

    #[inline]
    fn eval(&self, d: f32) -> f32 {
        let pos = d.clamp(0.0, Self::MAX_STOPS - 1e-3) * Self::STEPS_PER_STOP;
        let i = pos as usize;
        let t = pos - i as f32;
        self.table[i] + (self.table[i + 1] - self.table[i]) * t
    }
}

/// Identifies the source buffer: address, size, and a fingerprint of sampled pixels,
/// so a new image at a reused address is never mistaken for the old one.
type SourceKey = (usize, usize, u32, u32, u64);

fn source_key(source: &LinearImage) -> SourceKey {
    let data = source.data();
    let mut fingerprint: u64 = 0xcbf2_9ce4_8422_2325;
    for v in data.iter().step_by(997) {
        fingerprint = (fingerprint ^ u64::from(*v)).wrapping_mul(0x0100_0000_01b3);
    }
    (
        data.as_ptr() as usize,
        data.len(),
        source.width(),
        source.height(),
        fingerprint,
    )
}

type GainBits = [u32; 3];
/// A cached map and what it was built for.
type Keyed<K, T> = Option<(K, Arc<T>)>;

/// The most recent source's scene map and the maps derived from it, so dragging a
/// slider reuses them:
///
/// - the map without gains, read from the full image once per image;
/// - that map after the current white balance and exposure (a cheap rescale);
/// - the dehaze model and the surroundings map, for those gains and dehaze amount.
///
/// One source, so memory stays bounded (a 256-px map is well under 1 MB).
#[derive(Default)]
struct MapCache {
    key: Option<SourceKey>,
    unit: Option<Arc<SceneMap>>,
    scaled: Keyed<GainBits, SceneMap>,
    dehaze: Keyed<(GainBits, u32), DehazeModel>,
    tone: Keyed<(GainBits, Option<u32>), ToneBase>,
}

static MAPS: Mutex<Option<MapCache>> = Mutex::new(None);

/// Runs `f` on the cache entry for `key`, replacing an entry for another source.
fn with_maps<R>(key: SourceKey, f: impl FnOnce(&mut MapCache) -> R) -> R {
    let mut guard = MAPS.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(MapCache::default);
    if cache.key != Some(key) {
        *cache = MapCache {
            key: Some(key),
            ..MapCache::default()
        };
    }
    f(cache)
}

/// Looks up `get` in the cache, or builds with `build` (outside the lock, so other
/// renders are not held up) and stores with `put`.
fn cached<T>(
    key: SourceKey,
    get: impl FnOnce(&MapCache) -> Option<Arc<T>>,
    build: impl FnOnce() -> T,
    put: impl FnOnce(&mut MapCache, Arc<T>),
) -> Arc<T> {
    if let Some(hit) = with_maps(key, |c| get(c)) {
        return hit;
    }
    let value = Arc::new(build());
    with_maps(key, |c| put(c, Arc::clone(&value)));
    value
}

fn cached_scene(source: &LinearImage, gains: [f32; 3]) -> (SourceKey, Arc<SceneMap>) {
    let key = source_key(source);
    let bits = gains.map(f32::to_bits);
    let scene = cached(
        key,
        |c| {
            c.scaled
                .as_ref()
                .filter(|(b, _)| *b == bits)
                .map(|(_, m)| Arc::clone(m))
        },
        || {
            let unit = cached(
                key,
                |c| c.unit.clone(),
                || SceneMap::unit(source),
                |c, m| c.unit = Some(m),
            );
            unit.scaled(gains)
        },
        |c, m| c.scaled = Some((bits, m)),
    );
    (key, scene)
}

fn cached_dehaze(source: &LinearImage, gains: [f32; 3], amount: f32) -> Arc<DehazeModel> {
    let (key, scene) = cached_scene(source, gains);
    let bits = (gains.map(f32::to_bits), amount.to_bits());
    cached(
        key,
        |c| {
            c.dehaze
                .as_ref()
                .filter(|(b, _)| *b == bits)
                .map(|(_, m)| Arc::clone(m))
        },
        || DehazeModel::build(&scene, amount),
        |c, m| c.dehaze = Some((bits, m)),
    )
}

/// The surroundings map for the tone and detail stages, measured after `dehaze`.
fn cached_base(
    source: &LinearImage,
    gains: [f32; 3],
    dehaze: Option<(f32, &DehazeModel)>,
) -> Arc<ToneBase> {
    let (key, scene) = cached_scene(source, gains);
    let bits = (
        gains.map(f32::to_bits),
        dehaze.map(|(amount, _)| amount.to_bits()),
    );
    cached(
        key,
        |c| {
            c.tone
                .as_ref()
                .filter(|(b, _)| *b == bits)
                .map(|(_, m)| Arc::clone(m))
        },
        || ToneBase::from_scene(&scene, dehaze.map(|(_, m)| m)),
        |c, m| c.tone = Some((bits, m)),
    )
}

/// Dehaze runs on the pixels as they are after white balance and exposure.
pub(super) struct DehazeKernel {
    model: Arc<DehazeModel>,
    columns: Vec<(u32, u32, f32)>,
}

thread_local! {
    static ROW_MODEL: std::cell::RefCell<RowModel> = std::cell::RefCell::default();
}

impl DehazeKernel {
    fn apply(&self, rgb: &mut [f32], span: RowSpan<'_>) {
        ROW_MODEL.with_borrow_mut(|rm| {
            for (r, row) in rgb.chunks_mut(span.width * 3).enumerate() {
                rm.load(self.model.map(), span.first_row + r, span.height);
                for (x, px) in row.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                    let t = rm.eval(self.columns[x], self.model.guide(scene::luma(*px)));
                    *px = self.model.apply(*px, t);
                }
            }
        });
    }
}

/// A dehaze stage before a neighbourhood stage: its amount (the cache key) and model.
#[derive(Clone)]
struct DehazeBefore {
    amount: f32,
    model: Arc<DehazeModel>,
    columns: Arc<Vec<(u32, u32, f32)>>,
}

pub(super) struct DetailKernel {
    params: DetailParams,
    /// Gains before the stage: the neighbourhoods are measured on the source after
    /// them (as the tone stage's map is).
    gains: [f32; 3],
    radius: usize,
    /// Rows either side the stage reads (see `DetailParams::reach`).
    reach: usize,
    base: Option<BaseWithColumns>,
    /// Dehaze before this stage: the luminance is measured after it.
    dehaze: Option<DehazeBefore>,
    exp2: SignedStopsLut,
}

/// Per-thread buffers for the detail stage's luminance planes.
#[derive(Default)]
struct DetailScratch {
    log_y: Vec<f32>,
    small: Vec<f32>,
    sharp_row: Vec<f32>,
    vertical: Vec<f32>,
    blur: detail::BlurScratch,
    row_model: RowModel,
    a_row: Vec<f32>,
    b_row: Vec<f32>,
}

thread_local! {
    static DETAIL_SCRATCH: std::cell::RefCell<DetailScratch> = std::cell::RefCell::default();
}

/// `2^s` for s in -MAX..MAX stops, every 1/256 stop (the detail gains).
struct SignedStopsLut {
    table: Vec<f32>,
}

impl SignedStopsLut {
    const STEPS_PER_STOP: f32 = 256.0;
    const MAX: f32 = detail::MAX_STOPS;

    fn new() -> Self {
        let n = (2.0 * Self::MAX * Self::STEPS_PER_STOP) as usize;
        Self {
            table: (0..=n)
                .map(|i| (i as f32 / Self::STEPS_PER_STOP - Self::MAX).exp2())
                .collect(),
        }
    }

    #[inline]
    fn eval(&self, s: f32) -> f32 {
        let pos = (s.clamp(-Self::MAX, Self::MAX - 1e-4) + Self::MAX) * Self::STEPS_PER_STOP;
        let i = pos as usize;
        let t = pos - i as f32;
        self.table[i] + (self.table[i + 1] - self.table[i]) * t
    }
}

impl DetailKernel {
    /// Rows a chunk should have so the band it measures (the chunk plus the blur's
    /// reach either side) is mostly the chunk itself.
    pub(super) fn min_chunk_rows(&self) -> usize {
        4 * self.reach
    }

    fn new(
        source: &LinearImage,
        gains: [f32; 3],
        params: DetailParams,
        dehaze: Option<DehazeBefore>,
    ) -> Self {
        let base = params.needs_base().then(|| {
            let base = cached_base(
                source,
                gains,
                dehaze.as_ref().map(|d| (d.amount, &*d.model)),
            );
            let cols = base.columns(source.width() as usize);
            (base, cols)
        });
        let (w, h) = (source.width() as usize, source.height() as usize);
        Self {
            params,
            gains,
            radius: detail::radius_for(w, h),
            reach: params.reach(w, h),
            base,
            dehaze,
            exp2: SignedStopsLut::new(),
        }
    }

    fn apply(&self, rgb: &mut [f32], span: RowSpan<'_>) {
        let w = span.width;
        let rows = rgb.len() / (w * 3);
        // Measure on a band reaching as far beyond the chunk as the blurs do, so
        // every chunk sees exactly what a whole-image blur would.
        let top = span.first_row.saturating_sub(self.reach);
        let bottom = (span.first_row + rows + self.reach).min(span.height);
        let band = bottom - top;
        DETAIL_SCRATCH.with_borrow_mut(|s| {
            s.log_y.resize(band * w, 0.0);
            for (y, out) in (top..bottom).zip(s.log_y.chunks_mut(w)) {
                let src = span.source.row(y as u32);
                match &self.dehaze {
                    None => detail::log_luminance_row(src, self.gains, out),
                    Some(d) => {
                        s.row_model.load(d.model.map(), y, span.height);
                        detail::dehazed_log_luminance_row(
                            src,
                            self.gains,
                            &d.model,
                            &s.row_model,
                            &d.columns,
                            out,
                        );
                    }
                }
            }
            let small_blur = self.params.uses_small_blur();
            if small_blur {
                s.small.clear();
                s.small.extend_from_slice(&s.log_y);
                detail::blur_plane(&mut s.small, w, band, self.radius, &mut s.blur);
            }
            let sharpen = self.params.sharpening != 0.0;
            s.sharp_row.resize(w, 0.0);

            for (r, row) in rgb.chunks_mut(w * 3).enumerate() {
                let y = span.first_row + r;
                let offset = (y - top) * w;
                if let Some((base, _)) = &self.base {
                    base.row(y, span.height, &mut s.a_row, &mut s.b_row);
                }
                if sharpen {
                    // Neighbouring rows are in the band (or clamped at the image's edges).
                    let band_row = |by: usize| &s.log_y[by * w..(by + 1) * w];
                    let by = y - top;
                    let (above, below) = (by.saturating_sub(1), (by + 1).min(band - 1));
                    detail::sharpen_blur_row(
                        band_row(above),
                        band_row(by),
                        band_row(below),
                        &mut s.vertical,
                        &mut s.sharp_row,
                    );
                }
                for (x, px) in row.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                    let log_y = s.log_y[offset + x];
                    // Unused blurs read as the pixel itself, which zeroes their terms.
                    let small = if small_blur {
                        s.small[offset + x]
                    } else {
                        log_y
                    };
                    let sharp = if sharpen { s.sharp_row[x] } else { log_y };
                    let base_log = match &self.base {
                        Some((_, cols)) => {
                            let (x0, x1, t) = cols[x];
                            let (x0, x1) = (x0 as usize, x1 as usize);
                            let a = s.a_row[x0] + (s.a_row[x1] - s.a_row[x0]) * t;
                            let b = s.b_row[x0] + (s.b_row[x1] - s.b_row[x0]) * t;
                            a * small + b
                        }
                        None => small,
                    };
                    let stops = if small_blur {
                        detail::gain_stops(&self.params, log_y, small, base_log, sharp)
                    } else {
                        detail::sharpen_stops(&self.params, log_y, sharp)
                    };
                    let gain = self.exp2.eval(stops);
                    px[0] *= gain;
                    px[1] *= gain;
                    px[2] *= gain;
                }
            }
        });
    }
}

impl ToneKernel {
    fn new(
        source: &LinearImage,
        gains: [f32; 3],
        params: ToneParams,
        dehaze: Option<&DehazeBefore>,
    ) -> Self {
        let base = params.is_local().then(|| {
            let base = cached_base(source, gains, dehaze.map(|d| (d.amount, &*d.model)));
            let cols = base.columns(source.width() as usize);
            (base, cols)
        });
        Self {
            base,
            local: StopsLut::build(|d| tone::local_stops(d, &params)),
            endpoints: StopsLut::build(|d| tone::endpoint_stops(d, &params)),
        }
    }

    fn apply(&self, rgb: &mut [f32], span: RowSpan<'_>) {
        let [wr, wg, wb] = REC709_LUMA;
        let (mut a_row, mut b_row) = (Vec::new(), Vec::new());
        for (r, row) in rgb.chunks_mut(span.width * 3).enumerate() {
            if let Some((base, _)) = &self.base {
                base.row(span.first_row + r, span.height, &mut a_row, &mut b_row);
            }
            for (x, px) in row.as_chunks_mut::<3>().0.iter_mut().enumerate() {
                let log_y = (px[0] * wr + px[1] * wg + px[2] * wb).max(1.0e-6).log2();
                let mut gain = self.endpoints.eval(-log_y);
                if let Some((_, cols)) = &self.base {
                    let (x0, x1, t) = cols[x];
                    let (x0, x1) = (x0 as usize, x1 as usize);
                    let a = a_row[x0] + (a_row[x1] - a_row[x0]) * t;
                    let b = b_row[x0] + (b_row[x1] - b_row[x0]) * t;
                    gain *= self.local.eval(-(a * log_y + b));
                }
                px[0] *= gain;
                px[1] *= gain;
                px[2] *= gain;
            }
        }
    }
}

impl Kernel {
    pub(super) fn apply(&self, rgb: &mut [f32], span: RowSpan<'_>) {
        match self {
            Self::Gain(g) => {
                for px in rgb.as_chunks_mut::<3>().0 {
                    px[0] *= g[0];
                    px[1] *= g[1];
                    px[2] *= g[2];
                }
            }
            Self::Curve(lut) => {
                for v in rgb.iter_mut() {
                    *v = lut.eval(*v);
                }
            }
            Self::Tone(k) => k.apply(rgb, span),
            Self::Detail(k) => k.apply(rgb, span),
            Self::Dehaze(k) => k.apply(rgb, span),
            Self::Saturation(f) => {
                let [wr, wg, wb] = REC709_LUMA;
                for px in rgb.as_chunks_mut::<3>().0 {
                    let y = px[0] * wr + px[1] * wg + px[2] * wb;
                    px[0] = (y + (px[0] - y) * f).max(0.0);
                    px[1] = (y + (px[1] - y) * f).max(0.0);
                    px[2] = (y + (px[2] - y) * f).max(0.0);
                }
            }
            Self::Mixer(table) => {
                for px in rgb.as_chunks_mut::<3>().0 {
                    *px = colour_mixer::apply(*px, table);
                }
            }
            Self::Vibrance(amount) => {
                for px in rgb.as_chunks_mut::<3>().0 {
                    *px = vibrance::apply(*px, *amount);
                }
            }
        }
    }
}

/// The fewest rows per chunk the kernels want (stages that read neighbouring rows
/// are cheaper on taller chunks).
pub(super) fn min_chunk_rows(kernels: &[Kernel]) -> usize {
    kernels
        .iter()
        .map(|k| match k {
            Kernel::Detail(d) => d.min_chunk_rows(),
            _ => 1,
        })
        .max()
        .unwrap_or(1)
}

/// Compiles plan stages into kernels, merging consecutive channel gains. Stages that
/// need a view of the whole image (tone) prepare it here, from `source` and the gains
/// before them.
pub(super) fn compile(plan: &RenderPlan, source: &LinearImage) -> Vec<Kernel> {
    let mut out: Vec<Kernel> = Vec::with_capacity(plan.stages.len());
    let mut gains_so_far = [1.0f32; 3];
    let mut dehaze_so_far: Option<DehazeBefore> = None;
    for stage in &plan.stages {
        match *stage {
            Stage::WhiteBalance { gains } => (0..3).for_each(|c| gains_so_far[c] *= gains[c]),
            Stage::Exposure { multiplier } => {
                gains_so_far.iter_mut().for_each(|g| *g *= multiplier)
            }
            _ => {}
        }
        let gain = match *stage {
            Stage::WhiteBalance { gains } => Some(gains),
            Stage::Exposure { multiplier } => Some([multiplier; 3]),
            _ => None,
        };
        if let Some(g) = gain {
            if let Some(Kernel::Gain(prev)) = out.last_mut() {
                for c in 0..3 {
                    prev[c] *= g[c];
                }
            } else {
                out.push(Kernel::Gain(g));
            }
            continue;
        }
        match *stage {
            Stage::Contrast { gamma } => {
                out.push(Kernel::Curve(Box::new(CurveLut::build(|x| {
                    contrast::apply(x, gamma)
                }))));
            }
            Stage::BaseCurve => out.push(Kernel::Curve(Box::new(CurveLut::build(look::standard)))),
            Stage::Dehaze { amount } => {
                let model = cached_dehaze(source, gains_so_far, amount);
                let columns = model.map().columns(source.width() as usize);
                out.push(Kernel::Dehaze(Box::new(DehazeKernel {
                    model: Arc::clone(&model),
                    columns: columns.clone(),
                })));
                dehaze_so_far = Some(DehazeBefore {
                    amount,
                    model,
                    columns: Arc::new(columns),
                });
            }
            Stage::Tone { params } => out.push(Kernel::Tone(Box::new(ToneKernel::new(
                source,
                gains_so_far,
                params,
                dehaze_so_far.as_ref(),
            )))),
            Stage::Saturation { factor } => out.push(Kernel::Saturation(factor)),
            Stage::Vibrance { amount } => out.push(Kernel::Vibrance(amount)),
            Stage::Detail { params } => out.push(Kernel::Detail(Box::new(DetailKernel::new(
                source,
                gains_so_far,
                params,
                dehaze_so_far.clone(),
            )))),
            Stage::ColourMixer { bands } => {
                out.push(Kernel::Mixer(Box::new(MixerTable::new(&bands))))
            }
            Stage::WhiteBalance { .. } | Stage::Exposure { .. } => unreachable!("handled above"),
        }
    }
    out
}
