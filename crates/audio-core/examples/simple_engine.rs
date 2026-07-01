use std::sync::Arc;
use ipc_layer::{RingBuffer, MpscRingBuffer};
use nullherz_traits::{AudioProcessor, ProcessContext};
use audio_core::AudioEngine;

struct GainProcessor {
    gain: f32,
}

impl nullherz_traits::SignalProcessor for GainProcessor {
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

impl nullherz_traits::MidiResponder for GainProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for GainProcessor { }

impl AudioProcessor for GainProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

fn main() {
    let (garbage_prod, _garbage_cons) = RingBuffer::<Box<dyn AudioProcessor>>::new(1024).split();
    let (tel_prod, _tel_cons) = RingBuffer::new(1024).split();
    let cmd_buffer = Arc::new(MpscRingBuffer::new(1024));

    let initial_proc: Box<dyn AudioProcessor> = Box::new(GainProcessor { gain: 0.5 });

    let resources = audio_core::engine::EngineResources {
        command_consumer: Box::new(ipc_layer::LocalMpscCommandConsumer(cmd_buffer.clone())),
        command_producer: Box::new(ipc_layer::LocalMpscCommandProducer(cmd_buffer.clone())),
        midi_consumer: None,
        bundle_consumer: None,
        topology_consumer: None,
        garbage_producer: garbage_prod,
        overflow_garbage_producer: None,
        bundle_garbage_producer: None,
        bundle_overflow_producer: None,
        telemetry_producer: Box::new(tel_prod),
    };

    let engine = AudioEngine::new(
        resources,
        initial_proc,
        Arc::new(audio_core::rt_logging::RtLogger::new(256)),
        audio_core::engine::processing_kernel::StandardKernel::default()
    );

    println!("Engine created. Peak ns: {}", engine.metrics.peak_ns.load(std::sync::atomic::Ordering::Relaxed));
}
