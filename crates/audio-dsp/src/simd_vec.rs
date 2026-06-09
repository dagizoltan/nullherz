use wide::*;

pub type FloatX4 = f32x4;

#[inline(always)]
pub fn load_f32x4(data: &[f32], offset: usize) -> FloatX4 {
    f32x4::new([data[offset], data[offset+1], data[offset+2], data[offset+3]])
}

#[inline(always)]
pub fn store_f32x4(data: &mut [f32], offset: usize, val: FloatX4) {
    let arr: [f32; 4] = val.into();
    data[offset..offset+4].copy_from_slice(&arr);
}
