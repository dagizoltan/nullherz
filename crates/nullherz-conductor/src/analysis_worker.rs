// Non-RT plane (analysis worker thread): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]
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
    library: Option<Arc<parking_lot::Mutex<nullherz_dna::LibraryDatabase>>>,
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

    pub fn with_library(mut self, library: Arc<parking_lot::Mutex<nullherz_dna::LibraryDatabase>>) -> Self {
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

                // Sample buffers are PLANAR. Analyse channel 0 alone: handing
                // the kernel the whole buffer makes it read L followed by R as
                // one stream, so it detects the track twice over — doubling the
                // apparent length and corrupting BPM, transients and peaks.
                let channels = (sample.metadata.channels as usize).max(1);
                let frames = if sample.metadata.total_samples > 0 {
                    (sample.metadata.total_samples as usize).min(sample.buffer.len())
                } else {
                    sample.buffer.len() / channels
                };
                // Arc clone (refcount only) so the buffer can be borrowed for
                // analysis and still handed on without copying the samples.
                let buffer = sample.buffer.clone();
                KERNEL.with(|kernel_cell| {
                    let mut kernel = kernel_cell.borrow_mut();
                    let first_channel = buffer.get(..frames).unwrap_or(&buffer);
                    let (metadata, dna) = kernel.analyze(first_channel);
                    let mut final_metadata = metadata;
                    final_metadata.dna = dna;
                    // analyze() starts from new_empty(), which reports one
                    // channel. Letting that land would re-register a stereo
                    // sample as mono and undo the planar layout entirely.
                    final_metadata.channels = channels as u16;
                    final_metadata.total_samples = frames as u64;
                    Some((id, final_metadata, sample.buffer))
                })
            }).collect();

        let mut tracks_to_save = Vec::new();

        for (id, mut metadata, buffer) in results {
            // --- WAVEFORM MIP-MAPPING GENERATION ---
            let mip_data = audio_dsp::util::WaveformProcessor::generate_mip_levels(&metadata.peaks, 5);
            metadata.mip_waveform.levels = mip_data.into_iter().map(Arc::new).collect();

            self.sample_registry.register_with_metadata(id, buffer, Arc::new(metadata.clone()));

            tracks_to_save.push((id, metadata));
            self.processed_ids.insert(id);
            self.dirty_ids.insert(id);
        }

        if !tracks_to_save.is_empty() {
            if let Some(ref lib_mutex) = self.library {
                let lib = lib_mutex.lock();
                for (id, metadata) in tracks_to_save {
                    if let Ok(Some(mut track)) = lib.get_track(id) {
                        track.metadata = Arc::new(metadata);
                        let _ = lib.save_track(&track);
                        println!("AnalysisWorker: Enriched metadata for ID={}", id);
                    }
                }
            }
        }
    }

    fn update_compatibility_matrix(&mut self) {
        let Some(ref lib_mutex) = self.library else { return; };
        if self.dirty_ids.is_empty() { return; }

        let tracks = {
            let lib = lib_mutex.lock();
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
