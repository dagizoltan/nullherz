// Non-RT plane (analysis worker thread): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]
#![allow(clippy::collapsible_if)]
use nullherz_traits::SampleRegistry;
use std::sync::Arc;
use nullherz_dna::GeneticLibrary;
use std::time::Duration;
use rayon::prelude::*;
use std::cell::RefCell;

thread_local! {
    // Placeholder only: `set_sample_rate` below stamps the analysed sample's
    // own source rate before every `analyze()` call, so this value is never the
    // one any result is computed against.
    static KERNEL: RefCell<crate::analysis_kernel::AnalysisKernel> =
        RefCell::new(crate::analysis_kernel::AnalysisKernel::new(nullherz_traits::DEFAULT_SAMPLE_RATE));
}

pub struct AnalysisWorker {
    sample_registry: Arc<dyn SampleRegistry>,
    library: Option<Arc<parking_lot::Mutex<nullherz_dna::LibraryDatabase>>>,
    /// Ids analysis is FINISHED with, shared with the conductor.
    ///
    /// Shared rather than owned because `start()` moves the worker onto its own
    /// thread (`conductor.analysis_worker.take()`), so a plain field here is
    /// invisible to the reaper exactly when it matters. An `Arc` survives the
    /// move; the conductor holds the other end from construction.
    processed_ids: Arc<parking_lot::Mutex<std::collections::HashSet<u64>>>,
    compatibility_matrix: std::collections::HashMap<u64, Vec<(u64, f32)>>,
    dirty_ids: std::collections::HashSet<u64>,
}

impl AnalysisWorker {
    pub fn new(sample_registry: Arc<dyn SampleRegistry>) -> Self {
        Self {
            sample_registry,
            library: None,
            processed_ids: Arc::new(parking_lot::Mutex::new(std::collections::HashSet::new())),
            compatibility_matrix: std::collections::HashMap::new(),
            dirty_ids: std::collections::HashSet::new(),
        }
    }

    pub fn with_library(mut self, library: Arc<parking_lot::Mutex<nullherz_dna::LibraryDatabase>>) -> Self {
        self.library = Some(library);
        self
    }

    pub fn request_analysis(&mut self, id: u64) {
        self.processed_ids.lock().remove(&id);
    }

    pub fn start(mut self) {
        std::thread::spawn(move || {
            loop {
                self.run_once();
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }

    /// A handle to the set of ids analysis is finished with.
    ///
    /// The registry reaper needs this. The scanner registers a track's full
    /// decoded audio PRECISELY as the hand-off to analysis, so a reaper that
    /// only asks "is it on a deck?" evicts tracks out of the queue before this
    /// worker sees them — observed live as "Hydrated registry for X" followed
    /// immediately by "released 1 sample, 121 MB", and the reason reaping was
    /// off by default.
    ///
    /// Returns the shared handle rather than answering a query, because
    /// `start()` moves the worker to its own thread and the conductor keeps its
    /// end from construction.
    pub fn analysed_ids(&self) -> Arc<parking_lot::Mutex<std::collections::HashSet<u64>>> {
        self.processed_ids.clone()
    }

    fn run_once(&mut self) {
        let ids = self.sample_registry.list_ids();
        let unprocessed_ids: Vec<u64> = ids.into_iter()
            .filter(|id| !self.processed_ids.lock().contains(id))
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
                // The kernel is a thread_local built once at 44.1 kHz; point it
                // at THIS sample's rate before analysing. Otherwise a 48 kHz
                // file reports a BPM 8.8% low, splits its bands at the wrong
                // frequencies, and resolves the wrong key — every derived value
                // in the kernel is scaled by the rate.
                let source_rate = sample.metadata.sample_rate;
                KERNEL.with(|kernel_cell| {
                    let mut kernel = kernel_cell.borrow_mut();
                    kernel.set_sample_rate(source_rate as f32);
                    let first_channel = buffer.get(..frames).unwrap_or(&buffer);
                    let (metadata, dna) = kernel.analyze(first_channel);
                    let mut final_metadata = metadata;
                    final_metadata.dna = dna;
                    // analyze() starts from new_empty(), which reports one
                    // channel. Letting that land would re-register a stereo
                    // sample as mono and undo the planar layout entirely.
                    final_metadata.channels = channels as u16;
                    final_metadata.total_samples = frames as u64;
                    final_metadata.sample_rate = source_rate;
                    Some((id, final_metadata, sample.buffer))
                })
            }).collect();

        let mut tracks_to_save = Vec::new();

        for (id, mut metadata, buffer) in results {
            // --- WAVEFORM MIP-MAPPING GENERATION ---
            // 8 levels: with the denser base resolution (128 samples/peak) a
            // long track needs deeper /2 downsampling before the renderer's
            // ~2-peaks-per-pixel LOD target is reachable.
            let mip_data = audio_dsp::util::WaveformProcessor::generate_mip_levels(&metadata.peaks, 8);
            metadata.mip_waveform.levels = mip_data.into_iter().map(Arc::new).collect();

            self.sample_registry.register_with_metadata(id, buffer, Arc::new(metadata.clone()));

            tracks_to_save.push((id, metadata));
            self.processed_ids.lock().insert(id);
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
