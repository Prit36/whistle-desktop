use super::*;

impl WhistleApp {
    pub(super) fn workspace(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let (bounds, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
        let compact = bounds.width() < 960.0;
        let margin = if compact { 16.0 } else { 20.0 };
        let sidebar_width = if compact { 244.0 } else { 280.0 };
        ui.painter().rect_filled(bounds, 0, BG);

        let header_rect = Rect::from_min_max(
            bounds.min + Vec2::new(margin, 14.0),
            egui::pos2(bounds.right() - margin, bounds.top() + 58.0),
        );
        let mut header = region(ui, "app-header", header_rect);
        self.header(&mut header);
        ui.painter().hline(
            bounds.left() + margin..=bounds.right() - margin,
            bounds.top() + 74.0,
            Stroke::new(1.0, LINE),
        );

        let sidebar = Rect::from_min_max(
            bounds.min + Vec2::new(margin, 94.0),
            egui::pos2(
                bounds.left() + margin + sidebar_width,
                bounds.bottom() - margin,
            ),
        );
        ui.painter()
            .vline(sidebar.right(), sidebar.y_range(), Stroke::new(1.0, LINE));
        let mut controls = region(ui, "sidebar", sidebar.shrink(18.0));
        egui::ScrollArea::vertical()
            .id_salt("controls-scroll")
            .auto_shrink([false, false])
            .show(&mut controls, |ui| self.controls(ui, ctx));

        let main_rect = Rect::from_min_max(
            egui::pos2(sidebar.right() + 20.0, sidebar.top()),
            bounds.max - Vec2::splat(margin),
        );
        let mut main = region(ui, "workspace", main_rect);
        self.document_toolbar(&mut main, ctx);
        main.add_space(12.0);
        if self.error.is_some() {
            self.error_banner(&mut main, ctx);
            main.add_space(10.0);
        }
        let document = Rect::from_min_max(main.cursor().min, main_rect.max);
        self.document(&mut main, document);
        self.drop_overlay(ui, ctx, document);
    }

    fn header(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.label(icon(icons::WAVEFORM, 32.0, ACCENT));
            ui.label(RichText::new("Whistle").font(Weight::SemiBold.font_id(23.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.ready {
                    ui.label(RichText::new("Model loaded").size(12.0).color(MUTED))
                        .on_hover_text("The local speech model is loaded. Recording and file jobs are shown in the transcript panel.");
                    status_dot(ui, SUCCESS);
                } else if self.error.is_some() {
                    ui.label(RichText::new("Setup needed").size(12.0).color(DANGER));
                    ui.label(icon(icons::WARNING, 16.0, DANGER));
                } else {
                    ui.label(RichText::new("Loading local model").size(12.0).color(MUTED));
                    ui.add(egui::Spinner::new().size(13.0).color(ACCENT));
                }
                ui.add_space(10.0);
                ui.label(RichText::new("Private · offline").size(12.0).color(MUTED))
                    .on_hover_text("Audio and transcription stay on this computer. No uploads or account.");
                ui.add_space(10.0);
                let shortcuts = ui
                    .add(egui::Button::new((
                        icon(icons::KEYBOARD, 17.0, MUTED),
                        RichText::new("Shortcuts").size(12.0).color(MUTED),
                    )).frame(false))
                    .on_hover_text("Keyboard shortcuts");
                shortcuts.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        ui.is_enabled(),
                        "Keyboard shortcuts",
                    )
                });
                egui::Popup::menu(&shortcuts).show(|ui| {
                    ui.label(
                        RichText::new("Keyboard shortcuts").font(Weight::SemiBold.font_id(13.0)),
                    );
                    egui::Grid::new("shortcut-reference")
                        .spacing([24.0, 8.0])
                        .show(ui, |ui| {
                            for (action, shortcut) in [
                                ("Open audio", "Ctrl+O"),
                                ("Record / stop", "Ctrl+Enter"),
                                ("Copy transcript", "Ctrl+Shift+C"),
                                ("Export text", "Ctrl+S"),
                                ("Undo editing", "Ctrl+Z"),
                                ("Focus transcript", "F6"),
                            ] {
                                ui.label(action);
                                ui.label(RichText::new(shortcut).color(ACCENT));
                                ui.end_row();
                            }
                        });
                });
            });
        });
    }

    fn controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.spacing_mut().item_spacing.y = 8.0;
        eyebrow(ui, "01  /  AUDIO SOURCE");
        ui.add_space(8.0);
        let enabled = self.ready && !self.busy && !self.dialog;
        let width = ui.available_width();
        if source_button(
            ui,
            enabled,
            icons::UPLOAD,
            "Open audio file",
            true,
            "Ctrl+O",
        )
        .on_hover_text("Open or drop MP3, WAV, FLAC, OGG, M4A, or AAC")
        .clicked()
        {
            self.pick(ctx);
        }
        if self.busy {
            let label = if self.stopping {
                "Finishing…"
            } else if self.listening {
                "Stop recording"
            } else if width < 228.0 {
                "Cancel"
            } else {
                "Cancel transcription"
            };
            ui.scope(|ui| {
                let widgets = &mut ui.style_mut().visuals.widgets;
                widgets.inactive.weak_bg_fill = ERROR_BG;
                widgets.inactive.bg_stroke = Stroke::new(1.0, DANGER.gamma_multiply(0.45));
                widgets.hovered.weak_bg_fill = Color32::from_rgb(78, 40, 45);
                widgets.hovered.bg_stroke = Stroke::new(1.0, DANGER);
                if source_button(ui, !self.stopping, icons::STOP, label, false, "Ctrl+Enter")
                    .clicked()
                {
                    self.stop();
                }
            });
        } else {
            let label = if width < 228.0 {
                "Record"
            } else {
                "Record microphone"
            };
            if source_button(ui, enabled, icons::MICROPHONE, label, false, "Ctrl+Enter")
                .on_hover_text("Record from your default microphone")
                .clicked()
            {
                self.start(None);
            }
        }
        if self.busy && self.listening {
            let level = f32::from_bits(self.worker.capture.level.load(Ordering::Relaxed));
            ui.add(
                egui::ProgressBar::new((level.sqrt() * 1.5).min(1.0))
                    .fill(ACCENT)
                    .desired_height(4.0),
            );
        }
        ui.add_space(20.0);
        ui.separator();
        ui.add_space(14.0);
        eyebrow(ui, "02  /  RECOGNITION");
        ui.add_space(6.0);
        ui.add_enabled_ui(!self.busy, |ui| {
            let widgets = &mut ui.style_mut().visuals.widgets;
            widgets.inactive.bg_stroke = Stroke::new(1.0, FIELD_LINE);
            widgets.noninteractive.bg_stroke = Stroke::new(1.0, FIELD_LINE);
            ui.label(RichText::new("Language").font(Weight::Medium.font_id(13.0)));
            ui.scope(|ui| {
                ui.spacing_mut().interact_size.y = 32.0;
                ui.spacing_mut().button_padding.y = 6.0;
                let widgets = &mut ui.style_mut().visuals.widgets;
                widgets.inactive.weak_bg_fill = INPUT;
                widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
                egui::ComboBox::from_id_salt("language")
                    .selected_text(LANGUAGES[self.language].1)
                    .width(ui.available_width() - 8.0)
                    .show_ui(ui, |ui| {
                        for (i, (_, name)) in LANGUAGES.iter().enumerate() {
                            ui.selectable_value(&mut self.language, i, *name);
                        }
                    });
            });
            ui.add_space(10.0);
            ui.label(RichText::new("Names & keywords").font(Weight::Medium.font_id(13.0)));
            ui.add_sized(
                [ui.available_width(), 88.0],
                egui::TextEdit::multiline(&mut self.keywords)
                    .id(egui::Id::new("keywords"))
                    .lock_focus(false)
                    .hint_text("Names, terms, product names…")
                    .desired_width(f32::INFINITY)
                    .margin(10)
                    .char_limit(2000)
                    .font(Weight::Regular.font_id(13.0)),
            )
            .on_hover_text("Optional context. Separate names or phrases with commas or newlines.");
        });
    }

    fn document_toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            eyebrow(ui, "03 /");
            ui.label(RichText::new("Transcript").font(Weight::SemiBold.font_id(24.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if action_button(
                    ui,
                    !self.text.is_empty() && !self.dialog,
                    icons::DOWNLOAD,
                    "Export .txt",
                    false,
                    Vec2::new(0.0, 36.0),
                )
                .on_hover_text("Ctrl+S · Export transcript")
                .clicked()
                {
                    self.save(ctx);
                }
                if action_button(
                    ui,
                    !self.text.is_empty(),
                    icons::COPY,
                    "Copy text",
                    false,
                    Vec2::new(0.0, 36.0),
                )
                .on_hover_text("Ctrl+Shift+C · Copy transcript")
                .clicked()
                {
                    self.copy(ctx);
                }
            });
        });
    }

    fn document(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let editor_focused = ui
            .ctx()
            .memory(|memory| memory.has_focus(egui::Id::new("transcript")));
        ui.painter().rect_filled(rect, 10, CARD);
        ui.painter().rect_stroke(
            rect,
            10,
            Stroke::new(1.0, if editor_focused { ACCENT } else { LINE }),
            egui::StrokeKind::Inside,
        );
        // A small signature accent keeps the editor recognizably Whistle.
        ui.painter().line_segment(
            [
                rect.min + Vec2::new(14.0, 0.5),
                rect.min + Vec2::new(42.0, 0.5),
            ],
            Stroke::new(2.0, ACCENT),
        );
        let status_rect = Rect::from_min_max(
            rect.min + Vec2::new(18.0, 10.0),
            egui::pos2(rect.right() - 18.0, rect.top() + 34.0),
        );
        let mut status = region(ui, "document-status", status_rect);
        status.horizontal(|ui| {
            if self.busy {
                ui.add(egui::Spinner::new().size(12.0).color(ACCENT));
            } else {
                status_dot(
                    ui,
                    if self.error.is_some() {
                        DANGER
                    } else if self.ready {
                        SUCCESS
                    } else {
                        MUTED
                    },
                );
            }
            let label = if self.stopping {
                if self.listening {
                    "Finishing recording"
                } else {
                    "Cancelling"
                }
            } else if self.busy && self.listening {
                "Listening"
            } else if self.busy {
                "Transcribing"
            } else if self.error.is_some() {
                "Attention needed"
            } else if !self.ready {
                "Waiting for model"
            } else if self.cancelled {
                "Cancelled"
            } else if self.text.is_empty() {
                "Awaiting audio"
            } else {
                "Editable transcript"
            };
            ui.label(
                RichText::new(label)
                    .font(Weight::Medium.font_id(12.0))
                    .color(if self.busy { ACCENT } else { MUTED }),
            );
            if self.busy {
                let seconds = self
                    .started
                    .map_or(0.0, |started| started.elapsed().as_secs_f64());
                let progress = if self.listening {
                    duration(seconds)
                } else if self.seconds > 0.0 {
                    format!("{} processed", duration(self.seconds))
                } else {
                    format!("{} elapsed", duration(seconds))
                };
                ui.label(RichText::new(progress).size(12.0).color(ACCENT));
            }
            if self.source != "No source selected" {
                ui.label(RichText::new("/").size(12.0).color(MUTED));
                ui.add(
                    egui::Label::new(RichText::new(&self.source).size(12.0).color(MUTED))
                        .truncate(),
                )
                .on_hover_text(&self.source);
            }
        });
        ui.painter()
            .hline(rect.x_range(), rect.top() + 44.0, Stroke::new(1.0, LINE));

        let provisional_height = if self.pending.is_empty() { 0.0 } else { 30.0 };
        let body = Rect::from_min_max(
            rect.min + Vec2::new(20.0, 74.0),
            rect.max - Vec2::new(20.0, 58.0 + provisional_height),
        );
        let mut editor = region(ui, "document-editor", body);
        egui::ScrollArea::vertical()
            .id_salt("transcript-scroll")
            .auto_shrink([false, false])
            .stick_to_bottom((self.busy || self.follow_append) && !editor_focused)
            .show(&mut editor, |ui| {
                let hint = if self.error.is_some() && !self.ready {
                    ""
                } else if self.busy {
                    "Transcribing…"
                } else {
                    "Open audio or record to begin."
                };
                let response = ui.add_sized(
                    [ui.available_width(), body.height().max(36.0)],
                    egui::TextEdit::multiline(&mut self.text)
                        .id(egui::Id::new("transcript"))
                        .lock_focus(false)
                        .frame(egui::Frame::NONE)
                        .margin(0)
                        .font(Weight::Regular.font_id(17.0))
                        .desired_width(f32::INFINITY)
                        .hint_text(hint)
                        .char_limit(1_000_000),
                );
                if response.changed() {
                    self.undo_clear = None;
                    self.recount();
                }
            });

        self.follow_append = false;

        if !self.pending.is_empty() {
            let note_rect = Rect::from_min_max(
                egui::pos2(rect.left() + 20.0, rect.bottom() - 76.0),
                egui::pos2(rect.right() - 20.0, rect.bottom() - 48.0),
            );
            let mut note = region(ui, "provisional-words", note_rect);
            note.add(
                egui::Label::new(
                    RichText::new(format!("Hearing: {}", self.pending))
                        .size(12.0)
                        .color(ACCENT),
                )
                .truncate(),
            )
            .on_hover_text("Provisional words may change before they are confirmed.");
        }

        ui.painter()
            .hline(rect.x_range(), rect.bottom() - 42.0, Stroke::new(1.0, LINE));
        let footer_rect = Rect::from_min_max(
            egui::pos2(rect.left() + 18.0, rect.bottom() - 36.0),
            rect.max - Vec2::new(14.0, 4.0),
        );
        let mut footer = region(ui, "document-footer", footer_rect);
        footer.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.label(
                RichText::new(format!("{} words", self.words))
                    .size(12.0)
                    .color(MUTED),
            );
            ui.label(
                RichText::new(format!("{} audio", duration(self.seconds)))
                    .size(12.0)
                    .color(MUTED),
            );
            if !self.detected.is_empty() {
                ui.label(
                    RichText::new(self.detected.to_uppercase())
                        .font(Weight::Medium.font_id(11.0))
                        .color(ACCENT),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if action_button(
                    ui,
                    !self.busy && !self.text.is_empty(),
                    icons::TRASH,
                    "Clear",
                    false,
                    Vec2::new(0.0, 28.0),
                )
                .on_hover_text("Clear transcript · Undo available")
                .clicked()
                {
                    self.clear();
                }
                if self.undo_clear.is_some()
                    && action_button(
                        ui,
                        true,
                        icons::UNDO,
                        "Undo clear",
                        false,
                        Vec2::new(0.0, 28.0),
                    )
                    .clicked()
                {
                    self.restore_clear();
                } else if rect.width() > 660.0 && self.elapsed > 0.0 && self.seconds > 0.0 {
                    ui.label(
                        RichText::new(format!("{:.1}× realtime", self.seconds / self.elapsed))
                            .size(11.0)
                            .color(MUTED),
                    );
                }
            });
        });
    }

    fn error_banner(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(error) = self.error.clone() else {
            return;
        };
        egui::Frame::new()
            .fill(ERROR_BG)
            .corner_radius(7)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.label(icon(icons::WARNING, 19.0, DANGER));
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(if self.ready {
                                "Request failed"
                            } else {
                                "Model unavailable"
                            })
                            .font(Weight::SemiBold.font_id(13.0))
                            .color(DANGER),
                        );
                        egui::ScrollArea::vertical()
                            .id_salt("error-details")
                            .max_height(42.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&error).size(12.0).color(DANGER),
                                    )
                                    .wrap(),
                                );
                            });
                        if !self.ready && ui.small_button("Reload local model").clicked() {
                            self.worker = Worker::spawn(ctx.clone());
                            self.error = None;
                        }
                        if self.ready && ui.small_button("Dismiss").clicked() {
                            self.error = None;
                        }
                    });
                });
            });
    }

    fn drop_overlay(&self, ui: &egui::Ui, ctx: &egui::Context, document: Rect) {
        if !ctx.input(|input| input.raw.hovered_files.is_empty()) {
            let painter = ui.painter().clone().with_layer_id(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop-overlay"),
            ));
            painter.rect_filled(
                document,
                10,
                Color32::from_rgba_unmultiplied(16, 22, 27, 245),
            );
            painter.rect_stroke(
                document,
                10,
                Stroke::new(2.0, ACCENT),
                egui::StrokeKind::Inside,
            );
            painter.text(
                document.center(),
                egui::Align2::CENTER_CENTER,
                if self.ready && !self.busy && !self.dialog {
                    "Drop audio to transcribe"
                } else {
                    "Finish the current task first"
                },
                Weight::Medium.font_id(18.0),
                TEXT,
            );
        }
    }
}

