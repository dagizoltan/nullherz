use std::sync::Arc;
use nullherz_traits::{Command, PerformanceCommand, SoundDNA, TopologyMutation};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub name: String,
    pub sequencer_pattern_idx: Option<u32>,
    pub sampler_slice_idx: Option<u32>,
    pub dna_template: Option<SoundDNA>,
    pub node_id: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClipGrid {
    pub clips: Vec<Vec<Option<Clip>>>, // [Row][Column]
}

pub struct ClipOrchestrator {
    pub grid: ClipGrid,
    pub pending_launches: Vec<(usize, usize)>, // (Row, Column)
}

impl ClipOrchestrator {
    pub fn new() -> Self {
        Self {
            grid: ClipGrid::default(),
            pending_launches: Vec::new(),
        }
    }

    pub fn launch_clip(&mut self, row: usize, col: usize) {
        self.pending_launches.push((row, col));
    }

    pub fn transfuse_row(&self, row: usize) -> Vec<TopologyMutation> {
        let mut mutations = Vec::new();
        if row >= self.grid.clips.len() { return mutations; }

        // Find the "Master" DNA in this row (first non-empty clip with DNA)
        let mut source_dna = None;
        for col in 0..self.grid.clips[row].len() {
            if let Some(clip) = &self.grid.clips[row][col] {
                if let Some(dna) = &clip.dna_template {
                    source_dna = Some(dna.clone());
                    break;
                }
            }
        }

        if let Some(dna) = source_dna {
             // Push DNA to all nodes in the row
             for col in 0..self.grid.clips[row].len() {
                 if let Some(clip) = &self.grid.clips[row][col] {
                     mutations.push(TopologyMutation::UpdateMetadata {
                         node_idx: clip.node_id,
                         metadata: Arc::new(nullherz_traits::SampleMetadata {
                             dna: dna.clone(),
                             ..nullherz_traits::SampleMetadata::new_empty()
                         }),
                     });
                 }
             }
        }

        mutations
    }

    pub fn tick(&mut self, current_beat: f64) -> Vec<Command> {
        let mut commands = Vec::new();

        // Simple 1-beat quantization for launches
        let is_on_beat = (current_beat * 100.0).round() % 100.0 < 5.0;

        if is_on_beat && !self.pending_launches.is_empty() {
            for (row, col) in self.pending_launches.drain(..) {
                if row < self.grid.clips.len() && col < self.grid.clips[row].len() {
                    if let Some(clip) = &self.grid.clips[row][col] {
                        if let Some(pattern_idx) = clip.sequencer_pattern_idx {
                            commands.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                                target_id: clip.node_id as u64,
                                param_id: 0, // Active Pattern
                                value: pattern_idx as f32,
                                ramp_duration_samples: 0,
                            }));
                        }
                        if let Some(slice_idx) = clip.sampler_slice_idx {
                            commands.push(Command::Performance(PerformanceCommand::TriggerSlice {
                                node_idx: clip.node_id,
                                slice_idx,
                            }));
                        }
                    }
                }
            }
        }

        commands
    }
}
