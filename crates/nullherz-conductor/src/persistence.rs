#![allow(clippy::collapsible_if)]
#![allow(clippy::needless_range_loop)]
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

/// 256 frames. **The limit is a ~3 ms non-DSP stall, not DSP cost.**
///
/// Period size is the dominant latency term — it sets the device ring
/// (`period x periods`, 3 under RT) and the graph's block quantisation, which
/// `examples/probe_deck_latency.rs` shows is exactly one block:
///
/// ```text
/// block 256 -> deck reaches master in 353 samples (96 PDC + 256 + 1)
/// block 128 -> 225 samples (96 + 128 + 1)
/// block  64 -> 161 samples (96 + 64 + 1)
/// ```
///
/// So halving this halves a real term twice over, and every instinct says keep
/// halving. Measured, it does not survive (`scripts/verify.sh` smoke, Threaded):
///
/// ```text
/// period 256 -> mean 135 us, peak 3785 us / 5333 us budget   PASS
/// period 128 -> mean 135 us, peak 2945 us / 2666 us budget   FAIL, 1 xrun at 18.8 s
/// period  64 -> peak 3046 us / 1333 us budget                FAIL, 1 xrun
/// ```
///
/// Read the mean against the peak: **135 us against 2945 us, a factor of 22.**
/// Steady-state DSP uses 5% of a 128-frame budget. Nothing about this ceiling is
/// audio work, and optimising DSP will not move it. One stall of roughly 3 ms
/// arrives regardless of block size; at 256 a 5333 us budget absorbs it, at 128
/// it does not.
///
/// A 128-frame default was tried and reverted: a single 4-minute run passed with
/// a 1177 us peak, which was the stall simply not landing. One sample does not
/// characterise a tail event.
///
/// WHAT IT IS NOT: the registry reaper. That was the obvious suspect — it frees
/// in bursts (`Registry reap: released 2 sample(s), ~107.2 MB of decoded audio`)
/// and returning 100+ MB at once means munmap, page-table teardown and
/// cross-core TLB shootdowns. A/B at 128 frames says the opposite:
///
/// ```text
/// NULLHERZ_REGISTRY_REAP=1 -> PASS, peak  823 us   (mean 114 us)
/// NULLHERZ_REGISTRY_REAP=0 -> FAIL, peak 3482 us, 1 xrun
/// ```
///
/// Disabling it makes the stall WORSE, so reaping is part of the mitigation, not
/// the cause. That inverts the reading: the cost tracks memory GROWTH, not
/// freeing. With no reaping the registry retains every decoded track, the heap
/// climbs, and `mlockall` is `MCL_CURRENT` only (deliberately — see
/// `ipc-layer`), so everything allocated afterwards faults lazily. First-touch
/// faults on a growing heap are precisely multi-millisecond events. Bounding the
/// footprint is what keeps them rare.
///
/// The stall is still present with reaping on: three runs at 128 frames gave
/// PASS 1177 us (4 min), PASS 823 us (2 min), FAIL 2945 us with an xrun at
/// 18.8 s (1 min). One in three. 128 is not safe, it is lucky, which is why this
/// is 256.
///
/// Next diagnostic, in order:
///   1. Fix the harness asymmetry below — without it the peak cannot be
///      attributed to the event that failed the run.
///   2. Test whether the stall correlates with hydration (decoding a ~100 MB
///      track) rather than with steady playback. The smoke re-hydrates every
///      run, so this needs the harness to separate the two.
///   3. Only then consider pre-faulting or a bounded arena for the decode path.
/// `isolcpus` is worth trying but will NOT help if the cause is our own
/// allocation behaviour rather than scheduler noise.
///
/// Confirming any of it needs the harness fixed first: `peak_process_time_ns` is
/// a running max over the WHOLE run including warm-up (`bin/survival.rs`), while
/// the xrun verdict excludes warm-up — so the peak figure and the pass/fail
/// verdict are measured over different windows.
///
/// `NULLHERZ_PERIOD_SIZE` evaluates a change without editing a file.
pub fn default_period_size() -> u64 {
    128
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub audio_backend: String,
    pub midi_ports: Vec<String>,
    pub sample_rate: u32,
    pub block_size: u32,
    #[serde(default)]
    pub calibration_samples: u32,
    #[serde(default = "default_period_size")]
    pub period_size: u64,
}

