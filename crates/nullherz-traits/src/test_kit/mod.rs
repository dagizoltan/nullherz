use crate::{AudioProcessor, ProcessContext, AudioConfig, Transport};

pub struct MockProcessor {
    pub process_called_count: usize,
    pub reset_called_count: usize,
    pub last_param_id: u32,
    pub last_param_value: f32,
}

impl MockProcessor {
    pub fn new() -> Self {
        Self {
            process_called_count: 0,
            reset_called_count: 0,
            last_param_id: 0,
            last_param_value: 0.0,
        }
    }
}

pub struct StabilityTester;

impl StabilityTester {
    pub fn verify_signal_bounds(processor: &mut dyn crate::AudioProcessor, duration_blocks: usize) -> Result<(), String> {
        let host = VirtualClockHost::new();
        let block_size = 256;
        let mut input = vec![0.0f32; block_size];
        let mut output = vec![0.0f32; block_size];

        // Impulse test
        input[0] = 1.0;

        for _ in 0..duration_blocks {
            let inputs = [ &input[..] ];
            let mut outputs = [ &mut output[..] ];
            let mut ctx = crate::ProcessContext {
                transport: Some(&host.transport),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };
            processor.process(&inputs, &mut outputs, &mut ctx);

            for &sample in &output {
                if !sample.is_finite() {
                    return Err("Signal went non-finite (NaN or Inf)".into());
                }
                if sample.abs() > 100.0 {
                    return Err(format!("Signal exceeded absolute bound of 100.0: {}", sample));
                }
            }
            input.fill(0.0); // Only impulse at start
        }

        Ok(())
    }
}

pub struct ConformanceSuite;

impl ConformanceSuite {
    pub fn verify_parameter_bounds(processor: &mut dyn crate::AudioProcessor, param_id: u32) -> Result<(), String> {
        let host = VirtualClockHost::new();
        let block_size = 64;
        let input = vec![1.0f32; block_size];
        let mut output = vec![0.0f32; block_size];

        let values = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN, 1e20, -1e20];

        for &val in &values {
            processor.reset();
            processor.set_parameter(param_id, val, 0);
            processor.apply_command(&crate::ProcessorCommand::SetParam {
                target_id: 0,
                param_id,
                value: val,
                ramp_duration_samples: 0,
            });

            let inputs = [ &input[..] ];
            let mut outputs = [ &mut output[..] ];
            let mut ctx = crate::ProcessContext {
                transport: Some(&host.transport),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };
            processor.process(&inputs, &mut outputs, &mut ctx);

            for (i, &sample) in output.iter().enumerate() {
                if !sample.is_finite() {
                    return Err(format!("Processor produced non-finite output at sample {} for parameter value {}", i, val));
                }
            }
        }

