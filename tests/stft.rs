//! The one property everything else rests on: the transform gives back what it was given.

use evp_mark::stft::{HOP, Stft, WINDOW};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

fn noise(len: usize) -> Vec<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(7);
    (0..len).map(|_| rng.random_range(-0.5f32..0.5)).collect()
}

#[test]
fn analysis_then_synthesis_returns_the_input() {
    let original = noise(48_000 * 3);
    let mut samples = original.clone();
    Stft::new().process(&mut samples, |_, _| {});

    // Overlap-add cannot reconstruct what fewer than four windows covered, which is the first
    // and last window's worth of samples. Everything between them must come back untouched.
    let frames = Stft::frames(original.len());
    let covered_end = (frames - 1) * HOP + WINDOW;
    for i in WINDOW..covered_end - WINDOW {
        assert!(
            (samples[i] - original[i]).abs() < 1e-5,
            "sample {i} came back as {} instead of {}",
            samples[i],
            original[i],
        );
    }
}

#[test]
fn a_buffer_shorter_than_one_window_is_left_alone() {
    let original = noise(WINDOW - 1);
    let mut samples = original.clone();
    Stft::new().process(&mut samples, |_, _| panic!("nothing to edit"));
    assert_eq!(samples, original);
}

#[test]
fn frames_counts_whole_windows_only() {
    assert_eq!(Stft::frames(0), 0);
    assert_eq!(Stft::frames(WINDOW - 1), 0);
    assert_eq!(Stft::frames(WINDOW), 1);
    assert_eq!(Stft::frames(WINDOW + HOP), 2);
    assert_eq!(Stft::frames(WINDOW + HOP - 1), 1);
}
