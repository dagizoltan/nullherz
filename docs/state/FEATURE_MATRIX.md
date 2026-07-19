# Nullherz System Feature Matrix (Stage 6: Evolutionary Intelligence)

**Current State:** Production Beta (see [SYSTEM_STATUS.md](./SYSTEM_STATUS.md) for qualifications)
**Last Updated:** July 19, 2026 — verified against code (reverse-engineering pass; see [ARCHITECTURE.md](../system/ARCHITECTURE.md))

---

## 1. Orchestration Plane (`nullherz-conductor`)

| Feature | Status | Description |
| :--- | :---: | :--- |
| **Declarative Topology** | ✅ | Kahn's algorithm for off-thread graph compilation and atomic commit. |
| **Node Removal**         | ✅ | Off-thread double-buffered node removal and dangling edge cleanup. |
| **Undo/Redo System**     | ✅ | Snapshot-based session undo/redo with parameter drag rate limiting. |
| **Lifecycle Management** | ✅ | Node lifecycle, sidecar supervisor, and graceful process teardown. |
| **Project Persistence** | ✅ | Bincode (.bin) and JSON serialization for full session recovery; `SystemConfig` now includes configurable `period_size` (serde-defaulted). |
| **Hardware Calibration** | ✅ | RTL measurement with dynamic sample-rate adjustment (10ms offset). |
| **Distributed Routing** | ✅ | Type 5/6 Protocol supporting batched remote audio send/return. |
| **Pattern Manager** | ✅ | `SongArrangement` scheduling of pattern events on the beat timeline. |
| **Clip Orchestrator** | ✅ | 8×8 clip grid with quantized launch, row transfusion, and active/starting-clip telemetry. |
| **Genetic Sequencer** | 🧪 | DNA-driven pattern evolution (`evolve_pattern`); heuristic kernel. |
| **Groove Transfusion** | ✅ | DNA micro-timing lands on live per-deck sequencer nodes and shifts step fire times (was silently dropped: sentinel targets 70–73 had no backing nodes and the sequencer ignored the params). |
| **Modulation Matrix** | ✅ | Macro → multi-target parameter broadcast with `TemporalShape` ramps. |
| **Master-Deck Suggestion**| ✅ | DNA suggestions bound to `active_master_deck` state (A–D). |
| **Offline Rendering** | ✅ | Safe bit-perfect WAV export with safe engine access. |
| **Library Analysis Pipeline** | ✅ | `folder_monitor` auto-scan + `analysis_worker` (BPM/key/transient extraction) feeding `library.redb`. |
| **Disk Streaming** | ✅ | `StreamingManager` double-buffered disk-to-SHM streaming, decoupled from orchestration tick. |

---

## 2. Protocol Plane (`ipc-layer`, `nullherz-traits`)

| Feature | Status | Description |
| :--- | :---: | :--- |
| **Lock-Free Command Bus** | ✅ | SPSC/MPSC RingBuffer for zero-allocation control passing. |
| **Broadcaster Telemetry** | ✅ | Multi-client WebSocket telemetry streaming via `nullherz-gateway` (:9001). |
| **SIMD Alignment** | ✅ | 64-byte alignment enforced for all `AudioBlock` and DSP kernels. |
| **Modular Hierarchy** | ✅ | Command sets split into Core/Mixer/Perf/Resource/Dna/Topology with decoupled translation logic. |
| **Zero-Allocation Serialization**| ✅ | `bincode::serialize_into` used on audio hot-paths for guest-host IPC. |
| **RT Thread Hardening** | ✅ | FTZ/DAZ flags, `SCHED_FIFO` priority, core pinning, cgroup memory limits (`ipc-layer`). |
| **Clock Sync (PTP)** | 🔶 | Typed SYNC/DELAY_REQ/DELAY_RESP protocol on UDP :319; measured round-trip (offset-free, 1/8-EMA, ≤100ms plausibility clamp) + PI servo (Kani-proved clamp). SO_TIMESTAMPING and BMC election pending. |
| **rkyv Integration** | 💤 | Proposed for zero-copy project persistence. |

---

## 3. Execution Plane (`audio-core`, `audio-dsp`)

| Feature | Status | Description |
| :--- | :---: | :--- |
| **Static Dispatch VM** | ✅ | Kernel devirtualization (`AudioEngine<K: ProcessingKernel>`) for zero-overhead graph execution. |
| **Sample-Accurate Commands** | ✅ | Sub-block splitting at command timestamps with same-timestamp batch draining (`processing_kernel.rs`). |
| **RT-Safe Sample Registry**| ✅ | Atomic-swap registry for lock-free sample/source access. |
| **SIMD Kernel Foundation** | ✅ | AVX-512/NEON optimized `FloatX16` and core math primitives. |
| **4x Unrolled Scalar Kernels**| ✅ | Optimized fallback paths for non-SIMD processors (e.g., `DjIsolator`). |
| **Exact Filter Math** | ✅ | Runtime Linkwitz-Riley coefficient generation for exact crossovers. |
| **Soft Fallback** | ✅ | Heartbeat-monitored instant swap to bypass node upon DSP failure; escalation to global Safe Mode. |
| **Spectral Processor** | ✅ | Hardened FFT overlap-add with exact COLA-normalized synthesis window. Supports variable block sizes up to 1024 samples. |
| **KeySync Pitch Shift** | ✅ | Real phase-vocoder pitch shifter with per-bin phase tracking (level preserved on shift). |
| **Planar Stereo Playback** | ✅ | Planar sample buffers end to end; frame-counted playhead; per-plane crop/stretch; deck strips stereo at every hop with per-channel DSP state (vocoder lanes, kernel banks) and private L/R buffers (`MAX_BUFFERS = 128` edge address space). Channel identity covered by full-chain regression tests. |
| **OLA Time-Stretch** | ✅ | Overlap-add `time_stretch` kernel with corrected ratio semantics (`audio-dsp/util.rs`). |
| **Transient Detection** | ✅ | Spectral-flux + RMS onset/transient detectors powering editor chop and analysis. |
| **Parallel Graph Execution** | ✅ | Static stage assignment `TaskPool`; safety covered by a Kani proof harness (`kani-verify` feature). |

