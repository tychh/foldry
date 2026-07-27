#![forbid(unsafe_code)]

mod ipc;

use tauri::Manager;

/// Starts the Foldry desktop application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .setup(|app| {
            let state = ipc::DesktopState::open(app.handle()).map_err(std::io::Error::other)?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::bootstrap_snapshot,
            ipc::browser_roots,
            ipc::browser_children,
            ipc::cancel_browser_request,
            ipc::save_settings,
            ipc::save_plan,
            ipc::add_task,
            ipc::add_dropped_sources,
            ipc::update_task,
            ipc::remove_task,
            ipc::create_profile,
            ipc::rename_profile,
            ipc::save_profile,
            ipc::delete_profile,
            ipc::restore_default_profile,
            ipc::save_preset,
            ipc::delete_preset,
            ipc::reset_preset,
            ipc::start_preview,
            ipc::preview_page,
            ipc::cancel_preview,
            ipc::run_task,
            ipc::run_all_enabled,
            ipc::repeat_run,
            ipc::scheduler_snapshot,
            ipc::pause_run,
            ipc::resume_run,
            ipc::stop_run,
            ipc::pause_all,
            ipc::resume_all,
            ipc::stop_all,
            ipc::history_page,
            ipc::run_details,
            ipc::logs_page,
            ipc::export_run_logs,
            ipc::pick_folders,
            ipc::reveal_run_output,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run the Foldry desktop application");
}
