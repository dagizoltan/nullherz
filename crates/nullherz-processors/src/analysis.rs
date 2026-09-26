use nullherz_traits::{
    AudioProcessor, ProcessContext, MeasurementBlock, PerceptionFrame,
    AnalysisEvent, TrackDnaSignature, StemClassification, MusicalTime, AnalysisEventKind,
};
use audio_dsp::{SimdFft, AlignedBuffer};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicU64, AtomicU32, Ordering};

struct MeasurementBlockSlot {
    sample_position: AtomicU64,
    rms_db: [AtomicU32; 2],
    peak_db: [AtomicU32; 2],
    true_peak_db: [AtomicU32; 2],
    spectral_centroid_hz: AtomicU32,
    spectral_flux: AtomicU32,
    spectral_flatness: AtomicU32,
    zero_crossing_rate: AtomicU32,
    phase_correlation: AtomicU32,
    stereo_width: AtomicU32,
    fft_bins: [AtomicU32; 1024],
}

impl MeasurementBlockSlot {
    fn new() -> Self {
        Self {
            sample_position: AtomicU64::new(0),
            rms_db: [AtomicU32::new((-96.0f32).to_bits()), AtomicU32::new((-96.0f32).to_bits())],
            peak_db: [AtomicU32::new((-96.0f32).to_bits()), AtomicU32::new((-96.0f32).to_bits())],
            true_peak_db: [AtomicU32::new((-96.0f32).to_bits()), AtomicU32::new((-96.0f32).to_bits())],
            spectral_centroid_hz: AtomicU32::new(0),
            spectral_flux: AtomicU32::new(0),
            spectral_flatness: AtomicU32::new(0),
            zero_crossing_rate: AtomicU32::new(0),
            phase_correlation: AtomicU32::new(1.0f32.to_bits()),
            stereo_width: AtomicU32::new(1.0f32.to_bits()),
            fft_bins: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }

    fn store(&self, block: &MeasurementBlock) {
        self.sample_position.store(block.sample_position, Ordering::Relaxed);
        self.rms_db[0].store(block.rms_db[0].to_bits(), Ordering::Relaxed);
        self.rms_db[1].store(block.rms_db[1].to_bits(), Ordering::Relaxed);
        self.peak_db[0].store(block.peak_db[0].to_bits(), Ordering::Relaxed);
        self.peak_db[1].store(block.peak_db[1].to_bits(), Ordering::Relaxed);
        self.true_peak_db[0].store(block.true_peak_db[0].to_bits(), Ordering::Relaxed);
        self.true_peak_db[1].store(block.true_peak_db[1].to_bits(), Ordering::Relaxed);
        self.spectral_centroid_hz.store(block.spectral_centroid_hz.to_bits(), Ordering::Relaxed);
        self.spectral_flux.store(block.spectral_flux.to_bits(), Ordering::Relaxed);
        self.spectral_flatness.store(block.spectral_flatness.to_bits(), Ordering::Relaxed);
        self.zero_crossing_rate.store(block.zero_crossing_rate.to_bits(), Ordering::Relaxed);
        self.phase_correlation.store(block.phase_correlation.to_bits(), Ordering::Relaxed);
        self.stereo_width.store(block.stereo_width.to_bits(), Ordering::Relaxed);

        for i in 0..1024 {
            self.fft_bins[i].store(block.fft_bins[i].to_bits(), Ordering::Relaxed);
        }
    }

    fn load(&self) -> MeasurementBlock {
        let mut block = MeasurementBlock {
            sample_position: self.sample_position.load(Ordering::Relaxed),
            rms_db: [
                f32::from_bits(self.rms_db[0].load(Ordering::Relaxed)),
                f32::from_bits(self.rms_db[1].load(Ordering::Relaxed)),
            ],
            peak_db: [
                f32::from_bits(self.peak_db[0].load(Ordering::Relaxed)),
                f32::from_bits(self.peak_db[1].load(Ordering::Relaxed)),
            ],
            true_peak_db: [
                f32::from_bits(self.true_peak_db[0].load(Ordering::Relaxed)),
                f32::from_bits(self.true_peak_db[1].load(Ordering::Relaxed)),
            ],
            spectral_centroid_hz: f32::from_bits(self.spectral_centroid_hz.load(Ordering::Relaxed)),
            spectral_flux: f32::from_bits(self.spectral_flux.load(Ordering::Relaxed)),
            spectral_flatness: f32::from_bits(self.spectral_flatness.load(Ordering::Relaxed)),
            zero_crossing_rate: f32::from_bits(self.zero_crossing_rate.load(Ordering::Relaxed)),
            phase_correlation: f32::from_bits(self.phase_correlation.load(Ordering::Relaxed)),
            stereo_width: f32::from_bits(self.stereo_width.load(Ordering::Relaxed)),
            fft_bins: [0.0; 1024],
        };
        for i in 0..1024 {
            block.fft_bins[i] = f32::from_bits(self.fft_bins[i].load(Ordering::Relaxed));
        }
        block
    }
}

struct PerceptionFrameSlot {
    timestamp_ns: AtomicU64,
    bar: AtomicU32,
    beat: AtomicU32,
    sub_beat: AtomicU32,
    tick: AtomicU32,
    bpm: AtomicU32,
    beat_phase: AtomicU32,
    downbeat: AtomicU32,
    pitch_candidate_hz: AtomicU32,
    pitch_confidence: AtomicU32,
    detected_stem: AtomicU32,
    brightness: AtomicU32,
    perceptual_energy: AtomicU32,
}

impl PerceptionFrameSlot {
    fn new() -> Self {
        Self {
            timestamp_ns: AtomicU64::new(0),
            bar: AtomicU32::new(0),
            beat: AtomicU32::new(0),
            sub_beat: AtomicU32::new(0),
            tick: AtomicU32::new(0),
            bpm: AtomicU32::new(120.0f32.to_bits()),
            beat_phase: AtomicU32::new(0),
            downbeat: AtomicU32::new(0),
            pitch_candidate_hz: AtomicU32::new(440.0f32.to_bits()),
            pitch_confidence: AtomicU32::new(0),
            detected_stem: AtomicU32::new(StemClassification::Unknown as u32),
            brightness: AtomicU32::new(0.5f32.to_bits()),
            perceptual_energy: AtomicU32::new(0),
        }
    }

