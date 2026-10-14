use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum QueueStatus {
    Waiting,
    Active,
    Done,
    Cancelled,
    Failed(String),
}

pub(super) struct QueueEntry {
    pub path: PathBuf,
    pub status: QueueStatus,
    pub heading: bool,
}

impl WhistleApp {
    pub(super) fn waiting_count(&self) -> usize {
        self.queue
            .iter()
            .filter(|entry| entry.status == QueueStatus::Waiting)
            .count()
    }

    pub(super) fn active_entry(&mut self) -> Option<&mut QueueEntry> {
        self.queue
            .iter_mut()
            .find(|entry| entry.status == QueueStatus::Active)
    }

    pub(super) fn import_paths(&mut self, paths: Vec<PathBuf>) {
        if self.dialog || !self.ready {
            let ctx = self.context.clone();
            self.notice(
                "Wait for the model or current file dialog, then drop again",
                &ctx,
            );
            return;
        }
        self.dialog = true;
        let sender = self.io_send.clone();
        let ctx = self.context.clone();
        let recursive = self.include_subfolders;
        std::thread::spawn(move || {
            let _ = sender.send(IoEvent::Imported(import::discover(paths, recursive)));
            ctx.request_repaint();
        });
    }

    pub(super) fn enqueue(&mut self, report: ImportReport) {
        let heading = report.files.len() > 1 || self.busy || self.waiting_count() > 0;
        let mut duplicates = 0;
        let mut added = 0;
        let mut full = false;
        for path in report.files {
            if self.queue.iter().any(|entry| entry.path == path) {
                duplicates += 1;
            } else if self.queue.len() < import::MAX_FILES {
                self.queue.push(QueueEntry {
                    path,
                    status: QueueStatus::Waiting,
                    heading,
                });
                added += 1;
            } else {
                full = true;
            }
        }
        self.import_summary = if added == 0 && duplicates == 0 && !full {
            "No supported audio files found".into()
        } else {
            let noun = if added == 1 { "file" } else { "files" };
            format!(
                "Added {added} audio {noun} · {duplicates} already in queue · {} skipped",
                report.skipped
            )
        };
        self.import_issues = report.issues;
        if report.truncated || full {
            self.import_issues.push(format!("Import limit reached ({} files or 100,000 folder entries). Clear finished entries or select a smaller folder to import the rest.", import::MAX_FILES));
        }
        self.show_queue =
            self.show_queue || self.queue.len() > 1 || added == 0 || !self.import_issues.is_empty();
    }

    pub(super) fn advance_queue(&mut self) {
        if self.busy || self.dialog || !self.ready || self.queue_paused {
            return;
        }
        let Some(index) = self
            .queue
            .iter()
            .position(|entry| entry.status == QueueStatus::Waiting)
        else {
            return;
        };
        let path = self.queue[index].path.clone();
        let heading = if self.queue[index].heading {
            format!(
                "--- {} ---\n",
                path.file_name().unwrap_or_default().to_string_lossy()
            )
        } else {
            String::new()
        };
        if self.text.len() + heading.len() + 4 >= TEXT_LIMIT {
            self.queue_paused = true;
            self.error = Some("Transcript limit reached (4 MB). Export and clear the text, then resume the queue.".into());
            return;
        }
        if self.start(Some(path)) {
            self.queue[index].status = QueueStatus::Active;
            self.text.push_str(&heading);
            self.recount();
        } else {
            self.queue_paused = true;
        }
    }

    pub(super) fn skip_current(&mut self) {
        if self.busy && !self.listening && !self.stopping {
            self.worker.stop.store(true, Ordering::Relaxed);
            self.stopping = true;
        }
    }

    fn retry_failed(&mut self) {
        for entry in &mut self.queue {
            if matches!(
                entry.status,
                QueueStatus::Failed(_) | QueueStatus::Cancelled
            ) {
                entry.status = QueueStatus::Waiting;
                entry.heading = true;
            }
        }
        // Retrying is explicit; preserve a deliberately paused queue.
        self.advance_queue();
    }

    pub(super) fn export_name(&self) -> String {
        if self.transcript_sources > 1 {
            "transcripts.txt".into()
        } else if self.transcript_sources == 0 || self.source == "Default microphone" {
            "transcript.txt".into()
        } else {
            let mut name = PathBuf::from(&self.source);
            name.set_extension("txt");
            name.to_string_lossy().into_owned()
        }
    }

