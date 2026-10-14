use super::*;
use std::sync::{Arc, atomic::AtomicBool, mpsc::SyncSender};
use whistle_desktop::{audio::CaptureStatus, engine::Transcript};

// Exercise the actual widgets and worker messages without recording audio,
// opening OS dialogs, or requiring the native model in ordinary test runs.
struct Harness {
    ctx: egui::Context,
    app: WhistleApp,
    commands: Receiver<Command>,
    events: SyncSender<Event>,
    size: Vec2,
}

impl Harness {
    fn new(size: Vec2) -> Self {
        let ctx = egui::Context::default();
        let (commands, receive_commands) = mpsc::sync_channel(1);
        let (events, receive_events) = mpsc::sync_channel(8);
        let worker = Worker {
            commands,
            events: receive_events,
            stop: Arc::new(AtomicBool::new(false)),
            capture: Arc::new(CaptureStatus::default()),
        };
        let app = WhistleApp::with_worker(&ctx, worker, None, Vec::new());
        let mut harness = Self {
            ctx,
            app,
            commands: receive_commands,
            events,
            size,
        };
        harness.events.send(Event::Ready).unwrap();
        harness.frame(vec![]);
        harness.frame(vec![]);
        harness
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.app.events();
        let ctx = self.ctx.clone();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, self.size)),
                events,
                ..Default::default()
            },
            |ui| {
                self.app.handle_input(&ctx);
                self.app.workspace(ui, &ctx);
                self.app.queue_window(&ctx);
            },
        );
        // This harness inspects layout and interactions, without a GPU renderer.
        output.textures_delta.clear();
        output
    }

    fn click(&mut self, label: &str) -> egui::FullOutput {
        let output = self.frame(vec![]);
        let pos = text_bounds(&output, label).0.center();
        self.click_at(pos)
    }

    fn click_at(&mut self, pos: egui::Pos2) -> egui::FullOutput {
        self.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        self.frame(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }])
    }

    fn key(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> egui::FullOutput {
        self.frame(
            [true, false]
                .into_iter()
                .map(|pressed| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers,
                })
                .collect(),
        )
    }
}

fn text_bounds(output: &egui::FullOutput, label: &str) -> (Rect, Rect) {
    fn find(shape: &egui::Shape, label: &str) -> Option<Rect> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.visual_bounding_rect())
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape, label).map(|rect| (rect, shape.clip_rect)))
        .unwrap_or_else(|| panic!("Missing rendered text: {label}"))
}

fn assert_visible(output: &egui::FullOutput, label: &str) {
    let (text, clip) = text_bounds(output, label);
    assert!(
        clip.expand(1.0).contains_rect(text),
        "{label} is clipped: {text:?} outside {clip:?}"
    );
}

#[test]
fn type_copy_clear_and_restore_through_the_editor_widgets() {
    let mut harness = Harness::new(Vec2::new(1120.0, 760.0));
    harness.click_at(egui::pos2(380.0, 310.0));
    harness.frame(vec![egui::Event::Text("A thought worth keeping.".into())]);
    assert_eq!(harness.app.words, 4);
    let output = harness.click("Copy text");
    assert!(
        output
            .platform_output
            .commands
            .contains(&egui::OutputCommand::CopyText(
                "A thought worth keeping.".into()
            ))
    );
    harness.click("Clear");
    assert!(harness.app.text.is_empty());
    harness.click("Undo clear");
    assert_eq!(harness.app.text, "A thought worth keeping.");
    assert_eq!(harness.app.words, 4);
}

#[test]
fn recognition_recording_and_stop_controls_reach_the_worker() {
    let mut harness = Harness::new(Vec2::new(1120.0, 760.0));
    harness.click("Auto detect");
    harness.click("English");
    assert_eq!(harness.app.language, 1);
    harness.click("Record microphone");
    assert!(matches!(
        harness.commands.try_recv(),
        Ok(Command::Listen(_))
    ));
    assert!(harness.app.busy && harness.app.listening);
    harness
        .events
        .send(Event::Text(Transcript {
            text: "Confirmed words.".into(),
            pending: "The next thought".into(),
            language: "en".into(),
            received: 2.0,
            ..Default::default()
        }))
        .unwrap();
    let output = harness.frame(vec![]);
    assert_visible(&output, "Hearing: The next thought");
    harness.click("Clear");
    assert_eq!(harness.app.text, "Confirmed words.");
    harness.click("Stop recording");
    assert!(harness.app.worker.stop.load(Ordering::Relaxed));
    assert!(harness.app.stopping);
    harness
        .events
        .send(Event::Finished {
            cancelled: false,
            seconds: 2.0,
            elapsed: 0.3,
        })
        .unwrap();
    harness.frame(vec![]);
    assert!(!harness.app.busy);
    assert_eq!(harness.app.text, "Confirmed words.");
}

