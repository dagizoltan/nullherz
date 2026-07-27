//! Tempo sync must lock BEAT PHASE, never absolute position.
//!
//! The old target was `transport.beat_position * samples_per_beat`, an absolute
//! frame position. That silently assumed the deck had been playing since
//! transport beat 0: load a track ten minutes into a set and the "expected"
//! position was ten minutes deep, so the deck was slammed there on its first
//! block. `% source_frames` was a patch — it kept the target inside the buffer,
//! but wrapped the TARGET while leaving the PLAYHEAD unwrapped, so near the end
//! of a track the two diverged by nearly a whole buffer and the deck jumped back
//! to the start instead of ending.
//!
//! Locking phase within the beat makes no assumption about how long the deck has
//! been playing, is bounded to half a beat by construction, and needs neither a
//! modulo nor a positional jump.

use nullherz_processors::sampler::SamplerProcessor;
use nullherz_traits::{
    AudioProcessor, Command, PerformanceCommand, ProcessContext, SampleMetadata, SignalProcessor,
    TopologyMutation, Transport,
};
use std::sync::Arc;

const RATE: f32 = 44_100.0;
const BLOCK: usize = 256;

struct Rig {
    sampler: SamplerProcessor,
    transport: Transport,
    id: u64,
}

impl Rig {
    fn new(track_bpm: f32, master_bpm: f32, frames: usize) -> Self {
        let id = 1u64;
        let mut sampler = SamplerProcessor::new(id);
        let mut metadata = SampleMetadata::new_empty();
        metadata.total_samples = frames as u64;
        metadata.channels = 1;
        metadata.sample_rate = RATE as u32;
        metadata.bpm = track_bpm;
        sampler.apply_topology_mutation(TopologyMutation::AddSource {
            node_idx: id as u32,
            buffer: Arc::new(vec![0.25f32; frames]),
            sample_id: id,
            metadata: Some(Arc::new(metadata)),
        });
        let transport = Transport {
            bpm: master_bpm,
            beat_position: 0.0,
            is_playing: true,
            sample_rate: RATE,
            absolute_samples: 0,
            system_time_ns: 0,
            device_time_ns: 0,
        };
        Self { sampler, transport, id }
    }

    fn send(&mut self, cmd: PerformanceCommand) {
        let ctx = ProcessContext { transport: Some(&self.transport), host: None, sub_block_offset: 0, is_last_sub_block: true };
        self.sampler.apply_command_with_context(&Command::Performance(cmd), Some(&ctx));
    }

    /// Render one block and advance the transport by it.
    fn tick(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        {
            let mut outs: Vec<&mut [f32]> = vec![out];
            let mut ctx = ProcessContext { transport: Some(&self.transport), host: None, sub_block_offset: 0, is_last_sub_block: true };
            self.sampler.process(&[], &mut outs, &mut ctx);
        }
        self.transport.absolute_samples += BLOCK as u64;
        self.transport.beat_position += (BLOCK as f64) * (self.transport.bpm as f64 / 60.0) / RATE as f64;
    }

    fn playhead(&self) -> Option<f64> {
        self.sampler.voices.iter().find(|v| v.is_active).map(|v| v.play_head)
    }

    fn any_playhead(&self) -> Option<f64> {
        self.sampler
            .voices
            .iter()
            .find(|v| v.is_active || v.buffer.is_some())
            .map(|v| v.play_head)
    }
}

