use std::sync::Arc;
use nullherz_dna::{SampleRegistry, GeneticLibrary};
use std::time::Duration;
use rayon::prelude::*;
use std::cell::RefCell;

thread_local! {
    static KERNEL: RefCell<crate::analysis_kernel::AnalysisKernel> = RefCell::new(crate::analysis_kernel::AnalysisKernel::new(44100.0));
}

pub struct AnalysisWorker {
    sample_registry: Arc<SampleRegistry>,
    library: Option<Arc<std::sync::Mutex<nullherz_dna::LibraryDatabase>>>,
    processed_ids: std::collections::HashSet<u64>,
    compatibility_matrix: std::collections::HashMap<u64, Vec<(u64, f32)>>,
    dirty_ids: std::collections::HashSet<u64>,
}

impl AnalysisWorker {
    pub fn new(sample_registry: Arc<SampleRegistry>) -> Self {
        Self {
            sample_registry,
            library: None,
            processed_ids: std::collections::HashSet::new(),
            compatibility_matrix: std::collections::HashMap::new(),
            dirty_ids: std::collections::HashSet::new(),
        }
    }

    pub fn with_library(mut self, library: Arc<std::sync::Mutex<nullherz_dna::LibraryDatabase>>) -> Self {
        self.library = Some(library);
        self
    }

    pub fn start(mut self) {
        std::thread::spawn(move || {
            loop {
                self.run_once();
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }

    fn run_once(&mut self) {
        let ids = self.sample_registry.list_ids();
        let unprocessed_ids: Vec<u64> = ids.into_iter()
            .filter(|id| !self.processed_ids.contains(id))
            .collect();

        if !unprocessed_ids.is_empty() {
            self.process_batch(unprocessed_ids);
        }

        self.update_compatibility_matrix();
    }

    fn process_batch(&mut self, unprocessed_ids: Vec<u64>) {
        println!("AnalysisWorker: Processing {} new samples in batch", unprocessed_ids.len());

        let registry = self.sample_registry.clone();
        let results: Vec<(u64, nullherz_traits::SampleMetadata, Arc<Vec<f32>>)> = unprocessed_ids.into_par_iter()
            .filter_map(|id| {
                let sample = registry.get(id)?;

                KERNEL.with(|kernel_cell| {
                    let mut kernel = kernel_cell.borrow_mut();
                    let (metadata, dna) = kernel.analyze(&sample.buffer);
                    let mut final_metadata = metadata;
                    final_metadata.dna = dna;
                    Some((id, final_metadata, sample.buffer))
                })
            }).collect();

        for (id, mut metadata, buffer) in results {
            // --- WAVEFORM MIP-MAPPING GENERATION ---
            let mut mip_levels = Vec::new();
            let mut current_peaks = metadata.peaks.clone();
            mip_levels.push(current_peaks.clone());

            // Generate 4 additional downsampled levels (total 5)
            // Using a 3-tap windowed average for smoother transitions between levels
            for _ in 0..4 {
                if current_peaks.len() <= 128 { break; }
                let mut next_level = Vec::with_capacity(current_peaks.len() / 2);
                for i in (0..current_peaks.len()).step_by(2) {
                    let prev = if i > 0 { current_peaks[i-1] } else { current_peaks[i] };
                    let curr = current_peaks[i];
                    let next = if i + 1 < current_peaks.len() { current_peaks[i+1] } else { curr };

                    // Multi-tap weighted average (0.25, 0.5, 0.25)
                    let avg = (prev * 0.25) + (curr * 0.5) + (next * 0.25);
                    next_level.push(avg);
                }
                let next_arc = Arc::new(next_level);
                mip_levels.push(next_arc.clone());
                current_peaks = next_arc;
            }
            metadata.mip_waveform.levels = mip_levels;

            self.sample_registry.register_with_metadata(id, buffer, metadata.clone());

            if let Some(ref lib_mutex) = self.library {
                let lib = lib_mutex.lock().unwrap();
                if let Ok(Some(mut track)) = lib.get_track(id) {
                    track.metadata = metadata;
                    let _ = lib.save_track(&track);
                }
            }
            self.processed_ids.insert(id);
            self.dirty_ids.insert(id);
            println!("AnalysisWorker: Enriched metadata for ID={}", id);
        }
    }

    fn update_compatibility_matrix(&mut self) {
        let Some(ref lib_mutex) = self.library else { return; };
        if self.dirty_ids.is_empty() { return; }

        let tracks = {
            let lib = lib_mutex.lock().unwrap();
            let Ok(t) = lib.list_tracks() else { return; };
            t
        };

        let dirty_list: Vec<u64> = self.dirty_ids.drain().collect();
        for id in dirty_list {
            if let Some(track) = tracks.iter().find(|t| t.id == id) {
                let compatibility = nullherz_dna::Matchmaker::rank_compatibility(&track.metadata.dna, &tracks, 10);
                self.compatibility_matrix.insert(id, compatibility);
            }
        }

        for track in &tracks {
            if !self.compatibility_matrix.contains_key(&track.id) {
                 let compatibility = nullherz_dna::Matchmaker::rank_compatibility(&track.metadata.dna, &tracks, 10);
                 self.compatibility_matrix.insert(track.id, compatibility);
            }
        }
    }
}
