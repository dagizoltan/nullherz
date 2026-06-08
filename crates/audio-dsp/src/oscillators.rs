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
        for i in 0..LUT_SIZE {
            lut[i] = ((i as f32 * 2.0 * std::f32::consts::PI) / LUT_SIZE as f32).sin();
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
        for i in 0..2048 {
            table[i] = ((i as f32 * 2.0 * std::f32::consts::PI) / 2048.0).sin();
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

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    pub unsafe fn process_8_channels_avx2(&mut self, fm: [*const f32; 8], pm: [*const f32; 8], outputs: [*mut f32; 8], len: usize) {
        // Validation of channel availability is performed by the caller (processor)
        unsafe {
        use std::arch::x86_64::*;
        let mut b_phases = _mm256_loadu_ps(self.phases.as_ptr());
        let b_base_incs = _mm256_loadu_ps(self.phase_incs.as_ptr());
        let b_2048 = _mm256_set1_ps(2048.0);
        let b_1 = _mm256_set1_ps(1.0);

        for i in 0..len {
            // Optimization: If buffers are aligned, we could use faster loads.
            // For now, we still need to collect from 8 separate pointers.
            let b_fm = _mm256_setr_ps(
                *fm.get_unchecked(0).add(i), *fm.get_unchecked(1).add(i), *fm.get_unchecked(2).add(i), *fm.get_unchecked(3).add(i),
                *fm.get_unchecked(4).add(i), *fm.get_unchecked(5).add(i), *fm.get_unchecked(6).add(i), *fm.get_unchecked(7).add(i)
            );
            let b_pm = _mm256_setr_ps(
                *pm.get_unchecked(0).add(i), *pm.get_unchecked(1).add(i), *pm.get_unchecked(2).add(i), *pm.get_unchecked(3).add(i),
                *pm.get_unchecked(4).add(i), *pm.get_unchecked(5).add(i), *pm.get_unchecked(6).add(i), *pm.get_unchecked(7).add(i)
            );

            let b_mod_inc = _mm256_mul_ps(b_base_incs, _mm256_add_ps(b_1, b_fm));
            let b_mod_phase = _mm256_add_ps(b_phases, _mm256_mul_ps(b_pm, b_2048));

            // Linear interpolation via gather
            // Use floor instead of truncate to handle negative phase correctly
            let b_idx_f = _mm256_floor_ps(b_mod_phase);
            let b_idx = _mm256_cvttps_epi32(b_idx_f);
            let b_frac = _mm256_sub_ps(b_mod_phase, b_idx_f);

            // Mask indices to table size (2048)
            let b_mask = _mm256_set1_epi32(2047);
            let b_idx0 = _mm256_and_si256(b_idx, b_mask);
            let b_idx1 = _mm256_and_si256(_mm256_add_epi32(b_idx0, _mm256_set1_epi32(1)), b_mask);

            let v0 = _mm256_i32gather_ps(self.table.as_ptr(), b_idx0, 4);
            let v1 = _mm256_i32gather_ps(self.table.as_ptr(), b_idx1, 4);

            // res = v0 + frac * (v1 - v0)
            let b_res = _mm256_add_ps(v0, _mm256_mul_ps(b_frac, _mm256_sub_ps(v1, v0)));

            // Use storeu to an array and then distribute
            let mut out_v = [0.0f32; 8];
            _mm256_storeu_ps(out_v.as_mut_ptr(), b_res);
            *outputs.get_unchecked(0).add(i) = out_v[0];
            *outputs.get_unchecked(1).add(i) = out_v[1];
            *outputs.get_unchecked(2).add(i) = out_v[2];
            *outputs.get_unchecked(3).add(i) = out_v[3];
            *outputs.get_unchecked(4).add(i) = out_v[4];
            *outputs.get_unchecked(5).add(i) = out_v[5];
            *outputs.get_unchecked(6).add(i) = out_v[6];
            *outputs.get_unchecked(7).add(i) = out_v[7];

            b_phases = _mm256_add_ps(b_phases, b_mod_inc);

            // Robust wrap: handle both positive and negative overflow
            let b_zero = _mm256_setzero_ps();
            let wrap_pos_mask = _mm256_cmp_ps(b_phases, b_2048, _CMP_GE_OQ);
            let wrap_neg_mask = _mm256_cmp_ps(b_phases, b_zero, _CMP_LT_OQ);

            b_phases = _mm256_sub_ps(b_phases, _mm256_and_ps(wrap_pos_mask, b_2048));
            b_phases = _mm256_add_ps(b_phases, _mm256_and_ps(wrap_neg_mask, b_2048));
        }
        _mm256_storeu_ps(self.phases.as_mut_ptr(), b_phases);

        // No scalar tail loop needed here as process_8_channels processes 8 full channels
        // for the entire block duration 'len'. The block splitting happens at the 'len' level.
        // However, if we were processing blocks of samples in SIMD, we'd need one.
        }
    }
}
