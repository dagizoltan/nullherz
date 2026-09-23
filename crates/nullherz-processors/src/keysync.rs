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
/// KNOWN LIMITATION: bins are remapped by ROUNDING to the nearest target, and
/// that rounding is now the component's ceiling. Two measured consequences
/// (`examples/probe_keysync_quality.rs`):
///
/// - **Level sag, and it is timbral.** The fundamental lands up to **-9.8 dB**
///   low, and on a three-note chord the partials do not sag together: 7.65 dB of
///   spread at -3 semitones, with one partial rising while another falls. So no
///   makeup gain can correct it — a scalar tuned on a sine would mis-level every
///   chord. The figure used to be quoted as "about -3.9 dB" from an RMS
///   measurement, which was reading artifact energy as signal; see
///   `test_level_is_read_in_the_lobe_not_the_rms`.
/// - **Frequency resolution caps the polyphonic result.** Signal-to-artifact on
///   a chord is -17.8 dB at N=1024, -33.6 dB at N=2048, -61.3 dB at N=4096 —
///   set by the window length, essentially independent of the hop. That axis
///   trades against latency (21.3 / 42.7 / 85.3 ms of PDC on every deck
///   carrying KEY), so it cannot be won by a vocoder at a latency a DJ can beat
///   match against.
///
/// Classic identity phase locking does NOT fix either: it assumes bin
/// frequencies stay put, so applied after a remap it detunes the result
/// (measured +5 st landing at 606 Hz instead of 587). Closing the gap properly
/// means shifting by time-stretch plus resampling rather than by bin remapping —
/// which puts the pitch change back through the resampler that already measures
/// -132 dB, and leaves the vocoder responsible for time alone.
pub struct KeySyncProcessor {
    pub id: u64,
    semitones: f32,
    ratio: f32,
    lanes: Vec<VocoderLane>,
}

impl KeySyncProcessor {
    /// 87.5% overlap — a hop of `fft_size / 8`.
    ///
    /// This was `fft_size / 2`, the pipeline's own default, and at that framing
    /// the vocoder is not a quality compromise but a broken component:
    /// `examples/probe_keysync_quality.rs` measures **-0.4 dB** of
    /// signal-to-artifact at +7 semitones on a single 997 Hz tone. Artifact
    /// energy equal to the signal. Across every interval the parameter allows it
    /// ran -0.4 to -15 dB, against -132 dB for the varispeed resampler that does
    /// the same job with keylock off.
    ///
    /// The cause is the phase extrapolation: a partial's frequency is estimated
    /// from the phase advance between frames, and at 50% overlap that estimate
    /// is stretched across half a window. Dividing the hop by four takes the
    /// worst interval to **-43.0 dB**, and it costs no latency at all — latency
    /// is one analysis window whatever the hop. It costs frame rate, so four
    /// times the FFT work, on a node that is detached by default and only
    /// instantiated when an operator engages KEY.
    ///
    /// `fft_size / 16` buys another 19 dB on a single tone and, measured,
    /// **nothing** on a chord (-17.8 vs -18.4 dB) for twice the CPU again. N/8
    /// is the knee.
    ///
    /// What the hop does NOT fix is frequency resolution, which is set by
    /// `fft_size`: the polyphonic worst case is -17.8 dB here, -33.6 dB at
    /// N=2048 and -61.3 dB at N=4096, essentially independent of the hop. That
    /// axis trades against latency (21.3 / 42.7 / 85.3 ms) and cannot be won by
    /// a vocoder at DJ-acceptable latency. Closing it properly means shifting by
    /// time-stretch plus resampling, which puts the pitch change back through
    /// the -132 dB resampler — see the KNOWN LIMITATION note on the struct.
    pub fn new(id: u64, fft_size: usize) -> Self {
        Self::with_framing(id, fft_size, fft_size / 8)
    }

