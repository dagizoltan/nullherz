use std::collections::HashMap;
use nullherz_traits::{AudioProcessor, ProcessorTypeId, ProcessorFactory};
use crate::factory::*;

pub struct ProcessorRegistry {
    factories: HashMap<u32, Box<dyn ProcessorFactory>>,
    named_factories: HashMap<String, u32>,
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
        };
        registry.register_defaults();
        registry
    }

    fn register_defaults(&mut self) {
        self.register_with_type(ProcessorTypeId::BIQUAD, Box::new(BiquadFactory));
        self.register_with_type(ProcessorTypeId::GAIN, Box::new(GainFactory));
        self.register_with_type(ProcessorTypeId::SAMPLER, Box::new(SamplerFactory));
        self.register_with_type(ProcessorTypeId::BIQUAD_EQ, Box::new(BiquadFactory)); // Reuse for now
        self.register_with_type(ProcessorTypeId::CROSSFADER, Box::new(CrossfaderFactory));
        self.register_with_type(ProcessorTypeId::SUMMING, Box::new(SummingFactory));
        self.register_with_type(ProcessorTypeId::SPECTRAL, Box::new(SpectralFactory));
        self.register_with_type(ProcessorTypeId::WAVETABLE, Box::new(WavetableFactory));
        self.register_with_type(ProcessorTypeId::MODULATION, Box::new(ModulationFactory));
        self.register_with_type(ProcessorTypeId::SEQUENCER, Box::new(SequencerFactory));
    }

    pub fn register(&mut self, id: u32, factory: Box<dyn ProcessorFactory>) {
        let name = factory.name().to_lowercase();
        self.named_factories.insert(name, id);
        self.factories.insert(id, factory);
    }

    pub fn register_with_type(&mut self, p_type: ProcessorTypeId, factory: Box<dyn ProcessorFactory>) {
        let id = p_type.0;
        self.factories.insert(id, factory);
    }

    pub fn create_by_id(&self, id: u32, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        self.factories.get(&id).and_then(|f| f.create_processor(node_idx, sample_rate))
    }

    pub fn create_by_name(&self, name: &str, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        let id = self.named_factories.get(&name.to_lowercase())?;
        self.create_by_id(*id, node_idx, sample_rate)
    }

    pub fn create(&self, p_type: ProcessorTypeId, node_idx: u32, sample_rate: f32) -> Option<Box<dyn AudioProcessor>> {
        self.create_by_id(p_type.0, node_idx, sample_rate)
    }

    pub fn list_available_processors(&self) -> Vec<(u32, &str)> {
        self.factories.iter().map(|(id, f)| (*id, f.name())).collect()
    }
}
