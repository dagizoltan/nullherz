use crate::*;


pub struct Matchmaker;

impl Matchmaker {
    pub fn rank_compatibility(target: &nullherz_traits::SoundDNA, candidates: &[LibraryTrack], limit: usize) -> Vec<(u64, f32)> {
        use rayon::prelude::*;
        let mut scores: Vec<(u64, f32)> = candidates.par_iter()
            .map(|track| {
                let score = calculate_similarity(target, &track.metadata.dna);
                (track.id, score)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(limit);
        scores
    }

    pub fn find_matches_above_threshold(target: &nullherz_traits::SoundDNA, candidates: &[LibraryTrack], threshold: f32) -> Vec<(u64, f32)> {
        use rayon::prelude::*;
        let mut results: Vec<(u64, f32)> = candidates.par_iter()
            .filter_map(|track| {
                let score = calculate_similarity(target, &track.metadata.dna);
                if score >= threshold {
                    Some((track.id, score))
                } else {
                    None
                }
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    pub fn find_best_matches(db: &LibraryDatabase, target: &nullherz_traits::SoundDNA, limit: usize) -> Result<Vec<(u64, f32)>, Box<dyn std::error::Error>> {
        let tracks = db.list_tracks()?;
        Ok(Self::rank_compatibility(target, &tracks, limit))
    }
}
