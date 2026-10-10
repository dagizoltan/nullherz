use nullherz_traits::{
    AudioProcessor, ProcessContext, SignalProcessor, MidiResponder, SnapshotProvider,
    ProcessorCommand,
};

const NUM_COMBS: usize = 4;
const NUM_ALLPASS: usize = 2;
const MAX_DELAY_BUF: usize = 8192;

/// The rate Freeverb's delay constants below are tunings for.
const REFERENCE_RATE: f32 = 44_100.0;

/// Freeverb's comb and allpass lengths, in SAMPLES at [`REFERENCE_RATE`].
///
/// Being sample counts is the whole problem: used unscaled they describe a
/// shorter room as the rate rises. Measured by `probe_reverb_quality`, the -60
/// dB tail went 0.790 s at 44.1 kHz, 0.710 s at 48 kHz (-10%) and 0.360 s at
/// 96 kHz (-54%) — a room that shrinks because the converter changed.
const COMB_LENGTHS_REF: [usize; NUM_COMBS] = [1116, 1188, 1277, 1356];
const ALLPASS_LENGTHS_REF: [usize; NUM_ALLPASS] = [556, 441];

/// Offset applied to the RIGHT channel's delays, in samples at
/// [`REFERENCE_RATE`] — Freeverb's `stereospread`.
///
/// Without it both channels run identical delay networks, so a stereo reverb
/// has no stereo image at all: the probe measured an L/R correlation of
/// 1.00000 and a maximum sample difference of exactly 0.0. The wet/dry mix
/// cannot widen that, because there is nothing to widen.
const STEREO_SPREAD_REF: usize = 23;

/// Delay length for one comb/allpass stage at the running rate.
///
/// Scales the reference tuning by `rate / 44100` so the delay is a DURATION
/// rather than a sample count, and offsets the right channel to decorrelate the
/// two. Clamped into the buffer: the read index is computed modulo
/// `MAX_DELAY_BUF`, so a length at or beyond it would alias onto the write
/// position and feed the comb its own input. 8192 holds the longest stage up to
/// 192 kHz (1356 + 23 scaled = 5926), so the clamp is a guard, not a limit.
#[inline]
fn delay_len(reference: usize, rate: f32, spread: usize) -> usize {
    let scale = if rate > 0.0 { rate / REFERENCE_RATE } else { 1.0 };
    let scaled = ((reference + spread) as f32 * scale).round() as usize;
    scaled.clamp(1, MAX_DELAY_BUF - 1)
}

/// Zero-allocation Algorithmic Stereo Reverb Processor.
pub struct AlgorithmicReverbProcessor {
    pub room_size: f32,
    pub damp: f32,
    pub wet_dry: f32,

    /// Rate to use when a block arrives with no transport to read.
    ///
    /// `ReverbFactory` passes the rate the node is built for; `process`
    /// prefers `ctx.transport.sample_rate` when it has one, so a device change
    /// is picked up without rebuilding the node.
    sample_rate: f32,

    comb_buffers: [[[f32; MAX_DELAY_BUF]; NUM_COMBS]; 2],
    comb_write_pos: [[usize; NUM_COMBS]; 2],
    comb_filter_store: [[f32; NUM_COMBS]; 2],

    allpass_buffers: [[[f32; MAX_DELAY_BUF]; NUM_ALLPASS]; 2],
    allpass_write_pos: [[usize; NUM_ALLPASS]; 2],
}

impl AlgorithmicReverbProcessor {
    pub fn new() -> Self {
        Self::with_sample_rate(nullherz_traits::DEFAULT_SAMPLE_RATE)
    }

    /// Build for a known rate. Preferred over [`Self::new`] wherever the rate
    /// is known, so the first block is right even before any transport arrives.
    pub fn with_sample_rate(sample_rate: f32) -> Self {
        Self {
            sample_rate: if sample_rate > 0.0 { sample_rate } else { nullherz_traits::DEFAULT_SAMPLE_RATE },
            room_size: 0.8,
            damp: 0.2,
            wet_dry: 0.35,
            comb_buffers: [[[0.0; MAX_DELAY_BUF]; NUM_COMBS]; 2],
            comb_write_pos: [[0; NUM_COMBS]; 2],
            comb_filter_store: [[0.0; NUM_COMBS]; 2],
            allpass_buffers: [[[0.0; MAX_DELAY_BUF]; NUM_ALLPASS]; 2],
            allpass_write_pos: [[0; NUM_ALLPASS]; 2],
        }
    }
}

