use nullherz_traits::{AudioProcessor, ProcessContext, ProcessorMetadata, ParameterMetadata, AudioConfig};
use audio_dsp::spectral::SpectralPipeline;

const TWO_PI: f32 = 2.0 * std::f32::consts::PI;

/// Channels a deck strip carries. Lanes beyond the wired channel count are
/// idle (the pipeline only runs when process() hands them a buffer).
const STEREO_LANES: usize = 2;

/// One channel's worth of phase-vocoder state.
///
/// Per-channel state is not an optimization but a correctness requirement:
/// `prev_phase`/`sum_phase` track a single signal's phase trajectory across
/// frames. Feeding two channels through one lane interleaves their phases and
/// the frequency estimate (the delta between successive frames) becomes
/// garbage for both.
struct VocoderLane {
    pipeline: SpectralPipeline,
    scratch_re: Vec<f32>,
    scratch_im: Vec<f32>,
    /// Per-bin analysis phase from the previous frame.
    prev_phase: Vec<f32>,
    /// Per-bin accumulated synthesis phase.
    sum_phase: Vec<f32>,
    /// Per-bin analysis magnitude and true frequency (in bins).
    ana_mag: Vec<f32>,
    ana_freq: Vec<f32>,
    /// Per-bin synthesis magnitude and target frequency (in bins).
    syn_mag: Vec<f32>,
    syn_freq: Vec<f32>,
}

impl VocoderLane {
    fn new(fft_size: usize) -> Self {
        let bins = fft_size / 2 + 1;
        Self {
            pipeline: SpectralPipeline::new(fft_size),
            scratch_re: vec![0.0; fft_size],
            scratch_im: vec![0.0; fft_size],
            prev_phase: vec![0.0; bins],
            sum_phase: vec![0.0; bins],
            ana_mag: vec![0.0; bins],
            ana_freq: vec![0.0; bins],
            syn_mag: vec![0.0; bins],
            syn_freq: vec![0.0; bins],
        }
    }

    fn clear_phase_state(&mut self) {
        self.prev_phase.fill(0.0);
        self.sum_phase.fill(0.0);
    }

    fn reset(&mut self) {
        self.pipeline.reset();
        self.clear_phase_state();
    }

