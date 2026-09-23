// Non-RT plane (orchestration-tick pacing): thread spawn/sleep are sanctioned here.
// The disallowed-methods lint exists to protect the audio hot path only.
#![allow(clippy::disallowed_methods)]
use crate::engine_coordinator::EngineCoordinator;
use crate::topology_manager::TopologyManager;
use crate::transfusion_manager::TransfusionManager;
use crate::mixer_bridge::MixerBridge;
use crate::ipc_audio_bridge::IpcAudioBridge;
use crate::sidecar_supervisor::SidecarSupervisor;
use crate::midi_mapper::MidiMapper;
use crate::pattern_manager::PatternManager;
use crate::clip_orchestrator::ClipOrchestrator;
use crate::modulation_matrix::ModulationMatrix;
use nullherz_traits::{Command, telemetry::Telemetry};
use std::sync::Arc;
use parking_lot::Mutex;
use nullherz_dna::{ GeneticLibrary};


/// Name of the MIDI-bridge shared-memory object, unique to one `Conductor`.
///
/// It used to be the constant `"nullherz_midi_bridge"`, written in two places
/// that had to agree — and every `Conductor` on the machine therefore raced for
/// the same object. `SharedMemory::create` opened it `O_TRUNC`, so a second
/// creator truncated the region while the first still had it MAPPED, and any
/// later touch of those pages is a SIGBUS. Latent while pages were faulted in
/// lazily; prefaulting at creation made it fire in 7 of 30 release runs of
/// `raw_mode_test`, and 0 of 20 with `--test-threads=1`.
///
/// The first fix keyed this on the PROCESS ID, with a comment saying the scope
/// was "a child of THIS conductor". The comment was right and the code did not
/// match it: a process can hold several conductors, which is exactly what a test
/// binary is — eleven tests in `playback_regression_test` each build one, in
/// parallel, and every one of them asked for the same pid-keyed name. That
/// collided on every run and intermittently HUNG, because the reclaim path
/// (unlink, then retry `O_EXCL`) races when several threads run it at once.
///
/// So: per INSTANCE. The counter makes two conductors in one process distinct,
/// and the pid keeps two processes distinct. Both the creator and the `--shm`
/// argument handed to the child read the same stored string, so they cannot
/// drift apart again.
fn next_midi_bridge_shm_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!(
        "nullherz_midi_bridge_{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

pub struct Conductor {
    pub engine_coordinator: EngineCoordinator,
    pub topology_manager: TopologyManager,
    pub sidecar_discovery: crate::discovery::SidecarDiscoveryService,
    pub transfusion_manager: TransfusionManager,
    pub mixer_bridge: MixerBridge,
    pub sidecar_supervisor: SidecarSupervisor,
    pub pattern_manager: PatternManager,
    pub clip_orchestrator: ClipOrchestrator,
    pub modulation_matrix: ModulationMatrix,
    pub audio_bridge: Arc<IpcAudioBridge>,
    pub midi_mapper: MidiMapper,
    pub midi_clock: crate::midi_clock::MidiClockTracker,
    pub analysis_worker: Option<crate::analysis_worker::AnalysisWorker>,
    pub folder_monitor: Option<crate::folder_monitor::FolderMonitor>,
    pub streaming_manager: crate::streaming_manager::StreamingManager,
    pub library: Arc<parking_lot::Mutex<nullherz_dna::LibraryDatabase>>,
    pub mixer_manager: nullherz_mixer::MixerManager,
    pub midi_producer: Option<ipc_layer::Producer<nullherz_traits::MidiEvent>>,
    pub midi_consumer: Option<ipc_layer::Consumer<nullherz_traits::MidiEvent>>,
    pub external_midi_consumer: Option<ipc_layer::IpcMidiConsumer>,
    midi_child: Option<std::process::Child>,
    midi_shm: Option<Arc<ipc_layer::SharedMemory>>,
    pub matchmaking_suggestions: Arc<Mutex<Vec<(u64, f32)>>>,
    pub calibration_samples: u32,
    pub period_size: u64,
    /// Registry length at the last reap sweep — see `reap_registry`. `usize::MAX`
    /// so the first sweep always runs.
    last_registry_reap_len: usize,
    /// Size of the analysed set at the last reap sweep. Paired with
    /// `last_registry_reap_len` — see `reap_registry` for why one is not enough.
    last_analysed_len: usize,
    /// This conductor's MIDI-bridge shared-memory name. See
    /// `next_midi_bridge_shm_name` — it must be stored, not regenerated, because
    /// the creator and the spawned bridge's `--shm` argument have to agree.
    midi_shm_name: String,
    /// Ids the analysis worker has finished with.
    ///
    /// Held as a shared handle because `analysis_worker` is `take()`n at startup
    /// and moved onto its own thread — reading the worker field instead would
    /// consult `None` in production and a permanently empty set in tests, which
    /// is exactly the wrong answer in both directions.
    analysed_ids: Arc<parking_lot::Mutex<std::collections::HashSet<u64>>>,
    pub ptp_clock: Option<Arc<nullherz_traits::PtpClockProvider>>,
    last_autosave_secs: u64,
    pub last_genetic_evolve_secs: u64,
    last_metadata_sync_secs: u64,
    last_registry_reap_secs: u64,
    /// Audio device names, refreshed on a slow timer rather than per frame.
    ///
    /// `enumerate_devices()` is a real driver query: dlopen + ~18 dlsym +
    /// `snd_device_name_hint`, which parses ALSA's config to build the list.
    /// Measured at 74.6 ms on the reference machine. Telemetry runs ~187 times
    /// a second, so calling it there costs ~14 SECONDS of work per second of
    /// audio and the conductor can never drain its telemetry queue — decks stop
    /// responding to load and play entirely. The device list changes when
    /// hardware is plugged in, not 187 times a second.
    cached_audio_devices: Vec<String>,
    /// Resident sample count and bytes, refreshed on a slow timer.
    ///
    /// Same reasoning as [`Conductor::cached_audio_devices`]: computing this
    /// walks every registered sample, and telemetry runs ~187 times a second.
    /// With a real library that is tens of thousands of refcount operations per
    /// second to render a number nobody watches change that fast.
    cached_residency: (u32, u64),
    last_residency_scan: Option<std::time::Instant>,
    last_device_scan: Option<std::time::Instant>,
    pub focused_node_idx: Option<u32>,
    pub active_transitions: Vec<DnaTransition>,
    pub undo_stack: Vec<(
        crate::persistence::ProjectState,
        std::collections::HashMap<u64, (std::sync::Arc<Vec<f32>>, std::sync::Arc<nullherz_traits::SampleMetadata>)>,
    )>,
    pub redo_stack: Vec<(
        crate::persistence::ProjectState,
        std::collections::HashMap<u64, (std::sync::Arc<Vec<f32>>, std::sync::Arc<nullherz_traits::SampleMetadata>)>,
    )>,
    // --- Live RTMP/Opus Broadcast Streaming ---
    /// Samples currently being decoded on background hydration threads
    /// (see command_handler): dedupes concurrent loads of the same track.
    pub hydration_pending: std::collections::HashSet<u64>,
    pub hydration_progress: std::sync::Arc<parking_lot::Mutex<std::collections::HashMap<u64, f32>>>,
    pub(crate) hydration_done_tx: std::sync::mpsc::Sender<u64>,
    hydration_done_rx: std::sync::mpsc::Receiver<u64>,
    /// Last metadata `sync_sampler_metadata` pushed to each sampler node, so the
    /// once-a-second reconciliation re-pushes only on a real change. The Arc is
    /// held (not just its address) because that is what makes `Arc::ptr_eq` a
    /// sound identity test — a dropped allocation could otherwise be reused at
    /// the same address and read as "unchanged".
    synced_node_metadata: std::collections::HashMap<u32, Arc<nullherz_traits::SampleMetadata>>,
    pub is_streaming: bool,
    pub stream_start_time: Option<std::time::Instant>,
    pub stream_bitrate: f32,
    pub stream_dropped_frames: u32,
    pub stream_viewers: u32,
    /// Which listeners `setup_engine` starts. See [`NetworkConfig`].
    pub network: NetworkConfig,
    /// Tripped by `Drop`. Every long-lived listener this conductor spawned
    /// polls it and exits, so a dropped `Conductor` does not leave sockets
    /// bound and threads running for the life of the process.
    shutdown: Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for Conductor {
    fn drop(&mut self) {
        self.shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub struct DnaTransition {
    pub source_deck: char,
    pub target_deck: char,
    pub start_beat: f64,
    pub duration_beats: f64,
    pub is_complete: bool,
}

/// Which network listeners `setup_engine` starts, and on which ports.
///
/// These used to be four hardcoded constants started unconditionally whenever
/// a tokio runtime was present — which includes every test that builds a
/// `Conductor`. Two consequences followed. Parallel test binaries fought over
/// the same fixed ports, and a test process inherited four listeners it had no
/// way to stop. `listeners_enabled` is the switch; port 0 asks the OS for an
/// ephemeral port and the `start_*` functions report back what they got.
#[derive(Debug, Clone, Copy)]
pub struct NetworkConfig {
    /// Start the remote-sidecar, discovery, audio-return and DNA listeners.
    pub listeners_enabled: bool,
    /// TCP: remote sidecars attach here.
    pub sidecar_port: u16,
    /// UDP: sidecar discovery beacons arrive here.
    pub discovery_port: u16,
    /// UDP: distributed audio-return blocks (protocol type 6).
    pub audio_return_port: u16,
    /// TCP: federated DNA pull server.
    pub dna_port: u16,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listeners_enabled: true,
            sidecar_port: 9000,
            discovery_port: 9001,
            audio_return_port: 9002,
            dna_port: 9003,
        }
    }
}

impl NetworkConfig {
    /// Every listener off. What tests want, and what `":memory:"` selects.
    pub fn disabled() -> Self {
        Self { listeners_enabled: false, ..Self::default() }
    }

    /// Listeners on, but every port ephemeral — for an integration test that
    /// genuinely exercises the network without colliding with a parallel one.
    pub fn ephemeral() -> Self {
        Self {
            listeners_enabled: true,
            sidecar_port: 0,
            discovery_port: 0,
            audio_return_port: 0,
            dna_port: 0,
        }
    }
}

impl Default for Conductor {
    fn default() -> Self {
        Self::new()
    }
}

/// Spawn background work, but only when a Tokio runtime is actually present.
///
/// `tick()` is periodic housekeeping and may legitimately be driven from a
/// NON-async host: tests, an offline render, or an embedding application that
/// owns its own event loop. `tokio::spawn` PANICS without a runtime, so an
/// unguarded call turns "this host isn't async" into a crash on the conductor
/// thread — and one that fires on a schedule, which is how it stayed hidden.
///
/// Everything spawned from `tick()` is optional: autosave, remote-node audio
/// forwarding, matchmaking suggestions. None of it is required for audio to keep
/// playing, so skipping is always the better failure than aborting the tick.
fn spawn_background<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::spawn(future);
    }
}

