# Nullherz System Feature Matrix (Stage 6: Evolutionary Intelligence)

**Current State:** see [IMPLEMENTATION_ROADMAP_2026_07.md](../roadmap/IMPLEMENTATION_ROADMAP_2026_07.md) for current phase progress. A ✅ below means **a user can reach it in the application**, verified by `crates/nullherz-conductor/tests/reachability_gate_test.rs`.
**Last Updated:** July 2026 — verified against code and hardware benchmark suites (see [ARCHITECTURE.md](../system/ARCHITECTURE.md) and [REVERSE_ENGINEERING_EVALUATION.md](./REVERSE_ENGINEERING_EVALUATION.md)).

---

## 1. Orchestration Plane (`nullherz-conductor`)

| Feature | Status | Description |
| :--- | :---: | :--- |
| **Declarative Topology** | ✅ | Kahn's algorithm for off-thread graph compilation and $O(1)$ atomic `SetTopology` commit. |
| **Node Removal** | ✅ | Off-thread double-buffered node removal and dangling edge cleanup. |
| **Undo/Redo System** | ✅ | Non-destructive snapshot-based session undo/redo with in-memory sample buffer tracking. |
| **Lifecycle Management** | ✅ | Node lifecycle, sidecar supervisor, and graceful process teardown. |
| **Project Persistence** | ✅ | Bincode (.bin) and JSON serialization for full session recovery; `SystemConfig` configurable `period_size`. |
| **Hardware Calibration** | ✅ | RTL measurement with dynamic sample-rate adjustment. |
| **Distributed Routing** | ✅ | Type 5/6 Protocol supporting batched remote audio send/return over LAN (`distributed-sidecar`). |
| **Pattern Manager** | ✅ | `SongArrangement` scheduling of pattern events on the beat timeline. |
| **Clip Orchestrator** | ✅ | 8×8 clip grid with quantized launch, row transfusion, and active/starting-clip telemetry. |
| **Genetic Sequencer** | ✅ | DNA-driven pattern evolution (`evolve_pattern`) driving live step sequencer micro-timing. |
| **Hot Cue Persistence** | ✅ | SetHotCue persists to registry + library + live node; cue set at deck playhead; numbered markers on waveform. |
| **Groove Transfusion** | ✅ | DNA micro-timing lands on live per-deck sequencer nodes and shifts step fire times. Rides per-deck SYNC latch. |
| **RAW Mode** | ✅ | Native tempo and pitch default. Tempo-match and harmonic shift are per-deck SYNC/KEY latches, off by default. |
| **Modulation Matrix** | ✅ | Macro → multi-target parameter broadcast with `TemporalShape` ramps and $W \cdot x + b$ control bus. |
| **Master-Deck Suggestion**| ✅ | DNA suggestions bound to `active_master_deck` state (A–D). |
| **Offline Rendering** | ✅ | Safe bit-perfect WAV export with safe engine access and TaskPool parallel acceleration (-7% latency). |
| **Library Analysis Pipeline** | ✅ | `folder_monitor` auto-scan + `analysis_worker` (BPM/key/transient/DNA extraction) feeding `library.redb`. |
| **Disk Streaming** | ✅ | `StreamingManager` double-buffered disk-to-SHM streaming, decoupled from orchestration tick. |

---

## 2. Protocol Plane (`ipc-layer`, `nullherz-traits`)

