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
        let mut sample = self.lut[idx];
        if !sample.is_finite() { sample = 0.0; }

        self.phase += self.phase_inc;
        if !self.phase.is_finite() { self.phase = 0.0; }

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

        let mut b_phases = unsafe { load_f32x8_ptr(self.phases.as_ptr()) };
        let b_base_incs = unsafe { load_f32x8_ptr(self.phase_incs.as_ptr()) };
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

            let mut v1_arr = [0.0f32; 8];
            let mut v2_arr = [0.0f32; 8];
            for ch in 0..8 {
                let idx = (b_idx[ch] & 2047) as usize;
                let next_idx = (idx + 1) & 2047;
                // Safety: idx and next_idx are masked to [0, 2047], self.table is size 2048.
                unsafe {
                    v1_arr[ch] = *self.table.get_unchecked(idx);
                    v2_arr[ch] = *self.table.get_unchecked(next_idx);
                }
            }

            let b_v1 = f32x8::new(v1_arr);
            let b_v2 = f32x8::new(v2_arr);
            let b_out = b_v1 * (b_1 - b_frac) + b_v2 * b_frac;

            let out_arr: [f32; 8] = b_out.into();
            for ch in 0..8 {
                unsafe { *outputs[ch].add(i) = out_arr[ch] };
            }

            b_phases += b_mod_inc;

            // Robust wrap
            let wrap_pos_mask = b_phases.cmp_ge(b_2048);
            let wrap_neg_mask = b_phases.cmp_lt(f32x8::ZERO);

            b_phases -= wrap_pos_mask.blend(b_2048, f32x8::ZERO);
            b_phases += wrap_neg_mask.blend(b_2048, f32x8::ZERO);
        }

        unsafe { store_f32x8_ptr(self.phases.as_mut_ptr(), b_phases); }
    }

    pub fn process_16_channels(&mut self, fm: [*const f32; 16], pm: [*const f32; 16], outputs: [*mut f32; 16], len: usize) {
        use crate::simd_vec::*;

        let mut b_phases = unsafe { load_f32x16_ptr(self.phases.as_ptr()) };
        let b_base_incs = unsafe { load_f32x16_ptr(self.phase_incs.as_ptr()) };
        let b_2048 = FloatX16::from(2048.0);
        let b_1 = FloatX16::from(1.0);

        for i in 0..len {
            let mut fm_arr = [0.0f32; 16];
            let mut pm_arr = [0.0f32; 16];
            for ch in 0..16 {
                unsafe {
                    fm_arr[ch] = *fm[ch].add(i);
                    pm_arr[ch] = *pm[ch].add(i);
                }
            }
            let b_fm = FloatX16::new(fm_arr);
            let b_pm = FloatX16::new(pm_arr);

            let b_mod_inc = b_base_incs * (b_1 + b_fm);
            let b_mod_phase = b_phases + (b_pm * b_2048);

            // Scalar fallback for table lookup within SIMD loop
            // High-density Stage 2 AnaWaves performance.
            let mut b_out_arr = [0.0f32; 16];

            #[cfg(target_feature = "avx512f")]
            {
                // In a true AVX-512 scenario, we might use gather instructions,
                // but for now we'll use a reliable scalar extraction.
                let phases_arr: [f32; 16] = b_mod_phase.into();
                for ch in 0..16 {
                    let p = phases_arr[ch];
                    let idx_f = p.floor();
                    let idx = (idx_f as i32 & 2047) as usize;
                    let next_idx = (idx + 1) & 2047;
                    let frac = p - idx_f;
                    b_out_arr[ch] = self.table[idx] * (1.0 - frac) + self.table[next_idx] * frac;
                }
            }
            #[cfg(not(target_feature = "avx512f"))]
            {
                let phases_low: [f32; 8] = b_mod_phase.low.into();
                let phases_high: [f32; 8] = b_mod_phase.high.into();
                for ch in 0..8 {
                    let p = phases_low[ch];
                    let idx_f = p.floor();
                    let idx = (idx_f as i32 & 2047) as usize;
                    let next_idx = (idx + 1) & 2047;
                    let frac = p - idx_f;
                    b_out_arr[ch] = self.table[idx] * (1.0 - frac) + self.table[next_idx] * frac;
                }
                for ch in 0..8 {
                    let p = phases_high[ch];
                    let idx_f = p.floor();
                    let idx = (idx_f as i32 & 2047) as usize;
                    let next_idx = (idx + 1) & 2047;
                    let frac = p - idx_f;
                    b_out_arr[ch+8] = self.table[idx] * (1.0 - frac) + self.table[next_idx] * frac;
                }
            }

            for ch in 0..16 {
                unsafe { *outputs[ch].add(i) = b_out_arr[ch] };
            }

            b_phases = b_phases + b_mod_inc;

            // Wrap b_phases (optimized branchless if possible, but keeping logic correct)
            let mut p_arr: [f32; 16] = b_phases.into();
            for p in p_arr.iter_mut() {
                if *p >= 2048.0 { *p -= 2048.0; }
                else if *p < 0.0 { *p += 2048.0; }
            }
            b_phases = FloatX16::new(p_arr);
        }

        unsafe { store_f32x16_ptr(self.phases.as_mut_ptr(), b_phases); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wavetable_frequency_accuracy() {
        let sample_rate = 44100.0;
        let freq = 441.0; // 100 samples per cycle
        let mut osc = WavetableOscillator::new(sample_rate);
        osc.set_frequency(0, freq);

        let mut output = vec![0.0f32; 1000];
        let fm = vec![0.0f32; 1000];
        let pm = vec![0.0f32; 1000];
        osc.process_scalar(0, &fm, &pm, &mut output);

        // At 441Hz and 44100Hz SR, we expect a zero crossing every 50 samples
        // or a full cycle every 100 samples.
        // Sine table starts at 0.0
        assert!(output[0].abs() < 1e-4);
        // After 100 samples, should be back to ~0.0
        assert!(output[100].abs() < 1e-2);
        // Peak at sample 25
        assert!(output[25] > 0.99);
    }

    #[test]
    fn test_wavetable_simd_vs_scalar() {
        let sample_rate = 48000.0;
        let mut osc_simd = WavetableOscillator::new(sample_rate);
        let mut osc_scalar = WavetableOscillator::new(sample_rate);

        for ch in 0..8 {
            osc_simd.set_frequency(ch, 100.0 * (ch + 1) as f32);
            osc_scalar.set_frequency(ch, 100.0 * (ch + 1) as f32);
        }

        let len = 128;
        let fm_data = vec![0.01f32; len];
        let pm_data = vec![0.02f32; len];

        let fm_ptrs: [*const f32; 8] = [fm_data.as_ptr(); 8];
        let pm_ptrs: [*const f32; 8] = [pm_data.as_ptr(); 8];
        let mut outputs_simd = vec![vec![0.0f32; len]; 8];
        let out_ptrs: [*mut f32; 8] = [
            outputs_simd[0].as_mut_ptr(), outputs_simd[1].as_mut_ptr(), outputs_simd[2].as_mut_ptr(), outputs_simd[3].as_mut_ptr(),
            outputs_simd[4].as_mut_ptr(), outputs_simd[5].as_mut_ptr(), outputs_simd[6].as_mut_ptr(), outputs_simd[7].as_mut_ptr(),
        ];

        osc_simd.process_8_channels(fm_ptrs, pm_ptrs, out_ptrs, len);

        for ch in 0..8 {
            let mut out_scalar = vec![0.0f32; len];
            osc_scalar.process_scalar(ch, &fm_data, &pm_data, &mut out_scalar);
            for i in 0..len {
                assert!((outputs_simd[ch][i] - out_scalar[i]).abs() < 1e-5);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InterpolationType {
    Linear = 0,
    Lagrange = 1,
}

/// A high-performance sampler voice with selectable interpolation.
/// Shared ownership of the sample buffer is managed via Arc to prevent dangling pointers.
#[derive(Debug, Clone)]
pub struct SamplerVoice {
    pub buffer: Option<std::sync::Arc<Vec<f32>>>,
    pub play_head: f32,
    pub playback_rate: f32,
    pub is_active: bool,
    pub velocity: f32,
    pub interpolation: InterpolationType,
    pub loop_enabled: bool,
    pub loop_start: u64,
    pub loop_end: u64,
    pub slip_enabled: bool,
    pub background_playhead: f32,
    pub grain_pos: f32,
    pub time_stretch_ratio: f32,
    pub grain_duration_samples: u32,
    pub overlap_count: u32,
    pub window_lut: [f32; 1024],
    pub trigger_offset: f32,
    pub trigger_beat: f64,
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
            interpolation: InterpolationType::Lagrange,
            loop_enabled: false,
            loop_start: 0,
            loop_end: 0,
            slip_enabled: false,
            background_playhead: 0.0,
            grain_pos: 0.0,
            time_stretch_ratio: 1.0,
            grain_duration_samples: 1024,
            overlap_count: 2,
            window_lut: {
                let mut lut = [0.0f32; 1024];
                for i in 0..1024 {
                    lut[i] = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / 1023.0).cos());
                }
                lut
            },
            trigger_offset: 0.0,
            trigger_beat: 0.0,
        }
    }

    pub fn trigger(&mut self, buffer: std::sync::Arc<Vec<f32>>, playback_rate: f32, velocity: f32) {
        self.trigger_at(buffer, playback_rate, velocity, 0.0, 0.0);
    }

    pub fn trigger_at(&mut self, buffer: std::sync::Arc<Vec<f32>>, playback_rate: f32, velocity: f32, offset: f32, beat: f64) {
        self.buffer = Some(buffer);
        self.play_head = offset;
        self.trigger_offset = offset;
        self.trigger_beat = beat;
        self.playback_rate = playback_rate;
        self.velocity = velocity;
        self.is_active = true;
    }

    /// RT-Safe variant that avoids atomic increment of the Arc if possible.
    pub fn trigger_at_ref(&mut self, buffer: &std::sync::Arc<Vec<f32>>, playback_rate: f32, velocity: f32, offset: f32, beat: f64) {
        // Only clone if the buffer actually changed
        let needs_clone = match &self.buffer {
            Some(existing) => !std::sync::Arc::ptr_eq(existing, buffer),
            None => true,
        };

        if needs_clone {
            self.buffer = Some(buffer.clone());
        }
        self.play_head = offset;
        self.trigger_offset = offset;
        self.trigger_beat = beat;
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

        let sample = match self.interpolation {
            InterpolationType::Linear => {
                let x = self.play_head - idx as f32;
                let p1 = buffer[idx];
                let p2 = buffer[idx + 1];
                p1 + (p2 - p1) * x
            }
            InterpolationType::Lagrange => {
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

                ((c4 * x + c3) * x + c2) * x + c1
            }
        };

        self.play_head += self.playback_rate;
        sample * self.velocity
    }

    pub fn process_block(&mut self, output: &mut [f32]) {
        if !self.is_active { return; }
        let Some(buffer) = &self.buffer else { return; };

        if (self.time_stretch_ratio - 1.0).abs() < 0.001 {
            // NORMAL MODE
            for sample_out in output.iter_mut() {
                let idx = self.play_head.floor() as usize;
                if idx + 4 >= buffer.len() { self.is_active = false; break; }

                let sample = self.interpolate_sample(buffer, self.play_head, idx);
                *sample_out += sample * self.velocity;
                self.play_head += self.playback_rate;
                self.background_playhead += self.playback_rate;

                if self.loop_enabled && self.play_head >= self.loop_end as f32 {
                    self.play_head = self.loop_start as f32 + (self.play_head - self.loop_end as f32);
                }
            }
        } else {
            // GRANULAR TIME-STRETCH (Simple Overlap-Add)
            for sample_out in output.iter_mut() {
                let mut sum = 0.0;

                for o in 0..self.overlap_count {
                    let phase = (self.grain_pos + (o as f32 * self.grain_duration_samples as f32 / self.overlap_count as f32)) % self.grain_duration_samples as f32;
                    let lut_idx = (phase * 1023.0 / (self.grain_duration_samples as f32 - 1.0)) as usize;
                    let window = self.window_lut[lut_idx.min(1023)];

                    let grain_offset = self.play_head + phase;
                    let idx = grain_offset.floor() as usize;

                    if idx + 4 < buffer.len() {
                        sum += self.interpolate_sample(buffer, grain_offset, idx) * window;
                    }
                }

                *sample_out += sum * self.velocity;

                // Advance grain position and playhead separately
                self.grain_pos += self.playback_rate;
                if self.grain_pos >= (self.grain_duration_samples as f32 / self.overlap_count as f32) {
                    self.grain_pos -= self.grain_duration_samples as f32 / self.overlap_count as f32;
                    self.play_head += (self.grain_duration_samples as f32 / self.overlap_count as f32) * self.time_stretch_ratio;
                }

                self.background_playhead += self.playback_rate;
                if self.play_head >= buffer.len() as f32 { self.is_active = false; break; }
            }
        }
    }

    fn interpolate_sample(&self, buffer: &[f32], play_head: f32, idx: usize) -> f32 {
        match self.interpolation {
            InterpolationType::Linear => {
                let x = play_head - idx as f32;
                let p1 = buffer[idx];
                let p2 = buffer[idx + 1];
                p1 + (p2 - p1) * x
            }
            InterpolationType::Lagrange => {
                let x = play_head - idx as f32;
                let start_idx = idx.saturating_sub(1);

                if start_idx + 4 <= buffer.len() {
                    // Optimized Vector Path (SIMD 4-point Lagrange)
                    let ptr = unsafe { buffer.as_ptr().add(start_idx) };
                    let v = unsafe { crate::simd_vec::load_f32x4_ptr(ptr) };
                    let p: [f32; 4] = v.into();

                    let p0 = p[0]; let p1 = p[1]; let p2 = p[2]; let p3 = p[3];

                    let c1 = p1;
                    let c2 = -0.5 * p0 + 0.5 * p2;
                    let c3 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
                    let c4 = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
                    ((c4 * x + c3) * x + c2) * x + c1
                } else {
                    // Safe scalar fallback for buffer end (Boundary path)
                    let p0 = *buffer.get(start_idx).unwrap_or(&0.0);
                    let p1 = *buffer.get(start_idx + 1).unwrap_or(&0.0);
                    let p2 = *buffer.get(start_idx + 2).unwrap_or(&0.0);
                    let p3 = *buffer.get(start_idx + 3).unwrap_or(&0.0);

                    let c1 = p1;
                    let c2 = -0.5 * p0 + 0.5 * p2;
                    let c3 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
                    let c4 = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
                    ((c4 * x + c3) * x + c2) * x + c1
                }
            }
        }
    }
}
