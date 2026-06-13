use nullherz_traits::AudioProcessor;

const MODULATION_THRESHOLD: f32 = 0.001;

pub struct ModulationProcessor {
    pub target_id: u64,
    pub param_id: u32,
    pub scale: f32,
    pub offset: f32,
    command_producer: Option<ipc_layer::Producer<control_plane::TimestampedCommand>>,
    last_sent_value: f32,
}

impl ModulationProcessor {
    pub fn new(target_id: u64, param_id: u32, scale: f32, offset: f32) -> Self {
        Self {
            target_id,
            param_id,
            scale,
            offset,
            command_producer: None,
            last_sent_value: f32::NAN,
        }
    }

    pub fn set_producer(&mut self, producer: ipc_layer::Producer<control_plane::TimestampedCommand>) {
        self.command_producer = Some(producer);
    }
}

impl AudioProcessor for ModulationProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], _outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        if inputs.is_empty() { return; }
        let cv = inputs[0];
        if cv.is_empty() { return; }

        // High-precision modulation: We process in 32-sample chunks to balance CPU and responsiveness.
        // For this prototype, we still average over the block but use the engine's sub_block_offset.
        let sum: f32 = cv.iter().sum();
        let avg_cv = sum / cv.len() as f32;
        let val = avg_cv * self.scale + self.offset;

        let is_mod_needed = (val - self.last_sent_value).abs() > MODULATION_THRESHOLD || self.last_sent_value.is_nan();
        if let (true, Some(prod)) = (is_mod_needed, &mut self.command_producer) {
                // Determine block_start_sample for this cycle via telemetry or counter
                // For now, we use a relative offset within the engine's block counter.
                let _ = prod.push(control_plane::TimestampedCommand {
                    timestamp_samples: 0, // 0 indicates current block relative in the MPSC hardened path
                    command: control_plane::Command::SetParam {
                        target_id: self.target_id,
                        param_id: self.param_id,
                        value: val,
                        ramp_duration_samples: 32, // Default smoothing for CV mappings
                    },
                });
                self.last_sent_value = val;
        }
    }
}
