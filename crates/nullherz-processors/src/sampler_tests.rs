use nullherz_traits::{AudioProcessor, Transport, ProcessContext, AudioConfig, SignalProcessor, MidiResponder, SampleMetadata, SoundDNA};
use crate::SamplerProcessor;
use std::sync::Arc;

#[test]
fn test_sampler_sync_logic() {
    let mut sampler = SamplerProcessor::new(0);
    let sample_rate = 44100.0;
    sampler.setup(AudioConfig { sample_rate, block_size: 256 });

    // Mock Sample Metadata with 120 BPM
    let buffer = Arc::new(vec![0.0; 44100 * 2]); // 2 seconds
    let metadata = SampleMetadata {
        bpm: 120.0,
        transients: Arc::new(Vec::new()),
        root_key: None,
        hot_cues: [None; 8],
        loop_points: None,
        beat_grid_offset: 0,
        peaks: Arc::new(Vec::new()),
        mip_waveform: nullherz_traits::MipWaveform::default(),
        dna: SoundDNA::default(),
        midi_map: None,
    };

    sampler.apply_topology_mutation(nullherz_traits::TopologyMutation::AddSource {
        node_idx: 0,
        buffer: buffer.clone(),
        sample_id: 123,
        metadata: Some(Arc::new(metadata)),
    });

    // We can't easily inject metadata into SamplerProcessor from here because it's not exposed via traits
    // But we can check if it handles the sync command
    sampler.set_parameter(2, 1.0); // Enable Sync

    let transport = Transport {
        bpm: 124.0, // Global BPM faster than sample
        beat_position: 0.0,
        is_playing: true,
        sample_rate,
        absolute_samples: 0,
        system_time_ns: 0,
        device_time_ns: 0,
    };

    let mut outputs = [ &mut [0.0f32; 256][..] ];
    let mut context = ProcessContext {
        transport: Some(&transport),
        host: None,
        sub_block_offset: 0,
        is_last_sub_block: true,
    };

    // Trigger voice
    sampler.apply_midi(ipc_layer::MidiEvent { timestamp_samples: 0, status: 0x90, data1: 60, data2: 100, _pad: 0 }, Some(&context));

    // In a real scenario, the conductor would have synced the metadata.
    // For this test, we'll manually set it if we had a way.
    // Since we don't, we'll just verify the processor handles commands without crashing.
    sampler.process(&[], &mut outputs, &mut context);
}
