mod commands;
mod settings;
mod state;
mod tray;

use settings::Keys;
use state::AppState;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub fn show_studio(app: &AppHandle, view: Option<String>) {
    if let Some(w) = app.get_webview_window("studio") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(v) = view {
            let _ = app.emit_to("studio", "cd://navigate", v);
        }
    }
}

fn build_windows(app: &AppHandle, onboarded: bool) -> tauri::Result<()> {
    let studio = WebviewWindowBuilder::new(app, "studio", WebviewUrl::App("index.html#/studio".into()))
        .title("Cutdeck")
        .inner_size(1360.0, 860.0)
        .min_inner_size(1080.0, 700.0)
        .decorations(false)
        .center()
        .visible(!onboarded)
        .background_color(tauri::window::Color(10, 10, 14, 255))
        .build()?;
    let app2 = app.clone();
    studio.on_window_event(move |ev| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = ev {
            // Closing the studio keeps Cutdeck running in the tray.
            api.prevent_close();
            if let Some(w) = app2.get_webview_window("studio") {
                let _ = w.hide();
            }
        }
    });
    let mut tb = WebviewWindowBuilder::new(app, "tray", WebviewUrl::App("index.html#/tray".into()))
        .title("Cutdeck Tray")
        .inner_size(tray::TRAY_W, tray::TRAY_H)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false)
        .visible(false);
    #[cfg(target_os = "macos")]
    {
        tb = tb.visible_on_all_workspaces(true);
    }
    tb = tb.background_color(tauri::window::Color(0, 0, 0, 0));
    let trayw = tb.build()?;
    let _ = trayw.set_ignore_cursor_events(true);
    if onboarded {
        tray::show(app);
    }
    Ok(())
}

fn build_tray_icon(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Cutdeck", true, None::<&str>)?;
    let paste = MenuItem::with_id(app, "paste", "Clip a link…", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Show / hide corner tray", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Cutdeck", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &paste, &toggle, &settings, &sep, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("icon"))
        .tooltip("Cutdeck")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "open" => show_studio(app, Some("library".into())),
            "settings" => show_studio(app, Some("settings".into())),
            "paste" => {
                tray::show(app);
                let _ = app.emit_to("tray", "cd://open-link", ());
            }
            "toggle" => {
                if let Some(w) = app.get_webview_window("tray") {
                    if w.is_visible().unwrap_or(false) {
                        let _ = w.hide();
                    } else {
                        tray::show(app);
                    }
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|icon, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                show_studio(icon.app_handle(), None);
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt().with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into())).try_init();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_studio(app, None)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--background"])))
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir()?;
            let config_dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let resources = app.path().resource_dir()?.join("resources");
            let resources = if resources.join("fonts").is_dir() {
                resources
            } else {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")
            };
            let settings_path = config_dir.join("settings.json");
            let s = settings::load(&settings_path);
            let onboarded = s.onboarded && s.ai.is_some();
            app.manage(AppState {
                data_dir: data_dir.clone(),
                resources,
                settings_path,
                settings: Mutex::new(s),
                keys: Keys { fallback: data_dir.join("keys.json") },
                engine: tokio::sync::RwLock::new(None),
                queue: Mutex::new(Default::default()),
                queue_wake: tokio::sync::Notify::new(),
                exports: Mutex::new(Default::default()),
                export_wake: tokio::sync::Notify::new(),
                hit: Mutex::new(vec![]),
                drop_zone: Mutex::new(None),
                deps_busy: AtomicBool::new(false),
            });
            build_windows(&handle, onboarded)?;
            build_tray_icon(&handle)?;
            tray::spawn_hit_loop(handle.clone());
            state::spawn_job_worker(handle.clone());
            state::spawn_export_worker(handle.clone());
            let h2 = handle.clone();
            tauri::async_runtime::spawn(async move {
                state::resume_exports(&h2).await;
                // Keep yt-dlp current; YouTube changes often.
                if let Ok(engine) = h2.state::<AppState>().engine().await {
                    let _ = cutdeck_engine::deps::update_ytdlp(&engine.tools).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::install_deps,
            commands::install_whisper,
            commands::test_ai,
            commands::save_ai,
            commands::forget_key,
            commands::save_settings,
            commands::finish_onboarding,
            commands::start_job,
            commands::cancel_job,
            commands::queue_state,
            commands::list_projects,
            commands::delete_project,
            commands::keep_clip,
            commands::discard_clip,
            commands::restore_clip,
            commands::edit_clip,
            commands::export_clip,
            commands::set_hit_rects,
            commands::focus_tray,
            commands::open_studio,
            commands::reveal,
            commands::quit,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Cutdeck")
        .run(|_app, ev| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = ev {
                // Keep running in the tray when windows close; quit only on request.
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
