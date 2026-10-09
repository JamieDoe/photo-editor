//! Apple Vision (ADR 0074): the system's subject and person instance masks, run
//! locally on the Neural Engine where there is one. Nothing is shipped: on a macOS
//! without the requests (both need 14) the kinds are simply unsupported.
//!
//! People use the person *instance* request, not the older person segmentation: that
//! one always answers with a mask (it is made for video of people). Even the instance
//! request took clay animal figures for people, so its mask is kept only when Vision's
//! human detector also finds someone ([`MIN_PERSON_CONFIDENCE`]).

use std::ptr;

use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2::rc::autoreleasepool;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_core_foundation::{CFData, CFRetained};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    kCGColorSpaceSRGB,
};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
    kCVPixelFormatType_OneComponent8, kCVPixelFormatType_OneComponent16Half,
    kCVPixelFormatType_OneComponent32Float,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSProcessInfo};
use objc2_vision::{
    VNDetectHumanRectanglesRequest, VNGenerateForegroundInstanceMaskRequest,
    VNGeneratePersonInstanceMaskRequest, VNImageOption, VNImageRequestHandler,
    VNInstanceMaskObservation, VNRequest,
};

/// A person mask is kept only if Vision's human detector finds someone this sure.
const MIN_PERSON_CONFIDENCE: f32 = 0.5;

use crate::{AiError, Coverage, MaskKind, Picture, Segmenter};

/// Apple Vision's segmentation.
pub(crate) struct VisionSegmenter;

/// The Vision class that makes a `kind` mask; Vision has none for the sky.
fn class_name(kind: MaskKind) -> Option<&'static std::ffi::CStr> {
    match kind {
        MaskKind::Subject => Some(c"VNGenerateForegroundInstanceMaskRequest"),
        MaskKind::People => Some(c"VNGeneratePersonInstanceMaskRequest"),
        MaskKind::Sky => None,
    }
}

impl Segmenter for VisionSegmenter {
    fn supports(&self, kind: MaskKind) -> bool {
        class_name(kind).is_some_and(|c| AnyClass::get(c).is_some())
    }

    fn generator(&self, kind: MaskKind) -> String {
        let os = NSProcessInfo::processInfo().operatingSystemVersion();
        // Vision's revision of the request, which changes when its model does.
        let revision = if self.supports(kind) {
            // SAFETY: the class exists (checked above); a new request is at the current
            // revision.
            unsafe {
                match kind {
                    MaskKind::Subject => VNGenerateForegroundInstanceMaskRequest::new().revision(),
                    MaskKind::People => VNGeneratePersonInstanceMaskRequest::new().revision(),
                    MaskKind::Sky => 0,
                }
            }
        } else {
            0
        };
        format!(
            "apple-vision/{kind:?}/r{revision}/macOS-{}.{}.{}",
            os.majorVersion, os.minorVersion, os.patchVersion
        )
        .to_lowercase()
    }

    fn segment(&self, picture: Picture<'_>, kind: MaskKind) -> Result<Option<Coverage>, AiError> {
        picture.check()?;
        if !self.supports(kind) {
            return Err(AiError::Unsupported(kind));
        }
        autoreleasepool(|_| {
            let image = cg_image(&picture)?;
            let options = NSDictionary::<VNImageOption, AnyObject>::new();
            // SAFETY: a freshly allocated handler initialised with a valid image and
            // an empty options dictionary of the expected types.
            let handler = unsafe {
                VNImageRequestHandler::initWithCGImage_options(
                    VNImageRequestHandler::alloc(),
                    &image,
                    &options,
                )
            };
            match kind {
                MaskKind::Subject => subject(&handler),
                MaskKind::People => people(&handler),
                MaskKind::Sky => Err(AiError::Unsupported(kind)),
            }
        })
    }
}

/// The subject mask: every foreground instance Vision finds, at the picture's size.
fn subject(handler: &VNImageRequestHandler) -> Result<Option<Coverage>, AiError> {
    // SAFETY: the class exists (`supports` was checked).
    let request = unsafe { VNGenerateForegroundInstanceMaskRequest::new() };
    let as_request: &VNRequest = &request;
    handler
        .performRequests_error(&NSArray::from_slice(&[as_request]))
        .map_err(failed)?;
    // SAFETY: results are read after the request was performed, on this thread.
    instances(handler, unsafe { request.results() })
}

/// The people mask: every person Vision finds, at the picture's size, when its human
/// detector agrees there is someone.
fn people(handler: &VNImageRequestHandler) -> Result<Option<Coverage>, AiError> {
    // SAFETY: the classes exist (`supports` was checked; the detector is older).
    let (mask, humans) = unsafe {
        let humans = VNDetectHumanRectanglesRequest::new();
        humans.setUpperBodyOnly(false);
        (VNGeneratePersonInstanceMaskRequest::new(), humans)
    };
    let (a, b): (&VNRequest, &VNRequest) = (&mask, &humans);
    handler
        .performRequests_error(&NSArray::from_slice(&[a, b]))
        .map_err(failed)?;
    // SAFETY: results are read after the requests were performed, on this thread.
    let someone = unsafe { humans.results() }.is_some_and(|found| {
        (0..found.count())
            .any(|i| unsafe { found.objectAtIndex(i).confidence() } >= MIN_PERSON_CONFIDENCE)
    });
    if !someone {
        return Ok(None);
    }
    // SAFETY: as above.
    instances(handler, unsafe { mask.results() })
}

