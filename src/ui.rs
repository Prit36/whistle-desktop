use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
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
    worker::{Command, Event, Worker},
};

const BG: Color32 = Color32::from_rgb(16, 22, 27);
const CARD: Color32 = Color32::from_rgb(23, 31, 37);
const LINE: Color32 = Color32::from_rgb(43, 54, 62);
const TEXT: Color32 = Color32::from_rgb(231, 236, 231);
const MUTED: Color32 = Color32::from_rgb(143, 159, 164);
const ACCENT: Color32 = Color32::from_rgb(163, 225, 184);
const DANGER: Color32 = Color32::from_rgb(246, 157, 144);
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

enum IoEvent {
    Picked(Option<PathBuf>),
    Saved(Result<bool, String>),
}

pub struct WhistleApp {
    worker: Worker,
    ready: bool,
    busy: bool,
    listening: bool,
    stopping: bool,
    started: Option<Instant>,
    text: String,
    undo_clear: Option<String>,
    pending: String,
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
    initial_file: Option<PathBuf>,
}
impl WhistleApp {
    pub fn new(
        ctx: &egui::Context,
        screenshot: Option<PathBuf>,
        initial_file: Option<PathBuf>,
    ) -> Self {
        let mut fonts = fastframe_fonts::FontSetup::default().definitions();
        let rendering = fastframe_text::detect();
        rendering.apply_to(&mut fonts);
        ctx.set_fonts(fonts);
        let mut style = egui::Style {
            visuals: egui::Visuals::dark(),
            ..Default::default()
        };
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.panel_fill = BG;
        style.visuals.window_fill = CARD;
        style.visuals.extreme_bg_color = BG;
        style.visuals.faint_bg_color = CARD;
        style.visuals.selection.bg_fill = Color32::from_rgb(55, 91, 75);
        style.visuals.selection.stroke = Stroke::new(1.0, TEXT);
        style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(33, 43, 50);
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
        style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(46, 62, 66);
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
        style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(49, 76, 63);
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
        style.spacing.item_spacing = Vec2::new(10.0, 10.0);
        style.spacing.button_padding = Vec2::new(14.0, 10.0);
        style.spacing.interact_size.y = 36.0;
        style
            .text_styles
            .insert(egui::TextStyle::Body, Weight::Regular.font_id(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, Weight::Medium.font_id(14.0));
        rendering.apply_to_visuals(&mut style.visuals);
        ctx.set_theme(egui::ThemePreference::Dark);
        ctx.set_style_of(egui::Theme::Dark, style);
        let (io_send, io_receive) = mpsc::channel();
        Self {
            worker: Worker::spawn(ctx.clone()),
            ready: false,
            busy: false,
            listening: false,
            stopping: false,
            started: None,
            text: String::new(),
            undo_clear: None,
            pending: String::new(),
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
            initial_file,
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
                    if let Some(path) = self.initial_file.take() {
                        self.start(Some(path));
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
                        self.words += result.text.split_whitespace().count();
                    } else if !result.text.is_empty() {
                        self.worker.stop.store(true, Ordering::Relaxed);
                        self.stopping = true;
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
                    if cancelled {
                        self.source = format!("{} · cancelled", self.source);
                    }
                }
                Event::Error(error) => {
                    self.busy = false;
                    self.listening = false;
                    self.stopping = false;
                    self.pending.clear();
                    self.error = Some(error);
                }
            }
        }
        while let Ok(event) = self.io_receive.try_recv() {
            self.dialog = false;
            match event {
                IoEvent::Picked(Some(path)) => self.start(Some(path)),
                IoEvent::Picked(None) => {}
                IoEvent::Saved(Ok(true)) => {
                    self.toast = Some(("Transcript saved".into(), Instant::now()))
                }
                IoEvent::Saved(Ok(false)) => {}
                IoEvent::Saved(Err(error)) => self.error = Some(error),
            }
        }
    }
    fn start(&mut self, path: Option<PathBuf>) {
        if !self.ready || self.busy || self.dialog {
            return;
        }
        if self.text.len() >= TEXT_LIMIT {
            self.error = Some("Export and clear the transcript before continuing.".into());
            return;
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
                return;
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
            return;
        }
        if !self.text.is_empty() {
            self.text.push_str("\n\n");
        }
        self.busy = true;
        self.undo_clear = None;
        self.stopping = false;
        self.started = Some(Instant::now());
        self.pending.clear();
        self.error = None;
        self.detected.clear();
        self.seconds = 0.0;
        self.elapsed = 0.0;
    }
    fn stop(&mut self) {
        self.worker.stop.store(true, Ordering::Relaxed);
        self.stopping = true;
    }
    fn pick(&mut self, ctx: &egui::Context) {
        if self.dialog || self.busy || !self.ready {
            return;
        }
        self.dialog = true;
        let sender = self.io_send.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let path = rfd::FileDialog::new()
                .set_title("Open audio")
                .add_filter("Audio", &["mp3", "wav", "flac", "ogg", "m4a", "aac"])
                .pick_file();
            let _ = sender.send(IoEvent::Picked(path));
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
        std::thread::spawn(move || {
            let result = if let Some(path) = rfd::FileDialog::new()
                .set_title("Export transcript")
                .set_file_name("transcript.txt")
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

    fn header(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(42.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 12, ACCENT);
            for (i, height) in [10.0, 20.0, 27.0, 16.0, 8.0].iter().enumerate() {
                let x = rect.left() + 11.0 + i as f32 * 5.0;
                ui.painter().line_segment(
                    [
                        egui::pos2(x, rect.center().y - height / 2.0),
                        egui::pos2(x, rect.center().y + height / 2.0),
                    ],
                    Stroke::new(2.5, BG),
                );
            }
            ui.vertical(|ui| {
                ui.label(RichText::new("whistle").font(Weight::SemiBold.font_id(24.0)));
                ui.label(
                    RichText::new("A little space for your words.")
                        .size(12.0)
                        .color(MUTED),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                pill(ui, "ON YOUR DEVICE", ACCENT);
                ui.label(
                    RichText::new("No cloud. No account.")
                        .size(12.0)
                        .color(MUTED),
                );
            });
        });
    }
    fn sidebar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.set_width(244.0);
        eyebrow(ui, "01  /  AUDIO SOURCE");
        ui.add_space(6.0);
        ui.label(RichText::new("Bring your audio.").font(Weight::SemiBold.font_id(22.0)));
        ui.label(
            RichText::new("Open a recording or speak freely. Your audio stays on this computer.")
                .color(MUTED)
                .size(13.0),
        );
        ui.add_space(12.0);
        let enabled = self.ready && !self.busy && !self.dialog;
        if ui
            .add_enabled(
                enabled,
                egui::Button::new(RichText::new("Open audio file").color(BG).strong())
                    .fill(ACCENT)
                    .min_size(Vec2::new(ui.available_width(), 46.0)),
            )
            .on_hover_text("Ctrl+O · MP3, WAV, FLAC, OGG, M4A, AAC")
            .clicked()
        {
            self.pick(ctx);
        }
        ui.label(
            RichText::new("or drop a file anywhere")
                .size(11.0)
                .color(MUTED),
        );
        ui.add_space(8.0);
        if self.busy {
            let label = if self.stopping {
                "Finishing…"
            } else if self.listening {
                "Stop recording"
            } else {
                "Cancel transcription"
            };
            if ui
                .add_enabled(
                    !self.stopping,
                    egui::Button::new(RichText::new(label).color(DANGER))
                        .min_size(Vec2::new(ui.available_width(), 44.0)),
                )
                .clicked()
            {
                self.stop();
            }
        } else if ui
            .add_enabled(
                enabled,
                egui::Button::new("Use microphone").min_size(Vec2::new(ui.available_width(), 44.0)),
            )
            .on_hover_text("Ctrl+Enter · Uses your default input device")
            .clicked()
        {
            self.start(None);
        }
        if self.busy && self.listening {
            let level = f32::from_bits(self.worker.capture.level.load(Ordering::Relaxed));
            ui.add(
                egui::ProgressBar::new((level.sqrt() * 1.5).min(1.0))
                    .fill(ACCENT)
                    .desired_height(4.0),
            );
            ui.label(
                RichText::new("Text appears at pauses · up to 6 s phrases")
                    .color(ACCENT)
                    .size(11.0),
            );
        }
        ui.add_space(18.0);
        ui.separator();
        ui.add_space(12.0);
        eyebrow(ui, "02  /  RECOGNITION");
        ui.add_space(6.0);
        ui.label("Language");
        ui.add_enabled_ui(!self.busy, |ui| {
            egui::ComboBox::from_id_salt("language")
                .selected_text(LANGUAGES[self.language].1)
                .width(ui.available_width() - 8.0)
                .show_ui(ui, |ui| {
                    for (i, (_, name)) in LANGUAGES.iter().enumerate() {
                        ui.selectable_value(&mut self.language, i, *name);
                    }
                });
            ui.add_space(6.0);
            ui.label("Names & keywords");
            ui.add(
                egui::TextEdit::multiline(&mut self.keywords)
                    .hint_text("Siobhan, product names…")
                    .desired_width(f32::INFINITY)
                    .desired_rows(3)
                    .char_limit(2000)
                    .font(egui::TextStyle::Body),
            );
        });
        ui.label(
            RichText::new("Separate phrases with commas. Helps Whistle recognize uncommon words.")
                .size(11.0)
                .color(MUTED),
        );
        ui.add_space(16.0);
        egui::Frame::new().fill(BG).stroke(Stroke::new(1.0, LINE)).corner_radius(10).inner_margin(14).show(ui, |ui| {
            ui.label(RichText::new("Small model. All local.").font(Weight::Medium.font_id(13.0)).color(ACCENT));
            ui.add_space(3.0);
            ui.label(RichText::new("Whistle · 16.9 MB\nCPU inference · 7 languages\nAudio is never saved by the app").size(11.0).color(MUTED));
        });
    }
    fn editor(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                eyebrow(ui, "03  /  YOUR WORDS");
                ui.label(RichText::new("Transcript").font(Weight::SemiBold.font_id(28.0)));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.text.is_empty() && !self.dialog,
                        egui::Button::new("Export .txt"),
                    )
                    .on_hover_text("Ctrl+S")
                    .clicked()
                {
                    self.save(ctx);
                }
                if ui
                    .add_enabled(!self.text.is_empty(), egui::Button::new("Copy text"))
                    .on_hover_text("Ctrl+Shift+C")
                    .clicked()
                {
                    self.copy(ctx);
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let status = if self.stopping {
                "FINISHING"
            } else if self.busy {
                if self.listening {
                    "LIVE"
                } else {
                    "TRANSCRIBING"
                }
            } else if self.ready {
                "READY"
            } else if self.error.is_some() {
                "SETUP NEEDED"
            } else {
                "LOADING MODEL"
            };
            pill(
                ui,
                status,
                if self.error.is_some() { DANGER } else { ACCENT },
            );
            ui.add(
                egui::Label::new(RichText::new(&self.source).size(12.0).color(MUTED)).truncate(),
            );
        });
        ui.add_space(10.0);
        let height =
            (ui.available_height() - 112.0 - if self.error.is_some() { 80.0 } else { 0.0 })
                .max(170.0);
        egui::Frame::new().fill(CARD).stroke(Stroke::new(1.0, LINE)).corner_radius(12).inner_margin(22).show(ui, |ui| {
            ui.set_min_height(height);
            ui.set_max_height(height);
            egui::ScrollArea::vertical().id_salt("transcript-scroll").auto_shrink([false, false]).show(ui, |ui| {
                let response = ui.add_sized([ui.available_width(), height - 10.0], egui::TextEdit::multiline(&mut self.text)
                    .id_salt("transcript").frame(egui::Frame::NONE).font(Weight::Regular.font_id(17.0)).desired_width(f32::INFINITY)
                    .hint_text("Your words will land here.\n\nOpen an audio file or start the microphone.\nThen edit, select, copy, or export your transcript.")
                    .char_limit(1_000_000));
                if response.changed() { self.undo_clear = None; self.recount(); }
            });
        });
        if !self.pending.is_empty() {
            ui.add(egui::Label::new(RichText::new(format!("Hearing: {}", self.pending)).size(12.0).color(MUTED)).truncate())
                .on_hover_text("Provisional words. These may change; confirmed words are added to your transcript.");
        } else {
            ui.label(RichText::new("Click anywhere in the transcript to edit. New recordings append below your text.").size(11.0).color(MUTED));
        }
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} words", self.words))
                    .color(MUTED)
                    .size(12.0),
            );
            ui.label(
                RichText::new(format!("{} audio", duration(self.seconds)))
                    .color(MUTED)
                    .size(12.0),
            );
            if !self.detected.is_empty() {
                ui.label(
                    RichText::new(self.detected.to_uppercase())
                        .color(ACCENT)
                        .size(11.0),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.busy && !self.text.is_empty(),
                        egui::Button::new(RichText::new("Clear").size(12.0)),
                    )
                    .clicked()
                {
                    self.undo_clear = Some(std::mem::take(&mut self.text));
                    self.recount();
                }
                if self.undo_clear.is_some() && ui.button("Undo clear").clicked() {
                    self.text = self.undo_clear.take().unwrap_or_default();
                    self.recount();
                }
                if self.elapsed > 0.0 && self.seconds > 0.0 {
                    ui.label(
                        RichText::new(format!("{:.1}× realtime", self.seconds / self.elapsed))
                            .color(MUTED)
                            .size(11.0),
                    );
                }
            });
        });
        if let Some(error) = self.error.clone() {
            egui::Frame::new()
                .fill(Color32::from_rgb(53, 34, 34))
                .corner_radius(8)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.label(RichText::new(error).color(DANGER).size(12.0));
                    if !self.ready && ui.button("Reload local model").clicked() {
                        self.worker = Worker::spawn(ctx.clone());
                        self.error = None;
                    }
                });
        }
    }
}
impl eframe::App for WhistleApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.events();
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let (open, save, copy, record) = ctx.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::CTRL, egui::Key::O),
                input.consume_key(egui::Modifiers::CTRL, egui::Key::S),
                input.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::C),
                input.consume_key(egui::Modifiers::CTRL, egui::Key::Enter),
            )
        });
        if open {
            self.pick(&ctx);
        }
        if save {
            self.save(&ctx);
        }
        if copy {
            self.copy(&ctx);
        }
        if record {
            if self.busy {
                self.stop();
            } else {
                self.start(None);
            }
        }
        let dropped = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .first()
                .map(|f| f.path().to_path_buf())
        });
        if let Some(path) = dropped {
            self.start(Some(path));
        }
        egui::Frame::new().fill(BG).inner_margin(24).show(ui, |ui| {
            self.header(ui);
            ui.add_space(18.0);
            ui.separator();
            ui.add_space(18.0);
            ui.horizontal_top(|ui| {
                egui::Frame::new()
                    .fill(CARD)
                    .corner_radius(14)
                    .inner_margin(20)
                    .show(ui, |ui| {
                        ui.set_width(244.0);
                        ui.vertical(|ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("controls-scroll")
                                .max_height(ui.available_height() - 48.0)
                                .show(ui, |ui| self.sidebar(ui, &ctx));
                        });
                    });
                ui.add_space(14.0);
                ui.vertical(|ui| self.editor(ui, &ctx));
            });
        });
        if let Some((message, since)) = &self.toast {
            if since.elapsed() < Duration::from_secs(3) {
                egui::Area::new("toast".into())
                    .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -18.0])
                    .show(&ctx, |ui| {
                        egui::Frame::new()
                            .fill(ACCENT)
                            .corner_radius(8)
                            .inner_margin(12)
                            .show(ui, |ui| {
                                ui.label(RichText::new(message).color(BG));
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
            if !self.screenshot_requested && self.ready && !self.busy {
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
fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(Weight::Medium.font_id(10.0))
            .color(MUTED),
    );
}
fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .corner_radius(5)
        .inner_margin(egui::Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .font(Weight::SemiBold.font_id(10.0))
                    .color(color),
            );
        });
}
fn duration(seconds: f64) -> String {
    let total = seconds as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}
