//! Bindings to the C shim in `shim/pe_libraw.h`. Keep in sync with that header.

use std::ffi::c_void;
use std::os::raw::{c_char, c_int};

#[repr(C)]
pub struct PeRawCtx {
    _private: [u8; 0],
}

#[repr(C)]
pub struct PeRawInfo {
    pub width: u32,
    pub height: u32,
    pub full_width: u32,
    pub full_height: u32,
    pub flip: i32,
    pub half_size: i32,
    pub iso: f32,
    pub shutter: f32,
    pub aperture: f32,
    pub focal_length: f32,
    pub make: [c_char; 64],
    pub model: [c_char; 64],
}

impl Default for PeRawInfo {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            full_width: 0,
            full_height: 0,
            flip: 0,
            half_size: 0,
            iso: 0.0,
            shutter: 0.0,
            aperture: 0.0,
            focal_length: 0.0,
            make: [0; 64],
            model: [0; 64],
        }
    }
}

#[repr(C)]
pub struct PeRawThumb {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Default)]
pub struct PeThumbInfo {
    pub format: i32,
    pub width: u32,
    pub height: u32,
    pub flip: i32,
    pub available: u32,
}

pub const THUMB_JPEG: i32 = 1;
pub const THUMB_BITMAP: i32 = 2;
pub const LIBRAW_NO_THUMBNAIL: c_int = -5;

pub type PeCancelFn = Option<extern "C" fn(ctx: *mut c_void) -> c_int>;

// LibRaw error codes used for classification (libraw_const.h).
pub const LIBRAW_FILE_UNSUPPORTED: c_int = -2;
pub const LIBRAW_UNSUFFICIENT_MEMORY: c_int = -100_007;
pub const LIBRAW_IO_ERROR: c_int = -100_009;
pub const LIBRAW_CANCELLED_BY_CALLBACK: c_int = -100_010;
pub const LIBRAW_TOO_BIG: c_int = -100_012;

unsafe extern "C" {
    pub fn pe_raw_decode(
        path: *const c_char,
        min_long_edge: u32,
        max_threads: u32,
        cancel: PeCancelFn,
        cancel_ctx: *mut c_void,
        out_ctx: *mut *mut PeRawCtx,
        info: *mut PeRawInfo,
    ) -> c_int;
    pub fn pe_raw_copy_rgb16(ctx: *const PeRawCtx, dst: *mut u16, samples: usize) -> c_int;
    pub fn pe_raw_release(ctx: *mut PeRawCtx);
    pub fn pe_raw_thumbnail(
        path: *const c_char,
        min_long_edge: u32,
        out: *mut *mut PeRawThumb,
        info: *mut PeThumbInfo,
        data: *mut *const u8,
        len: *mut usize,
    ) -> c_int;
    pub fn pe_raw_thumb_release(thumb: *mut PeRawThumb);
    pub fn pe_raw_strerror(code: c_int) -> *const c_char;
    pub fn pe_raw_libraw_version() -> *const c_char;
}
