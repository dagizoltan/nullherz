use std::sync::Arc;
use ipc_layer::{RingBuffer, MpscRingBuffer};
use nullherz_traits::{AudioProcessor, ProcessContext};
use audio_core::AudioEngine;

struct GainProcessor {
    gain: f32,
}

impl AudioProcessor for GainProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
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
    let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();
    let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();
    let cmd_buffer = Arc::new(MpscRingBuffer::new(1024));

    let initial_proc: Box<dyn AudioProcessor> = Box::new(GainProcessor { gain: 0.5 });
    let engine = AudioEngine::new(
        Box::new(ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone())),
        Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())),
        None, None, None, garbage_prod, None, None, None,
        Box::new(tel_prod),
        initial_proc,
        Arc::new(audio_core::rt_logging::RtLogger::new(256))
    );

    println!("Engine created. Peak ns: {}", engine.metrics.peak_ns.load(std::sync::atomic::Ordering::Relaxed));
}
