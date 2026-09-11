//! Marking a stream that never ends.
//!
//! The offline path takes a finite buffer and can look at all of it. The live mix cannot: it
//! arrives in blocks, the audio thread is waiting, and whatever comes out has to be the same
//! samples the offline path would have produced, only later.
//!
//! Later by exactly one window. A frame cannot be marked before its last sample has arrived,
//! and the caller must receive the same number of samples it supplied on every call. The marker
//! therefore emits one window of silence up front, then drains finished overlap-add hops from a
//! persistent queue. Keeping that queue across calls matters whenever a caller's block size is
//! not a multiple of the hop.
//!
//! 2048 samples at 48 kHz, about 43 ms. It is declared rather than discovered, and a test
//! measures it, because the caller has to know how far behind the mark puts the sound.

use rustfft::num_complex::Complex;
use std::collections::VecDeque;

use crate::embed::GATE_DBFS;
use crate::pattern::Pattern;
use crate::stft::{HOP, SAMPLE_RATE, Stft, WINDOW};
use crate::{MarkError, embed::ALPHA};

pub struct Marker {
    stft: Stft,
    pattern: Pattern,
    gate: f32,
    alpha: f32,
    /// Samples waiting to become a frame.
    input: Vec<f32>,
    /// Overlap-added output, of which only the first HOP samples of each step are finished.
    output: Vec<f32>,
    /// Finished samples waiting for the caller. This has to survive across calls: Android's
    /// 2,400-sample render block is not a multiple of the 512-sample STFT hop.
    ready: VecDeque<f32>,
    /// Silence still owed before the delayed stream starts.
    latency_remaining: usize,
}

impl Marker {
    /// What the caller gets back is this far behind what it gave. See the note at the top of
    /// the file for why it is a whole window.
    pub const LATENCY_SAMPLES: usize = WINDOW;

    pub fn new(sample_rate: u32, key: &[u8; 32]) -> Result<Self, MarkError> {
        if sample_rate != SAMPLE_RATE {
            return Err(MarkError::SampleRate {
                expected: SAMPLE_RATE,
                actual: sample_rate,
            });
        }
        Ok(Self {
            stft: Stft::new(),
            pattern: Pattern::for_key(key),
            gate: 10f32.powf(GATE_DBFS / 20.0),
            alpha: ALPHA,
            input: Vec::with_capacity(WINDOW * 2),
            output: vec![0.0; WINDOW],
            ready: VecDeque::with_capacity(WINDOW * 2),
            latency_remaining: Self::LATENCY_SAMPLES,
        })
    }

    /// The listening test needs to hear several values; nothing else should choose one.
    #[must_use]
    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    /// Marks `block` in place, replacing it with audio [`Self::LATENCY_SAMPLES`] older.
    ///
    /// The block may be any length. Nothing is allocated after the first few calls: both
    /// buffers reach their working size and stay there.
    pub fn process(&mut self, block: &mut [f32]) {
        self.input.extend_from_slice(block);

        while self.input.len() >= WINDOW {
            let mut frame: Vec<f32> = self.input[..WINDOW].to_vec();
            let alpha = self.alpha;
            let gate = self.gate;
            let pattern = &self.pattern;
            self.stft.process(&mut frame, |_, spectrum| {
                for (index, value) in pattern.values.iter().enumerate() {
                    let bin = pattern.low_bin + index;
                    let magnitude = spectrum[bin].norm();
                    if magnitude <= gate {
                        continue;
                    }
                    spectrum[bin] =
                        Complex::from_polar(magnitude * (1.0 + alpha * value), spectrum[bin].arg());
                }
            });

            // Stft::process overlap-adds the frame's own four hops into the buffer it was
            // given; add that into the running output at the right place and slide on by a hop.
            for (slot, marked) in self.output.iter_mut().zip(&frame) {
                *slot += marked;
            }
            self.ready.extend(&self.output[..HOP]);
            self.output.copy_within(HOP.., 0);
            let tail = self.output.len() - HOP;
            self.output[tail..].fill(0.0);
            self.input.drain(..HOP);
        }

        // Return a continuous delayed stream. Keeping `ready` on the Marker is essential:
        // rounding each individual caller block to whole hops would otherwise discard samples
        // on one call and insert silence on another.
        for sample in block {
            if self.latency_remaining > 0 {
                *sample = 0.0;
                self.latency_remaining -= 1;
            } else {
                *sample = self.ready.pop_front().expect(
                    "a window of latency always leaves one finished sample per input sample",
                );
            }
        }
    }
}
