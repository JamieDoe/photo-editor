# CLAUDE.md

## Project

This repository contains a cross-platform professional RAW photo editor designed for hobbyist and enthusiast photographers.

Core proposition:

> Professional photo editing that feels simple.

The application should provide the capabilities photographers expect from Lightroom-class software while being:

- easier to understand
- faster to use
- local-first
- non-destructive
- performant on low-end hardware
- free from mandatory cloud photo storage

---

# 1. Read this first

Before making changes:

1. Read `docs/PRODUCT.md`
2. Read `docs/ARCHITECTURE.md` if it exists
3. Read the relevant subsystem documentation
4. Inspect the existing implementation
5. Understand the current milestone
6. Do not implement speculative functionality

The documentation is the source of truth for product and architecture decisions.

When implementation and documentation disagree, identify the discrepancy before proceeding.

---

# 2. Core principles

## 2.1 Simple, not limited

The application should hide complexity rather than remove capability.

A beginner should be able to use the common controls immediately.

An advanced photographer should be able to access precise controls when needed.

Prefer progressive disclosure.

---

## 2.2 The photograph is the centre

The UI exists to help users work on photographs.

Avoid interfaces that feel like technical software where the photograph becomes secondary.

Prioritise:

- visual clarity
- obvious actions
- sensible defaults
- minimal friction
- direct manipulation

---

## 2.3 Performance is an architectural requirement

Never treat performance as a final optimisation phase.

Design around:

- asynchronous processing
- background jobs
- multi-resolution previews
- caching
- parallel CPU processing
- GPU acceleration where appropriate
- bounded memory usage
- cancellation of obsolete work

A feature that works but blocks the UI is considered incomplete.

---

## 2.4 Original files are immutable

The application must never modify the user's original photograph during ordinary editing.

The source photograph remains on the user's filesystem.

Edits are represented as data.

Conceptually:

    Original RAW + Edit Recipe = Rendered Image

---

## 2.5 Non-destructive editing

Do not create full-resolution copies for every editing operation.

Store edit parameters, masks, transformations and other metadata required to reconstruct the image.

Undo/history should operate on edit state rather than storing complete image copies.

---

## 2.6 Local-first

The core application must work without an internet connection.

Original photographs remain local.

Do not introduce:

- mandatory accounts
- cloud storage
- remote image processing
- cloud-dependent editing

unless explicitly approved as a product decision.

---

# 3. Architecture

Target architecture:

    Tauri 2
        |
        +-- React + TypeScript UI
        |
        +-- typed IPC
        |
        +-- Rust application core
                |
                +-- catalogue
                +-- RAW
                +-- rendering
                +-- masks
                +-- cache
                +-- metadata
                +-- export
                +-- AI
                +-- platform

The frontend is responsible for presentation and interaction.

The Rust layer is responsible for performance-sensitive and native functionality.

---

# 4. UI rules

The React layer must NOT perform expensive image processing.

Do not perform these operations in the UI thread:

- RAW decoding
- full-resolution image processing
- large file reads
- export processing
- AI inference
- large-scale thumbnail generation

These operations belong in native/background systems.

The UI should communicate with native code through deliberate typed interfaces.

Do not expose arbitrary native internals to the frontend.

---

# 5. Rendering rules

The renderer must support different quality levels.

At minimum:

- thumbnail
- interactive preview
- detail/high-resolution preview
- export/full resolution

Do not render the complete source resolution for every interactive slider change.

Prefer:

- image pyramids
- cached previews
- lazy evaluation
- incremental rendering
- render cancellation
- buffer reuse
- tile-based processing where appropriate

Interactive editing should prioritise responsiveness over maximum rendering quality.

Export should prioritise final image quality.

---

# 6. Rendering architecture

The conceptual pipeline is:

    RAW decode
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
    Lens corrections
        ↓
    Geometry
        ↓
    Crop
        ↓
    Display/output transform
        ↓
    Output

The implementation must not assume each stage requires a separate full-size image allocation.

Optimise for:

- minimal copies
- buffer reuse
- cache reuse
- parallel execution
- GPU execution where beneficial

---

# 7. CPU/GPU rules

The application must support both:

- CPU rendering
- GPU rendering

GPU acceleration must never be mandatory.

Always provide a functional CPU fallback.

Do not assume GPU processing is automatically faster.

Benchmark before moving operations to GPU.

`wgpu` is the preferred technology to evaluate for cross-platform GPU access unless an ADR documents a different decision.

---

# 8. RAW architecture

RAW decoding must exist behind an abstraction.

Do not couple the entire application directly to one decoder.

The renderer should consume a well-defined internal image representation.

LibRaw is the initial RAW decoder candidate.

RawSpeed may be evaluated later for performance.

Do not replace a working decoder merely because another implementation exists. Benchmark first.

---

# 9. Cache rules

Caches are disposable.

Permanent application state must not depend on cache contents.

Cache keys must account for at least:

- source identity
- edit recipe
- render resolution/quality
- relevant renderer version

Use bounded caches and eviction policies.

Never allow the cache to grow without limit.

---

# 10. Job system

Long-running work must execute as background jobs.

Jobs should support:

- priority
- cancellation
- progress
- failure reporting
- deduplication where practical

Interactive render requests have higher priority than background indexing.

Example priority:

    1. Current interactive render
    2. Visible image previews
    3. Visible thumbnails
    4. Background indexing
    5. Remaining previews/thumbnails