/// All the instances of an instance mask request's first observation, as one mask at
/// the picture's size; `None` when it found none.
fn instances(
    handler: &VNImageRequestHandler,
    results: Option<Retained<NSArray<VNInstanceMaskObservation>>>,
) -> Result<Option<Coverage>, AiError> {
    let Some(observation) = results.and_then(|r| r.firstObject()) else {
        return Ok(None);
    };
    // SAFETY: the observation came from this handler's request.
    let instances = unsafe { observation.allInstances() };
    if instances.count() == 0 {
        return Ok(None);
    }
    // SAFETY: as above; the mask is scaled to the handler's image.
    let mask = unsafe {
        observation
            .generateScaledMaskForImageForInstances_fromRequestHandler_error(&instances, handler)
    }
    .map_err(failed)?;
    read_mask(&mask).map(Some)
}

fn failed(e: Retained<NSError>) -> AiError {
    AiError::Failed(e.localizedDescription().to_string())
}

/// The picture as a Core Graphics image: 8-bit sRGB, its alpha ignored.
fn cg_image(picture: &Picture<'_>) -> Result<CFRetained<CGImage>, AiError> {
    let fail = || AiError::Failed("could not make an image for Vision".into());
    let data = CFData::from_bytes(picture.rgba);
    let provider = CGDataProvider::with_cf_data(Some(&data)).ok_or_else(fail)?;
    // SAFETY: a constant string from Core Graphics, valid for the process.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
        .or_else(CGColorSpace::new_device_rgb)
        .ok_or_else(fail)?;
    let (w, h) = (picture.width as usize, picture.height as usize);
    // SAFETY: the provider holds `w * h * 4` bytes, as the layout below describes;
    // no decode array.
    unsafe {
        CGImage::new(
            w,
            h,
            8,
            32,
            w * 4,
            Some(&space),
            CGBitmapInfo(CGImageAlphaInfo::NoneSkipLast.0),
            Some(&provider),
            ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
    .ok_or_else(fail)
}

/// A one-component mask buffer (8-bit, half or float) as coverage.
fn read_mask(buffer: &CVPixelBuffer) -> Result<Coverage, AiError> {
    let (w, h) = (
        CVPixelBufferGetWidth(buffer),
        CVPixelBufferGetHeight(buffer),
    );
    let format = CVPixelBufferGetPixelFormatType(buffer);
    // SAFETY: locked read-only for the reads below, and unlocked after.
    unsafe { CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) };
    let base = CVPixelBufferGetBaseAddress(buffer) as *const u8;
    let stride = CVPixelBufferGetBytesPerRow(buffer);
    let result = if base.is_null() {
        Err(AiError::Failed("Vision's mask has no pixels".into()))
    } else {
        let mut data = Vec::with_capacity(w * h);
        for y in 0..h {
            // SAFETY: each row is `stride` bytes from the locked base address, and
            // holds `w` samples of the format's size.
            let row = unsafe { base.add(y * stride) };
            for x in 0..w {
                let v = unsafe { sample(row, x, format) };
                data.push((v.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
        match format {
            f if f == kCVPixelFormatType_OneComponent8
                || f == kCVPixelFormatType_OneComponent16Half
                || f == kCVPixelFormatType_OneComponent32Float =>
            {
                Ok(Coverage {
                    width: w as u32,
                    height: h as u32,
                    data,
                })
            }
            other => Err(AiError::Failed(format!(
                "unexpected mask format {other:#x}"
            ))),
        }
    };
    // SAFETY: matches the lock above.
    unsafe { CVPixelBufferUnlockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) };
    result
}

/// Sample `x` of a mask row, 0..1.
///
/// # Safety
/// `row` must point at a row of at least `x + 1` samples of `format`.
unsafe fn sample(row: *const u8, x: usize, format: u32) -> f32 {
    unsafe {
        if format == kCVPixelFormatType_OneComponent8 {
            f32::from(*row.add(x)) / 255.0
        } else if format == kCVPixelFormatType_OneComponent16Half {
            half_to_f32(ptr::read_unaligned(row.add(x * 2) as *const u16))
        } else if format == kCVPixelFormatType_OneComponent32Float {
            ptr::read_unaligned(row.add(x * 4) as *const f32)
        } else {
            0.0
        }
    }
}

/// An IEEE half-precision value as `f32`.
fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = (h >> 10) & 0x1f;
    let frac = f32::from(h & 0x3ff);
    sign * match exp {
        0 => frac * 2f32.powi(-24),
        31 => f32::INFINITY,
        e => (1.0 + frac / 1024.0) * 2f32.powi(i32::from(e) - 15),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_read_as_floats() {
        assert_eq!(half_to_f32(0x3c00), 1.0);
        assert_eq!(half_to_f32(0x3800), 0.5);
        assert_eq!(half_to_f32(0), 0.0);
    }

    /// A bright disc on a plain grey ground: Vision finds it as the subject.
    #[test]
    fn vision_finds_a_plain_subject() {
        let seg = VisionSegmenter;
        if !seg.supports(MaskKind::Subject) {
            eprintln!("skipped: this macOS has no subject segmentation");
            return;
        }
        let (w, h) = (320u32, 240u32);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let inside = (x as f32 - 160.0).hypot(y as f32 - 120.0) < 60.0;
                rgba.extend(if inside {
                    [230, 60, 40, 255]
                } else {
                    [128, 128, 128, 255]
                });
            }
        }
        let mask = seg
            .segment(
                Picture {
                    width: w,
                    height: h,
                    rgba: &rgba,
                    scene_ev: None,
                },
                MaskKind::Subject,
            )
            .unwrap()
            .expect("a subject");
        assert!(mask.at(0.5, 0.5) > 0.5, "centre {}", mask.at(0.5, 0.5));
        assert!(mask.at(0.03, 0.05) < 0.5, "corner {}", mask.at(0.03, 0.05));
        assert!(
            seg.generator(MaskKind::Subject)
                .starts_with("apple-vision/subject/r")
        );
    }
}
