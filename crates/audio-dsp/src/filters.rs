pub trait Filter {
    fn process_sample(&mut self, input: f32) -> f32;
    fn reset(&mut self) {}
}

/// A 4-pole Moog Ladder filter with non-linear feedback.
#[derive(Debug, Clone, Copy)]
pub struct MoogLadder {
    pub sample_rate: f32,
    pub cutoff: f32,
    pub resonance: f32,
    pub drive: f32,
    // State variables
    s: [f32; 4],
    // Coefficients
    g: f32,
    h: f32,
}

impl MoogLadder {
    pub fn new(sample_rate: f32) -> Self {
        let mut filter = Self {
            sample_rate,
            cutoff: 1000.0,
            resonance: 0.1,
            drive: 1.0,
            s: [0.0; 4],
            g: 0.0,
            h: 0.0,
        };
        filter.update_coeffs();
        filter
    }

    pub fn set_params(&mut self, cutoff: f32, resonance: f32, drive: f32) {
        self.cutoff = cutoff.clamp(20.0, 20000.0);
        self.resonance = resonance.clamp(0.0, 4.0);
        self.drive = drive.clamp(1.0, 10.0);
        self.update_coeffs();
    }

    fn update_coeffs(&mut self) {
        let wd = 2.0 * std::f32::consts::PI * self.cutoff;
        let t = 1.0 / self.sample_rate;
        let wa = (2.0 / t) * (wd * t / 2.0).tan();
        self.g = wa * t / 2.0;
        self.h = self.g / (1.0 + self.g);
    }
}

impl Filter for MoogLadder {
    fn process_sample(&mut self, mut input: f32) -> f32 {
        if !input.is_finite() {
            input = 0.0;
        }
        let g = self.g;
        let h = self.h;
        let res = self.resonance;
        let drive = self.drive;

        // STAGE 9: Newton-Raphson Iterative Solver for Moog Ladder
        // Resolves the implicit equation: y = tanh(drive * (input - res * y4))
        // we solve for the feedback term 'u' at the input of the first stage.
        let solver = crate::util::IterativeSolver::new(4, 1e-4);

        let s = self.s;
        let mut u = solver.solve(input,
            |x| {
                // Calculate y4 given input 'x'
                let mut v = x;
                for i in 0..4 {
                    v = s[i] + (v - s[i]) * h;
                }
                // f(x) = x - (input - res * tanh(v))
                x - ((input * drive).tanh() - (v * res).tanh())
            },
            |_x| {
                // Derivative approximation (simplified)
                1.0 + g.powi(4) * res
            }
        );

        if !u.is_finite() {
            u = 0.0;
            self.reset();
        }

        // Apply stages with resolved feedback
        let mut x = u;
        for i in 0..4 {
            let v = (x - self.s[i]) * h;
            let y = v + self.s[i];
            self.s[i] = y + v;
            x = y;
        }

        if !x.is_finite() {
            x = 0.0;
            self.reset();
        }

        x
    }

    fn reset(&mut self) {
        self.s.fill(0.0);
    }
}

/// A Zero-Delay Feedback (ZDF) State Variable Filter (SVF) using TPT.
#[derive(Debug, Clone, Copy)]
pub struct ZdfSvf {
    pub sample_rate: f32,
    pub cutoff: f32,
    pub resonance: f32,
    // State variables
    s1: f32,
    s2: f32,
    // Coefficients
    g: f32,
    k: f32,
}

impl ZdfSvf {
    pub fn new(sample_rate: f32) -> Self {
        let mut filter = Self {
            sample_rate,
            cutoff: 1000.0,
            resonance: 0.707,
            s1: 0.0,
            s2: 0.0,
            g: 0.0,
            k: 0.0,
        };
        filter.update_coeffs();
        filter
    }

    pub fn set_params(&mut self, cutoff: f32, resonance: f32) {
        self.cutoff = cutoff.clamp(20.0, 20000.0);
        self.resonance = resonance.clamp(0.01, 10.0);
        self.update_coeffs();
    }

    fn update_coeffs(&mut self) {
        let g = (std::f32::consts::PI * self.cutoff / self.sample_rate).tan();
        let k = 1.0 / self.resonance;
        self.g = g;
        self.k = k;
    }

    pub fn process_lp(&mut self, mut input: f32) -> f32 {
        if !input.is_finite() { input = 0.0; }
        let g = self.g;
        let k = self.k;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let v3 = input - self.s2;
        let v1 = a1 * self.s1 + a2 * v3;
        let v2 = self.s2 + a2 * self.s1 + a3 * v3;

        self.s1 = 2.0 * v1 - self.s1;
        self.s2 = 2.0 * v2 - self.s2;

        if !self.s1.is_finite() || !self.s2.is_finite() {
            self.reset();
            return 0.0;
        }

        v2 // Lowpass
    }

    pub fn process_hp(&mut self, mut input: f32) -> f32 {
        if !input.is_finite() { input = 0.0; }
        let g = self.g;
        let k = self.k;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let v3 = input - self.s2;
        let v1 = a1 * self.s1 + a2 * v3;
        let v2 = self.s2 + a2 * self.s1 + a3 * v3;

        self.s1 = 2.0 * v1 - self.s1;
        self.s2 = 2.0 * v2 - self.s2;

        if !self.s1.is_finite() || !self.s2.is_finite() {
            self.reset();
            return 0.0;
        }

        input - k * v1 - v2 // Highpass
    }

    pub fn process_bp(&mut self, mut input: f32) -> f32 {
        if !input.is_finite() { input = 0.0; }
        let g = self.g;
        let k = self.k;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let v3 = input - self.s2;
        let v1 = a1 * self.s1 + a2 * v3;
        let v2 = self.s2 + a2 * self.s1 + a3 * v3;

        self.s1 = 2.0 * v1 - self.s1;
        self.s2 = 2.0 * v2 - self.s2;

        if !self.s1.is_finite() || !self.s2.is_finite() {
            self.reset();
            return 0.0;
        }

        v1 // Bandpass
    }
}

impl Filter for ZdfSvf {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.process_lp(input)
    }

    fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BiquadCoefficients {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

/// A Biquad Filter using Direct Form II Transposed.
#[derive(Debug, Clone, Copy)]
#[repr(C, align(64))]
pub struct BiquadFilter {
    pub coeffs: BiquadCoefficients,
    pub target_coeffs: BiquadCoefficients,
    pub ramp_duration: u32,
    pub ramp_counter: u32,
    pub(crate) b0_step: f32,
    pub(crate) b1_step: f32,
    pub(crate) b2_step: f32,
    pub(crate) a1_step: f32,
    pub(crate) a2_step: f32,
    pub(crate) z1: f32,
    pub(crate) z2: f32,
}

impl Default for BiquadCoefficients {
    fn default() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 }
    }
}

