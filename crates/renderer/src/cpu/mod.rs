//! CPU render backend.
//!
//! The plan is compiled into a short list of fused kernels (consecutive gains merge,
//! per-channel curves become LUTs). Rows are processed in parallel chunks: each chunk
//! converts its source rows to `f32` in a per-thread scratch buffer, runs every kernel
//! over the scratch while it is hot in cache, and writes the display-encoded result
//! straight into the output. No full-size intermediate image is ever allocated.

mod kernels;
mod lut;

use image_core::{Cancellation, LinearImage, OutputImage, PixelFormat};
use rayon::prelude::*;

use crate::{RenderBackend, RenderError, RenderPlan};
use kernels::{Kernel, compile};
use lut::output_lut;

/// Target pixels per parallel work item: large enough to amortise scheduling, small
/// enough (~0.75 MB of f32 scratch) to stay in per-core cache and to give cancellation
/// a fine granularity.
const CHUNK_PIXELS: usize = 64 * 1024;

#[derive(Debug, Default, Clone, Copy)]
pub struct CpuRenderer;

impl CpuRenderer {
    /// Renders into an existing buffer (reused across interactive frames by callers
    /// that own one). `out` must match the source dimensions.
    pub fn render_into(
        &self,
        plan: &RenderPlan,
        source: &LinearImage,
        out: &mut OutputImage,
        cancel: &dyn Cancellation,
    ) -> Result<(), RenderError> {
        if (out.width(), out.height()) != (source.width(), source.height()) {
            return Err(RenderError::Backend(
                "output size does not match source".into(),
            ));
        }
        let kernels = compile(plan);
        let width = source.width() as usize;
        let channels = out.format().channels();
        let rows_per_chunk = (CHUNK_PIXELS / width).max(1);
        let src = source.data();

        out.data_mut()
            .par_chunks_mut(rows_per_chunk * width * channels)
            .enumerate()
            .try_for_each_init(Vec::new, |scratch: &mut Vec<f32>, (i, out_chunk)| {
                if cancel.is_cancelled() {
                    return Err(RenderError::Cancelled);
                }
                let first = i * rows_per_chunk * width * 3;
                let src_chunk = &src[first..first + out_chunk.len() / channels * 3];
                process_chunk(&kernels, src_chunk, scratch, out_chunk, channels);
                Ok(())
            })
    }
}

impl RenderBackend for CpuRenderer {
    fn name(&self) -> &'static str {
        "cpu"
    }

    fn render(
        &self,
        plan: &RenderPlan,
        source: &LinearImage,
        format: PixelFormat,
        cancel: &dyn Cancellation,
    ) -> Result<OutputImage, RenderError> {
        let mut out = OutputImage::new(source.width(), source.height(), format)
            .map_err(|e| RenderError::Backend(e.to_string()))?;
        self.render_into(plan, source, &mut out, cancel)?;
        Ok(out)
    }
}

fn process_chunk(
    kernels: &[Kernel],
    src: &[u16],
    scratch: &mut Vec<f32>,
    out: &mut [u8],
    channels: usize,
) {
    const INV: f32 = 1.0 / 65535.0;
    scratch.clear();
    scratch.extend(src.iter().map(|&v| f32::from(v) * INV));
    for k in kernels {
        k.apply(scratch);
    }
    let lut = output_lut();
    let pixels = scratch.as_chunks::<3>().0;
    if channels == 4 {
        for (px, o) in pixels.iter().zip(out.as_chunks_mut::<4>().0) {
            o[0] = lut.encode(px[0]);
            o[1] = lut.encode(px[1]);
            o[2] = lut.encode(px[2]);
            o[3] = u8::MAX;
        }
    } else {
        for (px, o) in pixels.iter().zip(out.as_chunks_mut::<3>().0) {
            o[0] = lut.encode(px[0]);
            o[1] = lut.encode(px[1]);
            o[2] = lut.encode(px[2]);
        }
    }
}

#[cfg(test)]
mod tests;
