use whistle_desktop::{
    engine::{Engine, Options, runtime_dir},
    worker::StreamFeeder,
};

#[test]
fn option_validation_rejects_invalid_language_and_nulls() {
    assert!(Options::new("hi", "").is_err());
    assert!(Options::new("en", "bad\0word").is_err());
    assert!(Options::new("", "Siobhan\nWhistle").is_ok());
}

#[test]
#[ignore = "requires the pinned model and native library; run setup_runtime.py first"]
fn real_engine_silence_stream_reset_and_exclusive_ownership() {
    let mut engine = Engine::load(&runtime_dir()).unwrap();
    assert!(Engine::load(&runtime_dir()).is_err());
    let options = Options::new("en", "").unwrap();
    assert!(
        engine
            .transcribe(&[0.0; 16000], &options)
            .unwrap()
            .text
            .is_empty()
    );
    for _ in 0..2 {
        let mut text = String::new();
        let mut feeder = StreamFeeder::new(&mut engine, &options, 16000, |step| {
            text.push_str(&step.text);
            Ok(())
        });
        feeder.push(&[0.0; 16001]).unwrap();
        feeder.finish().unwrap();
        assert!(text.is_empty());
    }
    assert!(engine.transcribe(&vec![0.0; 480001], &options).is_err());
    drop(engine);
    // Recover from a bad file, then honor cancellation before the next job.
    use std::{sync::atomic::Ordering, time::Duration};
    use whistle_desktop::worker::{Command, Event, Worker};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("silence.wav");
    let mut writer = hound::WavWriter::create(
        &path,
        hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for _ in 0..16000 {
        writer.write_sample::<i16>(0).unwrap();
    }
    writer.finalize().unwrap();
    let worker = Worker::spawn(eframe::egui::Context::default());
    let next = || worker.events.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(matches!(next(), Event::Ready));
    worker
        .commands
        .send(Command::File(
            dir.path().join("missing.mp3"),
            Options::new("en", "").unwrap(),
        ))
        .unwrap();
    assert!(matches!(next(), Event::Started));
    assert!(matches!(next(), Event::Error(_)));
    worker.stop.store(true, Ordering::Relaxed);
    worker
        .commands
        .send(Command::File(path, Options::new("en", "").unwrap()))
        .unwrap();
    assert!(matches!(next(), Event::Started));
    assert!(matches!(
        next(),
        Event::Finished {
            cancelled: true,
            ..
        }
    ));
}