impl BiquadCoefficients {
    pub fn linkwitz_riley_lp(freq: f32, sample_rate: f32) -> Self {
        let omega = std::f32::consts::PI * freq / sample_rate;
        let theta = omega.tan();
        let k = theta.powi(2);
        let delta = k + 2.0_f32.sqrt() * theta + 1.0;

        Self {
            b0: k / delta,
            b1: 2.0 * k / delta,
            b2: k / delta,
            a1: 2.0 * (k - 1.0) / delta,
            a2: (k - 2.0_f32.sqrt() * theta + 1.0) / delta,
        }
    }

    pub fn linkwitz_riley_hp(freq: f32, sample_rate: f32) -> Self {
        let omega = std::f32::consts::PI * freq / sample_rate;
        let theta = omega.tan();
        let k = theta.powi(2);
        let delta = k + 2.0_f32.sqrt() * theta + 1.0;

        Self {
            b0: 1.0 / delta,
            b1: -2.0 / delta,
            b2: 1.0 / delta,
            a1: 2.0 * (k - 1.0) / delta,
            a2: (k - 2.0_f32.sqrt() * theta + 1.0) / delta,
        }
    }

    /// RBJ low shelf (Audio EQ Cookbook, S = 1). `gain` is LINEAR (1.0 =
    /// flat). At exactly 1.0 the coefficients are the bit-exact identity, so
    /// an untouched shelf is a true passthrough, not a numeric near-identity.
    pub fn low_shelf(freq: f32, gain: f32, sample_rate: f32) -> Self {
        if gain == 1.0 { return Self::default(); }
        let a = gain.sqrt();
        let omega = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let (sn, cs) = omega.sin_cos();
        let alpha = sn / 2.0 * 2.0_f32.sqrt();
        let two_ra = 2.0 * a.sqrt() * alpha;
        let a0 = (a + 1.0) + (a - 1.0) * cs + two_ra;
        Self {
            b0: a * ((a + 1.0) - (a - 1.0) * cs + two_ra) / a0,
            b1: 2.0 * a * ((a - 1.0) - (a + 1.0) * cs) / a0,
            b2: a * ((a + 1.0) - (a - 1.0) * cs - two_ra) / a0,
            a1: -2.0 * ((a - 1.0) + (a + 1.0) * cs) / a0,
            a2: ((a + 1.0) + (a - 1.0) * cs - two_ra) / a0,
        }
    }

    /// RBJ high shelf (Audio EQ Cookbook, S = 1). `gain` is LINEAR; 1.0 is
    /// the bit-exact identity, as with `low_shelf`.
    pub fn high_shelf(freq: f32, gain: f32, sample_rate: f32) -> Self {
        if gain == 1.0 { return Self::default(); }
        let a = gain.sqrt();
        let omega = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let (sn, cs) = omega.sin_cos();
        let alpha = sn / 2.0 * 2.0_f32.sqrt();
        let two_ra = 2.0 * a.sqrt() * alpha;
        let a0 = (a + 1.0) - (a - 1.0) * cs + two_ra;
        Self {
            b0: a * ((a + 1.0) + (a - 1.0) * cs + two_ra) / a0,
            b1: -2.0 * a * ((a - 1.0) + (a + 1.0) * cs) / a0,
            b2: a * ((a + 1.0) + (a - 1.0) * cs - two_ra) / a0,
            a1: 2.0 * ((a - 1.0) - (a + 1.0) * cs) / a0,
            a2: ((a + 1.0) - (a - 1.0) * cs - two_ra) / a0,
        }
    }

    /// RBJ peaking EQ (Audio EQ Cookbook). `gain` is LINEAR; 1.0 is the
    /// bit-exact identity, as with the shelves.
    pub fn peaking(freq: f32, q: f32, gain: f32, sample_rate: f32) -> Self {
        if gain == 1.0 { return Self::default(); }
        let a = gain.sqrt();
        let omega = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let (sn, cs) = omega.sin_cos();
        let alpha = sn / (2.0 * q);
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: -2.0 * cs / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: -2.0 * cs / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }
}

