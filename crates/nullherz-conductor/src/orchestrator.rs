use crate::engine_coordinator::EngineCoordinator;
use crate::topology_manager::TopologyManager;
use crate::transfusion_manager::TransfusionManager;
use crate::mixer_bridge::MixerBridge;
use crate::ipc_audio_bridge::IpcAudioBridge;
use crate::sidecar_supervisor::SidecarSupervisor;
use crate::midi_mapper::MidiMapper;
use crate::pattern_manager::PatternManager;
use crate::clip_orchestrator::ClipOrchestrator;
use crate::modulation_matrix::ModulationMatrix;
use nullherz_traits::{Command, telemetry::Telemetry};
use std::sync::{Arc, Mutex};
use nullherz_dna::{SampleRegistry, GeneticLibrary};

pub struct Conductor {
    pub engine_coordinator: EngineCoordinator,
    pub topology_manager: TopologyManager,
    pub sidecar_discovery: crate::discovery::SidecarDiscoveryService,
    pub transfusion_manager: TransfusionManager,
    pub mixer_bridge: MixerBridge,
    pub sidecar_supervisor: SidecarSupervisor,
    pub pattern_manager: PatternManager,
    pub clip_orchestrator: ClipOrchestrator,
    pub modulation_matrix: ModulationMatrix,
    pub audio_bridge: Arc<IpcAudioBridge>,
    pub midi_mapper: MidiMapper,
    pub midi_clock: crate::midi_clock::MidiClockTracker,
    pub analysis_worker: Option<crate::analysis_worker::AnalysisWorker>,
    pub folder_monitor: Option<crate::folder_monitor::FolderMonitor>,
    pub library: Arc<std::sync::Mutex<nullherz_dna::LibraryDatabase>>,
    pub mixer_manager: nullherz_mixer::MixerManager,
    pub midi_consumer: Option<ipc_layer::Consumer<nullherz_traits::MidiEvent>>,
    pub external_midi_consumer: Option<ipc_layer::IpcMidiConsumer>,
    midi_child: Option<std::process::Child>,
    midi_shm: Option<Arc<ipc_layer::SharedMemory>>,
    pub matchmaking_suggestions: Arc<Mutex<Vec<(u64, f32)>>>,
    pub active_master_deck: char,
    pub calibration_samples: u32,
    pub ptp_clock: Option<Arc<nullherz_traits::PtpClockProvider>>,
    last_autosave_secs: u64,
    pub last_genetic_evolve_secs: u64,
    pub focused_node_idx: Option<u32>,
}

impl Default for Conductor {
    fn default() -> Self {
        Self::new()
    }
}

impl Conductor {
    pub fn new() -> Self {
        Self::with_library_path("library.redb")
    }

    pub fn with_library_path(path: &str) -> Self {
        let sample_registry = Arc::new(SampleRegistry::new());
        let library = match nullherz_dna::LibraryDatabase::load(path) {
            Ok(db) => Arc::new(std::sync::Mutex::new(db)),
            Err(_) => {
                // If it's already open (e.g. in tests), we load it with a unique path
                // to avoid concurrent database access/locking collisions in tests.
                static FALLBACK_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                let count = FALLBACK_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let fallback_path = format!("fallback_{}_{}.redb", std::process::id(), count);
                Arc::new(std::sync::Mutex::new(nullherz_dna::LibraryDatabase::load(&fallback_path).unwrap()))
            }
        };
        let sidecar_discovery = crate::discovery::SidecarDiscoveryService::new("plugins").with_library(library.clone());
        let dna_discovery = sidecar_discovery.dna_discovery.clone();

        let mut transfusion_manager = TransfusionManager::new(sample_registry.clone());
        transfusion_manager.discovery_service = Some(dna_discovery);
        transfusion_manager = transfusion_manager.with_library(library.clone());

        Self {
            engine_coordinator: EngineCoordinator::new(),
            topology_manager: TopologyManager::new(),
            transfusion_manager,
            mixer_bridge: MixerBridge::new(),
            sidecar_supervisor: SidecarSupervisor::new(),
            pattern_manager: PatternManager::new(),
            clip_orchestrator: ClipOrchestrator::new(),
            modulation_matrix: ModulationMatrix::new(),
            audio_bridge: Arc::new(IpcAudioBridge::new()),
            sidecar_discovery,
            midi_mapper: MidiMapper::new(),
            midi_clock: crate::midi_clock::MidiClockTracker::new(),
            analysis_worker: Some(crate::analysis_worker::AnalysisWorker::new(sample_registry.clone()).with_library(library.clone())),
            folder_monitor: Some(crate::folder_monitor::FolderMonitor::new(sample_registry, library.clone())),
            library,
            mixer_manager: nullherz_mixer::MixerManager::new(),
            midi_consumer: None,
            external_midi_consumer: None,
            midi_child: None,
            midi_shm: None,
            matchmaking_suggestions: Arc::new(Mutex::new(Vec::new())),
            active_master_deck: 'A',
            calibration_samples: 0,
            ptp_clock: None,
            last_autosave_secs: 0,
            last_genetic_evolve_secs: 0,
            focused_node_idx: None,
        }
    }