    fn process(&mut self, input: &[f32], output: &mut [f32], ratio: f32) {
        // Unison: the vocoder is a no-op and the pipeline's reconstruction is
        // pure overlap-add, so skip BOTH the phase-vocoder math AND the FFT
        // round-trip via the identity path. Same framing/latency/buffers as the
        // full path, so engaging or releasing pitch shift stays continuous, and
        // the output matches the FFT path to within round-trip float error.
        if (ratio - 1.0).abs() < 0.001 {
            self.pipeline.process_identity(input, output);
            return;
        }

        // Frames per window: the phase advance a stationary bin accrues between
        // hops is 2*pi*k/oversampling.
        let oversampling = (self.pipeline.fft.size / self.pipeline.hop_size.max(1)) as f32;

        let scratch_re = &mut self.scratch_re;
        let scratch_im = &mut self.scratch_im;
        let prev_phase = &mut self.prev_phase;
        let sum_phase = &mut self.sum_phase;
        let ana_mag = &mut self.ana_mag;
        let ana_freq = &mut self.ana_freq;
        let syn_mag = &mut self.syn_mag;
        let syn_freq = &mut self.syn_freq;

        self.pipeline.process(input, output, |re, im, n, _window, _fft| {
            let n_half = n / 2;
            let bins = n_half + 1;

            // --- Analysis: magnitude and TRUE frequency (in bins) per bin ---
            for k in 0..bins {
                let real = re[k];
                let imag = im[k];
                let mag = (real * real + imag * imag).sqrt();
                let phase = imag.atan2(real);

                // Deviation from the phase advance this bin would accrue if it
                // sat exactly on its centre frequency.
                let expected = TWO_PI * k as f32 / oversampling;
                let delta = wrap_phase(phase - prev_phase[k] - expected);
                prev_phase[k] = phase;

                ana_mag[k] = mag;
                ana_freq[k] = k as f32 + delta * oversampling / TWO_PI;
            }

            // --- Shift: move each partial to its scaled frequency ---
            syn_mag[..bins].fill(0.0);
            syn_freq[..bins].fill(0.0);
            for k in 0..bins {
                let target = (k as f32 * ratio).round() as usize;
                if target < bins {
                    // Partials can collide; magnitudes add.
                    syn_mag[target] += ana_mag[k];
                    syn_freq[target] = ana_freq[k] * ratio;
                }
            }

            // Rounding each bin to the nearest target does not conserve energy:
            // a partial spread over several bins may collapse onto fewer (adding
            // coherently, gaining level) or scatter into sparser ones (losing it),
            // and bins beyond Nyquist/ratio drop out entirely. Left alone that
            // swings the output by about +/-3.5 dB depending purely on the
            // interval. Rescaling the frame back to the analysis energy makes the
            // shift level-transparent while still tracking the input envelope.
            let ana_energy: f32 = ana_mag[..bins].iter().map(|m| m * m).sum();
            let syn_energy: f32 = syn_mag[..bins].iter().map(|m| m * m).sum();
            if syn_energy > 1e-20 {
                let correction = (ana_energy / syn_energy).sqrt();
                for m in syn_mag[..bins].iter_mut() {
                    *m *= correction;
                }
            }

            // --- Synthesis: accumulate phase at each bin's new frequency ---
            scratch_re[..n].fill(0.0);
            scratch_im[..n].fill(0.0);
            for k in 0..bins {
                // Substituting the expected advance into the deviation form
                // collapses the increment to this single term.
                sum_phase[k] = wrap_phase(sum_phase[k] + TWO_PI * syn_freq[k] / oversampling);
                let (sin_p, cos_p) = sum_phase[k].sin_cos();
                let real = syn_mag[k] * cos_p;
                let imag = syn_mag[k] * sin_p;

                scratch_re[k] = real;
                scratch_im[k] = imag;
                // Hermitian symmetry keeps the inverse transform real-valued.
                if k > 0 && k < n_half {
                    scratch_re[n - k] = real;
                    scratch_im[n - k] = -imag;
                }
            }
            // DC and Nyquist have no imaginary part in a real signal.
            scratch_im[0] = 0.0;
            if n_half < n { scratch_im[n_half] = 0.0; }

            re.copy_from_slice(&scratch_re[..n]);
            im.copy_from_slice(&scratch_im[..n]);
        });
    }
}

/// Phase-vocoder pitch shifter.
///
/// Shifting a spectrum by moving bin MAGNITUDES alone does not work: each bin's
/// phase must also advance at the rate its new frequency implies, or successive
/// overlapping frames sum incoherently and cancel. That cancellation is not
/// subtle — it cost 51-85% of the signal level, varying erratically with the
/// interval (see the regression test) — and it also smears transients.
///
/// So this tracks per-bin phase across frames: the deviation from each bin's
/// expected advance gives the partial's true frequency, that frequency is
/// scaled by the pitch ratio, and the synthesis phase is accumulated from it.
///
/// Frequencies are carried in BIN units rather than Hz, which makes the whole
/// thing sample-rate independent and collapses the synthesis phase increment to
/// `2*pi * true_bin / oversampling`.
///
/// Stereo: one `VocoderLane` per channel — phase state is per-signal, so
/// channels must never share a lane.
///
/// KNOWN LIMITATION: bins are remapped by rounding to the nearest target, so a
/// partial's bins can still drift in relative phase and the level sags on large
/// upward shifts — measured RMS runs 0.64 at +5 and +12 semitones against
/// 0.84-0.96 elsewhere (worst case about -3.9 dB). Classic identity phase
/// locking does NOT fix this: it assumes bin frequencies stay put, so applied
/// after a remap it detunes the result (measured +5 st landing at 606 Hz
/// instead of 587). Closing the gap properly means shifting by time-stretch
/// plus resampling rather than by bin remapping.
pub struct KeySyncProcessor {
    pub id: u64,
    semitones: f32,
    ratio: f32,
    lanes: Vec<VocoderLane>,
}

impl KeySyncProcessor {
    pub fn new(id: u64, fft_size: usize) -> Self {
        Self {
            id,
            semitones: 0.0,
            ratio: 1.0,
            lanes: (0..STEREO_LANES).map(|_| VocoderLane::new(fft_size)).collect(),
        }
    }

