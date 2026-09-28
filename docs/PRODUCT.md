# Professional Photo Editor

## 01 - Product

### 1.1 Product vision

Build a desktop photo editor for hobbyist and enthusiast photographers that combines:

- The professional editing capabilities photographers expect from Lightroom-class software
- A significantly simpler and more intuitive interface
- Extremely fast interaction and rendering
- Local-first storage and processing
- A non-destructive editing model
- No mandatory cloud photo storage
- No requirement for photographers to understand technical image-processing concepts

The product should feel approachable immediately while remaining capable enough that an experienced photographer does not feel constrained.

### 1.2 Core proposition

> Professional photo editing that feels simple.

### 1.3 Target user

Primary user:

A hobbyist or enthusiast photographer who:

- shoots RAW
- owns a mirrorless or DSLR camera
- edits photographs regularly but does not make photography their primary income
- wants professional-quality results
- does not want to spend significant time learning complicated software
- may use an inexpensive or older computer
- prefers to own their photographs locally
- dislikes mandatory subscriptions

Secondary user:

A professional photographer who wants a fast, lightweight editor alongside their existing professional workflow.

### 1.4 Product principles

#### Simple, not limited

Hide complexity rather than removing capability.

Example:

A user should be able to make a photograph warmer with a single obvious control.

An advanced user should still be able to access precise white-balance temperature and tint controls.

#### The photograph is the centre

The interface exists to help the user work on the photograph.

The application should never feel like a database, spreadsheet or engineering tool.

#### Fast by architecture

Performance must not be treated as a future optimisation phase.

The application must be designed around:

- asynchronous processing
- caching
- multi-resolution previews
- incremental rendering
- parallel CPU processing
- GPU acceleration where useful
- bounded memory usage

#### Local first

Original photographs remain on the user's filesystem.

The application should function without an internet connection.

Cloud functionality may be added later, but the core application must never depend on a server.

#### Non-destructive

Never modify the original photograph during editing.

Edits are stored as instructions describing how the photograph should be rendered.

#### Progressive disclosure

The default interface should expose the controls most photographers commonly need.

Advanced functionality should be accessible without cluttering the primary workflow.

---

# 02 - Product scope

## 2.1 Core workflows

The application must optimise for these workflows.

### Workflow A - Browse

1. Open a folder
2. Generate thumbnails
3. Scan photographs quickly
4. Rate/flag photographs
5. Select photographs
6. Open a photograph for editing

### Workflow B - Edit one photograph

1. Open photograph
2. Adjust exposure
3. Adjust colour
4. Crop/straighten
5. Apply local adjustments
6. Retouch if required
7. Compare before/after
8. Save automatically

### Workflow C - Edit many photographs

1. Select photographs
2. Edit one photograph
3. Copy adjustments
4. Apply to selection
5. Make individual corrections
6. Export selection

### Workflow D - Import a shoot

1. Choose folder
2. Application indexes files
3. Metadata is extracted
4. Thumbnails are generated
5. Previews are generated in background
6. UI remains responsive throughout

### Workflow E - Export

1. Select photographs
2. Choose export destination
3. Choose format
4. Choose quality/size
5. Export in background
6. Continue using application
7. Display progress/errors

---

# 03 - Feature specification

## 3.1 Library

### V1

- Folder browser
- Recursive folder indexing
- Thumbnail grid
- Single-image view
- Multi-select
- Ratings 0-5
- Pick/reject flags
- Colour labels
- Search
- Sort
- Filter
- Recently edited
- Recently imported
- Favourites
- Collections/albums implemented as database references rather than copies

### Later

- Smart collections
- Stacks
- Face recognition
- Advanced metadata search
- Duplicate detection
- Similar-image detection

---

## 3.2 Supported file formats

### V1 input

- JPEG
- PNG
- TIFF
- DNG
- Major camera RAW formats

RAW support must be implemented through a dedicated RAW abstraction rather than hard-coding individual camera formats into the application.

LibRaw should initially be evaluated as the RAW decoder because it is explicitly intended for use inside RAW converters and supports a very broad range of camera RAW formats.

RawSpeed should be investigated as a future high-performance decoding path because its purpose is fast RAW decoding and it is already used in the darktable ecosystem.

### V1 output

- JPEG
- PNG
- TIFF

### Later

- WebP
- AVIF
- DNG export
- PSD interoperability where practical