pub const CURRENT_PROJECT_VERSION: u32 = 1;

pub fn default_project_version() -> u32 {
    0
}

#[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct ProjectState {
    #[serde(default = "default_project_version")]
    pub version: u32,
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
            version: CURRENT_PROJECT_VERSION,
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
        state.version = CURRENT_PROJECT_VERSION;
        let topo = &conductor.topology_manager.current_topology;

        // Use try_lock for non-blocking capture in auto-save context
        let mut engine_handle_lock = conductor.engine_coordinator.backend_manager.engine_handle.try_lock();

        if let Some(ref mut engine_opt) = engine_handle_lock {
            if let Some(engine) = engine_opt.as_mut() {
                for child in engine.list_children() {
                    let metadata = child.metadata();
                    let node_idx = if let Some(m) = metadata { m.processor_id as u32 } else { continue; };

                    if let Some(&type_id) = conductor.topology_manager.active_node_types.get(&node_idx) {
                        let mut params = Vec::new();
                        for p_id in 0..16 {
                            params.push((p_id, child.get_parameter(p_id)));
                        }

                        let position = if (node_idx as usize) < topo.node_positions.len() {
                            topo.node_positions[node_idx as usize]
                        } else {
                            None
                        };
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

        if state.nodes.is_empty() {
            // Fallback to active_node_types in conductor when the audio engine is not active/running
            for (&node_idx, &type_id) in &conductor.topology_manager.active_node_types {
                let position = if (node_idx as usize) < topo.node_positions.len() {
                    topo.node_positions[node_idx as usize]
                } else {
                    None
                };
                state.nodes.push(NodeState {
                    id: node_idx,
                    type_id,
                    params: Vec::new(),
                    position,
                });
            }
        }

        for n_idx in 0..topo.node_count {
            let routing = &topo.routing[n_idx];
            for i in 0..routing.input_count {
                state.edges.push(EdgeState {
                    node_idx: n_idx as u32,
                    input_idx: i as u32,
                    buffer_idx: routing.input_indices[i].0,
                });
            }
            for i in 0..routing.output_count {
                state.output_edges.push(OutputEdgeState {
                    node_idx: n_idx as u32,
                    output_idx: i as u32,
                    buffer_idx: routing.output_indices[i].0,
                });
            }
        }

        state.modulation_matrix = conductor.modulation_matrix.clone();
        state.arrangement = conductor.pattern_manager.arrangement.clone();
        state.bpm = conductor.mixer_bridge.timeline.bpm;
        state.transport_playing = true;
        state.active_master_deck = conductor.mixer_manager.active_master_deck;
        state.node_names = conductor.mixer_manager.node_names.clone();

        state
    }

    pub fn apply(&self, conductor: &mut crate::orchestrator::Conductor) -> std::io::Result<()> {
        // Compute which node_idx values currently exist in the live topology but are NOT present in self.nodes,
        // and issue a RemoveNode command for each before proceeding with the existing add/update logic.
        let incoming_node_ids: std::collections::HashSet<u32> = self.nodes.iter().map(|n| n.id).collect();
        let existing_node_ids: Vec<u32> = conductor.topology_manager.active_node_types.keys().copied().collect();

        for existing_id in existing_node_ids {
            if !incoming_node_ids.contains(&existing_id) {
                let remove_cmd = Command::Topology(nullherz_traits::TopologyCommand::RemoveNode {
                    node_idx: existing_id,
                });
                conductor.topology_manager.handle_topology_command(&remove_cmd);
            }
        }

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
        conductor.mixer_manager.active_master_deck = self.active_master_deck;
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

    fn check_version(version: u32) -> std::io::Result<()> {
        if version == CURRENT_PROJECT_VERSION {
            Ok(())
        } else if version < CURRENT_PROJECT_VERSION {
            match version {
                0 => {
                    println!("Loading project saved with older version 0 (predates versioning), no migration needed yet.");
                }
                _ => {
                    println!("Loading project saved with older version {}, no migration needed yet.", version);
                }
            }
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Cannot load project: file was saved with a newer version ({}) than the current supported version ({})",
                    version,
                    CURRENT_PROJECT_VERSION
                )
            ))
        }
    }

    pub fn load_from_file(path: &str) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let state: Self = serde_json::from_str(&content).map_err(|e| std::io::Error::other(e))?;
        Self::check_version(state.version)?;
        Ok(state)
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
        Self::check_version(deserialized.version)?;
        Ok(deserialized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::CoreCommand;

    fn non_trivial_state() -> ProjectState {
        let mut state = ProjectState::empty();
        state.active_master_deck = 'C';
        state.bpm = 174.0;
        state.transport_playing = true;
        state.node_names.insert("deck_a_sampler".to_string(), 4);
        state.modulation_matrix.add_mapping(1, 10, 0, 0.5, 64);
        state.arrangement.events.push(crate::pattern_manager::ArrangementEvent {
            beat: 8.0,
            command: Command::Core(CoreCommand::SetBpm(140.0)),
        });
        state
    }

    fn assert_state_survives(loaded: &ProjectState) {
        assert_eq!(loaded.version, CURRENT_PROJECT_VERSION);
        assert_eq!(loaded.active_master_deck, 'C');
        assert_eq!(loaded.bpm, 174.0);
        assert!(loaded.transport_playing);
        assert_eq!(loaded.node_names.get("deck_a_sampler"), Some(&4));
        assert_eq!(loaded.arrangement.events.len(), 1);
        assert_eq!(loaded.arrangement.events[0].beat, 8.0);
        let expanded = loaded.modulation_matrix.expand_macro(1, 1.0, 0.0);
        assert!(!expanded.is_empty(), "modulation matrix must survive the round trip");
    }

    #[test]
    fn test_project_state_json_round_trip() {
        let path = std::env::temp_dir().join(format!("nullherz_persist_json_{}.json", std::process::id()));
        let path = path.to_str().unwrap();
        non_trivial_state().save_to_file(path).unwrap();
        let loaded = ProjectState::load_from_file(path).unwrap();
        assert_state_survives(&loaded);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_project_state_rkyv_round_trip() {
        let path = std::env::temp_dir().join(format!("nullherz_persist_rkyv_{}.bin", std::process::id()));
        let path = path.to_str().unwrap();
        non_trivial_state().save_to_rkyv(path).unwrap();
        let loaded = ProjectState::load_from_rkyv(path).unwrap();
        assert_state_survives(&loaded);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_newer_version_is_rejected_older_is_migrated() {
        let path = std::env::temp_dir().join(format!("nullherz_persist_ver_{}.json", std::process::id()));
        let path = path.to_str().unwrap();

        // A file from the future must be refused, not half-loaded.
        let mut future = ProjectState::empty();
        future.version = CURRENT_PROJECT_VERSION + 1;
        future.save_to_file(path).unwrap();
        assert!(ProjectState::load_from_file(path).is_err());

        // A pre-versioning file (version field absent -> default 0) must load.
        let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("version");
        std::fs::write(path, serde_json::to_string(&json).unwrap()).unwrap();
        let loaded = ProjectState::load_from_file(path).unwrap();
        assert_eq!(loaded.version, 0, "missing version field defaults to 0 and loads");

        let _ = std::fs::remove_file(path);
    }
}
