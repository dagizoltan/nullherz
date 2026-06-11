pub trait Oscillator {
    fn next_sample(&mut self) -> f32;
    fn process_block(&mut self, output: &mut [f32]) {
        for sample in output.iter_mut() {
            *sample = self.next_sample();
        }
    }
}

const LUT_SIZE: usize = 1024;

/// A Sine Oscillator using a Look-Up Table for performance.
pub struct SineOscillator {
    pub phase: f32,
    pub phase_inc: f32,
    pub sample_rate: f32,
    pub lut: [f32; LUT_SIZE],
}

impl SineOscillator {
    pub fn new(sample_rate: f32, frequency: f32) -> Self {
        let mut lut = [0.0f32; LUT_SIZE];
        for (i, val) in lut.iter_mut().enumerate() {
            *val = ((i as f32 * 2.0 * std::f32::consts::PI) / LUT_SIZE as f32).sin();
        }
        Self {
            phase: 0.0,
            phase_inc: (frequency * LUT_SIZE as f32) / sample_rate,
            sample_rate,
            lut,
        }
    }

    pub fn set_frequency(&mut self, frequency: f32) {
        self.phase_inc = (frequency * LUT_SIZE as f32) / self.sample_rate;
    }
}

impl Oscillator for SineOscillator {
    fn next_sample(&mut self) -> f32 {
        let idx = self.phase as usize % LUT_SIZE;
        let sample = self.lut[idx];
        self.phase += self.phase_inc;
        if self.phase >= LUT_SIZE as f32 {
            self.phase -= LUT_SIZE as f32;
        }
        sample
    }
}

/// A high-performance Wavetable Oscillator with SIMD support and FM/PM.
#[repr(C, align(64))]
pub struct WavetableOscillator {
    pub table: [f32; 2048],
    pub(crate) phases: [f32; 16],
    pub(crate) phase_incs: [f32; 16],
    pub sample_rate: f32,
}

impl WavetableOscillator {
    pub fn new(sample_rate: f32) -> Self {
        let mut table = [0.0f32; 2048];
        for (i, val) in table.iter_mut().enumerate() {
            *val = ((i as f32 * 2.0 * std::f32::consts::PI) / 2048.0).sin();
        }
        Self {
            table,
            phases: [0.0; 16],
            phase_incs: [0.0; 16],
            sample_rate,
        }
    }