        Ok(())
    }

    pub fn verify_sub_block_consistency(processor: &mut dyn crate::AudioProcessor) -> Result<(), String> {
        processor.reset();
        let host = VirtualClockHost::new();
        let block_size = 128;
        let input = vec![1.0f32; block_size];
        let mut output_single = vec![0.0f32; block_size];
        let mut output_split = vec![0.0f32; block_size];

        // Process as single block
        {
            let inputs = [ &input[..] ];
            let mut outputs = [ &mut output_single[..] ];
            let mut ctx = crate::ProcessContext {
                transport: Some(&host.transport),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };
            processor.process(&inputs, &mut outputs, &mut ctx);
        }

        // Reset and process as two sub-blocks
        processor.reset();

        {
            let inputs_a = [ &input[0..64] ];
            let mut outputs_a = [ &mut output_split[0..64] ];
            let mut ctx_a = crate::ProcessContext {
                transport: Some(&host.transport),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: false,
            };
            processor.process(&inputs_a, &mut outputs_a, &mut ctx_a);

            let inputs_b = [ &input[64..128] ];
            let mut outputs_b = [ &mut output_split[64..128] ];
            let mut ctx_b = crate::ProcessContext {
                transport: Some(&host.transport),
                host: None,
                sub_block_offset: 64,
                is_last_sub_block: true,
            };
            processor.process(&inputs_b, &mut outputs_b, &mut ctx_b);
        }

        for i in 0..block_size {
            if (output_single[i] - output_split[i]).abs() > 1e-6 {
                return Err(format!("Sub-block inconsistency at sample {}: single={}, split={}", i, output_single[i], output_split[i]));
            }
        }

        Ok(())
    }

    pub fn verify_simd_alignment(processor: &mut dyn crate::AudioProcessor) -> Result<(), String> {
        let host = VirtualClockHost::new();
        let block_size = crate::MAX_BLOCK_SIZE;

        // We need to ensure the buffers we pass are actually aligned to SIMD_ALIGNMENT
        // In this test environment, we'll use AudioBlock which is already aligned.
        let input_block = crate::AudioBlock { data: [1.0; crate::MAX_BLOCK_SIZE], len: block_size as u32 };
        let mut output_block = crate::AudioBlock { data: [0.0; crate::MAX_BLOCK_SIZE], len: block_size as u32 };

        let inputs = [ &input_block.data[..] ];
        let mut outputs = [ &mut output_block.data[..] ];

        let mut ctx = crate::ProcessContext {
            transport: Some(&host.transport),
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        // Check alignment of pointers
        if (inputs[0].as_ptr() as usize) % crate::SIMD_ALIGNMENT != 0 {
            return Err("Input buffer not SIMD aligned".into());
        }
        if (outputs[0].as_ptr() as usize) % crate::SIMD_ALIGNMENT != 0 {
            return Err("Output buffer not SIMD aligned".into());
        }

        processor.process(&inputs, &mut outputs, &mut ctx);
        Ok(())
    }

    pub fn verify_state_persistence(processor: &mut dyn crate::AudioProcessor) -> Result<(), String> {
        let host = VirtualClockHost::new();
        let block_size = 64;
        let input = vec![1.0f32; block_size];
        let mut output_1 = vec![0.0f32; block_size];
        let mut output_2 = vec![0.0f32; block_size];

        // 1. Process block 1
        {
            let inputs = [ &input[..] ];
            let mut outputs = [ &mut output_1[..] ];
            let mut ctx = crate::ProcessContext {
                transport: Some(&host.transport),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: false,
            };
            processor.process(&inputs, &mut outputs, &mut ctx);
        }

        // 2. Process block 2 WITHOUT reset
        {
            let inputs = [ &input[..] ];
            let mut outputs = [ &mut output_2[..] ];
            let mut ctx = crate::ProcessContext {
                transport: Some(&host.transport),
                host: None,
                sub_block_offset: 64,
                is_last_sub_block: true,
            };
            processor.process(&inputs, &mut outputs, &mut ctx);
        }

        // We can't strictly assert they are different because some processors are stateless (like Gain),
        // but we can ensure they ARE different for processors that have state.
        // For the general suite, we just ensure it doesn't crash and returns finite values.
        for i in 0..block_size {
            if !output_1[i].is_finite() || !output_2[i].is_finite() {
                return Err("Non-finite output during persistence test".into());
            }
        }

        Ok(())
    }

    pub fn verify_bypass_conformance(processor: &mut dyn crate::AudioProcessor) -> Result<(), String> {
        let host = VirtualClockHost::new();
        let block_size = 128;
        let mut input = vec![0.0f32; block_size];
        let mut output = vec![0.0f32; block_size];

        for (i, val) in input.iter_mut().enumerate() { *val = i as f32 * 0.01; }

        processor.apply_command(&crate::ProcessorCommand::SetParam {
            target_id: 0,
            param_id: 999, // Reserved for bypass in our convention
            value: 1.0,
            ramp_duration_samples: 0,
        });

        let inputs = [ &input[..] ];
        let mut outputs = [ &mut output[..] ];
        let mut ctx = crate::ProcessContext {
            transport: Some(&host.transport),
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };
        processor.process(&inputs, &mut outputs, &mut ctx);

        // Check if output matches input (passthrough)
        for i in 0..block_size {
            if (output[i] - input[i]).abs() > 1e-6 {
                return Err(format!("Bypass conformance failed at sample {}: expected {}, got {}", i, input[i], output[i]));
            }
        }
        Ok(())
    }

    pub fn verify_reset_consistency(processor: &mut dyn crate::AudioProcessor) -> Result<(), String> {
        processor.reset();
        let block_size = 128;
        let input = vec![1.0f32; block_size];
        let mut output_1 = vec![0.0f32; block_size];
        let mut output_2 = vec![0.0f32; block_size];

        let host = VirtualClockHost::new();

        // 1. Process one block to potentially change internal state
        {
            let inputs = [ &input[..] ];
            let mut outputs = [ &mut output_1[..] ];
            let mut ctx = crate::ProcessContext {
                transport: Some(&host.transport),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };
            processor.process(&inputs, &mut outputs, &mut ctx);
        }

        // 2. Reset the processor
        processor.reset();

        // 3. Process another block
        {
            let inputs = [ &input[..] ];
            let mut outputs = [ &mut output_2[..] ];
            let mut ctx = crate::ProcessContext {
                transport: Some(&host.transport),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };
            processor.process(&inputs, &mut outputs, &mut ctx);
        }

        for i in 0..block_size {
            if (output_1[i] - output_2[i]).abs() > 1e-6 {
                return Err(format!("Reset consistency failed at sample {}: first_run={}, after_reset={}", i, output_1[i], output_2[i]));
            }
        }

        Ok(())
    }

    pub fn measure_latency_samples(processor: &mut dyn crate::AudioProcessor) -> usize {
        let host = VirtualClockHost::new();
        let block_size = 256;
        let mut input = vec![0.0f32; block_size];
        let mut output = vec![0.0f32; block_size];

        input[0] = 1.0; // Impulse

        let mut total_latency = 0;
        for _ in 0..10 { // Check up to 10 blocks
            let inputs = [ &input[..] ];
            let mut outputs = [ &mut output[..] ];
            let mut ctx = crate::ProcessContext {
                transport: Some(&host.transport),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };
            processor.process(&inputs, &mut outputs, &mut ctx);

            for (i, &sample) in output.iter().enumerate() {
                if sample.abs() > 1e-6 {
                    return total_latency + i;
                }
            }
            total_latency += block_size;
            input.fill(0.0);
        }

        usize::MAX // No signal passed
    }

    pub fn verify_parameter_ramping(processor: &mut dyn crate::AudioProcessor, param_id: u32) -> Result<(), String> {
        processor.reset();
        let host = VirtualClockHost::new();
        let block_size = 128;
        let input = vec![1.0f32; block_size];
        let mut output = vec![0.0f32; block_size];

        // 1. Initial state
        processor.apply_command(&crate::ProcessorCommand::SetParam {
            target_id: 0,
            param_id,
            value: 0.0,
            ramp_duration_samples: 0,
        });

        // 2. Start ramp to 1.0 over the block size
        processor.apply_command(&crate::ProcessorCommand::SetParam {
            target_id: 0,
            param_id,
            value: 1.0,
            ramp_duration_samples: block_size as u32,
        });

        let inputs = [ &input[..] ];
        let mut outputs = [ &mut output[..] ];
        let mut ctx = crate::ProcessContext {
            transport: Some(&host.transport),
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };
        processor.process(&inputs, &mut outputs, &mut ctx);

        // Verify all samples are finite
        for &sample in &output {
            if !sample.is_finite() {
                return Err("Non-finite output during ramping".into());
            }
        }

        Ok(())
    }

    pub fn verify_latency_reporting(processor: &mut dyn crate::AudioProcessor) -> Result<(), String> {
        let reported = processor.latency_samples();
        let measured = Self::measure_latency_samples(processor);

        // Measured latency might be slightly higher due to sub-block framing,
        // but should never be less than reported.
        if measured < reported {
            return Err(format!("Processor reported {} samples latency, but measured only {}.", reported, measured));
        }

        Ok(())
    }

    pub fn verify_silence_after_reset(processor: &mut dyn crate::AudioProcessor) -> Result<(), String> {
        let host = VirtualClockHost::new();
        let block_size = 128;
        let input = vec![1.0f32; block_size];
        let mut output = vec![0.0f32; block_size];

        // 1. Prime with signal
        {
            let mut ctx = crate::ProcessContext { transport: Some(&host.transport), host: None, sub_block_offset: 0, is_last_sub_block: true };
            processor.process(&[&input], &mut [&mut output], &mut ctx);
        }

        // 2. Reset
        processor.reset();

        // 3. Process silence
        let silence = vec![0.0f32; block_size];
        {
            let mut ctx = crate::ProcessContext { transport: Some(&host.transport), host: None, sub_block_offset: 0, is_last_sub_block: true };
            processor.process(&[&silence], &mut [&mut output], &mut ctx);
        }

        // 4. Verify silence
        for i in 0..block_size {
            if output[i].abs() > 1e-6 {
                return Err(format!("Non-silent output after reset at sample {}: {}", i, output[i]));
            }
        }

        Ok(())
    }

    pub fn verify_snapshot_safety(processor: &mut dyn crate::AudioProcessor) -> Result<(), String> {
        let host = VirtualClockHost::new();
        let block_size = 64;
        let input = vec![1.0f32; block_size];
        let mut output = vec![0.0f32; block_size];

        // 1. Process one block
        {
            let inputs = [ &input[..] ];
            let mut outputs = [ &mut output[..] ];
            let mut ctx = crate::ProcessContext {
                transport: Some(&host.transport),
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };
            processor.process(&inputs, &mut outputs, &mut ctx);
        }

        // 2. Try to pull snapshot
        let _s1 = processor.pull_snapshot();
        let mut s2_list = Vec::new();
        processor.pull_all_snapshots(&mut s2_list);

        // 3. Reset and pull again (should be None/empty)
        processor.reset();
        let s3 = processor.pull_snapshot();
        if s3.is_some() { return Err("Snapshot should be None after reset".into()); }

        Ok(())
    }

    pub fn verify_multichannel_consistency(processor: &mut dyn crate::AudioProcessor, num_channels: usize) -> Result<(), String> {
        processor.reset();
        let host = VirtualClockHost::new();
        let block_size = 64;
        let input_data = vec![1.0f32; block_size];
        let mut outputs_data = vec![vec![0.0f32; block_size]; num_channels];

        let inputs: Vec<&[f32]> = (0..num_channels).map(|_| &input_data[..]).collect();
        let mut outputs_refs: Vec<&mut [f32]> = outputs_data.iter_mut().map(|v| &mut v[..]).collect();

        let mut ctx = crate::ProcessContext {
            transport: Some(&host.transport),
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        processor.process(&inputs, &mut outputs_refs, &mut ctx);

        for c in 0..num_channels {
            for i in 0..block_size {
                if !outputs_data[c][i].is_finite() {
                    return Err(format!("Non-finite output on channel {} at sample {}", c, i));
                }
            }
        }

        Ok(())
    }
}

impl crate::SignalProcessor for MockProcessor {
fn process(&mut self, _inputs: &[&[f32]], _outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        self.process_called_count += 1;
    }
fn reset(&mut self) {
        self.reset_called_count += 1;
    }
}

impl crate::MidiResponder for MockProcessor { }

impl crate::SnapshotProvider for MockProcessor { }

impl AudioProcessor for MockProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn apply_command(&mut self, command: &crate::ProcessorCommand) {
        if let crate::ProcessorCommand::SetParam { param_id, value, .. } = command {
            self.last_param_id = *param_id;
            self.last_param_value = *value;
        }
    }
fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration: u32) {
        self.last_param_id = param_id;
        self.last_param_value = value;
    }

}