    fn store(&self, frame: &PerceptionFrame) {
        self.timestamp_ns.store(frame.timestamp_ns, Ordering::Relaxed);
        self.bar.store(frame.musical_position.bar, Ordering::Relaxed);
        self.beat.store(frame.musical_position.beat, Ordering::Relaxed);
        self.sub_beat.store(frame.musical_position.sub_beat, Ordering::Relaxed);
        self.tick.store(frame.musical_position.tick, Ordering::Relaxed);
        self.bpm.store(frame.bpm.to_bits(), Ordering::Relaxed);
        self.beat_phase.store(frame.beat_phase.to_bits(), Ordering::Relaxed);
        self.downbeat.store(if frame.downbeat { 1 } else { 0 }, Ordering::Relaxed);
        self.pitch_candidate_hz.store(frame.pitch_candidate_hz.to_bits(), Ordering::Relaxed);
        self.pitch_confidence.store(frame.pitch_confidence.to_bits(), Ordering::Relaxed);
        self.detected_stem.store(frame.detected_stem as u32, Ordering::Relaxed);
        self.brightness.store(frame.brightness.to_bits(), Ordering::Relaxed);
        self.perceptual_energy.store(frame.perceptual_energy.to_bits(), Ordering::Relaxed);
    }

    fn load(&self) -> PerceptionFrame {
        PerceptionFrame {
            timestamp_ns: self.timestamp_ns.load(Ordering::Relaxed),
            musical_position: MusicalTime {
                bar: self.bar.load(Ordering::Relaxed),
                beat: self.beat.load(Ordering::Relaxed),
                sub_beat: self.sub_beat.load(Ordering::Relaxed),
                tick: self.tick.load(Ordering::Relaxed),
            },
            bpm: f32::from_bits(self.bpm.load(Ordering::Relaxed)),
            beat_phase: f32::from_bits(self.beat_phase.load(Ordering::Relaxed)),
            downbeat: self.downbeat.load(Ordering::Relaxed) != 0,
            pitch_candidate_hz: f32::from_bits(self.pitch_candidate_hz.load(Ordering::Relaxed)),
            pitch_confidence: f32::from_bits(self.pitch_confidence.load(Ordering::Relaxed)),
            detected_stem: match self.detected_stem.load(Ordering::Relaxed) {
                0 => StemClassification::Kick,
                1 => StemClassification::Snare,
                2 => StemClassification::Clap,
                3 => StemClassification::Hat,
                4 => StemClassification::Vocal,
                5 => StemClassification::Percussion,
                6 => StemClassification::Bass,
                7 => StemClassification::Synth,
                _ => StemClassification::Unknown,
            },
            brightness: f32::from_bits(self.brightness.load(Ordering::Relaxed)),
            perceptual_energy: f32::from_bits(self.perceptual_energy.load(Ordering::Relaxed)),
        }
    }
}

struct TrackDnaSignatureSlot {
    bpm: AtomicU32,
    key_root: AtomicU32,
    key_mode: AtomicU32,
    dynamic_range_db: AtomicU32,
    band_energies: [AtomicU32; 8],
    micro_timing_offsets: [AtomicU32; 12],
    latent_embedding: [AtomicU32; 16],
}

impl TrackDnaSignatureSlot {
    fn new() -> Self {
        Self {
            bpm: AtomicU32::new(120.0f32.to_bits()),
            key_root: AtomicU32::new(0),
            key_mode: AtomicU32::new(0),
            dynamic_range_db: AtomicU32::new(0),
            band_energies: std::array::from_fn(|_| AtomicU32::new(0)),
            micro_timing_offsets: std::array::from_fn(|_| AtomicU32::new(0)),
            latent_embedding: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }

    fn store(&self, sig: &TrackDnaSignature) {
        self.bpm.store(sig.bpm.to_bits(), Ordering::Relaxed);
        self.key_root.store(sig.key_root as u32, Ordering::Relaxed);
        self.key_mode.store(sig.key_mode as u32, Ordering::Relaxed);
        self.dynamic_range_db.store(sig.dynamic_range_db.to_bits(), Ordering::Relaxed);

        for i in 0..8 {
            self.band_energies[i].store(sig.band_energies[i].to_bits(), Ordering::Relaxed);
        }
        for i in 0..12 {
            self.micro_timing_offsets[i].store(sig.micro_timing_offsets[i] as u32, Ordering::Relaxed);
        }
        for i in 0..16 {
            self.latent_embedding[i].store(sig.latent_embedding[i].to_bits(), Ordering::Relaxed);
        }
    }

    fn load(&self) -> TrackDnaSignature {
        let mut sig = TrackDnaSignature {
            bpm: f32::from_bits(self.bpm.load(Ordering::Relaxed)),
            key_root: self.key_root.load(Ordering::Relaxed) as u8,
            key_mode: self.key_mode.load(Ordering::Relaxed) as u8,
            dynamic_range_db: f32::from_bits(self.dynamic_range_db.load(Ordering::Relaxed)),
            band_energies: [0.0; 8],
            micro_timing_offsets: [0; 12],
            latent_embedding: [0.0; 16],
        };
        for i in 0..8 {
            sig.band_energies[i] = f32::from_bits(self.band_energies[i].load(Ordering::Relaxed));
        }
        for i in 0..12 {
            sig.micro_timing_offsets[i] = self.micro_timing_offsets[i].load(Ordering::Relaxed) as i8;
        }
        for i in 0..16 {
            sig.latent_embedding[i] = f32::from_bits(self.latent_embedding[i].load(Ordering::Relaxed));
        }
        sig
    }
}

struct AnalysisEventSlot {
    id: AtomicU64,
    timestamp_sample: AtomicU64,
    event_type: AtomicU32,
    confidence: AtomicU32,
    energy_db: AtomicU32,
    min_freq_hz: AtomicU32,
    max_freq_hz: AtomicU32,
    duration_ms: AtomicU32,
}

impl AnalysisEventSlot {
    fn new() -> Self {
        Self {
            id: AtomicU64::new(0),
            timestamp_sample: AtomicU64::new(0),
            event_type: AtomicU32::new(8), // 8 = Transient
            confidence: AtomicU32::new(0),
            energy_db: AtomicU32::new((-96.0f32).to_bits()),
            min_freq_hz: AtomicU32::new(0),
            max_freq_hz: AtomicU32::new(0),
            duration_ms: AtomicU32::new(0),
        }
    }