/// The headline regression: a deck loaded and played mid-session must start from
/// where it was triggered, not be teleported to wherever the transport's beat
/// count happens to point.
#[test]
fn test_deck_started_mid_session_is_not_teleported() {
    // Deliberately awkward numbers. With a round beat count and a round buffer
    // length the old `% source_frames` could land back on ~0 by coincidence and
    // the bug would hide: 1280 beats at 128 BPM is exactly ten minutes, which is
    // an exact multiple of a 60-second buffer. Pick a beat position that is not a
    // whole number of buffer lengths so the wrapped target is genuinely deep.
    let mut rig = Rig::new(127.0, 127.0, 44_100 * 37);
    rig.transport.beat_position = 1_013.37;

    rig.send(PerformanceCommand::PlayNode { node_idx: rig.id as u32 });
    let mut out = vec![0.0f32; BLOCK];
    for _ in 0..8 {
        rig.tick(&mut out);
    }

    let head = rig.playhead().expect("voice should still be active");
    // Eight blocks in, the playhead should be near 8 * 256 frames — not
    // 1280 beats * ~20672 frames deep (which is past the buffer entirely).
    assert!(
        head < (BLOCK * 16) as f64,
        "playhead jumped to {head} after 8 blocks; the deck was slammed to the \
         transport's absolute beat position instead of playing from its trigger"
    );
}

/// Resume after pause: `StopNode` holds the playhead, the transport keeps
/// running, and `PlayNode` resumes. The deck must pick up where it left off and
/// re-lock by RATE, not jump.
#[test]
fn test_resume_after_pause_does_not_jump() {
    let mut rig = Rig::new(128.0, 128.0, 44_100 * 60);
    rig.send(PerformanceCommand::PlayNode { node_idx: rig.id as u32 });

    let mut out = vec![0.0f32; BLOCK];
    for _ in 0..64 {
        rig.tick(&mut out);
    }
    let before_pause = rig.playhead().expect("active before pause");

    // Pause, let the transport run on for two seconds, then resume.
    rig.send(PerformanceCommand::StopNode { node_idx: rig.id as u32 });
    for _ in 0..(2 * RATE as usize / BLOCK) {
        rig.transport.absolute_samples += BLOCK as u64;
        rig.transport.beat_position += (BLOCK as f64) * (rig.transport.bpm as f64 / 60.0) / RATE as f64;
    }
    let held = rig.any_playhead().expect("paused voice keeps its position");
    assert!(
        (held - before_pause).abs() < 1.0,
        "pause moved the playhead: {before_pause} -> {held}"
    );

    rig.send(PerformanceCommand::PlayNode { node_idx: rig.id as u32 });
    rig.tick(&mut out);
    let after_resume = rig.playhead().expect("active after resume");

    // One block of playback past the held position — plus at most the ±2% rate
    // correction. Certainly not a jump of the two seconds the transport advanced.
    let advanced = after_resume - held;
    assert!(
        advanced > 0.0 && advanced < BLOCK as f64 * 1.5,
        "resume advanced the playhead by {advanced} frames in one block \
         (held {held}, now {after_resume}) — the deck jumped to the transport \
         position instead of resuming"
    );
}

/// A manual seek must STICK. Absolute-position sync treated any offset between
/// the playhead and its notion of "where the transport says we should be" as an
/// error to correct — so a beat jump was silently reverted on the next block.
/// Phase locking only cares where you are *within* the beat, so a jump of whole
/// beats is preserved.
#[test]
fn test_manual_beat_jump_is_not_reverted_by_sync() {
    let mut rig = Rig::new(128.0, 128.0, 44_100 * 60);
    rig.send(PerformanceCommand::PlayNode { node_idx: rig.id as u32 });

    let mut out = vec![0.0f32; BLOCK];
    for _ in 0..256 {
        rig.tick(&mut out);
    }
    let before = rig.playhead().expect("active");

    // Jump forward a whole number of beats — phase is unchanged, position is not.
    rig.send(PerformanceCommand::JumpByBeats { node_idx: rig.id as u32, beats: 16.0 });
    let after_jump = rig.playhead().expect("active");
    let jumped = after_jump - before;
    assert!(jumped > 0.0, "the jump itself did not move the playhead");

    // A few blocks later the jump must still be there.
    for _ in 0..8 {
        rig.tick(&mut out);
    }
    let settled = rig.playhead().expect("active");
    let drift = (settled - after_jump) - (8 * BLOCK) as f64;

    assert!(
        drift.abs() < BLOCK as f64,
        "8 blocks after a 16-beat jump the playhead drifted {drift} frames beyond \
         normal playback (jumped to {after_jump}, now {settled}) — sync pulled it \
         back toward an absolute position"
    );
}