    pub fn setup_engine(&mut self) -> crate::EngineContext {
        // Initialize PTP Clock if on Linux
        #[cfg(target_os = "linux")]
        if let Ok(clock) = nullherz_traits::PtpClockProvider::new("eth0") {
            let clock_arc = Arc::new(clock);
            self.ptp_clock = Some(clock_arc.clone());

            // Start PTP Engine
            if let Ok(ptp) = crate::ptp_engine::PtpEngine::new(clock_arc as Arc<dyn nullherz_traits::ClockProvider>, 319, false) {
                std::thread::spawn(move || ptp.run_loop());
            }
        }

        let handle = self.engine_coordinator.setup();

        self.mixer_bridge.bundle_producer = Some(handle.bundle_producer);
        self.mixer_bridge.bundle_pool = handle.bundle_garbage_consumer;
        self.topology_manager.topo_producer = Some(ipc_layer::NonRtProducer::new(handle.topology_producer));

        // Setup MIDI Bridge SHM
        let shm_name = "nullherz_midi_bridge";
        if let Ok(shm) = ipc_layer::SharedMemory::create(shm_name, 65536) {
            unsafe { ipc_layer::ShmRingBuffer::<nullherz_traits::MidiEvent>::init(shm.ptr(), 1024); }
            let rb = shm.ptr() as *const ipc_layer::ShmRingBuffer<nullherz_traits::MidiEvent>;

            let shm_arc = Arc::new(shm);
            self.midi_shm = Some(shm_arc.clone());
            self.external_midi_consumer = Some(ipc_layer::IpcMidiConsumer {
                buffer: shm_arc,
                rb,
            });
        }

        // Setup Remote Sidecar Listener (Stage 2 Distributed DSP)
        if let Ok(_handle) = tokio::runtime::Handle::try_current() {
            let remote_manager = self.sidecar_supervisor.remote_manager.clone();
            let audio_bridge = self.audio_bridge.clone();
            tokio::spawn(async move {
                let _ = crate::sidecar_supervisor::SidecarSupervisor::listen_for_remote_sidecars(remote_manager, audio_bridge, "0.0.0.0:9000").await;
            });

            // Start UDP Discovery Beacon (Conductor identifying itself)
            let discovery = crate::discovery::DiscoveryBeacon::new(9000, "Conductor");
            discovery.start_broadcast();

            // Start UDP Discovery Listener (Conductor finding sidecars)
            let remote_manager = self.sidecar_supervisor.remote_manager.clone();
            let audio_bridge = self.audio_bridge.clone();
            tokio::spawn(async move {
                let _ = crate::sidecar_supervisor::SidecarSupervisor::start_discovery_listener(remote_manager, audio_bridge, 9001).await;
            });

            // Start UDP Return Listener (Type 6)
            let audio_bridge = self.audio_bridge.clone();
            tokio::spawn(async move {
                let _ = crate::sidecar_supervisor::SidecarSupervisor::start_udp_return_listener(audio_bridge, 9002).await;
            });

            // Start Federated DNA Server (TCP pull)
            let lib = self.library.clone();
            let _ = nullherz_dna::DnaServer::start(lib, 9003, None);
        }

        crate::EngineContext {
            command_producer: handle.command_producer,
            telemetry_consumer: handle.telemetry_consumer,
            midi_producer: handle.midi_producer,
        }
    }

    pub fn start_midi_bridge(&mut self, binary_path: &str, port_filter: Option<&str>) {
        if self.midi_child.is_some() { return; }
        let mut cmd = std::process::Command::new(binary_path);
        cmd.arg("--shm").arg("nullherz_midi_bridge");
        if let Some(f) = port_filter { cmd.arg("--port").arg(f); }

        if let Ok(child) = cmd.spawn() {
            self.midi_child = Some(child);
            println!("MIDI Bridge process spawned (PID: {})", self.midi_child.as_ref().unwrap().id());
        }
    }

