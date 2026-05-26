use audio_core::{AudioEngine, AudioProcessor};
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
        if let Command::SetParam { target_id, param_id, value } = command {
            // NOTE: In a real system, we'd avoid println! in the RT thread.
            // For this simulation/example, we'll keep it but acknowledge it's not RT-safe.
            if *target_id == 1 && *param_id == 1 {
                self.osc.set_frequency(*value);
            }
        }
    }
}

fn main() {
    let rb = RingBuffer::new(1024);
    let (mut prod, cons) = rb.split();
    let mut engine = AudioEngine::new(cons);

    let osc = SineOscillator::new(44100.0, 440.0);
    engine.add_processor(Box::new(SineProcessor { osc }));

    let mut out_buffer = [0.0f32; 128];

    println!("Starting simulation...");

    // Block 1: Frequency is 440Hz
    {
        let mut out_ptrs = [&mut out_buffer[..]];
        engine.process_block(&[], &mut out_ptrs, 128);
    }
    println!("Block 1 sample 0: {}", out_buffer[0]);

    // Send command to change frequency EXACTLY at sample 192 (middle of next block)
    prod.push(TimestampedCommand {
        timestamp_samples: 192,
        command: Command::SetParam {
            target_id: 1,
            param_id: 1,
            value: 880.0,
        },
    }).unwrap();

    // Block 2: 128 to 256. Command at 192 should be applied.
    {
        let mut out_ptrs = [&mut out_buffer[..]];
        engine.process_block(&[], &mut out_ptrs, 128);
    }
    println!("Block 2 sample 0: {} (should be 440Hz part)", out_buffer[0]);
    println!("Block 2 sample 64: {} (should be after 880Hz switch)", out_buffer[64]);

    println!("Simulation finished.");
}
