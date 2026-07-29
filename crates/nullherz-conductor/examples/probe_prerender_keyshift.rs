//! **Is pre-rendering a key shift at load time practical?**
//!
//! KeySync costs 21.3 ms of latency in every deck because a phase vocoder needs
//! an analysis window. But the job it does — transpose by a fixed number of
//! semitones — does not have to happen in real time. Nothing couples it to
//! tempo (it is written only from the KEY latch), so the whole shifted buffer
//! can be produced once, on the background thread that already decodes the
//! track, and playback becomes plain sample playback at zero added latency.
//!
//! That trade is only worth making if the render is fast enough to sit inside a
//! deck load. This measures it against real track lengths.
//!
//!   cargo run --release -p nullherz-conductor --example probe_prerender_keyshift

use std::time::Instant;
use nullherz_traits::{AudioProcessor, ProcessContext, SignalProcessor, Transport};

const SR: f32 = 48_000.0;
const BLOCK: usize = 256;

fn transport() -> Transport {
    Transport {
        bpm: 120.0,
        beat_position: 0.0,
        is_playing: true,
        sample_rate: SR,
        absolute_samples: 0,
        system_time_ns: 0,
        device_time_ns: 0,
    }
}

/// Render an entire buffer through KeySync, the way an offline pre-render would.
fn prerender(input: &[f32], semitones: f32) -> (Vec<f32>, std::time::Duration) {
    let mut proc = nullherz_processors::keysync::KeySyncProcessor::new(0, 1024);
    proc.set_parameter(0, semitones, 0);
    let t = transport();
    let mut out = vec![0.0f32; input.len()];

    let start = Instant::now();
    for s in (0..input.len()).step_by(BLOCK) {
        let e = (s + BLOCK).min(input.len());
        let inp: [&[f32]; 1] = [&input[s..e]];
        let mut chunk = vec![0.0f32; e - s];
        {
            let mut slice: [&mut [f32]; 1] = [&mut chunk];
            let mut ctx = ProcessContext {
                transport: Some(&t),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: e == input.len(),
            };
            proc.process(&inp, &mut slice, &mut ctx);
        }
        out[s..e].copy_from_slice(&chunk);
    }
    let elapsed = start.elapsed();
    (out, elapsed)
}

fn main() {
    println!("Pre-rendering a key shift on the hydration thread — is it fast enough?\n");
    println!("{:>10}  {:>12}  {:>12}  {:>10}  {:>14}", "track", "mono frames", "render", "x realtime", "extra RAM/deck");

    for minutes in [0.25f32, 1.0, 3.0, 6.0, 10.0] {
        let frames = (SR * 60.0 * minutes) as usize;
        let input: Vec<f32> = (0..frames)
            .map(|i| (i as f32 * 220.0 * std::f32::consts::TAU / SR).sin() * 0.5)
            .collect();

        let (out, dt) = prerender(&input, 3.0);
        std::hint::black_box(&out);

        let audio_secs = frames as f32 / SR;
        let ratio = audio_secs / dt.as_secs_f32();
        // Stereo f32 copy held alongside the source.
        let ram_mb = (frames * 2 * 4) as f32 / 1_048_576.0;
        println!(
            "{:>8.1}m  {:>12}  {:>10.0} ms  {:>9.0}x  {:>11.1} MB",
            minutes, frames, dt.as_secs_f32() * 1000.0, ratio, ram_mb
        );
    }

    println!("\nOne mono channel; a stereo deck is ~2x the render time and the RAM shown.");
    println!("Compare against: async hydration already decodes on a background thread");
    println!("(`hydrate-<id>`), so this rides an existing seam rather than adding one.");
}
