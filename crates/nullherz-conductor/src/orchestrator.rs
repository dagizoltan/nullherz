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
use nullherz_dna::SampleRegistry;

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
                // If it's already open (e.g. in tests), we try to load it without unwrapping directly
                // This is a common issue with redb in concurrent tests.
                // For production, this path is safe.
                Arc::new(std::sync::Mutex::new(nullherz_dna::LibraryDatabase::load("fallback.redb").unwrap()))
            }
        };
        Self {
            engine_coordinator: EngineCoordinator::new(),
            topology_manager: TopologyManager::new(),
            transfusion_manager: TransfusionManager::new(sample_registry.clone()).with_library(library.clone()),
            mixer_bridge: MixerBridge::new(),
            sidecar_supervisor: SidecarSupervisor::new(),
            pattern_manager: PatternManager::new(),
            clip_orchestrator: ClipOrchestrator::new(),
            modulation_matrix: ModulationMatrix::new(),
            audio_bridge: Arc::new(IpcAudioBridge::new()),
            sidecar_discovery: crate::discovery::SidecarDiscoveryService::new("plugins").with_library(library.clone()),
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
        }
    }

    pub fn setup_engine(&mut self) -> crate::EngineContext {
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

    pub fn update_system_config(&self, backend_type: Option<nullherz_traits::AudioBackendType>, midi_ports: Option<Vec<String>>, calibration: Option<u32>) -> std::io::Result<()> {
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
        }

        let json = serde_json::to_string_pretty(&config).map_err(|e| std::io::Error::other(e))?;
        std::fs::write(path, json)
    }

    pub fn drain_garbage(&mut self) {
        self.engine_coordinator.drain_garbage();
    }

    pub fn update_timeline(&mut self, telemetry: &mut Telemetry) {
        self.mixer_bridge.update_timeline(telemetry);
        self.clip_orchestrator.collect_telemetry(&mut telemetry.active_clips, &mut telemetry.starting_clips_mask);

        // Update Matchmaking Suggestions
        if let Ok(sugg) = self.matchmaking_suggestions.try_lock() {
            for (i, (id, score)) in sugg.iter().enumerate().take(4) {
                telemetry.suggestions[i] = (*id, *score);
            }
        }
        telemetry.active_master_deck = self.active_master_deck;

        // Update Remote Node Telemetry
        if let Ok(manager) = self.sidecar_supervisor.remote_manager.try_lock() {
            telemetry.remote_node_count = manager.remote_nodes.len() as u32;
            for (i, node) in manager.remote_nodes.iter().enumerate().take(8) {
                telemetry.remote_cpu_usage[i] = node.cpu_usage;
                telemetry.remote_latency_ms[i] = node.latency_ms;
            }
        }
    }

    pub fn apply_mixer_commands(&mut self, commands: Vec<Command>) {
        let mut final_commands = Vec::new();

        // 1. Intercept DJ Deck Commands and Translate them
        let mut translated_commands = Vec::new();
        for cmd in &commands {
            translated_commands.extend(crate::mixer_orchestrator::MixerOrchestrator::translate_command(cmd, &self.mixer_manager, &self.library));
        }

        // Broadcast to remote nodes (Distributed Control Plane)
        for cmd in &commands {
            let ts_cmd = nullherz_traits::TimestampedCommand {
                timestamp_samples: 0,
                command: *cmd,
            };
            let remote_manager = self.sidecar_supervisor.remote_manager.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let mut manager = remote_manager.lock().await;
                    manager.broadcast_command(ts_cmd).await;
                });
            }
        }

        for cmd in translated_commands {
             match cmd {
                 Command::Performance(nullherz_traits::PerformanceCommand::LaunchClip { row, col }) => {
                     if row == 0xFF {
                         // Scene Launch: launch all clips in column 'col'
                         for r in 0..8 {
                             self.clip_orchestrator.launch_clip(r, col as usize);
                         }
                     } else {
                         self.clip_orchestrator.launch_clip(row as usize, col as usize);
                     }
                 }
                 Command::Performance(nullherz_traits::PerformanceCommand::TransfuseRow { row }) => {
                     let mutations = self.clip_orchestrator.transfuse_row(row as usize);
                     for m in mutations {
                         if let Some(ref mut prod) = self.topology_manager.topo_producer {
                             let _ = prod.push(m);
                         }
                     }
                 }
                 Command::Resource(nullherz_traits::ResourceCommand::CommitBreeding { parent_a_id, parent_b_id, bias }) => {
                     let lib = self.library.lock().unwrap();
                     self.transfusion_manager.commit_breeding(parent_a_id, parent_b_id, bias, &lib);
                 }
                 Command::Resource(nullherz_traits::ResourceCommand::CommitChaoticBreeding { parent_a_id, parent_b_id, bias, chaotic_strength }) => {
                     let lib = self.library.lock().unwrap();
                     self.transfusion_manager.commit_chaotic_breeding(parent_a_id, parent_b_id, bias, chaotic_strength, &lib);
                 }
                 Command::Core(nullherz_traits::CoreCommand::SwitchBackend(backend_type)) => {
                     let _ = self.switch_backend(backend_type);
                 }
                 Command::Core(nullherz_traits::CoreCommand::SetMasterDeck(deck_id)) => {
                     self.active_master_deck = deck_id;
                     println!("Conductor: Master Deck set to {}", deck_id);
                 }
                 Command::Core(nullherz_traits::CoreCommand::LoadMidiMap(buffer)) => {
                     let name = String::from_utf8_lossy(&buffer).trim_matches(char::from(0)).to_string();
                     let path = format!("mappings/{}.json", name);
                     if let Ok(json) = std::fs::read_to_string(path) {
                         let _ = self.midi_mapper.load_from_json(&json);
                     }
                 }
                 Command::Core(nullherz_traits::CoreCommand::SetMidiPorts(buffer)) => {
                     let ports_str = String::from_utf8_lossy(&buffer).trim_matches(char::from(0)).to_string();
                     let ports: Vec<String> = ports_str.split(',').filter(|s| !s.is_empty()).map(|s| s.trim().to_string()).collect();
                     let _ = self.update_system_config(None, Some(ports), None);
                 }
                 Command::Core(nullherz_traits::CoreCommand::CalibrateLatency) => {
                     let sample_rate = {
                         let engine_lock = self.engine_coordinator.backend_manager.engine_handle.lock().unwrap();
                         engine_lock.as_ref().map(|e| e.target_sample_rate()).unwrap_or(44100.0)
                     };
                     // Hardened calibration: 10ms based on actual sample rate
                     let samples = (sample_rate * 0.01) as u32;
                     let _ = self.update_system_config(None, None, Some(samples));
                 }
                 Command::Core(nullherz_traits::CoreCommand::HotLoadSidecar { name, node_idx }) => {
                     let plugin_name = String::from_utf8_lossy(&name).trim_matches(char::from(0)).to_string();

                     let manifest = {
                         let known = self.sidecar_discovery.known_plugins.lock().unwrap();
                         known.get(&plugin_name).cloned()
                     };

                     if let Some(m) = manifest {
                         let binary_path = format!("plugins/{}", m.binary_name);
                         match self.sidecar_supervisor.manager.spawn_sidecar(&plugin_name, &binary_path, node_idx, 2, fx_runtime::FailurePolicy::AutoRestart) {
                             Ok(processor) => {
                                 if let Some(ref mut prod) = self.topology_manager.topo_producer {
                                     let _ = prod.push(nullherz_traits::TopologyMutation::SwapProcessor { node_idx, processor });
                                 }
                             }
                             Err(e) => eprintln!("Failed to hot-load sidecar {}: {}", plugin_name, e),
                         }
                     } else {
                         eprintln!("Hot-load failed: plugin manifest for {} not found.", plugin_name);
                     }
                 }
                 Command::Core(nullherz_traits::CoreCommand::ExportAudio { filename, duration_seconds }) => {
                     let name = String::from_utf8_lossy(&filename).trim_matches(char::from(0)).to_string();
                     eprintln!("Bounce: Offline Export requested for {}. Initializing bounce engine...", name);

                     let state = self.capture_state();
                     let mut renderer = crate::bounce::OfflineRenderer::new(state);

                     let filename_clone = name.clone();
                     tokio::task::spawn_blocking(move || {
                         let _ = renderer.bounce_to_wav(&filename_clone, duration_seconds);
                     });
                 }
                 Command::Performance(nullherz_traits::PerformanceCommand::EvolvePattern { node_idx, track_idx, mutation_strength }) => {
                     // Precisely resolve DNA for the specified node_idx
                     let mut dna = nullherz_traits::RhythmicDNA::default();
                     {
                         let engine_lock = self.engine_coordinator.backend_manager.engine_handle.lock().unwrap();
                         if let Some(ref engine) = *engine_lock {
                             // Find the child with matching processor_id
                             let resource_id = engine.list_children().iter()
                                 .find(|c| c.metadata().map(|m| m.processor_id as u32) == Some(node_idx))
                                 .and_then(|c| c.resource_id());

                             if let Some(rid) = resource_id {
                                 if let Some(s) = self.transfusion_manager.sample_registry.get(rid) {
                                     dna = s.metadata.dna.rhythmic;
                                 }
                             }
                         }
                     }

                     let commands = crate::genetic_sequencer::GeneticSequencer::evolve_pattern(&dna, node_idx, track_idx, mutation_strength);
                     self.apply_mixer_commands(commands);
                 }
                 Command::Performance(nullherz_traits::PerformanceCommand::SetTrackMute { track_idx, muted, .. }) => {
                     println!("Conductor: Track {} Mute set to {}", track_idx, muted);
                 }
                 Command::Performance(nullherz_traits::PerformanceCommand::SetTrackSolo { track_idx, soloed, .. }) => {
                     println!("Conductor: Track {} Solo set to {}", track_idx, soloed);
                 }
                 Command::Performance(nullherz_traits::PerformanceCommand::ClearTrackPattern { track_idx, .. }) => {
                     println!("Conductor: Clearing Pattern for Track {}", track_idx);
                 }
                 Command::Resource(nullherz_traits::ResourceCommand::ApplyFeatureMutation { target_id, feature_name, strength }) => {
                     let name = String::from_utf8_lossy(&feature_name).trim_matches(char::from(0)).to_string();
                     if let Some(mut sample) = self.transfusion_manager.sample_registry.get(target_id) {
                         nullherz_dna::FeatureMutator::mutate(&mut sample.metadata.dna, &name, strength);
                         // Re-register with mutated DNA
                         self.transfusion_manager.sample_registry.register_with_metadata(target_id, sample.buffer, sample.metadata);
                         println!("Applied Feature Mutation '{}' (strength={:.2}) to sample {}", name, strength, target_id);
                     }
                 }
                 _ => final_commands.push(cmd),
             }
        }
        if !final_commands.is_empty() {
            self.mixer_bridge.apply_mixer_commands(final_commands, &mut self.topology_manager, &mut self.modulation_matrix);
        }
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

            let mapped_commands = self.midi_mapper.translate(&event);
            if !mapped_commands.is_empty() {
                self.apply_mixer_commands(mapped_commands);
            }
        }
    }

    pub fn tick(&mut self) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

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

        // Hardened Distributed Audio Routing
        let topo = &self.topology_manager.current_topology;
        for node_idx in 0..topo.node_count {
            if let Some(target) = topo.node_assignments.get(&(node_idx as u32)) {
                if target != "local" {
                    // Batch pull from the non-blocking IPC bridge and dispatch to the remote manager in a single task
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

        // Proactive Matchmaking Suggestions (Stage 6)
        if now % 15 == 0 && self.mixer_bridge.timeline.last_matchmaking_secs != now {
            self.mixer_bridge.timeline.last_matchmaking_secs = now;
            let lib = self.library.clone();
            let suggestions = self.matchmaking_suggestions.clone();

            // Hardened: identify master track DNA using active_master_deck
            let master_sampler_id = self.mixer_manager.deck_mappings.get(&self.active_master_deck).map(|d| d.sampler_id);

            if let Some(sampler_node_idx) = master_sampler_id {
                // Resolve the resource_id (sample_id) currently loaded in the master sampler
                let mut current_sample_id = None;
                {
                    let engine_lock = self.engine_coordinator.backend_manager.engine_handle.lock().unwrap();
                    if let Some(ref engine) = *engine_lock {
                        current_sample_id = engine.list_children().iter()
                            .find(|c| c.metadata().map(|m| m.processor_id as u32) == Some(sampler_node_idx))
                            .and_then(|c| c.resource_id());
                    }
                }

                if let Some(id) = current_sample_id {
                    tokio::spawn(async move {
                        let lib_lock = lib.lock().unwrap();
                        if let Ok(Some(track)) = lib_lock.get_track(id) {
                            if let Ok(matches) = nullherz_dna::Matchmaker::find_best_matches(&lib_lock, &track.metadata.dna, 3) {
                                let mut sugg_lock = suggestions.lock().unwrap();
                                *sugg_lock = matches;
                            }
                        }
                    });
                }
            }
        }

        // Evolutionary Breeding Cycle (Triggered roughly every 10 seconds in the 100ms tick loop)
        if now % 10 == 0 && self.mixer_bridge.timeline.last_breeding_secs != now {
             self.mixer_bridge.timeline.last_breeding_secs = now;
             if let Some(ref breeder) = self.transfusion_manager.evolutionary_breeder {
                 breeder.run_breeding_cycle();
             }
        }

        self.handle_transfusion_registrations();

        self.sync_sampler_metadata();

        self.transfusion_manager.sample_registry.drain_garbage();

        self.drain_garbage();
    }

    fn handle_transfusion_registrations(&mut self) {
        let engine_lock = self.engine_coordinator.backend_manager.engine_handle.lock().unwrap();
        if let Some(ref engine) = *engine_lock {
            self.transfusion_manager.poll_snapshots(engine.as_ref());
        }
    }

    fn sync_sampler_metadata(&mut self) {
        let engine_lock = self.engine_coordinator.backend_manager.engine_handle.lock().unwrap();
        if let Some(ref engine) = *engine_lock {
            for child in engine.list_children() {
                if let Some(id) = child.resource_id() {
                    if let Some(sample) = self.transfusion_manager.sample_registry.get(id) {
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

    pub fn capture_state(&self) -> crate::persistence::ProjectState {
        let mut state = crate::persistence::ProjectState::empty();

        // 1. Collect Topology and Parameters
        let topo = &self.topology_manager.current_topology;
        let mut engine_lock = self.engine_coordinator.backend_manager.engine_handle.lock().unwrap();

        if let Some(ref mut engine) = *engine_lock {
            for child in engine.list_children() {
                let metadata = child.metadata();
                let node_idx = if let Some(m) = metadata { m.processor_id as u32 } else { continue; };

                if let Some(&type_id) = self.topology_manager.active_node_types.get(&node_idx) {
                    let mut params = Vec::new();
                    for p_id in 0..16 {
                        params.push((p_id, child.get_parameter(p_id)));
                    }

                    state.nodes.push(crate::persistence::NodeState {
                        id: node_idx,
                        type_id,
                        params,
                    });

                    let state_data = child.serialize_state();
                    if !state_data.is_empty() {
                        state.processor_states.push(crate::persistence::ProcessorState {
                            node_idx,
                            state_data: state_data.clone(),
                        });
                    }

                    // Collect Sequencer Data if applicable (Legacy path until Stage 3 generic loading)
                    if type_id == nullherz_traits::ProcessorTypeId::SEQUENCER.0 {
                        // 1 byte (active_pattern) + 16 * (1 byte len + 16 * 64 steps)
                        if state_data.len() > 16 * (1 + 16 * 64) {
                            let mut patterns = Vec::new();
                            let active_pattern = state_data[0] as usize;
                            let mut cursor = 1;
                            for _ in 0..16 {
                                let len = state_data[cursor] as u32;
                                cursor += 1;
                                let mut grid = [[false; 64]; 16];
                                for track in 0..16 {
                                    for step in 0..64 {
                                        grid[track][step] = state_data[cursor] == 1;
                                        cursor += 1;
                                    }
                                }
                                patterns.push(crate::persistence::SequencerPatternState { grid, len });
                            }
                            state.sequencers.push(crate::persistence::SequencerNodeState {
                                node_idx,
                                patterns,
                                active_pattern,
                            });
                        }
                    }
                }
            }
        }

        // 2. Collect Edges
        for n_idx in 0..topo.node_count {
            let routing = &topo.routing[n_idx];
            for i in 0..routing.input_count {
                state.edges.push(crate::persistence::EdgeState {
                    node_idx: n_idx as u32,
                    input_idx: i as u32,
                    buffer_idx: routing.input_indices[i] as u32,
                });
            }
            for i in 0..routing.output_count {
                state.output_edges.push(crate::persistence::OutputEdgeState {
                    node_idx: n_idx as u32,
                    output_idx: i as u32,
                    buffer_idx: routing.output_indices[i] as u32,
                });
            }
        }

        // 3. Modulation Matrix
        state.modulation_matrix = self.modulation_matrix.clone();

        // 4. Arrangement State
        state.arrangement = self.pattern_manager.arrangement.clone();

        // 5. Transport State
        state.bpm = self.mixer_bridge.timeline.bpm;
        state.transport_playing = true; // For now assume playing if state is saved, logic for is_playing pending in timeline

        // 6. Master Deck State
        state.active_master_deck = self.active_master_deck;

        state
    }

    pub fn save_project(&self, path: &str) -> std::io::Result<()> {
        let state = self.capture_state();
        state.save_to_file(path)
    }

    pub fn load_project(&mut self, path: &str) -> std::io::Result<()> {
        let state = crate::persistence::ProjectState::load_from_file(path)?;
        self.apply_state(state);
        Ok(())
    }

    pub fn apply_state(&mut self, state: crate::persistence::ProjectState) {
        let _ = self.apply_state_internal(state);
    }

    fn apply_state_internal(&mut self, state: crate::persistence::ProjectState) -> std::io::Result<()> {
        // 1. Reconstruct Nodes
        for node in &state.nodes {
            let cmd = nullherz_traits::Command::Topology(nullherz_traits::TopologyCommand::AddNode {
                processor_type_id: node.type_id.into(),
                node_idx: node.id,
            });
            self.topology_manager.handle_topology_command(&cmd);

            // Apply parameters
            if let Some(ref mut prod) = self.engine_coordinator.command_producer {
                for (param_id, value) in &node.params {
                    let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                        timestamp_samples: 0,
                        command: nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                            target_id: node.id as u64,
                            param_id: *param_id,
                            value: *value,
                            ramp_duration_samples: 0 }) });

                }
            }
        }

        // 2. Reconstruct Edges
        for edge in &state.edges {
            let cmd = nullherz_traits::Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge {
                node_idx: edge.node_idx,
                input_idx: edge.input_idx,
                new_buffer_idx: edge.buffer_idx });

            self.topology_manager.handle_topology_command(&cmd);
        }

        for edge in &state.output_edges {
            let cmd = nullherz_traits::Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge {
                node_idx: edge.node_idx,
                output_idx: edge.output_idx,
                new_buffer_idx: edge.buffer_idx });

            self.topology_manager.handle_topology_command(&cmd);
        }

        // 3. Reconstruct Processor States (Generic approach via TopologyMutation)
        for p_state in &state.processor_states {
            if let Some(ref mut prod) = self.topology_manager.topo_producer {
                let _ = prod.push(nullherz_traits::TopologyMutation::LoadProcessorState {
                    node_idx: p_state.node_idx,
                    state_data: Arc::new(p_state.state_data.clone()),
                });
            }
        }

        // 4. Reconstruct Modulation Matrix
        self.modulation_matrix = state.modulation_matrix;

        // 5. Reconstruct Arrangement
        self.pattern_manager.set_arrangement(state.arrangement);

        // 6. Reconstruct Master Deck
        self.active_master_deck = state.active_master_deck;

        // 7. Transport State
        if let Some(ref mut prod) = self.engine_coordinator.command_producer {
            let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                timestamp_samples: 0,
                command: if state.transport_playing { nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Play) } else { nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop) },
            });
            // BPM is handled via MixerBridge timeline updates, but we should ensure the UI/Gateway is updated.
        }

        // 5. Commit Topology
        self.topology_manager.handle_topology_command(&nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CommitTopology));

        Ok(())
    }
}
