//! Asking whether the mark is there, and saying how sure the answer is.

use crate::pattern::Pattern;
use crate::stft::{HOP, SAMPLE_RATE, Stft};
use crate::{MarkError, check};

/// A block is about a second: 94 hops of 512 samples at 48 kHz.
pub const HOPS_PER_BLOCK: usize = (SAMPLE_RATE as usize) / HOP;

/// A frame with fewer carrying bins above the gate than this has nothing to correlate, and
/// including it would only dilute the blocks that do.
const MIN_BINS: usize = 128;

/// The spectral envelope is what the signal was doing anyway; the mark is a fast ripple across
/// frequency. Subtracting a nine-bin moving average is what makes the two separable.
const SMOOTHING: usize = 9;

/// How many standard deviations above the decoys before the answer is yes.
///
/// The null is not a constant this file could carry: what an unmarked file scores depends
/// entirely on how much spectral structure it has. A harmonic stack correlates strongly with
/// *any* fixed pattern, and by the same amount every time, which measured against a constant
/// looks exactly like a mark — deterministic, repeatable, and wrong. That was found on
/// 2026-09-09, when unmarked audio reached a hundred standard errors under the previous design.
///
/// So the file is judged against itself, and this is the margin over its own decoys. Five, for
/// a one-sided tail of about 3 × 10⁻⁷ if the decoys were Gaussian. They are close enough that
/// the measured corpus never comes near it, which is the check that matters.
pub const Z_THRESHOLD: f32 = 5.0;

/// Fewer blocks than this and the spread of the blocks cannot be estimated at all.
const MIN_BLOCKS: usize = 3;

/// The answer, with the numbers behind it, because a bare yes is not evidence of anything.
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub marked: bool,
    /// Mean correlation over the blocks. Comparable across files only through [`Verdict::z`].
    pub statistic: f32,
    /// Standard errors above the null. This is the number that decides, and the one to quote.
    pub z: f32,
    /// Blocks that had enough signal to say anything. Below three the answer is always no.
    pub blocks: usize,
    pub threshold: f32,
}

/// How many decoy patterns the file is judged against. Enough that the spread of their
/// correlations is a usable estimate, few enough that the extra work is a rounding error next
/// to the transform, which is done once for all of them.
const DECOYS: usize = 31;

pub fn detect(samples: &[f32], sample_rate: u32, key: &[u8; 32]) -> Result<Verdict, MarkError> {
    check(samples, sample_rate)?;
    let ours = Pattern::for_key(key);
    let decoys = crate::pattern::decoys(DECOYS);
    let gate = 10f32.powf(crate::embed::GATE_DBFS / 20.0);

    // One row per pattern, ours first: the transform runs once and every pattern reads the
    // same ripple, which is what makes the comparison between them fair.
    let mut per_frame: Vec<Vec<f32>> = vec![Vec::new(); DECOYS + 1];
    let mut buffer = samples.to_vec();
    Stft::new().process(&mut buffer, |_, spectrum| {
        let mut logs = Vec::with_capacity(ours.len());
        let mut usable = 0usize;
        for index in 0..ours.len() {
            let magnitude = spectrum[ours.low_bin + index].norm();
            if magnitude > gate {
                usable += 1;
                logs.push(magnitude.ln());
            } else {
                logs.push(f32::NAN);
            }
        }
        if usable < MIN_BINS {
            return;
        }
        let ripple = ripple_of(&logs);
        if ripple.is_empty() {
            return;
        }
        if let Some(correlation) = correlate(&ripple, &logs, &ours.values) {
            per_frame[0].push(correlation);
        }
        for (index, decoy) in decoys.iter().enumerate() {
            if let Some(correlation) = correlate(&ripple, &logs, &decoy.values) {
                per_frame[index + 1].push(correlation);
            }
        }
    });

    // Frames are averaged into one-second blocks first, so a loud second cannot outvote a quiet
    // one: every block gets one say, which is what the tiling is for.
    let statistics: Vec<f32> = per_frame
        .iter()
        .map(|frames| {
            let blocks: Vec<f32> = frames
                .chunks(HOPS_PER_BLOCK)
                .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
                .collect();
            if blocks.is_empty() {
                0.0
            } else {
                blocks.iter().sum::<f32>() / blocks.len() as f32
            }
        })
        .collect();
    let blocks = per_frame[0].chunks(HOPS_PER_BLOCK).count();
    let statistic = statistics[0];

    // Against the decoys, not against a constant. They saw the same signal, so whatever
    // structure it has is in their numbers too, and what is left is the mark or nothing.
    let z = if blocks < MIN_BLOCKS {
        0.0
    } else {
        let decoy_values = &statistics[1..];
        let mean = decoy_values.iter().sum::<f32>() / decoy_values.len() as f32;
        let variance = decoy_values
            .iter()
            .map(|c| (c - mean) * (c - mean))
            .sum::<f32>()
            / (decoy_values.len() - 1) as f32;
        let spread = variance.sqrt();
        if spread <= f32::EPSILON {
            0.0
        } else {
            (statistic - mean) / spread
        }
    };
    Ok(Verdict {
        marked: blocks >= MIN_BLOCKS && z > Z_THRESHOLD,
        statistic,
        z,
        blocks,
        threshold: Z_THRESHOLD,
    })
}