    pub fn set_semitones(&mut self, semitones: f32) {
        if semitones == self.semitones {
            return;
        }
        self.semitones = semitones;
        self.ratio = 2.0f32.powf(semitones / 12.0);
        // The accumulated phase describes the OLD ratio; carrying it across a
        // change makes the first frames after the change beat against
        // themselves. Start the new interval from a clean slate.
        for lane in self.lanes.iter_mut() {
            lane.clear_phase_state();
        }
    }
}

/// Wrap a phase deviation into (-pi, pi].
#[inline]
fn wrap_phase(x: f32) -> f32 {
    x - TWO_PI * (x / TWO_PI).round()
}

impl nullherz_traits::SignalProcessor for KeySyncProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        let n_ch = inputs.len().min(outputs.len());
        for ch in 0..n_ch {
            match self.lanes.get_mut(ch) {
                Some(lane) => lane.process(inputs[ch], outputs[ch], self.ratio),
                // Wired wider than we have lanes: pass through unshifted
                // rather than starve the channel with silence.
                None => {
                    let n = inputs[ch].len().min(outputs[ch].len());
                    outputs[ch][..n].copy_from_slice(&inputs[ch][..n]);
                }
            }
        }
    }

    fn setup(&mut self, _config: AudioConfig) {
        for lane in self.lanes.iter_mut() {
            lane.reset();
        }
    }

    fn reset(&mut self) {
        for lane in self.lanes.iter_mut() {
            lane.reset();
        }
    }
}

impl nullherz_traits::MidiResponder for KeySyncProcessor {}
impl nullherz_traits::SnapshotProvider for KeySyncProcessor {}

impl AudioProcessor for KeySyncProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        if param_id == 0 {
            self.set_semitones(value);
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        if param_id == 0 { self.semitones } else { 0.0 }
    }

    fn metadata(&self) -> Option<ProcessorMetadata> {
        let mut parameters = [ParameterMetadata { id: 0, name: [0; 32], min: 0.0, max: 0.0, default: 0.0 }; 16];
        parameters[0] = ParameterMetadata {
            id: 0,
            name: *b"Semitones\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
            min: -12.0,
            max: 12.0,
            default: 0.0,
        };
        Some(ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 1,
            parameters,
        })
    }
}

#[cfg(test)]
mod keysync_tests {
    use super::*;
    use nullherz_traits::SignalProcessor;

    /// Push a steady sine through the processor and return
    /// (rms_ratio, peak_ratio, dominant_output_frequency_hz).
    fn measure(semitones: f32) -> (f32, f32, f32) {
        const SR: f32 = 44100.0;
        const FREQ: f32 = 440.0;
        const BLOCK: usize = 128;
        const BLOCKS: usize = 260;
        // Skip pipeline priming and the phase-accumulator settling frames.
        const WARMUP_BLOCKS: usize = 60;

        let mut p = KeySyncProcessor::new(0, 1024);
        AudioProcessor::set_parameter(&mut p, 0, semitones, 0);

        let mut captured_in = Vec::new();
        let mut captured_out = Vec::new();

        for b in 0..BLOCKS {
            let base = (b * BLOCK) as f32;
            let inbuf: Vec<f32> = (0..BLOCK)
                .map(|i| (TWO_PI * FREQ * (base + i as f32) / SR).sin() * 0.5)
                .collect();
            let mut outbuf = vec![0.0f32; BLOCK];
            {
                let ins: [&[f32]; 1] = [&inbuf];
                let mut slot = &mut outbuf[..];
                let mut outs: [&mut [f32]; 1] = [&mut slot];
                let mut ctx = ProcessContext {
                    transport: None,
                    host: None,
                    sub_block_offset: 0,
                    is_last_sub_block: true,
                };
                p.process(&ins, &mut outs, &mut ctx);
            }
            if b >= WARMUP_BLOCKS {
                captured_in.extend_from_slice(&inbuf);
                captured_out.extend_from_slice(&outbuf);
            }
        }

        let rms = |s: &[f32]| (s.iter().map(|v| (v * v) as f64).sum::<f64>() / s.len() as f64).sqrt() as f32;
        let peak = |s: &[f32]| s.iter().fold(0.0f32, |a, b| a.max(b.abs()));

        // Dominant frequency by zero-crossing rate: cheap, and adequate to
        // confirm the shift lands on the right pitch.
        let crossings = captured_out
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        let dominant_hz = crossings as f32 * SR / captured_out.len() as f32;

        (
            rms(&captured_out) / rms(&captured_in),
            peak(&captured_out) / peak(&captured_in),
            dominant_hz,
        )
    }