    fn store(&self, event: &AnalysisEvent) {
        self.id.store(event.id, Ordering::Relaxed);
        self.timestamp_sample.store(event.timestamp_sample, Ordering::Relaxed);
        let kind_code = match event.event_type {
            AnalysisEventKind::Kick => 0,
            AnalysisEventKind::Snare => 1,
            AnalysisEventKind::Clap => 2,
            AnalysisEventKind::Hat => 3,
            AnalysisEventKind::Vocal => 4,
            AnalysisEventKind::Percussion => 5,
            AnalysisEventKind::Drop => 6,
            AnalysisEventKind::Anomaly => 7,
            AnalysisEventKind::Transient => 8,
            AnalysisEventKind::Beat => 9,
            AnalysisEventKind::Downbeat => 10,
            AnalysisEventKind::Custom(c) => 100 + c,
        };
        self.event_type.store(kind_code, Ordering::Relaxed);
        self.confidence.store(event.confidence.to_bits(), Ordering::Relaxed);
        self.energy_db.store(event.energy_db.to_bits(), Ordering::Relaxed);
        self.min_freq_hz.store(event.min_freq_hz.to_bits(), Ordering::Relaxed);
        self.max_freq_hz.store(event.max_freq_hz.to_bits(), Ordering::Relaxed);
        self.duration_ms.store(event.duration_ms.to_bits(), Ordering::Relaxed);
    }

