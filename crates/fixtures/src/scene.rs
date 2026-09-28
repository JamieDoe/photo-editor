//! The synthetic test chart, defined in scene-linear sRGB/Rec.709.
//!
//! Layout (by fraction of height):
//! - 0.00-0.20: horizontal neutral ramp, linear 0..1
//! - 0.20-0.70: 6x3 colour patches (ColorChecker-like reflectances + primaries)
//! - 0.70-0.85: 6 neutral patches (0.90 .. 0.03)
//! - 0.85-1.00: hue sweep at fixed saturation

const PATCHES: [[[f32; 3]; 6]; 3] = [
    [
        [0.173, 0.083, 0.056],
        [0.559, 0.304, 0.222],
        [0.117, 0.198, 0.338],
        [0.106, 0.150, 0.052],
        [0.231, 0.219, 0.438],
        [0.130, 0.515, 0.413],
    ],
    [
        [0.745, 0.201, 0.023],
        [0.064, 0.105, 0.389],
        [0.558, 0.081, 0.114],
        [0.107, 0.041, 0.141],
        [0.340, 0.502, 0.047],
        [0.800, 0.365, 0.017],
    ],
    [
        [0.020, 0.040, 0.600],
        [0.050, 0.500, 0.050],
        [0.600, 0.030, 0.030],
        [0.850, 0.750, 0.010],
        [0.600, 0.050, 0.500],
        [0.010, 0.450, 0.650],
    ],
];

const NEUTRALS: [f32; 6] = [0.90, 0.59, 0.36, 0.19, 0.09, 0.03];

/// Scene-linear RGB at pixel (x, y) of a `width x height` chart.
pub fn sample(x: u32, y: u32, width: u32, height: u32) -> [f32; 3] {
    let u = (x as f32 + 0.5) / width as f32;
    let v = (y as f32 + 0.5) / height as f32;
    if v < 0.20 {
        [u; 3]
    } else if v < 0.70 {
        let row = (((v - 0.20) / 0.50) * 3.0).min(2.0) as usize;
        let col = (u * 6.0).min(5.0) as usize;
        PATCHES[row][col]
    } else if v < 0.85 {
        [NEUTRALS[(u * 6.0).min(5.0) as usize]; 3]
    } else {
        hue_sweep(u)
    }
}

/// A pixel in the middle of the brightest neutral patch.
pub fn chart_probe_neutral(width: u32, height: u32) -> (u32, u32) {
    ((width as f32 / 12.0) as u32, (height as f32 * 0.775) as u32)
}

/// Scene-linear value of the patch at [`chart_probe_neutral`].
pub fn chart_neutral_value() -> f32 {
    NEUTRALS[0]
}

fn hue_sweep(h: f32) -> [f32; 3] {
    // HSV with S=0.8, V=0.5, computed in linear light.
    let (s, v) = (0.8_f32, 0.5_f32);
    let h6 = (h.fract() * 6.0).min(5.999);
    let f = h6.fract();
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match h6 as u32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}