    /// Unison must be a transparent bypass.
    #[test]
    fn test_unison_is_unity_gain() {
        let (rms_ratio, peak_ratio, _) = measure(0.0);
        assert!(
            (rms_ratio - 1.0).abs() < 0.05 && (peak_ratio - 1.0).abs() < 0.05,
            "0 semitones must pass through at unity: rms {:.4}, peak {:.4}",
            rms_ratio,
            peak_ratio
        );
    }

    /// The defect this replaces: remapping bin magnitudes without advancing
    /// each partial's phase made overlapping frames cancel, costing 51-85% of
    /// the level (measured peak ratios ran 0.15-0.49 across these intervals).
    /// A pitch shift must preserve level, not act as a random attenuator.
    #[test]
    fn test_pitch_shift_preserves_level() {
        for &semis in &[-7.0f32, -5.0, -2.0, 1.0, 2.0, 5.0, 7.0] {
            let (rms_ratio, _, _) = measure(semis);
            assert!(
                rms_ratio > 0.6 && rms_ratio < 1.4,
                "{:+.0} semitones: RMS ratio {:.4} — a pitch shift must preserve level",
                semis,
                rms_ratio
            );
        }
    }

    /// A pitch shift is only correct if it actually lands on the target pitch.
    #[test]
    fn test_pitch_shift_reaches_target_frequency() {
        for &semis in &[-5.0f32, 2.0, 7.0] {
            let (_, _, dominant_hz) = measure(semis);
            let expected = 440.0 * 2.0f32.powf(semis / 12.0);
            let cents = 1200.0 * (dominant_hz / expected).log2();
            assert!(
                cents.abs() < 60.0,
                "{:+.0} semitones: got {:.1} Hz, expected {:.1} Hz ({:+.0} cents off)",
                semis,
                dominant_hz,
                expected,
                cents
            );
        }
    }

    /// Stereo independence: a hot left channel must not bleed into a silent
    /// right channel, and the right channel's silence must not corrupt the
    /// left channel's phase tracking (they were one shared lane before).
    #[test]
    fn test_stereo_channels_are_independent() {
        const SR: f32 = 44100.0;
        const BLOCK: usize = 128;
        const BLOCKS: usize = 260;
        const WARMUP_BLOCKS: usize = 60;

        let mut p = KeySyncProcessor::new(0, 1024);
        AudioProcessor::set_parameter(&mut p, 0, 7.0, 0);

        let mut left_out = Vec::new();
        let mut right_out = Vec::new();

        for b in 0..BLOCKS {
            let base = (b * BLOCK) as f32;
            let l_in: Vec<f32> = (0..BLOCK)
                .map(|i| (TWO_PI * 440.0 * (base + i as f32) / SR).sin() * 0.5)
                .collect();
            let r_in = vec![0.0f32; BLOCK];
            let mut l_buf = vec![0.0f32; BLOCK];
            let mut r_buf = vec![0.0f32; BLOCK];
            {
                let ins: [&[f32]; 2] = [&l_in, &r_in];
                let (l_slot, r_slot) = (&mut l_buf[..], &mut r_buf[..]);
                let mut outs: [&mut [f32]; 2] = [l_slot, r_slot];
                let mut ctx = ProcessContext {
                    transport: None,
                    host: None,
                    sub_block_offset: 0,
                    is_last_sub_block: true,
                };
                p.process(&ins, &mut outs, &mut ctx);
            }
            if b >= WARMUP_BLOCKS {
                left_out.extend_from_slice(&l_buf);
                right_out.extend_from_slice(&r_buf);
            }
        }

        let rms = |s: &[f32]| (s.iter().map(|v| (v * v) as f64).sum::<f64>() / s.len() as f64).sqrt() as f32;
        let l_rms = rms(&left_out);
        let r_rms = rms(&right_out);
        assert!(l_rms > 0.2, "left channel must survive the shift, rms {:.4}", l_rms);
        assert!(r_rms < 1e-4, "silent right channel must stay silent, rms {:.6}", r_rms);
    }
}
