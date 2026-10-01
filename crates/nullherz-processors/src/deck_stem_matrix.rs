use nullherz_traits::{
    AudioProcessor, ProcessContext, ProcessorMetadata, ParameterMetadata,
    SignalProcessor, RtSafe, SnapshotProvider, MidiResponder, MeasurementBlock,
};
use audio_dsp::DjIsolatorStereo;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use crate::analysis::AnalysisBus;

pub const MAX_STEMS: usize = 12;

pub struct StemControl {
    pub mute: AtomicBool,
    pub solo: AtomicBool,
    pub target_gain_db: AtomicU32, // f32 bits
    pub target_pan: AtomicU32,     // f32 bits
    pub eq_low: AtomicU32,         // f32 bits
    pub eq_mid: AtomicU32,         // f32 bits
    pub eq_high: AtomicU32,        // f32 bits
    current_gain_linear: f32,
    current_pan: f32,
    isolator: DjIsolatorStereo,
}

impl StemControl {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            mute: AtomicBool::new(false),
            solo: AtomicBool::new(false),
            target_gain_db: AtomicU32::new(0.0f32.to_bits()), // 0 dB
            target_pan: AtomicU32::new(0.0f32.to_bits()),     // 0.0 center
            eq_low: AtomicU32::new(1.0f32.to_bits()),         // unity
            eq_mid: AtomicU32::new(1.0f32.to_bits()),         // unity
            eq_high: AtomicU32::new(1.0f32.to_bits()),        // unity
            current_gain_linear: 1.0,
            current_pan: 0.0,
            isolator: DjIsolatorStereo::with_sample_rate(sample_rate),
        }
    }
}

pub struct DeckStemMatrixProcessor {
    pub id: u64,
    pub stems: [StemControl; MAX_STEMS],
    pub analysis_bus: Option<Arc<AnalysisBus>>,
    sample_rate: f32,
    scratch_buf_l: [f32; ipc_layer::MAX_BLOCK_SIZE],
    scratch_buf_r: [f32; ipc_layer::MAX_BLOCK_SIZE],
    scratch_iso: [f32; ipc_layer::MAX_BLOCK_SIZE],
}

impl DeckStemMatrixProcessor {
    pub fn new(id: u64, sample_rate: f32) -> Self {
        Self {
            id,
            stems: std::array::from_fn(|_| StemControl::new(sample_rate)),
            analysis_bus: None,
            sample_rate,
            scratch_buf_l: [0.0; ipc_layer::MAX_BLOCK_SIZE],
            scratch_buf_r: [0.0; ipc_layer::MAX_BLOCK_SIZE],
            scratch_iso: [0.0; ipc_layer::MAX_BLOCK_SIZE],
        }
    }

    fn db_to_linear(db: f32) -> f32 {
        if db <= -90.0 {
            0.0
        } else {
            10.0f32.powf(db / 20.0)
        }
    }
}

impl RtSafe for DeckStemMatrixProcessor {}

impl SignalProcessor for DeckStemMatrixProcessor {
    fn setup(&mut self, config: nullherz_traits::AudioConfig) {
        self.sample_rate = config.sample_rate;
        for stem in self.stems.iter_mut() {
            stem.isolator = DjIsolatorStereo::with_sample_rate(config.sample_rate);
        }
    }

