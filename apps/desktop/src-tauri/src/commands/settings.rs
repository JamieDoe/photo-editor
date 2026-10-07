use std::path::Path;

use settings::{LibraryPlace, Settings};
use tauri::State;

use super::IpcResult;
use crate::AppState;
use crate::ipc::{IpcError, SettingsViewDto};

pub(super) fn view(state: &AppState) -> SettingsViewDto {
    SettingsViewDto {
        settings: state.settings.get(),
        restart_required: state.restart_required(),
        recovered_from: state
            .settings_recovered_from
            .as_ref()
            .map(|p| p.display().to_string()),
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> SettingsViewDto {
    view(&state)
}

/// Validates, saves and applies settings. Returns what was actually stored (values are
/// clamped to their ranges).
#[tauri::command]
pub async fn update_settings(
    state: State<'_, AppState>,
    settings: Settings,
) -> IpcResult<SettingsViewDto> {
    let before = state.settings.get();
    let settings = guard_library_changes(&before, settings);
    let saved = state
        .settings
        .update(settings)
        .map_err(IpcError::internal)?;
    if saved.performance.preview_cache_mb != before.performance.preview_cache_mb {
        state
            .engine
            .set_preview_cache_budget(saved.performance.preview_cache_mb as usize * 1024 * 1024);
    }
    if saved != before {
        log::info!(
            "settings updated: {}",
            serde_json::to_string(&saved).unwrap_or_default()
        );
    }
    Ok(view(&state))
}

/// Records where the Library is (ADR 0065), to reopen it at the next launch. A folder
/// is kept only if it lies inside a granted folder, by its canonical path: like every
/// folder in settings, it never comes from the UI unchecked, and it grants nothing.
#[tauri::command]
pub fn remember_place(state: State<'_, AppState>, place: LibraryPlace) -> IpcResult<()> {
    let place = match place {
        LibraryPlace::Folder { path } => {
            let canonical = state
                .folders
                .check(Path::new(&path))
                .ok_or_else(|| super::library::folder_unavailable(&path))?;
            LibraryPlace::Folder {
                path: canonical.display().to_string(),
            }
        }
        other => other,
    };
    let mut settings = state.settings.get();
    if settings.library.last_place.as_ref() == Some(&place) {
        return Ok(());
    }
    settings.library.last_place = Some(place);
    state
        .settings
        .update(settings)
        .map_err(IpcError::internal)?;
    Ok(())
}

/// Folders are chosen only through the native dialog (see the library, backup and
/// export commands), so a settings update from the UI may *clear* the default folder but can
/// never add or change a folder: that would grant access at next launch, or send
/// backups somewhere the user never chose. The last place (ADR 0065) is recorded only
/// by [`remember_place`].
fn guard_library_changes(before: &Settings, mut requested: Settings) -> Settings {
    requested.library.recent_folders = before.library.recent_folders.clone();
    requested.library.last_place = before.library.last_place.clone();
    if requested.library.default_folder.is_some() {
        requested.library.default_folder = before.library.default_folder.clone();
    }
    requested.backups.copy_folder = before.backups.copy_folder.clone();
    requested.export.folder = before.export.folder.clone();
    requested
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_folders(default: Option<&str>, recent: &[&str]) -> Settings {
        let mut s = Settings::default();
        s.library.default_folder = default.map(str::to_owned);
        s.library.recent_folders = recent.iter().map(|f| (*f).to_owned()).collect();
        s
    }

    #[test]
    fn ui_cannot_add_or_change_folders() {
        let before = with_folders(Some("/Photos"), &["/Photos"]);
        let mut requested = with_folders(Some("/etc"), &["/etc", "/Photos"]);
        requested.export.jpeg_quality = 80;
        let result = guard_library_changes(&before, requested);
        assert_eq!(result.library, before.library);
        assert_eq!(result.export.jpeg_quality, 80, "other settings still apply");
    }

    #[test]
    fn ui_cannot_set_the_last_place() {
        let mut before = with_folders(Some("/Photos"), &["/Photos"]);
        before.library.last_place = Some(LibraryPlace::Album { id: 3 });
        let mut requested = before.clone();
        requested.library.last_place = Some(LibraryPlace::Folder {
            path: "/etc".into(),
        });
        let result = guard_library_changes(&before, requested);
        assert_eq!(
            result.library.last_place,
            Some(LibraryPlace::Album { id: 3 })
        );
    }

    #[test]
    fn ui_can_clear_the_default_folder() {
        let before = with_folders(Some("/Photos"), &["/Photos"]);
        let result = guard_library_changes(&before, with_folders(None, &[]));
        assert_eq!(result.library.default_folder, None);
        assert_eq!(result.library.recent_folders, ["/Photos"]);
    }

    #[test]
    fn ui_cannot_set_or_change_the_backup_copy_folder() {
        let mut before = Settings::default();
        before.backups.copy_folder = Some("/Volumes/Backup".into());
        let mut requested = before.clone();
        requested.backups.copy_folder = Some("/tmp/elsewhere".into());
        let result = guard_library_changes(&before, requested);
        assert_eq!(
            result.backups.copy_folder.as_deref(),
            Some("/Volumes/Backup")
        );
    }

    #[test]
    fn ui_cannot_set_a_default_when_none_existed() {
        let before = with_folders(None, &[]);
        let result = guard_library_changes(&before, with_folders(Some("/"), &[]));
        assert_eq!(result.library.default_folder, None);
    }

    #[test]
    fn the_export_folder_is_never_set_from_the_ui() {
        let mut before = Settings::default();
        before.export.folder = Some("/Users/me/Exports".to_owned());
        let mut requested = before.clone();
        requested.export.folder = Some("/somewhere/else".to_owned());
        requested.export.long_edge = Some(1350);
        let kept = guard_library_changes(&before, requested);
        assert_eq!(kept.export.folder.as_deref(), Some("/Users/me/Exports"));
        assert_eq!(
            kept.export.long_edge,
            Some(1350),
            "other export settings still apply"
        );
    }
}
