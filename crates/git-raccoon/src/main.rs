// No console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;

fn main() -> eframe::Result {
    // Started by Git to ask for a password or passphrase: ask the running
    // app, print the answer, and finish (no window).
    if let Some(code) = git_raccoon_core::askpass::run_if_asked() {
        std::process::exit(code);
    }
    // Velopack's install, update and uninstall steps run here (and exit).
    velopack::VelopackApp::build().run();
    // Bugs leave a report beside the settings, offered next time.
    git_raccoon::crash::install();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Git Raccoon")
        .with_app_id("git-raccoon")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([720.0, 480.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    // `git-raccoon <path>` opens that repository.
    let path = std::env::args_os()
        .nth(1)
        .map(|p| p.to_string_lossy().into_owned());
    eframe::run_native(
        "Git Raccoon",
        options,
        Box::new(move |cc| {
            let mut app = git_raccoon::App::new(cc);
            if let Some(path) = path {
                app.open(&cc.egui_ctx, path);
            }
            Ok(Box::new(app))
        }),
    )
}
