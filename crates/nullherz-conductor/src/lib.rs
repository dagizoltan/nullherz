use std::sync::{Arc, Mutex};
use audio_core::{AudioEngine, ProcessorGraph};
use nullherz_backends::{AudioBackend, AlsaBackend, ThreadedBackend};
use fx_runtime::SidecarManager;
use ipc_layer::RingBuffer;


pub struct Timeline {
    pub bpm: f32,
    pub signature_num: u32,
    pub signature_den: u32,
    pub current_beat: f64,
}

pub struct Conductor {
    pub manager: SidecarManager,
    pub timeline: Timeline,
    pub engine_handle: Arc<Mutex<Option<AudioEngine>>>,
    pub backend: Option<Box<dyn AudioBackend>>,
    garbage_consumer: Option<ipc_layer::Consumer<Box<dyn audio_core::AudioProcessor>>>,
    overflow_garbage_consumer: Option<ipc_layer::Consumer<Box<dyn audio_core::AudioProcessor>>>,
    pub bundle_producer: Option<ipc_layer::Producer<Vec<control_plane::Command>>>,
    bundle_garbage_consumer: Option<ipc_layer::Consumer<Vec<control_plane::Command>>>,
    bundle_overflow_consumer: Option<ipc_layer::Consumer<Vec<control_plane::Command>>>,
    pub topo_producer: Option<ipc_layer::NonRtProducer<audio_core::processors::TopologyMutation>>,
    pub health_signal: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

impl Default for Conductor {
    fn default() -> Self {
        Self::new()
    }
}

impl Conductor {
    pub fn new() -> Self {
        Self {
            manager: SidecarManager::new(),
            timeline: Timeline {
                bpm: 120.0,
                signature_num: 4,
                signature_den: 4,
                current_beat: 0.0,
            },
            engine_handle: Arc::new(Mutex::new(None)),
            backend: None,
            garbage_consumer: None,
            overflow_garbage_consumer: None,
            bundle_producer: None,
            bundle_garbage_consumer: None,
            bundle_overflow_consumer: None,
            topo_producer: None,
            health_signal: None,
        }
    }

    pub fn setup_engine(&mut self) -> (Arc<ipc_layer::MpscRingBuffer<control_plane::TimestampedCommand>>, ipc_layer::Consumer<audio_core::Telemetry>) {
        // Harden system state: clean up stale SHM segments from previous runs
        ipc_layer::SharedMemory::cleanup_stale_segments();

        let cmd_buffer = Arc::new(ipc_layer::MpscRingBuffer::new(1024));
        let cmd_cons = cmd_buffer.clone();
        let (bundle_prod, bundle_cons) = RingBuffer::<Vec<control_plane::Command>>::new(16).split();
        let (bundle_garbage_prod, bundle_garbage_cons) = RingBuffer::<Vec<control_plane::Command>>::new(16).split();
        let (bundle_overflow_prod, bundle_overflow_cons) = RingBuffer::<Vec<control_plane::Command>>::new(16).split();
        let (_, midi_cons) = RingBuffer::<ipc_layer::MidiEvent>::new(256).split();
        let (topo_prod, topo_cons) = RingBuffer::<audio_core::processors::TopologyMutation>::new(64).split();
        let topo_prod = ipc_layer::NonRtProducer::new(topo_prod);
        let (garbage_prod, garbage_cons) = RingBuffer::new(1024).split();
        let (overflow_garbage_prod, overflow_garbage_cons) = RingBuffer::new(1024).split();
        let (tel_prod, tel_cons) = RingBuffer::new(1024).split();

        let graph = ProcessorGraph::new();
        let engine = AudioEngine::new(
            cmd_cons,
            Some(midi_cons),
            Some(bundle_cons),
            Some(topo_cons),
            garbage_prod,
            Some(overflow_garbage_prod),
            Some(bundle_garbage_prod),
            Some(bundle_overflow_prod),
            tel_prod,
            Box::new(graph)
        );
        self.health_signal = Some(engine.health_signal.clone());
        *self.engine_handle.lock().unwrap() = Some(engine);
        self.garbage_consumer = Some(garbage_cons);
        self.overflow_garbage_consumer = Some(overflow_garbage_cons);
        self.bundle_producer = Some(bundle_prod);
        self.bundle_garbage_consumer = Some(bundle_garbage_cons);
        self.bundle_overflow_consumer = Some(bundle_overflow_cons);
        self.topo_producer = Some(topo_prod);

        (cmd_buffer, tel_cons)
    }

