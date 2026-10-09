//! CPU render backend.
//!
//! The plan is compiled into a short list of fused kernels (consecutive gains merge,
//! per-channel curves become LUTs). Rows are processed in parallel chunks: each chunk
//! converts its source rows to `f32` in a per-thread scratch buffer, runs every kernel
//! over the scratch while it is hot in cache, and writes the display-encoded result
//! straight into the output. No full-size intermediate image is ever allocated.

mod kernels;
mod lut;

use image_core::gamut::{ON_OUTPUT, compress};
use image_core::{Cancellation, LinearImage, OutputImage, PixelFormat};
use rayon::prelude::*;

use crate::masks::{Frame, SourceFrame};
use crate::{RenderBackend, RenderError, RenderPlan};
use kernels::{Kernel, KernelScratch, RowSpan, compile, min_chunk_rows};
use lut::output_lut;

/// Target pixels per parallel work item: large enough to amortise scheduling, small
/// enough (~0.75 MB of f32 scratch) to stay in per-core cache and to give cancellation
/// a fine granularity.
const CHUNK_PIXELS: usize = 64 * 1024;

#[derive(Debug, Default, Clone, Copy)]
pub struct CpuRenderer;

impl CpuRenderer {
    /// Renders into an existing buffer (reused across interactive frames by callers
    /// that own one). `out` must have the plan's output size for this source
    /// ([`RenderPlan::output_size`]).
    pub fn render_into(
        &self,
        plan: &RenderPlan,
        source: &LinearImage,
        out: &mut OutputImage,
        cancel: &dyn Cancellation,
    ) -> Result<(), RenderError> {
        let rows = 0..out.height() as usize;
        self.render_rows_into(plan, source, rows, out, cancel)
    }

    /// A window of the plan's output for `source`: `x`, `y`, `width` x `height` in its
    /// pixels (clamped to it). Every pixel is what a whole render gives there: stages
    /// that look around a pixel read the whole framed source, and only the window's
    /// rows are worked through (ADR 0070, zoom). Running-sum blurs (clarity) start
    /// their sums at each chunk of rows, so a value may round one level apart, as it
    /// already may between whole renders split into different chunks.
    pub fn render_window(
        &self,
        plan: &RenderPlan,
        source: &LinearImage,
        format: PixelFormat,
        window: (u32, u32, u32, u32),
        cancel: &dyn Cancellation,
    ) -> Result<OutputImage, RenderError> {
        let (ow, oh) = plan.output_size(source.width(), source.height());
        let (x, y) = (
            window.0.min(ow.saturating_sub(1)),
            window.1.min(oh.saturating_sub(1)),
        );
        let (w, h) = (window.2.clamp(1, ow - x), window.3.clamp(1, oh - y));
        let backend = |e: image_core::ImageError| RenderError::Backend(e.to_string());
        let mut rows = OutputImage::new(ow, h, format).map_err(backend)?;
        self.render_rows_into(
            plan,
            source,
            y as usize..(y + h) as usize,
            &mut rows,
            cancel,
        )?;
        if w == ow {
            return Ok(rows);
        }
        let bytes = format.bytes_per_pixel();
        let (stride, from, len) = (ow as usize * bytes, x as usize * bytes, w as usize * bytes);
        let data: Vec<u8> = rows
            .data()
            .chunks_exact(stride)
            .flat_map(|row| row[from..from + len].iter().copied())
            .collect();
        OutputImage::from_raw(w, h, format, data).map_err(backend)
    }

    /// Renders `rows` of the plan's output for `source` into `out`: the output's full
    /// width, `rows.len()` tall.
    fn render_rows_into(
        &self,
        plan: &RenderPlan,
        source: &LinearImage,
        rows: std::ops::Range<usize>,
        out: &mut OutputImage,
        cancel: &dyn Cancellation,
    ) -> Result<(), RenderError> {
        // Removals (ADR 0066), then spots (ADR 0054), then red eyes (ADR 0080):
        // everything after works on the retouched source, which is cached while other
        // controls change.
        let retouched;
        let source = if plan.spots.is_empty()
            && plan.removals.is_empty()
            && plan.removal_fill.is_none()
            && plan.red_eyes.is_empty()
        {
            source
        } else {
            retouched = kernels::cached_retouch(
                source,
                &plan.removals,
                plan.removal_fill.as_ref(),
                &plan.spots,
                &plan.red_eyes,
                cancel,
            )?;
            &*retouched
        };
        let (sw, sh) = (source.width(), source.height());
        if plan.geometry.is_none() && plan.chromatic_aberration.is_none() && plan.lens.is_none() {
            return self.render_frame(plan, source, Frame::whole(sw, sh), rows, out, cancel);
        }
        // Framing first (crop, straighten, perspective, lens, chromatic aberration);
        // the stages run on the framed image, which is cached while other controls
        // change.
        let g = plan.geometry.unwrap_or_default();
        let (fw, fh) = g.oriented_size(sw as f32, sh as f32);
        let where_in_frame = Frame {
            crop: g.effective_crop(sw as f32, sh as f32),
            width: fw,
            height: fh,
            from_source: Some(SourceFrame {
                geometry: g,
                width: sw as f32,
                height: sh as f32,
                lens: plan.lens,
            }),
        };
        let framed = kernels::cached_frame(
            source,
            &g,
            plan.chromatic_aberration.as_ref(),
            plan.lens.as_ref(),
        );
        self.render_frame(plan, &framed, where_in_frame, rows, out, cancel)
    }