---

# 04 - Editing system

## 4.1 Non-destructive edit model

Every photograph has:

```text
Original File
    +
Edit Recipe
    =
Rendered Photograph
```

The original file must never be overwritten by normal editing operations.

The edit recipe contains all adjustment parameters.

Example:

```json
{
  "exposure": 0.7,
  "contrast": 12,
  "highlights": -30,
  "shadows": 18,
  "temperature": 5400,
  "tint": 7,
  "vibrance": 14
}
```

The actual implementation may differ.

The important architectural rule is:

> Editing changes data describing the image, not the original image itself.

---

## 4.2 Adjustment categories

### Light

- Exposure
- Contrast
- Highlights
- Shadows
- Whites
- Blacks
- Brightness

### Tone

- Tone curve
- RGB curves
- Parametric curve
- Black point
- White point

### Colour

- Temperature
- Tint
- Vibrance
- Saturation

### Colour mixer

- Red
- Orange
- Yellow
- Green
- Aqua
- Blue
- Purple
- Magenta

Each colour supports:

- Hue
- Saturation
- Luminance

### Colour grading

- Shadows
- Midtones
- Highlights
- Global
- Blending
- Balance

### Presence

- Texture
- Clarity
- Dehaze

### Detail

- Sharpening
- Radius
- Detail
- Masking
- Luminance noise reduction
- Colour noise reduction

### Optics

- Lens correction
- Chromatic aberration
- Vignetting
- Distortion

### Geometry

- Crop
- Rotate
- Straighten
- Perspective
- Horizontal transform
- Vertical transform
- Aspect ratio

### Effects

- Vignette
- Grain

---

# 05 - Local adjustments

Local editing is a core professional feature and must not be an afterthought.

## 5.1 V1 masks

- Brush
- Linear gradient
- Radial gradient

Every mask must support:

- Add
- Subtract
- Invert
- Feather
- Density/opacity
- Multiple adjustment parameters

Example:

```text
Mask 01
    Linear gradient
    Exposure +0.5
    Highlights -15
    Temperature +200

Mask 02
    Brush
    Exposure -0.3
    Clarity +10
```

## 5.2 Future intelligent masks

- Subject
- Sky
- Background
- People
- Face
- Hair
- Clothing
- Landscape
- Architecture
- Colour range
- Luminance range

AI masks must ultimately be represented using the same generic mask system as manually created masks.

The AI component should generate a mask.

The renderer should not care where the mask came from.

This keeps the architecture clean.

---

# 06 - Retouching

## V1

- Heal
- Clone
- Spot removal
- Red-eye

## Later

- Content-aware remove
- Object removal
- Face retouch
- Skin smoothing
- Teeth/eye enhancement

Retouching should operate on the non-destructive edit graph rather than destructively modifying the source image.

---

# 07 - UX / information architecture

## 7.1 Primary application structure

The application should have four primary modes:

```text
LIBRARY
EDIT
EXPORT
SETTINGS
```

The user should not need to understand these as separate technical subsystems.

The transition between Library and Edit should feel instantaneous.

## 7.2 Editor layout

Desktop:

```text
┌──────────────────────────────────────────────────────┐
│ Library       Edit                         Export    │
├──────────────┬───────────────────────────┬───────────┤
│              │                           │           │
│ Collections  │                           │ Light     │
│ Folders      │                           │ Colour    │
│ Filters      │        PHOTOGRAPH         │ Detail    │
│              │                           │ Geometry  │
│              │                           │ Selective │
│              │                           │ Retouch   │
│              │                           │           │
├──────────────┴───────────────────────────┴───────────┤
│   241 photos       ◀   17 selected   ▶               │
└──────────────────────────────────────────────────────┘
```

The exact layout should evolve through UX prototyping.

## 7.3 Progressive disclosure

Default:

```text
Light
Colour
Detail
Geometry
Selective
Retouch
```

Advanced controls become available inside each group.

For example:

```text
Colour

Temperature     ──────────●────
Tint            ─────●────────

Advanced ▸
```

Opening Advanced reveals:

```text
Temperature
Tint
White Balance Mode
Camera Profile
Rendering Intent
```

The application should never force a beginner to understand the advanced model.

## 7.4 Direct manipulation

Where appropriate, controls should be usable directly on the photograph.

Examples:

- dragging crop boundaries
- dragging straightening guides
- painting masks
- moving gradients
- clicking a colour in the image to identify/select it

---

# 08 - Rendering architecture

This is the most important technical subsystem.

The application must NOT treat rendering as:

```text
Load full RAW
→ Process every pixel
→ Send complete image to UI
```

Instead it should use a multi-resolution pipeline.

## 8.1 Image pyramid

For a 24MP image:

```text
Original
6000 x 4000

Level 0
6000 x 4000

Level 1
3000 x 2000

Level 2
1500 x 1000

Level 3
750 x 500

Level 4
375 x 250

Thumbnail
```

The actual dimensions should be dynamically determined.

## 8.2 Render modes

### Thumbnail render

Very cheap.

Used in Library.

### Interactive render

Medium resolution.

Used while editing.

Must prioritise latency.

### Detail render

Higher resolution.

Used when zooming.

### Export render

Full resolution.

Used only when exporting.

These paths should share the same underlying rendering logic but have different quality/resolution requirements.

---

# 09 - Render graph

The rendering system should be designed as a graph/pipeline rather than a collection of independent image-processing functions.

Conceptually:

```text
RAW decode
    ↓
Linear raw buffer
    ↓
Camera processing
    ↓
White balance
    ↓
Demosaic
    ↓
Exposure
    ↓
Tone
    ↓
Colour
    ↓
Local adjustments
    ↓
Detail
    ↓
Lens correction
    ↓
Geometry
    ↓
Crop
    ↓
Display transform
    ↓
Output
```

The implementation should be capable of:

- eliminating unnecessary copies
- reusing buffers
- reordering safe operations
- combining compatible operations
- caching intermediate stages
- rendering only changed regions where possible

The frontend must never know the internal rendering implementation.

---

# 10 - Performance architecture

Performance is a product requirement.

## 10.1 UI thread

The UI thread must never perform:

- RAW decoding
- full-resolution image processing
- large file reads
- export processing
- AI inference
- thumbnail generation for large batches

The UI must remain responsive while these operations run.

## 10.2 Job system

Rust should manage a central background job system.

Conceptually:

```text
High Priority
    ↓
Current interactive render

Medium Priority
    ↓
Visible thumbnails
    ↓
Visible previews

Low Priority
    ↓
Background indexing
    ↓
Remaining thumbnails
    ↓
Remaining previews
```

Jobs should be cancellable.

Example:

User drags Exposure:

```text
Render A started
Render B started
Render C started
Render D started
```

If D is the latest request:

```text
Cancel A
Cancel B
Cancel C
Execute D
```

The renderer should avoid wasting CPU on obsolete renders.

---

# 11 - GPU architecture

Use Rust as the native rendering layer.

`wgpu` should be evaluated for cross-platform GPU rendering because it provides native backends including Metal, Direct3D 12, Vulkan and OpenGL.

The renderer must support:

```text
GPU path
    OR
CPU path
```

GPU acceleration must never be a hard dependency.

CPU fallback is mandatory.

The application should detect:

- available GPU
- available VRAM
- supported backend
- driver capability
- performance characteristics

and select an appropriate rendering strategy.

Do not blindly assume GPU acceleration is faster.

---

# 12 - CPU processing

Use Rust-native processing and parallelism.

CPU processing should be:

- multi-threaded
- cache-conscious
- allocation-aware
- vectorisation-friendly
- cancellable

Rayon should be evaluated for parallel workloads.

Avoid creating large temporary allocations inside per-pixel loops.

---

# 13 - Memory management

Memory usage must be bounded.

Never keep all full-resolution photographs in memory.

Use:

- memory budget
- LRU caches
- preview eviction
- tile-based processing where required
- lazy loading

Example:

```text
Memory budget: dynamic

Visible image:
    high priority

Nearby images:
    medium priority

Off-screen images:
    low priority

Unused previews:
    evict
```

The application must remain usable on machines with limited RAM.

---

# 14 - Cache architecture

Use a dedicated cache manager.

Conceptually:

```text
photo identity
+
render recipe hash
+
render level
+
renderer version
=
cache key
```

Caches:

```text
thumbnail
preview
histogram
mask
AI result
render stage
```

If any dependency changes:

```text
old cache invalid
new cache generated
```

Changing Exposure should not necessarily invalidate unrelated expensive processing.

---

# 15 - Histogram architecture

