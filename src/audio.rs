use anyhow::{Context, Result, bail, ensure};
use audioadapter_buffers::direct::SequentialSlice;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rubato::{
    Async, FixedAsync, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
};
use symphonia::core::{
    checksum::Crc32,
    codecs::{
        audio::{AudioDecoderOptions, well_known::CODEC_ID_OPUS},
        registry::CodecRegistry,
    },
    errors::Error as DecodeError,
    formats::{FormatOptions, TrackType, probe::Hint, well_known::FORMAT_ID_OGG},
    io::{MediaSourceStream, Monitor},
    meta::MetadataOptions,
};

pub const SAMPLE_RATE: usize = 16_000;
const BLOCK: usize = 1024;

fn finite(sample: f32) -> f32 {
    if sample.is_finite() {
        sample.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

/// Continuous sinc conversion; buffers are allocated once, delay is removed, EOF is flushed.
pub struct MonoResampler {
    inner: Option<Async<f32>>,
    input: Vec<f32>,
    output: Vec<f32>,
    held: usize,
    trim: usize,
    rate: u32,
    received: u64,
    emitted: u64,
}
impl MonoResampler {
    pub fn new(rate: u32) -> Result<Self> {
        ensure!(
            (8_000..=384_000).contains(&rate),
            "Unsupported sample rate: {rate} Hz"
        );
        let inner = if rate as usize == SAMPLE_RATE {
            None
        } else {
            let parameters = SincInterpolationParameters::new(128, WindowFunction::BlackmanHarris2)
                .oversampling_factor(128)
                .interpolation(SincInterpolationType::Cubic);
            Some(Async::<f32>::new_sinc(
                SAMPLE_RATE as f64 / rate as f64,
                1.0,
                &parameters,
                BLOCK,
                1,
                FixedAsync::Input,
            )?)
        };
        let trim = inner.as_ref().map_or(0, |r| r.output_delay());
        let capacity = inner.as_ref().map_or(BLOCK, |r| r.output_frames_max());
        Ok(Self {
            inner,
            input: vec![0.0; BLOCK],
            output: vec![0.0; capacity],
            held: 0,
            trim,
            rate,
            received: 0,
            emitted: 0,
        })
    }
    pub fn push(
        &mut self,
        samples: &[f32],
        emit: &mut impl FnMut(&[f32]) -> Result<()>,
    ) -> Result<()> {
        self.received += samples.len() as u64;
        if self.inner.is_none() {
            self.emitted += samples.len() as u64;
            return emit(samples);
        }
        let mut rest = samples;
        while !rest.is_empty() {
            let count = rest.len().min(BLOCK - self.held);
            self.input[self.held..self.held + count].copy_from_slice(&rest[..count]);
            self.held += count;
            rest = &rest[count..];
            if self.held == BLOCK {
                self.block(None, emit)?;
                self.held = 0;
            }
        }
        Ok(())
    }
    fn block(
        &mut self,
        limit: Option<u64>,
        emit: &mut impl FnMut(&[f32]) -> Result<()>,
    ) -> Result<()> {
        let input = SequentialSlice::new(&self.input, 1, BLOCK)?;
        let capacity = self.output.len();
        let mut output = SequentialSlice::new_mut(&mut self.output, 1, capacity)?;
        let (_, produced) = self
            .inner
            .as_mut()
            .context("Resampler not active")?
            .process_into_buffer(&input, &mut output, None)?;
        let skip = self.trim.min(produced);
        self.trim -= skip;
        let count = limit.map_or(produced - skip, |target| {
            (produced - skip).min(target.saturating_sub(self.emitted) as usize)
        });
        if count != 0 {
            emit(&self.output[skip..skip + count])?;
            self.emitted += count as u64;
        }
        Ok(())
    }
    pub fn finish(&mut self, emit: &mut impl FnMut(&[f32]) -> Result<()>) -> Result<()> {
        if self.inner.is_none() {
            return Ok(());
        }
        let target = self.received * SAMPLE_RATE as u64 / self.rate as u64;
        self.input[self.held..].fill(0.0);
        // At most a partial input block and filter tail remain. Padding never leaks to callers.
        while self.emitted < target {
            self.block(Some(target), emit)?;
            self.input.fill(0.0);
        }
        self.held = 0;
        Ok(())
    }
}

pub struct DecodeSummary {
    pub seconds: f64,
    pub cancelled: bool,
}

/// Some voice-note writers omit the OGG end marker. Only accept EOF when the
/// final page is intact, belongs to this stream, and ends on a packet boundary.
/// Reading at most one maximum-size page keeps this check bounded.
fn ogg_ends_on_complete_packet(path: &Path, serial: u32) -> std::io::Result<bool> {
    const MAX_PAGE: u64 = 27 + 255 + 255 * 255;
    let mut file = std::fs::File::open(path)?;
    let length = file.metadata()?.len();
    let tail_length = length.min(MAX_PAGE);
    file.seek(SeekFrom::Start(length - tail_length))?;
    let mut tail = vec![0; tail_length as usize];
    file.read_exact(&mut tail)?;
    for offset in (0..tail.len().saturating_sub(26)).rev() {
        let page = &tail[offset..];
        if &page[..4] != b"OggS" || page[4] != 0 || page[5] & !1 != 0 {
            continue;
        }
        if page[14..18] != serial.to_le_bytes() || page[6..14] == [0xff; 8] {
            continue;
        }
        let header_length = 27 + page[26] as usize;
        let Some(segments) = page.get(27..header_length) else {
            continue;
        };
        if segments.is_empty() || segments.last() == Some(&255) {
            continue;
        }
        let body_length: usize = segments.iter().map(|&n| n as usize).sum();
        if header_length + body_length != page.len() {
            continue;
        }
        let mut crc = Crc32::new(0);
        crc.process_buf_bytes(&page[..22]);
        crc.process_buf_bytes(&[0; 4]);
        crc.process_buf_bytes(&page[26..]);
        if page[22..26] == crc.crc().to_le_bytes() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Packet-by-packet decode; neither compressed audio nor decoded PCM is retained.
pub fn decode_file(
    path: &Path,
    cancel: &AtomicBool,
    mut emit: impl FnMut(&[f32]) -> Result<()>,
) -> Result<DecodeSummary> {
    let source = Box::new(std::fs::File::open(path).context("Could not open audio file")?);
    let stream = MediaSourceStream::new(source, Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|s| s.to_str()) {
        hint.with_extension(extension);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .context("Unsupported or invalid audio file")?;
    let track = format
        .default_track(TrackType::Audio)
        .context("The file contains no audio track")?;
    let id = track.id;
    let parameters = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .context("Missing audio codec parameters")?;
    let ogg_opus = parameters.codec == CODEC_ID_OPUS
        && format.format_info().format == FORMAT_ID_OGG
        && format.tracks().len() == 1;
    // OGG can contain Opus voice notes as well as Vorbis. Symphonia demuxes
    // both, but Opus needs an explicitly registered, statically bundled decoder.
    let mut codecs = CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut codecs);
    codecs.register_audio_decoder::<symphonia_adapter_libopus::OpusDecoder>();
    let mut decoder = codecs
        .make_audio_decoder(parameters, &AudioDecoderOptions::default())
        .context("The file's audio codec is unsupported")?;
    let mut converter: Option<MonoResampler> = None;
    let mut rate = 0;
    let mut frames = 0u64;
    let mut interleaved = Vec::<f32>::new();
    let mut mono = Vec::<f32>::new();
    while !cancel.load(Ordering::Relaxed) {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(DecodeError::IoError(ref error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof
                    && ogg_opus
                    && frames > 0
                    && ogg_ends_on_complete_packet(path, id).unwrap_or(false) =>
            {
                break;
            }
            Err(error) => return Err(error).context("Could not read audio packet"),
        };
        if packet.track_id != id {
            continue;
        }
        let decoded = decoder
            .decode(&packet)
            .context("Damaged or unsupported audio packet")?;
        let current_rate = decoded.spec().rate();
        let channels = decoded.spec().channels().count();
        ensure!(
            (1..=32).contains(&channels),
            "Unsupported channel count: {channels}"
        );
        ensure!(
            decoded.frames() <= 1_048_576,
            "Audio packet exceeds the memory limit"
        );
        if converter.is_none() {
            rate = current_rate;
            converter = Some(MonoResampler::new(rate)?);
        }
        ensure!(
            current_rate == rate,
            "The sample rate changes within this file; convert it to a constant rate first"
        );
        interleaved.resize(decoded.samples_interleaved(), 0.0);
        decoded.copy_to_slice_interleaved(&mut interleaved);
        mono.clear();
        mono.extend(
            interleaved.chunks_exact(channels).map(|frame| {
                finite(frame.iter().copied().map(finite).sum::<f32>() / channels as f32)
            }),
        );
        frames += mono.len() as u64;
        converter
            .as_mut()
            .expect("converter initialized")
            .push(&mono, &mut emit)?;
    }
    let cancelled = cancel.load(Ordering::Relaxed);
    if !cancelled && let Some(converter) = converter.as_mut() {
        converter.finish(&mut emit)?;
    }
    Ok(DecodeSummary {
        seconds: if rate == 0 {
            0.0
        } else {
            frames as f64 / rate as f64
        },
        cancelled,
    })
}

#[derive(Default)]
pub struct CaptureStatus {
    pub level: AtomicU32,
    pub overflow: AtomicBool,
    pub failed: AtomicBool,
}

pub struct Microphone {
    stream: Option<cpal::Stream>,
    pub samples: rtrb::Consumer<f32>,
    pub rate: u32,
    pub status: Arc<CaptureStatus>,
}
impl Microphone {
    pub fn open(status: Arc<CaptureStatus>) -> Result<Self> {
        let device = cpal::default_host()
            .default_input_device()
            .context("No microphone found. Connect one and check Windows microphone permissions")?;
        let config = device
            .default_input_config()
            .context("Could not access the microphone")?;
        let rate = config.sample_rate();
        ensure!(
            (8_000..=384_000).contains(&rate),
            "Unsupported microphone rate: {rate}"
        );
        let (producer, samples) = rtrb::RingBuffer::new(rate as usize * 10);
        let stream_config = config.config();
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                input_stream::<f32>(&device, stream_config, producer, status.clone())?
            }
            cpal::SampleFormat::I16 => {
                input_stream::<i16>(&device, stream_config, producer, status.clone())?
            }
            cpal::SampleFormat::I32 => {
                input_stream::<i32>(&device, stream_config, producer, status.clone())?
            }
            cpal::SampleFormat::U16 => {
                input_stream::<u16>(&device, stream_config, producer, status.clone())?
            }
            other => bail!("Unsupported microphone sample format: {other}"),
        };
        stream.play().context("Could not start the microphone")?;
        Ok(Self {
            stream: Some(stream),
            samples,
            rate,
            status,
        })
    }
    pub fn stop(&mut self) {
        self.stream.take();
        self.status.level.store(0, Ordering::Relaxed);
    }
}
fn input_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut producer: rtrb::Producer<f32>,
    status: Arc<CaptureStatus>,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::Sample,
    f32: cpal::FromSample<T>,
{
    let channels = config.channels as usize;
    ensure!(channels > 0, "Microphone has no channels");
    let errors = status.clone();
    Ok(device.build_input_stream(
        config,
        move |data: &[T], _| {
            let mut peak = 0.0f32;
            for frame in data.chunks_exact(channels) {
                let value = finite(
                    frame
                        .iter()
                        .map(|s| finite(s.to_sample::<f32>()))
                        .sum::<f32>()
                        / channels as f32,
                );
                peak = peak.max(value.abs());
                if producer.push(value).is_err() {
                    status.overflow.store(true, Ordering::Relaxed);
                    break;
                }
            }
            status.level.store(peak.to_bits(), Ordering::Relaxed);
        },
        move |_| {
            errors.failed.store(true, Ordering::Relaxed);
        },
        None,
    )?)
}
