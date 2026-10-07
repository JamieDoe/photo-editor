use tauri::{AppHandle, State};

use crate::AppState;
use crate::ipc::SelfTestConfigDto;

#[tauri::command]
pub fn self_test_config(state: State<'_, AppState>) -> Option<SelfTestConfigDto> {
    let image = state.self_test.as_ref()?;
    log::info!("self-test: the page asked for its config; starting");
    let export = std::env::temp_dir().join("photo-editor-self-test-export.jpg");
    Some(SelfTestConfigDto {
        image_path: image.display().to_string(),
        export_path: export.display().to_string(),
    })
}

/// Receives the UI-side self-test report, prints it, and exits.
#[tauri::command]
pub fn self_test_report(app: AppHandle, state: State<'_, AppState>, report: serde_json::Value) {
    if state.self_test.is_none() {
        return;
    }
    println!("SELF_TEST_REPORT {report}");
    let code = if report.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
        0
    } else {
        1
    };
    crate::lifecycle::shutdown(app, code);
}

/// Self-test only: asks the main window to close, exactly like the close button, so
/// the test can check that a running export blocks it.
#[tauri::command]
pub fn self_test_request_close(app: AppHandle, state: State<'_, AppState>) {
    use tauri::Manager;
    if state.self_test.is_none() {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.close();
    }
}

/// Self-test only: grants the folder containing the self-test image, as if the user
/// had chosen it, so the test can exercise indexing. Returns the folder path.
#[tauri::command]
pub fn self_test_grant_folder(state: State<'_, AppState>) -> Option<String> {
    let image = state.self_test.as_ref()?;
    let folder = image.parent()?;
    state
        .folders
        .grant(folder)
        .ok()
        .map(|p| p.display().to_string())
}
