use eframe::egui::{self, Color32, Rect, RichText, Stroke, Vec2};
use fastframe_fonts::Weight;
use std::{
    path::PathBuf,
    sync::{
        atomic::Ordering,
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, Instant},
};
use whistle_desktop::{
    engine::Options,
    import::{self, ImportReport},
    worker::{Command, Event, Worker},
};

const BG: Color32 = Color32::from_rgb(16, 22, 27);
const SIDEBAR: Color32 = Color32::from_rgb(23, 31, 37);
const CARD: Color32 = Color32::from_rgb(23, 31, 37);
const INPUT: Color32 = Color32::from_rgb(19, 26, 31);
const CONTROL: Color32 = Color32::from_rgb(33, 43, 50);
const LINE: Color32 = Color32::from_rgb(62, 77, 85);
const FIELD_LINE: Color32 = Color32::from_rgb(100, 119, 129);
const TEXT: Color32 = Color32::from_rgb(234, 240, 237);
const MUTED: Color32 = Color32::from_rgb(168, 184, 190);
const ACCENT: Color32 = Color32::from_rgb(163, 225, 184);
const ACCENT_HOVER: Color32 = Color32::from_rgb(185, 237, 202);
const TINT: Color32 = Color32::from_rgb(39, 62, 51);
const SUCCESS: Color32 = ACCENT;
const DANGER: Color32 = Color32::from_rgb(255, 170, 163);
const ERROR_BG: Color32 = Color32::from_rgb(55, 32, 37);

