use std::sync::atomic::AtomicBool;
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
fn invalid_files_and_invalid_rates_report_errors() {
    assert!(MonoResampler::new(0).is_err());
    assert!(MonoResampler::new(999999).is_err());
    let file = tempfile::NamedTempFile::new().unwrap();
    assert!(decode_file(file.path(), &AtomicBool::new(false), |_| Ok(())).is_err());
}
