use crate::engine_coordinator::EngineCoordinator;
use crate::topology_manager::TopologyManager;
use crate::mixer_bridge::MixerBridge;
use crate::sidecar_supervisor::SidecarSupervisor;
use nullherz_traits::{Command, CommandProducer, telemetry::Telemetry};
use std::sync::Arc;
use nullherz_dna::SampleRegistry;

pub struct Conductor {
    pub engine_coordinator: EngineCoordinator,
    pub topology_manager: TopologyManager,
    pub mixer_bridge: MixerBridge,
    pub sample_registry: Arc<SampleRegistry>,
    pub sidecar_supervisor: SidecarSupervisor,
}

impl Default for Conductor {
    fn default() -> Self {
        Self::new()
    }
}

impl Conductor {
    pub fn new() -> Self {
        Self {
            engine_coordinator: EngineCoordinator::new(),
            topology_manager: TopologyManager::new(),
            mixer_bridge: MixerBridge::new(),
            sample_registry: Arc::new(SampleRegistry::new()),
            sidecar_supervisor: SidecarSupervisor::new(),
        }
    }

    pub fn setup_engine(&mut self) -> (Box<dyn CommandProducer>, ipc_layer::Consumer<audio_core::Telemetry>) {
        let handle = self.engine_coordinator.setup();

        self.mixer_bridge.bundle_producer = Some(handle.bundle_producer);
        self.topology_manager.topo_producer = Some(ipc_layer::NonRtProducer::new(handle.topology_producer));
        self.engine_coordinator.garbage_consumer = Some(handle.garbage_consumer);

        (handle.command_producer, handle.telemetry_consumer)
    }

    pub fn start_backend(&mut self, backend_type: nullherz_backends::AudioBackendType) -> Result<(), String> {
        self.engine_coordinator.backend_manager.start(backend_type)
    }

    pub fn stop_backend(&mut self) {
        self.engine_coordinator.backend_manager.stop()
    }

    pub fn switch_backend(&mut self, backend_type: nullherz_backends::AudioBackendType) -> Result<(), String> {
        self.engine_coordinator.backend_manager.switch(backend_type)
    }

    pub fn drain_garbage(&mut self) {
        self.engine_coordinator.drain_garbage();
    }

    pub fn update_timeline(&mut self, telemetry: &Telemetry) {
        self.mixer_bridge.update_timeline(telemetry);
    }

    pub fn apply_mixer_commands(&mut self, commands: Vec<Command>) {
        self.mixer_bridge.apply_mixer_commands(commands, &mut self.topology_manager);
    }

    pub fn tick(&mut self) {
        if self.engine_coordinator.check_health() {
            eprintln!("CRITICAL: Engine health crisis detected. Prioritizing resource recovery...");
            for _ in 0..100 { self.drain_garbage(); }
        }

        self.sidecar_supervisor.supervise(&mut self.topology_manager);

        self.handle_transfusion_registrations();

        self.drain_garbage();
    }

    fn handle_transfusion_registrations(&mut self) {
        let mut engine_lock = self.engine_coordinator.backend_manager.engine_handle.lock().unwrap();
        if let Some(ref mut engine) = *engine_lock {
            let mut snapshots = Vec::new();
            engine.pull_all_snapshots(&mut snapshots);

            for (sample_id, snapshot) in snapshots {
                self.sample_registry.register(sample_id, snapshot);
                eprintln!("Registered new transfusion source: ID={}", sample_id);
            }
        }
    }
}
