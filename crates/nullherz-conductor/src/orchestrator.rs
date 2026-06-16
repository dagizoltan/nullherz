use audio_core::engine::builder::EngineBuilder;
use nullherz_processors::ProcessorRegistry;
use fx_runtime::SidecarManager;
use crate::timeline::Timeline;
use crate::backend::BackendManager;

pub struct Conductor {
    pub manager: SidecarManager,
    pub registry: ProcessorRegistry,
    pub timeline: Timeline,
    pub backend_manager: BackendManager,
    garbage_consumer: Option<ipc_layer::Consumer<Box<dyn audio_core::AudioProcessor>>>,
    overflow_garbage_consumer: Option<ipc_layer::Consumer<Box<dyn audio_core::AudioProcessor>>>,
    pub bundle_producer: Option<ipc_layer::Producer<Vec<nullherz_traits::Command>>>,
    bundle_garbage_consumer: Option<ipc_layer::Consumer<Vec<nullherz_traits::Command>>>,
    bundle_overflow_consumer: Option<ipc_layer::Consumer<Vec<nullherz_traits::Command>>>,
    pub topo_producer: Option<ipc_layer::NonRtProducer<audio_core::processors::TopologyMutation>>,
    pub health_signal: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub command_producer: Option<Box<dyn nullherz_traits::CommandProducer>>,
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
            command_producer: None,
        }
    }

    pub fn setup_engine(&mut self) -> (Box<dyn nullherz_traits::CommandProducer>, ipc_layer::Consumer<audio_core::Telemetry>) {
        ipc_layer::SharedMemory::cleanup_stale_segments();

        let (engine, handle) = EngineBuilder::new()
            .with_command_buffer_size(1024)
            .build();

        self.health_signal = Some(handle.health_signal.clone());
        self.command_producer = Some(handle.command_producer.clone());
        *self.backend_manager.engine_handle.lock().unwrap() = Some(engine);
        self.garbage_consumer = Some(handle.garbage_consumer);
        self.bundle_producer = Some(handle.bundle_producer);
        self.topo_producer = Some(ipc_layer::NonRtProducer::new(handle.topology_producer));

        (handle.command_producer, handle.telemetry_consumer)
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

    pub fn apply_mixer_commands(&mut self, commands: Vec<nullherz_traits::Command>) {
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
        for processor in new_processors {
            eprintln!("Recovered sidecar process. Re-inserting into audio graph...");
            if let Some(ref mut prod) = self.topo_producer {
                // In a real system, we'd need to know which node_idx this sidecar belongs to.
                // For now, we use a placeholder node_idx or just log it.
                let _ = prod.push(nullherz_traits::TopologyMutation::SwapProcessor { node_idx: 0, processor });
            }
        }

        self.drain_garbage();
    }

    fn handle_topology_command(&mut self, cmd: &nullherz_traits::Command) -> bool {
        let Some(ref mut prod) = self.topo_producer else { return false; };

        match *cmd {
            nullherz_traits::Command::AddNode { processor_type_id, node_idx } => {
                if let Some(processor) = self.registry.create_by_id(processor_type_id.0, node_idx, 44100.0) {
                    let _ = prod.push(nullherz_traits::TopologyMutation::AddNode { node_idx, processor });
                    return true;
                }
            }
            nullherz_traits::Command::SwapProcessor { node_idx, processor_type_id } => {
                if let Some(processor) = self.registry.create_by_id(processor_type_id.0, node_idx, 44100.0) {
                    let _ = prod.push(nullherz_traits::TopologyMutation::SwapProcessor { node_idx, processor });
                    return true;
                }
            }
            nullherz_traits::Command::UpdateEdge { node_idx, input_idx, new_buffer_idx } => {
                let _ = prod.push(nullherz_traits::TopologyMutation::UpdateEdge { node_idx, input_idx, new_buffer_idx });
                return true;
            }
            nullherz_traits::Command::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx } => {
                let _ = prod.push(nullherz_traits::TopologyMutation::UpdateOutputEdge { node_idx, output_idx, new_buffer_idx });
                return true;
            }
            _ => {}
        }
        false
    }
}
