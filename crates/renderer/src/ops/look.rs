//! Base looks (ADR 0022): the tone curve that turns scene-linear sensor data into a
//! pleasing picture before the photographer's adjustments, as a camera's JPEG engine
//! or a raw converter's default profile does.
//!
//! **Standard** is a film-like S-curve: a brightness lift, a toe that keeps deep
//! shadows deep, and a shoulder that rolls highlights off towards white instead of
//! clipping. Its three parameters were fitted to six cameras' own JPEGs
//! (`bench --look`, docs/PERFORMANCE.md §12), bringing the average tonal difference
//! from 0.153 to 0.058 (sRGB units).
//!
//! **Flat** is no curve: the look of recipes saved before version 2, kept so those
//! edits look exactly as they did.

use serde::{Deserialize, Serialize};

/// The base look a recipe starts from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Look {
    /// Camera-like tone curve (the default).
    #[default]
    Standard,
    /// No curve: scene-linear values encoded directly (recipe version 1 behaviour).
    Flat,
}

/// Brightness lift applied before the curve: 2^1.25 (1.25 EV).
const LIFT: f32 = 2.378_414_2;
/// Contrast of the S-curve (slope in log space).
const SLOPE: f32 = 1.65;
/// Lifted value that maps to half of the curve's range.
const PIVOT: f32 = 0.55;

fn log_logistic(v: f32) -> f32 {
    1.0 / (1.0 + (PIVOT / v).powf(SLOPE))
}

/// The Standard curve for one scene-linear channel value. Maps 0 to 0 and the sensor's
/// white (1.0) to 1.0; monotonic. Values above 1 are passed through (and clip at output).
pub fn standard(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return x;
    }
    log_logistic(x * LIFT) / log_logistic(LIFT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_black_and_white() {
        assert_eq!(standard(0.0), 0.0);
        assert_eq!(standard(-0.1), 0.0);
        assert!((standard(0.999_999) - 1.0).abs() < 1e-5);
        assert_eq!(standard(1.5), 1.5, "above white passes through");
    }

    #[test]
    fn is_monotonic() {
        let mut prev = 0.0;
        for i in 1..=1000 {
            let y = standard(i as f32 / 1000.0);
            assert!(y >= prev, "at {i}");
            prev = y;
        }
    }

    #[test]
    fn lifts_midtones_and_keeps_deep_shadows_deep() {
        // The fitted shape: mid grey brighter, highlights rolled off, blacks deeper.
        assert!((standard(0.18) - 0.434).abs() < 0.01);
        assert!(standard(0.5) > 0.83 && standard(0.5) < 0.87);
        assert!(standard(0.005) < 0.005);
    }
}
