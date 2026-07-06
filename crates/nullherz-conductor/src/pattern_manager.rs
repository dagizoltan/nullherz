use serde::{Serialize, Deserialize};
use nullherz_traits::Command;

#[derive(Debug, Clone, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct ArrangementEvent {
    pub beat: f64,
    pub command: Command,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct SongArrangement {
    pub events: Vec<ArrangementEvent>,
}

pub struct PatternManager {
    pub arrangement: SongArrangement,
    pub last_triggered_idx: usize,
    last_processed_beat: f64,
}

impl Default for PatternManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PatternManager {
    pub fn new() -> Self {
        Self {
            arrangement: SongArrangement::default(),
            last_triggered_idx: 0,
            last_processed_beat: 0.0,
        }
    }

    pub fn tick(&mut self, current_beat: f64) -> Vec<Command> {
        let mut triggered = Vec::new();

        if current_beat < self.last_processed_beat {
            // We jumped back, reset search.
            // In a more advanced version, we'd seek to the correct idx.
            self.last_triggered_idx = 0;
        }
        self.last_processed_beat = current_beat;

        while self.last_triggered_idx < self.arrangement.events.len() {
            let event = &self.arrangement.events[self.last_triggered_idx];
            if current_beat >= event.beat {
                triggered.push(event.command);
                self.last_triggered_idx += 1;
            } else {
                break;
            }
        }
        triggered
    }

    pub fn reset(&mut self) {
        self.last_triggered_idx = 0;
        self.last_processed_beat = 0.0;
    }

    pub fn set_arrangement(&mut self, arrangement: SongArrangement) {
        self.arrangement = arrangement;
        // Sort events by beat to ensure correct sequential processing
        self.arrangement.events.sort_by(|a, b| a.beat.partial_cmp(&b.beat).unwrap());
        self.reset();
    }
}

pub struct DnaSequencer;

impl DnaSequencer {
    pub fn dna_to_commands(dna: &nullherz_traits::RhythmicDNA, node_idx: u32, track_idx: u32) -> Vec<nullherz_traits::Command> {
        let mut commands = Vec::new();
        for (i, &mask) in dna.onset_mask.iter().enumerate() {
            for bit in 0..64 {
                let step = (i * 64) + bit;
                let value = if (mask >> bit) & 1 == 1 { 1.0 } else { 0.0 };
                commands.push(nullherz_traits::Command::Performance(
                    nullherz_traits::PerformanceCommand::SetSequencerStep {
                        node_idx,
                        track: track_idx,
                        step: step as u32,
                        value,
                    }
                ));
            }
        }
        // Micro-timing can be applied via parameter updates or specialized commands if the processor supports it.
        // For now, we focus on the onset mask.
        commands
    }

    pub fn mutate_pattern(
        dna: &nullherz_traits::RhythmicDNA,
        current_grid: &[[f32; 64]; 16],
        node_idx: u32,
        track_idx: u32,
        mutation_probability: f32
    ) -> Vec<nullherz_traits::Command> {
        let mut commands = Vec::new();
        for (i, &mask) in dna.onset_mask.iter().enumerate() {
            for bit in 0..64 {
                let step = (i * 64) + bit;
                let dna_value = if (mask >> bit) & 1 == 1 { 1.0 } else { 0.0 };
                let current_value = current_grid[track_idx as usize][step];

                // Deterministic pseudo-randomness for stable evolution
                let seed = (track_idx as u32).wrapping_mul(256).wrapping_add(step as u32);
                let rand_val = (seed.wrapping_mul(1103515245).wrapping_add(12345) as f32) / 4294967295.0;

                if rand_val < mutation_probability {
                    if (dna_value > 0.0) != (current_value > 0.0) {
                        commands.push(nullherz_traits::Command::Performance(
                            nullherz_traits::PerformanceCommand::SetSequencerStep {
                                node_idx,
                                track: track_idx,
                                step: step as u32,
                                value: dna_value,
                            }
                        ));
                    }
                }
            }
        }
        commands
    }
}
