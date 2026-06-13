use crate::{AudioProcessor, ProcessContext, AudioConfig};

pub struct MockProcessor {
    pub process_called_count: usize,
    pub last_param_id: u32,
    pub last_param_value: f32,
}

impl MockProcessor {
    pub fn new() -> Self {
        Self {
            process_called_count: 0,
            last_param_id: 0,
            last_param_value: 0.0,
        }
    }
}

impl AudioProcessor for MockProcessor {
    fn process(&mut self, _inputs: &[&[f32]], _outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        self.process_called_count += 1;
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