#[test]
fn minimum_window_keeps_actions_settings_and_error_recovery_visible() {
    let mut harness = Harness::new(Vec2::new(820.0, 600.0));
    let output = harness.frame(vec![]);
    for label in [
        "Open audio files",
        "Import folder",
        "Include subfolders",
        "Record",
        "Auto detect",
        "Names & keywords",
        "Names, terms, product names…",
        "Copy text",
        "Export .txt",
        "Clear",
        "Ctrl+Enter",
        icons::QUEUE,
    ] {
        assert_visible(&output, label);
    }
    harness.click(icons::KEYBOARD);
    let output = harness.frame(vec![]);
    for label in ["Keyboard shortcuts", "Ctrl+Shift+C", "Ctrl+S", "Ctrl+Z"] {
        assert_visible(&output, label);
    }
    harness.click_at(egui::pos2(400.0, 310.0));
    harness.app.busy = true;
    harness.app.listening = false;
    let output = harness.frame(vec![]);
    assert_visible(&output, "Cancel");
    assert_visible(&output, "Ctrl+Enter");
    harness.click("Cancel");
    assert!(harness.app.worker.stop.load(Ordering::Relaxed));
    harness.app.busy = false;
    harness.app.stopping = false;
    harness.app.ready = false;
    harness.app.error = Some(
        "Model missing. Run python scripts/setup_runtime.py to prepare your local runtime.".into(),
    );
    let output = harness.frame(vec![]);
    for label in ["Reload local model", "Export .txt", "Clear"] {
        assert_visible(&output, label);
    }
    harness.app.error = Some("A very long model setup diagnostic. ".repeat(50));
    let output = harness.frame(vec![]);
    assert_visible(&output, "Reload local model");
    assert_visible(&output, "Clear");
    harness.click("Record");
    assert!(harness.commands.try_recv().is_err());
    assert!(!harness.app.busy);
}

#[test]
fn keyboard_focus_can_enter_and_leave_the_editor_and_open_help() {
    let mut harness = Harness::new(Vec2::new(820.0, 600.0));
    harness.key(egui::Key::F6, egui::Modifiers::NONE);
    assert!(
        harness
            .ctx
            .memory(|memory| memory.has_focus(egui::Id::new("transcript")))
    );
    harness.frame(vec![egui::Event::Text("Keyboard editing.".into())]);
    harness.key(egui::Key::Tab, egui::Modifiers::NONE);
    harness.frame(vec![]);
    assert!(
        !harness
            .ctx
            .memory(|memory| memory.has_focus(egui::Id::new("transcript")))
    );
    assert!(!harness.app.text.contains('\t'));
    harness.key(egui::Key::Tab, egui::Modifiers::SHIFT);
    harness.frame(vec![]);
    assert!(
        harness
            .ctx
            .memory(|memory| memory.has_focus(egui::Id::new("transcript")))
    );
    harness
        .ctx
        .memory_mut(|memory| memory.request_focus(egui::Id::new("keywords")));
    harness.frame(vec![egui::Event::Text("Whistle, names".into())]);
    harness.key(egui::Key::Tab, egui::Modifiers::NONE);
    harness.frame(vec![]);
    assert!(
        !harness
            .ctx
            .memory(|memory| memory.has_focus(egui::Id::new("keywords")))
    );
    assert_eq!(harness.app.keywords, "Whistle, names");
    harness.click("Shortcuts");
    let output = harness.frame(vec![]);
    assert_visible(&output, "Focus transcript");
    assert_visible(&output, "F6");
    harness.key(egui::Key::Escape, egui::Modifiers::NONE);
    let output = harness.frame(vec![]);
    assert_visible(&output, "Private · offline");
    harness.key(egui::Key::Enter, egui::Modifiers::CTRL);
    assert!(matches!(
        harness.commands.try_recv(),
        Ok(Command::Listen(_))
    ));
    harness.key(egui::Key::Enter, egui::Modifiers::CTRL);
    assert!(harness.app.worker.stop.load(Ordering::Relaxed));
}

