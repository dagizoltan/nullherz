use std::collections::HashMap;
use nullherz_traits::{AudioProcessor, ProcessorType};
use crate::factory::create_processor;

pub type ProcessorCreator = fn(u32, f32) -> Box<dyn AudioProcessor>;

pub struct ProcessorRegistry {
    creators: HashMap<ProcessorType, ProcessorCreator>,
}

impl Default for ProcessorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessorRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            creators: HashMap::new(),
        };
        registry.register_defaults();
        registry
    }

    fn register_defaults(&mut self) {
        self.register(ProcessorType::Biquad, |idx, sr| create_processor(ProcessorType::Biquad as u32, idx, sr));
        self.register(ProcessorType::Gain, |idx, sr| create_processor(ProcessorType::Gain as u32, idx, sr));
        self.register(ProcessorType::Sampler, |idx, sr| create_processor(ProcessorType::Sampler as u32, idx, sr));
        self.register(ProcessorType::BiquadEQ, |idx, sr| create_processor(ProcessorType::BiquadEQ as u32, idx, sr));
        self.register(ProcessorType::Crossfader, |idx, sr| create_processor(ProcessorType::Crossfader as u32, idx, sr));
        self.register(ProcessorType::Summing, |idx, sr| create_processor(ProcessorType::Summing as u32, idx, sr));
        self.register(ProcessorType::Spectral, |idx, sr| create_processor(ProcessorType::Spectral as u32, idx, sr));
        self.register(ProcessorType::Wavetable, |idx, sr| create_processor(ProcessorType::Wavetable as u32, idx, sr));
    }

    pub fn register(&mut self, p_type: ProcessorType, creator: ProcessorCreator) {
        self.creators.insert(p_type, creator);
    }

    pub fn create(&self, p_type: ProcessorType, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        self.creators.get(&p_type).map(|creator| creator(node_idx, sample_rate))
    }
}
