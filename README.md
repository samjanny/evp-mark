# evp-mark

A machine-readable mark on synthetic audio, and the detector for it.

This is the watermark [Aftervoice](https://aftervoice.altrovelabs.net/) puts on every sound it plays,
published so that anyone can check our audio without asking us for anything. A detector whose
answer we control proves nothing about us.

## Check a file yourself

```sh
git clone https://github.com/samjanny/evp-mark
cd evp-mark
cargo run --release --bin detect-mark -- path/to/file.wav
```

It prints the score and a verdict, and sets its exit status to match: 0 marked, 1 not marked or
too little audio to say, 2 unreadable. `--json` gives the same answer for something that has to
act on it. Any WAV will do, and the file does not have to come from us — running it on audio
that is nobody's watermark is the best way to see what a negative answer looks like.

Nothing here reaches the network, and the pattern it compares against is derived from a key
printed in `src/lib.rs`. Two people running this on the same file get the same answer without
either of them asking us anything.

## What it does

A spread-spectrum watermark in the sound itself, not a tag on the file. The audio is cut into
overlapping 2048-sample windows at 48 kHz; in each one a pseudo-random pattern nudges some
frequency bins slightly louder and others slightly quieter, about 1.6 dB, in a band where a
small change is least likely to be heard and least likely to be destroyed. This strength
(`alpha = 0.20`) was selected by a blind listening gate on 2026-09-12.

Because it lives in the audio it survives sharing, re-encoding to a lossy format, a change of
container, renaming, and a trim down to a few seconds.

```rust
use evp_mark::{MARK_KEY, detect, mark};

// Broadband 48 kHz mono, and enough of it: the mark rides on what the signal has in the
// carrying band, so a few seconds of quiet noise sits near the threshold and silence
// carries nothing at all. Twelve seconds of a noise bed clears it comfortably.
let mut samples: Vec<f32> = load_audio();
mark(&mut samples, 48_000, &MARK_KEY)?;

let verdict = detect(&samples, 48_000, &MARK_KEY)?;
assert!(verdict.marked);
```

`Marker` is the streaming form, for a render loop: it costs 2048 samples of latency, 43 ms at
48 kHz, and the first window comes out as silence rather than as a quiet wrong version of what
was played.

## How a file is judged

Not against a constant. Against itself.

The statistic for our pattern is compared with the statistic for thirty-one decoy patterns on
the same audio. Whatever the signal has of its own — a chord, a loop, a tone — is in the decoys
too, and what is left is the mark or nothing. The result is a z-score; the threshold is 5.

That design replaced one that compared against a fixed number, and it had to: measured across
six kinds of signal, unmarked audio was reaching a hundred standard deviations against a
constant, because a harmonic chord correlates strongly with *any* fixed pattern and by the same
amount every time.

## What has been measured

| | worst score | read as marked |
|---|---|---|
| 960 judgements of synthetic signals | 3.07 | none |
| 65 produced music tracks, whole | 2.97 | none |
| the same, as ten-second clips | 3.18 | none |
| 72 utterances of recorded speech | 0.76 | none |
| **threshold** | **5.0** | |

## What it cannot do

It is **not a signature and not an identifier**: no device, session or installation id, no
timestamp. It says one thing — a machine made this audio.

It **does not survive everything**: heavy pitch shifting, time stretching or a band-stop filter
across the carrying band will remove it. It marks origin; it does not resist tampering.

It **cannot ride a signal with nothing to modulate**: a nearly pure tone will not carry it,
because there is nothing in the carrying band to nudge.

**Absence proves nothing.** Only presence is evidence, and only of one thing: this was generated.

## Why

Article 50 of the European AI Act asks that synthetic audio be marked in a machine-readable way,
and detectable as artificially generated, as far as is technically feasible. The numbers above
are our answer to "as far as is technically feasible", and the limits above are the ones we
found.

## Licence

MIT OR Apache-2.0, at your option.