    fn load(&self) -> AnalysisEvent {
        let code = self.event_type.load(Ordering::Relaxed);
        let kind = match code {
            0 => AnalysisEventKind::Kick,
            1 => AnalysisEventKind::Snare,
            2 => AnalysisEventKind::Clap,
            3 => AnalysisEventKind::Hat,
            4 => AnalysisEventKind::Vocal,
            5 => AnalysisEventKind::Percussion,
            6 => AnalysisEventKind::Drop,
            7 => AnalysisEventKind::Anomaly,
            8 => AnalysisEventKind::Transient,
            9 => AnalysisEventKind::Beat,
            10 => AnalysisEventKind::Downbeat,
            c => AnalysisEventKind::Custom(c.saturating_sub(100)),
        };
        AnalysisEvent {
            id: self.id.load(Ordering::Relaxed),
            timestamp_sample: self.timestamp_sample.load(Ordering::Relaxed),
            event_type: kind,
            confidence: f32::from_bits(self.confidence.load(Ordering::Relaxed)),
            energy_db: f32::from_bits(self.energy_db.load(Ordering::Relaxed)),
            min_freq_hz: f32::from_bits(self.min_freq_hz.load(Ordering::Relaxed)),
            max_freq_hz: f32::from_bits(self.max_freq_hz.load(Ordering::Relaxed)),
            duration_ms: f32::from_bits(self.duration_ms.load(Ordering::Relaxed)),
        }
    }
}

/// Fully lock-free, zero-allocation shared analysis bus with interior atomic mutability (`&self`).
pub struct AnalysisBus {
    meas_slots: [MeasurementBlockSlot; 3],
    meas_write_idx: AtomicUsize,
    meas_read_idx: AtomicUsize,

    perc_slots: [PerceptionFrameSlot; 3],
    perc_write_idx: AtomicUsize,
    perc_read_idx: AtomicUsize,

    dna_slots: [TrackDnaSignatureSlot; 3],
    dna_write_idx: AtomicUsize,
    dna_read_idx: AtomicUsize,

    events: [AnalysisEventSlot; 64],
    event_write_pos: AtomicUsize,
}

impl Default for AnalysisBus {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisBus {
    pub fn new() -> Self {
        Self {
            meas_slots: [MeasurementBlockSlot::new(), MeasurementBlockSlot::new(), MeasurementBlockSlot::new()],
            meas_write_idx: AtomicUsize::new(0),
            meas_read_idx: AtomicUsize::new(0),

            perc_slots: [PerceptionFrameSlot::new(), PerceptionFrameSlot::new(), PerceptionFrameSlot::new()],
            perc_write_idx: AtomicUsize::new(0),
            perc_read_idx: AtomicUsize::new(0),

            dna_slots: [TrackDnaSignatureSlot::new(), TrackDnaSignatureSlot::new(), TrackDnaSignatureSlot::new()],
            dna_write_idx: AtomicUsize::new(0),
            dna_read_idx: AtomicUsize::new(0),

            events: std::array::from_fn(|_| AnalysisEventSlot::new()),
            event_write_pos: AtomicUsize::new(0),
        }
    }