#[test]
fn loading_processing_cancellation_and_error_recovery_keep_confirmed_text() {
    let mut harness = Harness::new(Vec2::new(820.0, 600.0));
    harness.app.ready = false;
    let output = harness.frame(vec![]);
    assert_visible(&output, "Loading local model");
    assert_visible(&output, "Waiting for model");
    harness.events.send(Event::Ready).unwrap();
    harness.frame(vec![]);
    harness.app.start(Some(PathBuf::from("meeting.mp3")));
    assert!(matches!(
        harness.commands.try_recv(),
        Ok(Command::File(_, _))
    ));
    let output = harness.frame(vec![]);
    assert_visible(&output, "Model loaded");
    assert_visible(&output, "Transcribing");
    harness
        .events
        .send(Event::Text(Transcript {
            text: "Keep these confirmed words.".into(),
            received: 12.0,
            ..Default::default()
        }))
        .unwrap();
    let output = harness.frame(vec![]);
    assert_visible(&output, "00:12 processed");
    harness.click("Cancel");
    assert_visible(&harness.frame(vec![]), "Cancelling");
    harness
        .events
        .send(Event::Finished {
            cancelled: true,
            seconds: 12.0,
            elapsed: 0.7,
        })
        .unwrap();
    let output = harness.frame(vec![]);
    assert_visible(&output, "Cancelled");
    assert_eq!(harness.app.source, "meeting.mp3");
    assert_eq!(harness.app.text, "Keep these confirmed words.");
    harness.app.start(None);
    assert!(matches!(
        harness.commands.try_recv(),
        Ok(Command::Listen(_))
    ));
    harness
        .events
        .send(Event::Error(
            "Microphone disconnected. Reconnect it and try again.".into(),
        ))
        .unwrap();
    let output = harness.frame(vec![]);
    assert_visible(&output, "Attention needed");
    assert!(harness.app.text.contains("Keep these confirmed words."));
    harness.click("Dismiss");
    assert!(harness.app.error.is_none());
    assert_visible(&harness.frame(vec![]), "Editable transcript");
    harness.click("Record");
    assert!(matches!(
        harness.commands.try_recv(),
        Ok(Command::Listen(_))
    ));
}

fn long_transcript(paragraphs: usize) -> String {
    (0..paragraphs).map(|i| format!("Paragraph {i:04}. Planning, decisions, and follow-up notes from the recording are kept in order.\n\n")).collect()
}

#[test]
fn long_transcript_supports_end_navigation_editing_undo_and_full_copy() {
    let mut harness = Harness::new(Vec2::new(1120.0, 760.0));
    let original = long_transcript(1000);
    harness.app.text = original.clone();
    harness.app.recount();
    harness.frame(vec![]);
    harness.key(egui::Key::F6, egui::Modifiers::NONE);
    let command = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    harness.key(egui::Key::End, command);
    harness.frame(vec![egui::Event::Text("Final correction.".into())]);
    assert_eq!(harness.app.text, format!("{original}Final correction."));
    for _ in 0..20 {
        harness.frame(vec![]);
    }
    let output = harness.frame(vec![]);
    let (text, clip) = text_bounds(&output, &harness.app.text);
    assert!(
        text.bottom() <= clip.bottom() + 4.0,
        "End of long transcript is outside the editor viewport"
    );
    assert_visible(&output, "Clear");
    harness.key(egui::Key::Z, command);
    assert_eq!(harness.app.text, original);
    let output = harness.key(
        egui::Key::C,
        egui::Modifiers {
            shift: true,
            ..command
        },
    );
    assert!(
        output
            .platform_output
            .commands
            .contains(&egui::OutputCommand::CopyText(original))
    );
}

