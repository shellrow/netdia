use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

#[cfg(desktop)]
use tauri_plugin_autostart::{init as autostart_init, MacosLauncher, ManagerExt};

use crate::{
    command::{self, config::ConfigState},
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
    let app_conf: crate::config::AppConfig = crate::config::AppConfig::load();
    let startup = app_conf.startup;
    let background = app_conf.background;
    let _ = crate::log::init_logger(&app_conf);

    let conf_state = ConfigState(tokio::sync::RwLock::new(app_conf));

    let shared_app_state = Arc::new(AppState::default());

    tauri::Builder::default()
        // Plugins
        .plugin(tauri_plugin_opener::init())
        // Register AppState as Tauri State
        .manage(conf_state)
        .manage(shared_app_state.clone())
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
                let tray_icon_bytes = tray_icon_bytes(theme_is_dark(&app));
                let tray_icon = tauri::image::Image::from_bytes(tray_icon_bytes)
                    .unwrap_or(app.default_window_icon().unwrap().clone());

                let show_item = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
                let hide_item = MenuItem::with_id(app, "hide", "Hide Window", true, None::<&str>)?;
                let quit_item =
                    MenuItem::with_id(app, "quit", "Quit NetDia", true, None::<&str>)?;
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
                let _ = app
                    .handle()
                    .plugin(autostart_init(MacosLauncher::LaunchAgent, None));

                // Get the autostart manager
                let autostart_manager = app.autolaunch();

                if startup {
                    // Enable autostart
                    let _ = autostart_manager.enable();
                    // Check enable state
                    tracing::debug!(
                        "registered for autostart? {}",
                        autostart_manager.is_enabled().unwrap()
                    );
                } else {
                    // Disable autostart
                    let _ = autostart_manager.disable();
                    // Check enable state
                    tracing::debug!(
                        "registered for autostart? {}",
                        autostart_manager.is_enabled().unwrap()
                    );
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
                let tray_icon = tauri::image::Image::from_bytes(tray_icon_bytes)
                    .unwrap_or(app.default_window_icon().unwrap().clone());
                let _ = tray.set_icon(Some(tray_icon));
            }
        })
        // Register commands
        .invoke_handler(tauri::generate_handler![
            command::about,
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
            command::dns::lookup_host,
            command::dns::lookup_domain,
            command::dns::lookup_ip,
            command::dns::reverse_lookup,
            command::dns::lookup_all,
            command::ping::ping,
            command::scan::get_target_ports,
            command::scan::port_scan,
            command::scan::host_scan,
            command::scan::neighbor_scan,
            command::trace::traceroute,
            command::scan::init_probe_db,
            command::latency::measure_latency,
            command::speedtest::start_speedtest,
            command::speedtest::stop_speedtest,
        ])
        .run(tauri::generate_context!())
        .expect("error while running netdia application");
}
