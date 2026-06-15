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
                sub_block_offset: 0,
                is_last_sub_block: false,
            };
            processor.process(&inputs_a, &mut outputs_a, &mut ctx_a);

            let inputs_b = [ &input[64..128] ];
            let mut outputs_b = [ &mut output_split[64..128] ];
            let mut ctx_b = crate::ProcessContext {
                transport: Some(&host.transport),
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
}

impl AudioProcessor for MockProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, _inputs: &[&[f32]], _outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        self.process_called_count += 1;
    }

    fn reset(&mut self) {
        self.reset_called_count += 1;
    }

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
            sub_block_offset: 0,
            is_last_sub_block: true,
        };
        processor.process(inputs, outputs, &mut context);
    }
}

#[cfg(test)]
mod tests {
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