Histogram generation should be implemented in the native rendering layer.

Required:

- RGB histogram
- luminance histogram
- clipping indicators
- optional per-channel histograms

Histogram updates must be fast enough to update during interactive editing.

---

# 16 - Colour management

The internal pipeline should be designed around a high-quality colour-managed workflow.

Do not build the application around 8-bit RGB buffers.

Internal processing should use sufficiently high precision, with linear-light processing where appropriate.

The display pipeline should be separated from the editing pipeline.

Little CMS should be evaluated for ICC colour-management operations because it is specifically designed for portable ICC transforms and colour-management workflows.

The exact colour architecture must be validated with reference images before production.

---

# 17 - Metadata architecture

Metadata must be treated independently from image editing.

Read and index:

- EXIF
- camera
- lens
- focal length
- aperture
- shutter speed
- ISO
- date/time
- GPS where available
- rating where available
- orientation

Maintain application metadata separately:

- rating
- flag
- colour
- keywords
- albums
- edit recipe

Do not modify original RAW metadata during ordinary editing operations.

Export can optionally embed metadata in generated files.

Metadata implementation must be placed behind an abstraction so the underlying library can be replaced later.

Avoid making Exiv2 a core dependency without first completing a licensing review because Exiv2 is GPLv2.

---

# 18 - Data architecture

Use SQLite.

SQLite is appropriate because the application is local-first and requires a transactional embedded database without a separate server process. It is also designed to be self-contained and zero-configuration.

Suggested entities:

```text
photos
files
folders
albums
collections
ratings
flags
keywords
metadata
edit_recipes
edit_history
presets
exports
```

Example relationship:

```text
Photo
 ├── File
 ├── Metadata
 ├── Edit Recipe
 ├── Ratings
 ├── Keywords
 └── Albums
```

The database contains application state, not the original photographs.

---

# 19 - File identity

File paths alone must not be treated as permanent identity.

A file identity system should account for:

- canonical path
- file size
- modification timestamp
- stable content fingerprint/hash

A moved file should ideally be recoverable without treating it as a completely new photograph.

---

# 20 - Edit recipe versioning

Edit recipes must be versioned.

Example:

```text
recipe_version = 1
```

Future application releases may change the rendering algorithm.

The application must therefore distinguish:

```text
Recipe version
Renderer version
```

This allows old photographs to remain editable even as the rendering engine evolves.

---

# 21 - Presets

A preset is simply an edit recipe template.

Required operations:

- Create preset
- Apply preset
- Update preset
- Delete preset
- Import preset
- Export preset

Preset application must remain non-destructive.

---

# 22 - Copy/paste editing

User selects image A:

```text
Copy Adjustments
```

Selects images:

```text
B C D E F
```

Then:

```text
Paste Adjustments
```

The application should allow selective copying:

```text
☑ Light
☑ Colour
☑ Detail
☐ Crop
☐ Geometry
☑ Masks
☐ Retouch
```

---

# 23 - Undo / history

Undo must operate on application state rather than storing full image copies.

Bad:

```text
Image 1
Image 2
Image 3
...
```

Good:

```text
Exposure +0.4
Exposure +0.5
Contrast +10
Crop changed
Mask changed
```

History should therefore be compact.

---

# 24 - AI architecture

AI is an enhancement to the editing system, not the foundation.

Potential AI features:

- Subject detection
- Sky detection
- People detection
- Face detection
- Background detection
- AI denoise
- Object removal

Inference should be local wherever practical.

ONNX Runtime should be evaluated because it supports multiple hardware execution paths, including CPU and platform-specific accelerators.

The AI layer must expose generic operations such as:

```text
generate_mask()
denoise()
detect_subject()
remove_object()
```

It must not leak model-specific implementation details into the rest of the application.

---

# 25 - Desktop architecture

## Frontend

```text
Tauri 2
React
TypeScript
Vite
```

The frontend is responsible for:

- application UI
- navigation
- interaction
- visual state
- panels
- controls
- keyboard shortcuts

It is NOT responsible for heavy image processing.

## Native layer

```text
Rust
```

Responsible for:

- filesystem
- SQLite
- metadata
- RAW decoding
- rendering
- caching
- background jobs
- export
- image processing
- GPU
- AI integration

Tauri provides the bridge between the webview frontend and Rust backend.

---

# 26 - IPC/API boundary