    pub fn set_frequency(&mut self, channel: usize, freq: f32) {
        if channel < 16 {
            self.phase_incs[channel] = (freq * 2048.0) / self.sample_rate;
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        let old_rate = self.sample_rate;
        self.sample_rate = sample_rate;
        // Adjust phase increments for new sample rate
        for i in 0..16 {
            self.phase_incs[i] = (self.phase_incs[i] * old_rate) / sample_rate;
        }
    }

    pub fn process_scalar(&mut self, channel: usize, fm: &[f32], pm: &[f32], output: &mut [f32]) {
        let mut phase = self.phases[channel];
        let base_inc = self.phase_incs[channel];

        for i in 0..output.len() {
            let modulated_inc = base_inc * (1.0 + fm[i]);

            let modulated_phase = phase + pm[i] * 2048.0;
            let idx_f = modulated_phase.floor();
            let idx = (idx_f as i32 & 2047) as usize;
            let next_idx = (idx + 1) & 2047;
            let frac = modulated_phase - idx_f;

            output[i] = self.table[idx] * (1.0 - frac) + self.table[next_idx] * frac;

            phase += modulated_inc;
            if phase >= 2048.0 {
                phase -= 2048.0;
                if phase >= 2048.0 { phase %= 2048.0; }
            } else if phase < 0.0 {
                phase += 2048.0;
                if phase < 0.0 {
                    phase = phase.rem_euclid(2048.0);
                }
            }
        }
        self.phases[channel] = phase;
    }

    pub fn process_8_channels(&mut self, fm: [*const f32; 8], pm: [*const f32; 8], outputs: [*mut f32; 8], len: usize) {
        use wide::*;
        use crate::simd_vec::*;

        let mut b_phases = load_f32x8(&self.phases, 0);
        let b_base_incs = load_f32x8(&self.phase_incs, 0);
        let b_2048 = f32x8::from(2048.0);
        let b_1 = f32x8::from(1.0);

        for i in 0..len {
            let b_fm = unsafe {
                f32x8::new([
                    *fm[0].add(i), *fm[1].add(i), *fm[2].add(i), *fm[3].add(i),
                    *fm[4].add(i), *fm[5].add(i), *fm[6].add(i), *fm[7].add(i)
                ])
            };
            let b_pm = unsafe {
                f32x8::new([
                    *pm[0].add(i), *pm[1].add(i), *pm[2].add(i), *pm[3].add(i),
                    *pm[4].add(i), *pm[5].add(i), *pm[6].add(i), *pm[7].add(i)
                ])
            };

            let b_mod_inc = b_base_incs * (b_1 + b_fm);
            let b_mod_phase = b_phases + (b_pm * b_2048);

            let b_idx_f = b_mod_phase.floor();
            let b_frac = b_mod_phase - b_idx_f;

            let b_idx: [i32; 8] = b_idx_f.round_int().into();

            let mut out_v = [0.0f32; 8];
            for ch in 0..8 {
                let idx = (b_idx[ch] as i32 & 2047) as usize;
                let next_idx = (idx + 1) & 2047;
                let frac = b_frac.as_array_ref()[ch];
                out_v[ch] = self.table[idx] * (1.0 - frac) + self.table[next_idx] * frac;
            }

            for ch in 0..8 {
                unsafe { *outputs[ch].add(i) = out_v[ch] };
            }

            b_phases += b_mod_inc;

            // Robust wrap
            let wrap_pos_mask = b_phases.cmp_ge(b_2048);
            let wrap_neg_mask = b_phases.cmp_lt(f32x8::ZERO);

            b_phases -= wrap_pos_mask.blend(b_2048, f32x8::ZERO);
            b_phases += wrap_neg_mask.blend(b_2048, f32x8::ZERO);
        }

        store_f32x8(&mut self.phases, 0, b_phases);
    }
}

/// A high-performance sampler voice with Lagrange interpolation.
/// Shared ownership of the sample buffer is managed via Arc to prevent dangling pointers.
#[derive(Debug, Clone)]
pub struct SamplerVoice {
    pub buffer: Option<std::sync::Arc<Vec<f32>>>,
    pub play_head: f32,
    pub playback_rate: f32,
    pub is_active: bool,
    pub velocity: f32,
}

impl Default for SamplerVoice {
    fn default() -> Self {
        Self::new()
    }
}

impl SamplerVoice {
    pub fn new() -> Self {
        Self {
            buffer: None,
            play_head: 0.0,
            playback_rate: 1.0,
            is_active: false,
            velocity: 0.0,
        }
    }

    pub fn trigger(&mut self, buffer: std::sync::Arc<Vec<f32>>, playback_rate: f32, velocity: f32) {
        self.buffer = Some(buffer);
        self.play_head = 0.0;
        self.playback_rate = playback_rate;
        self.velocity = velocity;
        self.is_active = true;
    }

    pub fn process_scalar_frame(&mut self) -> f32 {
        if !self.is_active { return 0.0; }
        let Some(buffer) = &self.buffer else { return 0.0; };

        let idx = self.play_head.floor() as usize;
        if idx + 4 >= buffer.len() {
            self.is_active = false;
            return 0.0;
        }

        // 4-point Lagrange interpolation
        let x = self.play_head - idx as f32;
        let p0 = *buffer.get(idx.saturating_sub(1)).unwrap_or(&0.0);
        let p1 = buffer[idx];
        let p2 = buffer[idx + 1];
        let p3 = buffer[idx + 2];

        let c1 = p1;
        let c2 = -0.5 * p0 + 0.5 * p2;
        let c3 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
        let c4 = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;

        let sample = (((c4 * x + c3) * x + c2) * x + c1) * self.velocity;
        self.play_head += self.playback_rate;
        sample
    }
}