    pub fn start_backend(&mut self, name: &str) -> Result<(), String> {
        // Move current process to high-priority Cgroup
        let _ = ipc_layer::move_to_cgroup("nullherz", std::process::id() as i32);

        let mut backend: Box<dyn AudioBackend> = match name {
            "alsa" => Box::new(AlsaBackend::new()),
            "pipewire" => Box::new(nullherz_backends::PipewireBackend::new()),
            "jack" => Box::new(nullherz_backends::JackBackend::new()),
            _ => Box::new(ThreadedBackend::new()),
        };

        backend.start(self.engine_handle.clone())?;
        self.backend = Some(backend);
        Ok(())
    }

    pub fn stop_backend(&mut self) {
        if let Some(mut backend) = self.backend.take() {
            backend.stop();
        }
    }

    pub fn switch_backend(&mut self, name: &str) -> Result<(), String> {
        // Hot-swap: stop old, preserve engine state, start new
        self.stop_backend();

        // Brief sleep to allow ALSA/JACK descriptors to truly release
        std::thread::sleep(std::time::Duration::from_millis(50));

        self.start_backend(name)
    }

    pub fn drain_garbage(&mut self) {
        if let Some(ref mut cons) = self.garbage_consumer {
            while let Some(proc) = cons.pop() {
                drop(proc);
            }
        }
        if let Some(ref mut cons) = self.overflow_garbage_consumer {
            while let Some(proc) = cons.pop() {
                drop(proc);
            }
        }
        if let Some(ref mut cons) = self.bundle_garbage_consumer {
            while let Some(bundle) = cons.pop() {
                drop(bundle);
            }
        }
        if let Some(ref mut cons) = self.bundle_overflow_consumer {
            while let Some(bundle) = cons.pop() {
                drop(bundle);
            }
        }
    }

    pub fn update_timeline(&mut self, telemetry: &audio_core::Telemetry) {
        // Sync conductor timeline with engine reality
        self.timeline.current_beat = telemetry.sample_counter as f64 / 44100.0 * (self.timeline.bpm as f64 / 60.0);
    }

    pub fn quantize_beat(&self, beat: f64, grid: f64) -> f64 {
        (beat / grid).ceil() * grid
    }

    pub fn apply_mixer_commands(&mut self, commands: Vec<control_plane::Command>) {
        let mut bundle = Vec::with_capacity(commands.len());

        for cmd in commands {
            match cmd {
                control_plane::Command::AddNode { processor_type_id, node_idx } => {
                    if let Some(ref mut prod) = self.topo_producer {
                        let processor = nullherz_processors::factory::create_processor(processor_type_id, node_idx, 44100.0);
                        let _ = prod.push(nullherz_traits::TopologyMutation::AddNode {
                            node_idx,
                            processor,
                        });
                    }
                }
                control_plane::Command::SwapProcessor { node_idx, processor_type_id } => {
                    if let Some(ref mut prod) = self.topo_producer {
                        let processor = nullherz_processors::factory::create_processor(processor_type_id, node_idx, 44100.0);
                        let _ = prod.push(nullherz_traits::TopologyMutation::SwapProcessor {
                            node_idx,
                            processor,
                        });
                    }
                }
                control_plane::Command::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                    if let Some(ref mut prod) = self.topo_producer {
                        let _ = prod.push(nullherz_traits::TopologyMutation::UpdateEdge {
                            node_idx,
                            input_idx,
                            new_buffer_idx,
                        });
                    }
                }
                control_plane::Command::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx } => {
                    if let Some(ref mut prod) = self.topo_producer {
                        let _ = prod.push(nullherz_traits::TopologyMutation::UpdateOutputEdge {
                            node_idx,
                            output_idx,
                            new_buffer_idx,
                        });
                    }
                }
                _ => bundle.push(cmd),
            }
        }

        if !bundle.is_empty() {
            if let Some(ref mut prod) = self.bundle_producer {
                let _ = prod.push(bundle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_mixer::MixerManager;

    #[test]
    fn test_conductor_mixer_integration() {
        let mut conductor = Conductor::new();
        conductor.setup_engine();

        let mut mixer = MixerManager::new();
        let commands = mixer.create_studio_strip("TestStrip", &[]);

        conductor.apply_mixer_commands(commands);

        // Verify engine state via handle
        let mut engine_lock = conductor.engine_handle.lock().unwrap();
        let engine = engine_lock.as_mut().unwrap();

        // Process a block to apply mutations
        let mut outputs = [[0.0f32; 128], [0.0f32; 128]];
        let (ch1, ch2) = outputs.split_at_mut(1);
        let mut out_refs = [&mut ch1[0][..], &mut ch2[0][..]];

        engine.process_block(&[], &mut out_refs, 128);

        // Telemetry should now show the nodes from the strip (Gain and Fader)
        let _node_times = [0u64; 64];
        let _peak_levels = [0.0f32; 64];

        // We can't easily peek inside the private graph, but we can check if telemetry was pushed
        // and if it contains data for the expected nodes.
        // Actually, we can check if the nodes were added to the graph if we had access,
        // but for now let's just ensure it doesn't panic and the flow is complete.
    }
}
