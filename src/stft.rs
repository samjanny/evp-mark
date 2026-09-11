//! The short-time Fourier transform the mark lives in.
//!
//! A 2048-sample Hann window at a hop of 512 satisfies the constant-overlap-add condition, so
//! analysis followed by synthesis with nothing changed in between returns the input. That
//! property is the whole reason for the numbers: it means the perturbation is the only thing
//! the mark adds, and anything else that comes out different is a bug in this file rather than
//! a cost of marking.

use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::sync::Arc;

/// 2048 samples is 42.7 ms at 48 kHz: long enough that a bin is 23.44 Hz wide and the pattern
/// has somewhere to live, short enough that a block still averages several of them.
pub const WINDOW: usize = 2048;

/// 75 % overlap. Hann at this hop sums to a constant, which is what makes reconstruction exact.
pub const HOP: usize = 512;

/// The rate everything here assumes. The mark is applied after the resampler for exactly this
/// reason: a detector that has to guess the rate is guessing where every bin is.
pub const SAMPLE_RATE: u32 = 48_000;

/// Real input, so only the first half of the spectrum is independent.
pub const BINS: usize = WINDOW / 2 + 1;

/// Hann summed at 75 % overlap is a constant 1.5; synthesis divides it back out.
const COLA_GAIN: f32 = 1.5;

pub struct Stft {
    forward: Arc<dyn Fft<f32>>,
    inverse: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    scratch: Vec<Complex<f32>>,
}

impl Stft {
    #[must_use]
    pub fn new() -> Self {
        let mut planner = FftPlanner::new();
        // Periodic Hann, not symmetric: the symmetric one does not satisfy COLA and the
        // reconstruction test is what catches the difference.
        let window = (0..WINDOW)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / WINDOW as f32).cos())
            .collect();
        Self {
            forward: planner.plan_fft_forward(WINDOW),
            inverse: planner.plan_fft_inverse(WINDOW),
            window,
            scratch: vec![Complex::new(0.0, 0.0); WINDOW],
        }
    }

    /// How many hops fit in a buffer, counting only whole windows.
    #[must_use]
    pub fn frames(len: usize) -> usize {
        if len < WINDOW {
            0
        } else {
            (len - WINDOW) / HOP + 1
        }
    }

    /// Runs `edit` over the magnitudes of each frame and writes the result back into `samples`.
    ///
    /// `edit` receives the frame index and the half-spectrum, and may change magnitudes only:
    /// the phase it is given back is the phase it was handed. Everything before the first
    /// window and after the last is left as it was, because overlap-add cannot reconstruct
    /// what it never covered.
    pub fn process(
        &mut self,
        samples: &mut [f32],
        mut edit: impl FnMut(usize, &mut [Complex<f32>]),
    ) {
        let frames = Self::frames(samples.len());
        if frames == 0 {
            return;
        }
        let covered = (frames - 1) * HOP + WINDOW;
        let mut output = vec![0.0f32; covered];

        for frame in 0..frames {
            let start = frame * HOP;
            for i in 0..WINDOW {
                self.scratch[i] = Complex::new(samples[start + i] * self.window[i], 0.0);
            }
            self.forward.process(&mut self.scratch);

            // The caller sees the half-spectrum; the mirror is rebuilt from it, so a change to
            // bin k automatically applies to its conjugate and the output stays real.
            let mut half: Vec<Complex<f32>> = self.scratch[..BINS].to_vec();
            edit(frame, &mut half);
            self.scratch[..BINS].copy_from_slice(&half);
            for k in 1..BINS - 1 {
                self.scratch[WINDOW - k] = self.scratch[k].conj();
            }

            self.inverse.process(&mut self.scratch);
            let scale = 1.0 / (WINDOW as f32 * COLA_GAIN);
            for i in 0..WINDOW {
                output[start + i] += self.scratch[i].re * self.window[i] * scale;
            }
        }
        samples[..covered].copy_from_slice(&output);
    }
}

impl Default for Stft {
    fn default() -> Self {
        Self::new()
    }
}