impl BiquadFilter {
    pub fn new(coeffs: BiquadCoefficients) -> Self {
        Self {
            coeffs,
            target_coeffs: coeffs,
            ramp_duration: 0,
            ramp_counter: 0,
            b0_step: 0.0,
            b1_step: 0.0,
            b2_step: 0.0,
            a1_step: 0.0,
            a2_step: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn update_coeffs(&mut self, coeffs: BiquadCoefficients) {
        self.target_coeffs = coeffs;
        self.coeffs = coeffs;
        self.ramp_duration = 0;
        self.ramp_counter = 0;
        self.b0_step = 0.0;
        self.b1_step = 0.0;
        self.b2_step = 0.0;
        self.a1_step = 0.0;
        self.a2_step = 0.0;
    }

    pub fn set_coeffs_ramped(&mut self, coeffs: BiquadCoefficients, duration: u32) {
        if duration == 0 {
            self.update_coeffs(coeffs);
        } else {
            self.target_coeffs = coeffs;
            self.ramp_duration = duration;
            self.ramp_counter = duration;
            let inv_duration = 1.0 / duration as f32;
            self.b0_step = (coeffs.b0 - self.coeffs.b0) * inv_duration;
            self.b1_step = (coeffs.b1 - self.coeffs.b1) * inv_duration;
            self.b2_step = (coeffs.b2 - self.coeffs.b2) * inv_duration;
            self.a1_step = (coeffs.a1 - self.coeffs.a1) * inv_duration;
            self.a2_step = (coeffs.a2 - self.coeffs.a2) * inv_duration;
        }
    }

    /// Optimized block processing using scalar unrolling.
    pub fn process_block_unrolled(&mut self, input: &[f32], output: &mut [f32]) {
        let len = input.len();
        if len == 0 { return; }

        if self.ramp_duration > 0 {
            for i in 0..len {
                output[i] = self.process_sample(input[i]);
            }
            return;
        }

        let mut z1 = self.z1;
        let mut z2 = self.z2;
        let b0 = self.coeffs.b0;
        let b1 = self.coeffs.b1;
        let b2 = self.coeffs.b2;
        let a1 = self.coeffs.a1;
        let a2 = self.coeffs.a2;

        let mut i = 0;

        // SIMD unrolled path (4-lane)
        while i + 4 <= len {
            // Note: Since single-channel biquad has recursive dependencies (y depends on previous y),
            // a true vectorized path requires a specialized prefix-sum like approach.
            // For now, we utilize the SIMD lanes for independent filter stages or unroll scalar for throughput.
            unsafe {
                for _ in 0..4 {
                    let x = *input.get_unchecked(i);
                    let y = x * b0 + z1;
                    if !y.is_finite() {
                        z1 = 0.0; z2 = 0.0; *output.get_unchecked_mut(i) = 0.0;
                    } else {
                        z1 = x * b1 - y * a1 + z2;
                        z2 = x * b2 - y * a2;
                        *output.get_unchecked_mut(i) = y;
                    }
                    i += 1;
                }
            }
        }

        // Scalar fallback for small blocks
        while i < len {
            unsafe {
                let x = *input.get_unchecked(i);
                let mut y = x * b0 + z1;
                if !y.is_finite() {
                    y = 0.0;
                    z1 = 0.0;
                    z2 = 0.0;
                } else {
                    z1 = x * b1 - y * a1 + z2;
                    z2 = x * b2 - y * a2;
                }
                *output.get_unchecked_mut(i) = y;
            }
            i += 1;
        }

        self.z1 = z1;
        self.z2 = z2;
    }
}

impl Filter for BiquadFilter {
    fn process_sample(&mut self, input: f32) -> f32 {
        if self.ramp_counter > 0 {
            self.coeffs.b0 += self.b0_step;
            self.coeffs.b1 += self.b1_step;
            self.coeffs.b2 += self.b2_step;
            self.coeffs.a1 += self.a1_step;
            self.coeffs.a2 += self.a2_step;

            self.ramp_counter -= 1;
            if self.ramp_counter == 0 {
                self.coeffs = self.target_coeffs;
                self.ramp_duration = 0;
            }
        }

        let mut output = input * self.coeffs.b0 + self.z1;
        if !output.is_finite() {
            output = 0.0;
            self.z1 = 0.0;
            self.z2 = 0.0;
        } else {
            self.z1 = input * self.coeffs.b1 - output * self.coeffs.a1 + self.z2;
            self.z2 = input * self.coeffs.b2 - output * self.coeffs.a2;
        }
        output
    }
}

/// A Biquad Filter that processes 8 or 16 channels in parallel using AVX2/AVX-512.
#[derive(Clone)]
#[repr(C, align(64))]
pub struct SimdBiquad {
    pub coeffs: BiquadCoefficients,
    pub z1: [f32; 16],
    pub z2: [f32; 16],
}

impl crate::DspKernel for SimdBiquad {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        let num_ch = inputs.len().min(outputs.len());
        if num_ch >= 16 {
             let mut in_ptrs = [std::ptr::null(); 16];
             let mut out_ptrs = [std::ptr::null_mut(); 16];
             for i in 0..16 {
                 in_ptrs[i] = inputs[i].as_ptr();
                 out_ptrs[i] = outputs[i].as_mut_ptr();
             }
             self.process_16_channels(in_ptrs, out_ptrs, inputs[0].len());
        } else if num_ch >= 8 {
            let mut in_ptrs = [std::ptr::null(); 8];
            let mut out_ptrs = [std::ptr::null_mut(); 8];
            for i in 0..8 {
                in_ptrs[i] = inputs[i].as_ptr();
                out_ptrs[i] = outputs[i].as_mut_ptr();
            }
            self.process_8_channels(in_ptrs, out_ptrs, inputs[0].len());
        } else if num_ch >= 4 {
            let mut in_ptrs = [std::ptr::null(); 4];
            let mut out_ptrs = [std::ptr::null_mut(); 4];
            for i in 0..4 {
                in_ptrs[i] = inputs[i].as_ptr();
                out_ptrs[i] = outputs[i].as_mut_ptr();
            }
            self.process_4_channels(in_ptrs, out_ptrs, inputs[0].len());
        } else {
            for i in 0..num_ch {
                self.process_scalar(i, inputs[i], outputs[i]);
            }
        }
    }

    fn set_parameter(&mut self, id: u32, value: f32, _ramp_samples: u32) {
        // Simple implementation for now
        match id {
            0 => self.coeffs.b0 = value,
            1 => self.coeffs.b1 = value,
            2 => self.coeffs.b2 = value,
            3 => self.coeffs.a1 = value,
            4 => self.coeffs.a2 = value,
            _ => {}
        }
    }
}

impl SimdBiquad {
    pub fn new(coeffs: BiquadCoefficients) -> Self {
        Self {
            coeffs,
            z1: [0.0; 16],
            z2: [0.0; 16],
        }
    }

    pub fn process_scalar(&mut self, channel: usize, input: &[f32], output: &mut [f32]) {
        let mut z1 = self.z1[channel];
        let mut z2 = self.z2[channel];
        for i in 0..input.len() {
            let mut out = input[i] * self.coeffs.b0 + z1;
            if !out.is_finite() {
                out = 0.0;
                z1 = 0.0;
                z2 = 0.0;
            } else {
                z1 = input[i] * self.coeffs.b1 - out * self.coeffs.a1 + z2;
                z2 = input[i] * self.coeffs.b2 - out * self.coeffs.a2;
            }
            output[i] = out;
        }
        self.z1[channel] = z1;
        self.z2[channel] = z2;
    }

    pub fn process_16_channels(&mut self, inputs: [*const f32; 16], outputs: [*mut f32; 16], len: usize) {
        use crate::simd_vec::*;

        let b0 = FloatX16::from(self.coeffs.b0);
        let b1 = FloatX16::from(self.coeffs.b1);
        let b2 = FloatX16::from(self.coeffs.b2);
        let a1 = FloatX16::from(self.coeffs.a1);
        let a2 = FloatX16::from(self.coeffs.a2);

        let mut z1 = unsafe { load_f32x16_ptr(self.z1.as_ptr()) };
        let mut z2 = unsafe { load_f32x16_ptr(self.z2.as_ptr()) };

        for i in 0..len {
            let mut in_arr = [0.0f32; 16];
            for ch in 0..16 {
                unsafe { in_arr[ch] = *inputs[ch].add(i) };
            }
            let x = FloatX16::new(in_arr);

            let y = (x * b0) + z1;

            // NaN Hardening for SIMD path
            let finite_mask = y.is_finite_mask();
            let y = finite_mask.blend(y, FloatX16::splat(0.0));
            z1 = finite_mask.blend(((x * b1) - (y * a1)) + z2, FloatX16::splat(0.0));
            z2 = finite_mask.blend((x * b2) - (y * a2), FloatX16::splat(0.0));

            let out_arr: [f32; 16] = y.into();
            for ch in 0..16 { unsafe { *outputs[ch].add(i) = out_arr[ch] }; }
        }

        unsafe { store_f32x16_ptr(self.z1.as_mut_ptr(), z1) };
        unsafe { store_f32x16_ptr(self.z2.as_mut_ptr(), z2) };
    }