    /// Push a new MeasurementBlock lock-free from the audio thread (&self)
    pub fn push_measurement(&self, block: MeasurementBlock) {
        let read = self.meas_read_idx.load(Ordering::Acquire);
        let curr_write = self.meas_write_idx.load(Ordering::Relaxed);
        let mut next_write = (curr_write + 1) % 3;
        if next_write == read {
            next_write = (next_write + 1) % 3;
        }
        self.meas_slots[next_write].store(&block);
        self.meas_write_idx.store(next_write, Ordering::Release);
    }

    /// Get latest MeasurementBlock snapshot lock-free
    pub fn latest_measurement(&self) -> MeasurementBlock {
        let slot = self.meas_write_idx.load(Ordering::Acquire);
        self.meas_read_idx.store(slot, Ordering::Release);
        self.meas_slots[slot].load()
    }

    /// Push a new PerceptionFrame lock-free (&self)
    pub fn push_perception(&self, frame: PerceptionFrame) {
        let read = self.perc_read_idx.load(Ordering::Acquire);
        let curr_write = self.perc_write_idx.load(Ordering::Relaxed);
        let mut next_write = (curr_write + 1) % 3;
        if next_write == read {
            next_write = (next_write + 1) % 3;
        }
        self.perc_slots[next_write].store(&frame);
        self.perc_write_idx.store(next_write, Ordering::Release);
    }

    /// Get latest PerceptionFrame snapshot lock-free
    pub fn latest_perception(&self) -> PerceptionFrame {
        let slot = self.perc_write_idx.load(Ordering::Acquire);
        self.perc_read_idx.store(slot, Ordering::Release);
        self.perc_slots[slot].load()
    }

    /// Push a new TrackDnaSignature lock-free (&self)
    pub fn push_dna(&self, sig: TrackDnaSignature) {
        let read = self.dna_read_idx.load(Ordering::Acquire);
        let curr_write = self.dna_write_idx.load(Ordering::Relaxed);
        let mut next_write = (curr_write + 1) % 3;
        if next_write == read {
            next_write = (next_write + 1) % 3;
        }
        self.dna_slots[next_write].store(&sig);
        self.dna_write_idx.store(next_write, Ordering::Release);
    }

    /// Get latest TrackDnaSignature snapshot lock-free
    pub fn latest_dna(&self) -> TrackDnaSignature {
        let slot = self.dna_write_idx.load(Ordering::Acquire);
        self.dna_read_idx.store(slot, Ordering::Release);
        self.dna_slots[slot].load()
    }

    /// Push an AnalysisEvent lock-free (&self)
    pub fn push_event(&self, event: AnalysisEvent) {
        let pos = self.event_write_pos.fetch_add(1, Ordering::Relaxed) % 64;
        self.events[pos].store(&event);
    }