The frontend should communicate with Rust through a deliberate application API.

Example:

```text
open_photo(id)

get_thumbnail(id)

get_preview(id, render_config)

update_edit(id, edit)

copy_edits(id)

paste_edits(ids)

create_mask(id, mask)

export_photos(ids, config)

search_photos(query)
```

Do not expose arbitrary filesystem or renderer internals directly through IPC.

The boundary should be typed.

---

# 27 - Suggested repository structure

```text
photo-editor/
│
├── apps/
│   └── desktop/
│       ├── src/
│       │   ├── app/
│       │   ├── components/
│       │   ├── features/
│       │   ├── hooks/
│       │   ├── lib/
│       │   └── state/
│       │
│       └── package.json
│
├── crates/
│   ├── app-core/
│   ├── catalogue/
│   ├── metadata/
│   ├── raw/
│   ├── image-core/
│   ├── renderer/
│   ├── masks/
│   ├── cache/
│   ├── export/
│   ├── ai/
│   └── platform/
│
├── tests/
│   ├── rendering/
│   ├── raw/
│   ├── performance/
│   └── fixtures/
│
├── docs/
│   ├── ARCHITECTURE.md
│   ├── RENDERING.md
│   ├── PERFORMANCE.md
│   ├── DATA_MODEL.md
│   └── ADR/
│
├── CLAUDE.md
└── README.md
```

---

# 28 - Architecture rules for Claude Code

Claude must follow these rules throughout development.

## Rule 1

Do not place heavy image processing in TypeScript/React.

## Rule 2

Do not modify original photographs.

## Rule 3

Do not store full-resolution image copies for every edit.

## Rule 4

Do not block the UI while processing.

## Rule 5

Do not introduce a dependency without checking:

- license
- platform support
- maintenance
- performance
- binary size

## Rule 6

Do not prematurely optimise by making architecture incomprehensible.

Keep boundaries explicit.

## Rule 7

All performance-critical systems require benchmarks.

## Rule 8

Every major subsystem must have tests.

## Rule 9

Do not implement speculative features before the rendering architecture is stable.

## Rule 10

When architecture changes, update the appropriate ADR/documentation.

---

# 29 - Development phases

## Phase 0 - Architecture spike

Do NOT start by building the complete application.

Build a tiny technical prototype that answers:

1. Can Tauri + React communicate cleanly with Rust?
2. Can Rust decode representative RAW files?
3. Can a rendered image be displayed efficiently?
4. Can the renderer achieve acceptable interactive latency?
5. Can CPU and GPU paths coexist?
6. Can thumbnails be generated quickly?
7. Can the cache behave correctly?

The prototype should use real photographs.

Use:

- Nikon NEF
- Canon CR3
- Sony ARW
- Fuji RAF
- DNG
- JPEG

Do not proceed to large-scale UI implementation until this prototype works.

---

# 30 - Phase 1 - Desktop shell

Build:

- application window
- navigation
- file picker
- folder browser
- React/Rust IPC
- basic settings
- application lifecycle
- logging
- error reporting

No advanced editing.

---

# 31 - Phase 2 - Catalogue

Build:

- folder indexing
- SQLite database
- metadata extraction
- thumbnail generation
- image grid
- ratings
- flags
- filtering
- persistence

Goal:

A user can point the application at a photography folder and browse it quickly.

---

# 32 - Phase 3 - Rendering engine

This is the major engineering milestone.

Build:

- RAW abstraction
- RAW decoding
- image buffers
- colour pipeline
- resolution pyramid
- CPU renderer
- render graph
- cache manager
- interactive renderer

Goal:

A photograph can be opened and rendered rapidly.

---

# 33 - Phase 4 - Editing

Implement:

- Exposure
- Contrast
- Highlights
- Shadows
- Whites
- Blacks
- WB
- Vibrance
- Saturation
- Tone curve
- Colour mixer
- Clarity
- Texture
- Dehaze

Goal:

Produce genuinely useful RAW edits.

---

# 34 - Phase 5 - Geometry and detail

Implement:

- Crop
- Rotate
- Straighten
- Transform
- Lens correction
- Sharpening
- Noise reduction
- Vignette
- Grain

---

# 35 - Phase 6 - Masks

Implement:

- Brush
- Linear gradient
- Radial gradient
- Mask combination
- Feathering
- Inversion

