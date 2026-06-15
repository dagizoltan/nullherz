use audio_core::AudioEngine;
use nullherz_traits::{AudioProcessor, MidiHandler, CommandHandler, TopologyHandler, TelemetryProvider};
use ipc_layer::{RingBuffer, MpscRingBuffer};
use std::sync::Arc;

struct GainProcessor {
    gain: f32,
}

impl MidiHandler for GainProcessor {}
impl CommandHandler for GainProcessor {
    fn apply_command(&mut self, command: &nullherz_traits::Command) {
        if let nullherz_traits::Command::SetParam { value, .. } = command {
            self.gain = *value;
        }
    }
}
impl TopologyHandler for GainProcessor {}
impl TelemetryProvider for GainProcessor {}

impl AudioProcessor for GainProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut audio_core::processors::ProcessContext) {
        for (i, input) in inputs.iter().enumerate() {
            if i < outputs.len() {
                for (j, sample) in input.iter().enumerate() {
                    outputs[i][j] = sample * self.gain;
                }
            }
        }
    }
}

fn main() {
    let cmd_buffer = Arc::new(MpscRingBuffer::new(256));
    let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();
    let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();

    let processor = Box::new(GainProcessor { gain: 1.0 });
    let mut engine = AudioEngine::new(
        cmd_buffer.clone(),
        None,
        None,
        None,
        garbage_prod,
        None,
        None,
        None,
        tel_prod,
        processor,
    );

    let input_l = vec![1.0f32; 128];
    let input_r = vec![1.0f32; 128];
    let mut output_l = vec![0.0f32; 128];
    let mut output_r = vec![0.0f32; 128];

    let inputs = [&input_l[..], &input_r[..]];
    let mut outputs = [&mut output_l[..], &mut output_r[..]];

    engine.process_block(&inputs, &mut outputs, 128);

    println!("Processed block, first sample: {}", output_l[0]);
}
