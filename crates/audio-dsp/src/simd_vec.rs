use wide::*;

pub type FloatX4 = f32x4;
pub type FloatX8 = f32x8;

#[inline(always)]
pub fn load_f32x8(data: &[f32], offset: usize) -> FloatX8 {
    let mut arr = [0.0f32; 8];
    arr.copy_from_slice(&data[offset..offset+8]);
    f32x8::new(arr)
}

#[inline(always)]
pub fn load_f32x4(data: &[f32], offset: usize) -> FloatX4 {
    let mut arr = [0.0f32; 4];
    arr.copy_from_slice(&data[offset..offset+4]);
    f32x4::new(arr)
}

#[inline(always)]
pub fn store_f32x8(data: &mut [f32], offset: usize, val: FloatX8) {
    let arr: [f32; 8] = val.into();
    data[offset..offset+8].copy_from_slice(&arr);
}

#[inline(always)]
pub fn store_f32x4(data: &mut [f32], offset: usize, val: FloatX4) {
    let arr: [f32; 4] = val.into();
    data[offset..offset+4].copy_from_slice(&arr);
}

/// # Safety
/// The caller must ensure that `ptr` is valid for at least 8 elements of type `f32`.
#[inline(always)]
pub unsafe fn load_f32x8_ptr(ptr: *const f32) -> FloatX8 {
    let mut arr = [0.0f32; 8];
    unsafe { std::ptr::copy_nonoverlapping(ptr, arr.as_mut_ptr(), 8); }
    f32x8::new(arr)
}

/// # Safety
/// The caller must ensure that `ptr` is valid for at least 8 elements of type `f32`.
#[inline(always)]
pub unsafe fn store_f32x8_ptr(ptr: *mut f32, val: FloatX8) {
    let arr: [f32; 8] = val.into();
    unsafe { std::ptr::copy_nonoverlapping(arr.as_ptr(), ptr, 8); }
}

/// # Safety
/// The caller must ensure that `ptr` is valid for at least 4 elements of type `f32`.
#[inline(always)]
pub unsafe fn load_f32x4_ptr(ptr: *const f32) -> FloatX4 {
    let mut arr = [0.0f32; 4];
    unsafe { std::ptr::copy_nonoverlapping(ptr, arr.as_mut_ptr(), 4); }
    f32x4::new(arr)
}

/// A 16-wide f32 SIMD type, supporting AVX-512 or WASM SIMD128 (via 4x f32x4) where available.
/// Future infrastructure for AnaWaves Layer 2 (Spectral Personality) high-density operations.
#[derive(Clone, Copy)]
pub struct FloatX16 {
    #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
    pub(crate) val: wide::f32x16,
    #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), target_arch = "wasm32", target_feature = "simd128"))]
    pub(crate) parts: [f32x4; 4],
    #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
    pub(crate) low: f32x8,
    #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
    pub(crate) high: f32x8,
}

impl FloatX16 {
    pub fn new(data: [f32; 16]) -> Self {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
        { Self { val: wide::f32x16::from(data) } }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), target_arch = "wasm32", target_feature = "simd128"))]
        {
            Self {
                parts: [
                    f32x4::new([data[0], data[1], data[2], data[3]]),
                    f32x4::new([data[4], data[5], data[6], data[7]]),
                    f32x4::new([data[8], data[9], data[10], data[11]]),
                    f32x4::new([data[12], data[13], data[14], data[15]]),
                ]
            }
        }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
        {
            Self {
                low: f32x8::new([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]),
                high: f32x8::new([data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]]),
            }
        }
    }

    pub fn splat(val: f32) -> Self {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
        { Self { val: wide::f32x16::from(val) } }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), target_arch = "wasm32", target_feature = "simd128"))]
        { Self { parts: [f32x4::from(val); 4] } }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
        { Self { low: f32x8::from(val), high: f32x8::from(val) } }
    }
}

impl From<f32> for FloatX16 {
    fn from(val: f32) -> Self {
        Self::splat(val)
    }
}

