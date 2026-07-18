# Nullherz System Architecture Reference

**Source of truth:** reverse-engineered from the workspace code on 2026-07-17.
**Scope:** every crate and sidecar in the workspace, the runtime data flow, wire protocols, and on-disk state.

This document describes *what is actually in the tree*, as opposed to the strategy and status documents which describe intent and maturity. When this document and the code disagree, the code wins — please update this file in the same PR.

---

## 1. Workspace Map

~31,000 lines of Rust across 19 crates and 8 sidecar binaries, organized by the Triple-Plane Isolation Model (see [AGENTS.md](../../AGENTS.md)).

### 1.1 Execution Plane (the RT hot path)

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `audio-core` | ~4.2k | `AudioEngine<K: ProcessingKernel>` (statically dispatched), `ProcessorGraph` VM, sample-accurate command scheduling (`engine/processing_kernel.rs`), parallel stage execution (`processors/graph/pool.rs`), buffer pool with PDC lines, RT logging, resource recycler, telemetry finalizer. Contains Kani proof harness in `processors/graph/verification.rs`. |
| `audio-dsp` | ~3.3k | SIMD math foundation: `FloatX16` vector abstraction with AVX-512 / wasm-simd128 / scalar fallback paths (`simd_vec.rs`), biquad & Linkwitz-Riley filters, oscillators, spectral kernels (FFT overlap-add, `complex_mul_accumulate_wasm_simd`), and the editor DSP toolbox in `util.rs`: OLA `time_stretch`, spectral-flux transient/onset detection, spectral envelope extraction, waveform MIP-level generation, polyphase up/downsamplers, Newton solver, n-dimensional slerp. |
| `nullherz-processors` | ~4.9k | The processor library: 22 registered factories (Gain, Biquad, SimdBiquad, Sampler, StreamingSampler, Crossfader, Summing, Spectral, SpectralMorph, Wavetable, Modulation, Sequencer, EnvelopeFollower, Granular, Capture, DjIsolator, KeySync, PersonalityInheritance, DnaMorph, Limiter, Compressor, StereoUtility, Analysis) plus the `FallbackProcessor` (bypass) and the sidecar proxy processor. Conformance `test_kit` included. |

### 1.2 Protocol Plane (shared schemas & lock-free transport)

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `nullherz-traits` | ~2.8k | The ABI of the system. Command hierarchy (`CoreCommand`, `MixerCommand`, `PerformanceCommand`, `ResourceCommand`, `DnaCommand`, `TopologyCommand` wrapped in `TimestampedCommand`), `SignalProcessor`/`AudioProcessor` traits, `Transport`, `CompiledGraphPlan`, `GraphTopology`/`TopologyMutation`, `ModulationMatrix` with `TemporalShape` ramps, `SubBlockIterator`, RT-thread marking (`mark_as_rt_thread`/`run_rt_safe`), clock providers (incl. `PtpClockProvider` with hardware RX timestamps), telemetry schema, and a Kani harness for the PI clock-servo integral clamp. |
| `ipc-layer` | ~1.0k | Lock-free transport: SPSC/MPSC ring buffers, shared-memory (`shm_open`) ring buffers with `EventFd` signaling, `ShmSignal` heartbeats, TCP framing (`tcp.rs`), RT priority + FTZ/DAZ setup (`setup_rt_thread`), thread pinning, cgroup helpers (`move_to_cgroup`, `set_cgroup_memory_limit`), stale-segment cleanup. Defines `MAX_BLOCK_SIZE` and the 64-byte-aligned `AudioBlock`. |

