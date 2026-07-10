pub mod envelope_follower;
pub mod granular;
pub mod spectral_morph;
pub mod capture;
pub mod personality_inheritance;
pub mod dna_morpher;

pub use envelope_follower::EnvelopeFollowerProcessor;
pub use granular::GranularProcessor;
pub use spectral_morph::SpectralMorphProcessor;
pub use capture::CaptureProcessor;
pub use personality_inheritance::PersonalityInheritanceProcessor;
pub use dna_morpher::DnaMorpher;

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::{SoundDNA, AudioProcessor, SignalProcessor};

    #[test]
    fn test_personality_inheritance_dna_update() {
        let mut proc = PersonalityInheritanceProcessor::new(1234, 1024);
        let mut dna = SoundDNA::default();
        dna.spectral.latent_space[0] = 0.5;

        let metadata = std::sync::Arc::new(nullherz_traits::SampleMetadata {
            dna,
            ..nullherz_traits::SampleMetadata::new_empty()
        });

        proc.apply_topology_mutation(nullherz_traits::TopologyMutation::UpdateMetadata {
            node_idx: 0,
            metadata,
        });

        assert_eq!(proc.source_personality.latent_space[0], 0.5);
    }

    #[test]
    fn test_dna_morpher_slerp() {
        let mut morpher = DnaMorpher::new(1234, 1024);

        // Orthogonal vectors to clearly test Slerp
        let mut dna_a = SoundDNA::default();
        dna_a.spectral.latent_space[0] = 1.0;

        let mut dna_b = SoundDNA::default();
        dna_b.spectral.latent_space[1] = 1.0;

        morpher.dna_a = std::sync::Arc::new(dna_a);
        morpher.dna_b = std::sync::Arc::new(dna_b);

        morpher.set_parameter(0, 0.5, 0); // morph_pos = 0.5

        // At 0.5 morph between orthogonal unit vectors, both should be equal and roughly 0.707
        // (Since Slerp preserves magnitude)
        morpher.process(&[&[0.0; 128]], &mut [&mut [0.0; 128]], &mut nullherz_traits::ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        });

        let latent = morpher.current_latent;
        assert!((latent[0] - 0.707).abs() < 1e-3);
        assert!((latent[1] - 0.707).abs() < 1e-3);
    }
}
