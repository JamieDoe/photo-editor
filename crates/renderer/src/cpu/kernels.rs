use image_core::color::REC709_LUMA;

use super::lut::CurveLut;
use crate::ops::contrast;
use crate::{RenderPlan, Stage};

/// A compiled, fused CPU operation over interleaved RGB `f32` samples.
pub(super) enum Kernel {
    Gain([f32; 3]),
    Curve(Box<CurveLut>),
    Saturation(f32),
}

impl Kernel {
    pub(super) fn apply(&self, rgb: &mut [f32]) {
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
            Self::Saturation(f) => {
                let [wr, wg, wb] = REC709_LUMA;
                for px in rgb.as_chunks_mut::<3>().0 {
                    let y = px[0] * wr + px[1] * wg + px[2] * wb;
                    px[0] = (y + (px[0] - y) * f).max(0.0);
                    px[1] = (y + (px[1] - y) * f).max(0.0);
                    px[2] = (y + (px[2] - y) * f).max(0.0);
                }
            }
        }
    }
}

/// Compiles plan stages into kernels, merging consecutive channel gains.
pub(super) fn compile(plan: &RenderPlan) -> Vec<Kernel> {
    let mut out: Vec<Kernel> = Vec::with_capacity(plan.stages.len());
    for stage in &plan.stages {
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
            Stage::Saturation { factor } => out.push(Kernel::Saturation(factor)),
            Stage::WhiteBalance { .. } | Stage::Exposure { .. } => unreachable!("handled above"),
        }
    }
    out
}
