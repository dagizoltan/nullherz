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
/// The caller must ensure that `ptr` is valid for at least 4 elements of type `f32`.
#[inline(always)]
pub unsafe fn load_f32x4_ptr(ptr: *const f32) -> FloatX4 {
    let mut arr = [0.0f32; 4];
    unsafe { std::ptr::copy_nonoverlapping(ptr, arr.as_mut_ptr(), 4); }
    f32x4::new(arr)
}

/// A 16-wide f32 SIMD type, supporting AVX-512 where available.
/// Future infrastructure for AnaWaves Layer 2 (Spectral Personality) high-density operations.
#[cfg(target_feature = "avx512f")]
pub type FloatX16 = f32x16;

#[cfg(not(target_feature = "avx512f"))]
#[derive(Clone, Copy)]
pub struct FloatX16 {
    pub low: f32x8,
    pub high: f32x8,
}

impl FloatX16 {
    pub fn new(data: [f32; 16]) -> Self {
        Self {
            low: f32x8::new([data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7]]),
            high: f32x8::new([data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15]]),
        }
    }

    pub fn splat(val: f32) -> Self {
        Self { low: f32x8::from(val), high: f32x8::from(val) }
    }
}

impl From<f32> for FloatX16 {
    fn from(val: f32) -> Self {
        Self::splat(val)
    }
}

impl std::ops::Add for FloatX16 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self { low: self.low + rhs.low, high: self.high + rhs.high }
    }
}

impl std::ops::Sub for FloatX16 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self { low: self.low - rhs.low, high: self.high - rhs.high }
    }
}

impl std::ops::Mul for FloatX16 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self { low: self.low * rhs.low, high: self.high * rhs.high }
    }
}

#[inline(always)]
pub fn load_f32x16(data: &[f32], offset: usize) -> FloatX16 {
    #[cfg(target_feature = "avx512f")]
    {
        let mut arr = [0.0f32; 16];
        arr.copy_from_slice(&data[offset..offset+16]);
        f32x16::new(arr)
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
    #[cfg(target_feature = "avx512f")]
    {
        let arr: [f32; 16] = val.into();
        data[offset..offset+16].copy_from_slice(&arr);
    }
    #[cfg(not(target_feature = "avx512f"))]
    {
        store_f32x8(data, offset, val.low);
        store_f32x8(data, offset + 8, val.high);
    }
}

/// # Safety
/// The caller must ensure that `ptr` is valid for at least 16 elements of type `f32`.
#[inline(always)]
pub unsafe fn load_f32x16_ptr(ptr: *const f32) -> FloatX16 {
    #[cfg(target_feature = "avx512f")]
    {
        let mut arr = [0.0f32; 16];
        unsafe { std::ptr::copy_nonoverlapping(ptr, arr.as_mut_ptr(), 16); }
        f32x16::new(arr)
    }
    #[cfg(not(target_feature = "avx512f"))]
    {
        unsafe {
            FloatX16 {
                low: load_f32x8_ptr(ptr),
                high: load_f32x8_ptr(ptr.add(8)),
            }
        }
    }
}

/// # Safety
/// The caller must ensure that `ptr` is valid for at least 16 elements of type `f32`.
#[inline(always)]
pub unsafe fn store_f32x16_ptr(ptr: *mut f32, val: FloatX16) {
    #[cfg(target_feature = "avx512f")]
    {
        let arr: [f32; 16] = val.into();
        unsafe { std::ptr::copy_nonoverlapping(arr.as_ptr(), ptr, 16); }
    }
    #[cfg(not(target_feature = "avx512f"))]
    {
        let low_arr: [f32; 8] = val.low.into();
        let high_arr: [f32; 8] = val.high.into();
        unsafe {
            std::ptr::copy_nonoverlapping(low_arr.as_ptr(), ptr, 8);
            std::ptr::copy_nonoverlapping(high_arr.as_ptr(), ptr.add(8), 8);
        }
    }
}
