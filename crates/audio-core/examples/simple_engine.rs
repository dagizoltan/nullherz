use audio_core::{AudioEngine, AudioProcessor};
use nullherz_backends::{ThreadedBackend, AudioBackend};
use control_plane::{TimestampedCommand};
use ipc_layer::{RingBuffer, MpscRingBuffer};
use std::sync::{Arc, Mutex};
use std::thread;

struct GainProcessor {
    gain: f32,
}
impl AudioProcessor for GainProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut nullherz_traits::ProcessContext) {
        for i in 0..inputs.len().min(outputs.len()) {
            for j in 0..inputs[i].len() {
                outputs[i][j] = inputs[i][j] * self.gain;
            }
        }
    }
}

fn main() {
    let cmd_buffer = Arc::new(MpscRingBuffer::<TimestampedCommand>::new(1024));
    let (garbage_prod, _garbage_cons) = RingBuffer::new(1024).split();
    let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();

    let initial_proc = Box::new(GainProcessor { gain: 0.5 });
    let engine = AudioEngine::new(cmd_buffer, None, None, None, garbage_prod, None, None, None, tel_prod, initial_proc);
    let engine_handle = Arc::new(Mutex::new(Some(engine)));

    let mut backend = ThreadedBackend::new();
    backend.start(engine_handle).unwrap();

    println!("Engine running with simple processor...");
    thread::sleep(std::time::Duration::from_millis(200));

    backend.stop();
    println!("Engine stopped.");
}