| Feature | Status | Description |
| :--- | :---: | :--- |
| **Lock-Free Command Bus** | ✅ | SPSC/MPSC RingBuffer for zero-allocation control passing. |
| **Broadcaster Telemetry** | ✅ | Multi-client WebSocket telemetry streaming via `nullherz-gateway` (:9001). |
| **SIMD Cache Alignment** | ✅ | 64-byte alignment enforced for all `AudioBlock` structs and SIMD DSP kernels. |
| **Modular Hierarchy** | ✅ | Command sets split into Core/Mixer/Perf/Resource/Dna/Topology with decoupled translation logic. |
| **Zero-Alloc Hot-Path** | ✅ | Enforced via `nullherz_traits::test_kit::rt_alloc` counting allocator. 0 allocations in steady state. |
| **RT Thread Hardening** | ✅ | FTZ/DAZ flags, `SCHED_FIFO` priority, core pinning, `mlockall` page-locking, cgroup memory limits. |
| **Clock Sync (PTP)** | ✅ | 4-timestamp path-delay cancellation protocol on UDP :319 with 1/8-EMA and Kani-proven PI ClockServo anti-windup. |
| **rkyv Integration** | ✅ | High-speed zero-copy rkyv serialization for library track facets and tags (`NHZTRK02`). |

---

## 3. Execution Plane (`audio-core`, `audio-dsp`, `nullherz-processors`)

| Feature | Status | Description |
| :--- | :---: | :--- |
| **Static Dispatch VM** | ✅ | Monomorphized devirtualized kernel (`AudioEngine<K: ProcessingKernel>`) for zero-overhead graph execution. |
| **Sample-Accurate Commands** | ✅ | Sub-block splitting at command timestamps with same-timestamp batch draining (`processing_kernel.rs`). |
| **RT-Safe Sample Registry**| ✅ | Atomic-swap registry for lock-free sample/source access. |
| **SIMD Kernel Foundation** | ✅ | AVX-512/NEON/WASM-SIMD128 optimized `FloatX16` and Padé activation approximants (`tanh`, GELU). |
| **Signal Transparency** | ✅ | Bit-exact identity at unity; THD+N 0.00044% (-107.1 dB) against -134.7 dB analyser floor. |
| **16-Tap Sinc Resampler** | ✅ | High-fidelity 16-tap windowed sinc resampler: THD+N 0.0023% (-92.8 dB @ 10kHz), -90.9 dB alias suppression. |
| **64-bit f64 Playhead** | ✅ | 64-bit float playhead tracking in `SamplerVoice`, eliminating the 25.4-minute f32 playback freeze. |
| **Exact Filter Math** | ✅ | Runtime Linkwitz-Riley coefficient generation for exact crossovers; bounded -0.115 dB isolator re-sum error. |
| **Soft Fallback & Recovery** | ✅ | Heartbeat-monitored instant swap to bypass node upon DSP failure; escalation to global Safe Mode. |
| **Spectral Processor** | ✅ | Hardened FFT overlap-add with exact COLA-normalized synthesis window (up to 1024 frames). |
| **KeySync Pitch Shift** | ✅ | Phase-vocoder pitch shifter with per-bin phase tracking and zero-latency bypass slotting. |
| **Colored Waveforms** | ✅ | Per-window 3-band peaks + signed envelope (BandWaveform); filled per-vertex-colored GPU rendering. |
| **DJ Transport Semantics** | ✅ | Stop pauses (position held), play resumes in place, load clears voices; playhead reports held position. |
| **Planar Stereo Playback** | ✅ | Planar sample buffers end to end; frame-counted playhead; per-plane crop/stretch; deck strips stereo at every hop. |
| **OLA Time-Stretch** | ✅ | Overlap-add `time_stretch` kernel with corrected ratio semantics (`audio-dsp/util.rs`). |
| **Transient Detection** | ✅ | Spectral-flux + RMS onset/transient detectors powering editor chop and analysis. |
| **Parallel Graph Execution** | ✅ | Static stage assignment `TaskPool` with cost-gated threshold calibration. |

---

## 4. Intelligence & DNA Plane (`nullherz-dna`)

