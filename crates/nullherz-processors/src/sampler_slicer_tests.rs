use nullherz_traits::{AudioProcessor, Transport, ProcessContext, AudioConfig, SignalProcessor, SampleMetadata, SoundDNA, Command};
use crate::SamplerProcessor;
use std::sync::Arc;

#[test]
fn test_sampler_slicer_offsets() {
    let mut sampler = SamplerProcessor::new(0);
    let sample_rate = 44100.0;
    sampler.setup(AudioConfig { sample_rate, block_size: 256 });

    let buffer = Arc::new(vec![0.0; 44100 * 4]); // 4 seconds
    let metadata = SampleMetadata {
        bpm: 120.0,
        transients: Arc::new(Vec::new()),
        root_key: None,
        hot_cues: [None; 8],
        loop_points: None,
        beat_grid_offset: 0,
        peaks: Arc::new(Vec::new()),
        dna: SoundDNA::default(),
        midi_map: None,
    };

    sampler.apply_topology_mutation(nullherz_traits::TopologyMutation::AddSource {
        node_idx: 0,
        buffer: buffer.clone(),
        sample_id: 123,
        metadata: Some(Arc::new(metadata)),
    });

    sampler.set_parameter(3, 1.0); // Enable Slicer Mode
    sampler.set_parameter(4, 0.25); // 1/16 grid (0.25 beats)

    let transport = Transport {
        bpm: 120.0,
        beat_position: 1.0,
        is_playing: true,
        sample_rate,
        absolute_samples: 44100,
    };

    let context = ProcessContext {
        transport: Some(&transport),
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };

    // Trigger slice 4 via Command
    sampler.apply_command(&Command::TriggerSlice { node_idx: 0, slice_idx: 4 });

    // Offset should be: slice_idx(4) * grid(0.25) * samples_per_beat(22050) = 1.0 * 22050 = 22050
    let samples_per_beat = 22050.0;
    let expected_offset = 4.0 * 0.25 * samples_per_beat;

    let active_voice = sampler.voices.iter().find(|v| v.is_active).expect("No active voice after trigger");
    assert_eq!(active_voice.play_head, expected_offset as f32);

    // Now test with context
    sampler.reset();
    sampler.apply_command_with_context(&Command::TriggerSlice { node_idx: 0, slice_idx: 8 }, Some(&context));
    let active_voice = sampler.voices.iter().find(|v| v.is_active).expect("No active voice after trigger");
    assert_eq!(active_voice.play_head, 8.0 * 0.25 * samples_per_beat as f32);
    assert_eq!(active_voice.trigger_beat, 1.0);
}

#[test]
fn test_sampler_slicer_phase_lock() {
    let mut sampler = SamplerProcessor::new(0);
    let sample_rate = 44100.0;
    sampler.setup(AudioConfig { sample_rate, block_size: 256 });

    let buffer = Arc::new(vec![0.0; 44100 * 4]);
    let metadata = SampleMetadata {
        bpm: 120.0,
        transients: Arc::new(Vec::new()),
        root_key: None,
        hot_cues: [None; 8],
        loop_points: None,
        beat_grid_offset: 0,
        peaks: Arc::new(Vec::new()),
        dna: SoundDNA::default(),
        midi_map: None,
    };

    sampler.apply_topology_mutation(nullherz_traits::TopologyMutation::AddSource {
        node_idx: 0,
        buffer: buffer.clone(),
        sample_id: 123,
        metadata: Some(Arc::new(metadata)),
    });

    sampler.set_parameter(3, 1.0); // Slicer Mode
    sampler.set_parameter(4, 0.25); // 1/16

    let transport = Transport {
        bpm: 120.0,
        beat_position: 1.0,
        is_playing: true,
        sample_rate,
        absolute_samples: 44100,
    };

    let mut context = ProcessContext {
        transport: Some(&transport),
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };

    sampler.apply_command_with_context(&Command::TriggerSlice { node_idx: 0, slice_idx: 0 }, Some(&context));

    // Initial state: play_head = 0.0, trigger_beat = 1.0
    // Simulate drift: move playhead to 100.0
    sampler.voices[0].play_head = 100.0;

    // Process block (256 samples)
    let mut outputs = [ &mut [0.0f32; 256][..] ];
    sampler.process(&[], &mut outputs, &mut context);

    // Expected position before processing was 0.0.
    // After processing 256 samples, if no phase lock, it would be 100.0 + 256.0 = 356.0.
    // With 1% nudge towards 0.0 (diff = -100.0), it should be 356.0 - 1.0 = 355.0.
    assert!(sampler.voices[0].play_head < 356.0);
    assert!((sampler.voices[0].play_head - 355.0).abs() < 0.1);
}
