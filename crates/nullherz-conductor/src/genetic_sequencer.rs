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

        // 1. Process 4 bars * 64 steps = 256 steps from the Rhythmic DNA onset mask
        for (bar_idx, &mask) in dna.onset_mask.iter().enumerate() {
            for bit in 0..64 {
                let step = (bar_idx * 64) + bit;
                let dna_onset = (mask >> bit) & 1 == 1;

                // 2. Compute deterministic pseudo-random seed per step
                let seed = node_idx.wrapping_mul(1000)
                    .wrapping_add(track_idx.wrapping_mul(100))
                    .wrapping_add(step as u32);
                let rand_val = (seed.wrapping_mul(1103515245).wrapping_add(12345) as f32) / 4294967295.0;

                // 3. Read 12-entry micro-timing deviation profile
                // Each bar has multiple steps; map step to 12-entry profile cleanly
                let micro_deviation = dna.micro_timing[step % 12];
                let micro_bias = (micro_deviation as f32 / 128.0).clamp(-0.25, 0.25);

                // 4. Procedural mutation logic
                let base_velocity = if dna_onset { 0.8 } else { 0.0 };
                let syncopation = dna.syncopation_index;

                let value = if rand_val > strength {
                    // Follow DNA closely with micro-timing bias applied as a subtle timing/velocity modifier
                    if dna_onset {
                        (base_velocity + micro_bias).clamp(0.1, 1.0)
                    } else {
                        0.0
                    }
                } else {
                    // Mutate: Evolutionary drift introduces syncopated or muted hits
                    if rand_val < strength * (syncopation + 0.1) {
                        // Insert an evolved/syncopated hit influenced by micro-timing
                        (0.5 + micro_bias).clamp(0.1, 1.0)
                    } else {
                        // Drop existing hit or leave silent
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