    pub fn process_4_channels(&mut self, inputs: [*const f32; 4], outputs: [*mut f32; 4], len: usize) {
        use wide::*;

        let b0 = f32x4::from(self.coeffs.b0);
        let b1 = f32x4::from(self.coeffs.b1);
        let b2 = f32x4::from(self.coeffs.b2);
        let a1 = f32x4::from(self.coeffs.a1);
        let a2 = f32x4::from(self.coeffs.a2);

        let mut z1 = f32x4::new([self.z1[0], self.z1[1], self.z1[2], self.z1[3]]);
        let mut z2 = f32x4::new([self.z2[0], self.z2[1], self.z2[2], self.z2[3]]);

        for i in 0..len {
            let x = unsafe {
                f32x4::new([
                    *inputs[0].add(i), *inputs[1].add(i), *inputs[2].add(i), *inputs[3].add(i)
                ])
            };

            let mut y = (x * b0) + z1;

            let y_arr: [f32; 4] = y.into();
            let mut finite = true;
            for val in &y_arr { if !val.is_finite() { finite = false; break; } }

            if !finite {
                y = f32x4::ZERO;
                z1 = f32x4::ZERO;
                z2 = f32x4::ZERO;
            } else {
                z1 = ((x * b1) - (y * a1)) + z2;
                z2 = (x * b2) - (y * a2);
            }

            let out_v: [f32; 4] = y.into();
            unsafe {
                *outputs[0].add(i) = out_v[0];
                *outputs[1].add(i) = out_v[1];
                *outputs[2].add(i) = out_v[2];
                *outputs[3].add(i) = out_v[3];
            }
        }

        let z1_arr: [f32; 4] = z1.into();
        self.z1[0..4].copy_from_slice(&z1_arr);
        let z2_arr: [f32; 4] = z2.into();
        self.z2[0..4].copy_from_slice(&z2_arr);
    }

    pub fn process_8_channels(&mut self, inputs: [*const f32; 8], outputs: [*mut f32; 8], len: usize) {
        use wide::*;

        let b0 = f32x8::from(self.coeffs.b0);
        let b1 = f32x8::from(self.coeffs.b1);
        let b2 = f32x8::from(self.coeffs.b2);
        let a1 = f32x8::from(self.coeffs.a1);
        let a2 = f32x8::from(self.coeffs.a2);

        let mut z1 = f32x8::new([
            self.z1[0], self.z1[1], self.z1[2], self.z1[3],
            self.z1[4], self.z1[5], self.z1[6], self.z1[7],
        ]);
        let mut z2 = f32x8::new([
            self.z2[0], self.z2[1], self.z2[2], self.z2[3],
            self.z2[4], self.z2[5], self.z2[6], self.z2[7],
        ]);

        for i in 0..len {
            // SAFETY: Caller ensures input pointers are valid for 'len' elements.
            let x = unsafe {
                f32x8::new([
                    *inputs[0].add(i), *inputs[1].add(i), *inputs[2].add(i), *inputs[3].add(i),
                    *inputs[4].add(i), *inputs[5].add(i), *inputs[6].add(i), *inputs[7].add(i)
                ])
            };

            let mut y = (x * b0) + z1;

            // NaN Hardening for 8-channel path
            // We use a manual check because f32x8 finite check is not built-in to 'wide' for all targets
            let y_arr: [f32; 8] = y.into();
            let mut finite = true;
            for val in &y_arr { if !val.is_finite() { finite = false; break; } }

            if !finite {
                y = f32x8::ZERO;
                z1 = f32x8::ZERO;
                z2 = f32x8::ZERO;
            } else {
                z1 = ((x * b1) - (y * a1)) + z2;
                z2 = (x * b2) - (y * a2);
            }

            let out_v: [f32; 8] = y.into();
            for (ch, &val) in out_v.iter().enumerate() {
                // SAFETY: Caller ensures output pointers are valid for 'len' elements.
                unsafe { *outputs.get_unchecked(ch).add(i) = val };
            }
        }

        let z1_arr: [f32; 8] = z1.into();
        self.z1[0..8].copy_from_slice(&z1_arr);
        let z2_arr: [f32; 8] = z2.into();
        self.z2[0..8].copy_from_slice(&z2_arr);
    }
}

impl crate::DspKernel for DjIsolator {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        self.process_block(inputs[0], outputs[0]);
    }

    fn reset(&mut self) {
        self.reset();
    }

    fn set_parameter(&mut self, id: u32, value: f32, _ramp_samples: u32) {
        if !value.is_finite() { return; }
        if id < 3 {
            self.gains[id as usize] = value.clamp(0.0, 10.0);
        }
    }
}

/// A 3-band DJ Isolator (Kill EQ) using 4th-order Linkwitz-Riley crossovers.
/// Each crossover consists of two cascaded 2nd-order biquads for a 24dB/octave slope.
#[derive(Clone)]
pub struct DjIsolator {
    pub low_pass_1: BiquadFilter,
    pub low_pass_2: BiquadFilter,
    pub high_pass_1: BiquadFilter,
    pub high_pass_2: BiquadFilter,
    pub mid_low_hp_1: BiquadFilter,
    pub mid_low_hp_2: BiquadFilter,
    pub mid_high_lp_1: BiquadFilter,
    pub mid_high_lp_2: BiquadFilter,
    pub gains: [f32; 3], // Low, Mid, High gains (0.0 to 1.0+)
}

impl Default for DjIsolator {
    fn default() -> Self {
        Self::new()
    }
}

impl DjIsolator {
    pub fn new() -> Self {
        Self::with_sample_rate(44100.0)
    }

    pub fn with_sample_rate(sample_rate: f32) -> Self {
        let lp_coeffs = BiquadCoefficients::linkwitz_riley_lp(300.0, sample_rate);
        let hp_coeffs = BiquadCoefficients::linkwitz_riley_hp(3000.0, sample_rate);

        let mid_hp = BiquadCoefficients::linkwitz_riley_hp(300.0, sample_rate);
        let mid_lp = BiquadCoefficients::linkwitz_riley_lp(3000.0, sample_rate);

        Self {
            low_pass_1: BiquadFilter::new(lp_coeffs),
            low_pass_2: BiquadFilter::new(lp_coeffs),
            high_pass_1: BiquadFilter::new(hp_coeffs),
            high_pass_2: BiquadFilter::new(hp_coeffs),
            mid_low_hp_1: BiquadFilter::new(mid_hp),
            mid_low_hp_2: BiquadFilter::new(mid_hp),
            mid_high_lp_1: BiquadFilter::new(mid_lp),
            mid_high_lp_2: BiquadFilter::new(mid_lp),
            gains: [1.0, 1.0, 1.0],
        }
    }