    fn reset(&mut self) {
        for stem in self.stems.iter_mut() {
            stem.isolator = DjIsolatorStereo::with_sample_rate(self.sample_rate);
            stem.current_gain_linear = 1.0;
            stem.current_pan = 0.0;
        }
    }

    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], _context: &mut ProcessContext) {
        if outputs.is_empty() { return; }
        let num_samples = outputs[0].len().min(ipc_layer::MAX_BLOCK_SIZE);
        if num_samples == 0 { return; }

        // Clear output buses
        for out in outputs.iter_mut() {
            out[..num_samples].fill(0.0);
        }

        // Check if any stem is soloed
        let any_solo = self.stems.iter().any(|s| s.solo.load(Ordering::Relaxed));

        for idx in 0..MAX_STEMS {
            let stem = &mut self.stems[idx];
            let is_muted = stem.mute.load(Ordering::Relaxed);
            let is_soloed = stem.solo.load(Ordering::Relaxed);

            // Solo logic
            let is_active = if any_solo {
                is_soloed && !is_muted
            } else {
                !is_muted
            };

            if !is_active {
                continue;
            }

            // Input indices for stereo pair
            let in_l_idx = idx * 2;
            let in_r_idx = idx * 2 + 1;

            let has_inputs = in_l_idx < inputs.len();
            if !has_inputs {
                continue;
            }

            let in_l = inputs[in_l_idx];
            let in_r = if in_r_idx < inputs.len() { inputs[in_r_idx] } else { inputs[in_l_idx] };

            let len = num_samples.min(in_l.len()).min(in_r.len());

            // Target gain & pan
            let target_gain_db = f32::from_bits(stem.target_gain_db.load(Ordering::Relaxed));
            let target_gain_linear = Self::db_to_linear(target_gain_db);
            let target_pan = f32::from_bits(stem.target_pan.load(Ordering::Relaxed)).clamp(-1.0, 1.0);

            let eq_l = f32::from_bits(stem.eq_low.load(Ordering::Relaxed));
            let eq_m = f32::from_bits(stem.eq_mid.load(Ordering::Relaxed));
            let eq_h = f32::from_bits(stem.eq_high.load(Ordering::Relaxed));

            stem.isolator.set_gain(0, eq_l);
            stem.isolator.set_gain(1, eq_m);
            stem.isolator.set_gain(2, eq_h);

            // Process through isolator
            stem.isolator.process_stereo(
                &in_l[..len],
                &in_r[..len],
                &mut self.scratch_buf_l[..len],
                &mut self.scratch_buf_r[..len],
            );

            // Apply gain & pan with smooth linear interpolation
            let start_gain = stem.current_gain_linear;
            let start_pan = stem.current_pan;
            let step_gain = (target_gain_linear - start_gain) / len as f32;
            let step_pan = (target_pan - start_pan) / len as f32;

            let (out_l_slice, out_r_slice) = outputs.split_at_mut(1);
            let out_l = &mut out_l_slice[0][..len];
            let out_r = if !out_r_slice.is_empty() {
                &mut out_r_slice[0][..len]
            } else {
                &mut self.scratch_iso[..len]
            };

            for i in 0..len {
                let g = start_gain + step_gain * i as f32;
                let p = start_pan + step_pan * i as f32;

                // Constant power panning law
                let pan_rad = (p + 1.0) * (std::f32::consts::PI / 4.0);
                let gain_l = g * pan_rad.cos();
                let gain_r = g * pan_rad.sin();

                out_l[i] += self.scratch_buf_l[i] * gain_l;
                out_r[i] += self.scratch_buf_r[i] * gain_r;
            }

            stem.current_gain_linear = target_gain_linear;
            stem.current_pan = target_pan;
        }

        // Tap active stem outputs to AnalysisKernel / AnalysisBus
        if let Some(ref bus) = self.analysis_bus {
            let mut block = MeasurementBlock::default();
            block.sample_position = _context.transport.map(|t| t.absolute_samples).unwrap_or(0);
            let mut sum_sq_l = 0.0f32;
            let mut sum_sq_r = 0.0f32;
            let out_l = &outputs[0][..num_samples];
            let out_r = if outputs.len() > 1 { &outputs[1][..num_samples] } else { &outputs[0][..num_samples] };
            for i in 0..num_samples {
                sum_sq_l += out_l[i] * out_l[i];
                sum_sq_r += out_r[i] * out_r[i];
            }
            let rms_l = (sum_sq_l / num_samples as f32).sqrt();
            let rms_r = (sum_sq_r / num_samples as f32).sqrt();
            block.rms_db = [
                20.0 * rms_l.max(1e-5).log10(),
                20.0 * rms_r.max(1e-5).log10(),
            ];
            bus.push_measurement(block);
        }
    }
}

impl MidiResponder for DeckStemMatrixProcessor {
    fn apply_midi(&mut self, _event: ipc_layer::MidiEvent, _context: Option<&ProcessContext>) {}
}

impl SnapshotProvider for DeckStemMatrixProcessor {}

impl AudioProcessor for DeckStemMatrixProcessor {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn apply_command(&mut self, command: &nullherz_traits::ProcessorCommand) {
        if let nullherz_traits::Command::Mixer(nullherz_traits::MixerCommand::SetParam { target_id, param_id, value, ramp_duration_samples }) = *command {
            if target_id == self.id {
                self.set_parameter(param_id, value, ramp_duration_samples);
            }
        }
    }

    fn set_parameter(&mut self, param_id: u32, value: f32, _ramp_duration_samples: u32) {
        let stem_idx = (param_id / 10) as usize;
        let control_type = param_id % 10;

        if stem_idx < MAX_STEMS {
            let stem = &self.stems[stem_idx];
            match control_type {
                0 => stem.mute.store(value > 0.5, Ordering::Relaxed),
                1 => stem.solo.store(value > 0.5, Ordering::Relaxed),
                2 => {
                    let sanitized = if !value.is_finite() { -96.0 } else { value.clamp(-120.0, 24.0) };
                    stem.target_gain_db.store(sanitized.to_bits(), Ordering::Relaxed);
                }
                3 => {
                    let sanitized = if !value.is_finite() { 0.0 } else { value.clamp(-1.0, 1.0) };
                    stem.target_pan.store(sanitized.to_bits(), Ordering::Relaxed);
                }
                4 => {
                    let sanitized = if !value.is_finite() { 1.0 } else { value.clamp(0.0, 10.0) };
                    stem.eq_low.store(sanitized.to_bits(), Ordering::Relaxed);
                }
                5 => {
                    let sanitized = if !value.is_finite() { 1.0 } else { value.clamp(0.0, 10.0) };
                    stem.eq_mid.store(sanitized.to_bits(), Ordering::Relaxed);
                }
                6 => {
                    let sanitized = if !value.is_finite() { 1.0 } else { value.clamp(0.0, 10.0) };
                    stem.eq_high.store(sanitized.to_bits(), Ordering::Relaxed);
                }
                _ => {}
            }
        }
    }

