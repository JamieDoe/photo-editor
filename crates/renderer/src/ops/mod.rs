//! Scalar definitions of each stage's maths.
//!
//! These are the reference implementations: backends may restructure them (LUTs,
//! SIMD, shaders) but tests compare backends against these functions.

pub mod colour_mixer;
pub mod contrast;
pub mod dehaze;
pub mod detail;
pub mod finishing;
pub mod look;
pub mod noise;
pub mod parametric_curve;
pub mod point_curve;
pub mod saturation;
pub mod scene;
pub mod tone;
pub mod vibrance;
pub mod white_balance;
