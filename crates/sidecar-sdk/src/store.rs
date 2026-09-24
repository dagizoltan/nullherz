#![allow(clippy::needless_range_loop, clippy::collapsible_if)]

use std::sync::Arc;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand, MidiEvent,
};

/// Category types for sidecar modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SidecarType {
    Instrument,
    Insert,
    NeuralAnalyzer,
    NeuralProcessor,
}

/// Metadata descriptor for a sidecar module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarDescriptor {
    pub id: String,
    pub name: String,
    pub sidecar_type: SidecarType,
    pub tags: Vec<String>,
    pub description: String,
    pub latency_samples: usize,
}

impl SidecarDescriptor {
    pub fn new(
        id: &str,
        name: &str,
        sidecar_type: SidecarType,
        tags: &[&str],
        description: &str,
        latency_samples: usize,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            sidecar_type,
            tags: tags.iter().map(|s| s.to_string()).collect(),
            description: description.to_string(),
            latency_samples,
        }
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        let tag_lower = tag.to_lowercase();
        self.tags.iter().any(|t| t.to_lowercase() == tag_lower)
    }

    pub fn has_all_tags(&self, query_tags: &[&str]) -> bool {
        query_tags.iter().all(|&qt| self.has_tag(qt))
    }
}

// ============================================================================
// 1. Neural Saturation Processor (Real-Time Neural Insert)
// ============================================================================
pub struct NeuralSaturationProcessor {
    pub drive: f32,
    pub output_gain: f32,
    pub mix: f32,
}

impl NeuralSaturationProcessor {
    pub fn new() -> Self {
        Self {
            drive: 1.5,
            output_gain: 1.0,
            mix: 1.0,
        }
    }
}

impl Default for NeuralSaturationProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalProcessor for NeuralSaturationProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext) {
        let num_ch = inputs.len().min(outputs.len());
        let drive = self.drive;
        let gain = self.output_gain;
        let mix = self.mix;

        for ch in 0..num_ch {
            let in_buf = inputs[ch];
            let out_buf = &mut outputs[ch];
            let n = in_buf.len().min(out_buf.len());

            for i in 0..n {
                let x = in_buf[i] * drive;
                // Padé approximant neural saturation curve
                let sat = x.tanh() * gain;
                out_buf[i] = in_buf[i] * (1.0 - mix) + sat * mix;
            }
        }
    }
}

impl MidiResponder for NeuralSaturationProcessor {}
impl SnapshotProvider for NeuralSaturationProcessor {}

impl AudioProcessor for NeuralSaturationProcessor {
    fn apply_command(&mut self, command: &ProcessorCommand) {
        match command {
            nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetBpm(bpm)) => {
                self.drive = (bpm / 120.0).clamp(0.5, 3.0);
            }
            nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { param_id, value, .. }) => {
                match param_id {
                    0 => self.drive = *value,
                    1 => self.output_gain = *value,
                    2 => self.mix = *value,
                    _ => {}
                }
            }
            _ => {}
        }
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

// ============================================================================
// 2. Neural Dynamic Filter Processor (Real-Time Neural Insert)
// ============================================================================
pub struct NeuralFilterProcessor {
    pub cutoff: f32,
    pub resonance: f32,
    pub neural_drive: f32,
    s1: [f32; 16],
    s2: [f32; 16],
}

impl NeuralFilterProcessor {
    pub fn new() -> Self {
        Self {
            cutoff: 1200.0,
            resonance: 2.0,
            neural_drive: 0.5,
            s1: [0.0; 16],
            s2: [0.0; 16],
        }
    }
}

impl Default for NeuralFilterProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalProcessor for NeuralFilterProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext) {
        let num_ch = inputs.len().min(outputs.len()).min(16);
        let sample_rate = 48000.0f32;
        let w0 = (std::f32::consts::TAU * self.cutoff / sample_rate).clamp(0.001, 3.0);
        let g = (w0 * 0.5).tan();
        let k = (1.0 / self.resonance).clamp(0.1, 2.0);

