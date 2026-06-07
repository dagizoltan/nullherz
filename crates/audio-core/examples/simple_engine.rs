use audio_core::{AudioEngine, AudioProcessor, ProcessorGraph, ThreadedBackend, AudioBackend};
use audio_dsp::{SineOscillator};
use control_plane::{Command, TimestampedCommand};
use ipc_layer::RingBuffer;

struct SineProcessor {
    osc: SineOscillator,
}

impl AudioProcessor for SineProcessor {
    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        for channel in outputs {
            for sample in channel.iter_mut() {
                use audio_dsp::Oscillator;
                *sample = self.osc.next_sample();
            }
        }
    }
    fn apply_command(&mut self, command: &Command) {
        if let Command::SetParam { target_id, param_id, value, .. } = command {
            if *target_id == 1 && *param_id == 1 { self.osc.set_frequency(*value); }
        }
    }
}

fn main() {
    let rb = RingBuffer::new(1024);
    let (mut prod, cons) = rb.split();
    let garbage_rb = RingBuffer::new(32);
    let (garbage_prod, _) = garbage_rb.split();
    let tel_rb = RingBuffer::new(1024);
    let (tel_prod, _) = tel_rb.split();

    let osc = SineOscillator::new(44100.0, 440.0);
    let engine = AudioEngine::new(cons, garbage_prod, tel_prod, Box::new(SineProcessor { osc }));

    let mut backend = ThreadedBackend::new();
    backend.start(engine).unwrap();

    println!("Starting simulation...");
    prod.push(TimestampedCommand {
        timestamp_samples: 44100,
        command: Command::SetParam { target_id: 1, param_id: 1, value: 880.0, ramp_duration_samples: 0 },
    }).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));
    println!("Simulation finished.");
    backend.stop();
}
