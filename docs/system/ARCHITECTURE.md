# Nullherz System Architecture Reference

**Source of truth:** reverse-engineered from the workspace code on 2026-07-20; refreshed 2026-07-21; evaluated and hardened 2026-07-28 (16-tap sinc resampler, -107.1 dB THD+N signal transparency, 7.33 ms RAW latency, f64 playhead tracking, RT zero-alloc counting allocator, warning-free workspace compilation); audited and verified 2026-09-22 (19 workspace crates, 23 sidecars, 418+ green tests).
**Scope:** every crate and sidecar in the workspace, the runtime data flow, wire protocols, and on-disk state.

This document describes *what is actually in the tree*, as opposed to the strategy and status documents which describe intent and maturity. When this document and the code disagree, the code wins — please update this file in the same PR.

---

## 1. Workspace Map

~60,000 lines of Rust (tests included) across 19 crates and 23 sidecar binaries, organized by the Triple-Plane Isolation Model (see [AGENTS.md](../../AGENTS.md)).

### 1.1 Execution Plane (the RT hot path)

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `audio-core` | ~4.4k | `AudioEngine<K: ProcessingKernel>` (statically dispatched), `ProcessorGraph` VM, sample-accurate command scheduling (`engine/processing_kernel.rs`), parallel stage execution (`processors/graph/pool.rs`), buffer pool with PDC lines (`MAX_BUFFERS` audio blocks + crossfade blocks), RT logging, resource recycler, telemetry finalizer. `processors/graph/verification.rs` holds the executor's block-geometry proptests (see §5). |
| `audio-dsp` | ~3.7k | SIMD math foundation: `FloatX16` vector abstraction with AVX-512 / wasm-simd128 / scalar fallback paths (`simd_vec.rs`), biquad & Linkwitz-Riley filters plus RBJ shelf/peaking constructors and the 3-band `MasteringEq` master tone stage (low shelf 120 Hz / mid peak 1 kHz / high shelf 8 kHz in series; every band at unity gain is the bit-exact identity biquad, coefficient changes ramp), oscillators (incl. the planar `SamplerVoice` — see §2.1), spectral kernels (FFT overlap-add with exact COLA-normalized synthesis window, `complex_mul_accumulate_wasm_simd`), and the editor DSP toolbox in `util.rs`: OLA `time_stretch`, spectral-flux transient/onset detection, spectral envelope extraction, waveform MIP-level generation, polyphase up/downsamplers, Newton solver, n-dimensional slerp. Real-time zero-allocation neural DSP insert specifications (TCN, SSM, HyperNetwork parametric EQs, Padé activations) are detailed in [`NEURAL_DSP_INSERT_SPECIFICATION.md`](./NEURAL_DSP_INSERT_SPECIFICATION.md). Embedded ARM `#![no_std]` compilation is supported with `--no-default-features`. |
| `nullherz-processors` | ~5.7k | The processor library: **36 registered factories** (Gain, Biquad, SimdBiquad, Sampler, StreamingSampler, Crossfader, Summing, Spectral, SpectralMorph, Wavetable, Modulation, Sequencer, EnvelopeFollower, Granular, Capture, DjIsolator, MasteringEq, KeySync — a real phase-vocoder pitch shifter with per-bin phase tracking, PersonalityInheritance, DnaMorph, Limiter, Compressor, StereoUtility, Analysis, Delay, NeuralSaturator, NeuralFilter, NeuralTcn, NeuralSsm, NeuralNam, HyperNetworkEq, TubePreamp, MultiBandCompressor, Reverb, ModulationFx) plus the `FallbackProcessor` (bypass) and the sidecar proxy processor. Registration is not reachability: **14 of the 36 are never instantiated by the bootstrapped console**, each declared with a reason in `known_unreachable()` (`conductor/tests/reachability_gate_test.rs`). Conformance `test_kit` and golden-hash render regression tests included. |