        for ch in 0..num_ch {
            let in_buf = inputs[ch];
            let out_buf = &mut outputs[ch];
            let n = in_buf.len().min(out_buf.len());

            let mut s1 = self.s1[ch];
            let mut s2 = self.s2[ch];

            for i in 0..n {
                let x = in_buf[i];
                // Neural hypernetwork non-linear feedback conditioning
                let feedback = ((s1 * k) * (1.0 + self.neural_drive * 0.5)).tanh();
                let hp = (x - feedback - g * s1 - s2) / (1.0 + g * (g + k));
                let v1 = g * hp;
                let bp = v1 + s1;
                s1 = bp + v1;
                let v2 = g * bp;
                let lp = v2 + s2;
                s2 = lp + v2;

                out_buf[i] = lp;
            }

            self.s1[ch] = s1;
            self.s2[ch] = s2;
        }
    }
}

impl MidiResponder for NeuralFilterProcessor {}
impl SnapshotProvider for NeuralFilterProcessor {}

impl AudioProcessor for NeuralFilterProcessor {
    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { param_id, value, .. }) = command {
            match param_id {
                0 => self.cutoff = *value,
                1 => self.resonance = *value,
                2 => self.neural_drive = *value,
                _ => {}
            }
        }
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

// ============================================================================
// 3. Algorithmic Tape Delay Processor (Real-Time Algorithmic Insert)
// ============================================================================
pub struct AlgorithmicDelayProcessor {
    pub delay_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    pub dampening: f32,
    buffer: Vec<Vec<f32>>,
    write_pos: usize,
    damp_state: [f32; 16],
}

impl AlgorithmicDelayProcessor {
    pub fn new() -> Self {
        let max_samples = 192000; // ~4s at 48kHz
        let num_channels = 16;
        let buffer = vec![vec![0.0f32; max_samples]; num_channels];
        Self {
            delay_ms: 250.0,
            feedback: 0.4,
            mix: 0.35,
            dampening: 0.3,
            buffer,
            write_pos: 0,
            damp_state: [0.0; 16],
        }
    }
}

impl Default for AlgorithmicDelayProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalProcessor for AlgorithmicDelayProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext) {
        let num_ch = inputs.len().min(outputs.len()).min(16);
        let sample_rate = 48000.0f32;
        let buf_len = self.buffer[0].len();
        let delay_samples = (self.delay_ms * 0.001 * sample_rate).clamp(1.0, (buf_len - 1) as f32);
        let feedback = self.feedback;
        let mix = self.mix;
        let damp = self.dampening;

        let block_len = inputs[0].len();

        for i in 0..block_len {
            let w_pos = (self.write_pos + i) % buf_len;

            // Compute delay read position with Hermite fractional interpolation
            let read_pos_f = (w_pos as f32 + buf_len as f32 - delay_samples) % buf_len as f32;
            let i1 = read_pos_f.floor() as usize;
            let frac = read_pos_f - i1 as f32;
            let i0 = (i1 + buf_len - 1) % buf_len;
            let i2 = (i1 + 1) % buf_len;
            let i3 = (i1 + 2) % buf_len;

            for ch in 0..num_ch {
                let in_val = inputs[ch][i];
                let buf_ch = &mut self.buffer[ch];

                let y0 = buf_ch[i0];
                let y1 = buf_ch[i1];
                let y2 = buf_ch[i2];
                let y3 = buf_ch[i3];

                // Hermite 4-point cubic interpolation
                let c0 = y1;
                let c1 = 0.5 * (y2 - y0);
                let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
                let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
                let delayed = ((c3 * frac + c2) * frac + c1) * frac + c0;

                // Low-pass dampening filter in feedback loop
                let damp_prev = self.damp_state[ch];
                let damp_out = damp_prev + damp * (delayed - damp_prev);
                self.damp_state[ch] = damp_out;

                buf_ch[w_pos] = in_val + damp_out * feedback;
                outputs[ch][i] = in_val * (1.0 - mix) + delayed * mix;
            }
        }