#[test]
fn live_append_follows_the_end_but_respects_scrolling_back() {
    let mut harness = Harness::new(Vec2::new(1120.0, 760.0));
    harness.app.text = long_transcript(100);
    harness.app.recount();
    harness.app.busy = true;
    harness.app.listening = true;
    for _ in 0..3 {
        harness.frame(vec![]);
    }
    let output = harness.frame(vec![]);
    let (text, clip) = text_bounds(&output, &harness.app.text);
    assert!(text.bottom() <= clip.bottom() + 4.0);
    // The final chunk and completion may arrive together in one UI update.
    harness
        .events
        .send(Event::Text(Transcript {
            text: long_transcript(10),
            ..Default::default()
        }))
        .unwrap();
    harness
        .events
        .send(Event::Finished {
            cancelled: false,
            seconds: 12.0,
            elapsed: 0.8,
        })
        .unwrap();
    for _ in 0..3 {
        harness.frame(vec![]);
    }
    let output = harness.frame(vec![]);
    let (text, clip) = text_bounds(&output, &harness.app.text);
    assert!(
        text.bottom() <= clip.bottom() + 4.0,
        "Completion hid the final confirmed phrase"
    );
    harness.app.busy = true;
    harness.app.listening = true;
    harness.frame(vec![
        egui::Event::PointerMoved(egui::pos2(500.0, 350.0)),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            phase: egui::TouchPhase::Move,
            delta: Vec2::new(0.0, 500.0),
            modifiers: egui::Modifiers::NONE,
        },
    ]);
    for _ in 0..20 {
        harness.frame(vec![]);
    }
    let output = harness.frame(vec![]);
    let before = text_bounds(&output, &harness.app.text).0.top();
    harness
        .events
        .send(Event::Text(Transcript {
            text: long_transcript(10),
            ..Default::default()
        }))
        .unwrap();
    let output = harness.frame(vec![]);
    let after = text_bounds(&output, &harness.app.text).0.top();
    assert!(
        (after - before).abs() < 4.0,
        "New text pulled the user away from the paragraph they were reading"
    );
}

fn import_files(harness: &mut Harness, names: &[&str]) {
    harness
        .app
        .io_send
        .send(IoEvent::Imported(ImportReport {
            files: names.iter().map(PathBuf::from).collect(),
            ..Default::default()
        }))
        .unwrap();
    harness.frame(vec![]);
    harness.frame(vec![]);
}

fn finish_file(harness: &mut Harness, cancelled: bool) {
    harness
        .events
        .send(Event::Finished {
            cancelled,
            seconds: 2.0,
            elapsed: 0.2,
        })
        .unwrap();
    harness.frame(vec![]);
}

#[test]
fn batch_queue_runs_in_order_with_headings_and_source_export_names() {
    let mut harness = Harness::new(Vec2::new(1120.0, 760.0));
    import_files(&mut harness, &["a.wav", "b.opus"]);
    assert!(
        matches!(harness.commands.try_recv(), Ok(Command::File(path, _)) if path == std::path::Path::new("a.wav"))
    );
    assert!(harness.commands.try_recv().is_err());
    harness
        .events
        .send(Event::Text(Transcript {
            text: "First recording.".into(),
            ..Default::default()
        }))
        .unwrap();
    finish_file(&mut harness, false);
    assert_eq!(harness.app.queue[0].status, QueueStatus::Done);
    assert_eq!(harness.app.queue[1].status, QueueStatus::Active);
    assert!(
        matches!(harness.commands.try_recv(), Ok(Command::File(path, _)) if path == std::path::Path::new("b.opus"))
    );
    harness
        .events
        .send(Event::Text(Transcript {
            text: "Second recording.".into(),
            ..Default::default()
        }))
        .unwrap();
    finish_file(&mut harness, false);
    assert_eq!(
        harness.app.text,
        "--- a.wav ---\nFirst recording.\n\n--- b.opus ---\nSecond recording."
    );
    assert_eq!(harness.app.export_name(), "transcripts.txt");
    assert!(!harness.app.busy);
    assert_visible(
        &harness.frame(vec![]),
        "2/2 completed · 0 waiting · 0 failed/cancelled",
    );
    // Clearing queue history does not clear the document.
    harness.click("Clear finished");
    assert!(harness.app.queue.is_empty());
    assert!(harness.app.text.contains("First recording."));
    assert_eq!(harness.app.export_name(), "transcripts.txt");
    harness.app.clear();
    assert_eq!(harness.app.export_name(), "transcript.txt");
    harness.app.restore_clear();
    assert_eq!(harness.app.export_name(), "transcripts.txt");

    let mut single = Harness::new(Vec2::new(1120.0, 760.0));
    import_files(&mut single, &["interview.part.one.MP3"]);
    assert_eq!(single.app.export_name(), "interview.part.one.txt");
    assert!(single.app.text.is_empty()); // Single imports keep the original plain transcript.
    assert!(!single.app.show_queue);
}