    /// Explicit framing: window length AND hop.
    ///
    /// The hop is the quality knob nobody could reach. `new` picks
    /// `fft_size / 2` — 50% overlap, two frames per window — which is the
    /// cheapest framing that reconstructs at all and the coarsest one anybody
    /// ships. A phase vocoder's artifacts are dominated by how far a partial's
    /// phase estimate has to be extrapolated between frames, so halving the hop
    /// halves that extrapolation. It also doubles the frame rate, and with it
    /// the FFT cost; the trade is measured in
    /// `examples/probe_keysync_quality.rs`.
    ///
    /// Latency is unchanged by the hop — it is one analysis window either way,
    /// which is what [`SignalProcessor::latency_samples`] reports.
    pub fn with_framing(id: u64, fft_size: usize, hop_size: usize) -> Self {
        Self {
            id,
            semitones: 0.0,
            ratio: 1.0,
            lanes: (0..STEREO_LANES)
                .map(|_| {
                    let mut lane = VocoderLane::new(fft_size);
                    lane.pipeline.set_hop_size(hop_size);
                    lane
                })
                .collect(),
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
    /// One analysis window, always.
    ///
    /// This was inheriting the trait default of `0` while running a phase
    /// vocoder — so every deck in the console carried **1024 samples (21.3 ms at
    /// 48 kHz) that the graph believed was zero**. Measured end to end: an
    /// impulse fed to a deck emerged at master after 1376 samples (28.7 ms)
    /// while `latency_samples()` summed to 0 across every node
    /// (`conductor/examples/probe_deck_latency.rs`).
    ///
    /// PDC is only as good as this number. A path latency of 0 means the
    /// compiler aligns nothing, the RTL calibration compensates the wrong
    /// amount, and any future chain where decks differ is silently misaligned by
    /// the difference. Unconditional here because the vocoder runs on every
    /// block regardless of `ratio` — a lane at ratio 1.0 still goes through the
    /// window, so the delay is there whether or not the pitch is being shifted.
    fn latency_samples(&self) -> usize {
        self.lanes.first().map(|l| l.pipeline.fft.size).unwrap_or(0)
    }

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

    /// Probe constants, shared with `examples/probe_keysync_quality.rs` so a
    /// number here and a number there are the same measurement.
    const P_SR: f32 = 48_000.0;
    const P_TONE: f32 = 997.0;
    const P_AMP: f32 = 0.5;
    /// 8192 rather than the probe's 16384: the analyser is an f64 transform with
    /// a `sin_cos` per butterfly, and these tests run in debug too.
    const P_ANA: usize = 8192;

    /// Level error in the shifted fundamental's own main lobe, and the energy
    /// outside it, for one interval at one framing.
    ///
    /// The lobe — not the capture's RMS. See
    /// `test_level_is_read_in_the_lobe_not_the_rms` for why that distinction is
    /// the whole point of this helper.
    fn measure_spectral(semitones: f32, fft_size: usize, hop: usize) -> Measured {
        use audio_dsp::measurement::{level_db_at, spectrum, thd_n, tone_sample};

        let warmup = 8 * fft_size;
        let total = warmup + P_ANA;
        let input: Vec<f32> = (0..total).map(|i| tone_sample(i, P_TONE, P_SR, P_AMP)).collect();

        let mut p = KeySyncProcessor::with_framing(0, fft_size, hop);
        AudioProcessor::set_parameter(&mut p, 0, semitones, 0);

        let mut out = vec![0.0f32; total];
        for start in (0..total).step_by(256) {
            let end = (start + 256).min(total);
            let ins: [&[f32]; 1] = [&input[start..end]];
            let mut outs: [&mut [f32]; 1] = [&mut out[start..end]];
            let mut ctx = ProcessContext {
                transport: None,
                host: None,
                sub_block_offset: 0,
                is_last_sub_block: true,
            };
            p.process(&ins, &mut outs, &mut ctx);
        }

        let settled = &out[warmup..];
        let target = P_TONE * 2.0f32.powf(semitones / 12.0);
        let db = |x: f32| 20.0 * x.max(1e-30).log10();

        let rms = |x: &[f32]| {
            (x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len() as f64).sqrt() as f32
        };

        Measured {
            lobe: level_db_at(&spectrum(settled, P_ANA), target, P_SR, P_ANA)
                - level_db_at(&spectrum(&input[..P_ANA], P_ANA), P_TONE, P_SR, P_ANA),
            rms: db(rms(settled) / rms(&input[..P_ANA]).max(1e-30)),
            artifacts: db(thd_n(settled, target, P_SR, P_ANA)),
        }
    }

    /// What one interval at one framing measures.
    struct Measured {
        /// Level of the shifted fundamental's main lobe, relative to the input's.
        lobe: f32,
        /// Level of the whole capture's RMS, relative to the input's. Differs
        /// from `lobe` by however much artifact energy there is to mistake for
        /// signal.
        rms: f32,
        /// Energy outside the fundamental's lobe, against the energy inside it.
        artifacts: f32,
    }

    /// The hop is the difference between a working vocoder and a broken one, and
    /// nothing used to hold it in place.
    ///
    /// `new` picks `fft_size / 8`. At the pipeline's own `fft_size / 2` default,
    /// which this shipped with, the signal-to-artifact ratio at +7 semitones is
    /// **-0.4 dB** — artifact energy equal to the signal. At N/8 it is -56 dB.
    /// Asserted at -35 dB: far enough below the real figure to survive CPU
    /// arithmetic differences, and four orders of magnitude away from what the
    /// old framing produces, so a revert cannot pass this.
    #[test]
    fn test_default_hop_is_at_the_quality_knee() {
        let hop = {
            let p = KeySyncProcessor::new(0, 1024);
            let lane = p.lanes.first().expect("a lane");
            lane.pipeline.hop_size
        };
        assert_eq!(hop, 1024 / 8, "the default framing is 87.5% overlap");

        for &semis in &[-7.0f32, -5.0, 1.0, 5.0, 7.0, 12.0] {
            let art = measure_spectral(semis, 1024, hop).artifacts;
            assert!(
                art < -35.0,
                "{semis:+.0} semitones: signal-to-artifact {art:.1} dB. The phase \
                 estimate is extrapolated across the hop, so a wider hop puts \
                 artifact energy at the signal's own level."
            );
        }
    }

    /// A pitch shift must not act as a random attenuator.
    ///
    /// Two separate defects have lived in this assertion. The first was real
    /// cancellation: remapping bin magnitudes without advancing each partial's
    /// phase cost 51-85% of the level (peak ratios 0.15-0.49). The second was in
    /// the test — it measured the whole capture's RMS, which at the old 50%
    /// overlap counted artifact energy sitting at the signal's own level AS
    /// signal, and reported a level 2.8 dB better than the audio was. It passed
    /// on that padding; remove the artifacts and the same audio fails it.
    ///
    /// So the bound here is deliberately wide and the reason is documented
    /// rather than hidden: the remaining sag is the bin-rounding remap, measured
    /// at -9.8 dB worst case across the interval range
    /// (`examples/probe_keysync_quality.rs`), and it is **timbral** — on a
    /// three-note chord the partials do not move together (7.65 dB of spread,
    /// one partial rising while another falls), so no makeup gain can correct
    /// it. Tightening this bound means replacing the remap with time-stretch
    /// plus resampling, not adjusting a constant. -13 dB leaves 3 dB of headroom
    /// over the measured worst case; +1 dB catches a shift that gets LOUDER.
    #[test]
    fn test_pitch_shift_preserves_level() {
        for &semis in &[-12.0f32, -7.0, -5.0, -2.0, 1.0, 2.0, 5.0, 7.0, 12.0] {
            let lobe = measure_spectral(semis, 1024, 1024 / 8).lobe;
            assert!(
                (-13.0..=1.0).contains(&lobe),
                "{semis:+.0} semitones: fundamental landed {lobe:+.2} dB off. Read in \
                 the lobe, not the capture RMS — RMS credits artifacts as signal."
            );
        }
    }

    /// The instrument check for the test above: an RMS reading of a pitch shift
    /// counts artifacts as signal, and at the old framing there were enough of
    /// them to change the answer by 2.8 dB.
    ///
    /// This is the guard that stops the level assertion from ever being "fixed"
    /// by reverting to an RMS measurement, which would make it pass by
    /// readmitting the distortion it is supposed to be blind to. At +7
    /// semitones, the worst interval on the legacy framing:
    ///
    /// - legacy hop N/2: RMS -5.4 dB, lobe -8.3 dB. The 2.8 dB gap IS the
    ///   artifact energy, and `test_pitch_shift_preserves_level` used to pass on
    ///   it while the audio was at -0.4 dB signal-to-artifact.
    /// - default hop N/8: RMS and lobe agree to a hundredth of a dB, because
    ///   there is nothing left to pad with.
    ///
    /// Note which direction that runs: the clean framing reports the fundamental
    /// at a level no HIGHER than the dirty one. Removing artifacts does not
    /// recover level. The sag is a separate, unfixed defect.
    #[test]
    fn test_level_is_read_in_the_lobe_not_the_rms() {
        let legacy = measure_spectral(7.0, 1024, 1024 / 2);
        let clean = measure_spectral(7.0, 1024, 1024 / 8);

        assert!(
            legacy.artifacts > -20.0,
            "the legacy 50% overlap is supposed to be audibly broken here, got \
             {:.1} dB — if it is not, this test has stopped demonstrating anything",
            legacy.artifacts
        );
        assert!(clean.artifacts < -35.0, "the default framing should be clean, got {:.1} dB", clean.artifacts);

        let legacy_gap = (legacy.rms - legacy.lobe).abs();
        let clean_gap = (clean.rms - clean.lobe).abs();

        assert!(
            legacy_gap > 1.5,
            "at {:.1} dB signal-to-artifact, RMS ({:+.2}) and lobe ({:+.2}) must \
             disagree — that gap is the artifact energy an RMS reading books as signal",
            legacy.artifacts, legacy.rms, legacy.lobe
        );
        assert!(
            clean_gap < 0.3,
            "with artifacts at {:.1} dB there is nothing to pad the RMS with, so RMS \
             ({:+.2}) and lobe ({:+.2}) must agree",
            clean.artifacts, clean.rms, clean.lobe
        );
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
