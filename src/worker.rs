use crate::{
    audio::{self, CaptureStatus, Microphone, MonoResampler, SAMPLE_RATE},
    engine::{self, Engine, Options, Transcript},
};
use anyhow::{Context, Result, bail};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

pub enum Command {
    File(PathBuf, Options),
    Listen(Options),
}
pub enum Event {
    Ready,
    Started,
    Text(Transcript),
    Finished {
        cancelled: bool,
        seconds: f64,
        elapsed: f64,
    },
    Error(String),
}
pub struct Worker {
    pub commands: SyncSender<Command>,
    pub events: Receiver<Event>,
    pub stop: Arc<AtomicBool>,
    pub capture: Arc<CaptureStatus>,
}
struct Sink {
    sender: SyncSender<Event>,
    context: egui::Context,
}
impl Sink {
    fn send(&self, event: Event) -> Result<()> {
        self.sender.send(event).context("Window closed")?;
        self.context.request_repaint();
        Ok(())
    }
}
impl Worker {
    pub fn spawn(context: egui::Context) -> Self {
        let (commands, receiver) = mpsc::sync_channel(1);
        let (sender, events) = mpsc::sync_channel(8);
        let stop = Arc::new(AtomicBool::new(false));
        let capture = Arc::new(CaptureStatus::default());
        let control = stop.clone();
        let status = capture.clone();
        std::thread::Builder::new()
            .name("whistle-inference".into())
            .spawn(move || {
                let sink = Sink { sender, context };
                let mut engine = match Engine::load(&engine::runtime_dir()) {
                    Ok(engine) => engine,
                    Err(error) => {
                        let _ = sink.send(Event::Error(format!("{error:#}")));
                        return;
                    }
                };
                if sink.send(Event::Ready).is_err() {
                    return;
                }
                while let Ok(command) = receiver.recv() {
                    let started = Instant::now();
                    let result = match command {
                        Command::File(path, options) => {
                            file_job(&mut engine, &path, &options, &control, &sink)
                        }
                        Command::Listen(options) => {
                            live_job(&mut engine, &options, &control, status.clone(), &sink)
                        }
                    };
                    match result {
                        Ok((cancelled, seconds)) => {
                            if sink
                                .send(Event::Finished {
                                    cancelled,
                                    seconds,
                                    elapsed: started.elapsed().as_secs_f64(),
                                })
                                .is_err()
                            {
                                return;
                            }
                        }
                        Err(error) => {
                            let _ = engine.stop();
                            if sink.send(Event::Error(format!("{error:#}"))).is_err() {
                                return;
                            }
                        }
                    }
                }
            })
            .expect("could not spawn transcription worker");
        Self {
            commands,
            events,
            stop,
            capture,
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Collects a fixed PCM chunk and retains no audio history outside Whistle's own window.
pub struct StreamFeeder<'a, F: FnMut(Transcript) -> Result<()>> {
    engine: &'a mut Engine,
    options: &'a Options,
    pcm: Vec<f32>,
    chunk: usize,
    emit: F,
}
impl<'a, F: FnMut(Transcript) -> Result<()>> StreamFeeder<'a, F> {
    pub fn new(engine: &'a mut Engine, options: &'a Options, chunk: usize, emit: F) -> Self {
        assert!((1..=SAMPLE_RATE * 30).contains(&chunk));
        Self {
            engine,
            options,
            pcm: Vec::with_capacity(chunk),
            chunk,
            emit,
        }
    }
    pub fn push(&mut self, mut samples: &[f32]) -> Result<()> {
        while !samples.is_empty() {
            let count = samples.len().min(self.chunk - self.pcm.len());
            self.pcm.extend_from_slice(&samples[..count]);
            samples = &samples[count..];
            if self.pcm.len() == self.chunk {
                (self.emit)(self.engine.process(&self.pcm, self.options)?)?;
                self.pcm.clear();
            }
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<()> {
        if !self.pcm.is_empty() {
            (self.emit)(self.engine.process(&self.pcm, self.options)?)?;
        }
        (self.emit)(self.engine.stop()?)
    }
}
/// Decodes bounded phrases once, avoiding repeated inference on expanding audio.
pub struct FileFeeder<'a, F: FnMut(Transcript) -> Result<()>> {
    engine: &'a mut Engine,
    options: &'a Options,
    segmenter: crate::segment::Segmenter,
    consumed: usize,
    emit: F,
}
impl<'a, F: FnMut(Transcript) -> Result<()>> FileFeeder<'a, F> {
    pub fn new(engine: &'a mut Engine, options: &'a Options, emit: F) -> Self {
        Self {
            engine,
            options,
            segmenter: crate::segment::Segmenter::new(false),
            consumed: 0,
            emit,
        }
    }
    pub fn new_live(engine: &'a mut Engine, options: &'a Options, emit: F) -> Self {
        Self {
            engine,
            options,
            segmenter: crate::segment::Segmenter::new(true),
            consumed: 0,
            emit,
        }
    }
    pub fn push(&mut self, samples: &[f32]) -> Result<()> {
        let Self {
            engine,
            options,
            segmenter,
            consumed,
            emit,
        } = self;
        segmenter.push(samples, &mut |pcm| {
            transcribe_phrase(engine, options, consumed, emit, pcm)
        })
    }
    pub fn finish(mut self) -> Result<()> {
        self.segmenter.finish(&mut |pcm| {
            transcribe_phrase(
                self.engine,
                self.options,
                &mut self.consumed,
                &mut self.emit,
                pcm,
            )
        })
    }
}

fn transcribe_phrase(
    engine: &mut Engine,
    options: &Options,
    consumed: &mut usize,
    emit: &mut impl FnMut(Transcript) -> Result<()>,
    pcm: &[f32],
) -> Result<()> {
    let started = Instant::now();
    let mut result = engine.transcribe(pcm, options)?;
    *consumed += pcm.len();
    result.received = *consumed as f64 / SAMPLE_RATE as f64;
    result.pass_ms = started.elapsed().as_secs_f64() * 1000.0;
    emit(result)
}

fn file_job(
    engine: &mut Engine,
    path: &std::path::Path,
    options: &Options,
    stop: &AtomicBool,
    sink: &Sink,
) -> Result<(bool, f64)> {
    sink.send(Event::Started)?;
    let mut feeder = FileFeeder::new(engine, options, |text| sink.send(Event::Text(text)));
    let summary = audio::decode_file(path, stop, |pcm| feeder.push(pcm))?;
    if !summary.cancelled {
        feeder.finish()?;
    }
    Ok((summary.cancelled, summary.seconds))
}
fn live_job(
    engine: &mut Engine,
    options: &Options,
    stop: &AtomicBool,
    status: Arc<CaptureStatus>,
    sink: &Sink,
) -> Result<(bool, f64)> {
    status.overflow.store(false, Ordering::Relaxed);
    status.failed.store(false, Ordering::Relaxed);
    let mut microphone = Microphone::open(status.clone())?;
    let mut converter = MonoResampler::new(microphone.rate)?;
    let mut feeder = FileFeeder::new_live(engine, options, |text| sink.send(Event::Text(text)));
    let mut raw = Vec::with_capacity(4096);
    let mut received = 0u64;
    sink.send(Event::Started)?;
    let mut stopping = false;
    loop {
        if status.overflow.load(Ordering::Relaxed) {
            bail!(
                "Microphone buffer overrun: transcription fell over 10 seconds behind. Capture stopped; committed text is preserved."
            );
        }
        if status.failed.load(Ordering::Relaxed) {
            bail!("Microphone disconnected or capture failed. Check the device and try again.");
        }
        if stop.load(Ordering::Relaxed) && !stopping {
            microphone.stop();
            stopping = true;
        }
        raw.clear();
        while raw.len() < 4096 {
            match microphone.samples.pop() {
                Ok(sample) => raw.push(sample),
                Err(_) => break,
            }
        }
        if raw.is_empty() {
            if stopping {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        } else {
            received += raw.len() as u64;
            converter.push(&raw, &mut |pcm| feeder.push(pcm))?;
        }
    }
    converter.finish(&mut |pcm| feeder.push(pcm))?;
    feeder.finish()?;
    Ok((false, received as f64 / microphone.rate as f64))
}
