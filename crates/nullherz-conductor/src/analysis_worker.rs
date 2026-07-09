use nullherz_traits::SampleRegistry;
use std::sync::Arc;
use nullherz_dna::GeneticLibrary;
use std::time::Duration;
use rayon::prelude::*;
use std::cell::RefCell;

thread_local! {
    static KERNEL: RefCell<crate::analysis_kernel::AnalysisKernel> = RefCell::new(crate::analysis_kernel::AnalysisKernel::new(44100.0));
}

pub struct AnalysisWorker {
    sample_registry: Arc<dyn SampleRegistry>,
    library: Option<Arc<std::sync::Mutex<nullherz_dna::LibraryDatabase>>>,
    processed_ids: std::collections::HashSet<u64>,
    compatibility_matrix: std::collections::HashMap<u64, Vec<(u64, f32)>>,
    dirty_ids: std::collections::HashSet<u64>,
}

impl AnalysisWorker {
    pub fn new(sample_registry: Arc<dyn SampleRegistry>) -> Self {
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

    pub fn request_analysis(&mut self, id: u64) {
        self.processed_ids.remove(&id);
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
            let mip_data = audio_dsp::util::WaveformProcessor::generate_mip_levels(&metadata.peaks, 5);
            metadata.mip_waveform.levels = mip_data.into_iter().map(Arc::new).collect();

            self.sample_registry.register_with_metadata(id, buffer, Arc::new(metadata.clone()));

            if let Some(ref lib_mutex) = self.library {
                let lib = lib_mutex.lock().unwrap();
                if let Ok(Some(mut track)) = lib.get_track(id) {
                    track.metadata = Arc::new(metadata);
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
                let compatibility = nullherz_dna::Matchmaker::rank_compatibility(&(*track.metadata).dna, &tracks, 10);
                self.compatibility_matrix.insert(id, compatibility);
            }
        }

        for track in &tracks {
            if !self.compatibility_matrix.contains_key(&track.id) {
                 let compatibility = nullherz_dna::Matchmaker::rank_compatibility(&(*track.metadata).dna, &tracks, 10);
                 self.compatibility_matrix.insert(track.id, compatibility);
            }
        }
    }
}
