mod tests {
    use crate::*;
    use std::sync::Arc;

    #[test]
    fn sample_metadata_serialization_preserves_waveform() {
        let metadata = SampleMetadata {
            bpm: 120.0,
            transients: Arc::new(vec![0, 1024, 2048]),
            root_key: Some(60.0),
            hot_cues: [Some(0), None, None, None, None, None, None, None],
            loop_points: Some((0, 44100)),
            beat_grid_offset: 0,
            peaks: Arc::new((0..1024).map(|i| (i as f32 / 1024.0).sin().abs()).collect()),
            total_samples: 44100,
            mip_waveform: MipWaveform {
                levels: vec![Arc::new(vec![0.0, 0.5, 1.0])],
            },
            dna: SoundDNA::default(),
            midi_map: None,
        };

        let serialized = serde_json::to_string(&metadata).expect("serialize metadata");
        let deserialized: SampleMetadata = serde_json::from_str(&serialized).expect("deserialize metadata");

        assert_eq!(deserialized.bpm, 120.0);
        assert_eq!(deserialized.peaks.len(), 1024);
        assert_eq!(deserialized.mip_waveform.levels.len(), 1);
        assert_eq!(deserialized.mip_waveform.levels[0].as_slice(), &[0.0, 0.5, 1.0]);
    }

    #[test]
    fn test_mip_waveform_default() {
        let mip = MipWaveform::default();
        assert!(mip.levels.is_empty());
    }

    #[test]
    fn test_dna_transfusion_packing_roundtrip() {
        let latent = [1.0f32; 16];
        let micro_timing = [10i16; 12];
        let onset_mask = [0x1234567890ABCDEFu64; 4];

        let cmd = DnaCommand::pack_transfusion(1234, &latent, &micro_timing, &onset_mask);
        let (u_latent, u_micro_timing, u_onset_mask) = cmd.unpack_transfusion();

        assert_eq!(u_latent, latent);
        assert_eq!(u_micro_timing, micro_timing);
        assert_eq!(u_onset_mask, onset_mask);
    }

    #[test]
    fn test_dna_transfusion_hardening() {
        let mut latent = [0.0f32; 16];
        latent[0] = f32::NAN;
        latent[1] = f32::INFINITY;
        let micro_timing = [0i16; 12];
        let onset_mask = [0u64; 4];

        let cmd = DnaCommand::pack_transfusion(1234, &latent, &micro_timing, &onset_mask);
        let (u_latent, _, _) = cmd.unpack_transfusion();

        assert!(u_latent[0].is_finite());
        assert_eq!(u_latent[0], 0.0);
        assert!(u_latent[1].is_finite());
        assert_eq!(u_latent[1], 0.0);
    }

    #[test]
    fn test_binary_serialization() {
        let cmd = TimestampedCommand {
            timestamp_samples: 1234,
            command: Command::Core(CoreCommand::Play),
        };
        let binary = cmd.to_binary().unwrap();
        let decoded = TimestampedCommand::from_binary(&binary).unwrap();
        assert_eq!(cmd, decoded);

        let mut dna = SoundDNA::default();
        dna.spectral.latent_space[0] = 1.0;
        let dna_binary = dna.to_binary().unwrap();
        let dna_decoded = SoundDNA::from_binary(&dna_binary).unwrap();
        assert_eq!(dna, dna_decoded);
    }

    #[test]
    fn test_modulation_matrix_add_remove() {
        let mut matrix = ModulationMatrix::new();
        matrix.add_mapping(1, 100, 2, 0.5, 1024, Some(TemporalShape::Sine));

        assert!(matrix.mappings[0].active);
        assert_eq!(matrix.mappings[0].macro_id, 1);
        assert_eq!(matrix.mappings[0].target_id, 100);
        assert_eq!(matrix.mappings[0].param_id, 2);
        assert_eq!(matrix.mappings[0].scaling, 0.5);
        assert_eq!(matrix.mappings[0].temporal_shape, Some(TemporalShape::Sine));

        // Update existing
        matrix.add_mapping(1, 100, 2, 0.7, 512, None);
        assert!(matrix.mappings[0].active);
        assert_eq!(matrix.mappings[0].scaling, 0.7);
        assert_eq!(matrix.mappings[0].temporal_shape, None);

        // Remove
        matrix.remove_mapping(1, 100, 2);
        assert!(!matrix.mappings[0].active);
    }

    #[test]
    fn test_modulation_matrix_expansion() {
        let mut matrix = ModulationMatrix::new();
        matrix.add_mapping(1, 100, 2, 0.5, 1024, None);
        matrix.add_mapping(1, 101, 3, 2.0, 0, None);

        let mut results = Vec::new();
        matrix.expand_macro(1, 0.8, 0.0, |target, param, val, ramp| {
            results.push((target, param, val, ramp));
        });

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], (100, 2, 0.4, 1024));
        assert_eq!(results[1], (101, 3, 1.6, 0));
    }
}

