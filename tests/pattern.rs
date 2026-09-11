use evp_mark::pattern::{Pattern, band};

const KEY: [u8; 32] = [7u8; 32];
const OTHER: [u8; 32] = [8u8; 32];

#[test]
fn the_band_is_the_one_the_design_names() {
    let (low, high) = band();
    assert_eq!(low, 55, "1.3 kHz at 23.44 Hz a bin");
    assert_eq!(high, 341, "8 kHz at 23.44 Hz a bin");
    assert_eq!(Pattern::for_key(&KEY).len(), high - low + 1);
}

#[test]
fn the_sequence_is_plus_or_minus_one_and_nothing_else() {
    let pattern = Pattern::for_key(&KEY);
    assert!(pattern.values.iter().all(|v| *v == 1.0 || *v == -1.0));
}

#[test]
fn the_same_key_gives_the_same_sequence_and_a_different_key_does_not() {
    assert_eq!(Pattern::for_key(&KEY).values, Pattern::for_key(&KEY).values);
    assert_ne!(
        Pattern::for_key(&KEY).values,
        Pattern::for_key(&OTHER).values
    );
}

/// A biased sequence would tilt the spectrum, which is audible in a way the pattern is not.
#[test]
fn the_sequence_has_no_mean_worth_hearing() {
    let pattern = Pattern::for_key(&KEY);
    let mean: f32 = pattern.values.iter().sum::<f32>() / pattern.len() as f32;
    assert!(mean.abs() < 0.1, "mean was {mean}");
}
