use std::collections::VecDeque;
use nullherz_traits::{AudioProcessor, SignalProcessor, ProcessContext, AudioConfig, ProcessorCommand, Command, MidiEvent, MidiResponder, SnapshotProvider};
use crate::MAX_CHANNELS;

pub struct LimiterProcessor {
    pub id: u64,
    buffers: [Vec<f32>; MAX_CHANNELS],
    peak_buffer: Vec<f32>,
    write_pos: usize,
    sample_rate: f32,

    // Parameters
    threshold: f32,
    release_ms: f32,
    lookahead_ms: f32,
    ceiling: f32,

    // Calculated state
    release_coef: f32,
    lookahead_samples: usize,
    envelope: f32,
    capacity: usize,

    // Sliding-window-maximum state (monotonic deque). Replaces an O(window)
    // rescan per sample with O(1) amortized: `max_deque` holds (sample_index,
    // peak) pairs in strictly-decreasing peak order, so the front is always
    // the max over the current look-ahead window. Pre-reserved to `capacity`
    // (> lookahead), so push/pop never allocate on the audio thread. Produces
    // the identical window max the rescan did — output is bit-for-bit the same.
    sample_counter: u64,
    max_deque: VecDeque<(u64, f32)>,
}

impl LimiterProcessor {
    pub fn new(id: u64, sample_rate: f32) -> Self {
        let capacity = 2048; // Safe upper bound for look-ahead
        let buffers = std::array::from_fn(|_| vec![0.0; capacity]);
        let peak_buffer = vec![0.0; capacity];

        let mut processor = Self {
            id,
            buffers,
            peak_buffer,
            write_pos: 0,
            sample_rate,
            threshold: 1.0,
            release_ms: 100.0,
            lookahead_ms: 2.0,
            ceiling: 1.0,
            release_coef: 0.0,
            lookahead_samples: 0,
            envelope: 0.0,
            capacity,
            sample_counter: 0,
            max_deque: VecDeque::with_capacity(capacity),
        };
        processor.update_derived_params();
        processor
    }

    fn update_derived_params(&mut self) {
        // Look-ahead samples calculation
        let lookahead_samples = (self.lookahead_ms * 0.001 * self.sample_rate).round() as usize;
        self.lookahead_samples = lookahead_samples.clamp(1, self.capacity - 10);

        // Exponential release coefficient
        let release_samples = self.release_ms * 0.001 * self.sample_rate;
        self.release_coef = (-1.0 / release_samples.max(1.0)).exp();
    }
}

impl nullherz_traits::RtSafe for LimiterProcessor {}

impl SignalProcessor for LimiterProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        let num_channels = inputs.len().min(outputs.len()).min(MAX_CHANNELS);
        if num_channels == 0 { return; }

        let len = inputs[0].len();
        let threshold = self.threshold;
        let ceiling = self.ceiling;

        for i in 0..len {
            // Find current sample multi-channel absolute peak
            let mut current_peak = 1e-6_f32;
            for ch in 0..num_channels {
                current_peak = current_peak.max(inputs[ch][i].abs());
                self.buffers[ch][self.write_pos] = inputs[ch][i];
            }
            self.peak_buffer[self.write_pos] = current_peak;

            // Look-ahead peak detection via monotonic deque (sliding-window
            // max over the last `lookahead_samples` samples, current included).
            // Drop candidates the new peak dominates, append it, then drop the
            // front once it falls out of the window. Front = window max — the
            // same value the old O(window) rescan produced.
            let t = self.sample_counter;
            while let Some(&(_, v)) = self.max_deque.back() {
                if v <= current_peak { self.max_deque.pop_back(); } else { break; }
            }
            self.max_deque.push_back((t, current_peak));
            // saturating: at startup t+1 < lookahead, so the window simply
            // starts at sample 0 (matches the old scan, whose extra slots read
            // zero-initialized peaks that never beat a real |sample| >= 1e-6).
            let window_start = (t + 1).saturating_sub(self.lookahead_samples as u64);
            while let Some(&(idx, _)) = self.max_deque.front() {
                if idx < window_start { self.max_deque.pop_front(); } else { break; }
            }
            let window_max = self.max_deque.front().map(|&(_, v)| v).unwrap_or(0.0);
            self.sample_counter += 1;

            // Smooth the peak envelope using release ballistics
            self.envelope = window_max.max(self.envelope * self.release_coef);

            // Compute the target gain reduction to brickwall limit the signal
            let gain = if self.envelope > threshold {
                threshold / self.envelope
            } else {
                1.0
            };

            // Calculate output multiplier (including makeup gain to ceiling)
            let output_scale = gain * (ceiling / threshold);

            // Retrieve the delayed audio signal from the look-ahead delay line and apply gain
            let read_pos = (self.write_pos + self.capacity - self.lookahead_samples) % self.capacity;
            for ch in 0..num_channels {
                outputs[ch][i] = self.buffers[ch][read_pos] * output_scale;
            }

            self.write_pos = (self.write_pos + 1) % self.capacity;
        }
    }

    fn setup(&mut self, config: AudioConfig) {
        self.sample_rate = config.sample_rate;
        self.reset();
    }

    fn reset(&mut self) {
        for b in self.buffers.iter_mut() { b.fill(0.0); }
        self.peak_buffer.fill(0.0);
        self.write_pos = 0;
        self.envelope = 0.0;
        self.sample_counter = 0;
        self.max_deque.clear();
        self.update_derived_params();
    }
}

