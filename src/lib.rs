//! A machine-readable mark on synthetic audio, and the detector for it.
//!
//! Article 50 of Regulation (EU) 2024/1689 asks a provider of a system that generates synthetic
//! audio to mark its output in a machine-readable form, detectable as artificially generated,
//! as far as is technically feasible. This crate is that mark.
//!
//! It is a spread-spectrum signature, not a hidden tone: minute coordinated perturbations of
//! the short-time spectrum, and a detector that looks for a correlation rather than for a
//! whistle. The shape is forced by the product's own signal chain, which low-passes at 1.3 kHz
//! and would erase anything above it without an attacker doing anything at all.
//!
//! The key is inside the app that embeds. Anyone who extracts it can also remove the mark, and
//! this crate says so rather than implying otherwise: the mark exists for compliance and
//! provenance, not for tamper-proofing.
//!
//! Design: `docs/superpowers/specs/2026-09-09-evp-mark-design.md`.

#![forbid(unsafe_code)]

/// The project key.
///
/// It is in the open on purpose, and this is the decision of 2026-09-08 rather than an
/// oversight. The embedding happens on the phone, so the key ships inside the APK whatever is
/// done here; pretending otherwise would only mean the detector could not be published, and a
/// mark nobody can check is not a machine-readable mark.
///
/// What follows from that is stated rather than hidden: anyone who wants to can remove the
/// mark. It exists for compliance and provenance, not to resist someone who has decided to
/// defeat it.
pub const MARK_KEY: [u8; 32] = *b"aftervoice/mark/v1/project-key!!";

pub mod detect;
pub mod embed;
pub mod pattern;
pub mod stft;
pub mod stream;

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MarkError {
    #[error("the mark is defined at {expected} Hz, not {actual} Hz")]
    SampleRate { expected: u32, actual: u32 },
    #[error("a buffer shorter than one window ({0} < {1}) has nowhere to carry a mark")]
    TooShort(usize, usize),
    #[error("the buffer contains a sample that is not a finite number")]
    NotFinite,
}

/// The checks both the embedder and the detector make before doing anything.
pub(crate) fn check(samples: &[f32], sample_rate: u32) -> Result<(), MarkError> {
    if sample_rate != stft::SAMPLE_RATE {
        return Err(MarkError::SampleRate {
            expected: stft::SAMPLE_RATE,
            actual: sample_rate,
        });
    }
    if samples.len() < stft::WINDOW {
        return Err(MarkError::TooShort(samples.len(), stft::WINDOW));
    }
    if samples.iter().any(|s| !s.is_finite()) {
        return Err(MarkError::NotFinite);
    }
    Ok(())
}

pub use detect::{Verdict, detect};
pub use embed::mark;
pub use stream::Marker;
