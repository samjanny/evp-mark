//! The streaming marker has one job beyond marking: to be the offline path, delayed.

use evp_mark::stft::SAMPLE_RATE;
use evp_mark::{Marker, detect, mark};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const KEY: [u8; 32] = [7u8; 32];

fn bed(seconds: f32) -> Vec<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(31);
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

/// The block size the app's 50 ms render loop actually uses at 48 kHz. It deliberately is not
/// a multiple of the 512-sample STFT hop.
const RENDER_BLOCK: usize = 2_400;

fn stream(samples: &[f32]) -> Vec<f32> {
    let mut marker = Marker::new(SAMPLE_RATE, &KEY).unwrap();
    let mut out = Vec::with_capacity(samples.len());
    for chunk in samples.chunks(RENDER_BLOCK) {
        let mut block = chunk.to_vec();
        marker.process(&mut block);
        out.extend_from_slice(&block);
    }
    out
}

fn offline_delayed(samples: &[f32]) -> Vec<f32> {
    let mut marked = samples.to_vec();
    mark(&mut marked, SAMPLE_RATE, &KEY).unwrap();
    let mut expected = vec![0.0; samples.len()];
    expected[Marker::LATENCY_SAMPLES..]
        .copy_from_slice(&marked[..samples.len() - Marker::LATENCY_SAMPLES]);
    expected
}

fn assert_same_audio(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    let (index, error) = actual
        .iter()
        .zip(expected)
        .enumerate()
        .map(|(index, (actual, expected))| (index, (actual - expected).abs()))
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .unwrap();
    assert!(error < 1e-5, "sample {index} differs by {error}");
}

#[test]
fn what_comes_out_carries_the_mark() {
    let marked = stream(&bed(10.0));
    let verdict = detect(&marked, SAMPLE_RATE, &KEY).unwrap();
    assert!(
        verdict.marked,
        "the streamed audio is not marked: {verdict:?}"
    );
}

#[test]
fn the_stream_gives_back_exactly_what_it_was_given_the_length_of() {
    let input = bed(3.0);
    assert_eq!(stream(&input).len(), input.len());
}

#[test]
fn android_render_blocks_match_the_offline_mark_without_drops_or_padding() {
    let input = bed(3.0);
    assert_same_audio(&stream(&input), &offline_delayed(&input));
}

/// A block of any size, including one that is not a multiple of the hop.
#[test]
fn odd_block_sizes_are_handled() {
    let input = bed(6.0);
    let mut marker = Marker::new(SAMPLE_RATE, &KEY).unwrap();
    let mut out = Vec::with_capacity(input.len());
    let mut cursor = 0usize;
    for size in [37usize, 1024, 5, 2048, 511, 700].iter().cycle() {
        if cursor >= input.len() {
            break;
        }
        let end = (cursor + size).min(input.len());
        let mut block = input[cursor..end].to_vec();
        marker.process(&mut block);
        out.extend_from_slice(&block);
        cursor = end;
    }
    assert_eq!(out.len(), input.len());
    assert_same_audio(&out, &offline_delayed(&input));
    let verdict = detect(&out, SAMPLE_RATE, &KEY).unwrap();
    assert!(verdict.marked, "odd blocks lost the mark: {verdict:?}");
}

#[test]
fn a_rate_it_does_not_know_is_refused() {
    assert!(Marker::new(44_100, &KEY).is_err());
}

/// The latency is a promise the caller plans around, so it is asserted rather than described.
#[test]
fn the_declared_latency_is_the_real_one() {
    assert_eq!(Marker::LATENCY_SAMPLES, 2048);
    let tone: Vec<f32> = (0..SAMPLE_RATE as usize)
        .map(|i| (std::f32::consts::TAU * 2_000.0 * i as f32 / SAMPLE_RATE as f32).sin() * 0.3)
        .collect();
    let out = stream(&tone);

    // The Hann ramp makes the first reconstructed samples deliberately tiny, so an amplitude
    // threshold cannot measure latency. The contract is the exact silent prefix followed by
    // the offline transform at the same sample positions.
    assert!(
        out[..Marker::LATENCY_SAMPLES]
            .iter()
            .all(|sample| *sample == 0.0),
        "the latency prefix contains audio",
    );
    assert_same_audio(&out, &offline_delayed(&tone));
}
