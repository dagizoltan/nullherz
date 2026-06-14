use std::sync::Arc;
use audio_core::{AudioEngine, ProcessorGraph};
use nullherz_processors::ProcessorRegistry;
use fx_runtime::SidecarManager;
use ipc_layer::RingBuffer;
use crate::timeline::Timeline;
use crate::backend::BackendManager;

pub struct Conductor {
    pub manager: SidecarManager,
    pub registry: ProcessorRegistry,
    pub timeline: Timeline,
    pub backend_manager: BackendManager,
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
            registry: ProcessorRegistry::new(),
            timeline: Timeline::default(),
            backend_manager: BackendManager::default(),
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
        *self.backend_manager.engine_handle.lock().unwrap() = Some(engine);
        self.garbage_consumer = Some(garbage_cons);
        self.overflow_garbage_consumer = Some(overflow_garbage_cons);
        self.bundle_producer = Some(bundle_prod);
        self.bundle_garbage_consumer = Some(bundle_garbage_cons);
        self.bundle_overflow_consumer = Some(bundle_overflow_cons);
        self.topo_producer = Some(topo_prod);

        (cmd_buffer, tel_cons)
    }

    pub fn start_backend(&mut self, backend_type: nullherz_backends::AudioBackendType) -> Result<(), String> {
        self.backend_manager.start(backend_type)
    }

    pub fn stop_backend(&mut self) {
        self.backend_manager.stop()
    }

    pub fn switch_backend(&mut self, backend_type: nullherz_backends::AudioBackendType) -> Result<(), String> {
        self.backend_manager.switch(backend_type)
    }

    pub fn drain_garbage(&mut self) {
        if let Some(ref mut cons) = self.garbage_consumer {
            while let Some(proc) = cons.pop() { drop(proc); }
        }
        if let Some(ref mut cons) = self.overflow_garbage_consumer {
            while let Some(proc) = cons.pop() { drop(proc); }
        }
        if let Some(ref mut cons) = self.bundle_garbage_consumer {
            while let Some(bundle) = cons.pop() { drop(bundle); }
        }
        if let Some(ref mut cons) = self.bundle_overflow_consumer {
            while let Some(bundle) = cons.pop() { drop(bundle); }
        }
    }

    pub fn update_timeline(&mut self, telemetry: &audio_core::Telemetry) {
        self.timeline.update(telemetry);
    }

    pub fn apply_mixer_commands(&mut self, commands: Vec<control_plane::Command>) {
        let mut bundle = Vec::with_capacity(commands.len());

        for cmd in commands {
            if self.handle_topology_command(&cmd) {
                continue;
            }
            bundle.push(cmd);
        }

        if !bundle.is_empty() {
            if let Some(ref mut prod) = self.bundle_producer {
                let _ = prod.push(bundle);
            }
        }
    }

    pub fn tick(&mut self) {
        // Check for engine health crisis
        let health_crisis = if let Some(ref signal) = self.health_signal {
            signal.swap(false, std::sync::atomic::Ordering::Relaxed)
        } else {
            false
        };

        if health_crisis {
            eprintln!("CRITICAL: Engine health crisis detected. Prioritizing resource recovery...");
            // Mitigation: Perform deep drain of garbage
            for _ in 0..100 { self.drain_garbage(); }
        }

        // Reap zombie sidecars and handle automated recovery
        let new_processors = self.manager.reap_zombies();
        for _processor in new_processors {
            eprintln!("Recovered sidecar process. Re-inserting into audio graph...");
            // Trigger recovery (e.g. via SwapProcessor command)
            // Implementation detail: we could send a command here if we had the producer
        }

        self.drain_garbage();
    }

    fn handle_topology_command(&mut self, cmd: &control_plane::Command) -> bool {
        let Some(ref mut prod) = self.topo_producer else { return false; };

        match *cmd {
            control_plane::Command::AddNode { processor_type_id, node_idx } => {
                if let Some(processor) = self.registry.create_by_id(processor_type_id, node_idx, 44100.0) {
                    let _ = prod.push(nullherz_traits::TopologyMutation::AddNode { node_idx, processor });
                    return true;
                }
            }
            control_plane::Command::SwapProcessor { node_idx, processor_type_id } => {
                if let Some(processor) = self.registry.create_by_id(processor_type_id, node_idx, 44100.0) {
                    let _ = prod.push(nullherz_traits::TopologyMutation::SwapProcessor { node_idx, processor });
                    return true;
                }
            }
            control_plane::Command::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                let _ = prod.push(nullherz_traits::TopologyMutation::UpdateEdge { node_idx, input_idx, new_buffer_idx });
                return true;
            }
            control_plane::Command::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx } => {
                let _ = prod.push(nullherz_traits::TopologyMutation::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx });
                return true;
            }
            _ => {}
        }
        false
    }
}