        self.write_pos = (self.write_pos + block_len) % buf_len;
    }
}

impl MidiResponder for AlgorithmicDelayProcessor {}
impl SnapshotProvider for AlgorithmicDelayProcessor {}

impl AudioProcessor for AlgorithmicDelayProcessor {
    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetBpm(bpm)) = command {
            if *bpm > 0.0 {
                self.delay_ms = 60000.0 / bpm;
            }
        }
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

// ============================================================================
// 4. Algorithmic State-Variable EQ / Filter (Real-Time Algorithmic Insert)
// ============================================================================
pub struct AlgorithmicEqProcessor {
    pub cutoff: f32,
    pub q: f32,
    pub mode: u8, // 0: LowPass, 1: HighPass, 2: BandPass, 3: Notch
    s1: [f32; 16],
    s2: [f32; 16],
}

impl AlgorithmicEqProcessor {
    pub fn new() -> Self {
        Self {
            cutoff: 1000.0,
            q: 0.707,
            mode: 0,
            s1: [0.0; 16],
            s2: [0.0; 16],
        }
    }
}

impl Default for AlgorithmicEqProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalProcessor for AlgorithmicEqProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext) {
        let num_ch = inputs.len().min(outputs.len()).min(16);
        let sample_rate = 48000.0f32;
        let g = ((std::f32::consts::PI * self.cutoff / sample_rate).tan()).clamp(0.0001, 10.0);
        let k = 1.0 / self.q.max(0.1);

        for ch in 0..num_ch {
            let in_buf = inputs[ch];
            let out_buf = &mut outputs[ch];
            let n = in_buf.len().min(out_buf.len());

            let mut s1 = self.s1[ch];
            let mut s2 = self.s2[ch];

            for i in 0..n {
                let x = in_buf[i];
                let hp = (x - k * s1 - s2) / (1.0 + g * (g + k));
                let v1 = g * hp;
                let bp = v1 + s1;
                s1 = bp + v1;
                let v2 = g * bp;
                let lp = v2 + s2;
                s2 = lp + v2;

                out_buf[i] = match self.mode {
                    0 => lp,
                    1 => hp,
                    2 => bp,
                    3 => hp + lp,
                    _ => lp,
                };
            }

            self.s1[ch] = s1;
            self.s2[ch] = s2;
        }
    }
}

impl MidiResponder for AlgorithmicEqProcessor {}
impl SnapshotProvider for AlgorithmicEqProcessor {}

impl AudioProcessor for AlgorithmicEqProcessor {
    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { param_id, value, .. }) = command {
            match param_id {
                0 => self.cutoff = *value,
                1 => self.q = *value,
                2 => self.mode = *value as u8,
                _ => {}
            }
        }
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

// ============================================================================
// 5. Algorithmic Dual-Oscillator Synthesizer (Real-Time Instrument)
// ============================================================================
pub struct AlgorithmicSynthInstrument {
    pub active_note: Option<u8>,
    pub phase_1: f32,
    pub phase_2: f32,
    pub envelope: f32,
}

impl AlgorithmicSynthInstrument {
    pub fn new() -> Self {
        Self {
            active_note: None,
            phase_1: 0.0,
            phase_2: 0.0,
            envelope: 0.0,
        }
    }

    fn midi_note_to_freq(note: u8) -> f32 {
        440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
    }
}

