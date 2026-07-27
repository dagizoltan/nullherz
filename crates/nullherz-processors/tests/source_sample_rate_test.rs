//! A source recorded at a rate other than the device's must still play at its
//! recorded pitch and duration.
//!
//! The sampler advances one frame of source per frame of output, so a 48 kHz
//! file on a 44.1 kHz device used to play 8.8% slow — about 1.5 semitones flat —
//! and a 96 kHz file an octave down. The decoder discarded
//! `codec_params.sample_rate` entirely, so this was EVERY non-44.1 kHz file,
//! which is most modern production material.
//!
//! `SampleMetadata.sample_rate` records what the frame counts are measured in,
//! and `SamplerVoice::source_rate_ratio` converts on playback.

use nullherz_processors::sampler::SamplerProcessor;
use nullherz_traits::{
    AudioProcessor, Command, PerformanceCommand, ProcessContext, SampleMetadata, SignalProcessor,
    TopologyMutation, Transport,
};
use std::sync::Arc;

const DEVICE_RATE: f32 = 44_100.0;
const BLOCK: usize = 256;

/// Render a mono source of `frames` frames recorded at `source_rate`, and return
/// how many frames of source the voice consumed per block on average.
fn frames_consumed_per_output_block(source_rate: u32, frames: usize) -> f64 {
    let id = 1u64;
    let mut sampler = SamplerProcessor::new(id);

    let mut metadata = SampleMetadata::new_empty();
    metadata.total_samples = frames as u64;
    metadata.channels = 1;
    metadata.sample_rate = source_rate;
    // bpm 0 keeps the tempo-sync path out of this: we are measuring rate
    // conversion alone, not the PLL.
    metadata.bpm = 0.0;

    sampler.apply_topology_mutation(TopologyMutation::AddSource {
        node_idx: id as u32,
        buffer: Arc::new(vec![0.25f32; frames]),
        sample_id: id,
        metadata: Some(Arc::new(metadata)),
    });

    let transport = Transport {
        bpm: 120.0,
        beat_position: 0.0,
        is_playing: true,
        sample_rate: DEVICE_RATE,
        absolute_samples: 0,
        system_time_ns: 0,
        device_time_ns: 0,
    };
    {
        let ctx = ProcessContext { transport: Some(&transport), host: None, sub_block_offset: 0, is_last_sub_block: true };
        sampler.apply_command_with_context(
            &Command::Performance(PerformanceCommand::PlayNode { node_idx: id as u32 }),
            Some(&ctx),
        );
    }

    let mut out = vec![0.0f32; BLOCK];
    let blocks = 64;
    let start;
    {
        let mut outs: Vec<&mut [f32]> = vec![&mut out];
        let mut ctx = ProcessContext { transport: Some(&transport), host: None, sub_block_offset: 0, is_last_sub_block: true };
        sampler.process(&[], &mut outs, &mut ctx);
        start = sampler.voices.iter().find(|v| v.is_active).expect("voice active").play_head;
    }
    for _ in 1..blocks {
        out.fill(0.0);
        let mut outs: Vec<&mut [f32]> = vec![&mut out];
        let mut ctx = ProcessContext { transport: Some(&transport), host: None, sub_block_offset: 0, is_last_sub_block: true };
        sampler.process(&[], &mut outs, &mut ctx);
    }
    let end = sampler.voices.iter().find(|v| v.is_active).expect("voice still active").play_head;
    (end - start) / (blocks - 1) as f64
}

#[test]
fn test_source_rate_is_converted_to_device_rate() {
    // Frames consumed per output block must equal BLOCK * (source_rate / device_rate),
    // which is exactly what keeps pitch and duration correct.
    for (source_rate, label) in [(44_100u32, "44.1k"), (48_000, "48k"), (96_000, "96k")] {
        let frames = (source_rate as usize) * 4; // 4 seconds of source
        let consumed = frames_consumed_per_output_block(source_rate, frames);
        let expected = BLOCK as f64 * (source_rate as f64 / DEVICE_RATE as f64);
        assert!(
            (consumed - expected).abs() < 0.5,
            "{label} source: consumed {consumed:.2} frames per {BLOCK}-frame output block, \
             expected {expected:.2} — playback would be transposed"
        );
    }
}

