//! What the mark survives, and what it does not. Both halves matter: a compliance claim that
//! oversells itself is worse than a modest one that is true.

use evp_mark::stft::SAMPLE_RATE;
use evp_mark::{detect, mark};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const KEY: [u8; 32] = [7u8; 32];

fn bed(seconds: f32) -> Vec<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(23);
    let len = (SAMPLE_RATE as f32 * seconds) as usize;
    let mut previous = 0.0f32;
    (0..len)
        .map(|_| {
            let white: f32 = rng.random_range(-1.0..1.0);
            previous = 0.98 * previous + 0.02 * white;
            (previous * 8.0).clamp(-1.0, 1.0) * 0.2
        })
        .collect()
}

fn marked(seconds: f32) -> Vec<f32> {
    let mut samples = bed(seconds);
    mark(&mut samples, SAMPLE_RATE, &KEY).unwrap();
    samples
}

/// Linear interpolation, which is a worse resampler than anything real and therefore a
/// harder test than the attack it stands for.
fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    let ratio = from as f32 / to as f32;
    let len = (input.len() as f32 / ratio) as usize;
    (0..len)
        .map(|i| {
            let position = i as f32 * ratio;
            let low = position.floor() as usize;
            let fraction = position - low as f32;
            let a = input.get(low).copied().unwrap_or(0.0);
            let b = input.get(low + 1).copied().unwrap_or(a);
            a + (b - a) * fraction
        })
        .collect()
}

#[test]
fn a_round_trip_through_44_1_khz_keeps_the_mark() {
    let original = marked(10.0);
    let there = resample(&original, SAMPLE_RATE, 44_100);
    let back = resample(&there, 44_100, SAMPLE_RATE);
    let verdict = detect(&back, SAMPLE_RATE, &KEY).unwrap();
    assert!(
        verdict.marked,
        "44.1 kHz round trip lost the mark: {verdict:?}"
    );
}

#[test]
fn a_telephone_band_leaves_the_mark_readable_or_says_it_cannot() {
    // 300 Hz … 3.4 kHz keeps only the bottom third of the carrying band, so this is the case
    // the design expects to degrade. What it must not do is answer confidently either way by
    // accident: it either still detects, or it reports too little to say.
    let original = marked(10.0);
    let mut low = 0.0f32;
    let mut high = 0.0f32;
    let filtered: Vec<f32> = original
        .iter()
        .map(|s| {
            low += 0.35 * (s - low); // roughly 3.4 kHz at 48 kHz
            high += 0.04 * (low - high); // roughly 300 Hz
            low - high
        })
        .collect();
    let verdict = detect(&filtered, SAMPLE_RATE, &KEY).unwrap();
    assert!(
        verdict.marked || verdict.z < 5.0,
        "a telephone band must not produce a confident wrong answer: {verdict:?}",
    );
}

/// Longer audio earns confidence by having more blocks, not by being trusted more.
#[test]
fn confidence_grows_with_the_length_of_the_recording() {
    let short = detect(&marked(4.0), SAMPLE_RATE, &KEY).unwrap();
    let long = detect(&marked(30.0), SAMPLE_RATE, &KEY).unwrap();
    assert!(long.blocks > short.blocks * 5);
    assert!(
        long.z > short.z,
        "30 s read {} against 4 s at {}",
        long.z,
        short.z
    );
}