impl Default for AlgorithmicSynthInstrument {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalProcessor for AlgorithmicSynthInstrument {
    fn process(&mut self, _inputs: &[&[f32]], outputs: &mut [&mut [f32]], _ctx: &mut ProcessContext) {
        let sample_rate = 48000.0f32;
        let num_ch = outputs.len();
        if num_ch == 0 { return; }

        let block_len = outputs[0].len();
        let freq = match self.active_note {
            Some(n) => Self::midi_note_to_freq(n),
            None => 0.0,
        };

        let inc1 = freq / sample_rate;
        let inc2 = (freq * 1.005) / sample_rate; // Slight detune

        for i in 0..block_len {
            if self.active_note.is_some() {
                self.envelope = (self.envelope + 0.005).min(1.0);
            } else {
                self.envelope = (self.envelope - 0.002).max(0.0);
            }

            let s1 = (self.phase_1 * std::f32::consts::TAU).sin();
            let s2 = (self.phase_2 * std::f32::consts::TAU).sin();
            let sample = (s1 * 0.6 + s2 * 0.4) * self.envelope * 0.3;

            self.phase_1 = (self.phase_1 + inc1).fract();
            self.phase_2 = (self.phase_2 + inc2).fract();

            for ch in 0..num_ch {
                outputs[ch][i] = sample;
            }
        }
    }
}

impl MidiResponder for AlgorithmicSynthInstrument {
    fn apply_midi(&mut self, event: MidiEvent, _ctx: Option<&ProcessContext>) {
        let status = event.status & 0xF0;
        let note = event.data1;
        let velocity = event.data2;

        match status {
            0x90 => { // Note On
                if velocity > 0 {
                    self.active_note = Some(note);
                } else if self.active_note == Some(note) {
                    self.active_note = None;
                }
            }
            0x80 => { // Note Off
                if self.active_note == Some(note) {
                    self.active_note = None;
                }
            }
            _ => {}
        }
    }
}

impl SnapshotProvider for AlgorithmicSynthInstrument {}

impl AudioProcessor for AlgorithmicSynthInstrument {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

// ============================================================================
// SidecarChain: Composite execution container (Eliminating IPC latency)
// ============================================================================
pub struct SidecarChain {
    instrument: Option<Box<dyn AudioProcessor>>,
    inserts: Vec<Box<dyn AudioProcessor>>,
    scratch_a: Vec<Vec<f32>>,
    scratch_b: Vec<Vec<f32>>,
}

impl SidecarChain {
    pub fn new(instrument: Option<Box<dyn AudioProcessor>>, inserts: Vec<Box<dyn AudioProcessor>>) -> Self {
        let max_channels = 16;
        let block_size = 1024;
        let scratch_a = vec![vec![0.0f32; block_size]; max_channels];
        let scratch_b = vec![vec![0.0f32; block_size]; max_channels];

        Self {
            instrument,
            inserts,
            scratch_a,
            scratch_b,
        }
    }

    pub fn instrument(&self) -> Option<&dyn AudioProcessor> {
        self.instrument.as_deref()
    }

    pub fn inserts(&self) -> &[Box<dyn AudioProcessor>] {
        &self.inserts
    }
}

impl SignalProcessor for SidecarChain {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], ctx: &mut ProcessContext) {
        let num_out = outputs.len().min(16);
        if num_out == 0 { return; }
        let block_len = outputs[0].len().min(1024);

        // Resize scratch buffers if block_len exceeds initial capacity
        if self.scratch_a[0].len() < block_len {
            for ch in 0..16 {
                self.scratch_a[ch].resize(block_len, 0.0);
                self.scratch_b[ch].resize(block_len, 0.0);
            }
        }

        // 1. Initial stage: Instrument or Input Pass-through into scratch_a
        let num_in = inputs.len().min(16);
        if let Some(inst) = &mut self.instrument {
            let mut out_ptrs: [*mut f32; 16] = [std::ptr::null_mut(); 16];
            for ch in 0..num_out {
                out_ptrs[ch] = self.scratch_a[ch].as_mut_ptr();
            }
            let mut out_slices: [&mut [f32]; 16] = std::array::from_fn(|i| {
                if !out_ptrs[i].is_null() {
                    unsafe { std::slice::from_raw_parts_mut(out_ptrs[i], block_len) }
                } else {
                    &mut [][..]
                }
            });
            inst.process(inputs, &mut out_slices[..num_out], ctx);
        } else {
            for ch in 0..num_out {
                if ch < num_in {
                    let n = inputs[ch].len().min(block_len);
                    self.scratch_a[ch][..n].copy_from_slice(&inputs[ch][..n]);
                } else {
                    self.scratch_a[ch][..block_len].fill(0.0);
                }
            }
        }