fn region(ui: &mut egui::Ui, id: &str, rect: Rect) -> egui::Ui {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(ui.clip_rect().intersect(rect));
    child.spacing_mut().interact_size.y = rect.height().min(36.0);
    child
}

pub(super) fn icon_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("phosphor".into()))
}

pub(super) fn icon(glyph: &str, size: f32, color: Color32) -> RichText {
    RichText::new(glyph).font(icon_font(size)).color(color)
}

fn button_style(ui: &mut egui::Ui, primary: bool, height: f32) {
    if primary {
        let visuals = &mut ui.style_mut().visuals;
        visuals.widgets.inactive.weak_bg_fill = ACCENT;
        visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        visuals.widgets.hovered.weak_bg_fill = ACCENT_HOVER;
        visuals.widgets.hovered.bg_stroke = Stroke::NONE;
        visuals.widgets.active.weak_bg_fill = ACCENT_HOVER;
        visuals.widgets.active.bg_stroke = Stroke::NONE;
    }
    ui.spacing_mut().button_padding = Vec2::new(10.0, if height < 32.0 { 5.0 } else { 8.0 });
    ui.spacing_mut().interact_size.y = height;
}

fn source_button(
    ui: &mut egui::Ui,
    enabled: bool,
    glyph: &str,
    label: &str,
    primary: bool,
    shortcut: &str,
) -> egui::Response {
    ui.scope(|ui| {
        button_style(ui, primary, 42.0);
        let color = if primary { BG } else { TEXT };
        let shortcut_color = if primary { BG } else { MUTED };
        ui.add_enabled(
            enabled,
            egui::Button::new((
                icon(glyph, 17.0, color),
                RichText::new(label)
                    .font(Weight::Medium.font_id(12.5))
                    .color(color),
            ))
            .right_text(
                RichText::new(shortcut)
                    .font(Weight::Regular.font_id(10.0))
                    .color(shortcut_color),
            )
            .min_size(Vec2::new(ui.available_width(), 42.0))
            .corner_radius(6),
        )
    })
    .inner
}

fn action_button(
    ui: &mut egui::Ui,
    enabled: bool,
    glyph: &str,
    label: &str,
    primary: bool,
    size: Vec2,
) -> egui::Response {
    ui.scope(|ui| {
        button_style(ui, primary, size.y);
        let color = if primary { BG } else { TEXT };
        ui.add_enabled(
            enabled,
            egui::Button::new((
                icon(glyph, 17.0, color),
                RichText::new(label)
                    .font(Weight::Medium.font_id(if size.y < 30.0 { 12.0 } else { 13.0 }))
                    .color(color),
            ))
            .min_size(size)
            .corner_radius(6),
        )
    })
    .inner
}

fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(Weight::Medium.font_id(10.0))
            .color(MUTED),
    );
}

fn status_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 12.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
}
