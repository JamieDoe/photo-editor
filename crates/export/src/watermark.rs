//! Watermarks (ADR 0069): a line of text, such as "© 2026 Jamie", in a corner of an
//! export.
//!
//! The text is drawn by the UI in its own typeface (white with a soft dark shadow, so it
//! reads on any photo) and handed over once per export as an RGBA PNG. Here it is
//! scaled to each photo, its height a share of the photo's short edge, and blended in
//! at a fixed opacity, after resizing and sharpening, so it is crisp at the size
//! written.

use image_core::{OutputImage, PixelFormat};
use rayon::prelude::*;

use crate::ExportError;

/// Where the text sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
    Centre,
}

/// How large the text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}

impl Size {
    /// The text's height (the drawing's, shadow included) as a share of the photo's
    /// short edge.
    fn share(self) -> f32 {
        match self {
            Self::Small => 0.035,
            Self::Medium => 0.055,
            Self::Large => 0.085,
        }
    }
}

/// The gap from the photo's edges, as a share of its short edge.
const MARGIN: f32 = 0.03;
/// How strongly the drawing is laid over the photo.
pub const OPACITY: f32 = 0.7;

/// A watermark, ready to lay on exports.
#[derive(Debug, Clone, PartialEq)]
pub struct Watermark {
    /// The drawing: straight (not premultiplied) RGBA, 8-bit.
    rgba: Vec<u8>,
    width: usize,
    height: usize,
    pub position: Position,
    pub size: Size,
}

impl Watermark {
    /// From the UI's drawing, an RGBA (or grey-alpha) PNG.
    pub fn from_png(png: &[u8], position: Position, size: Size) -> Result<Self, ExportError> {
        let err = |e: png::DecodingError| ExportError::Encode(format!("watermark: {e}"));
        let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(err)?;
        let mut buf = vec![
            0;
            reader
                .output_buffer_size()
                .ok_or_else(|| ExportError::Encode("watermark: too large".into()))?
        ];
        let info = reader.next_frame(&mut buf).map_err(err)?;
        let (w, h) = (info.width as usize, info.height as usize);
        if w == 0 || h == 0 || w > 8192 || h > 8192 {
            return Err(ExportError::Encode("watermark: unusable size".into()));
        }
        let channels = info.color_type.samples();
        let rgba: Vec<u8> = buf[..info.buffer_size()]
            .chunks_exact(channels)
            .flat_map(|p| match channels {
                4 => [p[0], p[1], p[2], p[3]],
                3 => [p[0], p[1], p[2], 255],
                2 => [p[0], p[0], p[0], p[1]],
                _ => [p[0], p[0], p[0], 255],
            })
            .collect();
        Ok(Self {
            rgba,
            width: w,
            height: h,
            position,
            size,
        })
    }

    /// Where the drawing goes on a `w` x `h` photo: its top-left corner and its scale.
    fn placement(&self, w: usize, h: usize) -> (f32, f32, f32) {
        let short = w.min(h) as f32;
        let scale = (self.size.share() * short / self.height as f32).max(1e-3);
        // Never wider than the photo less its margins.
        let margin = MARGIN * short;
        let scale = scale.min((w as f32 - 2.0 * margin).max(1.0) / self.width as f32);
        let (dw, dh) = (self.width as f32 * scale, self.height as f32 * scale);
        let (left, right) = (margin, w as f32 - margin - dw);
        let (top, bottom) = (margin, h as f32 - margin - dh);
        let (x, y) = match self.position {
            Position::TopLeft => (left, top),
            Position::TopRight => (right, top),
            Position::BottomLeft => (left, bottom),
            Position::BottomRight => (right, bottom),
            Position::Centre => ((w as f32 - dw) / 2.0, (h as f32 - dh) / 2.0),
        };
        (x, y, scale)
    }

