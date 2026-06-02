use serde_big_array::BigArray;
use serde::{Serialize, Deserialize};

#[repr(C)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Telemetry {
    pub process_time_ns: u64,
    pub sample_counter: u64,
    pub xrun_count: u32,
    #[serde(with = "BigArray")]
    pub node_times_cycles: [u64; 64],
    #[serde(with = "BigArray")]
    pub peak_levels: [f32; 64],
}