/// The end-of-track outcome guard: a deck reaching the end of its buffer must
/// stop, not wrap.
///
/// Note this one does NOT fail against the old absolute-position sync, and that
/// is expected rather than a weak assertion. With the target advancing at
/// `master_bpm * samples_per_beat(track)` — which is identically the playhead's
/// rate — the two only diverge at the modulo wrap itself, and at that instant the
/// voice's own end-of-buffer check deactivates it first. The restart was only
/// reachable once the playhead was OFFSET from the target, which is what the
/// mid-session and manual-jump tests above cover. Kept as an outcome regression
/// guard: whatever the sync scheme, a track must end.
#[test]
fn test_track_ends_instead_of_restarting() {
    let frames = 44_100 * 4; // 4 seconds
    // Master SLOWER than the track, so the deck plays below 1.0x and the playhead
    // LAGS the absolute target. That is the condition that exposes the wrap: the
    // old target reached the buffer end and wrapped to ~0 while the playhead was
    // still mid-buffer, producing a near-buffer-sized negative error and a jump
    // back to the start. At matched tempo the two advance together and the
    // end-of-buffer check wins the race, hiding it.
    let mut rig = Rig::new(128.0, 96.0, frames);
    rig.send(PerformanceCommand::PlayNode { node_idx: rig.id as u32 });

    let mut out = vec![0.0f32; BLOCK];
    let mut max_head = 0.0f64;
    let mut wrapped = false;
    // Run well past the end of the buffer.
    for _ in 0..(frames / BLOCK * 2) {
        rig.tick(&mut out);
        match rig.playhead() {
            Some(h) => {
                if h + 1.0 < max_head {
                    wrapped = true;
                }
                max_head = max_head.max(h);
            }
            None => break, // voice ended, which is the correct outcome
        }
    }

    assert!(!wrapped, "the playhead moved backwards — the track restarted");
    assert!(
        rig.playhead().is_none(),
        "voice still active after running twice the buffer length; it reached \
         {max_head} of {frames} frames"
    );
}

/// A large tempo ratio must stay stable: the rate settles near `master/track`
/// and inside the ±2% correction band, with no positional jumps.
#[test]
fn test_high_sync_ratio_stays_stable() {
    let (track_bpm, master_bpm) = (128.0f32, 174.0f32);
    let mut rig = Rig::new(track_bpm, master_bpm, 44_100 * 120);
    rig.send(PerformanceCommand::PlayNode { node_idx: rig.id as u32 });

    let mut out = vec![0.0f32; BLOCK];
    let expected_ratio = master_bpm / track_bpm; // 1.359

    // Let the rate EMA settle before measuring.
    for _ in 0..256 {
        rig.tick(&mut out);
    }

    let mut prev = rig.playhead().expect("active");
    let mut jumps = 0;
    let mut backwards = 0;
    for _ in 0..2000 {
        rig.tick(&mut out);
        let Some(h) = rig.playhead() else { break };
        let step = h - prev;
        if step < 0.0 {
            backwards += 1;
        }
        // A block advances BLOCK * rate frames; anything beyond 1.5x that is a jump.
        if step > BLOCK as f64 * expected_ratio as f64 * 1.5 {
            jumps += 1;
        }
        prev = h;
    }

    let rate = rig
        .sampler
        .voices
        .iter()
        .find(|v| v.is_active)
        .map(|v| v.playback_rate)
        .expect("active");

    assert_eq!(backwards, 0, "playhead moved backwards {backwards} times");
    assert_eq!(jumps, 0, "playhead jumped {jumps} times");
    assert!(
        (rate - expected_ratio).abs() <= expected_ratio * 0.021,
        "rate settled at {rate}, outside the ±2% band around {expected_ratio}"
    );
}
