use std::sync::Arc;
use nullherz_traits::{Command, PerformanceCommand, SoundDNA, TopologyMutation};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipState {
    Stopped,
    Starting,
    Playing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub name: String,
    pub sequencer_pattern_idx: Option<u32>,
    pub sampler_slice_idx: Option<u32>,
    pub dna_template: Option<SoundDNA>,
    pub node_id: u32,
    pub state: ClipState,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClipGrid {
    pub clips: Vec<Vec<Option<Clip>>>, // [Row][Column]
}

pub struct ClipOrchestrator {
    pub grid: ClipGrid,
    pub pending_launches: Vec<(usize, usize)>, // (Row, Column)
    pub last_quantize_beat: i64,
}

impl ClipOrchestrator {
    pub fn collect_telemetry(&self, active_clips: &mut [u8; 8], starting_masks: &mut [u8; 8]) {
        active_clips.fill(255);
        starting_masks.fill(0);

        for (r, row) in self.grid.clips.iter().enumerate().take(8) {
            for (c, clip_opt) in row.iter().enumerate().take(8) {
                if let Some(clip) = clip_opt {
                    match clip.state {
                        ClipState::Playing => active_clips[r] = c as u8,
                        ClipState::Starting => starting_masks[r] |= 1 << c,
                        _ => {}
                    }
                }
            }
        }
    }

    pub fn new() -> Self {
        Self {
            grid: ClipGrid::default(),
            pending_launches: Vec::new(),
            last_quantize_beat: -1,
        }
    }

    pub fn launch_clip(&mut self, row: usize, col: usize) {
        if row < self.grid.clips.len() && col < self.grid.clips[row].len() {
            if let Some(clip) = &mut self.grid.clips[row][col] {
                clip.state = ClipState::Starting;
            }
        }
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

        let current_int_beat = current_beat.floor() as i64;
        let is_quantize_trigger = current_int_beat > self.last_quantize_beat;

        if is_quantize_trigger {
            self.last_quantize_beat = current_int_beat;

            if !self.pending_launches.is_empty() {
                for (row, col) in self.pending_launches.drain(..) {
                    if row >= self.grid.clips.len() || col >= self.grid.clips[row].len() { continue; }

                    // Stop other clips in the same row
                    for c_idx in 0..self.grid.clips[row].len() {
                        if let Some(other_clip) = &mut self.grid.clips[row][c_idx] {
                            if c_idx != col {
                                other_clip.state = ClipState::Stopped;
                            }
                        }
                    }

                    if let Some(clip) = &mut self.grid.clips[row][col] {
                        clip.state = ClipState::Playing;
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
