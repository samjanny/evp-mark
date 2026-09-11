//! The sequence the mark is made of, and where in the spectrum it lives.

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use crate::stft::SAMPLE_RATE;

/// 1.3 kHz. Below this the product's own low-pass has shaped the voice and a mark would be
/// sitting in the band people are listening to; above it the bed dominates and masks by
/// construction. Bin 56 at a width of 23.44 Hz.
pub const BAND_LOW_HZ: f32 = 1_300.0;

/// 8 kHz. Above this there is neither voice nor bed on the devices measured, and a
/// perturbation in an empty band is a lone artefact rather than a modulation of anything.
pub const BAND_HIGH_HZ: f32 = 8_000.0;

/// The domain separator. A second one exists for the day the key is rotated; a detector would
/// then try both.
pub const DOMAIN: &str = "evp-voice/mark/v1";

/// Bins are `sample_rate / window` apart; these are the first and last that carry the mark.
#[must_use]
pub const fn band() -> (usize, usize) {
    let width = SAMPLE_RATE as f32 / crate::stft::WINDOW as f32;
    let low = (BAND_LOW_HZ / width) as usize;
    let high = (BAND_HIGH_HZ / width) as usize;
    (low, high)
}

/// A fixed ±1 sequence, one value per carrying bin, and the same sequence in every block.
///
/// Repeating it is the answer to synchronisation, which is the real problem: a single pattern
/// as long as the clip slides and collapses the moment someone trims the first 200 ms or
/// resamples. With one-second tiles the detector searches each block on its own and integrates
/// what it finds.
pub struct Pattern {
    pub low_bin: usize,
    pub values: Vec<f32>,
}

impl Pattern {
    /// From the key and nothing else. Deriving through a domain string rather than using the
    /// key directly keeps this sequence unrelated to anything else the same key ever seeds;
    /// it is the same construction as `evp_core::seed::derive_stream_seed`, restated here so
    /// the crate can be copied into the public repository on its own.
    #[must_use]
    pub fn for_key(key: &[u8; 32]) -> Self {
        let mut hasher = blake3::Hasher::new();
        write_blob(&mut hasher, DOMAIN.as_bytes());
        write_blob(&mut hasher, key);
        let seed: [u8; 32] = *hasher.finalize().as_bytes();

        let (low, high) = band();
        let mut rng = ChaCha8Rng::from_seed(seed);
        let values = (low..=high)
            .map(|_| if rng.random::<bool>() { 1.0 } else { -1.0 })
            .collect();
        Self {
            low_bin: low,
            values,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Patterns that are definitely not ours, for judging a file against itself.
///
/// A signal with a lot of spectral structure — a harmonic stack, a sweep, a buzz — correlates
/// strongly with *any* fixed pattern, and by the same amount every time because the signal is
/// the same every time. Measured against a fixed null that is what a false positive looks like:
/// deterministic, repeatable, and indistinguishable from a mark.
///
/// So the file is judged against itself. The decoys see exactly the same structure, and what
/// counts is whether ours stands out from them. They do not depend on the key, so they are the
/// same reference set for everyone, which is what makes two detectors agree.
#[must_use]
pub fn decoys(count: usize) -> Vec<Pattern> {
    (0..count)
        .map(|index| {
            let mut hasher = blake3::Hasher::new();
            write_blob(&mut hasher, b"evp-voice/mark/decoy/v1");
            hasher.update(&(index as u64).to_le_bytes());
            let seed: [u8; 32] = *hasher.finalize().as_bytes();
            let (low, _) = band();
            let mut rng = ChaCha8Rng::from_seed(seed);
            let values = (0..Pattern::for_key(&[0u8; 32]).len())
                .map(|_| if rng.random::<bool>() { 1.0 } else { -1.0 })
                .collect();
            Pattern {
                low_bin: low,
                values,
            }
        })
        .collect()
}

fn write_blob(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}
