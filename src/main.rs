//! Ada Steam Update — get notified the moment a Steam game you follow updates.

// Hide the console window on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod bbcode;
mod detect;
mod icon_art;
mod model;
mod platform;
mod steam;
mod store;
mod ui;
mod worker;

use platform::{AUTOSTART_ARG, Instance};

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu=warn,eframe=warn")).init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Ada Steam Update {}\n\nUSAGE:\n  ada-steam-update [--minimized | --quit]\n\nOPTIONS:\n  --minimized   Start hidden in the system tray\n  --quit        Exit the running instance\n  {AUTOSTART_ARG}   Used when launched at login (honours the 'start in background' setting)\n\nENVIRONMENT:\n  {}   Store data in this directory (portable mode)",
            env!("CARGO_PKG_VERSION"),
            store::DATA_DIR_ENV
        );
        return Ok(());
    }

    if args.iter().any(|a| a == "--quit") {
        return match platform::send_signal(platform::Signal::Quit) {
            Ok(true) => Ok(()),
            _ => {
                eprintln!("Ada Steam Update is not running.");
                std::process::exit(1);
            }
        };
    }

    let listener = match platform::acquire_instance() {
        Instance::Primary(l) => l,
        Instance::Secondary => {
            log::info!("already running — brought the existing window to the front");
            return Ok(());
        }
    };

    let paths = match store::Paths::resolve() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("fatal: {e:#}");
            std::process::exit(1);
        }
    };
    let library = store::load_library(&paths);
    let settings = store::load_settings(&paths);

    // Keep the OS autostart entry in sync with the setting (e.g. after the exe moved).
    if settings.launch_at_startup
        && let Err(e) = platform::set_launch_at_startup(true)
    {
        log::warn!("could not refresh autostart entry: {e:#}");
    }

    let launched_at_login = args.iter().any(|a| a == AUTOSTART_ARG);
    let start_hidden = args.iter().any(|a| a == "--minimized") || (launched_at_login && settings.start_minimized);

    // Icon for toasts.
    let icon_png = paths.data.join("icon.png");
    if !icon_png.exists() {
        let _ = std::fs::create_dir_all(&paths.data);
        let rgba = icon_art::render_icon(128);
        if let Some(img) = image::RgbaImage::from_raw(128, 128, rgba) {
            let _ = img.save(&icon_png);
        }
    }
    platform::register_notification_identity(&icon_png);

    let state = worker::SharedState::new(paths, library, settings);
    let window_icon = egui::IconData { rgba: icon_art::render_icon(256), width: 256, height: 256 };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(platform::APP_NAME)
            .with_app_id("io.github.AdaSteamUpdate")
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([760.0, 540.0])
            .with_icon(window_icon)
            .with_visible(!start_hidden || !cfg!(any(target_os = "windows", target_os = "macos"))),
        persist_window: true,
        ..Default::default()
    };

    eframe::run_native(
        "AdaSteamUpdate",
        options,
        Box::new(move |cc| Ok(Box::new(ui::AdaApp::new(cc, state, listener, start_hidden)))),
    )
}