// Phosphor Regular 2.1.2. The font is embedded so the app stays fully offline.
mod icons {
    pub const WAVEFORM: &str = "\u{e802}";
    pub const MICROPHONE: &str = "\u{e326}";
    pub const UPLOAD: &str = "\u{e4c0}";
    pub const FOLDER: &str = "\u{e25a}";
    pub const COPY: &str = "\u{e1ca}";
    pub const DOWNLOAD: &str = "\u{e20c}";
    pub const STOP: &str = "\u{e46c}";
    pub const TRASH: &str = "\u{e4a6}";
    pub const UNDO: &str = "\u{e038}";
    pub const CHECK: &str = "\u{e184}";
    pub const WARNING: &str = "\u{e4e2}";
    pub const KEYBOARD: &str = "\u{e2d8}";
    pub const QUEUE: &str = "\u{e6ac}";
}
const LANGUAGES: [(&str, &str); 8] = [
    ("", "Auto detect"),
    ("en", "English"),
    ("de", "German"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("it", "Italian"),
    ("nl", "Dutch"),
    ("pl", "Polish"),
];
const TEXT_LIMIT: usize = 4_000_000;

#[path = "ui_view.rs"]
mod view;

#[path = "ui_queue.rs"]
mod queue;
use queue::{QueueEntry, QueueStatus};

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;

enum IoEvent {
    Imported(ImportReport),
    ImportCancelled,
    Saved(Result<bool, String>),
}

pub struct WhistleApp {
    worker: Worker,
    ready: bool,
    busy: bool,
    listening: bool,
    stopping: bool,
    cancelled: bool,
    text_limit_reached: bool,
    started: Option<Instant>,
    text: String,
    undo_clear: Option<(String, usize)>,
    transcript_sources: usize,
    pending: String,
    follow_append: bool,
    words: usize,
    language: usize,
    detected: String,
    keywords: String,
    source: String,
    seconds: f64,
    elapsed: f64,
    pass_ms: f64,
    error: Option<String>,
    toast: Option<(String, Instant)>,
    io_send: Sender<IoEvent>,
    io_receive: Receiver<IoEvent>,
    dialog: bool,
    screenshot: Option<PathBuf>,
    screenshot_requested: bool,
    initial_files: Vec<PathBuf>,
    queue: Vec<QueueEntry>,
    queue_paused: bool,
    show_queue: bool,
    include_subfolders: bool,
    import_summary: String,
    import_issues: Vec<String>,
    context: egui::Context,
}
impl WhistleApp {
    pub fn new(
        ctx: &egui::Context,
        screenshot: Option<PathBuf>,
        initial_files: Vec<PathBuf>,
    ) -> Self {
        Self::with_worker(ctx, Worker::spawn(ctx.clone()), screenshot, initial_files)
    }

    fn with_worker(
        ctx: &egui::Context,
        worker: Worker,
        screenshot: Option<PathBuf>,
        initial_files: Vec<PathBuf>,
    ) -> Self {
        configure_style(ctx);
        let (io_send, io_receive) = mpsc::channel();
        Self {
            worker,
            ready: false,
            busy: false,
            listening: false,
            stopping: false,
            cancelled: false,
            text_limit_reached: false,
            started: None,
            text: String::new(),
            undo_clear: None,
            transcript_sources: 0,
            pending: String::new(),
            follow_append: false,
            words: 0,
            language: 0,
            detected: String::new(),
            keywords: String::new(),
            source: "No source selected".into(),
            seconds: 0.0,
            elapsed: 0.0,
            pass_ms: 0.0,
            error: None,
            toast: None,
            io_send,
            io_receive,
            dialog: false,
            screenshot,
            screenshot_requested: false,
            initial_files,
            queue: Vec::new(),
            queue_paused: false,
            show_queue: false,
            include_subfolders: false,
            import_summary: String::new(),
            import_issues: Vec::new(),
            context: ctx.clone(),
        }
    }
    fn notice(&mut self, text: &str, ctx: &egui::Context) {
        self.toast = Some((text.into(), Instant::now()));
        ctx.request_repaint_after(Duration::from_secs(3));
    }
    fn recount(&mut self) {
        self.words = self.text.split_whitespace().count();
    }
    fn events(&mut self) {
        while let Ok(event) = self.worker.events.try_recv() {
            match event {
                Event::Ready => {
                    self.ready = true;
                    self.error = None;
                    if !self.initial_files.is_empty() {
                        let paths = std::mem::take(&mut self.initial_files);
                        self.import_paths(paths);
                    }
                }
                Event::Started => {
                    self.started = Some(Instant::now());
                }
                Event::Text(result) => {
                    if !result.text.is_empty()
                        && self.text.len() + result.text.len() + 2 <= TEXT_LIMIT
                    {
                        if !self.text.is_empty() && !self.text.ends_with(char::is_whitespace) {
                            self.text.push(' ');
                        }
                        self.text.push_str(&result.text);
                        self.follow_append = true;
                        self.words += result.text.split_whitespace().count();
                    } else if !result.text.is_empty() {
                        self.worker.stop.store(true, Ordering::Relaxed);
                        self.stopping = true;
                        self.queue_paused = true;
                        self.text_limit_reached = true;
                        self.error = Some("Transcript limit reached (4 MB). Export and clear the text before continuing.".into());
                    }
                    self.pending = result.pending;
                    if !result.language.is_empty() {
                        self.detected = result.language;
                    }
                    self.seconds = result.received;
                    self.pass_ms = result.pass_ms;
                }
                Event::Finished {
                    cancelled,
                    seconds,
                    elapsed,
                } => {
                    self.busy = false;
                    self.listening = false;
                    self.stopping = false;
                    self.pending.clear();
                    self.seconds = seconds;
                    self.elapsed = elapsed;
                    self.cancelled = cancelled;
                    let text_limit_reached = self.text_limit_reached;
                    if let Some(entry) = self.active_entry() {
                        entry.status = if text_limit_reached {
                            QueueStatus::Failed("Transcript limit reached; part of this file was not appended. Export and clear the text before retrying.".into())
                        } else if cancelled {
                            QueueStatus::Cancelled
                        } else {
                            QueueStatus::Done
                        };
                    }
                }
                Event::Error(error) => {
                    self.busy = false;
                    self.listening = false;
                    self.stopping = false;
                    self.pending.clear();
                    if let Some(entry) = self.active_entry() {
                        entry.status = QueueStatus::Failed(error.clone());
                    }
                    self.error = Some(error);
                }
            }
        }
        while let Ok(event) = self.io_receive.try_recv() {
            self.dialog = false;
            match event {
                IoEvent::Imported(report) => self.enqueue(report),
                IoEvent::ImportCancelled => {}
                IoEvent::Saved(Ok(true)) => {
                    self.toast = Some(("Transcript saved".into(), Instant::now()))
                }
                IoEvent::Saved(Ok(false)) => {}
                IoEvent::Saved(Err(error)) => {
                    self.error = Some(error);
                    self.queue_paused = true;
                }
            }
        }
        self.advance_queue();
    }
    fn start(&mut self, path: Option<PathBuf>) -> bool {
        if !self.ready || self.busy || self.dialog {
            return false;
        }
        if path.is_none() && self.waiting_count() > 0 {
            return false;
        }
        if self.text.len() >= TEXT_LIMIT {
            self.error = Some("Export and clear the transcript before continuing.".into());
            return false;
        }
        let keywords = self
            .keywords
            .split([',', '\n'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let options = match Options::new(LANGUAGES[self.language].0, &keywords) {
            Ok(options) => options,
            Err(error) => {
                self.error = Some(error.to_string());
                return false;
            }
        };
        self.worker.stop.store(false, Ordering::Relaxed);
        self.listening = path.is_none();
        let command = if let Some(path) = path {
            self.source = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            Command::File(path, options)
        } else {
            self.source = "Default microphone".into();
            Command::Listen(options)
        };
        if self.worker.commands.try_send(command).is_err() {
            self.error = Some("Transcription worker unavailable. Restart the app.".into());
            return false;
        }
        if !self.text.is_empty() {
            self.text.push_str("\n\n");
        }
        self.busy = true;
        self.transcript_sources += 1;
        self.cancelled = false;
        self.text_limit_reached = false;
        self.undo_clear = None;
        self.stopping = false;
        self.started = Some(Instant::now());
        self.pending.clear();
        self.error = None;
        self.detected.clear();
        self.seconds = 0.0;
        self.elapsed = 0.0;
        true
    }
    fn stop(&mut self) {
        self.worker.stop.store(true, Ordering::Relaxed);
        self.stopping = true;
        if self
            .queue
            .iter()
            .any(|entry| entry.status == QueueStatus::Active)
        {
            self.queue_paused = true;
        }
    }
    fn pick(&mut self, ctx: &egui::Context, folder: bool) {
        if self.dialog || !self.ready {
            return;
        }
        self.dialog = true;
        let sender = self.io_send.clone();
        let ctx = ctx.clone();
        let recursive = self.include_subfolders;
        std::thread::spawn(move || {
            let paths = if folder {
                rfd::FileDialog::new()
                    .set_title("Import audio folder")
                    .pick_folder()
                    .map(|path| vec![path])
            } else {
                rfd::FileDialog::new()
                    .set_title("Open audio files")
                    .add_filter("Audio", import::AUDIO_EXTENSIONS)
                    .pick_files()
            };
            let event = paths.map_or(IoEvent::ImportCancelled, |paths| {
                IoEvent::Imported(import::discover(paths, recursive))
            });
            let _ = sender.send(event);
            ctx.request_repaint();
        });
    }
    fn save(&mut self, ctx: &egui::Context) {
        if self.dialog || self.text.is_empty() {
            return;
        }
        self.dialog = true;
        let sender = self.io_send.clone();
        let ctx = ctx.clone();
        let text = self.text.clone();
        let file_name = self.export_name();
        std::thread::spawn(move || {
            let result = if let Some(path) = rfd::FileDialog::new()
                .set_title("Export transcript")
                .set_file_name(file_name)
                .add_filter("Text", &["txt"])
                .save_file()
            {
                fastframe_file::write_private(&path, text.as_bytes())
                    .map(|()| true)
                    .map_err(|e| format!("Could not save transcript: {e}"))
            } else {
                Ok(false)
            };
            let _ = sender.send(IoEvent::Saved(result));
            ctx.request_repaint();
        });
    }
    fn copy(&mut self, ctx: &egui::Context) {
        if !self.text.is_empty() {
            ctx.copy_text(self.text.clone());
            self.notice("Copied to clipboard", ctx);
        }
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F6)) {
            ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("transcript")));
        }
        let (folder, open, save, copy, record) = ctx.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::O),
                input.consume_key(egui::Modifiers::CTRL, egui::Key::O),
                input.consume_key(egui::Modifiers::CTRL, egui::Key::S),
                input.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::C),
                input.consume_key(egui::Modifiers::CTRL, egui::Key::Enter),
            )
        });
        if folder {
            self.pick(ctx, true);
        }
        if open {
            self.pick(ctx, false);
        }
        if save {
            self.save(ctx);
        }
        if copy {
            self.copy(ctx);
        }
        if record {
            if self.busy {
                self.stop();
            } else if self.waiting_count() > 0 {
                self.queue_paused = false;
                self.advance_queue();
            } else {
                self.start(None);
            }
        }
        let dropped = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect::<Vec<_>>()
        });
        if !dropped.is_empty() {
            self.import_paths(dropped);
        }
    }

    fn clear(&mut self) {
        if !self.busy && !self.text.is_empty() {
            self.undo_clear = Some((std::mem::take(&mut self.text), self.transcript_sources));
            self.transcript_sources = 0;
            self.recount();
        }
    }

    fn restore_clear(&mut self) {
        if let Some((text, sources)) = self.undo_clear.take() {
            self.text = text;
            self.transcript_sources = sources;
            self.recount();
        }
    }
}