    pub fn set_midi_consumer(&mut self, consumer: ipc_layer::Consumer<nullherz_traits::MidiEvent>) {
        self.midi_consumer = Some(consumer);
    }

    pub fn start_backend(&mut self, backend_type: nullherz_traits::AudioBackendType) -> Result<(), String> {
        self.engine_coordinator.backend_manager.start(backend_type)
    }

    pub fn stop_backend(&mut self) {
        self.engine_coordinator.backend_manager.stop()
    }

    pub fn switch_backend(&mut self, backend_type: nullherz_traits::AudioBackendType) -> Result<(), String> {
        self.stop_backend();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let res = self.start_backend(backend_type);
        if res.is_ok() {
            let _ = self.update_system_config(Some(backend_type), None, None);
        }
        res
    }

    pub fn load_system_config(&mut self) -> std::io::Result<()> {
        let path = "system_config.json";
        if std::path::Path::new(path).exists() {
            let content = std::fs::read_to_string(path)?;
            if let Ok(config) = serde_json::from_str::<crate::persistence::SystemConfig>(&content) {
                self.calibration_samples = config.calibration_samples;
            }
        }
        Ok(())
    }

    pub fn update_system_config(&mut self, backend_type: Option<nullherz_traits::AudioBackendType>, midi_ports: Option<Vec<String>>, calibration: Option<u32>) -> std::io::Result<()> {
        let path = "system_config.json";
        let mut config = if std::path::Path::new(path).exists() {
            let content = std::fs::read_to_string(path)?;
            serde_json::from_str::<crate::persistence::SystemConfig>(&content).unwrap_or(crate::persistence::SystemConfig {
                audio_backend: "Mock".to_string(),
                midi_ports: vec![],
                sample_rate: 44100,
                block_size: 256,
                calibration_samples: 0,
            })
        } else {
            crate::persistence::SystemConfig {
                audio_backend: "Mock".to_string(),
                midi_ports: vec![],
                sample_rate: 44100,
                block_size: 256,
                calibration_samples: 0,
            }
        };

        if let Some(bt) = backend_type {
            config.audio_backend = format!("{:?}", bt);
        }
        if let Some(ports) = midi_ports {
            config.midi_ports = ports;
        }
        if let Some(c) = calibration {
            config.calibration_samples = c;
            self.calibration_samples = c;
        }

        let json = serde_json::to_string_pretty(&config).map_err(|e| std::io::Error::other(e))?;
        std::fs::write(path, json)
    }

    pub fn drain_garbage(&mut self) {
        self.engine_coordinator.drain_garbage();
    }

    fn process_distributed_audio(&mut self) {
        let topo = &self.topology_manager.current_topology;
        for node_idx in 0..topo.node_count {
            if let Some(target) = topo.node_assignments.get(&(node_idx as u32)) {
                if target != "local" {
                    let mut blocks = Vec::with_capacity(4);
                    while let Some(block) = self.audio_bridge.pop_block(node_idx as u32) {
                        blocks.push(block);
                    }

                    if !blocks.is_empty() {
                        let remote_manager = self.sidecar_supervisor.remote_manager.clone();
                        let node_idx_u32 = node_idx as u32;
                        tokio::spawn(async move {
                            let mut manager = remote_manager.lock().await;
                            for block in blocks {
                                let _ = manager.send_audio_block(node_idx_u32, block).await;
                            }
                        });
                    }
                }
            }
        }
        self.audio_bridge.process_return_queues();
    }

    fn process_evolutionary_breeding(&mut self, now: u64) {
        if now % 10 == 0 && self.mixer_bridge.timeline.last_breeding_secs != now {
             self.mixer_bridge.timeline.last_breeding_secs = now;
             if let Some(ref breeder) = self.transfusion_manager.evolutionary_breeder {
                 breeder.run_breeding_cycle();
             }
        }
    }

    pub fn trigger_matchmaking_suggestions(&mut self) {
        self.update_matchmaking_suggestions(0);
    }

