use std::sync::Arc;
use audio_core::{AudioEngine, ProcessorGraph, AudioBackend, AlsaBackend, ThreadedBackend};
use fx_runtime::SidecarManager;
use ipc_layer::RingBuffer;


pub struct Timeline {
    pub bpm: f32,
    pub signature_num: u32,
    pub signature_den: u32,
    pub current_beat: f64,
}

pub struct Conductor {
    pub manager: SidecarManager,
    pub timeline: Timeline,
    pub engine: Option<AudioEngine>,
    pub backend: Option<Box<dyn AudioBackend>>,
    garbage_consumer: Option<ipc_layer::Consumer<Box<dyn audio_core::AudioProcessor>>>,
    pub topo_producer: Option<ipc_layer::NonRtProducer<control_plane::TopologyCommand>>,
}

impl Conductor {
    pub fn new() -> Self {
        Self {
            manager: SidecarManager::new(),
            timeline: Timeline {
                bpm: 120.0,
                signature_num: 4,
                signature_den: 4,
                current_beat: 0.0,
            },
            engine: None,
            backend: None,
            garbage_consumer: None,
            topo_producer: None,
        }
    }

    pub fn setup_engine(&mut self) -> (Arc<ipc_layer::MpscRingBuffer<control_plane::TimestampedCommand>>, ipc_layer::Consumer<audio_core::Telemetry>) {
        let cmd_buffer = Arc::new(ipc_layer::MpscRingBuffer::new(1024));
        let cmd_cons = cmd_buffer.clone();
        let (_, bundle_cons) = RingBuffer::<Vec<control_plane::Command>>::new(16).split();
        let (topo_prod, topo_cons) = RingBuffer::new(64).split();
        let topo_prod = ipc_layer::NonRtProducer::new(topo_prod);
        let (garbage_prod, garbage_cons) = RingBuffer::new(1024).split();
        let (tel_prod, tel_cons) = RingBuffer::new(1024).split();

        let graph = ProcessorGraph::new();
        let engine = AudioEngine::new(cmd_cons, Some(bundle_cons), Some(topo_cons), garbage_prod, tel_prod, Box::new(graph));
        self.engine = Some(engine);
        self.garbage_consumer = Some(garbage_cons);
        self.topo_producer = Some(topo_prod);

        (cmd_buffer, tel_cons)
    }

    pub fn start_backend(&mut self, name: &str) -> Result<(), String> {
        let engine = self.engine.take().ok_or("Engine not initialized")?;

        // Move current process to high-priority Cgroup
        let _ = ipc_layer::move_to_cgroup("nullherz", std::process::id() as i32);

        let mut backend: Box<dyn AudioBackend> = match name {
            "alsa" => Box::new(AlsaBackend::new()),
            "pipewire" => Box::new(audio_core::PipewireBackend::new()),
            "jack" => Box::new(audio_core::JackBackend::new()),
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
        // Hot-swap: stop old, preserve engine state, start new
        self.stop_backend();

        // Brief sleep to allow ALSA/JACK descriptors to truly release
        std::thread::sleep(std::time::Duration::from_millis(50));

        self.start_backend(name)
    }

    pub fn drain_garbage(&mut self) {
        if let Some(ref mut cons) = self.garbage_consumer {
            while let Some(proc) = cons.pop() {
                drop(proc);
            }
        }
    }

    pub fn update_timeline(&mut self, telemetry: &audio_core::Telemetry) {
        // Sync conductor timeline with engine reality
        self.timeline.current_beat = telemetry.sample_counter as f64 / 44100.0 * (self.timeline.bpm as f64 / 60.0);
    }

    pub fn quantize_beat(&self, beat: f64, grid: f64) -> f64 {
        (beat / grid).ceil() * grid
    }
}