impl From<FloatX16> for [f32; 16] {
    fn from(val: FloatX16) -> Self {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
        { val.val.into() }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), target_arch = "wasm32", target_feature = "simd128"))]
        {
            let p0: [f32; 4] = val.parts[0].into();
            let p1: [f32; 4] = val.parts[1].into();
            let p2: [f32; 4] = val.parts[2].into();
            let p3: [f32; 4] = val.parts[3].into();
            [
                p0[0], p0[1], p0[2], p0[3], p1[0], p1[1], p1[2], p1[3],
                p2[0], p2[1], p2[2], p2[3], p3[0], p3[1], p3[2], p3[3]
            ]
        }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
        {
            let low: [f32; 8] = val.low.into();
            let high: [f32; 8] = val.high.into();
            [
                low[0], low[1], low[2], low[3], low[4], low[5], low[6], low[7],
                high[0], high[1], high[2], high[3], high[4], high[5], high[6], high[7]
            ]
        }
    }
}

impl std::ops::Add for FloatX16 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
        { Self { val: self.val + rhs.val } }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), target_arch = "wasm32", target_feature = "simd128"))]
        {
            use std::arch::wasm32::*;
            Self { parts: [
                f32x4_add(self.parts[0], rhs.parts[0]),
                f32x4_add(self.parts[1], rhs.parts[1]),
                f32x4_add(self.parts[2], rhs.parts[2]),
                f32x4_add(self.parts[3], rhs.parts[3]),
            ]}
        }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
        { Self { low: self.low + rhs.low, high: self.high + rhs.high } }
    }
}

impl std::ops::Sub for FloatX16 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
        { Self { val: self.val - rhs.val } }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), target_arch = "wasm32", target_feature = "simd128"))]
        {
            use std::arch::wasm32::*;
            Self { parts: [
                f32x4_sub(self.parts[0], rhs.parts[0]),
                f32x4_sub(self.parts[1], rhs.parts[1]),
                f32x4_sub(self.parts[2], rhs.parts[2]),
                f32x4_sub(self.parts[3], rhs.parts[3]),
            ]}
        }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
        { Self { low: self.low - rhs.low, high: self.high - rhs.high } }
    }
}

impl std::ops::Mul for FloatX16 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
        { Self { val: self.val * rhs.val } }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), target_arch = "wasm32", target_feature = "simd128"))]
        {
            use std::arch::wasm32::*;
            Self { parts: [
                f32x4_mul(self.parts[0], rhs.parts[0]),
                f32x4_mul(self.parts[1], rhs.parts[1]),
                f32x4_mul(self.parts[2], rhs.parts[2]),
                f32x4_mul(self.parts[3], rhs.parts[3]),
            ]}
        }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
        { Self { low: self.low * rhs.low, high: self.high * rhs.high } }
    }
}

impl std::ops::Div for FloatX16 {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
        { Self { val: self.val / rhs.val } }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), target_arch = "wasm32", target_feature = "simd128"))]
        {
            use std::arch::wasm32::*;
            Self { parts: [
                f32x4_div(self.parts[0], rhs.parts[0]),
                f32x4_div(self.parts[1], rhs.parts[1]),
                f32x4_div(self.parts[2], rhs.parts[2]),
                f32x4_div(self.parts[3], rhs.parts[3]),
            ]}
        }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
        { Self { low: self.low / rhs.low, high: self.high / rhs.high } }
    }
}

/// Helper to ensure WASM SIMD128 (4-wide) optimized paths are clear and utilized.
#[inline(always)]
pub fn complex_mul_accumulate_wasm_simd(re: &mut f32x4, im: &mut f32x4, hr: f32x4, hi: f32x4, ir: f32x4, ii: f32x4) {
    *re += hr * ir - hi * ii ;
    *im += hr * ii + hi * ir ;
}

/// SIMD-optimized tanh approximation for neural activation.
/// Uses a Padé approximant for high performance in real-time paths.
pub fn tanh_simd(x: f32x4) -> f32x4 {
    let large_pos_mask = x.cmp_gt(f32x4::from(4.0));
    let large_neg_mask = x.cmp_lt(f32x4::from(-4.0));

    let x2 = x * x;
    let a = x * (f32x4::from(1.0) + f32x4::from(0.12317192) * x2);
    let b = f32x4::from(1.0) + f32x4::from(0.4565311) * x2 + f32x4::from(0.01524316) * x2 * x2;
    let approx = a / b;

    let approx_clamped = large_neg_mask.blend(f32x4::from(-1.0), approx);
    large_pos_mask.blend(f32x4::from(1.0), approx_clamped)
}

