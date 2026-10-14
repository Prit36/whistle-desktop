use std::sync::atomic::{AtomicBool, Ordering};
use symphonia::core::{checksum::Crc32, io::Monitor};
use whistle_desktop::audio::{MonoResampler, decode_file};

fn convert(rate: u32, input: &[f32], packet: usize) -> Vec<f32> {
    let mut resampler = MonoResampler::new(rate).unwrap();
    let mut output = Vec::new();
    let mut emit = |pcm: &[f32]| {
        output.extend_from_slice(pcm);
        Ok(())
    };
    for part in input.chunks(packet) {
        resampler.push(part, &mut emit).unwrap();
    }
    resampler.finish(&mut emit).unwrap();
    output
}

#[test]
fn eof_preserves_exact_length_even_for_tiny_clips() {
    for rate in [8000, 16000, 22050, 44100, 48000, 96000] {
        for count in [0, 1, 11, 1023, 1024, 1025, 16001] {
            let output = convert(rate, &vec![0.2; count], 317);
            assert_eq!(
                output.len(),
                count * 16000 / rate as usize,
                "rate={rate} count={count}"
            );
            assert!(output.iter().all(|v| v.is_finite()));
        }
    }
}

#[test]
fn packet_boundaries_do_not_change_the_signal() {
    let input: Vec<f32> = (0..88213)
        .map(|n| (n as f32 * std::f32::consts::TAU * 440.0 / 44100.0).sin() * 0.3)
        .collect();
    assert_eq!(convert(44100, &input, 17), convert(44100, &input, 4096));
}

#[test]
fn downsampling_filters_frequencies_above_nyquist() {
    let input: Vec<f32> = (0..48000)
        .map(|n| (n as f32 * std::f32::consts::TAU * 12000.0 / 48000.0).sin() * 0.5)
        .collect();
    let output = convert(48000, &input, 1000);
    let rms = (output[100..15900].iter().map(|s| s * s).sum::<f32>() / 15800.0).sqrt();
    assert!(rms < 0.005, "aliasing RMS {rms}");
}

#[test]
fn stereo_decode_downmixes_and_resamples() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stereo.wav");
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    for _ in 0..48013 {
        writer.write_sample::<i16>(10000).unwrap();
        writer.write_sample::<i16>(-10000).unwrap();
    }
    writer.finalize().unwrap();
    let mut output = Vec::new();
    let result = decode_file(&path, &AtomicBool::new(false), |pcm| {
        output.extend_from_slice(pcm);
        Ok(())
    })
    .unwrap();
    assert_eq!(output.len(), 48013 * 16000 / 48000);
    assert!(output.iter().all(|s| s.abs() < 0.00001));
    assert!(!result.cancelled);
    assert!((result.seconds - 48013.0 / 48000.0).abs() < 1e-6);
    let cancelled = decode_file(&path, &AtomicBool::new(true), |_| {
        panic!("cancelled decode emitted PCM")
    })
    .unwrap();
    assert!(cancelled.cancelled);
}

#[test]
fn opus_voice_notes_decode_with_exact_length_and_finite_pcm() {
    let dir = tempfile::tempdir().unwrap();
    for (name, data, samples) in [
        (
            "voice-note.ogg",
            include_bytes!("fixtures/opus-mono.ogg").as_slice(),
            48013,
        ),
        (
            "voice-note.opus",
            include_bytes!("fixtures/opus-mono.ogg").as_slice(),
            48013,
        ),
        (
            "short.ogg",
            include_bytes!("fixtures/opus-short.ogg").as_slice(),
            483,
        ),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, data).unwrap();
        let mut output = Vec::new();
        let summary = decode_file(&path, &AtomicBool::new(false), |pcm| {
            output.extend_from_slice(pcm);
            Ok(())
        })
        .unwrap();
        assert_eq!(output.len(), samples / 3, "{name}");
        assert!(!summary.cancelled);
        assert!((summary.seconds - samples as f64 / 48000.0).abs() < 1e-6);
        assert!(output.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        let rms = (output.iter().map(|s| s * s).sum::<f32>() / output.len() as f32).sqrt();
        assert!((0.1..0.3).contains(&rms), "{name}: RMS {rms}");
    }
}

