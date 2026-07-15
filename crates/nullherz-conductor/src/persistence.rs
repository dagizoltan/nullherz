use serde::{Serialize, Deserialize};
use serde_with::serde_as;

#[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct NodeState {
    pub id: u32,
    pub type_id: u32,
    pub params: Vec<(u32, f32)>,
    pub position: Option<(f32, f32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct EdgeState {
    pub node_idx: u32,
    pub input_idx: u32,
    pub buffer_idx: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct OutputEdgeState {
    pub node_idx: u32,
    pub output_idx: u32,
    pub buffer_idx: u32,
}

#[serde_as]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct SequencerPatternState {
    #[serde_as(as = "[[_; 64]; 16]")]
    pub grid: [[bool; 64]; 16],
    pub len: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct SequencerNodeState {
    pub node_idx: u32,
    pub patterns: Vec<SequencerPatternState>,
    pub active_pattern: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct ProcessorState {
    pub node_idx: u32,
    pub state_data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub audio_backend: String,
    pub midi_ports: Vec<String>,
    pub sample_rate: u32,
    pub block_size: u32,
    #[serde(default)]
    pub calibration_samples: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct ProjectState {
    pub active_master_deck: char,
    pub nodes: Vec<NodeState>,
    pub edges: Vec<EdgeState>,
    pub output_edges: Vec<OutputEdgeState>,
    pub sequencers: Vec<SequencerNodeState>,
    pub processor_states: Vec<ProcessorState>,
    pub modulation_matrix: crate::modulation_matrix::ModulationMatrix,
    pub arrangement: crate::pattern_manager::SongArrangement,
    pub clip_grid: crate::clip_orchestrator::ClipGrid,
    pub bpm: f32,
    pub transport_playing: bool,
    #[serde(default)]
    pub node_names: std::collections::HashMap<String, u32>,
}

use std::sync::Arc;
use nullherz_traits::{Command, TimestampedCommand, TopologyMutation, ProcessorTypeId};

impl ProjectState {
    pub fn empty() -> Self {
        Self {
            active_master_deck: 'A',
            nodes: Vec::new(),
            edges: Vec::new(),
            output_edges: Vec::new(),
            sequencers: Vec::new(),
            processor_states: Vec::new(),
            modulation_matrix: crate::modulation_matrix::ModulationMatrix::default(),
            arrangement: crate::pattern_manager::SongArrangement::default(),
            clip_grid: crate::clip_orchestrator::ClipGrid::default(),
            bpm: 120.0,
            transport_playing: false,
            node_names: std::collections::HashMap::new(),
        }
    }

    pub fn capture(conductor: &crate::orchestrator::Conductor) -> Self {
        let mut state = Self::empty();
        let topo = &conductor.topology_manager.current_topology;

        // Use try_lock for non-blocking capture in auto-save context
        let mut engine_handle_lock = conductor.engine_coordinator.backend_manager.engine_handle.try_lock();

        if let Ok(ref mut engine_opt) = engine_handle_lock {
            if let Some(engine) = engine_opt.as_mut() {
                for child in engine.list_children() {
                    let metadata = child.metadata();
                    let node_idx = if let Some(m) = metadata { m.processor_id as u32 } else { continue; };

                    if let Some(&type_id) = conductor.topology_manager.active_node_types.get(&node_idx) {
                        let mut params = Vec::new();
                        for p_id in 0..16 {
                            params.push((p_id, child.get_parameter(p_id)));
                        }

                        let position = topo.node_positions[node_idx as usize];
                        state.nodes.push(NodeState {
                            id: node_idx,
                            type_id,
                            params,
                            position,
                        });

                        let state_data = child.serialize_state();
                        if !state_data.is_empty() {
                            state.processor_states.push(ProcessorState {
                                node_idx,
                                state_data: state_data.clone(),
                            });
                        }

                        if type_id == ProcessorTypeId::SEQUENCER.0 {
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
                                    patterns.push(SequencerPatternState { grid, len });
                                }
                                state.sequencers.push(SequencerNodeState {
                                    node_idx,
                                    patterns,
                                    active_pattern,
                                });
                            }
                        }
                    }
                }
            }
        }

        for n_idx in 0..topo.node_count {
            let routing = &topo.routing[n_idx];
            for i in 0..routing.input_count {
                state.edges.push(EdgeState {
                    node_idx: n_idx as u32,
                    input_idx: i as u32,
                    buffer_idx: routing.input_indices[i] as u32,
                });
            }
            for i in 0..routing.output_count {
                state.output_edges.push(OutputEdgeState {
                    node_idx: n_idx as u32,
                    output_idx: i as u32,
                    buffer_idx: routing.output_indices[i] as u32,
                });
            }
        }

        state.modulation_matrix = conductor.modulation_matrix.clone();
        state.arrangement = conductor.pattern_manager.arrangement.clone();
        state.bpm = conductor.mixer_bridge.timeline.bpm;
        state.transport_playing = true;
        state.active_master_deck = conductor.active_master_deck;
        state.node_names = conductor.mixer_manager.node_names.clone();

        state
    }

    pub fn apply(&self, conductor: &mut crate::orchestrator::Conductor) -> std::io::Result<()> {
        for node in &self.nodes {
            let cmd = Command::Topology(nullherz_traits::TopologyCommand::AddNode {
                processor_type_id: node.type_id.into(),
                node_idx: node.id,
            });
            conductor.topology_manager.handle_topology_command(&cmd);

            if let Some((x, y)) = node.position {
                let pos_cmd = Command::Topology(nullherz_traits::TopologyCommand::SetNodePosition {
                    node_idx: node.id,
                    x,
                    y,
                });
                conductor.topology_manager.handle_topology_command(&pos_cmd);
            }

            if let Some(ref mut prod) = conductor.engine_coordinator.command_producer {
                for (param_id, value) in &node.params {
                    let _ = prod.push_command(TimestampedCommand {
                        timestamp_samples: 0,
                        command: Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                            target_id: node.id as u64,
                            param_id: *param_id,
                            value: *value,
                            ramp_duration_samples: 0 }) });
                }
            }
        }

        for edge in &self.edges {
            let cmd = Command::Topology(nullherz_traits::TopologyCommand::UpdateEdge {
                node_idx: edge.node_idx,
                input_idx: edge.input_idx,
                new_buffer_idx: edge.buffer_idx });
            conductor.topology_manager.handle_topology_command(&cmd);
        }

        for edge in &self.output_edges {
            let cmd = Command::Topology(nullherz_traits::TopologyCommand::UpdateOutputEdge {
                node_idx: edge.node_idx,
                output_idx: edge.output_idx,
                new_buffer_idx: edge.buffer_idx });
            conductor.topology_manager.handle_topology_command(&cmd);
        }

        for p_state in &self.processor_states {
            if let Some(ref mut prod) = conductor.topology_manager.topo_producer {
                let _ = prod.push(TopologyMutation::LoadProcessorState {
                    node_idx: p_state.node_idx,
                    state_data: Arc::new(p_state.state_data.clone()),
                });
            }
        }

        conductor.modulation_matrix = self.modulation_matrix.clone();
        conductor.pattern_manager.set_arrangement(self.arrangement.clone());
        conductor.active_master_deck = self.active_master_deck;
        conductor.mixer_manager.node_names = self.node_names.clone();

        if let Some(ref mut prod) = conductor.engine_coordinator.command_producer {
            let _ = prod.push_command(TimestampedCommand {
                timestamp_samples: 0,
                command: if self.transport_playing { Command::Core(nullherz_traits::CoreCommand::Play) } else { Command::Core(nullherz_traits::CoreCommand::Stop) },
            });
        }

        conductor.topology_manager.handle_topology_command(&Command::Core(nullherz_traits::CoreCommand::CommitTopology));
        Ok(())
    }

    pub fn save_to_file(&self, path: &str) -> std::io::Result<()> {
        let temp_path = format!("{}.tmp", path);
        let json = serde_json::to_string_pretty(self).map_err(|e| std::io::Error::other(e))?;
        std::fs::write(&temp_path, json)?;
        std::fs::rename(temp_path, path)
    }

    pub fn load_from_file(path: &str) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(|e| std::io::Error::other(e))
    }

    pub fn save_to_rkyv(&self, path: &str) -> std::io::Result<()> {
        let temp_path = format!("{}.tmp.rkyv", path);
        let bytes = rkyv::to_bytes::<_, 1024>(self).map_err(|e| std::io::Error::other(format!("rkyv serialize error: {}", e)))?;
        std::fs::write(&temp_path, bytes)?;
        std::fs::rename(temp_path, path)
    }

    pub fn load_from_rkyv(path: &str) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        let archived = rkyv::check_archived_root::<Self>(&bytes[..])
            .map_err(|e| std::io::Error::other(format!("rkyv validation error: {}", e)))?;
        let deserialized: Self = rkyv::Deserialize::<Self, _>::deserialize(archived, &mut rkyv::Infallible).map_err(|e| std::io::Error::other(format!("rkyv deserialize error: {}", e)))?;
        Ok(deserialized)
    }
}
