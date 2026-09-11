//! The claim the whole crate makes: a marked buffer says so, and an unmarked one does not.

use evp_mark::stft::{SAMPLE_RATE, WINDOW};
use evp_mark::{MarkError, detect, mark};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

const KEY: [u8; 32] = [7u8; 32];
const OTHER_KEY: [u8; 32] = [8u8; 32];

/// Pink-ish noise at a level like the bed's: the material the mark actually rides on, not a
/// sine wave that would make any spectral test look better than it is.
fn bed(seconds: f32) -> Vec<f32> {
    let mut rng = ChaCha8Rng::seed_from_u64(11);
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

#[test]
fn a_marked_buffer_detects_and_an_unmarked_one_does_not() {
    let clean = bed(6.0);
    let mut marked = clean.clone();
    mark(&mut marked, SAMPLE_RATE, &KEY).unwrap();

    let found = detect(&marked, SAMPLE_RATE, &KEY).unwrap();
    let absent = detect(&clean, SAMPLE_RATE, &KEY).unwrap();

    assert!(found.marked, "marked buffer read {found:?}");
    assert!(!absent.marked, "clean buffer read {absent:?}");
    assert!(
        found.z > 5.0,
        "the marked buffer should be far from the null: {found:?}"
    );
    assert!(
        absent.z < 3.0,
        "a clean buffer should sit near the null: {absent:?}"
    );
}

/// The property that makes the statistic mean anything: the answer is about *this* key.
#[test]
fn a_mark_from_another_key_reads_as_no_mark() {
    let mut marked = bed(6.0);
    mark(&mut marked, SAMPLE_RATE, &OTHER_KEY).unwrap();
    let verdict = detect(&marked, SAMPLE_RATE, &KEY).unwrap();
    assert!(
        !verdict.marked,
        "a foreign mark was read as ours: {verdict:?}"
    );
}

/// Scale-free by construction: a gain change is a constant added to every log-magnitude.
#[test]
fn gain_does_not_move_the_statistic() {
    let mut marked = bed(6.0);
    mark(&mut marked, SAMPLE_RATE, &KEY).unwrap();
    let plain = detect(&marked, SAMPLE_RATE, &KEY).unwrap();

    for gain in [0.25f32, 4.0] {
        let loud: Vec<f32> = marked.iter().map(|s| s * gain).collect();
        let verdict = detect(&loud, SAMPLE_RATE, &KEY).unwrap();
        assert!(verdict.marked, "gain {gain} lost the mark: {verdict:?}");
        assert!(
            (verdict.statistic - plain.statistic).abs() < plain.statistic * 0.25,
            "gain {gain} moved the statistic from {} to {}",
            plain.statistic,
            verdict.statistic,
        );
    }
}

/// The tiling exists for this: a cut from an arbitrary offset must not lose the mark.
#[test]
fn trimming_from_an_arbitrary_offset_keeps_the_mark() {
    let mut marked = bed(10.0);
    mark(&mut marked, SAMPLE_RATE, &KEY).unwrap();
    let cut = &marked[9_137..9_137 + SAMPLE_RATE as usize * 5];
    let verdict = detect(cut, SAMPLE_RATE, &KEY).unwrap();
    assert!(
        verdict.marked,
        "a trimmed five seconds lost the mark: {verdict:?}"
    );
}

#[test]
fn silence_is_left_alone_because_every_bin_is_under_the_gate() {
    let mut silence = vec![0.0f32; SAMPLE_RATE as usize];
    mark(&mut silence, SAMPLE_RATE, &KEY).unwrap();
    assert!(
        silence.iter().all(|s| s.abs() < 1e-9),
        "silence was perturbed"
    );
}

#[test]
fn the_refusals_are_explicit() {
    let mut short = vec![0.0f32; WINDOW - 1];
    assert_eq!(
        mark(&mut short, SAMPLE_RATE, &KEY),
        Err(MarkError::TooShort(WINDOW - 1, WINDOW)),
    );
    let mut wrong_rate = vec![0.0f32; WINDOW];
    assert_eq!(
        mark(&mut wrong_rate, 44_100, &KEY),
        Err(MarkError::SampleRate {
            expected: SAMPLE_RATE,
            actual: 44_100
        }),
    );
    let mut broken = vec![0.0f32; WINDOW];
    broken[10] = f32::NAN;
    assert_eq!(
        mark(&mut broken, SAMPLE_RATE, &KEY),
        Err(MarkError::NotFinite)
    );
}

/// Two or fewer blocks is a second of audio: not enough to say anything, and it says so.
#[test]
fn too_little_audio_refuses_to_be_confident() {
    let mut short = bed(1.5);
    mark(&mut short, SAMPLE_RATE, &KEY).unwrap();
    let verdict = detect(&short, SAMPLE_RATE, &KEY).unwrap();
    assert!(
        verdict.blocks < 3,
        "expected too few blocks, got {verdict:?}"
    );
    assert!(
        !verdict.marked,
        "a confident answer from {} blocks",
        verdict.blocks
    );
}

/// The example on the crate's front page is a promise about this API, so it is compiled here.
/// A README that no longer builds is worse than no README: it is a wrong instruction.
#[test]
fn the_readme_example_still_works() {
    use evp_mark::MARK_KEY;

    let mut samples: Vec<f32> = vec![0.0; 48_000 * 12];
    // Silence carries nothing, so the example's shape is exercised on the bed it is meant for.
    let mut state = 0.0f32;
    let mut rng = ChaCha8Rng::seed_from_u64(4);
    for sample in &mut samples {
        let white: f32 = rng.random_range(-1.0..1.0);
        state = 0.98 * state + 0.02 * white;
        *sample = (state * 8.0).clamp(-1.0, 1.0) * 0.2;
    }
    mark(&mut samples, 48_000, &MARK_KEY).unwrap();

    let verdict = detect(&samples, 48_000, &MARK_KEY).unwrap();
    assert!(verdict.marked, "{verdict:?}");
}
