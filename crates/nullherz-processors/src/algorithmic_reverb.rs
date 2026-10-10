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

/// This processor's parameters, declared once.
///
/// REFERENCE USE of `ParamSpec`. The range lived in two places before: a
/// `value.clamp(0.0, 0.98)` inside `set_parameter` and a parallel `mins`/`maxs`
/// array inside `metadata()`. Two statements of one fact, with nothing
/// connecting them — which is how 31 parameters across 10 processors came to
/// declare ranges their clamps do not honour
/// (`declared_params_match_behaviour_test`). Here `metadata()` publishes this
/// table and `set_parameter` clamps against it, so there is no second place for
/// a range to live and therefore none for it to drift to.
///
/// Ids are part of the saved-project format: a project references
/// `(processor_type, param_id)`, so renumbering these reinterprets saved
/// sessions. Append, never renumber.
const PARAMS: &[nullherz_traits::ParamSpec] = &[
    nullherz_traits::ParamSpec { id: 0, name: "ROOM SIZE", min: 0.0, max: 0.98, default: 0.8 },
    nullherz_traits::ParamSpec { id: 1, name: "DAMP", min: 0.0, max: 0.95, default: 0.2 },
    nullherz_traits::ParamSpec { id: 2, name: "MIX", min: 0.0, max: 1.0, default: 0.35 },
];

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
            // From the table, so a declared default is the default in fact.
            room_size: PARAMS[0].default,
            damp: PARAMS[1].default,
            wet_dry: PARAMS[2].default,
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
        // Range from the table, not from a literal here. The match now only
        // says WHERE the value goes; what is acceptable is declared once.
        let Some(value) = nullherz_traits::ParamSpec::clamp_in(PARAMS, param_id, value) else {
            return;
        };
        match param_id {
            0 => self.room_size = value,
            1 => self.damp = value,
            2 => self.wet_dry = value,
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

    fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        // Same table `set_parameter` clamps against, so what a host draws and
        // what the processor accepts cannot disagree.
        Some(nullherz_traits::ProcessorMetadata::from_specs(0, PARAMS))
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
