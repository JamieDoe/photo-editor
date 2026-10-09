# ADR 0075: Lens corrections from the camera's own profile

- Status: Accepted
- Date: 2026-10-09

## Context

The design's Geometry section has a "Lens correction" switch, with the lens's name and
"auto" under it. Lightroom corrects a lens's distortion and vignetting from a profile,
by default for lenses whose profile is built in.

Where profiles can come from:
- **The raw file.** Many cameras record their lens's corrections in each file;
  mirrorless lenses rely on them. These are the camera maker's own data for this lens
  at these settings, so no licence question arises.
- **A lens database,** lensfun's. Its data is CC BY-SA, which needs a licensing
  decision (CLAUDE.md §17). Not used here.

What the camera fixtures record (read with a small TIFF walker):

| File | Corrections in the file |
|---|---|
| Sony A7 III, A7R IV (ARW) | distortion (`0x7037`), vignetting (`0x7032`) and chromatic aberration (`0x7035`), 16 knots each, in the raw IFD |
| Nikon Z 6 | only whether the camera corrected its own JPEG; the lens data is encrypted |
| Ricoh GR III (DNG) | no opcode lists |
| Fujifilm X-T3, Canon R6 | none |

## Decision

1. **Read the profile from the file** (`raw::lens`). Only the headers are read, a few
   kilobytes: IFD0, its SubIFDs and the EXIF IFD. The result is a camera-neutral
   `LensProfile`: curves of the radius from the centre (0) to the corner (1, the
   half-diagonal).
   - **Sony:**
     - distortion is `1 + v / 2^14`, the real radius over the ideal;
     - vignetting is `2^(0.5 − 2^(v / 2^13 − 1))`, the brightness relative to the
       centre;
     - knots are evenly spaced.
   - **The lens's name:** from EXIF `LensModel`.
2. **The renderer applies it** (`renderer::lens`).
   - **Distortion** is undone in the geometry mapping (`Mapping::with_lens`), last
     after crop, straighten, perspective and quarter turns. So the viewer, windows,
     exports, crop checks and generated masks all agree.
   - **Filling the frame:** the ideal image is scaled, where pincushion correction
     needs it, so the photo still fills the frame. Barrel correction needs none.
   - **Interpolation:** smooth (cubic Hermite) between knots.
   - **Vignetting** is lifted while framing. Framed images stop at the sensor's white,
     so they are stored divided by the largest gain (the headroom). A gain at the
     start of the plan multiplies it back, in floating point, so bright corners don't
     clip. Every stage after (tone maps, dehaze, the photo's white) sees the
     corrected light.
   - **Chromatic aberration** from the profile isn't applied yet. The "Remove
     chromatic aberration" switch measures it from the photo (ADR 0035), and both
     would correct it twice.
3. **The recipe:** `profileCorrections`, on by default and written only when off.
   - **Recipe version 29.** Older recipes that edit something read with it off, so
     they render as they did. Older recipes that change nothing are no edit, like a
     photo never edited, and get it.
   - **It belongs to the photo:** in the "Crop, geometry and lens" group for copy and
     paste. Presets keep the photo's own setting.
4. **The UI:** the design's switch, first under "Perspective & lens".
   - **With a profile:** it shows the lens's name and "· auto".
   - **Without one:** "No lens profile in this photo's file", and it is disabled.
5. **Exports and thumbnails** read the profile from the file.

## How the reading was checked

Against the camera's own JPEG preview of the A7R IV (24–70 mm at 24 mm, f/4), which the
camera corrected.

| Check | Measured | Expected |
|---|---|---|
| Distortion: our uncorrected render against the camera's JPEG, radial displacement from 0.3 to 0.9 of the half-diagonal | −3.5 % | −3.53 % (the profile) |
| Distortion: what the correction removes between bands | 1.68 % and 2.39 % | 1.35 % and 2.45 % |
| Vignetting: sky's corner-to-middle brightness, ours | 1.002 off, 1.018 on | camera 1.022 |

The camera's JPEG keeps about 1 % of distortion at the corners that the profile
removes, so our correction is the fuller one.

## Consequences

- Sony photos (and others as readers are added) open corrected, as in Lightroom and in
  the camera's JPEGs. Turning the switch off shows the lens as it is.
- Framing with a profile costs about 65 ms more for a 15 MP view (once, when the
  framing changes); slider changes cost nothing extra.
- **Next:**
  - the profile's chromatic aberration, which would replace the measured one where
    there is a profile;
  - DNG opcode lists (`WarpRectilinear`, `FixVignetteRadial`), Fujifilm, Olympus/OM
    and Panasonic, each checked against a file that has them.
- **Older DSLR lenses** need lensfun's database: a licensing decision for later.