impl eframe::App for WhistleApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.events();
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_input(&ctx);
        self.workspace(ui, &ctx);
        self.queue_window(&ctx);
        if let Some((message, since)) = &self.toast {
            if since.elapsed() < Duration::from_secs(3) {
                egui::Area::new("toast".into())
                    .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -18.0])
                    .show(&ctx, |ui| {
                        egui::Frame::new()
                            .fill(TINT)
                            .corner_radius(10)
                            .inner_margin(12)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(view::icon(icons::CHECK, 18.0, ACCENT));
                                    ui.label(RichText::new(message).size(13.0).color(TEXT));
                                });
                            });
                    });
                ctx.request_repaint_after(Duration::from_secs(3).saturating_sub(since.elapsed()));
            } else {
                self.toast = None;
            }
        }
        if self.busy && self.listening {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if self.screenshot.is_some() {
            if !self.screenshot_requested
                && (self.ready || self.error.is_some())
                && !self.busy
                && !self.dialog
                && (self.waiting_count() == 0 || self.queue_paused)
            {
                self.screenshot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            let events = ctx.input(|input| input.events.clone());
            for event in events {
                if let egui::Event::Screenshot { image, .. } = event
                    && let Some(path) = self.screenshot.take()
                {
                    let bytes: Vec<u8> = image
                        .pixels
                        .iter()
                        .flat_map(|pixel| pixel.to_array())
                        .collect();
                    if let Err(error) = image::save_buffer(
                        path,
                        &bytes,
                        image.width() as u32,
                        image.height() as u32,
                        image::ColorType::Rgba8,
                    ) {
                        self.error = Some(error.to_string());
                    } else {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }
    }
}
fn duration(seconds: f64) -> String {
    let total = seconds as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

fn configure_style(ctx: &egui::Context) {
    let mut fonts = fastframe_fonts::FontSetup::default().definitions();
    let rendering = fastframe_text::detect();
    rendering.apply_to(&mut fonts);
    fonts.font_data.insert(
        "phosphor".into(),
        egui::FontData::from_static(include_bytes!("../assets/Phosphor.ttf")).into(),
    );
    fonts.families.insert(
        egui::FontFamily::Name("phosphor".into()),
        vec!["phosphor".into()],
    );
    ctx.set_fonts(fonts);
    let mut style = egui::Style {
        visuals: egui::Visuals::dark(),
        ..Default::default()
    };
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = CARD;
    style.visuals.extreme_bg_color = INPUT;
    style.visuals.faint_bg_color = SIDEBAR;
    style.visuals.selection.bg_fill = TINT;
    style.visuals.selection.stroke = Stroke::new(1.0, TEXT);
    style.visuals.text_cursor.stroke = Stroke::new(1.5, ACCENT);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.noninteractive,
        &mut style.visuals.widgets.open,
    ] {
        widget.bg_fill = CONTROL;
        widget.weak_bg_fill = CONTROL;
        widget.bg_stroke = Stroke::new(1.0, LINE);
        widget.fg_stroke = Stroke::new(1.0, TEXT);
        widget.corner_radius = 6.into();
        widget.expansion = 0.0;
    }
    style.visuals.widgets.hovered.weak_bg_fill = TINT;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.active.weak_bg_fill = TINT;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.open.weak_bg_fill = TINT;
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 9.0);
    style.spacing.interact_size.y = 36.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, Weight::Regular.font_id(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, Weight::Medium.font_id(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, Weight::Regular.font_id(12.0));
    rendering.apply_to_visuals(&mut style.visuals);
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_style_of(egui::Theme::Dark, style);
}
