# ADR 0074: Subject, People and Sky masks (proposal)

- Status: **Accepted for phase 1** (Subject and People on macOS through Apple
  Vision). Phase 2 (Sky, and other platforms) waits for its benchmark.
- Date: 2026-10-08

## Context

The design's Masks panel offers **Subject** and **Sky** next to Brush, Linear and
Radial, and PRODUCT.md §5.2 and Phase 9 list Subject, Sky, Background and People.
These are the largest Lightroom-parity gap left. CLAUDE.md §16 and PRODUCT.md §24
set the constraints:

- **Optional and local:** AI is an optional subsystem, with local inference.
- **Generic output:** it returns generic primitives (a mask), which the renderer
  treats like any other mask (ADR 0040).
- **No GPL:** no GPL native libraries, and every new dependency evaluated for
  licence, size and maintenance (§17).
- **Deterministic edits:** edits must be deterministic and portable (§13).

This ADR is the research step. Nothing is added to the app until the decisions
below are made.

## What was found

### The licence question is the training data, not the code

Most segmentation projects publish their code under MIT or Apache 2.0. The
**weights**, though, are trained on datasets whose terms are research-only or
unclear. Shipping such weights in a product is a legal risk that a permissive code
licence doesn't remove.

| Candidate | Purpose | Code / weights | Training data | Notes |
|---|---|---|---|---|
| **Apple Vision** `VNGenerateForegroundInstanceMaskRequest` | Subject (class-agnostic, instance masks, soft edges) | System API: nothing shipped | Apple's (not our concern) | macOS 14+. Runs on the Neural Engine. Rust bindings: `objc2-vision` |
| **Apple Vision** `VNGeneratePersonSegmentationRequest` | People | System API | Apple's | macOS 12+. Quality levels accurate, balanced and fast. The mask comes at its own resolution |
| U²-Net / u2netp | Subject (salient object) | Reported Apache 2.0 (unconfirmed; one source says MIT) | DUTS: no licence; mirrors say research-only | 4.7 MB (u2netp) to 176 MB. Fixed 320 px input |
| BiRefNet | Subject, high resolution | MIT | DIS5K, DUTS: research terms | 221 M parameters. Official ONNX exists but uses deformable convolution, which the ONNX exporters don't support (workarounds needed) |
| BRIA RMBG 1.4 / 2.0 | Subject | Non-commercial licence | Licensed data | **Excluded:** not for commercial use |
| SAM (Segment Anything) | Any object, given a point or box | Apache 2.0, code and weights | SA-1B (research licence; Meta released the weights Apache 2.0) | Class-agnostic: it needs a prompt, so it doesn't know "sky" |
| MobileSAM / EfficientSAM / EfficientViT-SAM | SAM, small | Apache 2.0 (MobileSAM: one ONNX mirror says AFL-3.0) | Distilled from SAM | MobileSAM about 45 MB ONNX (encoder plus decoder), about 10 M parameters |
| SAM 2 | As SAM, and video | Apache 2.0 | SA-V (CC BY 4.0) | One source says the Hiera backbone weights are CC BY-NC: unconfirmed, so avoid until checked |
| SegFormer (ADE20K) | Semantic, including a sky class | NVIDIA Source Code Licence | ADE20K | **Excluded:** both the weights and ADE20K are non-commercial |
| Other ADE20K-trained models | Semantic (sky) | Various | ADE20K: non-commercial research and education only | Sky is ADE20K class 2. Its terms carry over to models trained on it |
| `skyseg` (Parskatt; JianyuanWang on Hugging Face) | Sky | No licence found | Unknown | 176 MB ONNX. Its author calls it conservative; it doesn't handle clouds |

**What this means:**
- **Subject and People:** on macOS, Apple Vision avoids the whole question; nothing
  is shipped or licensed.
- **Sky:** there is **no off-the-shelf sky model with clean terms**. Every capable
  one is trained on ADE20K or carries NVIDIA's licence.
