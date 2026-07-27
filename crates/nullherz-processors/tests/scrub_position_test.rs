//! Scrubbing must move the playhead on any loaded audio.
//!
//! Scroll-to-scrub on a deck lane used to send `JumpByBeats`, which the sampler
//! acts on only when ALL of these hold:
//!
//!   * a `ProcessContext` is present,
//!   * the sampler has metadata,
//!   * the context carries a transport,
//!   * and `metadata.bpm > 0.0`.
//!
//! Those are the right guards for a musical jump and the wrong ones for
//! dragging a needle. A track whose tempo had not been detected — anything
//! freshly imported, anything ambient, anything without a beat — scrubbed
//! nowhere, and did so silently. `NudgePosition` is the time-domain path and
//! carries none of those preconditions.

use nullherz_processors::sampler::SamplerProcessor;
use nullherz_traits::{AudioProcessor, Command, PerformanceCommand, SampleMetadata};
use std::sync::Arc;

const FRAMES: usize = 48_000;

fn loaded_sampler(bpm: f32) -> SamplerProcessor {
    let mut s = SamplerProcessor::new(1);
    let buf: Arc<Vec<f32>> = Arc::new((0..FRAMES).map(|i| (i as f32 * 0.001).sin()).collect());
    let mut meta = SampleMetadata::new_empty();
    meta.bpm = bpm;
    meta.sample_rate = 48_000;
    meta.channels = 1;
    meta.total_samples = FRAMES as u64;
    s.apply_topology_mutation(nullherz_traits::TopologyMutation::AddSource {
        node_idx: 1,
        buffer: buf,
        sample_id: 7,
        metadata: Some(Arc::new(meta)),
    });
    s
}

/// The position the UI sees. Deliberately the public reporting path rather
/// than poking at voices: a deck that has never been played has no voice, so
/// reading voices directly would test an implementation detail and miss the
/// case that was actually broken.
fn playhead(s: &SamplerProcessor) -> f64 {
    s.get_playback_position() as f64
}

fn nudge(s: &mut SamplerProcessor, frames: i64) {
    s.apply_command(&Command::Performance(PerformanceCommand::NudgePosition {
        node_idx: 1,
        frames,
    }));
}

#[test]
fn test_scrub_moves_the_playhead_without_a_tempo() {
    // The exact case the old path dropped on the floor.
    let mut s = loaded_sampler(0.0);
    nudge(&mut s, 4_800);
    assert!(
        playhead(&s) >= 4_799.0,
        "playhead is {} after nudging 4800 frames on a track with bpm=0 — scrubbing \
         still depends on tempo detection",
        playhead(&s)
    );
}

#[test]
fn test_scrub_works_with_a_tempo_too() {
    // The fix must not trade one gated path for another.
    let mut s = loaded_sampler(128.0);
    nudge(&mut s, 2_400);
    assert!(playhead(&s) >= 2_399.0, "playhead is {}", playhead(&s));
}

#[test]
fn test_scrub_moves_backwards() {
    let mut s = loaded_sampler(0.0);
    nudge(&mut s, 10_000);
    let fwd = playhead(&s);
    nudge(&mut s, -6_000);
    let back = playhead(&s);
    assert!(
        back < fwd && back >= 3_999.0,
        "backwards scrub went from {fwd} to {back}, expected about 4000"
    );
}

#[test]
fn test_scrub_clamps_at_the_start() {
    // Rolling the wheel back past the beginning must settle at 0, not go
    // negative — a negative play_head makes the interpolator extrapolate.
    let mut s = loaded_sampler(0.0);
    nudge(&mut s, 500);
    nudge(&mut s, -100_000);
    let p = playhead(&s);
    assert!((0.0..1.0).contains(&p), "playhead is {p}, expected to clamp to the start");
}

#[test]
fn test_scrub_clamps_at_the_end() {
    let mut s = loaded_sampler(0.0);
    nudge(&mut s, 10_000_000);
    let p = playhead(&s);
    assert!(
        p < FRAMES as f64 && p > FRAMES as f64 - 2.0,
        "playhead is {p}, expected to clamp just inside {FRAMES}"
    );
}

#[test]
fn test_scrub_ignores_other_nodes() {
    // Every sampler sees every command; only the addressed one may move.
    let mut s = loaded_sampler(0.0);
    nudge(&mut s, 5_000);
    let before = playhead(&s);
    s.apply_command(&Command::Performance(PerformanceCommand::NudgePosition {
        node_idx: 99,
        frames: 20_000,
    }));
    assert_eq!(
        playhead(&s),
        before,
        "a nudge addressed to node 99 moved node 1 — scrubbing one deck would drag them all"
    );
}

#[test]
fn test_scrub_works_on_a_stopped_deck() {
    // "Set position" is the stopped case: park the needle, then hit play.
    let mut s = loaded_sampler(0.0);
    s.apply_command(&Command::Performance(PerformanceCommand::StopNode { node_idx: 1 }));
    nudge(&mut s, 12_000);
    assert!(
        playhead(&s) >= 11_999.0,
        "playhead is {} — a stopped deck cannot be repositioned",
        playhead(&s)
    );
}
