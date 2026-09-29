# ADR 0010: Folder access scope for the Library

- Status: Accepted (Phase 1)
- Date: 2026-09-28

## Context

The Library browses the user's photo folders. CLAUDE.md requires deliberate IPC that
does not expose arbitrary filesystem internals. If a compromised or buggy webview
could make Rust list or read any path, it could reach files the user never chose.

## Decision

- **`folders::FolderAccess`** holds granted roots (canonical paths). A path is allowed
  only if its canonical form is inside a granted root. Canonicalisation resolves `..`
  and symlinks, so neither can escape the grant.
- **Grants come only from the user:**
  - the native folder dialog (`choose_folder`, run from Rust);
  - folders granted that way in earlier sessions and remembered in settings as the
    default or recent folders. Remembered folders that no longer exist are ignored.
- **Checked commands:** `list_folder` and `set_default_folder`, plus `open_image_path`
  (with an exception for the self-test file).
  - `open_image_dialog` needs no check: the user picks the file.
  - A refusal is logged with a reference, and the user sees "This folder isn't
    available… choose it again".
- **Settings cannot escalate:** `update_settings` keeps the stored recent folders and
  only allows clearing the default folder. Covered by `guard_library_changes` tests.
- **Breadcrumbs stop at the granted root**, so the UI never offers to navigate above
  it.
- **Listing reads metadata only:**
  - one level: subfolders and supported photos, natural sort;
  - hidden and `._` entries skipped;
  - unreadable entries counted, not fatal;
  - runs on Tauri's blocking pool.

## Consequences

- Browsing outside granted folders needs another "Choose folder…". This is the
  intended behaviour.
- A granted folder that is moved on disk must be chosen again. Phase 2 file identity
  can relocate photos, but folder grants stay explicit.
- The same scope will gate Phase 2 indexing and thumbnail generation.
