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
