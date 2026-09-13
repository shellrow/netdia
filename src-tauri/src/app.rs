use futures::executor::block_on;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

#[cfg(desktop)]
use tauri_plugin_autostart::{init as autostart_init, MacosLauncher, ManagerExt};

use crate::{
    command::{self, config::ConfigState, updater::PendingUpdate},
    db::DatabaseState,
    service,
    state::AppState,
};

fn theme_is_dark(app: &tauri::App) -> bool {
    match app.get_webview_window("main") {
        Some(win) => match win.theme() {
            Ok(theme) => matches!(theme, tauri::Theme::Dark),
            Err(_) => false,
        },
        None => false,
    }
}

#[allow(unused_variables)]
fn tray_icon_bytes(dark: bool) -> &'static [u8] {
    #[cfg(target_os = "macos")]
    {
        if dark {
            include_bytes!("../icons/tray/tray-icon-dark-32x32.png")
        } else {
            include_bytes!("../icons/tray/tray-icon-light-32x32.png")
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        include_bytes!("../icons/tray/tray-icon-32x32.png")
    }
}

pub fn run() {
    let loaded = block_on(async {
        let db = DatabaseState::initialize().await?;
        let config = db.load_app_config().await?;
        command::validation::validate_config(&config).map_err(anyhow::Error::msg)?;
        Ok::<_, anyhow::Error>((db, config))
    });
    let (db_state, app_conf) = match loaded {
        Ok(loaded) => loaded,
        Err(error) => {
            let detail = format!("{error:#}");
            eprintln!("NetDia could not load saved data: {detail}");
            // Keep diagnostics available, but never write defaults over failed storage.
            (
                DatabaseState::unavailable(detail),
                crate::config::AppConfig {
                    auto_update_check: false,
                    auto_internet_check: false,
                    ..crate::config::AppConfig::default()
                },
            )
        }
    };
    let storage_available = db_state.startup_error().is_none();
    let startup = app_conf.startup;
    let background = app_conf.background;
    if let Err(error) = crate::log::init_logger(&app_conf) {
        eprintln!("NetDia could not initialize logging: {error}");
    }

    let conf_state = ConfigState(tokio::sync::RwLock::new(app_conf));

    let shared_app_state = Arc::new(AppState::default());

    let result = tauri::Builder::default()
        // Plugins
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Register AppState as Tauri State
        .manage(conf_state)
        .manage(db_state)
        .manage(shared_app_state.clone())
        .manage(PendingUpdate::default())
        // Setup: spawn background task
        .setup(move |app| {
            // Spawn background supervisor task
            let handle = service::spawn_supervisor(app.handle().clone(), shared_app_state.clone());
            tauri::async_runtime::spawn({
                let shared = shared_app_state.clone();
                async move {
                    let mut t = shared.task.lock().await;
                    *t = Some(handle);
                }
            });

            if background {
                let tray_icon_bytes = tray_icon_bytes(theme_is_dark(app));
                let tray_icon = tauri::image::Image::from_bytes(tray_icon_bytes)
                    .ok()
                    .or_else(|| app.default_window_icon().cloned())
                    .ok_or_else(|| std::io::Error::other("no tray icon is available"))?;

                let show_item = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
                let hide_item = MenuItem::with_id(app, "hide", "Hide Window", true, None::<&str>)?;
                let quit_item = MenuItem::with_id(app, "quit", "Quit NetDia", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show_item, &hide_item, &quit_item])?;
                let _tray = TrayIconBuilder::with_id("tray")
                    .icon(tray_icon)
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "show" => {
                            if let Some(win) = app.get_webview_window("main") {
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                        }
                        "hide" => {
                            if let Some(win) = app.get_webview_window("main") {
                                let _ = win.hide();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| match event {
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } => {
                            let app = tray.app_handle();
                            if let Some(w) = app.get_webview_window("main") {
                                let visible = w.is_visible().unwrap_or(true);
                                if visible {
                                    let _ = w.hide();
                                } else {
                                    let _ = w.unminimize();
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                        }
                        TrayIconEvent::DoubleClick { .. } => {
                            let app = tray.app_handle();
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.unminimize();
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        _ => {}
                    })
                    .build(app)?;
            }

            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(autostart_init(MacosLauncher::LaunchAgent, None))?;

                // Get the autostart manager
                let autostart_manager = app.autolaunch();

                let update_result = if !storage_available {
                    Ok(())
                } else if startup {
                    autostart_manager.enable()
                } else {
                    autostart_manager.disable()
                };
                if let Err(error) = update_result {
                    tracing::warn!("failed to update autostart registration: {error}");
                }
                match autostart_manager.is_enabled() {
                    Ok(enabled) => tracing::debug!("registered for autostart? {enabled}"),
                    Err(error) => {
                        tracing::warn!("failed to read autostart registration: {error}");
                    }
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // react to theme changes on THIS window
            if let WindowEvent::ThemeChanged(theme) = event {
                let app = window.app_handle();
                let Some(tray) = app.tray_by_id("tray") else {
                    return;
                };
                let tray_icon_bytes = tray_icon_bytes(matches!(theme, tauri::Theme::Dark));
                if let Some(tray_icon) = tauri::image::Image::from_bytes(tray_icon_bytes)
                    .ok()
                    .or_else(|| app.default_window_icon().cloned())
                {
                    let _ = tray.set_icon(Some(tray_icon));
                } else {
                    tracing::warn!("unable to update tray icon because no icon is available");
                }
            }
        })
        // Register commands
        .invoke_handler(tauri::generate_handler![
            command::about,
            command::startup::get_startup_status,
            command::startup::retry_startup,
            crate::operation::prepare_operation,
            crate::operation::cancel_operation,
            command::interfaces::get_network_interfaces,
            command::interfaces::reload_interfaces,
            command::interfaces::get_default_network_interface,
            command::interfaces::get_network_address_map,
            command::routes::get_routes,
            command::routes::get_neighbor_table,
            command::socket::get_sockets_all,
            command::internet::get_public_ip_info,
            command::system::get_sys_info,
            command::config::get_config,
            command::config::reload_config,
            command::config::save_config,
            command::config::logs_dir_path,
            command::ui_preferences::get_ui_preferences,
            command::ui_preferences::patch_ui_preferences,
            command::ui_preferences::migrate_legacy_ui_preferences,
            command::dns::lookup_host,
            command::dns::lookup_domain,
            command::dns::lookup_ip,
            command::dns::reverse_lookup,
            command::dns::lookup_all,
            command::ping::ping,
            command::ping::cancel_ping,
            command::scan::get_target_ports,
            command::scan::preview_port_input,
            command::scan::preview_host_scan_targets,
            command::scan::port_scan,
            command::scan::cancel_portscan,
            command::scan::host_scan,
            command::scan::cancel_hostscan,
            command::scan::neighbor_scan,
            command::scan::cancel_neighborscan,
            command::trace::traceroute,
            command::trace::cancel_traceroute,
            command::scan::init_probe_db,
            command::latency::measure_latency,
            command::speedtest::start_speedtest,
            command::speedtest::stop_speedtest,
            command::notifications::list_notifications,
            command::notifications::mark_all_notifications_read,
            command::notifications::dismiss_notification,
            command::notifications::upsert_update_notification,
            command::updater::check_update,
            command::updater::install_update,
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        tracing::error!(%error, "native application runtime failed");
        eprintln!("NetDia could not run its native application: {error}");
        std::process::exit(1);
    }
}
