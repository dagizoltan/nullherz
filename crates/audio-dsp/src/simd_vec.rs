use wide::*;

pub type FloatX4 = f32x4;
pub type FloatX8 = f32x8;

#[inline(always)]
pub fn load_f32x8(data: &[f32], offset: usize) -> FloatX8 {
    f32x8::new(data[offset..offset+8].try_into().unwrap())
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