    /// The drawing's colour and coverage at its point (`u`, `v`), in its pixels,
    /// bilinear; transparent outside.
    fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        let (u, v) = (u - 0.5, v - 0.5);
        let (x0, y0) = (u.floor(), v.floor());
        let (tx, ty) = (u - x0, v - y0);
        let at = |x: f32, y: f32| -> [f32; 4] {
            if x < 0.0 || y < 0.0 || x >= self.width as f32 || y >= self.height as f32 {
                return [0.0; 4];
            }
            let i = (y as usize * self.width + x as usize) * 4;
            let a = f32::from(self.rgba[i + 3]) / 255.0;
            // Premultiplied, so transparent pixels' colour doesn't bleed in.
            [
                f32::from(self.rgba[i]) / 255.0 * a,
                f32::from(self.rgba[i + 1]) / 255.0 * a,
                f32::from(self.rgba[i + 2]) / 255.0 * a,
                a,
            ]
        };
        let (p00, p10, p01, p11) = (
            at(x0, y0),
            at(x0 + 1.0, y0),
            at(x0, y0 + 1.0),
            at(x0 + 1.0, y0 + 1.0),
        );
        std::array::from_fn(|c| {
            let top = p00[c] + (p10[c] - p00[c]) * tx;
            let bottom = p01[c] + (p11[c] - p01[c]) * tx;
            top + (bottom - top) * ty
        })
    }
}