#[test]
fn queue_errors_continue_and_retry_appends_a_new_attempt() {
    let mut harness = Harness::new(Vec2::new(1120.0, 760.0));
    import_files(&mut harness, &["bad.wav", "good.wav"]);
    harness.commands.try_recv().unwrap();
    harness
        .events
        .send(Event::Text(Transcript {
            text: "Partial text.".into(),
            ..Default::default()
        }))
        .unwrap();
    harness
        .events
        .send(Event::Error("Damaged audio packet".into()))
        .unwrap();
    let output = harness.frame(vec![]);
    assert_eq!(
        harness.app.queue[0].status,
        QueueStatus::Failed("Damaged audio packet".into())
    );
    assert_visible(&output, "Damaged audio packet");
    assert!(
        matches!(harness.commands.try_recv(), Ok(Command::File(path, _)) if path == std::path::Path::new("good.wav"))
    );
    finish_file(&mut harness, false);
    harness.click("Retry failed");
    assert!(
        matches!(harness.commands.try_recv(), Ok(Command::File(path, _)) if path == std::path::Path::new("bad.wav"))
    );
    assert!(harness.app.text.contains("Partial text."));
    assert_eq!(harness.app.text.matches("--- bad.wav ---").count(), 2);
    assert_eq!(harness.app.queue[0].status, QueueStatus::Active);
}

#[test]
fn cancel_pauses_remaining_files_and_resume_and_skip_reach_the_worker() {
    let mut harness = Harness::new(Vec2::new(1120.0, 760.0));
    import_files(&mut harness, &["a.wav", "b.wav", "c.wav"]);
    harness.commands.try_recv().unwrap();
    harness.key(egui::Key::Enter, egui::Modifiers::CTRL);
    assert!(harness.app.queue_paused);
    finish_file(&mut harness, true);
    assert_eq!(harness.app.queue[0].status, QueueStatus::Cancelled);
    assert_eq!(harness.app.waiting_count(), 2);
    assert!(harness.commands.try_recv().is_err());
    harness.click("Resume queue");
    assert!(
        matches!(harness.commands.try_recv(), Ok(Command::File(path, _)) if path == std::path::Path::new("b.wav"))
    );
    assert!(!harness.app.worker.stop.load(Ordering::Relaxed));
    harness.click("Skip current");
    assert!(harness.app.worker.stop.load(Ordering::Relaxed));
    assert!(!harness.app.queue_paused);
    finish_file(&mut harness, true);
    assert!(
        matches!(harness.commands.try_recv(), Ok(Command::File(path, _)) if path == std::path::Path::new("c.wav"))
    );
    assert_eq!(harness.app.queue[1].status, QueueStatus::Cancelled);
}

#[test]
fn pause_remove_and_clear_waiting_preserve_the_active_file() {
    let mut harness = Harness::new(Vec2::new(820.0, 600.0));
    import_files(&mut harness, &["a.wav", "b.wav", "c.wav"]);
    harness.commands.try_recv().unwrap();
    for label in [
        "Import queue",
        "Pause after file",
        "Skip current",
        "Clear waiting",
        "Retry failed",
        "Clear finished",
        "Auto detect",
        "Names & keywords",
        "Names, terms, product names…",
    ] {
        assert_visible(&harness.frame(vec![]), label);
    }
    harness.app.show_queue = false;
    let output = harness.frame(vec![]);
    assert_visible(&output, icons::QUEUE);
    assert_visible(&output, "2");
    assert!(text_bounds(&output, icons::QUEUE).0.bottom() < 74.0);
    harness.click(icons::QUEUE);
    assert!(harness.app.show_queue);
    harness.click(icons::QUEUE);
    assert!(!harness.app.show_queue);
    harness.click(icons::QUEUE);
    assert!(harness.app.show_queue);
    harness.app.dialog = true;
    assert_visible(&harness.frame(vec![]), "Names, terms, product names…");
    harness.app.dialog = false;
    // Both waiting rows have Remove; the rendered text search selects the first one.
    harness.click("Remove");
    assert_eq!(harness.app.queue.len(), 2);
    assert_eq!(harness.app.queue[1].path, PathBuf::from("c.wav"));
    harness.click("Pause after file");
    assert!(!harness.app.worker.stop.load(Ordering::Relaxed));
    finish_file(&mut harness, false);
    assert!(harness.commands.try_recv().is_err());
    assert_eq!(harness.app.waiting_count(), 1);
    harness.click("Clear waiting");
    assert_eq!(harness.app.queue.len(), 1);
    assert_eq!(harness.app.queue[0].status, QueueStatus::Done);
}

