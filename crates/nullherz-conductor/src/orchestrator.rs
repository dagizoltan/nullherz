use crate::engine_coordinator::EngineCoordinator;
use crate::topology_manager::TopologyManager;
use crate::transfusion_manager::TransfusionManager;
use crate::mixer_bridge::MixerBridge;
use crate::sidecar_supervisor::SidecarSupervisor;
use crate::pattern_manager::PatternManager;
use crate::modulation_matrix::ModulationMatrix;
use nullherz_traits::{Command, CommandProducer, RenderingEngine, telemetry::Telemetry};
use std::sync::Arc;
use nullherz_dna::SampleRegistry;

pub struct Conductor {
    pub engine_coordinator: EngineCoordinator,
    pub topology_manager: TopologyManager,
    pub transfusion_manager: TransfusionManager,
    pub mixer_bridge: MixerBridge,
    pub sidecar_supervisor: SidecarSupervisor,
    pub pattern_manager: PatternManager,
    pub modulation_matrix: ModulationMatrix,
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
        Self::with_library_path("library.redb")
    }

    pub fn with_library_path(path: &str) -> Self {
        let sample_registry = Arc::new(SampleRegistry::new());
        let library = Arc::new(std::sync::Mutex::new(nullherz_dna::LibraryDatabase::load(path).unwrap()));
        Self {
            engine_coordinator: EngineCoordinator::new(),
            topology_manager: TopologyManager::new(),
            transfusion_manager: TransfusionManager::new(sample_registry.clone()),
            mixer_bridge: MixerBridge::new(),
            sidecar_supervisor: SidecarSupervisor::new(),
            pattern_manager: PatternManager::new(),
            modulation_matrix: ModulationMatrix::new(),
            analysis_worker: Some(crate::analysis_worker::AnalysisWorker::new(sample_registry.clone()).with_library(library.clone())),
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
        self.mixer_bridge.apply_mixer_commands(commands, &mut self.topology_manager, &mut self.modulation_matrix);
    }

    pub fn tick(&mut self) {
        // Update Pattern Orchestration
        let arrangement_commands = self.pattern_manager.tick(self.mixer_bridge.timeline.current_beat);
        if !arrangement_commands.is_empty() {
            self.apply_mixer_commands(arrangement_commands);
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
                if let Some(sampler) = child.as_any().downcast_ref::<nullherz_processors::SamplerProcessor>()
                    && let Some(id) = sampler.id_getter()
                        && let Some(sample) = self.transfusion_manager.sample_registry.get(id)
                             && let Some(ref mut prod) = self.topology_manager.topo_producer {
                                 let _ = prod.push(nullherz_traits::TopologyMutation::UpdateMetadata {
                                     node_idx: sampler.id as u32,
                                     metadata: Arc::new(sample.metadata),
                                 });
                             }
            }
        }
    }

    pub fn save_project(&self, path: &str) -> std::io::Result<()> {
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

                    // Collect Sequencer Data if applicable
                    if type_id == nullherz_traits::ProcessorTypeId::SEQUENCER.0 {
                        let bytes = child.serialize_state();
                        // 1 byte (active_pattern) + 16 * (1 byte len + 16 * 64 steps)
                        if bytes.len() > 16 * (1 + 16 * 64) {
                            let mut patterns = Vec::new();
                            let active_pattern = bytes[0] as usize;
                            let mut cursor = 1;
                            for _ in 0..16 {
                                let len = bytes[cursor] as u32;
                                cursor += 1;
                                let mut grid = [[false; 64]; 16];
                                for track in 0..16 {
                                    for step in 0..64 {
                                        grid[track][step] = bytes[cursor] == 1;
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

        state.save_to_file(path)
    }

    pub fn load_project(&mut self, path: &str) -> std::io::Result<()> {
        let state = crate::persistence::ProjectState::load_from_file(path)?;

        // 1. Reconstruct Nodes
        for node in &state.nodes {
            let cmd = nullherz_traits::Command::AddNode {
                processor_type_id: node.type_id.into(),
                node_idx: node.id,
            };
            self.topology_manager.handle_topology_command(&cmd);

            // Apply parameters
            if let Some(ref mut prod) = self.engine_coordinator.command_producer {
                for (param_id, value) in &node.params {
                    let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                        timestamp_samples: 0,
                        command: nullherz_traits::Command::SetParam {
                            target_id: node.id as u64,
                            param_id: *param_id,
                            value: *value,
                            ramp_duration_samples: 0,
                        },
                    });
                }
            }
        }

        // 2. Reconstruct Edges
        for edge in &state.edges {
            let cmd = nullherz_traits::Command::UpdateEdge {
                node_idx: edge.node_idx,
                input_idx: edge.input_idx,
                new_buffer_idx: edge.buffer_idx,
            };
            self.topology_manager.handle_topology_command(&cmd);
        }

        for edge in &state.output_edges {
            let cmd = nullherz_traits::Command::UpdateOutputEdge {
                node_idx: edge.node_idx,
                output_idx: edge.output_idx,
                new_buffer_idx: edge.buffer_idx,
            };
            self.topology_manager.handle_topology_command(&cmd);
        }

        // 3. Reconstruct Sequencer Patterns
        for seq in &state.sequencers {
            if let Some(ref mut prod) = self.engine_coordinator.command_producer {
                for (p_idx, pat) in seq.patterns.iter().enumerate() {
                    // Set active pattern temporarily to write steps
                    let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                        timestamp_samples: 0,
                        command: nullherz_traits::Command::SetParam {
                            target_id: seq.node_idx as u64,
                            param_id: 0, // Active Pattern
                            value: p_idx as f32,
                            ramp_duration_samples: 0,
                        },
                    });

                    // Set length
                    let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                        timestamp_samples: 0,
                        command: nullherz_traits::Command::SetParam {
                            target_id: seq.node_idx as u64,
                            param_id: 1, // Pattern Length
                            value: pat.len as f32,
                            ramp_duration_samples: 0,
                        },
                    });

                    for track in 0..16 {
                        for step in 0..64 {
                            let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                                timestamp_samples: 0,
                                command: nullherz_traits::Command::SetSequencerStep {
                                    node_idx: seq.node_idx,
                                    track,
                                    step,
                                    value: pat.grid[track as usize][step as usize],
                                },
                            });
                        }
                    }
                }

                // Restore active pattern
                let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                    timestamp_samples: 0,
                    command: nullherz_traits::Command::SetParam {
                        target_id: seq.node_idx as u64,
                        param_id: 0,
                        value: seq.active_pattern as f32,
                        ramp_duration_samples: 0,
                    },
                });
            }
        }

        // 4. Reconstruct Modulation Matrix
        self.modulation_matrix = state.modulation_matrix;

        // 5. Reconstruct Arrangement
        self.pattern_manager.set_arrangement(state.arrangement);

        // 6. Transport State
        if let Some(ref mut prod) = self.engine_coordinator.command_producer {
            let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                timestamp_samples: 0,
                command: if state.transport_playing { nullherz_traits::Command::Play } else { nullherz_traits::Command::Stop },
            });
            // BPM is handled via MixerBridge timeline updates, but we should ensure the UI/Gateway is updated.
        }

        // 5. Commit Topology
        self.topology_manager.handle_topology_command(&nullherz_traits::Command::CommitTopology);

        Ok(())
    }
}
