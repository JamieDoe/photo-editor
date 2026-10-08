// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backups;
mod commands;
mod export_queue;
mod ipc;
mod lifecycle;
mod logging;
mod state;

use std::path::PathBuf;

use tauri::Manager;

pub use state::AppState;

fn main() {
    logging::install_panic_hook();
    let self_test = std::env::var_os("PE_SELF_TEST").map(PathBuf::from);

    let mut builder = tauri::Builder::default();
    // Must be first: a second launch hands over to the running app and exits. Self-test
    // runs are isolated (in-memory catalogue, temporary cache), so they may run beside
    // the user's app instead of handing over to it.
    if self_test.is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            log::info!("second launch; focusing the existing window");
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }));
    }
    let result = builder
        // Registered early so that everything after it can log.
        .plugin(logging::plugin())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let info = app.package_info();
            log::info!(
                "{} {} starting ({} {})",
                info.name,
                info.version,
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            // Needs the app handle for the platform config directory, so the state is
            // created here rather than before the builder.
            let settings_path = if self_test.is_some() {
                // Self-test runs start from default settings and never change the user's.
                let path = std::env::temp_dir().join("photo-editor-self-test-settings.json");
                let _ = std::fs::remove_file(&path);
                path
            } else {
                app.path().app_config_dir()?.join("settings.json")
            };
            let catalogue_path = app.path().app_data_dir()?.join("catalogue.sqlite");
            let backups_dir = app.path().app_data_dir()?.join("backups");
            let thumbnail_dir = if self_test.is_some() {
                // Self-test runs start cold and never touch the user's cache.
                let dir = std::env::temp_dir().join("photo-editor-self-test-thumbnails");
                let _ = std::fs::remove_dir_all(&dir);
                dir
            } else {
                app.path().app_cache_dir()?.join("thumbnails")
            };
            app.manage(AppState::new(
                settings_path,
                catalogue_path,
                thumbnail_dir,
                backups_dir,
                self_test.clone(),
            ));
            if self_test.is_none() {
                backups::start(app.handle().clone());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::engine_info,
            commands::system::diagnostics,
            commands::system::open_logs_folder,
            commands::system::report_client_error,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::remember_place,
            commands::library::choose_folder,
            commands::library::list_folder,
            commands::library::set_default_folder,
            commands::library::index_library_folder,
            commands::library::library_status,
            commands::library::library_thumbnail,
            commands::library::cancel_thumbnail,
            commands::marks::set_photo_marks,
            commands::marks::library_collection,
            commands::marks::search_library,
            commands::albums::list_albums,
            commands::albums::create_album,
            commands::albums::rename_album,
            commands::albums::delete_album,
            commands::albums::add_to_album,
            commands::albums::remove_from_album,
            commands::albums::album_photos,
            commands::edits::save_edit,
            commands::edits::paste_edits_to,
            commands::presets::list_presets,
            commands::presets::create_preset,
            commands::presets::rename_preset,
            commands::presets::update_preset,
            commands::presets::delete_preset,
            commands::presets::export_preset,
            commands::presets::import_presets,
            commands::backups::library_backups,
            commands::backups::back_up_library,
            commands::backups::show_backups,
            commands::backups::choose_backup_copy_folder,
            commands::backups::stop_backup_copies,
            commands::images::open_image_dialog,
            commands::images::open_image_path,
            commands::images::render_preview,
            commands::images::prepare_full,
            commands::images::auto_level,
            commands::images::new_spot,
            commands::images::find_dust,
            commands::images::measure_chromatic_aberration,
            commands::export::export_image,
            commands::export::estimate_export,
            commands::export::choose_export_folder,
            commands::export::start_export,
            commands::export::cancel_exports,
            commands::selftest::self_test_config,
            commands::selftest::self_test_report,
            commands::selftest::self_test_request_close,
            commands::selftest::self_test_grant_folder,
            lifecycle::quit,
        ])
        .on_window_event(lifecycle::on_window_event)
        .build(tauri::generate_context!());
    let app = match result {
        Ok(app) => app,
        Err(e) => {
            log::error!("fatal: failed to start application: {e}");
            eprintln!("fatal: failed to start application: {e}");
            std::process::exit(1);
        }
    };
    app.run(lifecycle::on_run_event);
}
