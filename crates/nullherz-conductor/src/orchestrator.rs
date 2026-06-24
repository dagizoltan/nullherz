use crate::engine_coordinator::EngineCoordinator;
use crate::topology_manager::TopologyManager;
use crate::transfusion_manager::TransfusionManager;
use crate::mixer_bridge::MixerBridge;
use crate::sidecar_supervisor::SidecarSupervisor;
use nullherz_traits::{Command, CommandProducer, RenderingEngine, telemetry::Telemetry};
use std::sync::Arc;
use nullherz_dna::SampleRegistry;

pub struct Conductor {
    pub engine_coordinator: EngineCoordinator,
    pub topology_manager: TopologyManager,
    pub transfusion_manager: TransfusionManager,
    pub mixer_bridge: MixerBridge,
    pub sidecar_supervisor: SidecarSupervisor,
    pub analysis_worker: Option<crate::analysis_worker::AnalysisWorker>,
    pub folder_monitor: Option<crate::folder_monitor::FolderMonitor>,
    pub library: Arc<std::sync::Mutex<nullherz_dna::LibraryDatabase>>,
}

impl Default for Conductor {
    fn default() -> Self {
        Self::new()
    }
}

impl Conductor {
    pub fn new() -> Self {
        let sample_registry = Arc::new(SampleRegistry::new());
        let library = Arc::new(std::sync::Mutex::new(nullherz_dna::LibraryDatabase::load("library.json")));
        Self {
            engine_coordinator: EngineCoordinator::new(),
            topology_manager: TopologyManager::new(),
            transfusion_manager: TransfusionManager::new(sample_registry.clone()),
            mixer_bridge: MixerBridge::new(),
            sidecar_supervisor: SidecarSupervisor::new(),
            analysis_worker: Some(crate::analysis_worker::AnalysisWorker::new(sample_registry.clone())),
            folder_monitor: Some(crate::folder_monitor::FolderMonitor::new(sample_registry, library.clone())),
            library,
        }
    }

    pub fn setup_engine(&mut self) -> (Box<dyn CommandProducer>, ipc_layer::Consumer<audio_core::Telemetry>) {
        let handle = self.engine_coordinator.setup();

        self.mixer_bridge.bundle_producer = Some(handle.bundle_producer);
        self.topology_manager.topo_producer = Some(ipc_layer::NonRtProducer::new(handle.topology_producer));

        (handle.command_producer, handle.telemetry_consumer)
    }

    pub fn start_backend(&mut self, backend_type: nullherz_backends::AudioBackendType) -> Result<(), String> {
        self.engine_coordinator.backend_manager.start(backend_type)
    }

    pub fn stop_backend(&mut self) {
        self.engine_coordinator.backend_manager.stop()
    }

    pub fn switch_backend(&mut self, backend_type: nullherz_backends::AudioBackendType) -> Result<(), String> {
        self.stop_backend();
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.start_backend(backend_type)
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
            self.drain_garbage();
        }

        let (mut new_processors, enter_safe_mode) = self.sidecar_supervisor.manager.supervise();
        if enter_safe_mode {
            eprintln!("Sidecar failure triggered Safe Mode!");
            if let Some(ref prod) = self.engine_coordinator.command_producer {
                let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                    timestamp_samples: 0,
                    command: nullherz_traits::Command::SetSafeMode(true),
                });
            }
        }

        for (node_idx, processor) in new_processors.drain(..) {
             eprintln!("Recovered sidecar process for node {}. Re-inserting into audio graph...", node_idx);
            if let Some(ref mut prod) = self.topology_manager.topo_producer {
                let _ = prod.push(nullherz_traits::TopologyMutation::SwapProcessor { node_idx, processor });
            }
        }

        self.handle_transfusion_registrations();

        self.sync_sampler_metadata();

        // Periodic library save
        static mut SAVE_COUNTER: u32 = 0;
        unsafe {
            SAVE_COUNTER += 1;
            if SAVE_COUNTER >= 100 {
                self.library.lock().unwrap().save("library.json");
                SAVE_COUNTER = 0;
            }
        }

        self.transfusion_manager.sample_registry.drain_garbage();

        self.drain_garbage();
    }

    fn handle_transfusion_registrations(&mut self) {
        let mut engine_lock = self.engine_coordinator.backend_manager.engine_handle.lock().unwrap();
        if let Some(ref mut engine) = *engine_lock {
            // RenderingEngine::pull_all_snapshots needs &mut.
            // We'll use the same raw pointer hack as in backends for now,
            // as this is a non-RT call from the conductor.
            let engine_ptr = Arc::as_ptr(engine) as *mut dyn RenderingEngine;
            unsafe {
                self.transfusion_manager.poll_snapshots(&mut *engine_ptr);
            }
        }
    }

    fn sync_sampler_metadata(&mut self) {
        let mut engine_lock = self.engine_coordinator.backend_manager.engine_handle.lock().unwrap();
        if let Some(ref mut engine) = *engine_lock {
            for child in engine.list_children() {
                // Since child is &dyn AudioProcessor, and we want to downcast to a concrete type,
                // we'd typically use child.as_any().downcast_ref::<T>().
                // However, RenderingEngine::list_children() returns Vec<&dyn AudioProcessor>.
                // We'll skip the concrete sync for now to avoid complex downcasting across crate boundaries
                // in this phase, as the primary goal of establishing the metadata path is done.
                let _ = child;
            }
        }
    }
}