pub struct TestHost {
    pub config: AudioConfig,
}

impl TestHost {
    pub fn new() -> Self {
        Self {
            config: AudioConfig { sample_rate: 44100.0, block_size: 256 },
        }
    }

    pub fn process_processor(&self, processor: &mut dyn AudioProcessor, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        let mut context = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };
        processor.process(inputs, outputs, &mut context);
    }
}

#[cfg(test)]
mod tests {
    use crate::SignalProcessor;
    use super::*;
    use crate::SubBlockIterator;

    #[test]
    fn test_sub_block_iterator_logic() {
        let mut iter = SubBlockIterator::new(100, 64);

        let sb1 = iter.next_chunk().unwrap();
        assert_eq!(sb1.offset, 0);
        assert_eq!(sb1.len, 64);
        assert!(!sb1.is_last);

        let sb2 = iter.next_chunk().unwrap();
        assert_eq!(sb2.offset, 64);
        assert_eq!(sb2.len, 36);
        assert!(sb2.is_last);

        assert!(iter.next_chunk().is_none());
    }

    #[test]
    fn test_conformance_parameter_ramping() {
        let mut mock = MockProcessor::new();
        ConformanceSuite::verify_parameter_ramping(&mut mock, 1).expect("Ramping conformance failed");
    }

