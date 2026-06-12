use std::sync::{Arc, Mutex};
use audio_core::AudioEngine;
use nullherz_backends::{AudioBackend, AlsaBackend, ThreadedBackend};

pub struct BackendManager {
    pub backend: Option<Box<dyn AudioBackend>>,
    pub engine_handle: Arc<Mutex<Option<AudioEngine>>>,
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
    pub fn start(&mut self, name: &str) -> Result<(), String> {
        // Move current process to high-priority Cgroup
        let _ = ipc_layer::move_to_cgroup("nullherz", std::process::id() as i32);

        let mut backend: Box<dyn AudioBackend> = match name {
            "alsa" => Box::new(AlsaBackend::new()),
            "pipewire" => Box::new(nullherz_backends::PipewireBackend::new()),
            "jack" => Box::new(nullherz_backends::JackBackend::new()),
            _ => Box::new(ThreadedBackend::new()),
        };

        backend.start(self.engine_handle.clone())?;
        self.backend = Some(backend);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut backend) = self.backend.take() {
            backend.stop();
        }
    }

    pub fn switch(&mut self, name: &str) -> Result<(), String> {
        self.stop();
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.start(name)
    }
}