#[test]
fn opus_stereo_is_downmixed_before_resampling() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stereo.ogg");
    std::fs::write(&path, include_bytes!("fixtures/opus-stereo.ogg")).unwrap();
    let mut output = Vec::new();
    let summary = decode_file(&path, &AtomicBool::new(false), |pcm| {
        output.extend_from_slice(pcm);
        Ok(())
    })
    .unwrap();
    assert_eq!(output.len(), 96013 / 3);
    assert!((summary.seconds - 96013.0 / 48000.0).abs() < 1e-6);
    let rms = (output.iter().map(|s| s * s).sum::<f32>() / output.len() as f32).sqrt();
    assert!(
        rms < 0.005,
        "opposite stereo channels did not cancel: {rms}"
    );
}

#[test]
fn opus_decode_honors_cancellation_and_consumer_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voice-note.ogg");
    std::fs::write(&path, include_bytes!("fixtures/opus-mono.ogg")).unwrap();
    let stopped = AtomicBool::new(true);
    let summary = decode_file(&path, &stopped, |_| panic!("cancelled decode emitted PCM")).unwrap();
    assert!(summary.cancelled);
    assert_eq!(summary.seconds, 0.0);

    stopped.store(false, Ordering::Relaxed);
    let mut calls = 0;
    let summary = decode_file(&path, &stopped, |_| {
        calls += 1;
        stopped.store(true, Ordering::Relaxed);
        Ok(())
    })
    .unwrap();
    assert!(summary.cancelled);
    assert_eq!(calls, 1);
    assert!(summary.seconds < 1.0);

    let error = decode_file(&path, &AtomicBool::new(false), |_| {
        anyhow::bail!("consumer stopped")
    })
    .err()
    .unwrap();
    assert_eq!(error.to_string(), "consumer stopped");
}

fn opus_without_end_marker() -> Vec<u8> {
    let mut data = include_bytes!("fixtures/opus-mono.ogg").to_vec();
    let offset = data.windows(4).rposition(|s| s == b"OggS").unwrap();
    let page = &mut data[offset..];
    page[5] &= !4;
    update_ogg_crc(page);
    data
}

fn update_ogg_crc(page: &mut [u8]) {
    page[22..26].fill(0);
    let mut crc = Crc32::new(0);
    crc.process_buf_bytes(page);
    page[22..26].copy_from_slice(&crc.crc().to_le_bytes());
}

#[test]
fn opus_without_end_marker_preserves_all_decoded_audio() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("voice-note.ogg");
    let mut outputs = Vec::new();
    for data in [
        include_bytes!("fixtures/opus-mono.ogg").to_vec(),
        opus_without_end_marker(),
    ] {
        std::fs::write(&path, data).unwrap();
        let mut output = Vec::new();
        let summary = decode_file(&path, &AtomicBool::new(false), |pcm| {
            output.extend_from_slice(pcm);
            Ok(())
        })
        .unwrap();
        assert_eq!(output.len(), 48013 / 3);
        assert!((summary.seconds - 48013.0 / 48000.0).abs() < 1e-6);
        assert!(!summary.cancelled);
        outputs.push(output);
    }
    assert_eq!(outputs[0], outputs[1]);
}

#[test]
fn opus_missing_end_marker_does_not_hide_damaged_final_pages() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("damaged.ogg");
    let valid = opus_without_end_marker();
    let mut truncated = valid.clone();
    truncated.pop();
    let mut partial_header = valid.clone();
    partial_header.extend_from_slice(b"OggS");
    let mut incomplete_packet = valid.clone();
    let offset = incomplete_packet
        .windows(4)
        .rposition(|s| s == b"OggS")
        .unwrap();
    let last_segment = offset + 26 + incomplete_packet[offset + 26] as usize;
    let padding = 255 - incomplete_packet[last_segment] as usize;
    incomplete_packet[last_segment] = 255;
    incomplete_packet.resize(incomplete_packet.len() + padding, 0);
    update_ogg_crc(&mut incomplete_packet[offset..]);
    let mut bad_crc = valid;
    *bad_crc.last_mut().unwrap() ^= 1;
    for data in [truncated, partial_header, incomplete_packet, bad_crc] {
        std::fs::write(&path, data).unwrap();
        assert!(decode_file(&path, &AtomicBool::new(false), |_| Ok(())).is_err());
    }
}

#[test]
fn invalid_files_and_invalid_rates_report_errors() {
    assert!(MonoResampler::new(0).is_err());
    assert!(MonoResampler::new(999999).is_err());
    let file = tempfile::NamedTempFile::new().unwrap();
    assert!(decode_file(file.path(), &AtomicBool::new(false), |_| Ok(())).is_err());
}