#[test]
fn test_a_48k_source_plays_for_its_true_duration() {
    // The regression in plain terms: two seconds of 48 kHz audio must take two
    // seconds to play on a 44.1 kHz device, not 2.177 s.
    let source_rate = 48_000u32;
    let seconds = 2.0f64;
    let frames = (source_rate as f64 * seconds) as usize;

    let consumed_per_block = frames_consumed_per_output_block(source_rate, frames);
    let output_blocks_to_finish = frames as f64 / consumed_per_block;
    let played_seconds = output_blocks_to_finish * BLOCK as f64 / DEVICE_RATE as f64;

    assert!(
        (played_seconds - seconds).abs() < 0.01,
        "2.000 s of 48 kHz audio plays for {played_seconds:.3} s on a 44.1 kHz \
         device (uncompensated would be ~2.177 s)"
    );
}

/// Beat-derived positions are written to `play_head`, which counts SOURCE
/// frames — so a beat jump on a 48 kHz track must move by 48000*60/bpm frames,
/// not 44100*60/bpm. Computed from the device rate it lands 8.8% short.
#[test]
fn test_beat_jump_moves_by_source_frames_not_device_frames() {
    let source_rate = 48_000u32;
    let bpm = 120.0f32;
    let frames = source_rate as usize * 10;

    let id = 3u64;
    let mut sampler = SamplerProcessor::new(id);
    let mut metadata = SampleMetadata::new_empty();
    metadata.total_samples = frames as u64;
    metadata.channels = 1;
    metadata.sample_rate = source_rate;
    metadata.bpm = bpm;
    sampler.apply_topology_mutation(TopologyMutation::AddSource {
        node_idx: id as u32,
        buffer: Arc::new(vec![0.25f32; frames]),
        sample_id: id,
        metadata: Some(Arc::new(metadata)),
    });

    let transport = Transport {
        bpm, beat_position: 0.0, is_playing: true, sample_rate: DEVICE_RATE,
        absolute_samples: 0, system_time_ns: 0, device_time_ns: 0,
    };
    let ctx = ProcessContext { transport: Some(&transport), host: None, sub_block_offset: 0, is_last_sub_block: true };
    sampler.apply_command_with_context(
        &Command::Performance(PerformanceCommand::PlayNode { node_idx: id as u32 }),
        Some(&ctx),
    );

    let before = sampler.voices.iter().find(|v| v.is_active).expect("voice active").play_head;
    sampler.apply_command_with_context(
        &Command::Performance(PerformanceCommand::JumpByBeats { node_idx: id as u32, beats: 4.0 }),
        Some(&ctx),
    );
    let after = sampler.voices.iter().find(|v| v.is_active || v.buffer.is_some()).expect("voice present").play_head;

    let moved = after - before;
    let expected = 4.0 * (source_rate as f64 * 60.0 / bpm as f64); // 96,000 frames
    let device_based = 4.0 * (DEVICE_RATE as f64 * 60.0 / bpm as f64); // 88,200 — the bug
    assert!(
        (moved - expected).abs() < 1.0,
        "4-beat jump moved {moved} frames; expected {expected} (source-rate). \
         A device-rate computation would give {device_based}"
    );
}

#[test]
fn test_unknown_source_rate_does_not_transpose() {
    // A row with sample_rate 0 (or a source registered without metadata) must
    // pass through at 1:1 rather than being scaled by a garbage ratio.
    let id = 2u64;
    let mut sampler = SamplerProcessor::new(id);
    let mut metadata = SampleMetadata::new_empty();
    metadata.total_samples = 44_100;
    metadata.channels = 1;
    metadata.sample_rate = 0; // unknown
    metadata.bpm = 0.0;
    sampler.apply_topology_mutation(TopologyMutation::AddSource {
        node_idx: id as u32,
        buffer: Arc::new(vec![0.25f32; 44_100]),
        sample_id: id,
        metadata: Some(Arc::new(metadata)),
    });

    let transport = Transport {
        bpm: 120.0, beat_position: 0.0, is_playing: true, sample_rate: DEVICE_RATE,
        absolute_samples: 0, system_time_ns: 0, device_time_ns: 0,
    };
    {
        let ctx = ProcessContext { transport: Some(&transport), host: None, sub_block_offset: 0, is_last_sub_block: true };
        sampler.apply_command_with_context(
            &Command::Performance(PerformanceCommand::PlayNode { node_idx: id as u32 }),
            Some(&ctx),
        );
    }
    let mut out = vec![0.0f32; BLOCK];
    let mut outs: Vec<&mut [f32]> = vec![&mut out];
    let mut ctx = ProcessContext { transport: Some(&transport), host: None, sub_block_offset: 0, is_last_sub_block: true };
    sampler.process(&[], &mut outs, &mut ctx);

    let v = sampler.voices.iter().find(|v| v.is_active).expect("voice active");
    assert_eq!(
        v.source_rate_ratio, 1.0,
        "an unknown source rate must not scale playback"
    );
}