/// The fast ripple across frequency, which is where the mark is; the envelope is what the
/// signal was doing anyway. Computed once per frame and shared by every pattern.
fn ripple_of(logs: &[f32]) -> Vec<f32> {
    let smooth = moving_average(logs, SMOOTHING);
    logs.iter()
        .zip(&smooth)
        .filter(|(value, _)| value.is_finite())
        .map(|(value, average)| value - average)
        .collect()
}

/// Normalised correlation between the ripple of the log-spectrum and the pattern.
///
/// Two things here are not obvious and both were measured before they were written.
///
/// **The pattern is filtered the same way the signal is.** Subtracting a moving average from
/// the log-spectrum also subtracts it from the mark inside it, so comparing the filtered signal
/// against an unfiltered template throws away the part the filter removed and normalises by an
/// energy the template no longer has. Filtering both is worth about a third of the statistic.
///
/// **Both are centred.** Neither the ripple nor the pattern has a mean of exactly zero over a
/// finite band, and the product of two small means is a bias that sits under every answer,
/// marked or not. Removing it is what lets the null be centred on nothing.
///
/// Normalising is what makes the result scale-free: a gain change multiplies every magnitude by
/// the same factor, which is a constant added to every log, which both the envelope subtraction
/// and the centring remove.
fn correlate(ripple: &[f32], logs: &[f32], pattern: &[f32]) -> Option<f32> {
    let mut template = Vec::with_capacity(ripple.len());
    for (index, value) in logs.iter().enumerate() {
        if value.is_finite() {
            template.push(pattern[index]);
        }
    }
    if ripple.len() < MIN_BINS || template.len() != ripple.len() {
        return None;
    }
    let mut ripple = ripple.to_vec();
    let mut filtered = high_pass(&template, SMOOTHING);
    centre(&mut ripple);
    centre(&mut filtered);

    let dot: f32 = ripple.iter().zip(&filtered).map(|(a, b)| a * b).sum();
    let energy: f32 = ripple.iter().map(|a| a * a).sum();
    let template_energy: f32 = filtered.iter().map(|b| b * b).sum();
    if energy <= f32::EPSILON || template_energy <= f32::EPSILON {
        return None;
    }
    Some(dot / (energy * template_energy).sqrt())
}

fn centre(values: &mut [f32]) {
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    for value in values.iter_mut() {
        *value -= mean;
    }
}

/// The same subtraction the log-spectrum gets, applied to the template.
fn high_pass(values: &[f32], span: usize) -> Vec<f32> {
    let smooth = moving_average(values, span);
    values.iter().zip(&smooth).map(|(v, s)| v - s).collect()
}

/// A moving average that skips the gated bins rather than treating them as zero, which would
/// drag the envelope down wherever the signal happened to be quiet.
fn moving_average(values: &[f32], span: usize) -> Vec<f32> {
    let half = span / 2;
    (0..values.len())
        .map(|index| {
            let start = index.saturating_sub(half);
            let end = (index + half + 1).min(values.len());
            let mut sum = 0.0;
            let mut count = 0usize;
            for value in &values[start..end] {
                if value.is_finite() {
                    sum += value;
                    count += 1;
                }
            }
            if count == 0 { 0.0 } else { sum / count as f32 }
        })
        .collect()
}
