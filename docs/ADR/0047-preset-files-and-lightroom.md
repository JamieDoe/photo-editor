# ADR 0047: Preset files, and Lightroom presets

- Status: Accepted (Phase 7, milestone 4)
- Date: 2026-09-30

## Context

The product requires importing and exporting presets (`docs/PRODUCT.md` §21). Many of
the photographers the product is for come from Lightroom and bring preset collections
with them, so importing Lightroom presets lowers the cost of switching.

Lightroom Classic (since 7.3) and Lightroom store presets as `.xmp` files. These hold
Camera Raw settings (`crs:` names), written as attributes of an `rdf:Description` or as
elements (the name, tone curves).

## Decision

1. **The app's own preset file** (`.preset`) is JSON:
   `{"format": "photo-editor-preset", "formatVersion": 1, "name": …, "recipe": …}`.
   - The recipe carries its own schema version, so older files migrate like saved
     edits.
   - A newer `formatVersion` or recipe is refused as "made by a newer version".
   - Only the look is written and read (ADR 0046).
   - The renderer owns the format (`renderer::presets::to_file` / `from_file`).
2. **Lightroom `.xmp` presets** are read into a look (`app_core::lightroom`). Settings
   with a counterpart map by name and range:

   | This app | Lightroom setting |
   |---|---|
   | Contrast … Blacks, Texture, Clarity, Dehaze, Vibrance, Saturation | Basic panel and Presence (`…2012`) |
   | Temperature, Tint (a shift from the photo's own white balance) | `IncrementalTemperature`, `IncrementalTint` |
   | Sharpening, Noise reduction | `Sharpness`, `LuminanceSmoothing` |
   | Vignette, Grain | `PostCropVignetteAmount`, `GrainAmount` |
   | Colour mixer (the same eight bands) | HSL |
   | Tone curve, and the red, green and blue curves | `ToneCurvePV2012…` points (0–255) |
   | Saturation −100 | `ConvertToGrayscale` |

   - **Left out, and named in the import's result**, by Lightroom's panel names:
     - white balance in kelvin;
     - the parametric curve;
     - Color Grading and split toning;
     - B&W mix;
     - Calibration;
     - masks and healing;
     - Lens Corrections;
     - crop and transform;
     - the profile;
     - colour noise reduction.
   - **Exposure is not taken:** a preset never sets it here (ADR 0046).
   - **Only the preset's own settings are read.** Settings nested in another, such as
     a Lightroom mask's, are ignored, and the outer one is only noted as present.
   - **Refused:** a file that is not a Camera Raw preset, or that has nothing usable.
   - **Not an exact match:** the two apps process photos differently, so an imported
     preset comes close to its Lightroom look rather than matching it. The import's
     result says so.
   - **Not read:** the older `.lrtemplate` format (Lightroom before 7.3).
3. **Dependency:** `quick-xml` 0.42 reads the XMP.
   - Licence: MIT.
   - It is already in the app, through `tauri` → `plist`, so the desktop binary gains
     no new third-party code. The core crates now compile it too.
   - Parsing by hand would be fragile against XML's escaping and layout, so the parser
     is worth the dependency.
4. **Import:** `import_presets` opens the system's file dialog (preset files and
   `.xmp`, several at once).
   - Each file is imported or reported on its own, with a plain reason.
   - Files are read up to 2 MB; presets are a few kilobytes.
   - The name comes from the file's own name field, or else the file name.
   - Each import is saved like a new preset (its look, sanitised).
5. **Export:** `export_preset` opens the system's save dialog, suggesting the preset's
   name with characters file systems refuse replaced. The file is written whole or not
   at all. Built-in presets can be exported too.
6. **Paths from the webview** are accepted only in self-test mode, as for photo exports.
   Otherwise files are chosen in the system's dialogs.
7. **UI:**
   - **Import…** sits next to Save… in the Presets header. Afterwards, a popover in the
     same style lists what came in and, for Lightroom presets, what was left out. It
     also lists the files that could not be imported, with why, and notes that
     Lightroom looks come close rather than match.
   - **Export…** is in a saved preset's ⋯ menu.
8. **Tests:**
   - **Unit tests:** the file round trip and refusals; the Lightroom mapping on a
     fixture written in Lightroom Classic's layout
     (`tests/fixtures/presets/lightroom-sample.xmp`), including a mask whose own
     `Exposure2012` must not be taken; black-and-white presets; element-style
     settings; entity references in names; foreign files.
   - **Release self-test:**
     - A saved preset is exported to a file and imported back, identical.
     - The Lightroom fixture imports as "Soft & Warm" (Contrast +18) with its
       left-out settings named.
     - A missing file fails on its own.

## Consequences

- Lightroom presets made on raw files usually set white balance in kelvin, which has
  no counterpart here yet. Those presets keep the photo's own white balance and say
  so.
- `.lrtemplate` import can be added later on the same path, if photographers ask for
  it.
