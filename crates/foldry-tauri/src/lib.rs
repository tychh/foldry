#![forbid(unsafe_code)]

mod ipc;

#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::Manager;
#[cfg(target_os = "macos")]
use tauri::menu::{HELP_SUBMENU_ID, MenuItem};

#[cfg(target_os = "macos")]
const ABOUT_MENU_ID: &str = "foldry-about";
#[cfg(target_os = "macos")]
const HELP_MENU_ID: &str = "foldry-help";
#[cfg(target_os = "macos")]
const ABOUT_MENU_EVENT: &str = "foldry://open-about";
#[cfg(target_os = "macos")]
const HELP_MENU_EVENT: &str = "foldry://open-help";

/// Starts the Foldry desktop application.
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        );
    #[cfg(target_os = "macos")]
    let builder = builder.on_menu_event(|app, event| {
        let frontend_event = match event.id().as_ref() {
            ABOUT_MENU_ID => Some(ABOUT_MENU_EVENT),
            HELP_MENU_ID => Some(HELP_MENU_EVENT),
            _ => None,
        };

        if let Some(frontend_event) = frontend_event
            && let Some(window) = app.get_webview_window("main")
        {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
            let _ = window.emit(frontend_event, ());
        }
    });
    builder
        .setup(|app| {
            let state = ipc::DesktopState::open(app.handle()).map_err(std::io::Error::other)?;
            app.manage(state);
            #[cfg(target_os = "macos")]
            install_information_menu(app).map_err(std::io::Error::other)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::bootstrap_snapshot,
            ipc::browser_roots,
            ipc::browser_children,
            ipc::cancel_browser_request,
            ipc::browser_node,
            ipc::browser_size,
            ipc::cancel_browser_size,
            ipc::save_settings,
            ipc::save_plan,
            ipc::add_folder,
            ipc::add_dropped_sources,
            ipc::update_folder,
            ipc::unlist_folder,
            ipc::unlisted_folders,
            ipc::forget_folders,
            ipc::forget_all_unlisted_folders,
            ipc::profile_usage,
            ipc::browser_favorites,
            ipc::browser_recent,
            ipc::set_browser_view,
            ipc::set_favorite,
            ipc::add_action,
            ipc::update_action,
            ipc::remove_action,
            ipc::reorder_actions,
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
            ipc::run_action,
            ipc::run_folder,
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

#[cfg(target_os = "macos")]
fn install_information_menu(app: &tauri::App) -> Result<(), String> {
    let menu = app
        .menu()
        .ok_or_else(|| "the native application menu is unavailable".to_owned())?;
    let app_menu = menu
        .items()
        .map_err(|error| error.to_string())?
        .into_iter()
        .next()
        .and_then(|item| item.as_submenu().cloned())
        .ok_or_else(|| "the native Foldry menu is unavailable".to_owned())?;

    if app_menu
        .remove_at(0)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err("the native About item is unavailable".to_owned());
    }

    let about_item = MenuItem::with_id(app, ABOUT_MENU_ID, "About Foldry", true, None::<&str>)
        .map_err(|error| error.to_string())?;
    app_menu
        .insert(&about_item, 0)
        .map_err(|error| error.to_string())?;

    let help_menu = menu
        .get(HELP_SUBMENU_ID)
        .and_then(|item| item.as_submenu().cloned())
        .ok_or_else(|| "the native Help menu is unavailable".to_owned())?;
    let help_item = MenuItem::with_id(app, HELP_MENU_ID, "Foldry Help", true, None::<&str>)
        .map_err(|error| error.to_string())?;
    help_menu
        .append(&help_item)
        .map_err(|error| error.to_string())?;

    Ok(())
}
