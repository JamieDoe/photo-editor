//! Camera RAW decoding via LibRaw (see ADR 0003).

mod ffi;

use std::ffi::{CStr, CString, c_void};
use std::os::raw::c_int;
use std::path::Path;

use image_core::{Cancellation, LinearImage};

use crate::preview::{self, Rgb8};
use crate::{
    DecodeError, DecodeOptions, DecodeScale, DecodedImage, Decoder, EmbeddedPreview, SourceInfo,
    SourceKind,
};

pub(crate) const EXTENSIONS: &[&str] = &[
    "3fr", "ari", "arw", "bay", "cr2", "cr3", "crw", "dcr", "dng", "erf", "fff", "iiq", "k25",
    "kdc", "mef", "mos", "mrw", "nef", "nrw", "orf", "pef", "raf", "raw", "rw2", "rwl", "sr2",
    "srf", "srw", "x3f",
];

/// LibRaw-backed decoder producing linear sRGB-primaries data with as-shot white balance.
#[derive(Debug, Default, Clone, Copy)]
pub struct LibRawDecoder;

impl LibRawDecoder {
    pub fn libraw_version() -> String {
        // SAFETY: returns a pointer to a static NUL-terminated string.
        unsafe { CStr::from_ptr(ffi::pe_raw_libraw_version()) }
            .to_string_lossy()
            .into_owned()
    }
}

impl Decoder for LibRawDecoder {
    fn name(&self) -> &'static str {
        "libraw"
    }

    fn handles(&self, path: &Path) -> bool {
        crate::has_extension(path, EXTENSIONS)
    }

    fn read_metadata(&self, path: &Path) -> Result<crate::PhotoMetadata, DecodeError> {
        use crate::metadata::{non_empty, positive, rotation_from_flip};
        if !path.exists() {
            return Err(DecodeError::NotFound(path.display().to_string()));
        }
        let c_path = path_to_cstring(path)?;
        let mut m = ffi::PeRawMeta::default();
        // SAFETY: valid path and out-pointer for the duration of the call.
        let rc = unsafe { ffi::pe_raw_metadata(c_path.as_ptr(), &mut m) };
        if rc != 0 {
            return Err(map_error(rc));
        }
        let text = |c: &[std::os::raw::c_char]| non_empty(&c_chars_to_string(c));
        Ok(crate::PhotoMetadata {
            camera_make: text(&m.make),
            camera_model: text(&m.model),
            lens: text(&m.lens),
            captured_at: text(&m.captured_at),
            iso: positive(m.iso).map(|v| v.round() as u32),
            aperture: positive(m.aperture),
            shutter_seconds: positive(m.shutter),
            focal_length_mm: positive(m.focal_length),
            width: (m.width > 0).then_some(m.width),
            height: (m.height > 0).then_some(m.height),
            rotation: rotation_from_flip(m.flip),
            gps: (m.has_gps != 0 && (m.latitude != 0.0 || m.longitude != 0.0))
                .then_some((m.latitude, m.longitude)),
        })
    }

    fn embedded_preview(
        &self,
        path: &Path,
        min_long_edge: u32,
        cancel: &dyn Cancellation,
    ) -> Result<Option<EmbeddedPreview>, DecodeError> {
        if cancel.is_cancelled() {
            return Err(DecodeError::Cancelled);
        }
        if !path.exists() {
            return Err(DecodeError::NotFound(path.display().to_string()));
        }
        self.extract_embedded(path, min_long_edge)
    }

    fn decode(
        &self,
        path: &Path,
        options: DecodeOptions,
        cancel: &dyn Cancellation,
    ) -> Result<DecodedImage, DecodeError> {
        if !path.exists() {
            return Err(DecodeError::NotFound(path.display().to_string()));
        }
        let c_path = path_to_cstring(path)?;
        let min_long_edge = match options.scale {
            DecodeScale::Full => 0,
            DecodeScale::AtLeast(n) => n.max(1),
        };

        // The callback receives a thin pointer to this fat trait-object reference.
        let cancel_ref: &dyn Cancellation = cancel;
        let cancel_ctx = &cancel_ref as *const &dyn Cancellation as *mut c_void;
        let mut ctx: *mut ffi::PeRawCtx = std::ptr::null_mut();
        let mut info = ffi::PeRawInfo::default();
        // SAFETY: all pointers are valid for the duration of the call; `cancel_ctx`
        // outlives it (it borrows from this stack frame).
        let rc = unsafe {
            ffi::pe_raw_decode(
                c_path.as_ptr(),
                min_long_edge,
                options
                    .max_threads
                    .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX)),
                Some(cancel_trampoline),
                cancel_ctx,
                &mut ctx,
                &mut info,
            )
        };
        if rc != 0 {
            return Err(map_error(rc));
        }
        let ctx = CtxGuard(ctx);

        let samples = info.width as usize * info.height as usize * 3;
        let mut data = vec![0u16; samples];
        // SAFETY: ctx is a live handle; dst has exactly `samples` elements.
        let rc = unsafe { ffi::pe_raw_copy_rgb16(ctx.0, data.as_mut_ptr(), samples) };
        drop(ctx);
        if rc != 0 {
            return Err(map_error(rc));
        }
        if cancel.is_cancelled() {
            return Err(DecodeError::Cancelled);
        }

        let image = LinearImage::new(info.width, info.height, data)
            .map_err(|e| DecodeError::Internal(format!("LibRaw buffer: {e}")))?;
        let positive = |v: f32| (v.is_finite() && v > 0.0).then_some(v);
        Ok(DecodedImage {
            image,
            info: SourceInfo {
                decoder: self.name(),
                kind: SourceKind::CameraRaw,
                make: c_chars_to_string(&info.make),
                model: c_chars_to_string(&info.model),
                full_width: info.full_width,
                full_height: info.full_height,
                iso: positive(info.iso),
                shutter_seconds: positive(info.shutter),
                aperture: positive(info.aperture),
                focal_length_mm: positive(info.focal_length),
            },
        })
    }
}

