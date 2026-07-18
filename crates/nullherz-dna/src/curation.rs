use crate::*;


pub struct SmartCrateManager;

impl SmartCrateManager {
    pub fn filter_tracks(def: &SmartCrateDefinition, tracks: Vec<LibraryTrack>) -> Vec<LibraryTrack> {
        let mut results = tracks;

        // 1. Filter by DNA Similarity if target_dna is present
        if let Some(ref target) = def.target_dna {
            let matched = Matchmaker::find_matches_above_threshold(target, &results, def.threshold);
            let matched_ids: std::collections::HashSet<u64> = matched.into_iter().map(|(id, _)| id).collect();
            results.retain(|t| matched_ids.contains(&t.id));
        }

        // 2. Filter by Spectral Tilt
        if let Some((min, max)) = def.spectral_tilt_range {
            results.retain(|t| {
                let val = t.metadata.dna.spectral.tilt;
                val >= min && val <= max
            });
        }

        // 3. Filter by Rhythmic Syncopation
        if let Some((min, max)) = def.rhythmic_syncopation_range {
            results.retain(|t| {
                let val = t.metadata.dna.rhythmic.syncopation_index;
                val >= min && val <= max
            });
        }

        // 4. Filter by Glitch Density
        if let Some((min, max)) = def.glitch_density_range {
            results.retain(|t| {
                let val = t.metadata.dna.artifacts.glitch_density;
                val >= min && val <= max
            });
        }

        // 5. Filter by Genre
        if let Some(ref genre) = def.genre {
            results.retain(|t| t.genre == *genre);
        }

        // 6. Filter by BPM range
        if let Some((min, max)) = def.bpm_range {
            results.retain(|t| t.metadata.bpm >= min && t.metadata.bpm <= max);
        }

        // 7. Filter by Energy level
        if let Some((min, max)) = def.energy_range {
            results.retain(|t| t.energy_level >= min && t.energy_level <= max);
        }

        // 8. Filter by Root Key
        if let Some(key) = def.root_key {
            results.retain(|t| t.metadata.root_key == Some(key));
        }

        results
    }

    /// Automatically generates a smart crate based on "energy-level-matching" to a seed track.
    pub fn generate_energy_matched_crate(seed_track: &LibraryTrack, _all_tracks: Vec<LibraryTrack>, threshold: f32) -> SmartCrateDefinition {
        SmartCrateDefinition {
            name: format!("Energy Match: {}", seed_track.title),
            target_dna: Some(seed_track.metadata.dna.clone()),
            threshold,
            spectral_tilt_range: None,
            rhythmic_syncopation_range: None,
            glitch_density_range: None,
            genre: Some(seed_track.genre.clone()),
            bpm_range: Some((seed_track.metadata.bpm - 5.0, seed_track.metadata.bpm + 5.0)),
            energy_range: Some((seed_track.energy_level - 0.2, seed_track.energy_level + 0.2)),
            root_key: seed_track.metadata.root_key,
        }
    }
}

