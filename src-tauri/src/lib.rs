mod commands;
mod db;
mod model;
mod scheduler;

use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::{
    db::Database,
    scheduler::{AppState, queue_all, start_background_scheduler},
};

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(
            |app, _arguments, _cwd| {
                show_main(app);
            },
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            let database_path = app
                .path()
                .app_data_dir()
                .map_err(|error| error.to_string())?
                .join("walle.sqlite3");
            let database = Database::initialize(database_path).map_err(|error| {
                format!("Walle could not initialize its local database: {error}")
            })?;
            let settings = database.get_settings()?;
            let state = AppState::new(database)?;
            app.manage(state.clone());

            let open_item = MenuItem::with_id(app, "open", "Open Walle", true, None::<&str>)?;
            let refresh_item = MenuItem::with_id(
                app,
                "refresh",
                "Refresh active packages",
                true,
                None::<&str>,
            )?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &refresh_item, &quit_item])?;
            let mut tray = TrayIconBuilder::with_id("walle-tray")
                .tooltip("Walle package tracker")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main(app),
                    "refresh" => {
                        let state = app.state::<AppState>().inner().clone();
                        let _ = queue_all(app.clone(), state);
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            let minimized_argument = std::env::args().any(|argument| argument == "--minimized");
            if (settings.start_minimized || minimized_argument)
                && let Some(window) = app.get_webview_window("main")
            {
                let _ = window.hide();
            }
            start_background_scheduler(app.handle().clone(), state);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main"
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_parcels,
            commands::get_parcel,
            commands::add_parcel,
            commands::update_parcel,
            commands::archive_parcel,
            commands::restore_parcel,
            commands::delete_parcel,
            commands::refresh_parcel,
            commands::refresh_all,
            commands::get_settings,
            commands::update_settings,
            commands::get_source_health,
            commands::open_tracking_page,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Walle");
}