/// `spawn_blocking` counterpart of [`spawn_background`], with the same rationale.
fn spawn_blocking_background<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::spawn_blocking(f);
    }
}

impl Conductor {
    pub fn new() -> Self {
        Self::with_library_path("library.redb")
    }

    pub fn with_library(library: Arc<parking_lot::Mutex<nullherz_dna::LibraryDatabase>>) -> Self {
        let sample_registry = Arc::new(nullherz_dna::SampleRegistry::new());
        let sidecar_discovery = crate::discovery::SidecarDiscoveryService::new("plugins").with_library(library.clone());
        let dna_discovery = sidecar_discovery.dna_discovery.clone();

        let mut transfusion_manager = TransfusionManager::new(sample_registry.clone());
        transfusion_manager.discovery_service = Some(dna_discovery);
        transfusion_manager = transfusion_manager.with_library(library.clone());

        let (hydration_done_tx, hydration_done_rx) = std::sync::mpsc::channel();

        // Built here so its "finished" set can be shared BEFORE `start()` moves
        // the worker onto its own thread. See `analysed_ids`.
        let analysis_worker = crate::analysis_worker::AnalysisWorker::new(sample_registry.clone())
            .with_library(library.clone());
        let analysis_worker_handle = analysis_worker.analysed_ids();

        Self {
            engine_coordinator: EngineCoordinator::new(),
            topology_manager: TopologyManager::new(),
            transfusion_manager,
            mixer_bridge: MixerBridge::new(),
            sidecar_supervisor: SidecarSupervisor::new(),
            pattern_manager: PatternManager::new(),
            clip_orchestrator: ClipOrchestrator::new(),
            modulation_matrix: ModulationMatrix::new(),
            audio_bridge: Arc::new(IpcAudioBridge::new()),
            sidecar_discovery,
            midi_mapper: MidiMapper::new(),
            midi_clock: crate::midi_clock::MidiClockTracker::new(),
            analysis_worker: Some(analysis_worker),
            folder_monitor: Some(
                crate::folder_monitor::FolderMonitor::new(sample_registry, library.clone())
                    .with_analysed_ids(analysis_worker_handle.clone()),
            ),
            streaming_manager: crate::streaming_manager::StreamingManager::new(),
            library,
            mixer_manager: nullherz_mixer::MixerManager::new(),
            midi_producer: None,
            midi_consumer: None,
            external_midi_consumer: None,
            midi_child: None,
            midi_shm: None,
            matchmaking_suggestions: Arc::new(Mutex::new(Vec::new())),
            calibration_samples: 0,
            period_size: 128,
            ptp_clock: None,
            last_autosave_secs: 0,
            last_genetic_evolve_secs: 0,
            last_metadata_sync_secs: 0,
            last_registry_reap_secs: 0,
            last_registry_reap_len: usize::MAX,
            last_analysed_len: usize::MAX,
            analysed_ids: analysis_worker_handle,
            midi_shm_name: next_midi_bridge_shm_name(),
            cached_audio_devices: Vec::new(),
            cached_residency: (0, 0),
            last_residency_scan: None,
            last_device_scan: None,
            focused_node_idx: None,
            active_transitions: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            hydration_pending: std::collections::HashSet::new(),
            hydration_progress: std::sync::Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
            hydration_done_tx,
            hydration_done_rx,
            synced_node_metadata: std::collections::HashMap::new(),
            is_streaming: false,
            stream_start_time: None,
            stream_bitrate: 256.0,
            stream_dropped_frames: 0,
            stream_viewers: 42,
            network: NetworkConfig::default(),
            shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Open (or fall back to) the library at `path` and build a conductor on it.
    ///
    /// `":memory:"` is the established test sentinel (see
    /// `tests/test_isolation_gate_test.rs`); it also turns the network
    /// listeners OFF. Tests do not want four fixed ports bound, and parallel
    /// test binaries fighting over 9000-9003 is not a thing any of them meant
    /// to exercise. A test that genuinely wants listeners sets
    /// `network = NetworkConfig::ephemeral()` before `setup_engine`.
    pub fn with_library_path(path: &str) -> Self {
        let library = Self::open_library(path);
        let mut conductor = Self::with_library(library);
        if path == ":memory:" {
            conductor.network = NetworkConfig::disabled();
        }
        conductor
    }

    fn open_library(path: &str) -> Arc<parking_lot::Mutex<nullherz_dna::LibraryDatabase>> {
        match nullherz_dna::LibraryDatabase::load(path) {
            Ok(db) => Arc::new(parking_lot::Mutex::new(db)),
            Err(e) => {
                // redb holds an exclusive file lock, so a second opener of the
                // same path lands here and gets a private database instead.
                //
                // ANNOUNCED, not silent. This branch is a RACE: whoever opens
                // first gets the real library and everyone after gets an empty
                // one, so a caller's view of the library depends on scheduling.
                // Under `cargo test --workspace` that meant a test could see the
                // developer's 233 MB library on one run and an empty database on
                // the next — the same test, two different worlds. Silence is what
                // made the resulting flake unfindable; if this line appears in a
                // test run, that test should be asking for `":memory:"`.
                static FALLBACK_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                let count = FALLBACK_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                eprintln!(
                    "Conductor: library {path:?} unavailable ({e}); falling back to a private \
                     database. The caller will NOT see the real library."
                );
                let fallback_path = std::env::temp_dir()
                    .join(format!("nullherz_fallback_{}_{}.redb", std::process::id(), count));
                let fallback = nullherz_dna::LibraryDatabase::load(&fallback_path.to_string_lossy())
                    .or_else(|e| {
                        eprintln!("Library fallback DB failed ({}); using in-memory library.", e);
                        nullherz_dna::LibraryDatabase::load(":memory:")
                    })
                    .expect("in-memory library database cannot fail to open");
                Arc::new(parking_lot::Mutex::new(fallback))
            }
        }
    }

    pub fn setup_engine(&mut self) -> crate::EngineContext {
        let registry = self.transfusion_manager.sample_registry.clone();
        // Initialize PTP Clock if on Linux
        #[cfg(target_os = "linux")]
        if let Ok(clock) = nullherz_traits::PtpClockProvider::new("eth0") {
            let clock_arc = Arc::new(clock);
            self.ptp_clock = Some(clock_arc.clone());

            // Start PTP Engine
            if let Ok(ptp) = crate::ptp_engine::PtpEngine::new(clock_arc as Arc<dyn nullherz_traits::ClockProvider>, 319, false) {
                std::thread::spawn(move || ptp.run_loop());
            }
        }

        let handle = self.engine_coordinator.setup(registry);

        self.midi_producer = Some(handle.midi_producer.clone());
        self.mixer_bridge.bundle_producer = Some(handle.bundle_producer);
        self.mixer_bridge.bundle_pool = handle.bundle_garbage_consumer;
        self.topology_manager.topo_producer = Some(ipc_layer::NonRtProducer::new(handle.topology_producer));

        // Setup MIDI Bridge SHM
        let shm_name = self.midi_shm_name.clone();
        if let Ok(shm) = ipc_layer::SharedMemory::create(&shm_name, 65536) {
            unsafe { ipc_layer::ShmRingBuffer::<nullherz_traits::MidiEvent>::init(shm.ptr(), 1024); }
            let rb = shm.ptr() as *const ipc_layer::ShmRingBuffer<nullherz_traits::MidiEvent>;

            let shm_arc = Arc::new(shm);
            self.midi_shm = Some(shm_arc.clone());
            self.external_midi_consumer = Some(ipc_layer::IpcMidiConsumer {
                buffer: shm_arc,
                rb,
            });
        }

        // Setup Remote Sidecar Listener (Stage 2 Distributed DSP).
        //
        // Gated on `network.listeners_enabled`, not merely on "is there a tokio
        // runtime". Every test that builds a Conductor runs inside one, so the
        // old condition started four fixed-port listeners in every test process
        // — including the blocking UDP receive that made the process unable to
        // exit. See `NetworkConfig`.
        if self.network.listeners_enabled && tokio::runtime::Handle::try_current().is_ok() {
            let net = self.network;
            let shutdown = self.shutdown.clone();

            let remote_manager = self.sidecar_supervisor.remote_manager.clone();
            let audio_bridge = self.audio_bridge.clone();
            let sd = shutdown.clone();
            tokio::spawn(async move {
                let addr = format!("0.0.0.0:{}", net.sidecar_port);
                let _ = crate::sidecar_supervisor::SidecarSupervisor::listen_for_remote_sidecars(remote_manager, audio_bridge, &addr, sd).await;
            });

            // Start UDP Discovery Beacon (Conductor identifying itself)
            let discovery = crate::discovery::DiscoveryBeacon::new(net.sidecar_port, "Conductor");
            discovery.start_broadcast();

            // Start UDP Discovery Listener (Conductor finding sidecars)
            let remote_manager = self.sidecar_supervisor.remote_manager.clone();
            let audio_bridge = self.audio_bridge.clone();
            let sd = shutdown.clone();
            tokio::spawn(async move {
                let _ = crate::sidecar_supervisor::SidecarSupervisor::start_discovery_listener(remote_manager, audio_bridge, net.discovery_port, sd).await;
            });

            // Start UDP Return Listener (Type 6)
            let audio_bridge = self.audio_bridge.clone();
            let sd = shutdown.clone();
            tokio::spawn(async move {
                let _ = crate::sidecar_supervisor::SidecarSupervisor::start_udp_return_listener(audio_bridge, net.audio_return_port, sd).await;
            });

            // Start Federated DNA Server (TCP pull)
            let lib = self.library.clone();
            let signing_key = self.sidecar_discovery.dna_discovery.lock().signing_key;
            let _ = nullherz_dna::DnaServer::start(lib, net.dna_port, signing_key, shutdown);
        }

        crate::EngineContext {
            command_producer: handle.command_producer,
            telemetry_consumer: handle.telemetry_consumer,
            midi_producer: handle.midi_producer,
        }
    }

    pub fn start_midi_bridge(&mut self, binary_path: &str, port_filter: Option<&str>) {
        if self.midi_child.is_some() { return; }
        let mut cmd = std::process::Command::new(binary_path);
        cmd.arg("--shm").arg(&self.midi_shm_name);
        if let Some(f) = port_filter { cmd.arg("--port").arg(f); }

        if let Ok(child) = cmd.spawn() {
            println!("MIDI Bridge process spawned (PID: {})", child.id());
            self.midi_child = Some(child);
        }
    }

    pub fn set_midi_consumer(&mut self, consumer: ipc_layer::Consumer<nullherz_traits::MidiEvent>) {
        self.midi_consumer = Some(consumer);
    }

    pub fn start_backend(&mut self, backend_type: nullherz_traits::AudioBackendType) -> Result<(), String> {
        Self::prepare_realtime_environment();
        let period = Self::effective_period_size(self.period_size);
        self.engine_coordinator.backend_manager.start(backend_type, period)
    }

    /// The period size to ask the device for, with `NULLHERZ_PERIOD_SIZE`
    /// applied.
    ///
    /// The saved session config decides this normally. The override exists
    /// because period size is the DOMINANT latency term and the only way to
    /// evaluate a change is to run one: at 256/48k the engine costs one block
    /// (5.33 ms) and the device ring three more, so the ladder down to 64 frames
    /// is worth about 16 ms — far more than the 3-to-2 period change
    /// `NULLHERZ_BUFFER_PERIODS` already exposes.
    ///
    /// It asks; ALSA answers. `snd_pcm_hw_params_set_period_size_near` will
    /// negotiate something else if the device cannot do it, and the backend logs
    /// what it actually got. A value the hardware refuses is not an error here.
    ///
    /// Clamped to `MAX_BLOCK_SIZE` for the same reason `BackendManager::start`
    /// clamps: the graph's buffers cannot hold a larger block.
    fn effective_period_size(configured: u64) -> u64 {
        match std::env::var("NULLHERZ_PERIOD_SIZE").ok().and_then(|v| v.parse::<u64>().ok()) {
            Some(v) if v > 0 => {
                let v = v.min(nullherz_traits::MAX_BLOCK_SIZE as u64);
                if v != configured {
                    println!("[audio] period size {configured} -> {v} (NULLHERZ_PERIOD_SIZE)");
                }
                v
            }
            _ => configured,
        }
    }

    /// Lock memory and report anything that will degrade realtime behaviour.
    ///
    /// Runs once, before the first audio thread exists — `mlockall` must precede
    /// the allocations it is meant to protect, and `MCL_FUTURE` then covers
    /// everything allocated afterwards (topology commits, session buffers).
    ///
    /// Nothing here is fatal. A machine that cannot meet the guarantees says so
    /// ONCE, at startup, instead of presenting as unexplained xruns hours into a
    /// set. Idempotent: repeated `start_backend` calls (backend switching) report
    /// only the first time.
    fn prepare_realtime_environment() {
        use std::sync::Once;
        static ONCE: Once = Once::new();
        ONCE.call_once(|| {
            // `mlockall(MCL_CURRENT | MCL_FUTURE)` returns success as soon as the
            // CURRENT footprint is locked, and then lets every LATER allocation
            // fail to lock once RLIMIT_MEMLOCK is reached — silently. So an `Ok`
            // here is not the claim the old message made ("audio pages cannot be
            // swapped out"); with an 8 MiB limit and a 35 MiB graph it was simply
            // untrue. Report the call and the limit separately and let the reader
            // draw the conclusion.
            match ipc_layer::lock_memory() {
                Ok(()) => match ipc_layer::memlock_limit() {
                    Some(l) if l == u64::MAX => {
                        println!("[RT] memory locked (mlockall), RLIMIT_MEMLOCK unlimited — audio pages are resident")
                    }
                    Some(l) => println!(
                        "[RT] mlockall succeeded, but RLIMIT_MEMLOCK is {} KiB — allocations past \
                         that stay swappable (see the warning below)",
                        l / 1024
                    ),
                    None => println!("[RT] mlockall succeeded; RLIMIT_MEMLOCK unreadable"),
                },
                Err(e) => eprintln!(
                    "[RT] WARNING: could not lock memory ({e}). Audio buffers stay \
                     swappable, so a block deadline can become a disk read."
                ),
            }
            // Scheduling comes first in this list — see
            // `realtime_environment_warnings`.
            for w in ipc_layer::realtime_environment_warnings() {
                eprintln!("[RT] WARNING: {w}");
            }
            println!(
                "[RT] realtime policy obtainable: {}",
                if ipc_layer::realtime_available() { "yes" } else { "NO" }
            );
        });
    }

    pub fn stop_backend(&mut self) {
        self.engine_coordinator.backend_manager.stop()
    }

    pub fn switch_backend(&mut self, backend_type: nullherz_traits::AudioBackendType) -> Result<(), String> {
        self.stop_backend();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let res = self.start_backend(backend_type);
        if res.is_ok() {
            let _ = self.update_system_config(Some(backend_type), None, None);
        }
        res
    }

    pub fn load_system_config(&mut self) -> std::io::Result<()> {
        let path = "system_config.json";
        if std::path::Path::new(path).exists() {
            let content = std::fs::read_to_string(path)?;
            if let Ok(config) = serde_json::from_str::<crate::persistence::SystemConfig>(&content) {
                self.calibration_samples = config.calibration_samples;
                self.period_size = config.period_size;
                // Captures are stamped with the device rate; a wrong value there
                // makes the sampler transpose them on playback.
                self.transfusion_manager.set_device_sample_rate(config.sample_rate);
            }
        }
        Ok(())
    }

    pub fn update_system_config(&mut self, backend_type: Option<nullherz_traits::AudioBackendType>, midi_ports: Option<Vec<String>>, calibration: Option<u32>) -> std::io::Result<()> {
        let path = "system_config.json";
        let mut config = if std::path::Path::new(path).exists() {
            let content = std::fs::read_to_string(path)?;
            serde_json::from_str::<crate::persistence::SystemConfig>(&content).unwrap_or(crate::persistence::SystemConfig {
                audio_backend: "Mock".to_string(),
                midi_ports: vec![],
                sample_rate: 44100,
                block_size: 256,
                calibration_samples: 0,
                period_size: 128,
            })
        } else {
            crate::persistence::SystemConfig {
                audio_backend: "Mock".to_string(),
                midi_ports: vec![],
                sample_rate: 44100,
                block_size: 256,
                calibration_samples: 0,
                period_size: 128,
            }
        };

        if let Some(bt) = backend_type {
            config.audio_backend = format!("{:?}", bt);
        }
        if let Some(ports) = midi_ports {
            config.midi_ports = ports;
        }
        if let Some(c) = calibration {
            config.calibration_samples = c;
            self.calibration_samples = c;
        }
        config.period_size = self.period_size;

        let json = serde_json::to_string_pretty(&config).map_err(|e| std::io::Error::other(e))?;
        std::fs::write(path, json)
    }

    pub fn drain_garbage(&mut self) {
        self.engine_coordinator.drain_garbage();
    }

    fn process_distributed_audio(&mut self) {
        let topo = &self.topology_manager.current_topology;
        for node_idx in 0..topo.node_count {
            let target_assignment = &topo.node_assignments[node_idx];
            if target_assignment.0[0] != 0 {
                let target = String::from_utf8_lossy(&target_assignment.0).trim_matches(char::from(0)).to_string();
                if target != "local" {
                    let mut blocks = Vec::with_capacity(4);
                    while let Some(block) = self.audio_bridge.pop_block(node_idx as u32) {
                        blocks.push(block);
                    }

                    if !blocks.is_empty() {
                        let remote_manager = self.sidecar_supervisor.remote_manager.clone();
                        let node_idx_u32 = node_idx as u32;
                        spawn_background(async move {
                            let mut manager = remote_manager.lock().await;
                            for block in blocks {
                                let _ = manager.send_audio_block(node_idx_u32, block).await;
                            }
                        });
                    }
                }
            }
        }
        self.audio_bridge.process_return_queues();
    }

    fn process_evolutionary_breeding(&mut self, now: u64) {
        // Auto-breeding is opt-in (default OFF): it used to generate a new child
        // into the library every 10 s, unbounded. Manual breeding from the DNA
        // Breeder screen (CommitBreeding) runs regardless of this flag.
        if !self.mixer_bridge.timeline.auto_breed_enabled { return; }
        if now % 10 == 0 && self.mixer_bridge.timeline.last_breeding_secs != now {
             self.mixer_bridge.timeline.last_breeding_secs = now;
             if let Some(ref breeder) = self.transfusion_manager.evolutionary_breeder {
                 breeder.run_breeding_cycle();
             }
        }
    }

    pub fn trigger_matchmaking_suggestions(&mut self) {
        self.update_matchmaking_suggestions(0);
    }

    fn update_matchmaking_suggestions(&mut self, now: u64) {
        self.mixer_bridge.timeline.last_matchmaking_secs = now;
        let lib = self.library.clone();
        let suggestions = self.matchmaking_suggestions.clone();

        // identify master track DNA using active_master_deck
        let master_sampler_id = self.mixer_manager.deck_mappings.get(&self.mixer_manager.active_master_deck).map(|d| d.sampler_id);

        if let Some(sampler_node_idx) = master_sampler_id {
            // Resolve the resource_id (sample_id) currently loaded in the master sampler
            let mut current_sample_id = None;
            {
                { let engine_lock = self.engine_coordinator.backend_manager.engine_handle.lock();
                    if let Some(ref engine) = *engine_lock {
                        current_sample_id = engine.list_children().iter()
                            .find(|c| c.metadata().map(|m| m.processor_id as u32) == Some(sampler_node_idx))
                            .and_then(|c| c.resource_id());
                    }
                }
            }

            if let Some(id) = current_sample_id {
                spawn_background(async move {
                    // FACETS: only `dna` is wanted here, and a full row would
                    // deserialize the master deck's whole waveform (61 ms for a
                    // 6-minute track) while holding the library mutex that the
                    // command path needs.
                    { let lib_lock = lib.lock();
                        if let Ok(Some(track)) = lib_lock.get_track_facets(id) {
                            if let Ok(matches) = nullherz_dna::Matchmaker::find_best_matches(&lib_lock, &track.dna, 3) {
                                { let mut sugg_lock = suggestions.lock();
                                    *sugg_lock = matches;
                                }
                            }
                        }
                    }
                });
            }
        }
    }

    pub fn update_timeline(&mut self, telemetry: &mut Telemetry) {
        crate::telemetry_service::TelemetryService::update_timeline(self, telemetry);
    }

    pub fn apply_mixer_commands(&mut self, commands: Vec<Command>) {
        let has_topology = commands.iter().any(|cmd| matches!(cmd, Command::Topology(_)));
        if has_topology {
            self.checkpoint();
        }
        crate::command_handler::CommandHandler::apply_mixer_commands(self, commands);
    }

    /// Builds the 4-channel DJ console on the conductor's OWN MixerManager so
    /// that `deck_mappings` is populated. Bootstrapping through a throwaway
    /// MixerManager leaves the conductor's map empty, which silently kills
    /// every Performance command that resolves a deck (LoadTrackToDeck,
    /// PlayDeck, SyncDecks, master-deck sampler resolution).
    pub fn bootstrap_4channel_mixer(&mut self) {
        let mut commands = self.mixer_manager.create_4channel_mixer();
        // The engine executes only what the compiled plan stages; without this
        // commit the first partial plan (bounded per-block mutation drain)
        // stays live forever and everything past it renders silence.
        commands.push(nullherz_traits::Command::Core(nullherz_traits::CoreCommand::CommitTopology));
        self.apply_mixer_commands(commands);
    }

    pub fn handle_midi_events(&mut self, events: Vec<nullherz_traits::MidiEvent>) {
        if let Some(ref mut prod) = self.midi_producer {
            for event in &events {
                let _ = prod.push(*event);
            }
        }

        for event in events {
            // Handle System Real-time (Clock, Start, Stop)
            if event.status >= 0xF8 {
                if let Some(new_bpm) = self.midi_clock.handle_event(event.status) {
                    self.mixer_bridge.timeline.bpm = new_bpm;
                    // Broadcast new BPM to engine
                    if let Some(ref prod) = self.engine_coordinator.command_producer {
                         let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                             timestamp_samples: 0,
                             command: nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetBpm(new_bpm)),
                         });
                    }
                }

                if event.status == 0xFA || event.status == 0xFB {
                     if let Some(ref prod) = self.engine_coordinator.command_producer {
                        let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                            timestamp_samples: 0,
                            command: nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Play),
                        });
                    }
                } else if event.status == 0xFC {
                    if let Some(ref prod) = self.engine_coordinator.command_producer {
                        let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                            timestamp_samples: 0,
                            command: nullherz_traits::Command::Core(nullherz_traits::CoreCommand::Stop),
                        });
                    }
                }
            }

            let mapped_commands = self.midi_mapper.translate(&event, &self.mixer_manager.node_names, self.focused_node_idx);
            if !mapped_commands.is_empty() {
                self.apply_mixer_commands(mapped_commands);
            }
        }
    }

    /// The ids analysis has finished with — the set `reap_registry` consults.
    ///
    /// Public because "has this been analysed yet?" is a real question about
    /// session state, and because a reaper whose pin cannot be observed is a
    /// reaper whose pin cannot be tested. The worker writes it from its own
    /// thread; readers see a consistent snapshot per lock.
    pub fn analysed_ids(&self) -> &Arc<parking_lot::Mutex<std::collections::HashSet<u64>>> {
        &self.analysed_ids
    }

    /// Whether the registry reaper is enabled. See [`Conductor::reap_registry`]
    /// for why the default is ON, and what had to be true first.
    pub fn registry_reap_enabled() -> bool {
        // Opt-OUT. See `reap_registry` for why this flipped.
        !matches!(
            std::env::var("NULLHERZ_REGISTRY_REAP").ok().as_deref(),
            Some("0") | Some("false") | Some("no")
        )
    }

    /// Release decoded audio the session is not using and can decode again.
    ///
    /// The library scanner registers the FULL decoded audio of every file it
    /// finds so the analysis worker can read it, and nothing ever released it:
    /// a 500-track library meant every track resident for the whole session,
    /// tens of gigabytes. Analysis needs the samples exactly once, and the
    /// deck-load path already decodes on demand when the registry misses, so
    /// holding them afterwards buys nothing.
    ///
    /// Two predicates decide, and both matter:
    ///
    /// **Not in use.** A sample on a deck stays. Evicting one would not free it
    /// anyway (the sampler holds its own `Arc`), and it would drop the entry
    /// that `sync_sampler_metadata` reads. Anything mid-hydration stays too, or
    /// the decode that is running right now would be discarded on arrival.
    ///
    /// **Recoverable.** Only evict what we can produce again — a library track
    /// whose file is still on disk. The registry also holds samples that exist
    /// NOWHERE else: transfusion children, captures, chopped edits. Those have
    /// no file to decode from, so evicting one destroys the user's work. This
    /// is the reason the predicate is "recoverable" and not merely "unused".
    ///
    /// Runs on the orchestration thread, so the `Arc` released here is dropped
    /// here — never on the audio thread.
    fn reap_registry(&mut self, now: u64) {
        // ON by default as of the analysis-signal fix below. Disable with
        // NULLHERZ_REGISTRY_REAP=0.
        //
        // This comment used to list three things the policy needed before it
        // could default on. They are addressed, in order:
        //
        //   1. "An 'analysis finished' signal. The conductor cannot currently
        //      see `AnalysisWorker::processed_ids`." — it can now, through
        //      `has_analysed`, and the sweep below pins anything the worker is
        //      not done with. That was the actual correctness race: evicting a
        //      track out of the analysis queue does not defer enrichment, it
        //      SKIPS it, because the worker finds its work by scanning the
        //      registry for ids it has not processed.
        //   2. "Trigger on memory pressure or an LRU threshold instead of a
        //      fixed 1 Hz sweep, so a quiet session does no work at all." — the
        //      sweep now returns immediately unless the registry has actually
        //      grown since the last one. A session that is not scanning does
        //      nothing beyond one length comparison per second.
        //   3. "Evidence from a full Gate 1 run, not a 2-minute smoke test." —
        //      `scripts/verify.sh` passes with this on.
        //
        // And the cost of leaving it off turned out not to be a slow session but
        // a CRASH. The scanner registers the FULL decoded audio of every file it
        // finds; a 957 MB folder of 15 WAVs decodes to roughly 2 GB of f32, and
        // the smoke run died on "memory allocation of 75497472 bytes failed"
        // before finishing a one-minute run — on an unmodified tree. A default
        // that cannot open the user's own library is not the conservative
        // choice it looks like.
        //
        // Explicitly NOT a reason, then or now: underruns. A 2-minute survival
        // run failed with 3 underruns both with this enabled and disabled, and
        // peak block time stayed at ~500 µs of a 5333 µs budget — those were the
        // development machine being busy. An early version of this comment
        // blamed the reaper; that was wrong.
        if !Self::registry_reap_enabled() { return; }

        // Sweeping the whole registry against the library is not free; once a
        // second is far more often than a scan can grow it.
        if self.last_registry_reap_secs == now { return; }
        self.last_registry_reap_secs = now;

        let ids = self.transfusion_manager.sample_registry.list_ids();
        if ids.is_empty() { return; }

        // Point 2 above: a quiet session does no work. The per-id library
        // lookups below are one database read each, and there is nothing to
        // find unless something changed.
        //
        // BOTH lengths, and the second one is not optional. Registry length
        // alone was the first version of this and it was wrong: a track becomes
        // reapable when ANALYSIS finishes with it, which does not change the
        // registry at all. With only the first check a track analysed after its
        // sweep was never revisited until some unrelated file was scanned —
        // caught by `test_reap_keeps_a_track_that_analysis_has_not_reached`,
        // which analyses one track and expects the next sweep to release it.
        //
        // Lengths rather than contents, deliberately: hashing both sets every
        // second to catch an add and a remove landing between two sweeps costs
        // more than the sweep it saves, and both sets only shrink through paths
        // that run here. An idle session changes neither and does nothing.
        let analysed_len = self.analysed_ids.lock().len();
        if ids.len() == self.last_registry_reap_len
            && analysed_len == self.last_analysed_len
        {
            return;
        }
        self.last_registry_reap_len = ids.len();
        self.last_analysed_len = analysed_len;

        let pinned: std::collections::HashSet<u64> = self
            .mixer_manager
            .deck_samples
            .values()
            .copied()
            .chain(self.hydration_pending.iter().copied())
            .collect();

        let mut evicted = 0usize;
        let mut freed_frames = 0usize;
        let mut awaiting_analysis = 0usize;
        for id in ids {
            if pinned.contains(&id) { continue; }

            // NOT YET ANALYSED — the race that kept this whole mechanism
            // switched off. The scanner registers a track's decoded audio
            // precisely as the hand-off to `AnalysisWorker`, which picks work up
            // by scanning the registry for ids it has not processed. Evicting
            // one out of that window does not merely defer analysis, it SKIPS
            // it: the id disappears from the registry, so the worker never sees
            // it and the track keeps whatever metadata it arrived with.
            //
            // Observed live as "Hydrated registry for X" immediately followed by
            // "released 1 sample, 121 MB".
            //
            // Asking the worker closes it. A sample is reapable only once
            // analysis is DONE with it, which is the same shape as the two
            // pins above: in use, or not finished with.
            if !self.analysed_ids.lock().contains(&id) {
                awaiting_analysis += 1;
                continue;
            }

            let recoverable = {
                let lib = self.library.lock();
                lib.get_track(id)
                    .ok()
                    .flatten()
                    .map(|t| !t.path.is_empty() && std::path::Path::new(&t.path).exists())
                    .unwrap_or(false)
            };
            if !recoverable { continue; }

            if let Some(sample) = self.transfusion_manager.sample_registry.remove(id) {
                freed_frames += sample.buffer.len();
                evicted += 1;
                // `sample` drops here, on the orchestration thread. If a
                // processor still holds a reference the buffer simply survives
                // until that one goes — the refcount decides, not this call.
            }
        }

        if awaiting_analysis > 0 {
            println!(
                "Registry reap: held {awaiting_analysis} sample(s) still queued for analysis"
            );
        }
        if evicted > 0 {
            self.transfusion_manager.sample_registry.drain_garbage();
            println!(
                "Registry reap: released {} sample(s), ~{:.1} MB of decoded audio (re-decoded on demand)",
                evicted,
                freed_frames as f64 * 4.0 / (1024.0 * 1024.0)
            );
        }
    }

    /// Re-scan audio devices at most every few seconds. See
    /// [`Conductor::cached_audio_devices`] for why this must never run per frame.
    fn refresh_audio_devices(&mut self) {
        const RESCAN: std::time::Duration = std::time::Duration::from_secs(5);
        let due = self.last_device_scan.map(|t| t.elapsed() >= RESCAN).unwrap_or(true);
        if !due { return; }
        self.last_device_scan = Some(std::time::Instant::now());
        if let Some(ref backend) = self.engine_coordinator.backend_manager.backend {
            self.cached_audio_devices = backend.enumerate_devices();
        }
    }

    /// Re-measure resident decoded audio at most twice a second.
    fn refresh_residency(&mut self) {
        const RESCAN: std::time::Duration = std::time::Duration::from_millis(500);
        let due = self.last_residency_scan.map(|t| t.elapsed() >= RESCAN).unwrap_or(true);
        if !due { return; }
        self.last_residency_scan = Some(std::time::Instant::now());
        let reg = &self.transfusion_manager.sample_registry;
        let ids = reg.list_ids();
        let mut bytes = 0u64;
        for id in &ids {
            if let Some(s) = reg.get(*id) {
                bytes += (s.buffer.len() * std::mem::size_of::<f32>()) as u64;
            }
        }
        self.cached_residency = (ids.len() as u32, bytes);
    }

    /// Resident sample count and audio bytes, from the cache.
    pub fn residency(&self) -> (u32, u64) { self.cached_residency }

    /// Device names for telemetry, from the cache.
    pub fn audio_device_names(&self) -> &[String] { &self.cached_audio_devices }

    /// Adopt the rate the audio device actually negotiated.
    ///
    /// The backend calls `set_config` on the engine once the device answers,
    /// which fixes the transport and re-runs `setup` on nodes already in the
    /// ACTIVE graph. It does not reach two other places, and both matter:
    ///
    ///  - `topology_manager.current_sample_rate` is what the factory passes to
    ///    every node it CONSTRUCTS. Left stale, every node added after the
    ///    device opened is built for the wrong rate — filter corners and
    ///    envelope times land in the wrong place, and `setup` never revisits
    ///    them because it already ran.
    ///  - `timeline.sample_rate` converts between samples and musical time.
    ///
    /// Cheap enough to run every tick (one lock, one compare) and self-healing:
    /// it also covers a device change mid-session.
    fn sync_session_rate(&mut self) {
        let device_rate = {
            let lock = self.engine_coordinator.backend_manager.engine_handle.lock();
            lock.as_ref().map(|e| e.target_sample_rate()).unwrap_or(0.0)
        };
        if device_rate <= 0.0 || device_rate == self.topology_manager.current_sample_rate {
            return;
        }
        self.topology_manager.current_sample_rate = device_rate;
        self.mixer_bridge.timeline.sample_rate = device_rate;
        self.transfusion_manager.set_device_sample_rate(device_rate as u32);
    }

    pub fn tick(&mut self) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);

        self.sync_session_rate();
        self.refresh_audio_devices();
        self.refresh_residency();
        self.reap_registry(now);

        // Complete background hydrations: the decode thread has registered
        // the sample; re-drive the load for every deck still mapped to it so
        // the engine's AddSourceFromRegistry (a no-op while the registry
        // missed) finally lands. A deck the user re-loaded meanwhile is
        // mapped to a different sample and is left alone.
        let hydrated: Vec<u64> = self.hydration_done_rx.try_iter().collect();
        for sample_id in hydrated {
            self.hydration_pending.remove(&sample_id);
            self.hydration_progress.lock().remove(&sample_id);
            let decks: Vec<char> = self
                .mixer_manager
                .deck_samples
                .iter()
                .filter(|&(_, &s)| s == sample_id)
                .map(|(&d, _)| d)
                .collect();
            for deck_id in decks {
                self.apply_mixer_commands(vec![nullherz_traits::Command::Performance(
                    nullherz_traits::PerformanceCommand::LoadTrackToDeck { deck_id, sample_id },
                )]);
            }
        }

        // 0. Handle Background Auto-Save (Every 60 seconds)
        if now % 60 == 0 && self.last_autosave_secs != now {
            self.last_autosave_secs = now;
            let state = self.capture_state();
            spawn_blocking_background(move || {
                let _ = state.save_to_file("autosave.json");
                let _ = state.save_to_rkyv("autosave.rkyv");
                println!("Conductor: Background Auto-Save complete.");
            });
        }

        // Drain Local MIDI Consumer
        if let Some(ref mut consumer) = self.midi_consumer {
            let mut events = Vec::new();
            while let Some(event) = consumer.pop() {
                events.push(event);
            }
            if !events.is_empty() {
                self.handle_midi_events(events);
            }
        }

        // Drain External MIDI Consumer (Sidecar Bridge)
        if let Some(ref mut consumer) = self.external_midi_consumer {
            let mut events = Vec::new();
            while let Some(event) = consumer.pop() {
                events.push(event);
            }
            if !events.is_empty() {
                self.handle_midi_events(events);
            }
        }

        // Update Pattern Orchestration
        let arrangement_commands = self.pattern_manager.tick(self.mixer_bridge.timeline.current_beat);
        if !arrangement_commands.is_empty() {
            self.apply_mixer_commands(arrangement_commands);
        }

        let clip_commands = self.clip_orchestrator.tick(self.mixer_bridge.timeline.current_beat);
        if !clip_commands.is_empty() {
            self.apply_mixer_commands(clip_commands);
        }

        if self.engine_coordinator.check_health() {
            eprintln!("CRITICAL: Engine health crisis detected. Prioritizing resource recovery...");
            self.drain_garbage();
        }

        let (mut new_processors, enter_safe_mode) = self.sidecar_supervisor.manager.supervise();
        if enter_safe_mode {
            eprintln!("Sidecar failure triggered Safe Mode!");
            if let Some(ref prod) = self.engine_coordinator.command_producer {
                let _ = prod.push_command(nullherz_traits::TimestampedCommand {
                    timestamp_samples: 0,
                    command: nullherz_traits::Command::Core(nullherz_traits::CoreCommand::SetSafeMode(true)),
                });
            }
        }

        for (node_idx, processor) in new_processors.drain(..) {
             eprintln!("Recovered sidecar process for node {}. Re-inserting into audio graph...", node_idx);
            if let Some(ref mut prod) = self.topology_manager.topo_producer {
                let _ = prod.push(nullherz_traits::TopologyMutation::SwapProcessor { node_idx, processor });
            }
        }

        let remote_commands = self.sidecar_supervisor.supervise(&mut self.topology_manager);
        for ts_cmd in remote_commands {
            if let Some(ref prod) = self.engine_coordinator.command_producer {
                let _ = prod.push_command(ts_cmd);
            }
        }

        self.process_distributed_audio();

        // Proactive Matchmaking Suggestions (Stage 6)
        if now % 15 == 0 && self.mixer_bridge.timeline.last_matchmaking_secs != now {
            self.update_matchmaking_suggestions(now);
        }

        self.process_evolutionary_breeding(now);
        self.tick_dna_transitions();

        // NOTE: the former auto-EvolvePattern demo block fired every 8 seconds
        // at hardcoded node 0 — silently mutating deck A's live sample and
        // breeding evolution children into the library. Evolution now runs
        // only on explicit EvolvePattern commands from the UI.

        self.handle_transfusion_registrations();

        self.sync_sampler_metadata();

        {
            self.transfusion_manager.sample_registry.drain_garbage();
        }

        self.drain_garbage();
    }

    fn handle_transfusion_registrations(&mut self) {
        { let engine_lock = self.engine_coordinator.backend_manager.engine_handle.lock();
            if let Some(ref engine) = *engine_lock {
                self.transfusion_manager.poll_snapshots(engine.as_ref());
            }
        }
    }

    fn sync_sampler_metadata(&mut self) {
        // Once per second is plenty for BPM/metadata reconciliation; every
        // 16ms tick flooded the topology ring alongside user commands.
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        if self.last_metadata_sync_secs == now { return; }
        self.last_metadata_sync_secs = now;

        // Snapshot (node, sample) pairs and RELEASE the engine lock before
        // resolving any metadata. The backend takes that same lock to render
        // every block, so anything slow held across it stalls the audio thread
        // outright — and this loop used to hold it while calling
        // `library.get_track()` per loaded deck. A library row is JSON with the
        // whole waveform inside it: 61 ms for a 6-minute track against 2.5 ms
        // for a 17-second one. Two full-length decks meant a ~120 ms audio
        // dropout every single second, which is why long tracks were unplayable
        // while the short demo WAVs sounded fine.
        let loaded: Vec<(u32, u64)> = {
            let engine_lock = self.engine_coordinator.backend_manager.engine_handle.lock();
            match *engine_lock {
                Some(ref engine) => engine
                    .list_children()
                    .iter()
                    .filter_map(|c| Some((c.metadata()?.processor_id as u32, c.resource_id()?)))
                    .collect(),
                None => Vec::new(),
            }
        };

        for (node_idx, id) in loaded {
            // REGISTRY FIRST, library only as a fallback. The registry is the
            // in-memory Arc every mutation path writes before it touches the
            // library (analysis enrichment, hot cues, stretch, chop), so it is
            // both fresher and free — no deserialization at all.
            let metadata = match self.transfusion_manager.sample_registry.get(id) {
                Some(sample) => Some(sample.metadata),
                None => {
                    let lib_lock = self.library.lock();
                    lib_lock.get_track(id).ok().flatten().map(|t| t.metadata)
                }
            };
            let Some(metadata) = metadata else { continue };

            // Nothing changed since the last push: the sampler already holds
            // this exact Arc, and re-pushing floods the topology ring alongside
            // the user's commands.
            if self.synced_node_metadata.get(&node_idx).is_some_and(|prev| Arc::ptr_eq(prev, &metadata)) {
                continue;
            }
            // Record the sync ONLY if the push landed. The topology ring is
            // bounded and drops on overflow; marking a dropped push as synced
            // would strand the node on stale metadata until the next real
            // change, where the old unconditional re-push self-healed.
            if let Some(ref mut prod) = self.topology_manager.topo_producer {
                let landed = prod.push(nullherz_traits::TopologyMutation::UpdateMetadata {
                    node_idx,
                    metadata: metadata.clone(),
                }).is_ok();
                if landed {
                    self.synced_node_metadata.insert(node_idx, metadata);
                }
            }
        }
    }

    pub fn capture_state(&self) -> crate::persistence::ProjectState {
        crate::persistence::ProjectState::capture(self)
    }

    pub fn checkpoint(&mut self) {
        let state = self.capture_state();
        let mut sample_history = std::collections::HashMap::new();
        let ids = self.transfusion_manager.sample_registry.list_ids();
        for id in ids {
            if let Some(sample) = self.transfusion_manager.sample_registry.get(id) {
                sample_history.insert(id, (sample.buffer.clone(), sample.metadata.clone()));
            }
        }
        self.undo_stack.push((state, sample_history));
        self.redo_stack.clear();
        while self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
    }

    pub fn checkpoint_parameter_edit(&mut self) {
        self.checkpoint();
    }

    pub fn undo(&mut self) -> bool {
        if self.undo_stack.is_empty() {
            return false;
        }
        let current_state = self.capture_state();
        let mut current_sample_history = std::collections::HashMap::new();
        let ids = self.transfusion_manager.sample_registry.list_ids();
        for id in ids {
            if let Some(sample) = self.transfusion_manager.sample_registry.get(id) {
                current_sample_history.insert(id, (sample.buffer.clone(), sample.metadata.clone()));
            }
        }
        self.redo_stack.push((current_state, current_sample_history));
        while self.redo_stack.len() > 50 {
            self.redo_stack.remove(0);
        }
        if let Some((popped_state, popped_history)) = self.undo_stack.pop() {
            for (id, (buffer, metadata)) in popped_history {
                self.transfusion_manager.sample_registry.register_with_metadata(id, buffer, metadata);
            }
            self.apply_state(popped_state);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if self.redo_stack.is_empty() {
            return false;
        }
        let current_state = self.capture_state();
        let mut current_sample_history = std::collections::HashMap::new();
        let ids = self.transfusion_manager.sample_registry.list_ids();
        for id in ids {
            if let Some(sample) = self.transfusion_manager.sample_registry.get(id) {
                current_sample_history.insert(id, (sample.buffer.clone(), sample.metadata.clone()));
            }
        }
        self.undo_stack.push((current_state, current_sample_history));
        while self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
        if let Some((popped_state, popped_history)) = self.redo_stack.pop() {
            for (id, (buffer, metadata)) in popped_history {
                self.transfusion_manager.sample_registry.register_with_metadata(id, buffer, metadata);
            }
            self.apply_state(popped_state);
            true
        } else {
            false
        }
    }

    pub fn save_project(&self, path: &str) -> std::io::Result<()> {
        let state = self.capture_state();
        // Standardized: Prioritize .rkyv for zero-copy performance unless JSON explicitly requested
        if path.ends_with(".json") {
            state.save_to_file(path)
        } else {
            let rkyv_path = if path.ends_with(".rkyv") { path.to_string() } else { format!("{}.rkyv", path) };
            state.save_to_rkyv(&rkyv_path)
        }
    }

    pub fn load_project(&mut self, path: &str) -> std::io::Result<()> {
        let state = if path.ends_with(".json") {
            crate::persistence::ProjectState::load_from_file(path)?
        } else {
            let rkyv_path = if path.ends_with(".rkyv") { path.to_string() } else { format!("{}.rkyv", path) };
            match crate::persistence::ProjectState::load_from_rkyv(&rkyv_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Warning: Failed to load .rkyv project ({}), discarding / trying JSON fallback: {}", rkyv_path, e);
                    // Discard the incompatible .rkyv file
                    let _ = std::fs::remove_file(&rkyv_path);

                    // Attempt fallback to corresponding .json if it exists
                    let json_path = rkyv_path.replace(".rkyv", ".json");
                    if std::path::Path::new(&json_path).exists() {
                        crate::persistence::ProjectState::load_from_file(&json_path)?
                    } else {
                        return Err(e);
                    }
                }
            }
        };
        self.apply_state(state);
        Ok(())
    }

    pub fn apply_state(&mut self, state: crate::persistence::ProjectState) {
        let _ = state.apply(self);
    }

    pub fn start_deck_transition(&mut self, source_deck: char, target_deck: char, duration_beats: f64) {
        let current_beat = self.mixer_bridge.timeline.current_beat;
        self.active_transitions.push(DnaTransition {
            source_deck,
            target_deck,
            start_beat: current_beat,
            duration_beats,
            is_complete: false,
        });
        println!("Conductor: Starting Semantic Transition: {} -> {} over {} beats", source_deck, target_deck, duration_beats);
    }

    fn tick_dna_transitions(&mut self) {
        let current_beat = self.mixer_bridge.timeline.current_beat;
        let mut commands = Vec::new();

        self.active_transitions.retain_mut(|t| {
            if t.is_complete { return false; }

            let progress = (current_beat - t.start_beat) / t.duration_beats;
            let progress = progress.clamp(0.0, 1.0) as f32;

            // Orchestrate DNA Morphing via DnaMorpher if available
            // For now, we apply slerp/morphing parameters to the respective deck processors
            if let Some(src_nodes) = self.mixer_manager.deck_mappings.get(&t.source_deck) {
                if let Some(dst_nodes) = self.mixer_manager.deck_mappings.get(&t.target_deck) {
                     // 1. Cross-fade volumes (Constant Power)
                     let gain_src = (1.0 - progress).sqrt();
                     let gain_dst = progress.sqrt();

                     commands.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                         target_id: src_nodes.gain_id as u64,
                         param_id: 0,
                         value: gain_src,
                         ramp_duration_samples: 1024,
                     }));
                     commands.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                         target_id: dst_nodes.gain_id as u64,
                         param_id: 0,
                         value: gain_dst,
                         ramp_duration_samples: 1024,
                     }));

                     // 2. DNA Morphing (Latent Space Slerp via DnaMorpher node if assigned)
                     commands.push(Command::Mixer(nullherz_traits::MixerCommand::SetParam {
                        target_id: dst_nodes.dna_slot_id as u64,
                        param_id: 0, // Morph Position
                        value: progress,
                        ramp_duration_samples: 0,
                     }));
                }
            }

            if progress >= 1.0 {
                t.is_complete = true;
                println!("Conductor: Transition {} -> {} complete.", t.source_deck, t.target_deck);
            }
            true
        });

        if !commands.is_empty() {
            self.apply_mixer_commands(commands);
        }
    }
}
