pub mod alsa;
pub mod pipewire;
pub mod jack;
pub mod threaded;
pub mod mock;

pub use alsa::AlsaBackend;
pub use pipewire::PipewireBackend;
pub use jack::JackBackend;
pub use threaded::ThreadedBackend;
pub use mock::MockBackend;


use std::sync::Arc;
use parking_lot::Mutex;
pub use nullherz_traits::{RenderingEngine, AudioBackendType};

pub trait AudioBackend: Send {
    fn start(&mut self, engine: Arc<Mutex<Option<Arc<dyn RenderingEngine>>>>, period_size: u64) -> Result<(), String>;
    fn stop(&mut self);
    fn enumerate_devices(&self) -> Vec<String> { Vec::new() }
}

pub struct BackendFactory;

impl BackendFactory {
    pub fn create(backend_type: AudioBackendType) -> Box<dyn AudioBackend> {
        match backend_type {
            AudioBackendType::Alsa => Box::new(AlsaBackend::new()),
            AudioBackendType::Pipewire => Box::new(PipewireBackend::new()),
            AudioBackendType::Jack => Box::new(JackBackend::new()),
            AudioBackendType::Threaded => Box::new(ThreadedBackend::new()),
            AudioBackendType::Mock => Box::new(MockBackend::new()),
        }
    }
}
