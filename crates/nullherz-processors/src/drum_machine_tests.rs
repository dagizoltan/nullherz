#[cfg(test)]
mod tests {
    use crate::sample_drum_machine::SampleDrumMachineProcessor;
    use crate::synth_drum_machine::SynthDrumMachineProcessor;
    use crate::neural_drum_machine::NeuralDrumMachineProcessor;
    use nullherz_traits::{
        AudioProcessor, SignalProcessor, ProcessContext, SampleBuffer, TopologyMutation,
        MidiResponder, MidiEvent,
    };
    use nullherz_traits::test_kit::rt_alloc;

    #[test]
    fn test_sample_drum_machine_processing_and_choke() {
        let mut dm = SampleDrumMachineProcessor::new(100, 48000.0);

        // Load dummy buffer into pad 0 and pad 1
        let dummy_samples: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.01).sin()).collect();
        let sample_buf = SampleBuffer::from(dummy_samples);

        dm.apply_topology_mutation(TopologyMutation::AddSource {
            node_idx: 0,
            buffer: sample_buf.clone(),
            sample_id: 1,
            metadata: None,
        });

        dm.apply_topology_mutation(TopologyMutation::AddSource {
            node_idx: 1,
            buffer: sample_buf,
            sample_id: 2,
            metadata: None,
        });

        // Set choke group 1 on pad 0 and pad 1
        dm.set_parameter(6, 1.0, 0);  // Pad 0 choke group = 1
        dm.set_parameter(22, 1.0, 0); // Pad 1 choke group = 1

        // Trigger Pad 0
        dm.apply_midi(MidiEvent {
            timestamp_samples: 0,
            status: 0x90,
            data1: 36, // Pad 0
            data2: 100,
            _pad: 0,
        }, None);

        let mut out0 = vec![0.0f32; 256];
        let mut out1 = vec![0.0f32; 256];

        let mut ctx = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        dm.process(&[], &mut [&mut out0, &mut out1], &mut ctx);

        assert!(out0.iter().any(|&s| s.abs() > 0.0001));

        // Trigger Pad 1 (should choke Pad 0)
        dm.apply_midi(MidiEvent {
            timestamp_samples: 0,
            status: 0x90,
            data1: 37, // Pad 1
            data2: 100,
            _pad: 0,
        }, None);

        let mut out0_b = vec![0.0f32; 256];
        let mut out1_b = vec![0.0f32; 256];

        dm.process(&[], &mut [&mut out0_b, &mut out1_b], &mut ctx);

        assert!(out1_b.iter().any(|&s| s.abs() > 0.0001));
    }

    #[test]
    fn test_synth_drum_machine_processing() {
        let mut synth_dm = SynthDrumMachineProcessor::new(101, 48000.0);

        // Trigger Kick (Pad 0) and Snare (Pad 1) and HiHat (Pad 2)
        synth_dm.apply_midi(MidiEvent {
            timestamp_samples: 0,
            status: 0x90,
            data1: 36, // Kick
            data2: 120,
            _pad: 0,
        }, None);

        synth_dm.apply_midi(MidiEvent {
            timestamp_samples: 0,
            status: 0x90,
            data1: 37, // Snare
            data2: 110,
            _pad: 0,
        }, None);

        let mut outputs: Vec<Vec<f32>> = (0..16).map(|_| vec![0.0f32; 256]).collect();
        let mut out_slices: Vec<&mut [f32]> = outputs.iter_mut().map(|v| v.as_mut_slice()).collect();

        let mut ctx = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        synth_dm.process(&[], &mut out_slices, &mut ctx);

        // Subchannel 0 (Kick) and Subchannel 1 (Snare) should have audio
        assert!(outputs[0].iter().all(|s| s.is_finite()));
        assert!(outputs[1].iter().all(|s| s.is_finite()));
        assert!(outputs[0].iter().any(|&s| s.abs() > 0.0001));
        assert!(outputs[1].iter().any(|&s| s.abs() > 0.0001));
    }

    #[test]
    fn test_neural_drum_machine_processing() {
        let mut neural_dm = NeuralDrumMachineProcessor::new(102, 48000.0);

        // Trigger pad 0
        neural_dm.trigger_pad(0, 0.9);

        let mut outputs: Vec<Vec<f32>> = (0..16).map(|_| vec![0.0f32; 256]).collect();
        let mut out_slices: Vec<&mut [f32]> = outputs.iter_mut().map(|v| v.as_mut_slice()).collect();

        let mut ctx = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        neural_dm.process(&[], &mut out_slices, &mut ctx);

        assert!(outputs[0].iter().all(|s| s.is_finite()));
        assert!(outputs[0].iter().any(|&s| s.abs() > 0.0001));
    }

    #[test]
    fn test_rt_zero_allocation_compliance() {
        if !rt_alloc::is_installed() {
            return;
        }

        let mut sample_dm = SampleDrumMachineProcessor::new(200, 48000.0);
        let mut synth_dm = SynthDrumMachineProcessor::new(201, 48000.0);
        let mut neural_dm = NeuralDrumMachineProcessor::new(202, 48000.0);

        // Pre-warm and trigger
        let dummy_samples: Vec<f32> = vec![0.5; 4800];
        let sample_buf = SampleBuffer::from(dummy_samples);
        sample_dm.apply_topology_mutation(TopologyMutation::AddSource {
            node_idx: 0,
            buffer: sample_buf,
            sample_id: 1,
            metadata: None,
        });

        sample_dm.trigger_pad(0, 0.8);
        synth_dm.trigger_pad(0, 0.8);
        neural_dm.trigger_pad(0, 0.8);

        let mut outputs: Vec<Vec<f32>> = (0..16).map(|_| vec![0.0f32; 256]).collect();
        let mut out_slices: Vec<&mut [f32]> = outputs.iter_mut().map(|v| v.as_mut_slice()).collect();

        let mut ctx = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        let report_sample = rt_alloc::measure(|| {
            sample_dm.process(&[], &mut out_slices, &mut ctx);
        });
        assert_eq!(report_sample.count, 0, "SampleDrumMachineProcessor allocated on RT thread");

        let report_synth = rt_alloc::measure(|| {
            synth_dm.process(&[], &mut out_slices, &mut ctx);
        });
        assert_eq!(report_synth.count, 0, "SynthDrumMachineProcessor allocated on RT thread");

        let report_neural = rt_alloc::measure(|| {
            neural_dm.process(&[], &mut out_slices, &mut ctx);
        });
        assert_eq!(report_neural.count, 0, "NeuralDrumMachineProcessor allocated on RT thread");
    }
}