/// `image` with `watermark` laid over it. The image's samples are blended as written
/// (display-encoded), as the drawing's are.
pub fn apply(image: &OutputImage, watermark: &Watermark) -> OutputImage {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let (x0, y0, scale) = watermark.placement(w, h);
    let (x1, y1) = (
        x0 + watermark.width as f32 * scale,
        y0 + watermark.height as f32 * scale,
    );
    let rows = (y0.floor().max(0.0) as usize)..(y1.ceil().min(h as f32) as usize);
    let cols = (x0.floor().max(0.0) as usize)..(x1.ceil().min(w as f32) as usize);
    let format = image.format();
    let channels = format.channels();
    // Each pixel's cover: the drawing averaged over the pixel (2x2 samples), so a
    // drawing scaled down stays smooth.
    let blend = |x: usize, y: usize, under: [f32; 3]| -> [f32; 3] {
        let mut sum = [0.0f32; 4];
        for (dx, dy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
            let s = watermark.sample((x as f32 + dx - x0) / scale, (y as f32 + dy - y0) / scale);
            for c in 0..4 {
                sum[c] += s[c] / 4.0;
            }
        }
        let a = sum[3] * OPACITY;
        [0, 1, 2].map(|c| sum[c] * OPACITY + under[c] * (1.0 - a))
    };
    let mut out = image.clone();
    match format {
        PixelFormat::Rgb8 | PixelFormat::Rgba8 => {
            let stride = w * channels;
            let data = out.data_mut();
            data.par_chunks_mut(stride)
                .enumerate()
                .filter(|(y, _)| rows.contains(y))
                .for_each(|(y, row)| {
                    for x in cols.clone() {
                        let p = &mut row[x * channels..x * channels + 3];
                        let under = [0, 1, 2].map(|c| f32::from(p[c]) / 255.0);
                        let v = blend(x, y, under);
                        for c in 0..3 {
                            p[c] = (v[c] * 255.0).round().clamp(0.0, 255.0) as u8;
                        }
                    }
                });
        }
        PixelFormat::Rgb16 => {
            let stride = w * 6;
            let data = out.data_mut();
            data.par_chunks_mut(stride)
                .enumerate()
                .filter(|(y, _)| rows.contains(y))
                .for_each(|(y, row)| {
                    for x in cols.clone() {
                        let p = &mut row[x * 6..x * 6 + 6];
                        let under = [0, 1, 2].map(|c| {
                            f32::from(u16::from_ne_bytes([p[2 * c], p[2 * c + 1]])) / 65535.0
                        });
                        let v = blend(x, y, under);
                        for c in 0..3 {
                            let s = (v[c] * 65535.0).round().clamp(0.0, 65535.0) as u16;
                            p[2 * c..2 * c + 2].copy_from_slice(&s.to_ne_bytes());
                        }
                    }
                });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A solid white drawing, `w` x `h`, as a PNG.
    fn drawing(w: u32, h: u32, alpha: u8) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let mut writer = e.write_header().unwrap();
        writer
            .write_image_data(&[255, 255, 255, alpha].repeat((w * h) as usize))
            .unwrap();
        writer.finish().unwrap();
        out
    }

    fn grey(w: u32, h: u32, format: PixelFormat) -> OutputImage {
        let data = match format {
            PixelFormat::Rgb16 => (0..w * h * 3)
                .flat_map(|_| 10_000u16.to_ne_bytes())
                .collect(),
            _ => vec![40; (w * h) as usize * format.channels()],
        };
        OutputImage::from_raw(w, h, format, data).unwrap()
    }

    fn level(img: &OutputImage, x: usize, y: usize) -> f32 {
        match img.format() {
            PixelFormat::Rgb16 => {
                let i = (y * img.width() as usize + x) * 6;
                f32::from(u16::from_ne_bytes([img.data()[i], img.data()[i + 1]])) / 65535.0
            }
            f => f32::from(img.data()[(y * img.width() as usize + x) * f.channels()]) / 255.0,
        }
    }

    #[test]
    fn sits_in_its_corner_at_its_size_and_nowhere_else() {
        let wm = Watermark::from_png(&drawing(200, 50, 255), Position::BottomRight, Size::Medium)
            .unwrap();
        let img = grey(1000, 600, PixelFormat::Rgb8);
        let out = apply(&img, &wm);
        // Medium: 5.5 % of the short edge (600) tall = 33 px, 4:1 wide = 132 px; a 3 %
        // margin = 18 px from the right and bottom edges.
        let (x0, y0) = (1000 - 18 - 132, 600 - 18 - 33);
        let inside = level(&out, x0 + 66, y0 + 16);
        let expected = 1.0 * OPACITY + 40.0 / 255.0 * (1.0 - OPACITY);
        assert!((inside - expected).abs() < 0.01, "{inside} vs {expected}");
        for (x, y) in [
            (x0 - 3, y0 + 16),
            (x0 + 66, y0 - 3),
            (990, 590),
            (10, 10),
            (500, 300),
        ] {
            assert_eq!(level(&out, x, y), 40.0 / 255.0, "({x}, {y}) changed");
        }
    }

    #[test]
    fn every_position_lands_where_it_says() {
        let png = drawing(100, 20, 255);
        let img = grey(800, 800, PixelFormat::Rgb8);
        for (position, (x, y)) in [
            (Position::TopLeft, (40, 30)),
            (Position::TopRight, (760, 30)),
            (Position::BottomLeft, (40, 770)),
            (Position::BottomRight, (760, 770)),
            (Position::Centre, (400, 400)),
        ] {
            let out = apply(
                &img,
                &Watermark::from_png(&png, position, Size::Small).unwrap(),
            );
            assert!(level(&out, x, y) > 0.5, "{position:?} not at ({x}, {y})");
        }
    }

    #[test]
    fn transparency_and_16_bit_are_kept() {
        // A half-transparent drawing lays half as much.
        let wm =
            Watermark::from_png(&drawing(100, 25, 128), Position::Centre, Size::Large).unwrap();
        let out = apply(&grey(400, 400, PixelFormat::Rgb8), &wm);
        let half = level(&out, 200, 200);
        let a = 128.0 / 255.0 * OPACITY;
        assert!(
            (half - (a + 40.0 / 255.0 * (1.0 - a))).abs() < 0.01,
            "{half}"
        );
        // 16-bit stays 16-bit and is laid over the same way.
        let img16 = grey(400, 400, PixelFormat::Rgb16);
        let out16 = apply(&img16, &wm);
        assert_eq!(out16.format(), PixelFormat::Rgb16);
        assert!((level(&out16, 200, 200) - (a + 10_000.0 / 65535.0 * (1.0 - a))).abs() < 0.01);
        assert_eq!(level(&out16, 5, 5), 10_000.0 / 65535.0);
    }

    #[test]
    fn a_long_line_never_runs_off_the_photo() {
        let wm = Watermark::from_png(&drawing(4000, 40, 255), Position::BottomLeft, Size::Large)
            .unwrap();
        let img = grey(300, 1000, PixelFormat::Rgb8);
        let out = apply(&img, &wm);
        // Scaled to fit between the margins (9 px each side of a 300 px width).
        assert_eq!(level(&out, 3, 990), 40.0 / 255.0);
        assert_eq!(level(&out, 296, 990), 40.0 / 255.0);
        assert!(level(&out, 150, 1000 - 9 - 1) > 0.5 || level(&out, 150, 989) > 0.5);
    }

    #[test]
    fn a_bad_drawing_is_an_error() {
        assert!(Watermark::from_png(b"not a png", Position::BottomRight, Size::Small).is_err());
    }
}