    fn get_parameter(&self, param_id: u32) -> f32 {
        let stem_idx = (param_id / 10) as usize;
        let control_type = param_id % 10;

        if stem_idx < MAX_STEMS {
            let stem = &self.stems[stem_idx];
            match control_type {
                0 => if stem.mute.load(Ordering::Relaxed) { 1.0 } else { 0.0 },
                1 => if stem.solo.load(Ordering::Relaxed) { 1.0 } else { 0.0 },
                2 => f32::from_bits(stem.target_gain_db.load(Ordering::Relaxed)),
                3 => f32::from_bits(stem.target_pan.load(Ordering::Relaxed)),
                4 => f32::from_bits(stem.eq_low.load(Ordering::Relaxed)),
                5 => f32::from_bits(stem.eq_mid.load(Ordering::Relaxed)),
                6 => f32::from_bits(stem.eq_high.load(Ordering::Relaxed)),
                _ => 0.0,
            }
        } else {
            0.0
        }
    }

    fn metadata(&self) -> Option<ProcessorMetadata> {
        let mut parameters = [ParameterMetadata {
            id: 0,
            name: [0; 32],
            min: -96.0,
            max: 12.0,
            default: 0.0,
        }; 16];

        for i in 0..12 {
            parameters[i].id = (i * 10 + 2) as u32; // Stem gain
            let name_str = format!("Stem{}Gain", i + 1);
            let bytes = name_str.as_bytes();
            parameters[i].name[..bytes.len()].copy_from_slice(bytes);
            parameters[i].min = -96.0;
            parameters[i].max = 12.0;
            parameters[i].default = 0.0;
        }

        Some(ProcessorMetadata {
            processor_id: self.id,
            num_parameters: 12,
            parameters,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nullherz_traits::{AudioConfig, ProcessContext, Command, MixerCommand};

    #[test]
    fn test_deck_stem_matrix_summing_and_mute_solo() {
        let sample_rate = 48000.0;
        let mut proc = DeckStemMatrixProcessor::new(1, sample_rate);
        proc.setup(AudioConfig { sample_rate, block_size: 256 });

        let stem0_l = vec![0.5f32; 256];
        let stem0_r = vec![0.5f32; 256];
        let stem1_l = vec![0.25f32; 256];
        let stem1_r = vec![0.25f32; 256];

        let inputs: Vec<&[f32]> = vec![
            &stem0_l[..], &stem0_r[..],
            &stem1_l[..], &stem1_r[..],
        ];

        let mut out_l = vec![0.0f32; 256];
        let mut out_r = vec![0.0f32; 256];

        let mut ctx = ProcessContext {
            transport: None,
            host: None,
            sub_block_offset: 0,
            is_last_sub_block: true,
        };

        // 1. Unmuted, no solo: sums stem0 and stem1
        {
            let mut outputs: Vec<&mut [f32]> = vec![&mut out_l[..], &mut out_r[..]];
            proc.process(&inputs, &mut outputs[..], &mut ctx);
        }
        // After 1 block, isolator and gain land near sum (0.5 + 0.25 = ~0.75 * pan_gain)
        assert!(out_l[200] > 0.4, "out_l should be non-zero sum, got {}", out_l[200]);

        // 2. Mute stem0 -> only stem1 should contribute
        proc.set_parameter(0, 1.0, 0); // Mute stem 0
        {
            let mut outputs: Vec<&mut [f32]> = vec![&mut out_l[..], &mut out_r[..]];
            proc.process(&inputs, &mut outputs[..], &mut ctx);
        }
        let peak_muted = out_l[200];

        // 3. Unmute stem0, Solo stem0 -> only stem0 should contribute
        proc.set_parameter(0, 0.0, 0); // Unmute stem 0
        proc.set_parameter(1, 1.0, 0); // Solo stem 0
        {
            let mut outputs: Vec<&mut [f32]> = vec![&mut out_l[..], &mut out_r[..]];
            proc.process(&inputs, &mut outputs[..], &mut ctx);
        }
        let peak_soloed = out_l[200];

        assert!(peak_soloed > peak_muted, "soloed stem 0 should contribute more than stem 1 alone");
    }

    #[test]
    fn test_deck_stem_matrix_parameter_smoothing() {
        let sample_rate = 48000.0;
        let mut proc = DeckStemMatrixProcessor::new(1, sample_rate);
        proc.setup(AudioConfig { sample_rate, block_size: 256 });

        // Set Stem 0 Gain to +6 dB
        proc.apply_command(&Command::Mixer(MixerCommand::SetParam {
            target_id: 1,
            param_id: 2, // Stem 0 Gain
            value: 6.0,
            ramp_duration_samples: 0,
        }));

        assert_eq!(proc.get_parameter(2), 6.0);
    }
}