impl MidiResponder for LimiterProcessor {
    fn apply_midi(&mut self, _event: MidiEvent, _context: Option<&ProcessContext>) {}
}

impl SnapshotProvider for LimiterProcessor {}

impl AudioProcessor for LimiterProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn apply_command(&mut self, command: &ProcessorCommand) {
        if let Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, .. }) = command
            && *target_id == self.id {
                self.set_parameter(*param_id, *value, 0);
            }
    }

    fn set_parameter(&mut self, param_id: u32, mut value: f32, _ramp_duration_samples: u32) {
        if !value.is_finite() { value = 0.0; }
        match param_id {
            0 => self.threshold = value.clamp(0.001, 1.0),
            1 => self.release_ms = value.clamp(1.0, 1000.0),
            2 => self.lookahead_ms = value.clamp(0.1, 5.0),
            3 => self.ceiling = value.clamp(0.01, 1.0),
            _ => {}
        }
        self.update_derived_params();
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        match param_id {
            0 => self.threshold,
            1 => self.release_ms,
            2 => self.lookahead_ms,
            3 => self.ceiling,
            _ => 0.0,
        }
    }

    fn metadata(&self) -> Option<nullherz_traits::ProcessorMetadata> {
        let mut parameters = [nullherz_traits::ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: 0.0,
            max: 1.0,
            default: 0.0,
        }; 16];

        let names: &[&[u8]] = &[b"Threshold", b"Release", b"Lookahead", b"Ceiling"];
        let mins = [0.001, 1.0, 0.1, 0.01];
        let maxs = [1.0, 1000.0, 5.0, 1.0];
        let defs = [1.0, 100.0, 2.0, 1.0];

        for (i, &name) in names.iter().enumerate() {
            parameters[i].id = i as u32;
            parameters[i].name[..name.len()].copy_from_slice(name);
            parameters[i].min = mins[i];
            parameters[i].max = maxs[i];
            parameters[i].default = defs[i];
        }

        Some(nullherz_traits::ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 4,
            parameters,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::ProcessContext;

    #[test]
    fn test_limiter_brickwall_containment() {
        let mut limiter = LimiterProcessor::new(1, 44100.0);
        limiter.set_parameter(0, 0.5, 0); // threshold = 0.5
        limiter.set_parameter(3, 0.5, 0); // ceiling = 0.5

        let input_left = vec![2.0, 1.5, -3.0, 0.0, 0.1];
        let input_right = vec![0.5, -2.0, 1.0, -1.0, 0.2];
        let mut out_left = vec![0.0; 5];
        let mut out_right = vec![0.0; 5];

        let mut context = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: false,
        };

        // Process a block
        {
            let inputs: &[&[f32]] = &[&input_left, &input_right];
            let mut out_l_ref = &mut out_left[..];
            let mut out_r_ref = &mut out_right[..];
            let outputs: &mut [&mut [f32]] = &mut [&mut out_l_ref, &mut out_r_ref];
            limiter.process(inputs, outputs, &mut context);
        }

        // Verify outputs are brickwalled strictly within the ceiling (with 1e-4 tolerance for float margin)
        for val in out_left {
            assert!(val.abs() <= 0.5001, "Left channel exceeded ceiling: {}", val);
        }
        for val in out_right {
            assert!(val.abs() <= 0.5001, "Right channel exceeded ceiling: {}", val);
        }
    }

    /// The monotonic-deque look-ahead must produce BIT-identical output to the
    /// original O(window) rescan. Drive a long pseudo-random stereo signal in
    /// odd-sized blocks (so block boundaries fall mid-window) and compare, per
    /// sample, against a brute-force reference that scans the whole window.
    #[test]
    fn test_deque_lookahead_matches_bruteforce_bitexact() {
        let sr = 44_100.0;
        let mut limiter = LimiterProcessor::new(1, sr);
        // Non-default params exercise a different lookahead/release than the
        // constructor's, and a threshold low enough to force real limiting.
        limiter.set_parameter(0, 0.3, 0);   // threshold
        limiter.set_parameter(1, 50.0, 0);  // release ms
        limiter.set_parameter(2, 3.5, 0);   // lookahead ms
        limiter.set_parameter(3, 0.9, 0);   // ceiling

        // Reference state mirroring the ORIGINAL algorithm exactly.
        let lookahead = (3.5 * 0.001 * sr).round() as usize;
        let cap = 2048usize;
        let mut ref_buf_l = vec![0.0f32; cap];
        let mut ref_buf_r = vec![0.0f32; cap];
        let mut ref_peak = vec![0.0f32; cap];
        let mut ref_wpos = 0usize;
        let mut ref_env = 0.0f32;
        let release_coef = (-1.0f32 / (50.0 * 0.001 * sr).max(1.0)).exp();
        let (threshold, ceiling) = (0.3f32, 0.9f32);

        // Deterministic pseudo-random signal (xorshift), a few thousand samples.
        let mut seed = 0x1234_5678u32;
        let mut rng = || {
            seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5;
            (seed as f32 / u32::MAX as f32) * 2.4 - 1.2 // spans past +/-1 to trigger limiting
        };
        let total = 5000usize;
        let sig_l: Vec<f32> = (0..total).map(|_| rng()).collect();
        let sig_r: Vec<f32> = (0..total).map(|_| rng()).collect();

        let mut ctx = ProcessContext { transport: None, host: None, sub_block_offset: 0, is_last_sub_block: false };

        // Cycle through varying block sizes so block boundaries land at many
        // different offsets relative to the look-ahead window.
        let block_sizes = [64usize, 37, 200, 129, 256, 91];
        let mut pos = 0usize;
        let mut bs = 0usize;
        while pos < total {
            let b = block_sizes[bs % block_sizes.len()].min(total - pos);
            bs += 1;
            let in_l = &sig_l[pos..pos + b];
            let in_r = &sig_r[pos..pos + b];
            let mut ol = vec![0.0f32; b];
            let mut or = vec![0.0f32; b];
            {
                let inputs: &[&[f32]] = &[in_l, in_r];
                let mut olr = &mut ol[..];
                let mut orr = &mut or[..];
                let outputs: &mut [&mut [f32]] = &mut [&mut olr, &mut orr];
                limiter.process(inputs, outputs, &mut ctx);
            }
            // Brute-force reference (the ORIGINAL algorithm) for the same block.
            for k in 0..b {
                let cur = (in_l[k].abs()).max(in_r[k].abs()).max(1e-6);
                ref_buf_l[ref_wpos] = in_l[k];
                ref_buf_r[ref_wpos] = in_r[k];
                ref_peak[ref_wpos] = cur;
                let mut wmax = 0.0f32;
                for off in 0..lookahead {
                    let idx = (ref_wpos + cap - off) % cap;
                    wmax = wmax.max(ref_peak[idx]);
                }
                ref_env = wmax.max(ref_env * release_coef);
                let gain = if ref_env > threshold { threshold / ref_env } else { 1.0 };
                let scale = gain * (ceiling / threshold);
                let rpos = (ref_wpos + cap - lookahead) % cap;
                let exp_l = ref_buf_l[rpos] * scale;
                let exp_r = ref_buf_r[rpos] * scale;
                assert_eq!(ol[k].to_bits(), exp_l.to_bits(), "L mismatch at sample {}", pos + k);
                assert_eq!(or[k].to_bits(), exp_r.to_bits(), "R mismatch at sample {}", pos + k);
                ref_wpos = (ref_wpos + 1) % cap;
            }
            pos += b;
        }
    }
}
