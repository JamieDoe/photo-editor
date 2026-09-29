# ADR 0009: Settings as a typed, validated JSON file owned by Rust

- Status: Accepted (Phase 1)
- Date: 2026-09-28

## Context

Phase 1 needs basic settings (PRODUCT.md §44). SQLite arrives with the Phase 2
catalogue. Settings affect both the engine (cache size, thread pools) and the UI
(theme), so one side must own them.

## Decision

- **A `settings` crate** defines `Settings` (serde + ts-rs), versioned
  (`SETTINGS_VERSION`). Only settings with a real effect today exist:
  - theme;
  - preview cache size;
  - background processing intensity;
  - export JPEG quality;
  - default and recent library folders.
- **Rust owns them.** `SettingsStore` keeps settings in memory and writes
  `settings.json` in the OS config directory atomically (`platform::fs::write_atomic`).
  The UI reads and writes through `get_settings` / `update_settings` only.
- **Validation on every load and update** (`Settings::sanitized`): values are clamped
  to their ranges, lists deduplicated and bounded. Missing fields take defaults and
  unknown fields are ignored, so files from older and newer versions load.
- **Loading never fails.**
  - Missing file: defaults.
  - Unreadable or corrupt file: moved aside to `settings.corrupt-<time>.json`,
    defaults are used, and the Settings screen says so.
- **Application of changes:**
  - Applied live: theme, preview cache budget (`Engine::set_preview_cache_budget`),
    export quality (read at export time).
  - Background intensity sizes thread pools created at start-up, so it applies on
    next launch; the UI shows "restart required".
- **The UI saves with a 250 ms debounce**, so dragging a slider writes once.
- **Folders in settings are access grants** (ADR 0010). `update_settings` may clear
  the default folder but can never add or change folders.

## Consequences

- One source of truth: engine configuration is derived from settings at start-up
  (`state::engine_config`).
- Newer-version files lose unknown fields when an older app saves. That is acceptable
  for preferences; catalogue data will live in SQLite with migrations.

## Alternatives considered

- *`tauri-plugin-store`*: an untyped key-value store written from JS. We want Rust to
  own validation and engine configuration.
- *SQLite now*: premature; the catalogue schema is Phase 2 work.
