use crate::*;


pub struct SmartCrateManager;

impl SmartCrateManager {
    /// The single smart-crate predicate, evaluated over a track's cached
    /// `TrackFacets`. Both `filter_tracks` (full tracks) and `filter_facet_ids`
    /// (the in-memory index) go through this, so the two paths can never diverge.
    /// All eight clauses are ANDed; DNA similarity is `calculate_similarity >=
    /// threshold`, matching the previous `find_matches_above_threshold` step.
    fn facet_matches(def: &SmartCrateDefinition, f: &TrackFacets) -> bool {
        if let Some(ref target) = def.target_dna
            && calculate_similarity(target, &f.dna) < def.threshold { return false; }
        if let Some((min, max)) = def.spectral_tilt_range {
            let v = f.dna.spectral.tilt;
            if v < min || v > max { return false; }
        }
        if let Some((min, max)) = def.rhythmic_syncopation_range {
            let v = f.dna.rhythmic.syncopation_index;
            if v < min || v > max { return false; }
        }
        if let Some((min, max)) = def.glitch_density_range {
            let v = f.dna.artifacts.glitch_density;
            if v < min || v > max { return false; }
        }
        if let Some(ref genre) = def.genre
            && f.genre != *genre { return false; }
        if let Some((min, max)) = def.bpm_range
            && (f.bpm < min || f.bpm > max) { return false; }
        if let Some((min, max)) = def.energy_range
            && (f.energy_level < min || f.energy_level > max) { return false; }
        if let Some(key) = def.root_key
            && f.root_key != Some(key) { return false; }
        true
    }

    /// Filter full tracks by a smart-crate definition (public API unchanged).
    pub fn filter_tracks(def: &SmartCrateDefinition, tracks: Vec<LibraryTrack>) -> Vec<LibraryTrack> {
        tracks.into_iter().filter(|t| Self::facet_matches(def, &t.facets())).collect()
    }

    /// Filter the in-memory facet index, returning the matching track ids (the
    /// caller fetches the full tracks for just these).
    pub fn filter_facet_ids<'a>(def: &SmartCrateDefinition, facets: impl Iterator<Item = &'a TrackFacets>) -> Vec<u64> {
        facets.filter(|f| Self::facet_matches(def, f)).map(|f| f.id).collect()
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