| Feature | Status | Description |
| :--- | :---: | :--- |
| **SoundDNA Schema (V6)** | ✅ | 16D Latent Space, Feature Vectors, and Rhythmic/Spatial profiles. |
| **Signed Lineages** | ✅ | ed25519 `SignedSoundDna` with signature + lineage/authorship-chain verification. |
| **Neural Transfusion** | ✅ | SIMD-optimized latent space interpolation via `NeuralTransfuser`. |
| **Chaotic Transfusion** | ✅ | Logistic Map mutation logic for non-linear trait inheritance. |
| **Smart Crates** | ✅ | Genetic similarity and range-based trait filtering on `redb` database. |
| **P2P Discovery & Gossip**| ✅ | TCP Gossipsub-style overlay (GRAFT mesh, `GOSSIP_SIGNED` ed25519 payloads) with TOFU peer-identity pinning. |

---

## 5. User Interface (`nullherz-inspector`, `nullherz-ui-hal`)

| Feature | Status | Description |
| :--- | :---: | :--- |
| **Industrial UI Primitives** | ✅ | Agnostic widget logic for knobs, faders, and VU meters with asymmetric ballistics. |
| **GPU Waveform Rendering** | ✅ | WGPU-based renderer with frequency-colored vertex shading and precise MIP/LOD selection. |
| **System Mixer View** | ✅ | Standardized 140px channel strips, vertical waveforms, input selector, Master strip, and global transport. |
| **DJ Console View** | ✅ | 4-deck full-height scrolling waveforms stack with needle-view auto-scrolling around playhead. |
| **Composer / Sequencer Grid** | ✅ | Endless-scroll step grid with per-track sequencer routing, pattern length selector, and step velocity editing. |
| **Audio Editor** | ✅ | Waveform selection, OLA time-stretch, transient chop, non-destructive sample undo stack. |
| **Analyzer View** | ✅ | 11-layer composable Spectral Field Canvas, A/B comparison toggling, 2D/3D perceptual trajectories. |
| **DNA Breeder UI** | ✅ | 4-parent multi-donor breeder slots (Carrier, Modulator, Texture, Groove) with Expected Offspring previews. |
| **Visual Mixer View** | ✅ | Modular visual channels with 64-neuron Spiking Neural Networks, per-pixel warp feedback, detached windows. |
| **Sidecar Store View** | ✅ | Sidebar/tab view rendering interactive tag filters (`insert`, `instrument`, `neural`, `delay`, `visual`, `eq`). |

---

## 6. Extensibility (`fx-runtime`, `sidecar-sdk`)

| Feature | Status | Description |
| :--- | :---: | :--- |
| **WASM Sidecar Host** | ✅ | `wasmtime` integration with RT-hardened host functions and fuel/epoch resource limiter (`wasm` feature). |
| **Sidecar SDK V2** | ✅ | Triple-Plane isolation SDK. Zero-copy SHM guest execution. |
| **Universal Extensibility** | ✅ | Lock-free IPC bridge for both local subprocess and WASM guests. |
| **Sidecar Resource Limits** | ✅ | Hierarchical cgroups with real RSS memory limits. |
| **wasm_simd128 Paths** | ✅ | Implemented in `audio-dsp` (`simd_vec.rs` cfg paths, `complex_mul_accumulate_wasm_simd`). |

---

## 7. Audio Backends (`nullherz-backends`)

| Backend | Status | Description |
| :--- | :---: | :--- |
| **ALSA** | ✅ | Default direct hardware backend; configurable period and buffer sizes. |
| **PipeWire** | ✅ | PipeWire graph integration with native ALSA/JACK interop. |
| **JACK** | ✅ | Professional pro-audio server integration. |
| **Threaded** | ✅ | Software-clocked fallback backend for desktop/headless execution. |
| **Mock** | ✅ | Deterministic zero-I/O backend for automated test suites and CI. |

---

**Legend:**
- ✅ **Hardened**: Fully implemented, reachability-verified, RT-safe, and green in CI.
- 🔶 **Active**: Functional implementation undergoing active refinement.
- 🧪 **Prototype**: Research prototype or experimental hardware spec.