This phase should validate that the underlying edit graph is extensible.

---

# 36 - Phase 7 - Workflow

Implement:

- Presets
- Copy/paste
- Batch adjustments
- History
- Before/after
- Rating workflow
- Export queue

At this point the product becomes a usable Lightroom-class MVP.

---

# 37 - Phase 8 - GPU

Move appropriate renderer operations to GPU.

Do not rewrite the entire renderer.

GPU code should implement operations against an already-established rendering abstraction.

Benchmark:

```text
CPU
vs
GPU
```

for:

- exposure
- tone
- colour
- blur
- masks
- sharpening
- transformations

Only use GPU paths where they provide measurable benefit.

---

# 38 - Phase 9 - AI

Implement:

- Subject mask
- Sky mask
- Background mask
- AI denoise
- Object removal

All AI results should feed into the generic image pipeline.

---

# 39 - Phase 10 - Performance hardening

Benchmark against deliberately weak hardware.

Test:

### Library

- 1,000 photographs
- 10,000 photographs
- 50,000 photographs

### RAW

- 24MP
- 45MP
- 60MP+

### RAM

Test constrained memory environments.

### GPU

Test:

- Apple Silicon integrated GPU
- Intel integrated GPU
- low-end Windows GPU
- mid-range Windows GPU
- no usable GPU

### Operations

Measure:

- folder opening
- thumbnail generation
- image opening
- slider interaction
- mask creation
- export
- batch export

---

# 40 - Performance acceptance criteria

Do not claim arbitrary superiority over Lightroom.

Instead establish measurable project targets.

Initial targets:

### UI

- Application remains responsive while rendering.
- No long-running operation executes on the UI thread.

### Thumbnail browsing

- Visible thumbnails appear progressively.
- Background indexing does not make the interface unusable.

### Editing

- Interactive controls update continuously.
- Obsolete render jobs are cancelled where practical.

### Memory

- Memory usage scales with active work rather than total catalogue size.

### Export

- Export continues in background.
- User can continue browsing/editing.

Exact numerical targets should be established after the Phase 0 prototype has produced real baseline measurements.

---

# 41 - Error handling

A photographer should never see:

```text
Rust panic
Segmentation fault
Decoder error code 17
```

Translate technical errors into useful user-facing messages.

Example:

```text
This RAW file could not be decoded.

The file may be damaged or this camera format may not yet be supported.
```

Detailed technical information belongs in logs.

---

# 42 - Reliability

The application handles valuable user data.

Therefore:

- Never modify source photographs accidentally.
- Save edit state transactionally.
- Recover gracefully after crashes.
- Make cache corruption recoverable.
- Make database corruption detectable.
- Never depend on cache contents for permanent application state.

The database should be rebuildable from indexed photographs where practical.

---

# 43 - Autosave

Editing state must be persisted automatically.

The user should not have to press:

```text
Save
```

after every edit.

The conceptual model is:

```text
Change slider
    ↓
Update in-memory recipe
    ↓
Render
    ↓
Persist recipe asynchronously
```

---

# 44 - Settings

Settings should include:

### Performance

- GPU acceleration
- Memory limit
- Cache size
- Preview quality
- Background processing intensity

### General

- Theme
- Language
- Keyboard shortcuts

### Library

- Default folders
- Import behaviour
- Metadata behaviour

### Export

- Default format
- Default quality
- Metadata defaults

---

# 45 - Platform strategy

Initial target:

```text
Windows
macOS
```

Linux should be considered after the core application is stable.

The architecture must avoid unnecessary platform-specific assumptions.

Platform-specific code belongs behind the Rust `platform` abstraction.

---

# 46 - Testing strategy

## Unit tests

- Edit recipes
- Colour transformations
- Mask mathematics
- Geometry
- Cache keys
- Database operations

## Integration tests

- Import photograph
- Generate thumbnail
- Generate preview
- Apply edit
- Persist edit
- Reopen photograph
- Export

## Golden image tests

Maintain reference photographs.

Example:

```text
input.nef
+
recipe.json
=
expected-render.png
```

Rendering changes must be compared against expected output.

This is essential for a serious image-processing application.

---

# 47 - Performance tests

Every renderer change should be benchmarkable.

Benchmarks should measure:

```text
Decode
Demosaic
Exposure
Colour
Mask
Sharpen
Noise reduction
Full render
Export
```

Record:

- execution time
- memory usage
- CPU utilisation
- GPU utilisation
- allocation count where practical

Maintain representative photographs from multiple cameras.

---

# 48 - Dependency philosophy

Prefer:

```text
Small
Mature
Well-maintained
Portable
Well-licensed
```

Avoid accumulating dozens of dependencies because they are convenient.

Every dependency must have a reason.

Before adding native libraries, verify their redistribution and licence implications.

---

# 49 - Distribution

Eventually support:

### Windows

- Signed installer
- Auto-update
- Uninstall
- File associations

### macOS

- `.dmg`
- Code signing
- Notarisation
- Auto-update
- File associations

Do not make distribution part of Phase 1.

First make the application technically sound.

---

# 50 - V1 definition

V1 is complete when a photographer can:

```text
Open folder
    ↓
Browse RAW photographs
    ↓
Pick/reject/rate
    ↓
Open photograph
    ↓
Edit exposure/colour/tone/detail
    ↓
Crop/straighten
    ↓
Create local adjustments
    ↓
Copy edits
    ↓
Batch edit
    ↓
Export
```

without needing another application for the normal RAW editing workflow.

The application should feel:

```text
Fast
Simple
Predictable
Local
Professional
```

The goal is not to reproduce every feature Adobe has.

The goal is to reproduce the **useful photographic capability** while creating a significantly better interaction model.

---

# 51 - What NOT to build initially

Do not start with:

- Cloud accounts
- Cloud photo storage
- Social features
- Collaboration
- Mobile app
- Video editing
- AI chat
- Online galleries
- Printing
- Tethering
- Advanced DAM features
- Subscription infrastructure

The foundation is:

```text
RAW
+
Renderer
+
Non-destructive edits
+
Library
+
Fast UI
```

Everything else depends on those.

---

# 52 - The first engineering objective

The first version of the project should NOT look like Lightroom.

It should look like a technical prototype.

The prototype should answer:

> Can we open a modern RAW file, render it correctly, modify it interactively, and keep the interaction feeling immediate on weak hardware?

Until that question has a good answer, UI polish is secondary.

---

# 53 - Development philosophy for Claude Code

Claude Code should work in small milestones.

For every milestone:

```text
1. Read architecture
2. State implementation approach
3. Implement smallest useful increment
4. Add tests
5. Run tests
6. Run benchmarks where applicable
7. Review architecture
8. Update documentation
```

Claude must not silently redesign major subsystems.

Large architectural changes require an ADR.

Example:

```text
docs/ADR/0001-render-pipeline.md
docs/ADR/0002-cache-system.md
docs/ADR/0003-raw-decoder.md
```

---

# 54 - Final architecture

Target architecture:

```text
                    ┌──────────────────────┐
                    │       React UI       │
                    │                      │
                    │ Library / Editor     │
                    │ Controls / Panels    │
                    └──────────┬───────────┘
                               │
                         Typed IPC
                               │
                    ┌──────────▼───────────┐
                    │     Rust App Core    │
                    ├──────────────────────┤
                    │ Catalogue            │
                    │ Edit State           │
                    │ Job System           │
                    │ Metadata             │
                    │ Cache                │
                    │ Export               │
                    └──────────┬───────────┘
                               │
              ┌────────────────┼─────────────────┐
              │                │                 │
              ▼                ▼                 ▼
        ┌───────────┐   ┌──────────────┐  ┌─────────────┐
        │ RAW Layer │   │ Render Engine│  │ AI Engine   │
        │           │   │              │  │             │
        │ LibRaw    │   │ CPU + GPU    │  │ ONNX        │
        │ RawSpeed* │   │ Render Graph │  │ Local       │
        └───────────┘   └──────┬───────┘  └─────────────┘
                               │
                         ┌─────▼─────┐
                         │   Cache   │
                         └───────────┘

                       SQLite
                         │
                         ▼
                   Application State

                 Original photographs
                     remain local
                   and untouched
```

`* RawSpeed should remain an evaluated optimisation path rather than an assumed mandatory dependency.`

# 55 - Product north star

The test for every feature is:

> **Does this make professional photographic editing easier without making the application harder to understand?**

The test for every architectural decision is:

> **Does this preserve correctness, responsiveness, portability and the ability to improve the renderer later?**

The test for performance is:

> **Does it remain pleasant to use on hardware we did not design around?**