When a newer interactive render supersedes an older one, obsolete work should be cancelled whenever practical.

---

# 11. Database rules

SQLite is the application database.

Store application state such as:

- indexed files
- folders
- photographs
- metadata
- ratings
- flags
- keywords
- albums
- edit recipes
- presets
- history
- export jobs

Do not store original photographs inside SQLite.

The database should be recoverable/rebuildable as far as practical from filesystem data.

---

# 12. File identity

Do not treat a file path as the permanent identity of a photograph.

Support a more robust identity model based on things such as:

- canonical path
- file size
- modification time
- content fingerprint/hash

Design the system so a photograph can potentially be recognised after being moved.

---

# 13. Edit recipe rules

Edits must be serialisable.

The recipe must be:

- deterministic
- versioned
- portable
- independent of UI implementation details

Separate:

- recipe version
- renderer version

When rendering behaviour changes, do not silently invalidate existing edits.

Use migrations or compatibility handling where required.

---

# 14. Presets

A preset is an edit recipe template.

Presets must not contain UI-specific state.

The preset system should support:

- create
- apply
- edit
- delete
- import
- export

---

# 15. Masks

Masks are a generic system.

Manual masks and AI masks should eventually produce the same internal mask representation.

Potential mask types:

- brush
- linear gradient
- radial gradient
- colour range
- luminance range
- subject
- sky
- background
- person

The renderer should not care how the mask was generated.

---

# 16. AI

AI is an optional subsystem, not the foundation of the application.

Prefer local inference where practical.

ONNX Runtime should be evaluated for local AI workloads.

Potential features:

- subject selection
- sky selection
- background selection
- AI denoise
- object removal

AI components should return useful primitives such as masks or processed image data rather than owning the editing workflow.

---

# 17. Dependency rules

Before adding a dependency, evaluate:

- licence
- redistribution implications
- maintenance
- platform support
- binary size
- performance
- security
- necessity

Avoid adding dependencies simply because they make one implementation easier.

For native dependencies, verify licensing before integration.

In particular, do not introduce GPL-licensed native libraries into the proprietary application core without an explicit licensing decision.

---

# 18. Code quality

Prefer:

- small modules
- explicit boundaries
- strong typing
- clear naming
- testable functions
- minimal hidden state
- predictable data flow

Avoid:

- giant components
- giant Rust modules
- duplicated business logic
- unnecessary abstractions
- premature generalisation
- tightly coupling UI and rendering code

---

# 19. TypeScript

Use strict TypeScript.

Avoid `any`.

IPC payloads must have explicit types.

Do not duplicate domain models independently in multiple places.

Where possible, generate or share types rather than manually maintaining incompatible copies.

---

# 20. Rust

Use idiomatic Rust.

Run:

- `cargo fmt`
- `cargo clippy`
- tests

Handle errors explicitly.

Do not use panics for expected runtime failures.

Avoid unnecessary allocations in performance-critical paths.

---

# 21. Testing

Every meaningful subsystem needs tests.

Required categories:

### Unit tests

- edit recipes
- calculations
- transformations
- mask operations
- cache keys
- database operations

### Integration tests

- import
- indexing
- thumbnail generation
- rendering
- edit persistence
- reopening
- export

### Golden image tests

Use fixed source images and known edit recipes.

Example:

    fixture.nef
        +
    recipe.json
        =
    expected.png

Rendering changes must be detectable through image comparisons.

---

# 22. Performance testing

Performance-sensitive code requires benchmarks.

Measure at least:

- RAW decode
- preview generation
- render
- mask processing
- export
- thumbnail generation

Record where useful:

- execution time
- memory usage
- CPU utilisation
- GPU utilisation

Benchmark representative RAW files from different cameras and resolutions.

Do not claim that an implementation is "fast" without measurement.

---

# 23. Documentation

Major architectural decisions require an ADR.

Example:

    docs/ADR/0001-render-pipeline.md

Update documentation when architectural behaviour changes.

Do not let documentation describe a system that no longer exists.

---

# 24. Development workflow

Work in small milestones.

For each milestone:

1. Read the relevant documentation.
2. Inspect existing code.
3. State the intended implementation briefly.
4. Implement the smallest complete increment.
5. Add/update tests.
6. Run relevant tests.
7. Run relevant benchmarks.
8. Check for regressions.
9. Update documentation if necessary.

Do not implement multiple unrelated features in one step.

---

# 25. Do not make assumptions

When requirements are genuinely unclear:

- inspect existing documentation
- inspect existing code
- prefer the simplest interpretation consistent with the architecture

Do not invent major product behaviour.

Do not silently introduce accounts, cloud services, analytics, subscriptions or external APIs.

---

# 26. Current development priority

The first engineering milestone is:

**Phase 0 - RAW/rendering prototype**

Do not start by building the full product UI.

The purpose of Phase 0 is to validate the hardest technical assumptions:

- Tauri ↔ Rust communication
- RAW decoding
- internal image representation
- preview generation
- basic rendering
- caching
- background processing
- CPU performance
- initial GPU feasibility

A technically sound rendering foundation is more important than visual polish at this stage.

---

# 27. Definition of done

A change is not complete merely because it compiles.

Consider it complete when:

- it works
- it is tested
- errors are handled
- performance is acceptable for its purpose
- it respects the architecture
- documentation is updated where necessary

When a trade-off is unavoidable, preserve:

1. correctness
2. responsiveness
3. maintainability
4. simplicity