/// 8-wide SIMD tanh approximation.
pub fn tanh_simd_x8(x: f32x8) -> f32x8 {
    let large_pos_mask = x.cmp_gt(f32x8::from(4.0));
    let large_neg_mask = x.cmp_lt(f32x8::from(-4.0));

    let x2 = x * x;
    let a = x * (f32x8::from(1.0) + f32x8::from(0.12317192) * x2);
    let b = f32x8::from(1.0) + f32x8::from(0.4565311) * x2 + f32x8::from(0.01524316) * x2 * x2;
    let approx = a / b;

    let approx_clamped = large_neg_mask.blend(f32x8::from(-1.0), approx);
    large_pos_mask.blend(f32x8::from(1.0), approx_clamped)
}

/// SIMD-optimized sigmoid approximation.
pub fn sigmoid_simd(x: f32x4) -> f32x4 {
    f32x4::from(0.5) + f32x4::from(0.5) * tanh_simd(x * f32x4::from(0.5))
}

pub fn soft_clip_simd(x: f32x4) -> f32x4 {
    tanh_simd(x)
}

/// 8-wide soft-clipping primitive.
pub fn soft_clip_simd_x8(x: f32x8) -> f32x8 {
    tanh_simd_x8(x)
}

impl FloatX16 {
    pub fn blend(self, on_true: Self, on_false: Self) -> Self {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
        {
            // Direct AVX-512 mask-based blending via 'wide'
            // In a full implementation, we'd use _mm512_mask_blend_ps,
            // but 'wide' f32x16.blend handles this efficiently.
            Self { val: self.val.blend(on_true.val, on_false.val) }
        }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), target_arch = "wasm32", target_feature = "simd128"))]
        {
            use std::arch::wasm32::*;
            // Wasm SIMD128 bitselect(on_true, on_false, mask)
            // mask is self.parts[i]
            Self { parts: [
                v128_bitselect(on_true.parts[0], on_false.parts[0], self.parts[0]),
                v128_bitselect(on_true.parts[1], on_false.parts[1], self.parts[1]),
                v128_bitselect(on_true.parts[2], on_false.parts[2], self.parts[2]),
                v128_bitselect(on_true.parts[3], on_false.parts[3], self.parts[3]),
            ]}
        }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
        {
             Self {
                 low: self.low.blend(on_true.low, on_false.low),
                 high: self.high.blend(on_true.high, on_false.high),
             }
        }
    }

    pub fn abs(self) -> Self {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
        { Self { val: self.val.abs() } }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), target_arch = "wasm32", target_feature = "simd128"))]
        {
            use std::arch::wasm32::*;
            Self { parts: [
                f32x4_abs(self.parts[0]),
                f32x4_abs(self.parts[1]),
                f32x4_abs(self.parts[2]),
                f32x4_abs(self.parts[3]),
            ]}
        }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
        { Self { low: self.low.abs(), high: self.high.abs() } }
    }

    pub fn sqrt(self) -> Self {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
        { Self { val: self.val.sqrt() } }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), target_arch = "wasm32", target_feature = "simd128"))]
        {
            use std::arch::wasm32::*;
            Self { parts: [
                f32x4_sqrt(self.parts[0]),
                f32x4_sqrt(self.parts[1]),
                f32x4_sqrt(self.parts[2]),
                f32x4_sqrt(self.parts[3]),
            ]}
        }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
        { Self { low: self.low.sqrt(), high: self.high.sqrt() } }
    }

    /// Lane-wise check for finite values. Returns a mask (all bits set for true).
    pub fn is_finite_mask(self) -> Self {
        #[cfg(all(target_arch = "x86_64", target_feature = "avx512f"))]
        {
            // wide's f32x16 doesn't have a direct is_finite yet in all versions,
            // so we use the standard trick: (x - x) == 0.0
            let mask = (self.val - self.val).cmp_eq(wide::f32x16::ZERO);
            let mut res = [0.0f32; 16];
            for i in 0..16 {
                if mask.to_array()[i] != 0 { res[i] = f32::from_bits(0xFFFFFFFF); }
            }
            Self::new(res)
        }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), target_arch = "wasm32", target_feature = "simd128"))]
        {
            let mut res = [f32x4::default(); 4];
            for i in 0..4 {
                let mut part_res = [0.0f32; 4];
                let a: [f32; 4] = self.parts[i].into();
                for j in 0..4 {
                    if a[j].is_finite() { part_res[j] = f32::from_bits(0xFFFFFFFF); }
                }
                res[i] = f32x4::new(part_res);
            }
            Self { parts: res }
        }
        #[cfg(all(not(all(target_arch = "x86_64", target_feature = "avx512f")), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
        {
            let mut l_res = [0.0f32; 8];
            let mut h_res = [0.0f32; 8];
            let l: [f32; 8] = self.low.into();
            let h: [f32; 8] = self.high.into();
            for i in 0..8 {
                if l[i].is_finite() { l_res[i] = f32::from_bits(0xFFFFFFFF); }
                if h[i].is_finite() { h_res[i] = f32::from_bits(0xFFFFFFFF); }
            }
            Self {
                low: f32x8::new(l_res),
                high: f32x8::new(h_res),
            }
        }
    }
}