    pub fn process_block(&mut self, input: &[f32], output: &mut [f32]) {
        let g_l = self.gains[0];
        let g_m = self.gains[1];
        let g_h = self.gains[2];

        for i in 0..input.len() {
            let mut s = input[i];
            if !s.is_finite() { s = 0.0; }

            // 4th Order Low (2 cascaded LP)
            let l = self.low_pass_2.process_sample(self.low_pass_1.process_sample(s));

            // 4th Order High (2 cascaded HP)
            let h = self.high_pass_2.process_sample(self.high_pass_1.process_sample(s));

            // 4th Order Mid (HP 300 -> LP 3000)
            let m_low = self.mid_low_hp_2.process_sample(self.mid_low_hp_1.process_sample(s));
            let m = self.mid_high_lp_2.process_sample(self.mid_high_lp_1.process_sample(m_low));

            let out_sample = l * g_l + m * g_m + h * g_h;
            if out_sample.is_finite() {
                output[i] = out_sample;
            } else {
                output[i] = 0.0;
                self.reset();
            }
        }
    }

    pub fn reset(&mut self) {
        self.low_pass_1.reset();
        self.low_pass_2.reset();
        self.high_pass_1.reset();
        self.high_pass_2.reset();
        self.mid_low_hp_1.reset();
        self.mid_low_hp_2.reset();
        self.mid_high_lp_1.reset();
        self.mid_high_lp_2.reset();
    }
}

impl crate::DspKernel for BiquadFilter {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        self.process_block_unrolled(inputs[0], outputs[0]);
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

impl BiquadFilter {
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// A biquad that runs TWO channels (stereo L/R) at once in SIMD lanes 0 and 1.
/// Coefficients are shared; per-channel z-state lives in the lanes of z1/z2.
/// Each lane is arithmetically identical to a scalar `BiquadFilter` (Direct
/// Form II Transposed) — the point is to do both channels per instruction.
/// Lanes 2/3 are unused (carry zero). No coefficient ramp path: the isolator
/// sets crossover coefficients once at construction.
#[derive(Clone, Copy)]
pub struct StereoBiquad {
    pub coeffs: BiquadCoefficients,
    z1: wide::f32x4,
    z2: wide::f32x4,
}

impl StereoBiquad {
    pub fn new(coeffs: BiquadCoefficients) -> Self {
        Self { coeffs, z1: wide::f32x4::ZERO, z2: wide::f32x4::ZERO }
    }

    /// One stereo sample. Per lane: `y = x*b0 + z1; z1 = x*b1 - y*a1 + z2;
    /// z2 = x*b2 - y*a2`, with a branchless per-lane guard that zeroes a lane's
    /// output and state on a non-finite result — matching scalar
    /// `process_sample` lane-for-lane, so finite input is bit-identical.
    #[inline(always)]
    pub fn process(&mut self, x: wide::f32x4) -> wide::f32x4 {
        use wide::*;
        let b0 = f32x4::from(self.coeffs.b0);
        let b1 = f32x4::from(self.coeffs.b1);
        let b2 = f32x4::from(self.coeffs.b2);
        let a1 = f32x4::from(self.coeffs.a1);
        let a2 = f32x4::from(self.coeffs.a2);

        let y = (x * b0) + self.z1;
        // finite lanes: (y - y) == 0 (Inf/NaN both fail); == f32::is_finite.
        let finite = (y - y).cmp_eq(f32x4::ZERO);
        let z1n = ((x * b1) - (y * a1)) + self.z2;
        let z2n = (x * b2) - (y * a2);
        self.z1 = finite.blend(z1n, f32x4::ZERO);
        self.z2 = finite.blend(z2n, f32x4::ZERO);
        finite.blend(y, f32x4::ZERO)
    }

    pub fn reset(&mut self) {
        self.z1 = wide::f32x4::ZERO;
        self.z2 = wide::f32x4::ZERO;
    }
}

/// Stereo (2-channel SIMD) `DjIsolator`: the identical 3-band LR crossover, but
/// L and R flow together through 8 `StereoBiquad`s in ONE register-resident
/// pass — bit-identical to two independent scalar `DjIsolator`s on finite
/// input, at roughly half the per-sample arithmetic.
#[derive(Clone)]
pub struct DjIsolatorStereo {
    low_pass_1: StereoBiquad,
    low_pass_2: StereoBiquad,
    high_pass_1: StereoBiquad,
    high_pass_2: StereoBiquad,
    mid_low_hp_1: StereoBiquad,
    mid_low_hp_2: StereoBiquad,
    mid_high_lp_1: StereoBiquad,
    mid_high_lp_2: StereoBiquad,
    pub gains: [f32; 3],
}

impl Default for DjIsolatorStereo {
    fn default() -> Self { Self::new() }
}

impl DjIsolatorStereo {
    pub fn new() -> Self { Self::with_sample_rate(44100.0) }

    pub fn with_sample_rate(sample_rate: f32) -> Self {
        let lp = BiquadCoefficients::linkwitz_riley_lp(300.0, sample_rate);
        let hp = BiquadCoefficients::linkwitz_riley_hp(3000.0, sample_rate);
        let mid_hp = BiquadCoefficients::linkwitz_riley_hp(300.0, sample_rate);
        let mid_lp = BiquadCoefficients::linkwitz_riley_lp(3000.0, sample_rate);
        Self {
            low_pass_1: StereoBiquad::new(lp),
            low_pass_2: StereoBiquad::new(lp),
            high_pass_1: StereoBiquad::new(hp),
            high_pass_2: StereoBiquad::new(hp),
            mid_low_hp_1: StereoBiquad::new(mid_hp),
            mid_low_hp_2: StereoBiquad::new(mid_hp),
            mid_high_lp_1: StereoBiquad::new(mid_lp),
            mid_high_lp_2: StereoBiquad::new(mid_lp),
            gains: [1.0, 1.0, 1.0],
        }
    }

    pub fn set_gain(&mut self, band: usize, value: f32) {
        if band < 3 && value.is_finite() {
            self.gains[band] = value.clamp(0.0, 10.0);
        }
    }