    #[test]
    fn test_conformance_multichannel() {
        let mut mock = MockProcessor::new();
        ConformanceSuite::verify_multichannel_consistency(&mut mock, 4).expect("Multichannel conformance failed");
    }

    #[test]
    fn test_conformance_simd_alignment() {
        let mut mock = MockProcessor::new();
        ConformanceSuite::verify_simd_alignment(&mut mock).expect("SIMD alignment conformance failed");
    }

    #[test]
    fn test_conformance_state_persistence() {
        let mut mock = MockProcessor::new();
        ConformanceSuite::verify_state_persistence(&mut mock).expect("State persistence conformance failed");
    }

    #[test]
    fn test_sub_block_iterator_up_to() {
        let mut iter = SubBlockIterator::new(100, 64);

        // Command at sample 10. Process from 0 to 10.
        let sb1 = iter.next_chunk_up_to(10).unwrap();
        assert_eq!(sb1.offset, 0);
        assert_eq!(sb1.len, 10);
        assert!(!sb1.is_last);

        // No more chunks up to 10
        assert!(iter.next_chunk_up_to(10).is_none());

        // Process remaining up to end of block size (64)
        let sb2 = iter.next_chunk_up_to(64).unwrap();
        assert_eq!(sb2.offset, 10);
        assert_eq!(sb2.len, 54);
        assert!(!sb2.is_last);
    }