### 1.2 Protocol Plane (shared schemas & lock-free transport)

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `nullherz-traits` | ~3.1k | The ABI of the system. Command hierarchy (`CoreCommand`, `MixerCommand`, `PerformanceCommand`, `ResourceCommand`, `DnaCommand`, `TopologyCommand` wrapped in `TimestampedCommand`), `SignalProcessor`/`AudioProcessor` traits, `Transport`, `CompiledGraphPlan`, `GraphTopology`/`TopologyMutation`, `ModulationMatrix` with `TemporalShape` ramps, `SubBlockIterator`, RT-thread marking (`mark_as_rt_thread`/`run_rt_safe`), clock providers (incl. `PtpClockProvider` with hardware RX timestamps), telemetry schema, PI clock-servo anti-windup proptests; `test_kit::rt_alloc`, the counting allocator behind the RT zero-allocation tests. `SampleMetadata` carries the waveform display data: proportional peaks (one per 128 samples), `MipWaveform` pyramids, and `BandWaveform` — per-window low/mid/high band peaks plus the signed min/max envelope for frequency-colored rendering (serde-defaulted for pre-band libraries). Home of the sizing constants (`execution.rs`): `MAX_BLOCK_SIZE=1024`, `MAX_NODES=128`, `MAX_BUFFERS=240`, `MAX_CHANNELS=16`, `MAX_CROSSFADE_BUFFERS=8`, and the 64-byte-aligned `AudioBlock` (re-exported by `ipc-layer`). |
| `ipc-layer` | ~1.2k | Lock-free transport: SPSC/MPSC ring buffers, shared-memory (`shm_open`) ring buffers with `EventFd` signaling, `ShmSignal` heartbeats, TCP framing (`tcp.rs`), RT priority + FTZ/DAZ setup (`setup_rt_thread`), thread pinning, cgroup helpers (`move_to_cgroup`, `set_cgroup_memory_limit`), Linux HugePages allocations (`MAP_HUGETLB`, `MAP_HUGE_2MB`, `MAP_HUGE_1GB`) when `NULLHERZ_HUGEPAGES` is set, stale-segment cleanup. `SchedStatus`/`realtime_available` (what the audio thread's scheduling policy ACTUALLY is, read back from the kernel), ring-buffer and `ShmSignal` tests. Explicit 64-bit casting for `rlim_cur` guarantees cross-platform safety on 32-bit ARM Linux targets. |

### 1.3 Orchestration Plane

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `nullherz-conductor` | ~8.0k | The daemon (`main.rs` binary + library). Subsystems: `orchestrator` (tick loop), `topology_manager` (off-thread Kahn compilation → `SetTopology` O(1) swap), `command_handler`, `engine_coordinator`, `sidecar_supervisor` (heartbeat → soft fallback → safe mode), `pattern_manager` (song arrangements), `clip_orchestrator` (8×8 clip grid with launch quantization + telemetry), `genetic_sequencer` (DNA-driven pattern evolution), `modulation_matrix`, `mixer_bridge`/`mixer_orchestrator`, `timeline`, `midi_clock`/`midi_mapper`/`midi_sequence_kernel`, `analysis_worker` + `analysis_kernel` (BPM/key/transient extraction), `folder_monitor` (library watch), `streaming_manager` (double-buffered disk streaming), `transfusion_manager`, `ptp_engine` (UDP clock sync), `discovery` (UDP beacon + plugin dir watcher), `persistence` (`SystemConfig`, `ProjectState` bincode/JSON), `bounce` (offline WAV render), `ipc_audio_bridge` (jitter buffer). |
| `nullherz-topology` | ~0.8k | Declarative graph reconciliation: diffs desired vs. actual `GraphTopology` into minimal `TopologyMutation` batches; `compiler.rs` produces `CompiledGraphPlan` stages, computes PDC path latencies, and re-verifies the plan hazard-free (`verify_no_hazards`, backed by unit tests and proptests). Hazard checking covers sidechains as well as inputs, and `verify_stage_ids_in_range` rejects any plan carrying a node id the executor could not index. Includes automated graph builders (`GraphPresetBuilder`) for 4-Deck DJ and Ableton-like composition topologies. |
| `nullherz-mixer` | ~0.6k | Console builders: `create_4channel_mixer`, `create_dj_deck` (A–D logical decks), `create_studio_strip`, `create_aux_bus`, `create_crossfader`, plus topology validation. Each deck strip is Sampler → DnaMorph → KeySync → Gain → Biquad → StereoUtility → *(insert fx)* → DjIsolator, **stereo at every hop** (`link_stereo` in `dj.rs`), ending in private per-deck L/R buffers plus stereo cue-bus sends; each deck also owns a live SEQUENCER node (`deck_x_sequencer`, trigger generator for DNA groove micro-timing). The master chain sums per side (`master_sum_l`/`master_sum_r`) with the preview sampler mixed in as a summing input, then runs stereo through `master_eq` into `master_limiter` and out to master L/R buffers. Dynamic `DeckNodes` resolution (`MixerManager::get_deck_nodes`) falls back cleanly after project restores from `autosave.json`. Emits command batches; owns no DSP. |
| `control-plane` | ~0.1k | Thin utility layer (largely superseded by conductor; minimal code). |
| `nullherz-setup` | ~0.1k | Setup binary (config bootstrap). |

### 1.4 Extensibility & Runtime Hosting

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `fx-runtime` | ~0.7k | Sidecar process host: spawns subprocess plugins, wires SHM rings + eventfds, applies RT priority, moves children into a hierarchical `nullherz` cgroup with real RSS memory limits, and hosts WASM guests via `wasmtime` (`wasm_runtime.rs`) with a fuel/epoch `resource_limiter` operating zero-copy directly on linear memory slices. |
| `sidecar-sdk` | ~0.5k | Guest-side SDK: `SidecarHost` main-loop that connects SHM, implements Sidecar Protocol V2 framing, drives a user-supplied `AudioProcessor`, and provides `SidecarStore` metadata management with multi-tag filtering (`filter_by_tags`). |
| `sidecar-macros` | ~0.1k | Attribute macros for declaring sidecar processors/params. |

### 1.5 Intelligence / DNA Plane & Transformation Engine

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `nullherz-dna` | ~1.8k | `SoundDNA` schema (16-D latent space, rhythmic/spatial profiles), ed25519-signed lineages (`SignedSoundDna`, `verify_signature`/`verify_lineage`), `LibraryDatabase` on `redb` with tag-aware `NHZTRK02` rkyv serialization and JSON fallback, `SampleRegistry` (atomic-swap, lock-free reader), `GeneticLibrary`, `CloudPeerSync` — a TCP gossip overlay with explicit GRAFT/PRUNE mesh management (GOSSIP_PUB/GOSSIP_SIGNED) and ed25519 payload signatures, the **Musical DNA Architecture Specification** ([`MUSICAL_DNA_ARCHITECTURE_SPECIFICATION.md`](./MUSICAL_DNA_ARCHITECTURE_SPECIFICATION.md)), the **DNS & DNA Glossary** ([`DNS_DNA_GLOSSARY.md`](./DNS_DNA_GLOSSARY.md)), the **Musical Transformation Engine** specification ([`MUSICAL_TRANSFORMATION_ENGINE_SPECIFICATION.md`](./MUSICAL_TRANSFORMATION_ENGINE_SPECIFICATION.md)), the **DNA Instrumentation Specification** ([`DNA_INSTRUMENTATION_AND_TOOLING.md`](./DNA_INSTRUMENTATION_AND_TOOLING.md)), and the **Sound Analysis / Perception System Specification** ([`SOUND_ANALYSIS_PERCEPTION_SYSTEM_SPECIFICATION.md`](./SOUND_ANALYSIS_PERCEPTION_SYSTEM_SPECIFICATION.md)). |

### 1.6 UI Plane

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `nullherz-inspector` | ~6.4k | `egui`/`eframe` desktop app. Views: DJ Studio (mixer, waveform, transport, performance, DNA), Composer (endless-scroll step grid with sequencer routing, vector mini-waveform previews, pattern length selector, drag velocity editing, and per-step playback telemetry), Audio Editor (waveform selection, OLA time-stretch, transient chop, non-destructive undo), Sampler, Library, Breeder (4-parent multi-donor pad with Expected Offspring Result canvas and live genre centroids), Genetic Cloud, Topology, Visuals (Visual Mixer strips with 64-neuron Spiking Neural Networks, per-pixel warp feedback, YAML organism profiles, and detached OS windows), Mastering, Player, Broadcast, Metrics, Account, Modulation, Notifications, Settings (audio/MIDI/network/calibration/preferences). Opens in fullscreen by default with header toggle controls. |
| `nullherz-ui-hal` | ~1.0k | Backend-agnostic widget/render layer: knobs, faders, VU meters with asymmetric ballistics, WGPU waveform renderer: filled TriangleStrip body, per-vertex frequency-band color, asymmetric signed envelope, MIP/LOD selection with stride-downsampling. |
| `nullherz-gateway` | ~0.2k | WebSocket bridge (default `127.0.0.1:9001`): broadcasts JSON telemetry to any number of clients (non-blocking broadcaster pattern), accepts JSON `TimestampedCommand`s and library queries. |
| `nullherz-bench` | ~0.2k | A `main()` stress harness (100k redb inserts + matchmaker ranking). |
| `nullherz-backends` | ~1.1k | Audio I/O drivers: **ALSA, PipeWire, JACK, Threaded (software clock), Mock** — hot-swappable at runtime via `AudioBackendType`. ALSA driver features direct hardware MMAP mode (`SND_PCM_ACCESS_MMAP_INTERLEAVED`), `NO_PERIOD_WAKEUP` kernel bypass, immediate software start threshold, 2-period hardware double-buffering, and D-Bus device reservation (`org.freedesktop.ReserveDevice1`). |

### 1.7 Sidecars (out-of-process / guest DSP & visuals)

| Sidecar | Category | Purpose |
| :--- | :--- | :--- |
| `nullherz-midi` | Infrastructure | Bridges hardware MIDI (`midir`) into an SHM ring; translates CCs via mapping profiles (`mappings/`). |
| `nullherz-broadcast` | Infrastructure | Streams engine audio out over an async runtime. |
| `nullherz-sampler` | Instrument | Standalone 16-voice sample player sidecar. |
| `distributed-sidecar` | Network | Remote DSP node: listens on `:9002`, discovers conductor, receives Type 5 sends, returns Type 6 UDP blocks. |
| `spectral-transfuser` | DSP Insert | Spectral morph/transfusion DSP as a sidecar. |
| `reference_dsp` | DSP Insert | Minimal reference effect using `sidecar-sdk`. |
| `nullherz-dummy` | Testing | Pass-through processor for conformance/failover testing. |
| `nullherz-template` | Template | Starting blueprint for third-party sidecar authors. |
| `neural-saturation` | Neural Insert | Padé SIMD rational neural soft-clipping saturator. |
| `neural-filter` | Neural Insert | Hypernetwork dynamic filter with SIMD non-linearities. |
| `algorithmic-delay` | Audio Insert | Fractional Hermite delay line insert. |
| `algorithmic-eq` | Audio Insert | Multi-mode State-Variable Filter (SVF) insert. |
| `algorithmic-synth` | Audio Instrument | Dual-oscillator MIDI synthesizer instrument sidecar. |
| `neural-visuals` | Visual Insert | Real-time audio & telemetry driven neural visual surface. |
| `neural-latent-manifold` | Visual Generator | 2D/3D perceptual manifold trajectory projection sidecar. |
| `phase-goniometer-2d` | Visual Insert | Real-time Lissajous stereo phase goniometer visual sidecar. |
| `fft-spectrum-mesh` | Visual Insert | Real-time FFT frequency spectrum 3D mesh visual sidecar. |
| `reaction-diffusion-nn` | Visual Generator | Turing pattern reaction-diffusion neural visual synthesis. |
| `shader-particle-swarm` | Visual Generator | Audio-reactive 100k GPU particle swarm shader visualizer. |
| `bioluminescent-fluid-flow` | Visual Generator | Bioluminescent bio-fluid dynamics surface sidecar. |
| `harmonic-arrangement-lattice` | Visual Insert | Structural harmonic chroma lattice visual sidecar. |
| `abstract-quantum-swarm` | Visual Generator | Multi-spectral quantum particle swarm visualizer. |
| `neural-floral-mycelium` | Visual Generator | Organic growing floral mycelium tendril visual surface. |

---

## 2. Runtime Data Flow

```
                      ┌─────────────────────────────┐
                      │  nullherz-inspector (egui)  │
                      │  (in-process Conductor)     │
                      └──────────┬──────────────────┘
                                 │ commands / telemetry (in-proc rings)
   WebSocket :9001               ▼
 clients ◄──────────► ┌─────────────────────────────┐        UDP :319 (clock sync)
 (JSON telemetry      │     nullherz-conductor      │◄──────────────► peers
  + commands)         │  orchestrator tick loop     │        UDP beacon (discovery)
                      └───┬──────────┬──────────┬───┘
        off-thread Kahn   │          │          │  SHM rings + eventfd
        compile → O(1)    │          │          ▼
        SetTopology swap  │          │   ┌───────────────┐   TCP/UDP Type 5/6
                          ▼          │   │   sidecars    │◄────► distributed-sidecar :9002
              ┌─────────────────┐    │   │ (subprocess / │
              │   AudioEngine   │    │   │  WASM guests) │
              │ ProcessorGraph  │    │   └───────────────┘
              │  (RT thread,    │    ▼
              │  FTZ/DAZ, SCHED │  redb library.redb ◄── analysis_worker / folder_monitor
              │  _FIFO, pinned) │
              └───────┬─────────┘
                      ▼
      nullherz-backends: ALSA │ PipeWire │ JACK │ Threaded │ Mock
```

Key invariants observed in code:

- **Node vs. buffer address spaces**: nodes and audio buffers (graph edges) live in *separate* index spaces — `MAX_NODES = 128` processors, `MAX_BUFFERS = 240` virtual buffer slots (`nullherz-traits/src/execution.rs`). The `IdAllocator` hands out node IDs and buffer IDs from the two spaces independently. Buffer ids are carried as the **`BufferId` newtype** (serde-transparent u32).
- **Planar sample buffers**: decoded samples are stored planar — channel *c* occupies `buffer[c*frames .. (c+1)*frames]` — and `SampleMetadata.total_samples` means frames *per channel*.
- **Targeted commands**: `PerformanceCommand`s are BROADCAST to every node by the engine's command dispatch; every processor arm matches its own address (`node_idx == self.id`, `target_id == self.id`).
- **DJ transport semantics**: `StopNode` PAUSES (voices deactivate, position and buffer held); `PlayNode` resumes a paused voice of the CURRENT source; `AddSource` clears every voice.
- **RAW by default**: `LoadTrackToDeck` emits the source plus the deck's tempo mode. Tempo-matching and harmonic pitch shift are per-deck SYNC/KEY latches, OFF by default.
- **Async track hydration**: `LoadTrackToDeck` never decodes on the tick thread. Decode runs on a named background thread (`hydrate-<id>`).
- **Signal transparency at unity**: **THD+N is 0.00044% (−107.1 dB)** against an analyser floor of **−134.7 dB**.
- **Resampler quality**: 16-tap windowed sinc bandlimited resampler (`audio-dsp/src/resample.rs`) achieves **0.0023% THD+N (−92.8 dB)** at 10 kHz (+2.5% rate) and **−90.9 dB alias suppression** at rate 2.0. Rate 1.0 is bit-exact identity.
- **Action-to-sound latency**: RAW action-to-sound latency is **7.33 ms** (256 frames @ 48 kHz: 256 block + 96 limiter lookahead + 1 onset) or **3.33 ms** (64 frames @ 48 kHz).
- **Embedded ARM & Baremetal**: Cross-compilation support for 32-bit ARM Linux targets (`armv7-unknown-linux-gnueabihf`) such as Akai MPC Live 1 with `#![no_std]` DSP support, explicit `u64` rlimit conversions, and baremetal core-isolation scripts (`scripts/baremetal_core_isolate.sh`).

---

## 3. Wire Protocols & External Interfaces

| Interface | Transport | Notes |
| :--- | :--- | :--- |
| Sidecar Protocol V2 | SHM rings + eventfd; TCP/UDP for remote | Message types 1–8 (commands, sample mirroring, audio return, heartbeat, remote send, UDP return, experimental RDMA, MIDI fast-path). |
| Gateway | WebSocket, `127.0.0.1:9001` | JSON telemetry broadcast, JSON command ingest, library queries. |
| Clock sync | UDP `:319` | Typed protocol: SYNC broadcast at 1 Hz, DELAY_REQ/DELAY_RESP. Slaves calculate offset-free round-trip delay and discipline a PI clock servo. |
| Discovery | UDP broadcast beacon | `DiscoveryBeacon` announces conductor presence; `distributed-sidecar` listens. |
| DNA gossip | TCP overlay | Gossipsub-style GRAFT/PRUNE mesh links; ed25519 `GOSSIP_SIGNED` payloads. |

---

## 4. On-Disk State

| Path | Owner | Contents |
| :--- | :--- | :--- |
| `system_config.json` | conductor/setup | Backend choice, sample rate, block size, calibration offset, `period_size`. |
| `library.redb` | `nullherz-dna` | Track library, DNA metadata, smart crates, tags. Gitignored runtime state. |
| `library/tracks/`, `samples/`, `sequences/` | library storage | Categorized audio files, samples, and sequences with auto-tagged path indexing. |
| `graph.json` | inspector/conductor | Serialized topology snapshot. |
| `mappings/*.json` | `midi_mapper` | Controller mapping profiles (Pioneer DDJ-400/FLX4, Traktor S2, Akai MPK Mini, Launchkey, etc.). |
| `assets/organism_profiles/` | inspector | Generative visual organism YAML presets (`mycelial_bloom`, `fluid_geometry`, `geometric_swarm`...). |
| `plugins/` | discovery service | Drop-in sidecar manifests. |

---

## 5. Verification Infrastructure

- **418 `#[test]` functions** passing across the workspace (`cargo test --workspace` exit 0).
- **Golden stereo master render** (`conductor/tests/golden_master_render_test.rs`) and **Golden-hash render regression** (`nullherz-processors/src/golden_render_tests.rs`).
- **RT zero-allocation guard** (`nullherz_traits::test_kit::rt_alloc`): counting global allocator verifying zero steady-state heap allocations across processors and engine graph block cycles.
- **Formal Verification Harness** (`crates/ipc-layer/tests/ring_verification_test.rs`): Kani (`#[cfg(kani)]`) and Loom (`#[cfg(loom)]`) proof modules covering lock-free SPSC/MPSC ring buffers under high contention.
- **Warning-free build**: `cargo check --workspace --all-targets` completes with zero warnings under `RUSTFLAGS="-D warnings"`.
- **Pre-commit Gate**: `scripts/verify.sh` runs check, debug tests, release tests (for timing budgets), and smoke test run under strict wall-clock step timeouts.