    fn update_matchmaking_suggestions(&mut self, now: u64) {
        self.mixer_bridge.timeline.last_matchmaking_secs = now;
        let lib = self.library.clone();
        let suggestions = self.matchmaking_suggestions.clone();

        // identify master track DNA using active_master_deck
        let master_sampler_id = self.mixer_manager.deck_mappings.get(&self.active_master_deck).map(|d| d.sampler_id);

        if let Some(sampler_node_idx) = master_sampler_id {
            // Resolve the resource_id (sample_id) currently loaded in the master sampler
            let mut current_sample_id = None;
            {
                if let Ok(engine_lock) = self.engine_coordinator.backend_manager.engine_handle.lock() {
                    if let Some(ref engine) = *engine_lock {
                        current_sample_id = engine.list_children().iter()
                            .find(|c| c.metadata().map(|m| m.processor_id as u32) == Some(sampler_node_idx))
                            .and_then(|c| c.resource_id());
                    }
                }
            }

            if let Some(id) = current_sample_id {
                tokio::spawn(async move {
                    if let Ok(lib_lock) = lib.lock() {
                        if let Ok(Some(track)) = lib_lock.get_track(id) {
                            if let Ok(matches) = nullherz_dna::Matchmaker::find_best_matches(&lib_lock, &track.metadata.dna, 3) {
                                if let Ok(mut sugg_lock) = suggestions.lock() {
                                    *sugg_lock = matches;
                                }
                            }
                        }
                    }
                });
            }
        }
    }

    pub fn update_timeline(&mut self, telemetry: &mut Telemetry) {
        crate::telemetry_service::TelemetryService::update_timeline(self, telemetry);
    }

    pub fn apply_mixer_commands(&mut self, commands: Vec<Command>) {
        crate::command_handler::CommandHandler::apply_mixer_commands(self, commands);
    }

    pub fn handle_midi_events(&mut self, events: Vec<nullherz_traits::MidiEvent>) {
        for event in events {
            // Handle System Real-time (Clock, Start, Stop)
            if event.status >= 0xF8 {
                if let Some(new_bpm) = self.midi_clock.handle_event(event.status) {
                    self.mixer_bridge.timeline.bpm = new_bpm;
                    // Broadcast new BPM to engine
                    if let Some(ref prod) = self.engine_coordinator.command_producer {
                         let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                             timestamp_samples: 0,
                             command: nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetBpm(new_bpm)),
                         });
                    }
                }

