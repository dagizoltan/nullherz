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

/// A 16-wide f32 SIMD type, supporting AVX-512 where available.
/// Future infrastructure for AnaWaves Layer 2 (Spectral Personality) high-density operations.
#[derive(Clone, Copy)]
pub struct FloatX16 {
    #[cfg(target_feature = "avx512f")]
    pub(crate) val: wide::f32x16,
    #[cfg(not(target_feature = "avx512f"))]
    pub(crate) low: f32x8,
    #[cfg(not(target_feature = "avx512f"))]
    pub(crate) high: f32x8,
}

impl FloatX16 {
    pub fn new(data: [f32; 16]) -> Self {
        #[cfg(target_feature = "avx512f")]
        { Self { val: wide::f32x16::from(data) } }
        #[cfg(not(target_feature = "avx512f"))]
        {
            Self {
                low: f32x8::new([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]),
                high: f32x8::new([data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]]),
            }
        }
    }

    pub fn splat(val: f32) -> Self {
        #[cfg(target_feature = "avx512f")]
        { Self { val: wide::f32x16::from(val) } }
        #[cfg(not(target_feature = "avx512f"))]
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
        #[cfg(target_feature = "avx512f")]
        { val.val.into() }
        #[cfg(not(target_feature = "avx512f"))]
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
        #[cfg(target_feature = "avx512f")]
        { Self { val: self.val + rhs.val } }
        #[cfg(not(target_feature = "avx512f"))]
        { Self { low: self.low + rhs.low, high: self.high + rhs.high } }
    }
}

impl std::ops::Sub for FloatX16 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        #[cfg(target_feature = "avx512f")]
        { Self { val: self.val - rhs.val } }
        #[cfg(not(target_feature = "avx512f"))]
        { Self { low: self.low - rhs.low, high: self.high - rhs.high } }
    }
}

impl std::ops::Mul for FloatX16 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        #[cfg(target_feature = "avx512f")]
        { Self { val: self.val * rhs.val } }
        #[cfg(not(target_feature = "avx512f"))]
        { Self { low: self.low * rhs.low, high: self.high * rhs.high } }
    }
}

impl FloatX16 {
    pub fn blend(self, on_true: Self, on_false: Self) -> Self {
        #[cfg(target_feature = "avx512f")]
        {
            // Note: self is used as mask. We assume self lanes are 0.0 or non-zero.
            // true f32x16.blend takes a mask type.
            let mut res = [0.0f32; 16];
            let m: [f32; 16] = self.into();
            let t: [f32; 16] = on_true.into();
            let f: [f32; 16] = on_false.into();
            for i in 0..16 {
                res[i] = if m[i].to_bits() != 0 { t[i] } else { f[i] };
            }
            Self::new(res)
        }
        #[cfg(not(target_feature = "avx512f"))]
        {
             Self {
                 low: self.low.blend(on_true.low, on_false.low),
                 high: self.high.blend(on_true.high, on_false.high),
             }
        }
    }

    /// Lane-wise check for finite values. Returns a mask (all bits set for true).
    pub fn is_finite_mask(self) -> Self {
        #[cfg(target_feature = "avx512f")]
        {
            let mut res = [0.0f32; 16];
            let a: [f32; 16] = self.into();
            for i in 0..16 {
                if a[i].is_finite() { res[i] = f32::from_bits(0xFFFFFFFF); }
            }
            Self::new(res)
        }
        #[cfg(not(target_feature = "avx512f"))]
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
    #[cfg(not(target_feature = "avx512f"))]
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