impl Default for AlgorithmicReverbProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalProcessor for AlgorithmicReverbProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], ctx: &mut ProcessContext) {
        if inputs.is_empty() || outputs.is_empty() {
            return;
        }

        let num_channels = inputs.len().min(outputs.len()).min(2);
        let block_len = inputs[0].len();

        // Live transport first, so a device rate change is followed without
        // rebuilding the node; the constructed rate is the fallback for the
        // paths that have no transport (AddSource arrives as a topology
        // mutation with no ProcessContext).
        let rate = ctx
            .transport
            .map(|t| t.sample_rate)
            .filter(|r| *r > 0.0)
            .unwrap_or(self.sample_rate);

        let feedback = self.room_size.clamp(0.0, 0.98);
        let damp = self.damp.clamp(0.0, 0.95);
        let wet = self.wet_dry.clamp(0.0, 1.0);
        let dry = 1.0 - wet;

        for ch in 0..num_channels {
            let in_buf = inputs[ch];
            let out_buf = &mut outputs[ch][..block_len];

            // Once per channel, not per sample: six multiplies, no allocation.
            let spread = if ch == 1 { STEREO_SPREAD_REF } else { 0 };
            let mut comb_lengths = [0usize; NUM_COMBS];
            for c in 0..NUM_COMBS {
                comb_lengths[c] = delay_len(COMB_LENGTHS_REF[c], rate, spread);
            }
            let mut allpass_lengths = [0usize; NUM_ALLPASS];
            for a in 0..NUM_ALLPASS {
                allpass_lengths[a] = delay_len(ALLPASS_LENGTHS_REF[a], rate, spread);
            }

            for i in 0..block_len {
                let input = in_buf[i];
                if !input.is_finite() {
                    out_buf[i] = 0.0;
                    continue;
                }

                let mut comb_sum = 0.0f32;

                for c in 0..NUM_COMBS {
                    let delay_len = comb_lengths[c];
                    let write_pos = self.comb_write_pos[ch][c];
                    let read_pos = (write_pos + MAX_DELAY_BUF - delay_len) % MAX_DELAY_BUF;

                    let output = self.comb_buffers[ch][c][read_pos];
                    self.comb_filter_store[ch][c] = output * (1.0 - damp) + self.comb_filter_store[ch][c] * damp;

                    let new_val = input + self.comb_filter_store[ch][c] * feedback;
                    self.comb_buffers[ch][c][write_pos] = new_val;
                    self.comb_write_pos[ch][c] = (write_pos + 1) % MAX_DELAY_BUF;

                    comb_sum += output;
                }

                let mut ap_out = comb_sum * 0.25;
                for a in 0..NUM_ALLPASS {
                    let delay_len = allpass_lengths[a];
                    let write_pos = self.allpass_write_pos[ch][a];
                    let read_pos = (write_pos + MAX_DELAY_BUF - delay_len) % MAX_DELAY_BUF;

                    let buf_out = self.allpass_buffers[ch][a][read_pos];
                    let new_val = ap_out + buf_out * 0.5;

                    self.allpass_buffers[ch][a][write_pos] = new_val;
                    self.allpass_write_pos[ch][a] = (write_pos + 1) % MAX_DELAY_BUF;

                    ap_out = -ap_out + buf_out;
                }

                out_buf[i] = input * dry + ap_out * wet;
            }
        }
    }

    fn reset(&mut self) {
        self.comb_buffers = [[[0.0; MAX_DELAY_BUF]; NUM_COMBS]; 2];
        self.comb_write_pos = [[0; NUM_COMBS]; 2];
        self.comb_filter_store = [[0.0; NUM_COMBS]; 2];
        self.allpass_buffers = [[[0.0; MAX_DELAY_BUF]; NUM_ALLPASS]; 2];
        self.allpass_write_pos = [[0; NUM_ALLPASS]; 2];
    }
}

impl MidiResponder for AlgorithmicReverbProcessor {}
impl SnapshotProvider for AlgorithmicReverbProcessor {}

impl AudioProcessor for AlgorithmicReverbProcessor {
    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        if !value.is_finite() {
            return;
        }
        match param_id {
            0 => self.room_size = value.clamp(0.0, 0.98),
            1 => self.damp = value.clamp(0.0, 0.95),
            2 => self.wet_dry = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.room_size,
            1 => self.damp,
            2 => self.wet_dry,
            _ => 0.0,
        }
    }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { param_id, value, ramp_duration_samples, .. }) = command {
            self.set_parameter(*param_id, *value, *ramp_duration_samples);
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