    /// Run one packed sample through the crossover. Gains are pre-splatted by
    /// the caller (loop-invariant). Sanitizes the input and clamps a non-finite
    /// SUM to zero, exactly as the scalar `process_block` did per sample.
    #[inline(always)]
    fn run_sample(&mut self, raw: wide::f32x4, g_l: wide::f32x4, g_m: wide::f32x4, g_h: wide::f32x4) -> wide::f32x4 {
        use wide::*;
        let in_finite = (raw - raw).cmp_eq(f32x4::ZERO);
        let x = in_finite.blend(raw, f32x4::ZERO);

        let l = self.low_pass_2.process(self.low_pass_1.process(x));
        let h = self.high_pass_2.process(self.high_pass_1.process(x));
        let m_low = self.mid_low_hp_2.process(self.mid_low_hp_1.process(x));
        let m = self.mid_high_lp_2.process(self.mid_high_lp_1.process(m_low));

        let out = (l * g_l) + (m * g_m) + (h * g_h);
        let out_finite = (out - out).cmp_eq(f32x4::ZERO);
        out_finite.blend(out, f32x4::ZERO)
    }

    pub fn process_stereo(&mut self, in_l: &[f32], in_r: &[f32], out_l: &mut [f32], out_r: &mut [f32]) {
        use wide::*;
        let g_l = f32x4::from(self.gains[0]);
        let g_m = f32x4::from(self.gains[1]);
        let g_h = f32x4::from(self.gains[2]);
        let n = in_l.len().min(in_r.len()).min(out_l.len()).min(out_r.len());
        for i in 0..n {
            let out = self.run_sample(f32x4::new([in_l[i], in_r[i], 0.0, 0.0]), g_l, g_m, g_h);
            let arr: [f32; 4] = out.into();
            out_l[i] = arr[0];
            out_r[i] = arr[1];
        }
    }

    /// Single-channel path (lane 0 only) for mono wiring / conformance.
    pub fn process_mono(&mut self, input: &[f32], output: &mut [f32]) {
        use wide::*;
        let g_l = f32x4::from(self.gains[0]);
        let g_m = f32x4::from(self.gains[1]);
        let g_h = f32x4::from(self.gains[2]);
        let n = input.len().min(output.len());
        for i in 0..n {
            let out = self.run_sample(f32x4::new([input[i], 0.0, 0.0, 0.0]), g_l, g_m, g_h);
            let arr: [f32; 4] = out.into();
            output[i] = arr[0];
        }
    }

    pub fn reset(&mut self) {
        self.low_pass_1.reset();
        self.low_pass_2.reset();
        self.high_pass_1.reset();
        self.high_pass_2.reset();
        self.mid_low_hp_1.reset();
        self.mid_low_hp_2.reset();
        self.mid_high_lp_1.reset();
        self.mid_high_lp_2.reset();
    }
}

/// A simple Envelope Follower using rectification and a one-pole low-pass filter.
#[derive(Debug, Clone, Copy)]
pub struct EnvelopeFollower {
    pub sample_rate: f32,
    pub attack_coeff: f32,
    pub release_coeff: f32,
    pub envelope: f32,
}

impl EnvelopeFollower {
    pub fn new(sample_rate: f32, attack_ms: f32, release_ms: f32) -> Self {
        Self {
            sample_rate,
            attack_coeff: (-1.0 / (sample_rate * attack_ms * 0.001)).exp(),
            release_coeff: (-1.0 / (sample_rate * release_ms * 0.001)).exp(),
            envelope: 0.0,
        }
    }

    pub fn set_times(&mut self, attack_ms: f32, release_ms: f32) {
        self.attack_coeff = (-1.0 / (self.sample_rate * attack_ms * 0.001)).exp();
        self.release_coeff = (-1.0 / (self.sample_rate * release_ms * 0.001)).exp();
    }

    pub fn process_block(&mut self, input: &[f32], output: &mut [f32]) {
        self.process_with_sidechain(input, input, output);
    }

    pub fn process_with_sidechain(&mut self, input: &[f32], sidechain: &[f32], output: &mut [f32]) {
        let mut env = self.envelope;
        let attack = self.attack_coeff;
        let release = self.release_coeff;

        for i in 0..input.len().min(sidechain.len()) {
            let mut rect = sidechain[i].abs();
            if !rect.is_finite() { rect = 0.0; }
            if rect > env {
                env = rect + attack * (env - rect);
            } else {
                env = rect + release * (env - rect);
            }
            if !env.is_finite() { env = 0.0; }
            output[i] = env;
        }
        self.envelope = env;
    }
}

impl crate::DspKernel for EnvelopeFollower {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        self.process_block(inputs[0], outputs[0]);
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }

    fn set_parameter(&mut self, id: u32, value: f32, _ramp_samples: u32) {
        if id == 0 {
            self.attack_coeff = (-1.0 / (self.sample_rate * value.max(0.1) * 0.001)).exp();
        } else if id == 1 {
            self.release_coeff = (-1.0 / (self.sample_rate * value.max(0.1) * 0.001)).exp();
        }
    }
}

/// A 3-band mastering tone stage: RBJ low shelf, mid peak and high shelf in
/// SERIES. Unlike the DjIsolator (crossover split + re-sum, allpass phase
/// rotation even at unity), every band at gain 1.0 is the bit-exact identity
/// biquad — an untouched MasteringEq passes audio through unchanged, which is
/// what lets it sit in the master chain without shifting the golden render.
///
/// Params (linear gain, 1.0 = flat): 0 = LOW shelf, 1 = MID peak, 2 = HIGH
/// shelf. Coefficient changes are RAMPED (caller-provided duration) so knob
/// moves cannot click on the master bus.
#[derive(Clone)]
pub struct MasteringEq {
    pub low: BiquadFilter,
    pub mid: BiquadFilter,
    pub high: BiquadFilter,
    pub gains: [f32; 3],
    sample_rate: f32,
}

/// Corner frequencies chosen for master-bus tone shaping (broad strokes),
/// not surgical EQ: shelves at the spectrum edges, gentle mid bell.
const MASTERING_EQ_LOW_HZ: f32 = 120.0;
const MASTERING_EQ_MID_HZ: f32 = 1_000.0;
const MASTERING_EQ_MID_Q: f32 = 0.707;
const MASTERING_EQ_HIGH_HZ: f32 = 8_000.0;

/// Gain floor for coefficient math: the RBJ formulas divide by the gain, so
/// a full kill (0.0) is clamped to -30 dB — a master tone control is not a
/// kill switch, and -30 dB reads as "off" on program material.
const MASTERING_EQ_MIN_GAIN: f32 = 0.0316;
const MASTERING_EQ_MAX_GAIN: f32 = 4.0;

impl Default for MasteringEq {
    fn default() -> Self {
        Self::with_sample_rate(44_100.0)
    }
}

impl MasteringEq {
    pub fn with_sample_rate(sample_rate: f32) -> Self {
        let identity = BiquadCoefficients::default();
        Self {
            low: BiquadFilter::new(identity),
            mid: BiquadFilter::new(identity),
            high: BiquadFilter::new(identity),
            gains: [1.0, 1.0, 1.0],
            sample_rate,
        }
    }

