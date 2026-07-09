use std::collections::HashMap;
use nullherz_traits::{AudioProcessor, ProcessorTypeId, ProcessorFactory, ProcessorCapability};
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
        self.register_factory(Box::new(BiquadFactory));
        self.register_factory(Box::new(GainFactory));
        self.register_factory(Box::new(SamplerFactory));
        self.register_factory(Box::new(CrossfaderFactory));
        self.register_factory(Box::new(SummingFactory));
        self.register_factory(Box::new(SpectralFactory));
        self.register_factory(Box::new(WavetableFactory));
        self.register_factory(Box::new(ModulationFactory));
        self.register_factory(Box::new(SequencerFactory));
        self.register_factory(Box::new(EnvelopeFollowerFactory));
        self.register_factory(Box::new(GranularFactory));
        self.register_factory(Box::new(SpectralMorphFactory));
        self.register_factory(Box::new(CaptureFactory));
        self.register_factory(Box::new(DjIsolatorFactory));
        self.register_factory(Box::new(SimdBiquadFactory));
        self.register_factory(Box::new(KeySyncFactory));
        self.register_factory(Box::new(PersonalityInheritanceFactory));
    }

    pub fn register_factory(&mut self, factory: Box<dyn ProcessorFactory>) {
        let id = factory.type_id().0;
        let name = factory.name().to_lowercase();
        self.named_factories.insert(name, id);
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

    pub fn get_capabilities(&self, id: u32) -> Option<ProcessorCapability> {
        self.factories.get(&id).map(|f| f.capabilities())
    }
}
