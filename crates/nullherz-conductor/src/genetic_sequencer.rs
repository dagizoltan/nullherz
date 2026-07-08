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

                let micro_bias = (dna.micro_timing[step % 12] as f32 / 128.0).clamp(-0.2, 0.2);
                let base_velocity = if dna_value { 0.8 } else { 0.0 };

                let syncopation = dna.syncopation_index;
                let value = if rand_val > strength {
                    if dna_value { (base_velocity + micro_bias).clamp(0.1, 1.0) } else { 0.0 }
                } else {
                    // Evolutionary drift: Create new syncopated hits based on DNA complexity
                    if rand_val < strength * syncopation * 2.0 {
                        (0.6 + micro_bias).clamp(0.1, 1.0)
                    } else {
                        0.0
                    }
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
