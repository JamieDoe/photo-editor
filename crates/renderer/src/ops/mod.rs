//! Scalar definitions of each stage's maths.
//!
//! These are the reference implementations: backends may restructure them (LUTs,
//! SIMD, shaders) but tests compare backends against these functions.

pub mod contrast;
pub mod saturation;
pub mod white_balance;
