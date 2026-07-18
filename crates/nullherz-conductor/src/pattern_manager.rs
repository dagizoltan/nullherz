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

    /// Applies Groove Transfusion: transfers micro-timing offsets from DNA to a target track.
    pub fn apply_groove(dna: &nullherz_traits::RhythmicDNA, node_idx: u32, track_idx: u32) -> Vec<nullherz_traits::Command> {
        let mut commands = Vec::new();
        // RhythmicDNA.micro_timing contains 12 offsets (e.g. for 16th notes in a 3/4 bar or similar subdivision)
        // We map these to sequencer parameters if the processor supports timing offsets.
        for (i, &offset_i16) in dna.micro_timing.iter().enumerate() {
            let offset_f32 = (offset_i16 as f32) / 128.0; // Normalize to approx +/- 1.0 step fraction
            commands.push(nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                target_id: node_idx as u64,
                param_id: 100 + (track_idx * 16) + (i as u32), // Convention: param 100+ is timing offsets
                value: offset_f32,
                ramp_duration_samples: 0,
            }));
        }
        commands
    }

    pub fn mutate_pattern(
        dna: &nullherz_traits::RhythmicDNA,
        current_grid: &[Vec<f32>; 16],
        node_idx: u32,
        track_idx: u32,
        mutation_probability: f32
    ) -> Vec<nullherz_traits::Command> {
        let mut commands = Vec::new();
        for (i, &mask) in dna.onset_mask.iter().enumerate() {
            for bit in 0..64 {
                let step = (i * 64) + bit;
                let dna_value = if (mask >> bit) & 1 == 1 { 1.0 } else { 0.0 };
                let current_value = current_grid[track_idx as usize].get(step).copied().unwrap_or(0.0);

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

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::CoreCommand;

    fn arrangement(beats: &[f64]) -> SongArrangement {
        SongArrangement {
            events: beats
                .iter()
                .map(|&beat| ArrangementEvent {
                    beat,
                    command: Command::Core(CoreCommand::SetBpm(beat as f32)),
                })
                .collect(),
        }
    }

    fn bpm_of(cmd: &Command) -> f32 {
        match cmd {
            Command::Core(CoreCommand::SetBpm(v)) => *v,
            other => panic!("unexpected command {:?}", other),
        }
    }

    #[test]
    fn test_events_fire_in_order_and_only_once() {
        let mut pm = PatternManager::new();
        pm.set_arrangement(arrangement(&[1.0, 2.0, 4.0]));

        assert!(pm.tick(0.5).is_empty(), "nothing before the first event");

        let fired = pm.tick(2.5);
        assert_eq!(fired.len(), 2, "beats 1.0 and 2.0 due by 2.5");
        assert_eq!(bpm_of(&fired[0]), 1.0);
        assert_eq!(bpm_of(&fired[1]), 2.0);

        assert!(pm.tick(3.9).is_empty(), "no re-fire between events");

        let fired = pm.tick(4.0);
        assert_eq!(fired.len(), 1, "event exactly on the tick beat fires");
        assert_eq!(bpm_of(&fired[0]), 4.0);

        assert!(pm.tick(100.0).is_empty(), "arrangement exhausted");
    }

    #[test]
    fn test_set_arrangement_sorts_unsorted_events() {
        let mut pm = PatternManager::new();
        pm.set_arrangement(arrangement(&[4.0, 1.0, 2.0]));
        let fired = pm.tick(5.0);
        let bpms: Vec<f32> = fired.iter().map(bpm_of).collect();
        assert_eq!(bpms, vec![1.0, 2.0, 4.0], "events must fire in beat order regardless of insertion order");
    }

    /// Documented semantic: jumping backwards (loop/seek) rewinds the cursor
    /// and re-fires every event up to the new position, re-establishing
    /// arrangement state after the jump.
    #[test]
    fn test_jump_back_refires_past_events() {
        let mut pm = PatternManager::new();
        pm.set_arrangement(arrangement(&[1.0, 2.0]));
        assert_eq!(pm.tick(3.0).len(), 2);
        let refired = pm.tick(1.5);
        assert_eq!(refired.len(), 1, "jump back to 1.5 re-fires the beat-1.0 event");
        assert_eq!(bpm_of(&refired[0]), 1.0);
    }

    #[test]
    fn test_reset_replays_from_start() {
        let mut pm = PatternManager::new();
        pm.set_arrangement(arrangement(&[1.0]));
        assert_eq!(pm.tick(2.0).len(), 1);
        pm.reset();
        assert_eq!(pm.tick(2.0).len(), 1, "after reset the arrangement replays");
    }
}
