# ADR 0050: The export queue

- Status: Accepted (Phase 7, milestone 7)
- Date: 2026-09-30

## Context

Phase 7 requires an export queue (`docs/PRODUCT.md` §36). Until now, Export saved the
open photo as one JPEG through a save dialog.

The design has an export dialog, opened from the top bar's Export and the batch bar's
Export…:
- a header, "Export 12 photos", with a thumbnail and "DSC_0012.NEF and 11 others";
- preset tiles: Web (JPEG, 2048 px), Full quality (TIFF, original), Social (JPEG,
  1350 px) and Print (TIFF, 300 ppi);
- rows for Format, Quality, Size (Original / 2048 px / 1350 px), Colour space and
  Sharpen for;
- switches for Keep metadata, Strip location and Watermark;
- Save to (a folder), a size estimate, Cancel and "Export 12 photos";
- a toast when done: "Exported 12 photos to Lake District".

A full-resolution export holds the decoded photo, about 26 bytes a pixel (§4 of
`docs/PERFORMANCE.md`): 640 MB for 24 MP, 1.3 GB for 61 MP.

## Decision

1. **One queue, one photo at a time** (`export_queue.rs` in the desktop crate).
   - Several exports in parallel would multiply that memory, and decoding already uses
     the background lane's cores.
   - Photos added while it runs join the run.
   - Each running export's cancel token is registered with the quit guard, which also
     counts queued photos. A confirmed quit drops the queue and cancels the running
     export.
   - **Cancel** in the top bar does the same without quitting.
2. **Exports by file** (`Engine::export_file`). A photo need not be open.
   - The open photo exports with its edit as it is now, saved or not.
   - Ticked library photos export with their saved edits, read when queued.
   - Paths from the webview must be in a granted folder, and the open photo is named
     by its image id.
3. **Sized exports** fit the long edge. They decode at the smallest scale that still
   fills it (allowing for the crop), render, then shrink.
   - The shrink is an area average in linear light (`export::resize`), so fine detail
     keeps its brightness. A test checks that alternating black and white averages to
     sRGB 188, not 128.
   - Exports never enlarge.
   - A 1350 px export of the Nikon Z 6 raw takes about 300 ms, against about 930 ms at
     full size.
4. **File names:** the photo's name as `.jpg`, numbered when taken on disk or earlier
   in the run ("DSC_0012-2.jpg"). An export never replaces a file, including a JPEG
   exported next to itself.
5. **Settings remember the choices:** the folder, long edge, quality and preset.
   - The folder is set only through the system's folder dialog (`choose_export_folder`).
     A settings update from the UI can never change it, like the library and backup
     folders.
   - Defaults are the design's Web: 2048 px, quality 85.
   - With no folder chosen yet, Export asks for one first.
6. **Events:** the queue reports `Progress` (done, total, the current file, its
   fraction) and `Finished` (exported, the files written with their sizes, failures
   by file, the folder, whether it was cancelled).
   - The top bar shows "Exporting 3 of 12" with a bar and a stop button, in place of
     Export.
   - A toast says how it ended: "Exported 12 photos to Lake District · 1 photo couldn't
     be exported", "Export cancelled".
7. **The dialog** follows the design with what exists now:
   - the Web and Social presets, and Full quality as a full-size JPEG at 95;
   - Quality (50–100) and Size;
   - Save to, Cancel and Export.
   - Changing Quality or Size by hand clears the preset.
8. **Not yet, and hidden rather than shown doing nothing:**
   - TIFF, and with it the Print preset;
   - colour spaces other than sRGB (with an embedded profile);
   - output sharpening;
   - keeping metadata and stripping location;
   - the watermark;
   - the size estimate.

   Each is its own milestone.
9. **Self-test runs use a fresh temporary settings file.** Export choices are saved
   from the dialog, so a self-test run could otherwise change the user's settings.
10. **Self-test** (release, real queue, engine and files):
    - Three raw photos export at 1350 px into the temporary folder, one after another.
      All three are 1350 px on the long edge, with 15 progress events and about 300 ms
      a photo.
    - A full-size run of all eight, cancelled at its first progress event, stops and
      says so.

## Deviations from the design

Recorded in ADR 0016:
- three presets instead of four, with Full quality a JPEG (a 16-bit TIFF since ADR
  0057, which also added the Format row);
- the Format, Colour space, Sharpen for and switch rows, and the estimate, are left out
  until they work;
- progress in the top bar;
- Cancel while exporting.