    fn band_coeffs(&self, band: usize) -> BiquadCoefficients {
        let g = if self.gains[band] == 1.0 {
            1.0
        } else {
            self.gains[band].clamp(MASTERING_EQ_MIN_GAIN, MASTERING_EQ_MAX_GAIN)
        };
        match band {
            0 => BiquadCoefficients::low_shelf(MASTERING_EQ_LOW_HZ, g, self.sample_rate),
            1 => BiquadCoefficients::peaking(MASTERING_EQ_MID_HZ, MASTERING_EQ_MID_Q, g, self.sample_rate),
            _ => BiquadCoefficients::high_shelf(MASTERING_EQ_HIGH_HZ, g, self.sample_rate),
        }
    }
}

impl crate::DspKernel for MasteringEq {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]) {
        if inputs.is_empty() || outputs.is_empty() { return; }
        let input = inputs[0];
        let output = &mut outputs[0];
        let len = input.len().min(output.len());
        for i in 0..len {
            let mut s = input[i];
            if !s.is_finite() { s = 0.0; }
            s = self.low.process_sample(s);
            s = self.mid.process_sample(s);
            s = self.high.process_sample(s);
            if s.is_finite() {
                output[i] = s;
            } else {
                output[i] = 0.0;
                self.reset();
            }
        }
    }

    fn reset(&mut self) {
        self.low.reset();
        self.mid.reset();
        self.high.reset();
    }

    fn set_parameter(&mut self, id: u32, value: f32, ramp_samples: u32) {
        if !value.is_finite() || id >= 3 { return; }
        self.gains[id as usize] = value.clamp(0.0, MASTERING_EQ_MAX_GAIN);
        let coeffs = self.band_coeffs(id as usize);
        let filter = match id {
            0 => &mut self.low,
            1 => &mut self.mid,
            _ => &mut self.high,
        };
        filter.set_coeffs_ramped(coeffs, ramp_samples);
    }

    fn get_parameter(&self, id: u32) -> f32 {
        if id < 3 { self.gains[id as usize] } else { 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// An untouched MasteringEq must be a BIT-EXACT passthrough — that is the
    /// property that lets it live in the master chain without shifting the
    /// golden render.
    #[test]
    fn test_mastering_eq_unity_is_bit_exact_identity() {
        let mut eq = MasteringEq::with_sample_rate(44_100.0);
        let input: Vec<f32> = (0..4096)
            .map(|i| ((i as f32 * 0.01).sin() * 0.9 + (i as f32 * 0.37).sin() * 0.05) as f32)
            .collect();
        let mut output = vec![0.0f32; input.len()];
        crate::DspKernel::process(&mut eq, &[&input], &mut [&mut output]);
        for (i, (&a, &b)) in input.iter().zip(&output).enumerate() {
            assert_eq!(a.to_bits(), b.to_bits(), "sample {} not bit-identical at unity", i);
        }
    }

    /// A low-shelf cut must attenuate lows and leave highs alone — and vice
    /// versa nothing should explode at the clamp extremes.
    #[test]
    fn test_mastering_eq_low_cut_is_band_selective() {
        let sr = 44_100.0;
        let rms_after = |eq: &mut MasteringEq, hz: f32| -> f32 {
            let n = 44_100;
            let input: Vec<f32> = (0..n)
                .map(|i| (i as f32 * hz * 2.0 * std::f32::consts::PI / sr).sin() * 0.5)
                .collect();
            let mut output = vec![0.0f32; n];
            crate::DspKernel::process(eq, &[&input], &mut [&mut output]);
            // Skip the first half: filter settle + coefficient ramp.
            let tail = &output[n / 2..];
            (tail.iter().map(|&v| (v as f64) * (v as f64)).sum::<f64>() / tail.len() as f64).sqrt() as f32
        };
        let unity_rms = 0.5f32 / 2.0f32.sqrt();

        let mut eq = MasteringEq::with_sample_rate(sr);
        crate::DspKernel::set_parameter(&mut eq, 0, 0.25, 64); // low shelf -12 dB
        let low = rms_after(&mut eq, 60.0);
        assert!(
            low < unity_rms * 0.5,
            "60 Hz should be clearly attenuated by a -12 dB low shelf (rms {} vs unity {})",
            low, unity_rms
        );

        let mut eq = MasteringEq::with_sample_rate(sr);
        crate::DspKernel::set_parameter(&mut eq, 0, 0.25, 64);
        let high = rms_after(&mut eq, 10_000.0);
        assert!(
            (high - unity_rms).abs() < unity_rms * 0.1,
            "10 kHz should pass a low-shelf cut nearly untouched (rms {} vs unity {})",
            high, unity_rms
        );
    }

    /// Full kill and full boost requests stay finite: the coefficient math
    /// clamps to the documented gain floor instead of dividing by zero.
    #[test]
    fn test_mastering_eq_extremes_stay_finite() {
        let mut eq = MasteringEq::with_sample_rate(44_100.0);
        crate::DspKernel::set_parameter(&mut eq, 0, 0.0, 0);
        crate::DspKernel::set_parameter(&mut eq, 1, 0.0, 0);
        crate::DspKernel::set_parameter(&mut eq, 2, 4.0, 0);
        let input: Vec<f32> = (0..8192)
            .map(|i| (i as f32 * 0.05).sin() * 0.9)
            .collect();
        let mut output = vec![0.0f32; input.len()];
        crate::DspKernel::process(&mut eq, &[&input], &mut [&mut output]);
        assert!(output.iter().all(|v| v.is_finite()), "extreme settings produced non-finite output");
    }

    #[test]
    fn test_biquad_unrolled_vs_sample() {
        let coeffs = BiquadCoefficients {
            b0: 0.1, b1: 0.2, b2: 0.3, a1: -0.5, a2: 0.2
        };
        let mut filter_scalar = BiquadFilter::new(coeffs);
        let mut filter_unrolled = BiquadFilter::new(coeffs);

        let input = vec![0.5; 100];
        let mut out_scalar = vec![0.0; 100];
        let mut out_unrolled = vec![0.0; 100];

        for i in 0..100 {
            out_scalar[i] = filter_scalar.process_sample(input[i]);
        }
        filter_unrolled.process_block_unrolled(&input, &mut out_unrolled);

        for i in 0..100 {
            assert!((out_scalar[i] - out_unrolled[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn test_moog_ladder_stability() {
        let mut filter = MoogLadder::new(44100.0);
        filter.set_params(1000.0, 3.0, 2.0);
        let input = vec![1.0; 1000];
        let mut output = vec![0.0; 1000];
        for i in 0..1000 {
            output[i] = filter.process_sample(input[i]);
            assert!(output[i].is_finite());
        }
    }

    #[test]
    fn test_zdf_svf_bandpass_stability() {
        let mut filter = ZdfSvf::new(44100.0);
        filter.set_params(1500.0, 4.0);
        let input = vec![0.5; 1000];
        let mut output = vec![0.0; 1000];
        for i in 0..1000 {
            output[i] = filter.process_bp(input[i]);
            assert!(output[i].is_finite());
        }
    }

    #[test]
    fn test_zdf_svf_stability() {
        let mut filter = ZdfSvf::new(44100.0);
        filter.set_params(1000.0, 5.0);
        let input = vec![1.0; 1000];
        let mut output = vec![0.0; 1000];
        for i in 0..1000 {
            output[i] = filter.process_lp(input[i]);
            assert!(output[i].is_finite());
        }
    }

    #[test]
    fn test_simd_biquad_8_channels() {
        let coeffs = BiquadCoefficients {
            b0: 0.1, b1: 0.2, b2: 0.3, a1: -0.5, a2: 0.2
        };
        let mut simd_filter = SimdBiquad::new(coeffs);
        let mut scalar_filters: Vec<BiquadFilter> = (0..8).map(|_| BiquadFilter::new(coeffs)).collect();

        let len = 64;
        let mut inputs = vec![vec![0.0f32; len]; 8];
        let mut outputs_simd = vec![vec![0.0f32; len]; 8];
        let mut outputs_scalar = vec![vec![0.0f32; len]; 8];

        for (ch, input_ch) in inputs.iter_mut().enumerate().take(8) {
            for (i, val) in input_ch.iter_mut().enumerate().take(len) {
                *val = (ch + i) as f32 * 0.01;
            }
        }

        let in_ptrs: [*const f32; 8] = [
            inputs[0].as_ptr(), inputs[1].as_ptr(), inputs[2].as_ptr(), inputs[3].as_ptr(),
            inputs[4].as_ptr(), inputs[5].as_ptr(), inputs[6].as_ptr(), inputs[7].as_ptr(),
        ];
        let out_ptrs: [*mut f32; 8] = [
            outputs_simd[0].as_mut_ptr(), outputs_simd[1].as_mut_ptr(), outputs_simd[2].as_mut_ptr(), outputs_simd[3].as_mut_ptr(),
            outputs_simd[4].as_mut_ptr(), outputs_simd[5].as_mut_ptr(), outputs_simd[6].as_mut_ptr(), outputs_simd[7].as_mut_ptr(),
        ];

        simd_filter.process_8_channels(in_ptrs, out_ptrs, len);

        for ch in 0..8 {
            scalar_filters[ch].process_block_unrolled(&inputs[ch], &mut outputs_scalar[ch]);
            for i in 0..len {
                assert!((outputs_simd[ch][i] - outputs_scalar[ch][i]).abs() < 1e-6);
            }
        }
    }

    proptest! {
        #[test]
        fn test_biquad_stability(
            b0 in -1.0f32..1.0f32,
            b1 in -1.0f32..1.0f32,
            b2 in -1.0f32..1.0f32,
            a1 in -0.5f32..0.5f32,
            a2 in -0.2f32..0.2f32,
        ) {
            let coeffs = BiquadCoefficients { b0, b1, b2, a1, a2 };
            let mut filter = BiquadFilter::new(coeffs);
            let input = vec![1.0; 100];
            let mut output = vec![0.0; 100];
            filter.process_block_unrolled(&input, &mut output);
            for &sample in &output {
                prop_assert!(sample.is_finite());
            }
        }
    }

    /// The stereo-SIMD `DjIsolatorStereo` must produce BIT-identical output to
    /// TWO independent scalar `DjIsolator`s (one per channel) — the SIMD lanes
    /// run the same DFII-T recursion as the scalar path. Distinct L/R signals
    /// with non-unity band gains, in varying chunk sizes.
    #[test]
    fn test_dj_isolator_stereo_matches_two_scalar_bitexact() {
        let sr = 44_100.0;
        let gains = [0.8f32, 0.5f32, 1.2f32];

        let mut stereo = DjIsolatorStereo::with_sample_rate(sr);
        stereo.gains = gains;
        let mut scalar_l = DjIsolator::with_sample_rate(sr);
        scalar_l.gains = gains;
        let mut scalar_r = DjIsolator::with_sample_rate(sr);
        scalar_r.gains = gains;

        // Distinct deterministic L/R signals.
        let mut seed = 0x1357_9bdfu32;
        let mut rng = || {
            seed ^= seed << 13; seed ^= seed >> 17; seed ^= seed << 5;
            (seed as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        let total = 3000usize;
        let sig_l: Vec<f32> = (0..total).map(|_| rng()).collect();
        let sig_r: Vec<f32> = (0..total).map(|_| rng()).collect();

        let mut out_l = vec![0.0f32; total];
        let mut out_r = vec![0.0f32; total];
        let mut ref_l = vec![0.0f32; total];
        let mut ref_r = vec![0.0f32; total];

        let sizes = [64usize, 37, 200, 129, 256, 91];
        let (mut pos, mut bi) = (0usize, 0usize);
        while pos < total {
            let b = sizes[bi % sizes.len()].min(total - pos);
            bi += 1;
            stereo.process_stereo(&sig_l[pos..pos + b], &sig_r[pos..pos + b],
                                  &mut out_l[pos..pos + b], &mut out_r[pos..pos + b]);
            scalar_l.process_block(&sig_l[pos..pos + b], &mut ref_l[pos..pos + b]);
            scalar_r.process_block(&sig_r[pos..pos + b], &mut ref_r[pos..pos + b]);
            pos += b;
        }

        for i in 0..total {
            assert_eq!(out_l[i].to_bits(), ref_l[i].to_bits(), "L mismatch at {}", i);
            assert_eq!(out_r[i].to_bits(), ref_r[i].to_bits(), "R mismatch at {}", i);
        }
    }
}
