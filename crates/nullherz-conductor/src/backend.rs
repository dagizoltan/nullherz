use std::sync::{Arc, Mutex};
use nullherz_traits::{RenderingEngine, AudioBackendType};
use nullherz_backends::AudioBackend;

pub struct BackendManager {
    pub backend: Option<Box<dyn AudioBackend>>,
    pub engine_handle: Arc<Mutex<Option<Arc<dyn RenderingEngine>>>>,
}

impl Default for BackendManager {
    fn default() -> Self {
        Self {
            backend: None,
            engine_handle: Arc::new(Mutex::new(None)),
        }
    }
}

impl BackendManager {
    pub fn start(&mut self, backend_type: AudioBackendType, period_size: u64) -> Result<(), String> {
        // Move current process to high-priority Cgroup
        let _ = ipc_layer::move_to_cgroup("nullherz", std::process::id() as i32);

        let mut backend = nullherz_backends::BackendFactory::create(backend_type);

        backend.start(self.engine_handle.clone(), period_size)?;
        self.backend = Some(backend);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut backend) = self.backend.take() {
            backend.stop();
        }
    }

    pub fn switch(&mut self, backend_type: AudioBackendType, period_size: u64) -> Result<(), String> {
        self.stop();
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.start(backend_type, period_size)
    }
}
