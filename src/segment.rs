//! Pause-aware PCM segmentation. Every input sample is emitted exactly once.
use crate::audio::SAMPLE_RATE;
use anyhow::Result;

const FRAME: usize = SAMPLE_RATE / 50;

pub struct Segmenter {
    pcm: Vec<f32>,
    minimum: usize,
    maximum: usize,
    silence_needed: usize,
    silence: usize,
}
impl Segmenter {
    pub fn new(live: bool) -> Self {
        let maximum = SAMPLE_RATE * if live { 6 } else { 12 };
        Self {
            pcm: Vec::with_capacity(maximum),
            minimum: SAMPLE_RATE * if live { 1 } else { 4 },
            maximum,
            silence_needed: if live {
                SAMPLE_RATE / 2
            } else {
                SAMPLE_RATE / 4
            },
            silence: 0,
        }
    }
    pub fn push(
        &mut self,
        mut samples: &[f32],
        emit: &mut impl FnMut(&[f32]) -> Result<()>,
    ) -> Result<()> {
        while !samples.is_empty() {
            let count = samples.len().min(FRAME - self.pcm.len() % FRAME);
            self.pcm.extend_from_slice(&samples[..count]);
            samples = &samples[count..];
            if self.pcm.len().is_multiple_of(FRAME) {
                let level = energy(&self.pcm[self.pcm.len() - FRAME..]) / FRAME as f32;
                self.silence = if level < 0.000_064 {
                    self.silence + FRAME
                } else {
                    0
                };
                if self.pcm.len() >= self.minimum && self.silence >= self.silence_needed {
                    self.flush(self.pcm.len(), emit)?;
                } else if self.pcm.len() == self.maximum {
                    // Prefer the quietest 20 ms boundary in the final second.
                    let start = self.maximum - SAMPLE_RATE;
                    let index = self.pcm[start..]
                        .as_chunks::<FRAME>()
                        .0
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| energy(*a).total_cmp(&energy(*b)))
                        .map_or(0, |(index, _)| index);
                    self.flush(start + index * FRAME, emit)?;
                }
            }
        }
        Ok(())
    }
    fn flush(&mut self, count: usize, emit: &mut impl FnMut(&[f32]) -> Result<()>) -> Result<()> {
        emit(&self.pcm[..count])?;
        self.pcm.drain(..count);
        self.silence = 0;
        Ok(())
    }
    pub fn finish(&mut self, emit: &mut impl FnMut(&[f32]) -> Result<()>) -> Result<()> {
        if !self.pcm.is_empty() {
            self.flush(self.pcm.len(), emit)?;
        }
        Ok(())
    }
}
fn energy(samples: &[f32]) -> f32 {
    samples.iter().map(|s| s * s).sum()
}