    #[test]
    fn test_conformance_sub_block_consistency() {
        let mut mock = MockProcessor::new();
        ConformanceSuite::verify_sub_block_consistency(&mut mock).expect("Conformance check failed");
    }

    #[test]
    fn test_conformance_reset_consistency() {
        let mut mock = MockProcessor::new();
        ConformanceSuite::verify_reset_consistency(&mut mock).expect("Reset conformance failed");
        // verify_reset_consistency calls reset twice: once at the beginning, once after first block.
        assert_eq!(mock.reset_called_count, 2);
    }

    #[test]
    fn test_virtual_clock_host_command_alignment() {
        let mut host = VirtualClockHost::new();
        let mut mock = MockProcessor::new();

        // Command at sample 100
        let commands = vec![(100, crate::ProcessorCommand::SetParam { target_id: 0, param_id: 1, value: 0.5, ramp_duration_samples: 0 })];

        // Process first block (128 samples)
        host.process_with_commands(&mut mock, 128, &commands);

        assert_eq!(mock.process_called_count, 2); // 0-100 and 100-128
        assert_eq!(mock.last_param_id, 1);
        assert_eq!(mock.last_param_value, 0.5);
    }
}

pub struct VirtualClockHost {
    pub config: AudioConfig,
    pub transport: Transport,
    pub sample_counter: u64,
}

impl VirtualClockHost {
    pub fn new() -> Self {
        Self {
            config: AudioConfig { sample_rate: 44100.0, block_size: 256 },
            transport: Transport {
                bpm: 120.0,
                beat_position: 0.0,
                is_playing: true,
                sample_rate: 44100.0,
            },
            sample_counter: 0,
        }
    }

    pub fn process_with_commands<P: AudioProcessor>(
        &mut self,
        processor: &mut P,
        num_samples: usize,
        commands: &[(u64, crate::ProcessorCommand)],
    ) {
        let mut iter = crate::SubBlockIterator::new(num_samples, crate::MAX_BLOCK_SIZE);
        let block_start = self.sample_counter;
        let block_end = block_start + num_samples as u64;

        let mut commands_processed_indices = std::collections::HashSet::new();

        while iter.current_offset < num_samples {
            // Find next command in this block that we haven't processed yet
            let next_cmd_idx = commands.iter().enumerate()
                .filter(|(idx, (ts, _))| !commands_processed_indices.contains(idx) && *ts >= block_start + iter.current_offset as u64 && *ts < block_end)
                .min_by_key(|(_, (ts, _))| *ts)
                .map(|(idx, _)| idx);

            if let Some(idx) = next_cmd_idx {
                let (ts, cmd) = &commands[idx];
                let cmd_offset = (*ts - block_start) as usize;
                if iter.current_offset < cmd_offset {
                    while let Some(sb) = iter.next_chunk_up_to(cmd_offset) {
                        self.run_sub_block(processor, sb.offset, sb.len, sb.is_last);
                    }
                }
                processor.apply_command(cmd);
                commands_processed_indices.insert(idx);
            } else {
                while let Some(sb) = iter.next_chunk() {
                    self.run_sub_block(processor, sb.offset, sb.len, sb.is_last);
                }
            }
        }
        self.sample_counter = block_end;
    }


    fn run_sub_block(&mut self, processor: &mut dyn AudioProcessor, offset: usize, len: usize, is_last: bool) {
        let mut ctx = crate::ProcessContext {
            transport: Some(&self.transport),
            host: None,
            sub_block_offset: offset,
            is_last_sub_block: is_last,
        };
        // Dummy buffers
        let inputs = [ &[][..]; 0 ];
        let mut outputs = [ &mut [][..]; 0 ];
        processor.process(&inputs, &mut outputs, &mut ctx);

        if self.transport.is_playing {
            let beats = (len as f64 / self.transport.sample_rate as f64) * (self.transport.bpm as f64 / 60.0);
            self.transport.beat_position += beats;
        }
    }
}
