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
        channels: 1,
            total_samples: buffer.len() as u64,
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

/// Regression: AddSource on the RT thread must adopt the shared Arc, not
/// deep-clone the track. The old `(*buffer).clone()` memcpy'd the entire
/// sample (tens of MB for a full song) on the audio thread, producing
/// multi-millisecond block spikes at deck load (caught by the survival
/// harness). Adopting an Arc is O(1); we assert a generous ceiling that a
/// deep copy of 100 MB cannot meet even on fast hardware.
#[test]
fn test_add_source_is_o1_no_deep_clone() {
    let mut sampler = SamplerProcessor::new(0);
    sampler.setup(AudioConfig { sample_rate: 44100.0, block_size: 256 });

    // ~100 MB buffer. Constructing it is slow; adopting it must not be.
    let big = Arc::new(vec![0.25f32; 25_000_000]);
    let metadata = Arc::new(SampleMetadata {
        bpm: 174.0,
        transients: Arc::new(Vec::new()),
        root_key: None,
        hot_cues: [None; 8],
        loop_points: None,
        beat_grid_offset: 0,
        peaks: Arc::new(Vec::new()),
        channels: 1,
            total_samples: big.len() as u64,
        mip_waveform: nullherz_traits::MipWaveform::default(),
        dna: SoundDNA::default(),
        midi_map: None,
    });

    let start = std::time::Instant::now();
    sampler.apply_topology_mutation(nullherz_traits::TopologyMutation::AddSource {
        node_idx: 0,
        buffer: big.clone(),
        sample_id: 7,
        metadata: Some(metadata),
    });
    let elapsed = start.elapsed();

    // An Arc adoption is nanoseconds; a 100 MB clone is >10 ms. 2 ms leaves
    // plenty of headroom for CI noise while still failing any deep copy.
    assert!(
        elapsed < std::time::Duration::from_millis(2),
        "AddSource took {:?} — a deep clone has crept back into the RT path",
        elapsed
    );
    // And the buffer must actually be shared, not copied.
    assert_eq!(Arc::strong_count(&big), 2, "sampler must hold the same Arc");
}

#[cfg(test)]
mod stereo_playback_tests {
    use crate::sampler::SamplerProcessor;
    use nullherz_traits::{AudioProcessor, SignalProcessor, ProcessContext, TopologyMutation, SampleMetadata};
    use std::sync::Arc;

    const SR: f32 = 44100.0;

    /// Planar stereo source: channel 0 at `left_hz`, channel 1 at `right_hz`.
    fn planar_stereo(left_hz: f32, right_hz: f32, frames: usize) -> (Arc<Vec<f32>>, Arc<SampleMetadata>) {
        let mut samples = Vec::with_capacity(frames * 2);
        for hz in [left_hz, right_hz] {
            for i in 0..frames {
                samples.push((2.0 * std::f32::consts::PI * hz * i as f32 / SR).sin() * 0.5);
            }
        }
        let mut meta = SampleMetadata::new_empty();
        meta.total_samples = frames as u64;
        meta.channels = 2;
        // Quantize would retune playback against transport BPM; this test is
        // about channel layout, so leave BPM at 0 to keep the raw rate.
        (Arc::new(samples), Arc::new(meta))
    }

    fn render(p: &mut SamplerProcessor, channels: usize, blocks: usize, block: usize) -> Vec<Vec<f32>> {
        let mut captured = vec![Vec::new(); channels];
        for _ in 0..blocks {
            let mut bufs = vec![vec![0.0f32; block]; channels];
            {
                let mut refs: Vec<&mut [f32]> = bufs.iter_mut().map(|b| &mut b[..]).collect();
                let mut ctx = ProcessContext {
                    transport: None,
                    host: None,
                    sub_block_offset: 0,
                    is_last_sub_block: true,
                };
                p.process(&[], &mut refs, &mut ctx);
            }
            for (c, b) in bufs.iter().enumerate() {
                captured[c].extend_from_slice(b);
            }
        }
        captured
    }

    fn dominant_hz(s: &[f32]) -> f32 {
        let crossings = s.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        crossings as f32 * SR / s.len() as f32
    }

    /// The bug this closes: sample buffers were interleaved while the voice
    /// advanced one buffer ELEMENT per output frame, so L,R,L,R was consumed as
    /// four consecutive frames — every stereo file played an octave high at
    /// double tempo. Both demo tracks are stereo, so this was every track.
    #[test]
    fn test_stereo_sample_plays_at_correct_speed_and_keeps_channels_separate() {
        let frames = 44100;
        let (buffer, metadata) = planar_stereo(440.0, 880.0, frames);

        let mut p = SamplerProcessor::new(0);
        p.apply_topology_mutation(TopologyMutation::AddSource {
            node_idx: 0,
            buffer,
            sample_id: 1,
            metadata: Some(metadata),
        });
        p.apply_command(&nullherz_traits::Command::Performance(
            nullherz_traits::PerformanceCommand::PlayNode { node_idx: 0 },
        ));

        let out = render(&mut p, 2, 100, 128);

        let left_hz = dominant_hz(&out[0]);
        let right_hz = dominant_hz(&out[1]);

        assert!(
            (left_hz - 440.0).abs() < 15.0,
            "left channel should play at its authored 440 Hz, measured {:.1} Hz \
             (880 Hz here means frames are being consumed two at a time)",
            left_hz
        );
        assert!(
            (right_hz - 880.0).abs() < 25.0,
            "right channel should carry its OWN 880 Hz tone, measured {:.1} Hz \
             (440 Hz here means the right channel is a copy of the left)",
            right_hz
        );
    }

    /// A mono source must still fill both channels of a stereo strip.
    #[test]
    fn test_mono_sample_fills_both_outputs() {
        let frames = 44100;
        let samples: Vec<f32> = (0..frames)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR).sin() * 0.5)
            .collect();
        let mut meta = SampleMetadata::new_empty();
        meta.total_samples = frames as u64;
        meta.channels = 1;

        let mut p = SamplerProcessor::new(0);
        p.apply_topology_mutation(TopologyMutation::AddSource {
            node_idx: 0,
            buffer: Arc::new(samples),
            sample_id: 1,
            metadata: Some(Arc::new(meta)),
        });
        p.apply_command(&nullherz_traits::Command::Performance(
            nullherz_traits::PerformanceCommand::PlayNode { node_idx: 0 },
        ));

        let out = render(&mut p, 2, 100, 128);

        for (c, ch) in out.iter().enumerate() {
            let hz = dominant_hz(ch);
            assert!(
                (hz - 440.0).abs() < 15.0,
                "mono source must feed channel {} at 440 Hz, measured {:.1} Hz",
                c,
                hz
            );
        }
    }
}
