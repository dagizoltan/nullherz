use std::sync::Arc;
use std::cell::UnsafeCell;

pub trait AudioProcessor: Send {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]]);
    fn apply_command(&mut self, _command: &control_plane::Command) {}
    fn get_telemetry(&self, _node_load: &mut [u64; 64], _node_avg_load: &mut [u64; 64], _suggestions: &mut [u8; 64], _buffer_levels: &mut [f32; 64]) {}
}

pub struct ProcessorNode {
    pub processor: Arc<UnsafeCell<Box<dyn AudioProcessor>>>,
}

unsafe impl Send for ProcessorNode {}
unsafe impl Sync for ProcessorNode {}