---

## 4. Intelligence & DNA Plane (`nullherz-dna`)

| Feature | Status | Description |
| :--- | :---: | :--- |
| **SoundDNA Schema (V6)** | ✅ | 16D Latent Space, Feature Vectors, and Rhythmic/Spatial profiles. |
| **Signed Lineages** | ✅ | ed25519 `SignedSoundDna` with signature + lineage/authorship-chain verification. |
| **Neural Transfusion** | ✅ | SIMD-optimized latent space interpolation via `NeuralTransfuser`. |
| **Chaotic Transfusion** | ✅ | Logistic Map mutation logic for non-linear trait inheritance. |
| **Smart Crates** | ✅ | Genetic similarity and range-based trait filtering (redb backend). |
| **P2P Discovery** | ✅ | mDNS-style genetic cloud discovery and synchronization. |
| **Gossip Protocol** | 🔶 | TCP Gossipsub-style overlay (GRAFT mesh, `GOSSIP_SIGNED` ed25519 payloads) with TOFU peer-identity pinning and key-change rejection. `libp2p` migration planned. |

---

## 5. User Interface (`nullherz-inspector`, `nullherz-ui-hal`)

| Feature | Status | Description |
| :--- | :---: | :--- |
| **Industrial UI Primitives** | ✅ | Agnostic widget logic for knobs, faders, and VU meters. |
| **Geometry Caching** | ✅ | `egui::Shape` caching for high-performance static UI components. |
| **Asymmetrical Ballistics** | ✅ | High-precision meter smoothing (Fast Attack, Quadratic Decay). |
| **GPU Waveform Rendering** | ✅ | WGPU-based renderer with precise MIP/LOD selection. |
| **Liquid Visual Damping** | ✅ | Asymmetrical inertia for Spectrum and Goniometer visualizers. |
| **DJ Console Layout** | 🔶 | 4-deck layout functional (mixer/waveform/transport/performance/DNA sub-views). Ergonomics in progress. |
| **Composer / Sequencer Grid** | ✅ | Endless-scroll step grid with per-track sequencer routing dropdown and live per-step playback telemetry (tested to slot 512). |
| **Audio Editor** | 🔶 | Waveform selection, OLA time-stretch, transient chop, non-destructive undo. Refinement ongoing. |
| **In-Process Conductor** | ✅ | Inspector runs the Conductor in-process with live command routing and telemetry. |
| **DNA Breeding UI** | 🔶 | 2D Transfusion Pad with genetic similarity-ranked matchmaking. |
| **Interactive Topology** | 🧪 | Cable visualization active (heuristic). Migration/Edit in progress. |
| **Offline Mock Fallbacks** | 🧪 | Network/MIDI/Genetic-Cloud views present clearly-labeled mock data when no devices/peers are detected. |

---

## 6. Extensibility (`fx-runtime`, `sidecar-sdk`)

| Feature | Status | Description |
| :--- | :---: | :--- |
| **WASM Sidecar Host** | ✅ | `wasmtime` integration with RT-hardened host functions and fuel/epoch resource limiter. |
| **Sidecar SDK V2** | ✅ | Triple-Plane isolation SDK. Hardened sub-sample rhythmic jitter implemented. |
| **Sidecar Macros** | ✅ | Hardened attribute macros with structured argument parsing. |
| **Universal Extensibility** | ✅ | Lock-free IPC bridge for both local subprocess and WASM guests. |
| **Sidecar Resource Limits** | ✅ | Hierarchical cgroups with real RSS memory limits (SC-4). |
| **wasm_simd128 Paths** | ✅ | Implemented in `audio-dsp` (`simd_vec.rs` cfg paths, `complex_mul_accumulate_wasm_simd`). |
| **SHM Zero-Copy for WASM Guests** | 🧪 | Host exports integrated; true zero-copy memory mapping still a placeholder (`wasm_runtime.rs`). |

---

## 7. Audio Backends (`nullherz-backends`)

| Backend | Status | Description |
| :--- | :---: | :--- |
| **ALSA** | ✅ | Default backend; configurable period size. |
| **PipeWire** | 🔶 | Implemented; less battle-tested than ALSA. |
| **JACK** | ✅ | Pro-audio server integration. |
| **Threaded** | ✅ | Software-clocked fallback backend (also the automatic boot fallback). |
| **Mock** | ✅ | Deterministic backend for tests/CI. |

---

**Legend:**
- ✅ **Hardened**: Verified, RT-safe, and stable.
- 🔶 **Active**: Implemented and functional, undergoing refinement.
- 🧪 **Prototype**: Functional implementation with simplified or heuristic logic.
- 💤 **Planned**: Defined in roadmap, implementation pending.
