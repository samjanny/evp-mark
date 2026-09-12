//! Putting the mark in.

use rustfft::num_complex::Complex;

use crate::pattern::Pattern;
use crate::stft::Stft;
use crate::{MarkError, check};

/// Below this a bin is effectively silent, and a perturbation there is a lone artefact rather
/// than a modulation of something already audible. −60 dBFS on the magnitude of a windowed
/// frame, which is the level at which the bed itself has stopped.
pub const GATE_DBFS: f32 = -60.0;

/// How hard the mark pushes: `1 + ALPHA` on a marked bin, about 1.58 dB at this value.
///
/// **Measured, then heard.** The design opened at 0.02 with a search
/// over 0.01 … 0.06; over six-second beds that turned out to separate from the null by 1.5
/// standard errors, which is nothing. What the measurement of 2026-09-09 gives:
///
/// | α | separation |
/// |---|---|
/// | 0.02 | 1.5 σ |
/// | 0.05 | 3.4 σ |
/// | 0.10 | 6.6 σ |
/// | 0.20 | 12.7 σ |
///
/// So the usable range is 0.05 … 0.20, not 0.01 … 0.06. The listening gate on 2026-09-12
/// chose its upper end: the thirty forced choices were 14/30 overall, and a follow-up
/// triangle test at 0.20 was 5/12 (chance is 4/12).
///
/// Spectral separation alone was not enough to accept the upper bound. In this product a mark
/// that is even marginally audible does not become a defect, it becomes part of the phenomenon
/// it is marking — and identical in every generation, which is the opposite of "the voice
/// arrives". That is why the human gate above decides the constant.
pub const ALPHA: f32 = 0.20;

/// Marks a finite buffer in place.
///
/// Not idempotent: marking twice applies the perturbation twice, which is louder and detects
/// no better. Nothing calls it twice, and this note exists so nothing starts to.
pub fn mark(samples: &mut [f32], sample_rate: u32, key: &[u8; 32]) -> Result<(), MarkError> {
    check(samples, sample_rate)?;
    mark_with_alpha(samples, key, ALPHA);
    Ok(())
}

/// The listening test needs to hear several values of it; nothing else should choose one.
pub fn mark_with_alpha(samples: &mut [f32], key: &[u8; 32], alpha: f32) {
    let pattern = Pattern::for_key(key);
    let gate = 10f32.powf(GATE_DBFS / 20.0);
    Stft::new().process(samples, |_, spectrum| {
        for (index, value) in pattern.values.iter().enumerate() {
            let bin = pattern.low_bin + index;
            let magnitude = spectrum[bin].norm();
            if magnitude <= gate {
                continue;
            }
            // Multiplicative, so the energy follows the signal and stays masked by it; phase
            // is handed back exactly as it came, so nothing moves in time.
            let scaled = magnitude * (1.0 + alpha * value);
            spectrum[bin] = Complex::from_polar(scaled, spectrum[bin].arg());
        }
    });
}