### 1.3 Orchestration Plane

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `nullherz-conductor` | ~5.8k | The daemon (`main.rs` binary + library). Subsystems: `orchestrator` (tick loop), `topology_manager` (off-thread Kahn compilation → `SetTopology` O(1) swap), `command_handler`, `engine_coordinator`, `sidecar_supervisor` (heartbeat → soft fallback → safe mode), `pattern_manager` (song arrangements), `clip_orchestrator` (8×8 clip grid with launch quantization + telemetry), `genetic_sequencer` (DNA-driven pattern evolution), `modulation_matrix`, `mixer_bridge`/`mixer_orchestrator`, `timeline`, `midi_clock`/`midi_mapper`/`midi_sequence_kernel`, `analysis_worker` + `analysis_kernel` (BPM/key/transient extraction), `folder_monitor` (library watch), `streaming_manager` (double-buffered disk streaming), `transfusion_manager`, `ptp_engine` (UDP clock sync), `discovery` (UDP beacon + plugin dir watcher), `persistence` (`SystemConfig`, `ProjectState` bincode/JSON), `bounce` (offline WAV render), `ipc_audio_bridge` (jitter buffer, Kani-proved). |
| `nullherz-topology` | ~0.8k | Declarative graph reconciliation: diffs desired vs. actual `GraphTopology` into minimal `TopologyMutation` batches; `compiler.rs` produces `CompiledGraphPlan` stages. |
| `nullherz-mixer` | ~0.6k | Console builders: `create_4channel_mixer`, `create_dj_deck` (A–D logical decks), `create_studio_strip`, `create_aux_bus`, `create_crossfader`, plus topology validation. Emits command batches; owns no DSP. |
| `control-plane` | ~0.1k | Thin utility layer (largely superseded by conductor; minimal code). |
| `nullherz-setup` | ~0.1k | Setup binary (config bootstrap). |

### 1.4 Extensibility & Runtime Hosting

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `fx-runtime` | ~0.7k | Sidecar process host: spawns subprocess plugins, wires SHM rings + eventfds, applies RT priority, moves children into a hierarchical `nullherz` cgroup with real RSS memory limits (SC-4), and hosts WASM guests via `wasmtime` (`wasm_runtime.rs`) with a fuel/epoch `resource_limiter`. |
| `sidecar-sdk` | ~0.5k | Guest-side SDK: `SidecarHost` main-loop that connects SHM, implements Sidecar Protocol V2 framing, and drives a user-supplied `AudioProcessor`. |
| `sidecar-macros` | ~0.1k | Attribute macros for declaring sidecar processors/params. |

### 1.5 Intelligence / DNA Plane

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `nullherz-dna` | ~1.7k | `SoundDNA` schema (16-D latent space, rhythmic/spatial profiles), ed25519-signed lineages (`SignedSoundDna`, `verify_signature`/`verify_lineage`), `LibraryDatabase` on `redb` with Smart-Crate trait filtering, `SampleRegistry` (atomic-swap, lock-free reader), `GeneticLibrary`, and `CloudPeerSync` — a TCP gossip overlay with Gossipsub-style mesh links (GRAFT/GOSSIP_PUB/GOSSIP_SIGNED) and mDNS-style discovery. |

### 1.6 UI Plane

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `nullherz-inspector` | ~6.2k | `egui`/`eframe` desktop app. Views: DJ Studio (mixer, waveform, transport, performance, DNA), Composer (endless-scroll step grid with sequencer routing and per-step playback telemetry), Audio Editor (waveform selection, OLA time-stretch, transient chop, non-destructive undo), Sampler, Library, Breeder (2-D transfusion pad), Genetic Cloud, Topology, Mastering, Player, Broadcast, Metrics, Account, Modulation, Notifications, Settings (audio/MIDI/network/calibration/preferences). Runs an in-process Conductor and consumes live telemetry. |
| `nullherz-ui-hal` | ~1.0k | Backend-agnostic widget/render layer: knobs, faders, VU meters with asymmetric ballistics, WGPU waveform renderer with MIP/LOD selection. |
| `nullherz-gateway` | ~0.2k | WebSocket bridge (default `127.0.0.1:9001`): broadcasts JSON telemetry to any number of clients (non-blocking broadcaster pattern), accepts JSON `TimestampedCommand`s and library queries. |
| `nullherz-bench` | ~0.2k | Criterion benchmarks. |
| `nullherz-backends` | ~1.0k | Audio I/O drivers: **ALSA, PipeWire, JACK, Threaded (software clock), Mock** — hot-swappable at runtime via `AudioBackendType`. |

### 1.7 Sidecars (out-of-process / guest DSP)

| Sidecar | Purpose |
| :--- | :--- |
| `nullherz-midi` | Bridges hardware MIDI (via `midir`, feature-gated) into an SHM ring (`--shm <name>`). |
| `nullherz-broadcast` | Streams engine audio out (broadcast/recording sink) over an async runtime. |
| `nullherz-sampler` | Standalone 16-voice sample player sidecar. |
| `distributed-sidecar` | Remote DSP node: listens on `:9002`, discovers the conductor, receives Type 5 audio sends, returns Type 6 UDP blocks; holds its own `SampleRegistry` mirror. |
| `spectral-transfuser` | Spectral morph/transfusion DSP as a sidecar. |
| `reference_dsp` | Minimal reference effect using `sidecar-sdk`. |
| `nullherz-dummy` | Pass-through processor for conformance/failover testing. |
| `nullherz-template` | Copy-me starting point for third-party sidecar authors. |

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

