use std::collections::HashMap;
use nullherz_traits::{AudioProcessor, ProcessorType, ProcessorFactory};
use crate::factory::*;

pub struct ProcessorRegistry {
    factories: HashMap<u32, Box<dyn ProcessorFactory>>,
    named_factories: HashMap<String, u32>,
    type_to_id: HashMap<ProcessorType, u32>,
}

impl Default for ProcessorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessorRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            factories: HashMap::new(),
            named_factories: HashMap::new(),
            type_to_id: HashMap::new(),
        };
        registry.register_defaults();
        registry
    }

    fn register_defaults(&mut self) {
        self.register_with_type(ProcessorType::Biquad, Box::new(BiquadFactory));
        self.register_with_type(ProcessorType::Gain, Box::new(GainFactory));
        self.register_with_type(ProcessorType::Sampler, Box::new(SamplerFactory));
        self.register_with_type(ProcessorType::BiquadEQ, Box::new(BiquadFactory)); // Reuse for now
        self.register_with_type(ProcessorType::Crossfader, Box::new(CrossfaderFactory));
        self.register_with_type(ProcessorType::Summing, Box::new(SummingFactory));
        self.register_with_type(ProcessorType::Spectral, Box::new(SpectralFactory));
        self.register_with_type(ProcessorType::Wavetable, Box::new(WavetableFactory));
    }

    pub fn register(&mut self, id: u32, factory: Box<dyn ProcessorFactory>) {
        let name = factory.name().to_lowercase();
        self.named_factories.insert(name, id);
        self.factories.insert(id, factory);
    }

    pub fn register_with_type(&mut self, p_type: ProcessorType, factory: Box<dyn ProcessorFactory>) {
        let id = p_type as u32;
        self.type_to_id.insert(p_type, id);
        self.factories.insert(id, factory);
    }

    pub fn create_by_id(&self, id: u32, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        self.factories.get(&id).and_then(|f| f.create_processor(node_idx, sample_rate))
    }

    pub fn create_by_name(&self, name: &str, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        let id = self.named_factories.get(&name.to_lowercase())?;
        self.create_by_id(*id, node_idx, sample_rate)
    }

    pub fn create(&self, p_type: ProcessorType, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        let id = self.type_to_id.get(&p_type)?;
        self.create_by_id(*id, node_idx, sample_rate)
    }

    pub fn list_available_processors(&self) -> Vec<(u32, &str)> {
        self.factories.iter().map(|(id, f)| (*id, f.name())).collect()
    }
}
