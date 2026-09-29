use image_core::LinearImage;
use image_core::color::REC709_LUMA;

use super::lut::CurveLut;
use crate::ops::colour_mixer::{self, MixerTable};
use crate::ops::tone::{self, ToneBase, ToneParams};
use crate::ops::{contrast, look, vibrance};
use crate::{RenderPlan, Stage};

/// Where a chunk of pixels sits in the image (needed by stages that look at
/// neighbourhoods).
#[derive(Debug, Clone, Copy)]
pub(super) struct RowSpan {
    pub first_row: usize,
    pub width: usize,
    pub height: usize,
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

/// The most recent surroundings map. While a slider other than white balance or
/// exposure is dragged, every frame needs the same map, so it is built once. The key
/// is the source buffer (address, size, and a fingerprint of sampled pixels, so a new
/// image at a reused address is never mistaken for the old one) and the gains before
/// the tone stage. One entry, so memory stays bounded (a 256-px map is ~0.5 MB).
fn cached_base(source: &LinearImage, gains: [f32; 3]) -> std::sync::Arc<ToneBase> {
    use std::sync::{Arc, Mutex};
    type Key = (usize, usize, u32, u32, [u32; 3], u64);
    static LAST: Mutex<Option<(Key, Arc<ToneBase>)>> = Mutex::new(None);

    let data = source.data();
    let mut fingerprint: u64 = 0xcbf2_9ce4_8422_2325;
    for v in data.iter().step_by(997) {
        fingerprint = (fingerprint ^ u64::from(*v)).wrapping_mul(0x0100_0000_01b3);
    }
    let key: Key = (
        data.as_ptr() as usize,
        data.len(),
        source.width(),
        source.height(),
        gains.map(f32::to_bits),
        fingerprint,
    );
    if let Ok(last) = LAST.lock()
        && let Some((k, base)) = last.as_ref()
        && *k == key
    {
        return Arc::clone(base);
    }
    let base = Arc::new(ToneBase::build(source, gains));
    if let Ok(mut last) = LAST.lock() {
        *last = Some((key, Arc::clone(&base)));
    }
    base
}

impl ToneKernel {
    fn new(source: &LinearImage, gains: [f32; 3], params: ToneParams) -> Self {
        let base = params.is_local().then(|| {
            let base = cached_base(source, gains);
            let cols = base.columns(source.width() as usize);
            (base, cols)
        });
        Self {
            base,
            local: StopsLut::build(|d| tone::local_stops(d, &params)),
            endpoints: StopsLut::build(|d| tone::endpoint_stops(d, &params)),
        }
    }

    fn apply(&self, rgb: &mut [f32], span: RowSpan) {
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
    pub(super) fn apply(&self, rgb: &mut [f32], span: RowSpan) {
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

/// Compiles plan stages into kernels, merging consecutive channel gains. Stages that
/// need a view of the whole image (tone) prepare it here, from `source` and the gains
/// before them.
pub(super) fn compile(plan: &RenderPlan, source: &LinearImage) -> Vec<Kernel> {
    let mut out: Vec<Kernel> = Vec::with_capacity(plan.stages.len());
    let mut gains_so_far = [1.0f32; 3];
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
            Stage::Tone { params } => out.push(Kernel::Tone(Box::new(ToneKernel::new(
                source,
                gains_so_far,
                params,
            )))),
            Stage::Saturation { factor } => out.push(Kernel::Saturation(factor)),
            Stage::Vibrance { amount } => out.push(Kernel::Vibrance(amount)),
            Stage::ColourMixer { bands } => {
                out.push(Kernel::Mixer(Box::new(MixerTable::new(&bands))))
            }
            Stage::WhiteBalance { .. } | Stage::Exposure { .. } => unreachable!("handled above"),
        }
    }
    out
}