    /// Renders `rows` of `source`, which is `frame`'s crop of the frame masks are drawn
    /// in, into `out` (`source`'s width, `rows.len()` tall).
    fn render_frame(
        &self,
        plan: &RenderPlan,
        source: &LinearImage,
        frame: Frame,
        rows: std::ops::Range<usize>,
        out: &mut OutputImage,
        cancel: &dyn Cancellation,
    ) -> Result<(), RenderError> {
        if out.width() != source.width()
            || out.height() as usize != rows.len()
            || rows.end > source.height() as usize
        {
            return Err(RenderError::Backend(
                "output size does not match source".into(),
            ));
        }
        let kernels = compile(plan, source, frame);
        let width = source.width() as usize;
        let height = source.height() as usize;
        let format = out.format();
        let bytes = format.bytes_per_pixel();
        let output = Output {
            format,
            compress: plan.compresses_output(),
        };
        let rows_per_chunk = (CHUNK_PIXELS / width)
            .max(min_chunk_rows(&kernels))
            .min(rows.len())
            .max(1);
        let first_row = rows.start;
        let src = source.data();

        out.data_mut()
            .par_chunks_mut(rows_per_chunk * width * bytes)
            .enumerate()
            .try_for_each_init(
                || (Vec::new(), KernelScratch::default()),
                |(scratch, kernel_scratch): &mut (Vec<f32>, KernelScratch), (i, out_chunk)| {
                    if cancel.is_cancelled() {
                        return Err(RenderError::Cancelled);
                    }
                    let row = first_row + i * rows_per_chunk;
                    let first = row * width * 3;
                    let src_chunk = &src[first..first + out_chunk.len() / bytes * 3];
                    let span = RowSpan {
                        first_row: row,
                        width,
                        height,
                        source,
                    };
                    process_chunk(
                        &kernels,
                        src_chunk,
                        scratch,
                        kernel_scratch,
                        out_chunk,
                        output,
                        span,
                    );
                    Ok(())
                },
            )
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
        let (w, h) = plan.output_size(source.width(), source.height());
        let mut out =
            OutputImage::new(w, h, format).map_err(|e| RenderError::Backend(e.to_string()))?;
        self.render_into(plan, source, &mut out, cancel)?;
        Ok(out)
    }
}

/// How a render's pixels are written: their format, and whether colours past sRGB
/// are compressed first (ADR 0060; see `RenderPlan::compresses_output`).
#[derive(Clone, Copy)]
struct Output {
    format: PixelFormat,
    compress: bool,
}

fn process_chunk(
    kernels: &[Kernel],
    src: &[u16],
    scratch: &mut Vec<f32>,
    kernel_scratch: &mut KernelScratch,
    out: &mut [u8],
    output: Output,
    span: RowSpan<'_>,
) {
    let Output {
        format,
        compress: compress_output,
    } = output;
    const INV: f32 = 1.0 / 65535.0;
    scratch.clear();
    scratch.extend(src.iter().map(|&v| f32::from(v) * INV));
    for k in kernels {
        k.apply(scratch, span, kernel_scratch);
    }
    // Colours that edits pushed past sRGB are brought back smoothly rather than
    // clipped channel by channel (ADR 0060).
    if compress_output {
        for px in scratch.as_chunks_mut::<3>().0 {
            *px = compress(*px, &ON_OUTPUT);
        }
    }
    let pixels = scratch.as_chunks::<3>().0;
    if format == PixelFormat::Rgb16 {
        // 16-bit exports (ADR 0057): the sRGB curve worked out exactly, as no table
        // is finer than 16 bits everywhere (the curve is steepest near black).
        for (px, o) in pixels.iter().zip(out.as_chunks_mut::<6>().0) {
            for (c, v) in px.iter().enumerate() {
                let code =
                    (image_core::color::linear_to_srgb(v.clamp(0.0, 1.0)) * 65535.0).round() as u16;
                o[c * 2..c * 2 + 2].copy_from_slice(&code.to_ne_bytes());
            }
        }
        return;
    }
    let lut = output_lut();
    if format.channels() == 4 {
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
