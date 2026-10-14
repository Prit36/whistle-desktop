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
    // Optional capture size for checking the native layout at supported window sizes.
    let capture_size = screenshot.as_ref().and_then(|_| {
        let args: Vec<String> = std::env::args().collect();
        let value = args
            .windows(2)
            .find(|pair| pair[0] == "--screenshot-size")?;
        let (width, height) = value[1].split_once('x')?;
        let (width, height) = (width.parse::<u16>().ok()?, height.parse::<u16>().ok()?);
        (width >= 820 && height >= 600).then_some([f32::from(width), f32::from(height)])
    });
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size(capture_size.unwrap_or([1120.0, 760.0]))
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