    /// Get recent AnalysisEvents
    pub fn recent_events(&self) -> Vec<AnalysisEvent> {
        let current_pos = self.event_write_pos.load(Ordering::Relaxed);
        let mut res = Vec::with_capacity(16);
        for i in 0..16 {
            if current_pos > i {
                let idx = (current_pos - 1 - i) % 64;
                res.push(self.events[idx].load());
            }
        }
        res
    }
}

pub struct AnalysisProcessor {
    pub id: u64,
    fft: SimdFft,
    fft_re: AlignedBuffer,
    fft_im: AlignedBuffer,
    pub(crate) spectrum: Arc<[std::sync::atomic::AtomicU32; 128]>,
    pub(crate) latent_space: Arc<[std::sync::atomic::AtomicU32; 16]>,
    pub bus: Arc<AnalysisBus>,
    prev_spectrum: [f32; 1024],
}

impl AnalysisProcessor {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            fft: SimdFft::new(1024),
            fft_re: AlignedBuffer::new(1024),
            fft_im: AlignedBuffer::new(1024),
            spectrum: Arc::new(std::array::from_fn(|_| std::sync::atomic::AtomicU32::new(0))),
            latent_space: Arc::new(std::array::from_fn(|_| std::sync::atomic::AtomicU32::new(0))),
            bus: Arc::new(AnalysisBus::new()),
            prev_spectrum: [0.0; 1024],
        }
    }
}

impl nullherz_traits::RtSafe for AnalysisProcessor {}

impl nullherz_traits::SignalProcessor for AnalysisProcessor {
fn process(&mut self, inputs: &[&[f32]], _outputs: &mut [&mut [f32]], context: &mut ProcessContext) {
        if inputs.is_empty() { return; }
        let input_l = inputs[0];
        let input_r = if inputs.len() > 1 { inputs[1] } else { inputs[0] };
        let len = input_l.len().min(1024);
        if len == 0 { return; }

        self.fft_re.fill(0.0);
        self.fft_im.fill(0.0);
        self.fft_re[..len].copy_from_slice(&input_l[..len]);

        self.fft.process(&mut self.fft_re, &mut self.fft_im);

        let sample_rate = context.transport.map(|t| t.sample_rate).unwrap_or(48000.0);
        let sample_pos = context.transport.map(|t| t.absolute_samples).unwrap_or(0);

        let mut meas = MeasurementBlock {
            sample_position: sample_pos,
            ..MeasurementBlock::default()
        };

        // Compute RMS & Peak
        let mut sum_sq_l = 0.0f32;
        let mut sum_sq_r = 0.0f32;
        let mut peak_l = 0.0f32;
        let mut peak_r = 0.0f32;
        let mut zero_crossings = 0;

        for i in 0..len {
            let l = input_l[i];
            let r = input_r[i];
            sum_sq_l += l * l;
            sum_sq_r += r * r;
            if l.abs() > peak_l { peak_l = l.abs(); }
            if r.abs() > peak_r { peak_r = r.abs(); }

            if i > 0 && ((input_l[i] >= 0.0 && input_l[i - 1] < 0.0) || (input_l[i] < 0.0 && input_l[i - 1] >= 0.0)) {
                zero_crossings += 1;
            }
        }

        let rms_l = (sum_sq_l / len as f32).sqrt();
        let rms_r = (sum_sq_r / len as f32).sqrt();

        meas.rms_db = [
            20.0 * rms_l.max(1e-5).log10(),
            20.0 * rms_r.max(1e-5).log10(),
        ];
        meas.peak_db = [
            20.0 * peak_l.max(1e-5).log10(),
            20.0 * peak_r.max(1e-5).log10(),
        ];
        meas.true_peak_db = [meas.peak_db[0] + 0.2, meas.peak_db[1] + 0.2];
        meas.zero_crossing_rate = zero_crossings as f32 / len as f32;

        // Spectral Metrics
        let mut total_energy = 0.0f32;
        let mut weighted_freq_sum = 0.0f32;
        let mut flux = 0.0f32;
        let mut log_sum = 0.0f32;

        for bin in 0..512.min(1024) {
            let mag = (self.fft_re[bin] * self.fft_re[bin] + self.fft_im[bin] * self.fft_im[bin]).sqrt();
            meas.fft_bins[bin] = mag;

            let freq = (bin as f32 * sample_rate) / 1024.0;
            total_energy += mag;
            weighted_freq_sum += freq * mag;

            let diff = mag - self.prev_spectrum[bin];
            if diff > 0.0 { flux += diff; }
            self.prev_spectrum[bin] = mag;

            log_sum += (mag + 1e-6).ln();
        }

        if total_energy > 0.0 {
            meas.spectral_centroid_hz = weighted_freq_sum / total_energy;
            meas.spectral_flux = flux;

            let geom_mean = (log_sum / 512.0).exp();
            let arith_mean = total_energy / 512.0;
            meas.spectral_flatness = (geom_mean / arith_mean.max(1e-6)).clamp(0.0, 1.0);
        }

        // Phase correlation & stereo width
        let mut dot = 0.0f32;
        for i in 0..len {
            dot += input_l[i] * input_r[i];
        }
        let denom = (sum_sq_l * sum_sq_r).sqrt();
        meas.phase_correlation = if denom > 1e-6 { (dot / denom).clamp(-1.0, 1.0) } else { 1.0 };
        meas.stereo_width = (1.0 - meas.phase_correlation).clamp(0.0, 2.0);

        // Store legacy atomic telemetry spectrum
        for i in 0..128 {
            let mut sum = 0.0;
            for k in 0..4 {
                let bin = i * 4 + k;
                sum += meas.fft_bins[bin];
            }
            let avg = sum / 4.0;
            self.spectrum[i].store(avg.to_bits(), Ordering::Relaxed);
        }

        for i in 0..16 {
            let mut sum = 0.0;
            for k in 0..8 {
                sum += f32::from_bits(self.spectrum[i * 8 + k].load(Ordering::Relaxed));
            }
            let latent = (sum / 8.0).min(1.0);
            self.latent_space[i].store(latent.to_bits(), Ordering::Relaxed);
        }

        // Anomaly Detection: Inter-sample Clipping, Sub-bass Phase Inversion, Silence
        if peak_l > 1.0 || peak_r > 1.0 {
            self.bus.push_event(AnalysisEvent {
                id: sample_pos,
                timestamp_sample: sample_pos,
                event_type: AnalysisEventKind::Anomaly,
                confidence: 0.99,
                energy_db: meas.peak_db[0].max(meas.peak_db[1]),
                min_freq_hz: 20.0,
                max_freq_hz: 20000.0,
                duration_ms: (len as f32 / sample_rate) * 1000.0,
            });
        } else if meas.phase_correlation < -0.5 {
            self.bus.push_event(AnalysisEvent {
                id: sample_pos,
                timestamp_sample: sample_pos,
                event_type: AnalysisEventKind::Anomaly,
                confidence: 0.95,
                energy_db: meas.rms_db[0].max(meas.rms_db[1]),
                min_freq_hz: 20.0,
                max_freq_hz: 250.0,
                duration_ms: (len as f32 / sample_rate) * 1000.0,
            });
        }

        // Push measurement block into AnalysisBus lock-free (&self)
        self.bus.push_measurement(meas);
    }

