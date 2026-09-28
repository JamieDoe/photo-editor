// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod ipc;

use std::path::PathBuf;
use std::sync::atomic::AtomicU64;

use app_core::{Engine, EngineConfig};

/// Shared application state managed by Tauri.
pub struct AppState {
    pub engine: Engine,
    /// Set when launched with `PE_SELF_TEST=<image path>` (see docs/PERFORMANCE.md).
    pub self_test: Option<PathBuf>,
    /// Export ids are allocated here (not by the job system) so progress events can
    /// carry the id from the very first event.
    pub next_export_id: AtomicU64,
}

fn main() {
    let self_test = std::env::var_os("PE_SELF_TEST").map(PathBuf::from);
    let state = AppState {
        engine: Engine::new(EngineConfig::default()),
        self_test,
        next_export_id: AtomicU64::new(1),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::engine_info,
            commands::open_image_dialog,
            commands::open_image_path,
            commands::render_preview,
            commands::export_image,
            commands::self_test_config,
            commands::self_test_report,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("fatal: failed to run application: {e}");
            std::process::exit(1);
        });
}
