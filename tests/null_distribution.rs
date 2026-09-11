//! What the detector says about audio that is not ours.
//!
//! The threshold is the one number that decides, and deriving it from a Gaussian nobody
//! checked would be exactly the sort of claim this project does not want to make. So it is
//! measured: many signals that carry no mark of ours, correlated against the real pattern and
//! against decoys, and the tail is where it turns out to be.
//!
//! **What this covers and what it does not.** These are synthetic signals of many shapes, plus
//! our own bed with the embedder off. What is still owed is real recorded audio — speech and
//! music from outside this project — which is the half that says whether someone else's
//! recording can be mistaken for ours. That is task 6 of the plan and needs files this machine
//! does not have.
//!
//! Ignored by default: it takes minutes. Run it with `--ignored` when the constants move.

use evp_mark::stft::SAMPLE_RATE;
use evp_mark::{detect, mark};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::f32::consts::TAU;

const KEY: [u8; 32] = [7u8; 32];

/// Every shape that is not our mark: noise of several colours, tones, sweeps, harmonic stacks,
/// something speech-shaped, and our own bed unmarked.
fn unmarked(kind: usize, seed: u64, seconds: f32) -> Vec<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let len = (SAMPLE_RATE as f32 * seconds) as usize;
    let mut previous = 0.0f32;
    let mut brown = 0.0f32;
    (0..len)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            match kind % 6 {
                // White.
                0 => rng.random_range(-0.3f32..0.3),
                // Pink-ish: our own bed's shape, unmarked.
                1 => {
                    let white: f32 = rng.random_range(-1.0..1.0);
                    previous = 0.98 * previous + 0.02 * white;
                    (previous * 8.0).clamp(-1.0, 1.0) * 0.2
                }
                // Brown, which has almost nothing in the carrying band.
                2 => {
                    brown = (brown + rng.random_range(-0.02f32..0.02)).clamp(-1.0, 1.0);
                    brown * 0.5
                }
                // A harmonic stack, like an instrument.
                3 => {
                    (1..=8)
                        .map(|h| (TAU * 220.0 * h as f32 * t).sin() / h as f32)
                        .sum::<f32>()
                        * 0.1
                }
                // A sweep across the whole carrying band and out the other side.
                4 => (TAU * (200.0 + 9_000.0 * t / seconds) * t).sin() * 0.3,
                // Speech-shaped: three formants over a buzz, with noise on top.
                _ => {
                    let buzz = (TAU * 120.0 * t).sin().signum() * 0.1;
                    let formants: f32 = [700.0f32, 1_220.0, 2_600.0]
                        .iter()
                        .map(|f| (TAU * f * t).sin() * 0.08)
                        .sum();
                    buzz + formants + rng.random_range(-0.02f32..0.02)
                }
            }
        })
        .collect()
}

#[test]
#[ignore = "minutes, not seconds: run it when the constants move"]
fn measure_the_null() {
    let mut statistics = Vec::new();
    let mut block_counts = Vec::new();
    for kind in 0..6usize {
        for seed in 0..40u64 {
            let samples = unmarked(kind, seed * 7 + kind as u64, 8.0);
            for key_byte in [7u8, 8, 9, 10] {
                let key = [key_byte; 32];
                if let Ok(verdict) = detect(&samples, SAMPLE_RATE, &key)
                    && verdict.blocks >= 3
                {
                    statistics.push(verdict.statistic);
                    block_counts.push(verdict.blocks);
                }
            }
        }
    }
    let n = statistics.len() as f32;
    let mean = statistics.iter().sum::<f32>() / n;
    let sd = (statistics
        .iter()
        .map(|s| (s - mean) * (s - mean))
        .sum::<f32>()
        / (n - 1.0))
        .sqrt();
    let blocks = block_counts.iter().sum::<usize>() as f32 / n;
    println!();
    println!(
        "null over {} judgements of unmarked audio, {blocks:.1} blocks each",
        statistics.len()
    );
    println!("  statistic mean {mean:.5}");
    println!("  statistic sd   {sd:.5}");
    println!("  per-block sd   {:.5}", sd * blocks.sqrt());
    let mut sorted = statistics.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    for q in [0.5f64, 0.99, 0.999, 1.0] {
        println!(
            "  q{q:<7} {:.5}",
            sorted[((sorted.len() as f64 - 1.0) * q) as usize]
        );
    }
}

#[test]
#[ignore = "minutes, not seconds: run it when the constants move"]
fn no_unmarked_audio_reaches_the_threshold() {
    let mut worst = f32::MIN;
    let mut worst_shape = 0usize;
    let mut judged = 0usize;
    for kind in 0..6usize {
        for seed in 0..40u64 {
            let samples = unmarked(kind, seed * 7 + kind as u64, 8.0);
            for key_byte in [7u8, 8, 9, 10] {
                let key = [key_byte; 32];
                if let Ok(verdict) = detect(&samples, SAMPLE_RATE, &key)
                    && verdict.blocks >= 3
                {
                    judged += 1;
                    if verdict.z > worst {
                        worst = verdict.z;
                        worst_shape = kind;
                    }
                    assert!(
                        !verdict.marked,
                        "shape {kind} seed {seed} read as marked: {verdict:?}"
                    );
                }
            }
        }
    }
    println!();
    println!("{judged} judgements of unmarked audio, worst z {worst:.2} on shape {worst_shape}");
    assert!(judged > 500, "only {judged} usable judgements");
}

/// The other half: marked audio has to clear the threshold on every shape that can carry it.
#[test]
#[ignore = "minutes, not seconds: run it when the constants move"]
fn marked_audio_clears_the_threshold_on_every_shape_that_can_carry_it() {
    let mut missed = Vec::new();
    for kind in 0..6usize {
        for seed in 0..10u64 {
            let mut samples = unmarked(kind, seed * 13 + kind as u64, 8.0);
            mark(&mut samples, SAMPLE_RATE, &KEY).unwrap();
            let verdict = detect(&samples, SAMPLE_RATE, &KEY).unwrap();
            if !verdict.marked {
                missed.push((kind, seed, verdict.z));
            }
        }
    }
    // A mark cannot be put where there is no signal to modulate, and three of these shapes are
    // nearly tonal: a harmonic stack, a sweep and a buzz with three sine formants put almost
    // nothing in the carrying band, so most of its bins sit under the gate. That is a fact
    // about them, not a failure of the mark — and it is the reason the product's own material
    // is what has to be asserted, because a bed of broadband noise is always underneath
    // everything Aftervoice plays.
    println!();
    println!("shapes that did not carry the mark: {missed:?}");
    let broadband: Vec<_> = missed
        .iter()
        .filter(|(kind, _, _)| *kind == 0 || *kind == 1)
        .collect();
    assert!(
        broadband.is_empty(),
        "broadband material must always carry the mark: {broadband:?}",
    );
}
