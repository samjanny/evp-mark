//! Say whether a WAV carries Aftervoice's mark, and with what confidence.
//!
//! This is the public detector the AI Act compliance rests on, and it is the same code the
//! phone runs when it marks: a detector that is not the embedder's twin proves nothing about
//! it. Nothing here talks to a network, and the pattern it compares against comes from a key
//! printed in the source, so two people running this on the same file get the same answer
//! without either of them asking us anything.
//!
//! ```text
//! detect-mark recording.wav
//! detect-mark --json recording.wav
//! ```
//!
//! Exit status is the answer, for a script that has to act on it: 0 marked, 1 not marked or
//! not enough audio to say, 2 the file could not be read or judged.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut json = false;
    let mut input: Option<PathBuf> = None;

    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                print_usage();
                return ExitCode::from(0);
            }
            other if other.starts_with('-') => {
                eprintln!("unknown option {other}");
                print_usage();
                return ExitCode::from(2);
            }
            other => {
                if input.is_some() {
                    eprintln!("one file at a time");
                    return ExitCode::from(2);
                }
                input = Some(PathBuf::from(other));
            }
        }
    }

    let Some(input) = input else {
        print_usage();
        return ExitCode::from(2);
    };
    detect(&input, json)
}

fn print_usage() {
    eprintln!("usage: detect-mark [--json] <file.wav>");
    eprintln!();
    eprintln!("Says whether the audio carries Aftervoice's mark. Absence proves nothing;");
    eprintln!("only presence is evidence, and only that a machine generated the sound.");
}

fn detect(input: &Path, json: bool) -> ExitCode {
    let mut reader = match hound::WavReader::open(input) {
        Ok(reader) => reader,
        Err(error) => {
            eprintln!("cannot read {}: {error}", input.display());
            return ExitCode::from(2);
        }
    };
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(Result::ok)
                .map(|sample| sample as f32 * scale)
                .collect()
        }
    };

    // Mono is what the mark is defined on. A stereo file is mixed down rather than refused,
    // because a file that has been through an editor is exactly the case this has to answer.
    let mono: Vec<f32> = if spec.channels <= 1 {
        samples
    } else {
        let channels = spec.channels as usize;
        samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    let verdict = match evp_mark::detect(&mono, spec.sample_rate, &evp_mark::MARK_KEY) {
        Ok(verdict) => verdict,
        Err(error) => {
            eprintln!("cannot judge {}: {error}", input.display());
            return ExitCode::from(2);
        }
    };

    if json {
        // Hand-written rather than pulling in a serialiser: six scalars do not justify a
        // dependency in a crate whose whole argument is that you can read all of it.
        println!(
            "{{\"file\":{:?},\"marked\":{},\"z\":{:.4},\"statistic\":{:.6},\"blocks\":{},\"threshold\":{:.4}}}",
            input.display().to_string(),
            verdict.marked,
            verdict.z,
            verdict.statistic,
            verdict.blocks,
            verdict.threshold,
        );
    } else if verdict.marked {
        println!(
            "marked  z={:.1} over {} blocks, threshold {:.1}",
            verdict.z, verdict.blocks, verdict.threshold,
        );
    } else if verdict.blocks < 3 {
        println!(
            "too little audio to say  {} usable blocks, three needed",
            verdict.blocks
        );
    } else {
        println!(
            "not marked  z={:.1} over {} blocks, threshold {:.1}",
            verdict.z, verdict.blocks, verdict.threshold,
        );
    }

    ExitCode::from(if verdict.marked { 0 } else { 1 })
}