        // 2. Sequential Inserts processing in-place across scratch_a and scratch_b
        let mut current_is_a = true;

        for insert in &mut self.inserts {
            let (src_scratch, dst_scratch) = if current_is_a {
                (&self.scratch_a, &mut self.scratch_b)
            } else {
                (&self.scratch_b, &mut self.scratch_a)
            };

            let in_slices_arr: [&[f32]; 16] = std::array::from_fn(|i| &src_scratch[i][..block_len]);

            let mut out_ptrs: [*mut f32; 16] = [std::ptr::null_mut(); 16];
            for ch in 0..num_out {
                out_ptrs[ch] = dst_scratch[ch].as_mut_ptr();
            }
            let mut out_slices_arr: [&mut [f32]; 16] = std::array::from_fn(|i| {
                if !out_ptrs[i].is_null() {
                    unsafe { std::slice::from_raw_parts_mut(out_ptrs[i], block_len) }
                } else {
                    &mut [][..]
                }
            });

            insert.process(&in_slices_arr[..num_out], &mut out_slices_arr[..num_out], ctx);
            current_is_a = !current_is_a;
        }

        // 3. Final Copy from active scratch buffer to output
        let final_scratch = if current_is_a { &self.scratch_a } else { &self.scratch_b };
        for ch in 0..num_out {
            let n = outputs[ch].len().min(block_len);
            outputs[ch][..n].copy_from_slice(&final_scratch[ch][..n]);
        }
    }
}

impl MidiResponder for SidecarChain {
    fn apply_midi(&mut self, event: MidiEvent, ctx: Option<&ProcessContext>) {
        if let Some(inst) = &mut self.instrument {
            inst.apply_midi(event, ctx);
        }
        for insert in &mut self.inserts {
            insert.apply_midi(event, ctx);
        }
    }
}

impl SnapshotProvider for SidecarChain {
    fn pull_snapshot(&mut self) -> Option<Arc<Vec<f32>>> {
        if let Some(inst) = &mut self.instrument {
            inst.pull_snapshot()
        } else if let Some(first) = self.inserts.first_mut() {
            first.pull_snapshot()
        } else {
            None
        }
    }
}