impl LibRawDecoder {
    fn extract_embedded(
        &self,
        path: &Path,
        min_long_edge: u32,
    ) -> Result<Option<EmbeddedPreview>, DecodeError> {
        let c_path = path_to_cstring(path)?;
        let mut thumb: *mut ffi::PeRawThumb = std::ptr::null_mut();
        let mut info = ffi::PeThumbInfo::default();
        let mut data: *const u8 = std::ptr::null();
        let mut len = 0usize;
        // SAFETY: all out-pointers are valid; the thumb handle is released below.
        let rc = unsafe {
            ffi::pe_raw_thumbnail(
                c_path.as_ptr(),
                min_long_edge,
                &mut thumb,
                &mut info,
                &mut data,
                &mut len,
            )
        };
        if rc == ffi::LIBRAW_NO_THUMBNAIL {
            return Ok(None);
        }
        if rc != 0 {
            return Err(map_error(rc));
        }
        let decoded = {
            // SAFETY: `data`/`len` describe memory owned by `thumb`, alive until release.
            let bytes = unsafe { std::slice::from_raw_parts(data, len) };
            match info.format {
                ffi::THUMB_JPEG => preview::decode_jpeg(bytes, min_long_edge),
                ffi::THUMB_BITMAP if len == info.width as usize * info.height as usize * 3 => {
                    let rgb = Rgb8 {
                        width: info.width,
                        height: info.height,
                        data: bytes.to_vec(),
                    };
                    Ok((rgb, (info.width, info.height)))
                }
                _ => Err(DecodeError::Unsupported("embedded preview format".into())),
            }
        };
        // SAFETY: released exactly once; `bytes` is no longer used.
        unsafe { ffi::pe_raw_thumb_release(thumb) };
        let (mut rgb, (embedded_width, embedded_height)) = decoded?;
        // Scaled JPEG decoding usually lands within 2x of the target already.
        while rgb.width.max(rgb.height) / 2 >= min_long_edge.max(1) {
            rgb = preview::downsample_2x(&rgb);
        }
        Ok(Some(EmbeddedPreview {
            image: preview::orient_to_rgba(&rgb, info.flip),
            embedded_width,
            embedded_height,
        }))
    }
}

struct CtxGuard(*mut ffi::PeRawCtx);

impl Drop for CtxGuard {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful pe_raw_decode and is released once.
        unsafe { ffi::pe_raw_release(self.0) }
    }
}

extern "C" fn cancel_trampoline(ctx: *mut c_void) -> c_int {
    if ctx.is_null() {
        return 0;
    }
    // SAFETY: ctx points at a `&dyn Cancellation` on the decode() stack frame, which
    // is alive for the whole pe_raw_decode call.
    let cancel = unsafe { &*(ctx as *const &dyn Cancellation) };
    c_int::from(cancel.is_cancelled())
}

fn map_error(code: c_int) -> DecodeError {
    // SAFETY: LibRaw returns a static string for any code.
    let msg = unsafe { CStr::from_ptr(ffi::pe_raw_strerror(code)) }
        .to_string_lossy()
        .into_owned();
    match code {
        ffi::LIBRAW_FILE_UNSUPPORTED => DecodeError::Unsupported(msg),
        ffi::LIBRAW_CANCELLED_BY_CALLBACK => DecodeError::Cancelled,
        ffi::LIBRAW_UNSUFFICIENT_MEMORY | ffi::LIBRAW_TOO_BIG => DecodeError::OutOfMemory,
        ffi::LIBRAW_IO_ERROR => DecodeError::Io(std::io::Error::other(msg)),
        c if c > 0 => DecodeError::Io(std::io::Error::from_raw_os_error(c)),
        _ => DecodeError::Corrupt(format!("{msg} (LibRaw {code})")),
    }
}

#[cfg(unix)]
fn path_to_cstring(path: &Path) -> Result<CString, DecodeError> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| DecodeError::Unsupported("path contains NUL".into()))
}

#[cfg(not(unix))]
fn path_to_cstring(path: &Path) -> Result<CString, DecodeError> {
    // Windows needs libraw_open_wfile for non-ANSI paths; tracked for Phase 1.
    let s = path
        .to_str()
        .ok_or_else(|| DecodeError::Unsupported("non-UTF-8 path".into()))?;
    CString::new(s).map_err(|_| DecodeError::Unsupported("path contains NUL".into()))
}

fn c_chars_to_string(chars: &[std::os::raw::c_char]) -> String {
    let bytes: Vec<u8> = chars
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).trim().to_owned()
}

#[cfg(test)]
mod tests;
