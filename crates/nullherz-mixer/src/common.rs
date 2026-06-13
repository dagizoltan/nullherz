pub const BUF_MASTER_L: usize = 0;
pub const BUF_MASTER_R: usize = 1;
pub const BUF_CUE_L: usize = 2;
pub const BUF_CUE_R: usize = 3;
pub const BUF_BROADCAST_L: usize = 4;
pub const BUF_BROADCAST_R: usize = 5;
pub const BUF_DJ_A_L: usize = 8;
pub const BUF_DJ_A_R: usize = 9;
pub const BUF_DJ_B_L: usize = 10;
pub const BUF_DJ_B_R: usize = 11;

pub struct MixerConfig {
    pub master_l: usize,
    pub master_r: usize,
    pub cue_l: usize,
    pub cue_r: usize,
    pub dj_a_l: usize,
    pub dj_a_r: usize,
    pub dj_b_l: usize,
    pub dj_b_r: usize,
}

impl Default for MixerConfig {
    fn default() -> Self {
        Self {
            master_l: BUF_MASTER_L,
            master_r: BUF_MASTER_R,
            cue_l: BUF_CUE_L,
            cue_r: BUF_CUE_R,
            dj_a_l: BUF_DJ_A_L,
            dj_a_r: BUF_DJ_A_R,
            dj_b_l: BUF_DJ_B_L,
            dj_b_r: BUF_DJ_B_R,
        }
    }
}