- **Windows:** there is no system segmentation API (Windows ML only runs your own
  ONNX models; Paint's background removal is not an API). Windows and Linux will need
  a shipped model for Subject too.

### Runtimes (for any shipped model)

- **ONNX Runtime via `ort`:**
  - **Licence and reach:** MIT (bindings MIT/Apache); CPU, CoreML, DirectML and CUDA.
  - **Drawbacks:** a native library (the macOS arm64 package is about 7 MB
    compressed), and its build and dylib linking need care on macOS.
- **tract:**
  - **Licence:** pure Rust, MIT/Apache.
  - **Strengths:** no native dependency; CPU kernels hand-tuned.
  - **Limits:** speed unproven against ONNX Runtime for these models; it misses some
    operators; one low-severity crash on crafted models was fixed in 0.23.5.
- **Apple Vision:** no runtime of ours at all.

PRODUCT.md §24 says ONNX Runtime "should be evaluated". tract should be benchmarked
beside it on the chosen model before a native library is added (CLAUDE.md §7: no
assumption that the heavier option is faster).

### How others store generated masks

Lightroom Classic keeps AI masks in a separate data file next to the catalogue
(`.lrcat-data`), not in the edit. Deleting it makes Lightroom rebuild the masks
("Update AI"). The edit records which mask to make, and the pixels are a
regenerable result.

## Proposal

1. **A mask kind for generated masks** (ADR 0040's generic mask system).
   - **The kind:** `MaskShape::Generated { kind: Subject | People | Sky, generator,
     generator_version, mask: MaskRef }`. Its coverage is a stored raster, sampled
     like a brush mask's coverage map. The renderer doesn't know how it was made.
   - **Combining:** shapes combine with Add, Subtract and Intersect as today, so
     "Sky, minus Brush" works.
2. **Generated masks are stored, not recomputed per render.**
   - **What's stored:** the coverage, at up to 2048 px, compressed (tens of KB), in
     the catalogue's mask store. It's keyed by the photo's identity (ADR 0012), the
     generator and its version.
   - **Why:** recomputing is slow, and the result would change when the OS model
     updates. Edits must not change silently (§13).
   - **When it's missing** (a moved catalogue, a new machine): regenerate it and say
     so, as Lightroom's "Update AI" does.
   - **Exports and the Library** use the stored mask.
3. **Phase 1, macOS: Subject and People through Apple Vision.**
   - **Where:** a new `ai` crate behind a `Segmenter` trait (`segment(image, kind) ->
     Coverage`), with a macOS backend via `objc2-vision`.
   - **What's shipped:** no model, no ONNX Runtime, no licence question.
   - **Where it runs:** on the background lane, on the pyramid's base (about
     3000 px).
   - **Elsewhere:** Subject and People are hidden on platforms without a backend.
4. **Phase 2: Sky, and Subject on other platforms.** Benchmark before choosing:
   - **(a) SAM-family (MobileSAM or EfficientSAM, Apache 2.0) with automatic
     prompts.** Points placed by a sky heuristic (bright, low saturation or blue,
     connected to the top edge, smooth texture) give SAM what to segment, and its
     masks follow edges such as tree lines well. The same model can serve Subject on
     Windows and Linux (a saliency point or the photo's centre as the prompt), and
     click-to-select later.
   - **(b) A classical sky mask:** the same heuristic as a coverage map, refined with
     the guided filter we already have (ADR 0023's base map). No model at all; weaker
     on tree edges and cloudy skies.
   - **(c) Training our own sky model** on data whose terms allow it. Most effort;
     only if (a) and (b) fall short.
5. **Tests:**
   - **Determinism:** a stored mask renders identically after reopening.
   - **Missing masks:** a missing mask is regenerated and reported.
   - **Combining:** masks combine with other shapes.
   - **Golden images** use a fixed stored mask, so they don't depend on the OS model.
   - **Benchmarks:** time to mask on each camera fixture.

## Decisions (2026-10-08)

1. **Licensing policy: strict.** Model weights are acceptable only if their training
   data also allows commercial use. That rules out U²-Net, BiRefNet and every
   ADE20K-trained model, and leaves Apple Vision and the SAM family (Apache 2.0
   weights, distilled from SAM).
2. **macOS first through Apple Vision:** yes. Subject and People ship on macOS
   first; Windows and Linux follow in phase 2.
3. **Benchmark downloads: deferred to phase 2.** Phase 1 needs none. Each download
   will be asked for at the time, with its file, source and size.
4. **A native ONNX Runtime library:** still open. To be decided after the phase 2
   benchmark against tract.

## The decisions as they were put

1. **Licensing policy for model weights.** Should only weights whose **training
   data** also allows commercial use be acceptable (strict), or is a permissive
   licence on the weights themselves enough (common practice, but a residual risk)?
   - **Recommendation: strict.** It rules out U²-Net, BiRefNet and every
     ADE20K-trained model, and leaves Apple Vision, and SAM-family models (Meta
     released those weights Apache 2.0).
2. **macOS first through Apple Vision.** Acceptable to ship Subject and People on
   macOS first, with Windows and Linux following in phase 2?
   - **Recommendation: yes.** It's the fastest route to the feature, with no shipped
     model and the best quality available on the platform.
3. **Permission to download candidate models for benchmarking** (phase 2 only). These
   are not shipped; they are for local measurement:
   - MobileSAM ONNX (encoder about 28 MB, decoder about 17 MB);
   - EfficientSAM-S ONNX (size to confirm);
   - optionally the ONNX Runtime macOS arm64 package (about 7 MB compressed).
4. **A native ONNX Runtime library,** if tract proves too slow. To be decided after
   the benchmark, with numbers.

## Phase 1, part 1: the segmenter (2026-10-08)

Built (#70):
- **The `ai` crate:** `Segmenter` (`supports`, `generator`, `segment`) over an 8-bit
  sRGB `Picture`, returning a `Coverage` (0–255 per pixel, sampled by fractions of
  the picture).
- **The macOS backend** (`objc2-vision` 0.3.2 and its sibling crates: Zlib, Apache 2.0
  or MIT; only the headers used are enabled).
  - **Subject:** `VNGenerateForegroundInstanceMaskRequest`, all instances, at the
    picture's size.
  - **People:** `VNGeneratePersonInstanceMaskRequest`, also macOS 14.
  - **Generator string:** `apple-vision/<kind>/r<revision>/macos-<version>`, for
    stored masks to know their model.
- **`Engine::mask_kinds` and `Engine::segment`:** the photo as decoded (oriented, no
  edit, no crop) rendered at the pyramid level of at least 1536 px, segmented on the
  background lane.
- **`bench --segment [DIR]`:** times and shares per camera fixture, and the photo with
  each mask tinted over it.

Two findings changed People:
1. **The older person segmentation always answers with a mask** (it is made for
   video of people). On the Canon's clay animals it marked their heads; it has no
   "nobody" answer.
2. **The person instance request also took the clay animals for people** (40 % of
   the Canon frame), and gave a faint wash on the Z 6 still life. A People mask is now
   kept only when Vision's human detector (`VNDetectHumanRectanglesRequest`) finds
   someone with confidence of at least 0.5. Both are then None.

On the camera fixtures (release build, the model already loaded; PERFORMANCE §53):

| File | Subject | People |
|---|---|---|
| Canon EOS R6 (clay animals) | 77 ms, 38.5 %: all three figures, tight edges | none |
| Fujifilm X-T3 (still life) | 46 ms, 49.8 % | none |
| Nikon Z 6 (still life) | 79 ms, 24.0 %: star, monkey, globe and train | none |
| Ricoh GR III, Sony A7 III, A7R IV (landscapes) | none | none |

Not yet verified: People on a photo with people. None of the fixtures has any, and
downloading photos is deferred like the models. The first mask after launch takes
about 4 s more while macOS loads the model.

## Phase 1, part 2: masks in edits (2026-10-09)

Built:
- **The mask kind:** `MaskShape::Generated { of: Subject | People, mask }`. The
  recipe names the mask; the coverage is in the mask store. Shapes combine with
  Add, Subtract and Intersect like any other, and Density and Invert apply. The
  name is sanitised to hexadecimal digits, so a recipe can't name a path. Recipe
  version 27; older recipes have no generated masks and read unchanged.
- **Coordinates:** the coverage is in the photo's own coordinates (as decoded,
  oriented). The renderer maps each frame pixel back through the geometry
  (`Mapping::source`), so crops, turns, straightening and perspective move the mask
  with the photo. A turn leaves the recipe's shape alone.
- **The mask store:** one 8-bit greyscale PNG per mask in the app's data folder
  (`masks/`), at the size it was made (about 3000 px on the Z 6, 100–130 KB), written
  atomically. The 8 most recent are held decoded. Self-tests use a temporary folder.
- **Names belong to a photo.** A name is the photo's content fingerprint (ADR 0012),
  then a 128-bit hash of the generator and the pixels: 48 hexadecimal digits. A
  mask whose name doesn't start with the photo's fingerprint is treated as missing.
  So a pasted edit, a preset or a synced edit never uses another photo's subject.
- **Update masks.** When a photo opens, and whenever the edit names other generated
  masks (a paste, a preset, undo), the editor asks which the photo can't use
  (`missing_masks`). It makes those again from this photo, renames them in the edit
  (one undo step, "Update masks") and says so: "Mask updated for this photo". It also
  says when the photo has no subject or nobody, or this computer can't make that kind.
  Until then a missing mask adjusts nothing.
- **Exports** of a photo that was never opened with the edit (a batch paste) make
  missing masks from the photo as it exports, at the same size, under the edit's
  names.
- **The UI:** Subject and People first among the Add buttons (toolbar, Selective tiles,
  and the card's Add and Subtract rows), only where this computer makes them. A button
  says "Finding the subject…" while it works, and a toast says when there's none. The
  tint comes from the engine (`mask_view`): the stored mask mapped over the shown
  crop at up to 2048 px, as the renderer maps it, then scaled for zoom and pan. There
  are no handles: a generated mask is edited by combining it with other shapes.
- **Tests:**
  - **Engine:** a stored mask adjusts what it covers, and nothing else.
  - **Missing masks:** an unknown mask is reported and renders as if absent.
  - **Pasted masks:** a mask from another photo is missing there, is made again for
    it, and is made during an export.
  - **Store:** round trip, a fresh store reads from disk, bad names, ownership.
  - **Self-test:** a Subject mask on the Z 6, its effect against its view, and a
    foreign mask reported missing.

Differences from the proposal:
- **Where masks are stored:** files beside the catalogue's data, not inside it. This
  is how Lightroom stores them. It also keeps megabytes of pixels out of SQLite and its
  backups.
- **Key:** the photo's fingerprint plus a content hash, not the photo's identity plus
  the generator and its version. The generator is part of the hash, and a content name
  makes identical masks share one file.
- **Size:** stored at the size it was made, not capped at 2048 px. Fast compression is
  126 KB on the Z 6, against 106 KB for the best (3 ms to write, against 40 ms).

Golden images with a fixed stored mask are still to come. The engine tests use
Vision's masks of a synthetic subject, so they depend on the OS model. They skip on
systems without one.

## Consequences (if accepted as proposed)

- Subject and People arrive on macOS without adding a model, a runtime or a licence
  to the app.
- Sky waits for the phase 2 benchmark. The design's Sky button stays hidden until
  then (ADR 0016).
- Generated masks add a mask store to the catalogue and a "masks need updating" state
  to the editor.