#[inline(always)]
pub fn load_f32x16(data: &[f32], offset: usize) -> FloatX16 {
    #[cfg(target_feature = "avx512f")]
    {
        let mut arr = [0.0f32; 16];
        arr.copy_from_slice(&data[offset..offset+16]);
        FloatX16::new(arr)
    }
    #[cfg(all(not(target_feature = "avx512f"), target_arch = "wasm32", target_feature = "simd128"))]
    {
        FloatX16 {
            parts: [
                load_f32x4(data, offset),
                load_f32x4(data, offset + 4),
                load_f32x4(data, offset + 8),
                load_f32x4(data, offset + 12),
            ]
        }
    }
    #[cfg(all(not(target_feature = "avx512f"), not(all(target_arch = "wasm32", target_feature = "simd128"))))]
    {
        FloatX16 {
            low: load_f32x8(data, offset),
            high: load_f32x8(data, offset + 8),
        }
    }
}

#[inline(always)]
pub fn store_f32x16(data: &mut [f32], offset: usize, val: FloatX16) {
    let arr: [f32; 16] = val.into();
    data[offset..offset+16].copy_from_slice(&arr);
}

/// # Safety
/// The caller must ensure that `ptr` is valid for at least 16 elements of type `f32`.
#[inline(always)]
pub unsafe fn load_f32x16_ptr(ptr: *const f32) -> FloatX16 {
    let mut arr = [0.0f32; 16];
    unsafe { std::ptr::copy_nonoverlapping(ptr, arr.as_mut_ptr(), 16); }
    FloatX16::new(arr)
}

