//! The guided filter (He, Sun and Tang, 2010) on one channel: `input` smoothed so its
//! edges follow `guide`'s. A sky mask found on a coarse grid is refined with it, so it
//! follows tree lines and rooftops in the photo.

/// `input` filtered with `guide` (both `width` × `height`, row by row) over a
/// (2·`radius` + 1)² window; `eps` sets how strong an edge in the guide must be to be
/// kept (a variance, in the guide's units squared).
pub(super) fn guided_filter(
    guide: &[f32],
    input: &[f32],
    width: usize,
    height: usize,
    radius: usize,
    eps: f32,
) -> Vec<f32> {
    let n = width * height;
    debug_assert!(guide.len() == n && input.len() == n);
    let mean = |values: &[f32]| box_mean(values, width, height, radius);
    let mean_i = mean(guide);
    let mean_p = mean(input);
    let products: Vec<f32> = guide.iter().zip(input).map(|(i, p)| i * p).collect();
    let squares: Vec<f32> = guide.iter().map(|i| i * i).collect();
    let mean_ip = mean(&products);
    let mean_ii = mean(&squares);
    let mut a = vec![0.0; n];
    let mut b = vec![0.0; n];
    for k in 0..n {
        let var = (mean_ii[k] - mean_i[k] * mean_i[k]).max(0.0);
        let cov = mean_ip[k] - mean_i[k] * mean_p[k];
        a[k] = cov / (var + eps);
        b[k] = mean_p[k] - a[k] * mean_i[k];
    }
    let mean_a = mean(&a);
    let mean_b = mean(&b);
    (0..n).map(|k| mean_a[k] * guide[k] + mean_b[k]).collect()
}

/// The mean of `values` over a (2·`radius` + 1)² window around each pixel, the window
/// cut to the picture at its edges. From a summed-area table, so any radius costs the
/// same.
pub(super) fn box_mean(values: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    let stride = width + 1;
    let mut table = vec![0.0f64; stride * (height + 1)];
    for y in 0..height {
        let mut row = 0.0f64;
        for x in 0..width {
            row += f64::from(values[y * width + x]);
            table[(y + 1) * stride + x + 1] = table[y * stride + x + 1] + row;
        }
    }
    let mut out = vec![0.0; width * height];
    for y in 0..height {
        let (y0, y1) = (y.saturating_sub(radius), (y + radius + 1).min(height));
        for x in 0..width {
            let (x0, x1) = (x.saturating_sub(radius), (x + radius + 1).min(width));
            let sum = table[y1 * stride + x1] - table[y0 * stride + x1] - table[y1 * stride + x0]
                + table[y0 * stride + x0];
            out[y * width + x] = (sum / ((y1 - y0) * (x1 - x0)) as f64) as f32;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_mean_averages_its_window_cut_at_the_edges() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let m = box_mean(&values, 3, 2, 1);
        // The corner's window is its 2 × 2 corner.
        assert!((m[0] - 3.0).abs() < 1e-6);
        // The middle of the top row sees all six.
        assert!((m[1] - 3.5).abs() < 1e-6);
    }

    #[test]
    fn the_filter_moves_a_soft_edge_onto_the_guides_edge() {
        // The guide steps at x = 12; the input, a blurred step, at x = 8..16.
        let (w, h) = (24, 4);
        let guide: Vec<f32> = (0..w * h)
            .map(|k| if k % w < 12 { 0.9 } else { 0.2 })
            .collect();
        let input: Vec<f32> = (0..w * h)
            .map(|k| ((16.0 - (k % w) as f32) / 8.0).clamp(0.0, 1.0))
            .collect();
        let out = guided_filter(&guide, &input, w, h, 4, 1e-4);
        // Sharp at the guide's edge: high just before it, low just after.
        assert!(out[10] > 0.75, "{}", out[10]);
        assert!(out[13] < 0.35, "{}", out[13]);
    }
}
