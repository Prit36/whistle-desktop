use anyhow::{Context, Result};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};
use whistle_desktop::{
    audio::{CaptureStatus, Microphone, SAMPLE_RATE, decode_file},
    engine::{Engine, Options, runtime_dir},
    worker::{FileFeeder, StreamFeeder},
};

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let first = args
        .next()
        .context("Usage: whistle-cli AUDIO_FILE [LANGUAGE] [--stream] | --check-microphone")?;
    if first == "--check-microphone" {
        let mut microphone = Microphone::open(Arc::new(CaptureStatus::default()))?;
        std::thread::sleep(Duration::from_millis(250));
        microphone.stop();
        let mut count = 0;
        while microphone.samples.pop().is_ok() {
            count += 1;
        }
        anyhow::ensure!(count > 0, "Microphone produced no samples");
        println!(
            "Microphone capture OK: {count} mono samples at {} Hz; no audio stored",
            microphone.rate
        );
        return Ok(());
    }
    let path = PathBuf::from(first);
    let rest: Vec<_> = args.map(|s| s.to_string_lossy().into_owned()).collect();
    let language = rest
        .iter()
        .find(|s| !s.starts_with("--"))
        .map_or("", |s| s.as_str());
    let streaming = rest.iter().any(|s| s == "--stream");
    let live = rest.iter().any(|s| s == "--live");
    let options = Options::new(language, "")?;
    let started = Instant::now();
    let mut engine = Engine::load(&runtime_dir())?;
    let loaded = started.elapsed();
    let mut emit = |step: whistle_desktop::engine::Transcript| {
        if !step.text.is_empty() {
            print!("{} ", step.text);
        }
        Ok(())
    };
    let summary = if streaming {
        let mut feeder = StreamFeeder::new(&mut engine, &options, SAMPLE_RATE, &mut emit);
        let summary = decode_file(&path, &AtomicBool::new(false), |pcm| feeder.push(pcm))?;
        feeder.finish()?;
        summary
    } else {
        let mut feeder = if live {
            FileFeeder::new_live(&mut engine, &options, &mut emit)
        } else {
            FileFeeder::new(&mut engine, &options, &mut emit)
        };
        let summary = decode_file(&path, &AtomicBool::new(false), |pcm| feeder.push(pcm))?;
        feeder.finish()?;
        summary
    };
    println!();
    let elapsed = started.elapsed().as_secs_f64();
    eprintln!(
        "model_load_ms={:.1} audio_seconds={:.3} total_seconds={elapsed:.3} realtime_factor={:.1}x",
        loaded.as_secs_f64() * 1000.0,
        summary.seconds,
        summary.seconds / elapsed
    );
    Ok(())
}
