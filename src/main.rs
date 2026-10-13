#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod ui;

fn main() -> eframe::Result {
    let initial_file = std::env::args_os()
        .nth(1)
        .filter(|s| s != "--screenshot")
        .map(std::path::PathBuf::from);
    let screenshot = std::env::args_os()
        .skip_while(|s| s != "--screenshot")
        .nth(1)
        .map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 760.0])
            .with_min_inner_size([820.0, 600.0])
            .with_title("Whistle — Offline transcription"),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Whistle",
        options,
        Box::new(move |cc| {
            Ok(Box::new(ui::WhistleApp::new(
                &cc.egui_ctx,
                screenshot,
                initial_file,
            )))
        }),
    )
}