#[test]
fn adding_files_during_transcription_deduplicates_and_does_not_interrupt() {
    let mut harness = Harness::new(Vec2::new(1120.0, 760.0));
    import_files(&mut harness, &["a.wav", "b.wav"]);
    harness.commands.try_recv().unwrap();
    import_files(&mut harness, &["a.wav", "b.wav", "c.wav"]);
    assert_eq!(harness.app.queue.len(), 3);
    assert_eq!(harness.app.waiting_count(), 2);
    assert!(harness.commands.try_recv().is_err());
    assert!(!harness.app.worker.stop.load(Ordering::Relaxed));
    assert!(harness.app.import_summary.contains("2 already in queue"));
    finish_file(&mut harness, false);
    assert!(
        matches!(harness.commands.try_recv(), Ok(Command::File(path, _)) if path == std::path::Path::new("b.wav"))
    );
}

#[test]
fn transcript_limit_pauses_queue_without_losing_waiting_files() {
    let mut harness = Harness::new(Vec2::new(1120.0, 760.0));
    import_files(&mut harness, &["a.wav", "b.wav"]);
    harness.commands.try_recv().unwrap();
    // Exercise the event handler directly to avoid laying out 4 MB of synthetic text.
    harness.app.text = "x".repeat(TEXT_LIMIT - 4);
    harness
        .events
        .send(Event::Text(Transcript {
            text: "Too much text.".into(),
            ..Default::default()
        }))
        .unwrap();
    harness
        .events
        .send(Event::Finished {
            cancelled: false,
            seconds: 2.0,
            elapsed: 0.2,
        })
        .unwrap();
    harness.app.events();
    assert!(harness.app.queue_paused);
    assert!(matches!(
        harness.app.queue[0].status,
        QueueStatus::Failed(_)
    ));
    assert_eq!(harness.app.waiting_count(), 1);
    assert!(harness.commands.try_recv().is_err());
    assert_eq!(harness.app.text.len(), TEXT_LIMIT - 4);
    harness.app.clear();
    harness.app.queue_paused = false;
    harness.app.advance_queue();
    assert!(
        matches!(harness.commands.try_recv(), Ok(Command::File(path, _)) if path == std::path::Path::new("b.wav"))
    );
}

#[test]
fn every_dropped_file_and_folder_is_discovered_while_recording() {
    #[derive(Debug)]
    struct DroppedPath(PathBuf);
    impl egui::DroppedFile for DroppedPath {
        fn path(&self) -> &std::path::Path {
            &self.0
        }
        fn bytes(&self) -> Result<Vec<u8>, String> {
            unreachable!("Discovery only needs paths")
        }
    }
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("nested")).unwrap();
    for name in ["a.wav", "b.opus", "nested/c.mp3"] {
        std::fs::write(root.path().join(name), []).unwrap();
    }
    let mut harness = Harness::new(Vec2::new(1120.0, 760.0));
    harness.app.include_subfolders = true;
    harness.app.start(None);
    harness.commands.try_recv().unwrap();
    let ctx = harness.ctx.clone();
    let _ = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, harness.size)),
            dropped_files: vec![
                Arc::new(DroppedPath(root.path().join("a.wav"))),
                Arc::new(DroppedPath(root.path().into())),
            ],
            ..Default::default()
        },
        |_ui| harness.app.handle_input(&ctx),
    );
    assert!(harness.app.dialog);
    let event = harness
        .app
        .io_receive
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    harness.app.io_send.send(event).unwrap();
    harness.frame(vec![]);
    assert_eq!(harness.app.queue.len(), 3);
    assert_eq!(harness.app.waiting_count(), 3);
    assert!(harness.app.listening);
    assert!(harness.commands.try_recv().is_err());
    finish_file(&mut harness, false);
    assert!(matches!(
        harness.commands.try_recv(),
        Ok(Command::File(_, _))
    ));
}