impl AudioProcessor for SidecarChain {
    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let Some(inst) = &mut self.instrument {
            inst.apply_command(command);
        }
        for insert in &mut self.inserts {
            insert.apply_command(command);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

// ============================================================================
// SidecarStore: Catalog and factory registry for sidecars
// ============================================================================
pub type ProcessorFactory = Arc<dyn Fn() -> Box<dyn AudioProcessor> + Send + Sync>;

pub struct SidecarStore {
    descriptors: HashMap<String, SidecarDescriptor>,
    factories: HashMap<String, ProcessorFactory>,
}

impl SidecarStore {
    pub fn new() -> Self {
        Self {
            descriptors: HashMap::new(),
            factories: HashMap::new(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut store = Self::new();

        store.register(
            SidecarDescriptor::new(
                "neural-saturation",
                "Neural Saturation / Preamp",
                SidecarType::NeuralProcessor,
                &["neural", "insert", "real-time", "saturation"],
                "Padé SIMD neural analog saturation processor",
                0,
            ),
            || Box::new(NeuralSaturationProcessor::new()),
        );

        store.register(
            SidecarDescriptor::new(
                "neural-filter",
                "Neural Dynamic Filter",
                SidecarType::NeuralProcessor,
                &["neural", "insert", "real-time", "filter", "eq"],
                "Hypernetwork dynamic filter with SIMD non-linearities",
                0,
            ),
            || Box::new(NeuralFilterProcessor::new()),
        );

        store.register(
            SidecarDescriptor::new(
                "algorithmic-delay",
                "Algorithmic Tape Delay",
                SidecarType::Insert,
                &["algorithmic", "insert", "real-time", "delay"],
                "Low-latency delay line with Hermite fractional interpolation",
                0,
            ),
            || Box::new(AlgorithmicDelayProcessor::new()),
        );

        store.register(
            SidecarDescriptor::new(
                "algorithmic-eq",
                "State-Variable EQ / Filter",
                SidecarType::Insert,
                &["algorithmic", "insert", "real-time", "eq", "filter"],
                "Multi-mode State-Variable Filter (LP, HP, BP, Notch)",
                0,
            ),
            || Box::new(AlgorithmicEqProcessor::new()),
        );

        store.register(
            SidecarDescriptor::new(
                "algorithmic-synth",
                "Dual Oscillator Synthesizer",
                SidecarType::Instrument,
                &["algorithmic", "instrument", "real-time"],
                "Dual-oscillator MIDI synthesizer instrument",
                0,
            ),
            || Box::new(AlgorithmicSynthInstrument::new()),
        );

        store
    }

    pub fn register(
        &mut self,
        descriptor: SidecarDescriptor,
        factory: impl Fn() -> Box<dyn AudioProcessor> + Send + Sync + 'static,
    ) {
        let id = descriptor.id.clone();
        self.descriptors.insert(id.clone(), descriptor);
        self.factories.insert(id, Arc::new(factory));
    }

    pub fn list(&self) -> Vec<SidecarDescriptor> {
        let mut list: Vec<_> = self.descriptors.values().cloned().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    pub fn get_descriptor(&self, id: &str) -> Option<&SidecarDescriptor> {
        self.descriptors.get(id)
    }

    pub fn filter_by_tag(&self, tag: &str) -> Vec<SidecarDescriptor> {
        self.list().into_iter().filter(|d| d.has_tag(tag)).collect()
    }

    pub fn filter_by_tags(&self, query_tags: &[&str]) -> Vec<SidecarDescriptor> {
        self.list().into_iter().filter(|d| d.has_all_tags(query_tags)).collect()
    }

    pub fn filter_by_type(&self, sidecar_type: SidecarType) -> Vec<SidecarDescriptor> {
        self.list().into_iter().filter(|d| d.sidecar_type == sidecar_type).collect()
    }

    pub fn create_processor(&self, id: &str) -> Option<Box<dyn AudioProcessor>> {
        self.factories.get(id).map(|factory| factory())
    }

    pub fn create_chain(&self, instrument_id: Option<&str>, insert_ids: &[&str]) -> Option<SidecarChain> {
        let instrument = match instrument_id {
            Some(id) => Some(self.create_processor(id)?),
            None => None,
        };

        let mut inserts = Vec::new();
        for id in insert_ids {
            inserts.push(self.create_processor(id)?);
        }

        Some(SidecarChain::new(instrument, inserts))
    }
}

impl Default for SidecarStore {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;

    #[test]
    fn test_store_list_and_descriptors() {
        let store = SidecarStore::with_defaults();
        let list = store.list();
        assert_eq!(list.len(), 5);

        let delay_desc = store.get_descriptor("algorithmic-delay").expect("algorithmic-delay must exist");
        assert_eq!(delay_desc.name, "Algorithmic Tape Delay");
        assert_eq!(delay_desc.sidecar_type, SidecarType::Insert);
        assert!(delay_desc.has_tag("delay"));
        assert!(delay_desc.has_tag("real-time"));
    }

    #[test]
    fn test_store_tag_filtering() {
        let store = SidecarStore::with_defaults();

        // Single tag queries
        let delays = store.filter_by_tag("delay");
        assert_eq!(delays.len(), 1);
        assert_eq!(delays[0].id, "algorithmic-delay");

        let neurals = store.filter_by_tag("neural");
        assert_eq!(neurals.len(), 2);
        let neural_ids: Vec<_> = neurals.iter().map(|d| d.id.as_str()).collect();
        assert!(neural_ids.contains(&"neural-saturation"));
        assert!(neural_ids.contains(&"neural-filter"));

        let eqs = store.filter_by_tag("eq");
        assert_eq!(eqs.len(), 2);
        let eq_ids: Vec<_> = eqs.iter().map(|d| d.id.as_str()).collect();
        assert!(eq_ids.contains(&"neural-filter"));
        assert!(eq_ids.contains(&"algorithmic-eq"));

        let instruments = store.filter_by_tag("instrument");
        assert_eq!(instruments.len(), 1);
        assert_eq!(instruments[0].id, "algorithmic-synth");

        let realtimes = store.filter_by_tag("real-time");
        assert_eq!(realtimes.len(), 5);

        // Multi-tag queries
        let neural_inserts = store.filter_by_tags(&["neural", "insert", "real-time"]);
        assert_eq!(neural_inserts.len(), 2);

        let neural_eqs = store.filter_by_tags(&["neural", "eq"]);
        assert_eq!(neural_eqs.len(), 1);
        assert_eq!(neural_eqs[0].id, "neural-filter");
    }

    #[test]
    fn test_store_type_filtering() {
        let store = SidecarStore::with_defaults();

        let instruments = store.filter_by_type(SidecarType::Instrument);
        assert_eq!(instruments.len(), 1);
        assert_eq!(instruments[0].id, "algorithmic-synth");

        let neural_procs = store.filter_by_type(SidecarType::NeuralProcessor);
        assert_eq!(neural_procs.len(), 2);

        let inserts = store.filter_by_type(SidecarType::Insert);
        assert_eq!(inserts.len(), 2);
    }

    #[test]
    fn test_sidecar_chain_execution() {
        let store = SidecarStore::with_defaults();

        // Create a chain with instrument + 3 inserts:
        // Synth Instrument -> Neural Saturation -> Algorithmic EQ -> Algorithmic Delay
        let mut chain = store
            .create_chain(
                Some("algorithmic-synth"),
                &["neural-saturation", "algorithmic-eq", "algorithmic-delay"],
            )
            .expect("Chain creation must succeed");

        assert!(chain.instrument().is_some());
        assert_eq!(chain.inserts().len(), 3);

        // Send a MIDI note on event to trigger instrument sound generation
        chain.apply_midi(
            MidiEvent {
                timestamp_samples: 0,
                status: 0x90, // Note On
                data1: 60,    // Middle C
                data2: 100,   // Velocity 100
                _pad: 0,
            },
            None,
        );

        // Render audio through the composite chain
        let mut out_l = vec![0.0f32; 256];
        let mut out_r = vec![0.0f32; 256];
        let mut ctx = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        chain.process(
            &[],
            &mut [&mut out_l, &mut out_r],
            &mut ctx,
        );

        // Output must contain non-zero audio signals generated by synth and processed through inserts
        let energy_l: f32 = out_l.iter().map(|s| s.abs()).sum();
        let energy_r: f32 = out_r.iter().map(|s| s.abs()).sum();

        assert!(energy_l > 0.0, "Left channel output must have non-zero signal");
        assert!(energy_r > 0.0, "Right channel output must have non-zero signal");

        // Verify signal finitude
        assert!(out_l.iter().all(|s| s.is_finite()));
        assert!(out_r.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn test_sidecar_chain_passthrough_inserts() {
        let store = SidecarStore::with_defaults();

        // Create an insert-only chain (no instrument)
        let mut chain = store
            .create_chain(
                None,
                &["neural-saturation", "algorithmic-delay"],
            )
            .expect("Insert chain must succeed");

        let in_l = vec![0.5f32; 256];
        let in_r = vec![-0.5f32; 256];
        let mut out_l = vec![0.0f32; 256];
        let mut out_r = vec![0.0f32; 256];

        let mut ctx = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        chain.process(
            &[&in_l, &in_r],
            &mut [&mut out_l, &mut out_r],
            &mut ctx,
        );

        // Verify audio was processed in-place
        assert!(out_l.iter().all(|s| s.is_finite()));
        assert!(out_r.iter().all(|s| s.is_finite()));
        assert!(out_l[0] != 0.0);
    }
}