/// # Safety
/// The caller must ensure that `ptr` is valid for at least 16 elements of type `f32`.
#[inline(always)]
pub unsafe fn store_f32x16_ptr(ptr: *mut f32, val: FloatX16) {
    let arr: [f32; 16] = val.into();
    unsafe { std::ptr::copy_nonoverlapping(arr.as_ptr(), ptr, 16); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tanh_simd_precision_and_bounds() {
        let test_vals = [
            -10.0, -5.0, -2.0, -1.0, -0.5, -0.1, 0.0, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0
        ];

        for chunk in test_vals.chunks(4) {
            let mut arr = [0.0f32; 4];
            arr[..chunk.len()].copy_from_slice(chunk);
            let input = f32x4::new(arr);
            let output: [f32; 4] = tanh_simd(input).into();

            for i in 0..chunk.len() {
                let x = chunk[i];
                let ref_val = x.tanh();
                let approx_val = output[i];
                let error = (approx_val - ref_val).abs();
                // Padé approximant should be extremely precise (within 0.01 tolerance)
                assert!(error < 0.01, "tanh_simd error at x={}: got {}, ref={}", x, approx_val, ref_val);
                // Ensure outputs are correctly bounded inside [-1.0, 1.0]
                assert!(approx_val >= -1.0 && approx_val <= 1.0);
            }
        }
    }

    #[test]
    fn test_tanh_simd_x8_precision_and_bounds() {
        let test_vals = [
            -20.0, -8.0, -3.0, -1.5, -0.8, -0.05, 0.0, 0.05, 0.8, 1.5, 3.0, 8.0, 20.0
        ];

        for chunk in test_vals.chunks(8) {
            let mut arr = [0.0f32; 8];
            arr[..chunk.len()].copy_from_slice(chunk);
            let input = f32x8::new(arr);
            let output: [f32; 8] = tanh_simd_x8(input).into();

            for i in 0..chunk.len() {
                let x = chunk[i];
                let ref_val = x.tanh();
                let approx_val = output[i];
                let error = (approx_val - ref_val).abs();
                assert!(error < 0.01, "tanh_simd_x8 error at x={}: got {}, ref={}", x, approx_val, ref_val);
                assert!(approx_val >= -1.0 && approx_val <= 1.0);
            }
        }
    }

    #[test]
    fn test_sigmoid_simd_precision_and_bounds() {
        let test_vals = [
            -10.0, -4.0, -1.0, -0.2, 0.0, 0.2, 1.0, 4.0, 10.0
        ];

        for chunk in test_vals.chunks(4) {
            let mut arr = [0.0f32; 4];
            arr[..chunk.len()].copy_from_slice(chunk);
            let input = f32x4::new(arr);
            let output: [f32; 4] = sigmoid_simd(input).into();

            for i in 0..chunk.len() {
                let x = chunk[i];
                let ref_val = 1.0 / (1.0 + (-x).exp());
                let approx_val = output[i];
                let error = (approx_val - ref_val).abs();
                assert!(error < 0.01, "sigmoid_simd error at x={}: got {}, ref={}", x, approx_val, ref_val);
                assert!(approx_val >= 0.0 && approx_val <= 1.0);
            }
        }
    }

    #[test]
    fn test_soft_clip_simd_identity() {
        let test_vals = [-3.0, -1.0, 0.0, 1.0, 3.0];
        let mut arr = [0.0f32; 4];
        arr[..4].copy_from_slice(&test_vals[..4]);
        let input = f32x4::new(arr);

        let out_soft: [f32; 4] = soft_clip_simd(input).into();
        let out_tanh: [f32; 4] = tanh_simd(input).into();
        assert_eq!(out_soft, out_tanh, "soft_clip_simd must be identical to tanh_simd");
    }

    #[test]
    fn test_soft_clip_simd_x8_identity() {
        let test_vals = [-5.0, -2.0, -0.5, 0.0, 0.5, 2.0, 5.0, 10.0];
        let input = f32x8::new(test_vals);

        let out_soft: [f32; 8] = soft_clip_simd_x8(input).into();
        let out_tanh: [f32; 8] = tanh_simd_x8(input).into();
        assert_eq!(out_soft, out_tanh, "soft_clip_simd_x8 must be identical to tanh_simd_x8");
    }

    #[test]
    fn test_simd_extreme_limits_and_no_nan() {
        let extreme_vals = [
            f32::MIN, -1e6, -1000.0, 1000.0, 1e6, f32::MAX
        ];

        for chunk in extreme_vals.chunks(4) {
            let mut arr = [0.0f32; 4];
            arr[..chunk.len()].copy_from_slice(chunk);
            let input = f32x4::new(arr);

            let out_tanh: [f32; 4] = tanh_simd(input).into();
            let out_sig: [f32; 4] = sigmoid_simd(input).into();

            for i in 0..chunk.len() {
                let x = chunk[i];
                assert!(out_tanh[i].is_finite(), "tanh_simd produced non-finite value at x={}", x);
                assert!(out_sig[i].is_finite(), "sigmoid_simd produced non-finite value at x={}", x);

                if x > 100.0 {
                    assert!((out_tanh[i] - 1.0).abs() < 1e-4);
                    assert!((out_sig[i] - 1.0).abs() < 1e-4);
                } else if x < -100.0 {
                    assert!((out_tanh[i] + 1.0).abs() < 1e-4);
                    assert!(out_sig[i].abs() < 1e-4);
                }
            }
        }
    }
}