    pub(super) fn queue_window(&mut self, ctx: &egui::Context) {
        if !self.show_queue {
            return;
        }
        let mut open = self.show_queue;
        egui::Window::new("Import queue")
            .open(&mut open)
            .default_width(500.0)
            .default_pos(egui::pos2(285.0, 100.0))
            .resizable(true)
            .show(ctx, |ui| {
                if self.queue.is_empty() && self.import_summary.is_empty() {
                    ui.label(RichText::new("Import audio files or a folder to build your queue.").color(MUTED));
                    return;
                }
                let waiting = self.waiting_count();
                let done = self.queue.iter().filter(|entry| entry.status == QueueStatus::Done).count();
                let failed = self.queue.iter().filter(|entry| matches!(entry.status, QueueStatus::Failed(_) | QueueStatus::Cancelled)).count();
                ui.label(RichText::new(format!("{done}/{} completed · {waiting} waiting · {failed} failed/cancelled", self.queue.len())).size(13.0));
                if !self.queue.is_empty() {
                    ui.add(egui::ProgressBar::new((done + failed) as f32 / self.queue.len() as f32).fill(ACCENT).desired_height(6.0));
                }
                ui.horizontal_wrapped(|ui| {
                    let label = if self.queue_paused { "Resume queue" } else { "Pause after file" };
                    if ui.add_enabled(waiting > 0, egui::Button::new(label)).clicked() {
                        self.queue_paused = !self.queue_paused;
                        self.advance_queue();
                    }
                    if ui.add_enabled(self.active_entry().is_some() && !self.stopping, egui::Button::new("Skip current")).clicked() {
                        self.skip_current();
                    }
                    if ui.add_enabled(waiting > 0, egui::Button::new("Clear waiting")).clicked() {
                        self.queue.retain(|entry| entry.status != QueueStatus::Waiting);
                    }
                    if ui.add_enabled(failed > 0, egui::Button::new("Retry failed")).on_hover_text("Append a fresh attempt; any confirmed text from earlier attempts is kept. Resume the queue if paused.").clicked() {
                        self.retry_failed();
                    }
                    if ui.add_enabled(done + failed > 0, egui::Button::new("Clear finished")).clicked() {
                        self.queue.retain(|entry| matches!(entry.status, QueueStatus::Waiting | QueueStatus::Active));
                    }
                });
                if self.queue_paused {
                    ui.label(RichText::new("Queue paused · current file may finish").color(ACCENT));
                }
                if !self.import_summary.is_empty() {
                    ui.label(RichText::new(&self.import_summary).size(12.0).color(MUTED));
                }
                if !self.import_issues.is_empty() {
                    egui::CollapsingHeader::new("Import details").show(ui, |ui| {
                        egui::ScrollArea::vertical().id_salt("import-details").max_height(84.0).show(ui, |ui| {
                            for issue in &self.import_issues {
                                ui.label(RichText::new(issue).size(12.0).color(DANGER));
                            }
                        });
                    });
                }
                ui.separator();
                let mut remove = None;
                egui::ScrollArea::vertical().max_height(280.0).auto_shrink([false, true]).show(ui, |ui| {
                    for (index, entry) in self.queue.iter().enumerate() {
                        ui.push_id(index, |ui| {
                            ui.horizontal(|ui| {
                                if entry.status == QueueStatus::Waiting && ui.small_button("Remove").clicked() {
                                    remove = Some(index);
                                }
                                let (label, color) = match &entry.status {
                                    QueueStatus::Waiting => ("Waiting", MUTED),
                                    QueueStatus::Active => ("Transcribing", ACCENT),
                                    QueueStatus::Done => ("Done", ACCENT),
                                    QueueStatus::Cancelled => ("Cancelled", MUTED),
                                    QueueStatus::Failed(_) => ("Failed", DANGER),
                                };
                                ui.label(RichText::new(label).size(12.0).color(color));
                                ui.add(egui::Label::new(entry.path.file_name().unwrap_or_default().to_string_lossy()).truncate()).on_hover_text(entry.path.display().to_string());
                            });
                            if let QueueStatus::Failed(error) = &entry.status {
                                ui.label(RichText::new(error).size(12.0).color(DANGER));
                            }
                        });
                    }
                });
                if let Some(index) = remove {
                    self.queue.remove(index);
                }
            });
        self.show_queue = open;
    }
}