- **Sample-accurate commands**: `StandardKernel::execute` splits each period into sub-blocks at command timestamps, drains same-timestamp batches, and carries over future-dated commands (`pending_command`) — bounded by `MAX_COMMANDS_PER_BLOCK`.
- **O(1) topology swap**: the RT thread only ever executes `TopologyMutation::SetTopology` as an `Arc` pointer swap; Kahn's algorithm runs in `TopologyManager` off-thread. Regression-tested (`test_rt_topology_commit_is_no_op`).
- **Failure containment**: `SidecarSupervisor` watches SHM heartbeats; a stall (>200 ms) swaps in `FallbackProcessor` at the failed node's `node_idx`, and repeated failure can trigger global Safe Mode via the command bus.
- **Boot sequence** (`conductor/main.rs`): load `system_config.json` → resolve backend (fallback: Threaded) → start engine → spawn WebSocket gateway → bootstrap 4-channel DJ mixer topology → start MIDI sidecar bridge.

---

## 3. Wire Protocols & External Interfaces

| Interface | Transport | Notes |
| :--- | :--- | :--- |
| Sidecar Protocol V2 | SHM rings + eventfd; TCP/UDP for remote | Message types 1–8 (commands, sample mirroring, audio return, heartbeat, remote send, UDP return, experimental RDMA, MIDI fast-path). See [SIDECAR_PROTOCOL_V2.md](./SIDECAR_PROTOCOL_V2.md). |
| Gateway | WebSocket, `127.0.0.1:9001` | JSON telemetry broadcast (fan-out to N clients), JSON command ingest, library queries. |
| Clock sync | UDP broadcast `:319` | Master broadcasts a `u64` LE nanosecond timestamp at 1 Hz; slaves discipline a PI clock servo. **Simplified PTP**: fixed 1 ms wire-delay assumption, no Delay_Req/Delay_Resp exchange yet. Hardware RX timestamps used when `PtpClockProvider` is active. |
| Discovery | UDP broadcast beacon | `DiscoveryBeacon` announces conductor presence; `distributed-sidecar` listens; `SidecarDiscoveryService` also watches `plugins/` for drop-in manifests. |
| DNA gossip | TCP overlay | Gossipsub-style GRAFT/mesh links; `GOSSIP_SIGNED` payloads verified with ed25519. |

---

## 4. On-Disk State

| Path | Owner | Contents |
| :--- | :--- | :--- |
| `system_config.json` | conductor/setup | Backend choice, sample rate, block size, calibration offset, `period_size` (serde-defaulted). |
| `library.redb` | `nullherz-dna` | Track library, DNA metadata, smart crates. |
| `graph.json` | inspector/conductor | Serialized topology snapshot. |
| `mappings/default.json` | `midi_mapper` | MIDI CC → command mappings. |
| `plugins/` | discovery service | Drop-in sidecar manifests (e.g. `bitcrusher.json` + binary dir). |
| `tracks/` | demo assets | `track_a.wav`, `track_b.wav`. |
| `fallback_<pid>_<n>.redb` | test/fallback runs | **Transient artifacts** — safe to delete; should be gitignored/relocated (tracked in [TECHNICAL_DEBT_AND_STUBS.md](../state/TECHNICAL_DEBT_AND_STUBS.md)). |

---

## 5. Verification Infrastructure

- **121 `#[test]` functions** across the workspace; conformance `Gauntlet` (`nullherz-traits/src/test_kit`) runs every registered processor through NaN ingestion, buffer-size oscillation, sub-block consistency, reset determinism, parameter reachability, and snapshot safety.
- **Kani proof harnesses** (3, behind the `kani-verify` feature): PI clock-servo integral clamping (`nullherz-traits`), jitter-buffer size invariance (`conductor/ipc_audio_bridge.rs`), and parallel graph-execution safety (`audio-core/processors/graph/verification.rs`).
- **Warning-free**: `cargo check --workspace` completes with zero warnings (verified 2026-07-17).
- Integration/decoupling test suites live in `audio-core` (`integration_tests.rs`, `decoupling_tests.rs`, `engine_tests.rs`) and `conductor` (`mixing_test.rs`).
