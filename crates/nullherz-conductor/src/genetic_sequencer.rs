use nullherz_traits::{RhythmicDNA, Command, PerformanceCommand};

pub struct GeneticSequencer;

impl GeneticSequencer {
    pub fn evolve_pattern(
        dna: &RhythmicDNA,
        node_idx: u32,
        track_idx: u32,
        strength: f32
    ) -> Vec<Command> {
        let mut commands = Vec::new();

        // Translate RhythmicDNA Onset Mask to Sequencer Steps
        for (i, &mask) in dna.onset_mask.iter().enumerate() {
            for bit in 0..64 {
                let step = (i * 64) + bit;
                let dna_value = (mask >> bit) & 1 == 1;

                // Mutation Logic: Probability of following the DNA pattern
                // high strength = high mutation / randomization
                // low strength = closely follows DNA
                let seed = node_idx.wrapping_mul(100) + track_idx.wrapping_mul(10) + step as u32;
                let rand_val = (seed.wrapping_mul(1103515245).wrapping_add(12345) as f32) / 4294967295.0;

                let value = if rand_val > strength {
                    if dna_value { 1.0 } else { 0.0 }
                } else {
                    if rand_val > 0.8 { 0.8 } else { 0.0 } // Random trigger with high velocity
                };

                commands.push(Command::Performance(PerformanceCommand::SetSequencerStep {
                    node_idx,
                    track: track_idx,
                    step: step as u32,
                    value,
                }));
            }
        }

        commands
    }
}
