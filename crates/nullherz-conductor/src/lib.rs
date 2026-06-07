use audio_core::{AudioEngine, ProcessorGraph, AudioBackend, AlsaBackend, ThreadedBackend};
use fx_runtime::SidecarManager;
use ipc_layer::RingBuffer;


pub struct Conductor {
    pub manager: SidecarManager,
    pub engine: Option<AudioEngine>,
    pub backend: Option<Box<dyn AudioBackend>>,
}

impl Conductor {
    pub fn new() -> Self {
        Self {
            manager: SidecarManager::new(),
            engine: None,
            backend: None,
        }
    }

    pub fn setup_engine(&mut self) -> (ipc_layer::Producer<control_plane::TimestampedCommand>, ipc_layer::Consumer<audio_core::Telemetry>) {
        let (cmd_prod, cmd_cons) = RingBuffer::new(1024).split();
        let (garbage_prod, _garbage_cons) = RingBuffer::new(1024).split();
        let (tel_prod, tel_cons) = RingBuffer::new(1024).split();

        let graph = ProcessorGraph::new();
        let engine = AudioEngine::new(cmd_cons, garbage_prod, tel_prod, Box::new(graph));
        self.engine = Some(engine);

        (cmd_prod, tel_cons)
    }

    pub fn start_backend(&mut self, name: &str) -> Result<(), String> {
        let engine = self.engine.take().ok_or("Engine not initialized")?;
        let mut backend: Box<dyn AudioBackend> = match name {
            "alsa" => Box::new(AlsaBackend::new()),
            _ => Box::new(ThreadedBackend::new()),
        };

        backend.start(engine)?;
        self.backend = Some(backend);
        Ok(())
    }

    pub fn stop_backend(&mut self) {
        if let Some(mut backend) = self.backend.take() {
            self.engine = backend.stop();
        }
    }

    pub fn switch_backend(&mut self, name: &str) -> Result<(), String> {
        self.stop_backend();
        self.start_backend(name)
    }
}