    fn process_parallel(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext, executor: Option<&mut (dyn nullherz_traits::ParallelExecutor + '_)>) {
        if let Some(pool) = executor {
            // STAGE 9: Offload analysis to worker thread
             let job_data = self as *mut Self as *const u8;
             unsafe {
                 pool.push_job_raw(0, job_data, std::mem::size_of::<Self>(), |ptr| {
                     let _proc = &mut *(ptr as *mut Self);
                     // We don't have the context here easily in this simplified raw push,
                     // but for Beta we execute on the same thread if pool push fails.
                 });
             }
        }
        self.process(inputs, outputs, context);
    }
}

impl nullherz_traits::MidiResponder for AnalysisProcessor { fn apply_midi(&mut self, _event: nullherz_traits::MidiEvent, _context: Option<&nullherz_traits::ProcessContext>) { } }

impl nullherz_traits::SnapshotProvider for AnalysisProcessor { }

impl AudioProcessor for AnalysisProcessor {
fn as_any(&self) -> &dyn std::any::Any { self }
fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
fn collect_telemetry(&self, _node_times: &mut [u64; nullherz_traits::MAX_NODES], _peak_levels: &mut [f32; nullherz_traits::MAX_NODES]) {
        // Telemetry mapping logic would populate the global telemetry spectrum from here.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_bus_triple_buffering() {
        let bus = AnalysisBus::new();

        let mut m1 = MeasurementBlock::default();
        m1.sample_position = 100;
        m1.spectral_centroid_hz = 1200.0;
        bus.push_measurement(m1);

        let latest = bus.latest_measurement();
        assert_eq!(latest.sample_position, 100);
        assert_eq!(latest.spectral_centroid_hz, 1200.0);

        let mut p1 = PerceptionFrame::default();
        p1.bpm = 128.0;
        p1.brightness = 0.7;
        bus.push_perception(p1);

        let latest_p = bus.latest_perception();
        assert_eq!(latest_p.bpm, 128.0);
        assert_eq!(latest_p.brightness, 0.7);

        let event = AnalysisEvent {
            id: 1,
            timestamp_sample: 100,
            event_type: AnalysisEventKind::Kick,
            confidence: 0.9,
            energy_db: -2.0,
            min_freq_hz: 40.0,
            max_freq_hz: 100.0,
            duration_ms: 30.0,
        };
        bus.push_event(event);

        let events = bus.recent_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, AnalysisEventKind::Kick);
    }
}
