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

## Consequences (if accepted as proposed)

- Subject and People arrive on macOS without adding a model, a runtime or a licence
  to the app.
- Sky waits for the phase 2 benchmark. The design's Sky button stays hidden until
  then (ADR 0016).
- Generated masks add a mask store to the catalogue and a "masks need updating" state
  to the editor.