                if event.status == 0xFA || event.status == 0xFB {
                     if let Some(ref prod) = self.engine_coordinator.command_producer {
                        let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                            timestamp_samples: 0,
                            command: nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Play),
                        });
                    }
                } else if event.status == 0xFC {
                    if let Some(ref prod) = self.engine_coordinator.command_producer {
                        let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                            timestamp_samples: 0,
                            command: nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop),
                        });
                    }
                }
            }

            let mapped_commands = self.midi_mapper.translate(&event, &self.mixer_manager.node_names, self.focused_node_idx);
            if !mapped_commands.is_empty() {
                self.apply_mixer_commands(mapped_commands);
            }
        }
    }

    pub fn tick(&mut self) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);

        // 0. Handle Background Auto-Save (Every 60 seconds)
        if now % 60 == 0 && self.last_autosave_secs != now {
            self.last_autosave_secs = now;
            let state = self.capture_state();
            tokio::spawn(async move {
                let _ = state.save_to_file("autosave.json");
                let _ = state.save_to_rkyv("autosave.rkyv");
                println!("Conductor: Background Auto-Save complete.");
            });
        }

        // Drain Local MIDI Consumer
        if let Some(ref mut consumer) = self.midi_consumer {
            let mut events = Vec::new();
            while let Some(event) = consumer.pop() {
                events.push(event);
            }
            if !events.is_empty() {
                self.handle_midi_events(events);
            }
        }

        // Drain External MIDI Consumer (Sidecar Bridge)
        if let Some(ref mut consumer) = self.external_midi_consumer {
            let mut events = Vec::new();
            while let Some(event) = consumer.pop() {
                events.push(event);
            }
            if !events.is_empty() {
                self.handle_midi_events(events);
            }
        }

        // Update Pattern Orchestration
        let arrangement_commands = self.pattern_manager.tick(self.mixer_bridge.timeline.current_beat);
        if !arrangement_commands.is_empty() {
            self.apply_mixer_commands(arrangement_commands);
        }

        let clip_commands = self.clip_orchestrator.tick(self.mixer_bridge.timeline.current_beat);
        if !clip_commands.is_empty() {
            self.apply_mixer_commands(clip_commands);
        }

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
                    command: nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetSafeMode(true)),
                });
            }
        }

        for (node_idx, processor) in new_processors.drain(..) {
             eprintln!("Recovered sidecar process for node {}. Re-inserting into audio graph...", node_idx);
            if let Some(ref mut prod) = self.topology_manager.topo_producer {
                let _ = prod.push(nullherz_traits::TopologyMutation::SwapProcessor { node_idx, processor });
            }
        }

        let remote_commands = self.sidecar_supervisor.supervise(&mut self.topology_manager);
        for ts_cmd in remote_commands {
            if let Some(ref prod) = self.engine_coordinator.command_producer {
                let _ = prod.push_command(ts_cmd);
            }
        }

        self.process_distributed_audio();

        // Proactive Matchmaking Suggestions (Stage 6)
        if now % 15 == 0 && self.mixer_bridge.timeline.last_matchmaking_secs != now {
            self.update_matchmaking_suggestions(now);
        }

        self.process_evolutionary_breeding(now);

        // Genetic Pattern Evolution (Every 8 seconds)
        if now % 8 == 0 && self.last_genetic_evolve_secs != now {
            self.last_genetic_evolve_secs = now;
            let current_beat = self.mixer_bridge.timeline.current_beat;
            // Example: trigger evolution on node 0, track 0 if sampler is active
            let node_idx = 0;
            let track_idx = 0;
            let cmd = nullherz_traits::PerformanceCommand::EvolvePattern { node_idx, track_idx, mutation_strength: 0.2 };
            self.apply_mixer_commands(vec![nullherz_traits::Command::Performance(cmd)]);
        }

        self.handle_transfusion_registrations();

        self.sync_sampler_metadata();

        self.transfusion_manager.sample_registry.drain_garbage();

        self.drain_garbage();
    }

    fn handle_transfusion_registrations(&mut self) {
        if let Ok(engine_lock) = self.engine_coordinator.backend_manager.engine_handle.lock() {
            if let Some(ref engine) = *engine_lock {
                self.transfusion_manager.poll_snapshots(engine.as_ref());
            }
        }
    }

    fn sync_sampler_metadata(&mut self) {
        if let Ok(engine_lock) = self.engine_coordinator.backend_manager.engine_handle.lock() {
            if let Some(ref engine) = *engine_lock {
                for child in engine.list_children() {
                    if let Some(id) = child.resource_id() {
                        // Reconcile with LibraryDatabase for persistent metadata updates
                        let lib_lock = if let Ok(l) = self.library.lock() { l } else { continue; };
                        if let Ok(Some(track)) = lib_lock.get_track(id) {
                            if let Some(m) = child.metadata() {
                                if let Some(ref mut prod) = self.topology_manager.topo_producer {
                                    let _ = prod.push(nullherz_traits::TopologyMutation::UpdateMetadata {
                                        node_idx: m.processor_id as u32,
                                        metadata: Arc::new(track.metadata),
                                    });
                                }
                            }
                        } else if let Some(sample) = self.transfusion_manager.sample_registry.get(id) {
                            // Fallback to transient registry if not in persistent library
                            if let Some(m) = child.metadata() {
                                if let Some(ref mut prod) = self.topology_manager.topo_producer {
                                    let _ = prod.push(nullherz_traits::TopologyMutation::UpdateMetadata {
                                        node_idx: m.processor_id as u32,
                                        metadata: Arc::new(sample.metadata),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn capture_state(&self) -> crate::persistence::ProjectState {
        crate::persistence::ProjectState::capture(self)
    }

    pub fn save_project(&self, path: &str) -> std::io::Result<()> {
        let state = self.capture_state();
        // Standardized: Prioritize .rkyv for zero-copy performance unless JSON explicitly requested
        if path.ends_with(".json") {
            state.save_to_file(path)
        } else {
            let rkyv_path = if path.ends_with(".rkyv") { path.to_string() } else { format!("{}.rkyv", path) };
            state.save_to_rkyv(&rkyv_path)
        }
    }

    pub fn load_project(&mut self, path: &str) -> std::io::Result<()> {
        let state = if path.ends_with(".json") {
            crate::persistence::ProjectState::load_from_file(path)?
        } else {
            let rkyv_path = if path.ends_with(".rkyv") { path.to_string() } else { format!("{}.rkyv", path) };
            crate::persistence::ProjectState::load_from_rkyv(&rkyv_path)?
        };
        self.apply_state(state);
        Ok(())
    }

    pub fn apply_state(&mut self, state: crate::persistence::ProjectState) {
        let _ = state.apply(self);
    }
}
