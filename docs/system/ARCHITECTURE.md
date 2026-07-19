# Nullherz System Architecture Reference

**Source of truth:** reverse-engineered from the workspace code on 2026-07-19.
**Scope:** every crate and sidecar in the workspace, the runtime data flow, wire protocols, and on-disk state.

This document describes *what is actually in the tree*, as opposed to the strategy and status documents which describe intent and maturity. When this document and the code disagree, the code wins — please update this file in the same PR.

---

## 1. Workspace Map

~41,000 lines of Rust (tests included) across 19 crates and 8 sidecar binaries, organized by the Triple-Plane Isolation Model (see [AGENTS.md](../../AGENTS.md)).

### 1.1 Execution Plane (the RT hot path)

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `audio-core` | ~4.4k | `AudioEngine<K: ProcessingKernel>` (statically dispatched), `ProcessorGraph` VM, sample-accurate command scheduling (`engine/processing_kernel.rs`), parallel stage execution (`processors/graph/pool.rs`), buffer pool with PDC lines (`MAX_BUFFERS` audio blocks + crossfade blocks), RT logging, resource recycler, telemetry finalizer. Contains Kani proof harnesses in `processors/graph/verification.rs`. |
| `audio-dsp` | ~3.7k | SIMD math foundation: `FloatX16` vector abstraction with AVX-512 / wasm-simd128 / scalar fallback paths (`simd_vec.rs`), biquad & Linkwitz-Riley filters, oscillators (incl. the planar `SamplerVoice` — see §2.1), spectral kernels (FFT overlap-add with exact COLA-normalized synthesis window, `complex_mul_accumulate_wasm_simd`), and the editor DSP toolbox in `util.rs`: OLA `time_stretch`, spectral-flux transient/onset detection, spectral envelope extraction, waveform MIP-level generation, polyphase up/downsamplers, Newton solver, n-dimensional slerp. |
| `nullherz-processors` | ~5.7k | The processor library: 23 registered factories (Gain, Biquad, SimdBiquad, Sampler, StreamingSampler, Crossfader, Summing, Spectral, SpectralMorph, Wavetable, Modulation, Sequencer, EnvelopeFollower, Granular, Capture, DjIsolator, KeySync — a real phase-vocoder pitch shifter with per-bin phase tracking, PersonalityInheritance, DnaMorph, Limiter, Compressor, StereoUtility, Analysis) plus the `FallbackProcessor` (bypass) and the sidecar proxy processor. Conformance `test_kit` and golden-hash render regression tests included. |

### 1.2 Protocol Plane (shared schemas & lock-free transport)

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `nullherz-traits` | ~3.1k | The ABI of the system. Command hierarchy (`CoreCommand`, `MixerCommand`, `PerformanceCommand`, `ResourceCommand`, `DnaCommand`, `TopologyCommand` wrapped in `TimestampedCommand`), `SignalProcessor`/`AudioProcessor` traits, `Transport`, `CompiledGraphPlan`, `GraphTopology`/`TopologyMutation`, `ModulationMatrix` with `TemporalShape` ramps, `SubBlockIterator`, RT-thread marking (`mark_as_rt_thread`/`run_rt_safe`), clock providers (incl. `PtpClockProvider` with hardware RX timestamps), telemetry schema, and a Kani harness for the PI clock-servo integral clamp. Home of the sizing constants (`execution.rs`): `MAX_BLOCK_SIZE=256`, `MAX_NODES=64`, `MAX_BUFFERS=128`, `MAX_CHANNELS=16`, `MAX_CROSSFADE_BUFFERS=8`, and the 64-byte-aligned `AudioBlock` (re-exported by `ipc-layer`). |
| `ipc-layer` | ~1.2k | Lock-free transport: SPSC/MPSC ring buffers, shared-memory (`shm_open`) ring buffers with `EventFd` signaling, `ShmSignal` heartbeats, TCP framing (`tcp.rs`), RT priority + FTZ/DAZ setup (`setup_rt_thread`), thread pinning, cgroup helpers (`move_to_cgroup`, `set_cgroup_memory_limit`), stale-segment cleanup. Kani harnesses for SHM/MPSC ring safety and `ShmSignal` atomic ordering. |

### 1.3 Orchestration Plane

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `nullherz-conductor` | ~8.0k | The daemon (`main.rs` binary + library). Subsystems: `orchestrator` (tick loop), `topology_manager` (off-thread Kahn compilation → `SetTopology` O(1) swap), `command_handler`, `engine_coordinator`, `sidecar_supervisor` (heartbeat → soft fallback → safe mode), `pattern_manager` (song arrangements), `clip_orchestrator` (8×8 clip grid with launch quantization + telemetry), `genetic_sequencer` (DNA-driven pattern evolution), `modulation_matrix`, `mixer_bridge`/`mixer_orchestrator`, `timeline`, `midi_clock`/`midi_mapper`/`midi_sequence_kernel`, `analysis_worker` + `analysis_kernel` (BPM/key/transient extraction), `folder_monitor` (library watch), `streaming_manager` (double-buffered disk streaming), `transfusion_manager`, `ptp_engine` (UDP clock sync), `discovery` (UDP beacon + plugin dir watcher), `persistence` (`SystemConfig`, `ProjectState` bincode/JSON), `bounce` (offline WAV render), `ipc_audio_bridge` (jitter buffer, Kani-proved). |
| `nullherz-topology` | ~0.8k | Declarative graph reconciliation: diffs desired vs. actual `GraphTopology` into minimal `TopologyMutation` batches; `compiler.rs` produces `CompiledGraphPlan` stages, computes PDC path latencies, and re-verifies the plan hazard-free (`verify_no_hazards`, backed by a Kani proof + proptests). |
| `nullherz-mixer` | ~0.6k | Console builders: `create_4channel_mixer`, `create_dj_deck` (A–D logical decks), `create_studio_strip`, `create_aux_bus`, `create_crossfader`, plus topology validation. Each deck strip is Sampler → DnaMorph → KeySync → Gain → Biquad → StereoUtility → *(insert fx)* → DjIsolator, **stereo at every hop** (an L/R buffer pair per stage, `link_stereo` in `dj.rs`), ending in private per-deck L/R buffers plus stereo cue-bus sends; each deck also owns a live SEQUENCER node (`deck_x_sequencer`, trigger generator for DNA groove micro-timing — it needs an output edge to tick). The master chain sums per side (`master_sum_l`/`master_sum_r`, named for telemetry) with the preview sampler mixed in as a summing input. Node and buffer IDs come from the shared `IdAllocator` (separate address spaces). Emits command batches; owns no DSP. |
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
| `nullherz-dna` | ~1.8k | `SoundDNA` schema (16-D latent space, rhythmic/spatial profiles), ed25519-signed lineages (`SignedSoundDna`, `verify_signature`/`verify_lineage`), `LibraryDatabase` on `redb` with Smart-Crate trait filtering, `SampleRegistry` (atomic-swap, lock-free reader), `GeneticLibrary`, and `CloudPeerSync` — a TCP gossip overlay with Gossipsub-style mesh links (GRAFT/GOSSIP_PUB/GOSSIP_SIGNED) and mDNS-style discovery. |

### 1.6 UI Plane

| Crate | LOC | Responsibility |
| :--- | ---: | :--- |
| `nullherz-inspector` | ~6.4k | `egui`/`eframe` desktop app. Views: DJ Studio (mixer, waveform, transport, performance, DNA), Composer (endless-scroll step grid with sequencer routing and per-step playback telemetry), Audio Editor (waveform selection, OLA time-stretch, transient chop, non-destructive undo), Sampler, Library, Breeder (2-D transfusion pad), Genetic Cloud, Topology, Mastering, Player, Broadcast, Metrics, Account, Modulation, Notifications, Settings (audio/MIDI/network/calibration/preferences). Runs an in-process Conductor and consumes live telemetry. |
| `nullherz-ui-hal` | ~1.0k | Backend-agnostic widget/render layer: knobs, faders, VU meters with asymmetric ballistics, WGPU waveform renderer with MIP/LOD selection. |
| `nullherz-gateway` | ~0.2k | WebSocket bridge (default `127.0.0.1:9001`): broadcasts JSON telemetry to any number of clients (non-blocking broadcaster pattern), accepts JSON `TimestampedCommand`s and library queries. |
| `nullherz-bench` | ~0.2k | Criterion benchmarks. |
| `nullherz-backends` | ~1.1k | Audio I/O drivers: **ALSA, PipeWire, JACK, Threaded (software clock), Mock** — hot-swappable at runtime via `AudioBackendType`. |

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

- **Node vs. buffer address spaces**: nodes and audio buffers (graph edges) live in *separate* index spaces — `MAX_NODES = 64` processors, `MAX_BUFFERS = 128` virtual buffer slots (`nullherz-traits/src/execution.rs`). A stereo console needs ~2 buffers per strip stage, so 4 decks × 8 stages × 2 channels plus buses and master already exceeds 64. The `IdAllocator` hands out node IDs and buffer IDs from the two spaces independently; `GraphTopology.virtual_to_physical` is sized `[u32; MAX_BUFFERS]`. Out-of-range buffer indices are **rejected, loudly**, at three layers: `TopologyManager` refuses the command (conductor, off-thread), `GraphCompiler::compile` fails with `ConfigurationError`, and the RT-side `TopologyCoordinator` drops the mutation under a `debug_assert` (dropping is safe; clamping would alias a buffer another edge owns). Buffer ids are carried as the **`BufferId` newtype** (serde-transparent u32) through `NodeRouting` and `virtual_to_physical`, so comparing a buffer id against `MAX_NODES` is a compile error, not a review item. The crossfade-override sentinel (`block_x_map` cells = `MAX_BUFFERS + k` in a `u8`; requires `MAX_BUFFERS + MAX_CROSSFADE_BUFFERS ≤ 255`, enforced by a const assert) is encoded and decoded **only** through `BufferSlot` — the serial executor and worker pool share that single split point.
- **Planar sample buffers**: decoded samples are stored planar — channel *c* occupies `buffer[c*frames .. (c+1)*frames]` — and `SampleMetadata.total_samples` means frames *per channel* (`channels` is serde-defaulted for old libraries). `SamplerVoice::process_block_planar` advances its playhead in frames and renders each channel from its own plane (mono sources repeat channel 0), keeping the 4-wide SIMD interpolator valid on consecutive elements. The analysis worker analyses channel 0 only and preserves layout metadata; crop/time-stretch edits map per plane; decode overrides stale interleaved-era layout metadata on hydration.
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
| Clock sync | UDP `:319` | Typed protocol: SYNC `[0x01][t1]` broadcast at 1 Hz, DELAY_REQ `[0x02][id]`, DELAY_RESP `[0x03][id][t4]` (legacy bare-u64 SYNC accepted). Slaves measure the round trip offset-free from the four timestamps, EMA-filter (1/8) with a 100 ms plausibility clamp, and discipline a PI clock servo. Engine socket uses SO_REUSEADDR/PORT to coexist with `PtpClockProvider`'s SO_TIMESTAMPING socket. |
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
| `$TMPDIR/nullherz_fallback_*.redb` | test/fallback runs | Transient fallback library DBs, written to the system temp dir (never the repo root). |

---

## 5. Verification Infrastructure

- **197 `#[test]` functions** across the workspace (197/197 green, verified 2026-07-19); channel-identity tests drive one-sided stereo sources through the full chain and assert the silent side stays silent at the per-side master sums; a **golden-hash stereo master render** (`conductor/tests/golden_master_render_test.rs`) drives the full 4-deck console deterministically (no backend thread, direct `process_block`) and pins bit-exact FNV hashes of master L and R independently; conformance `Gauntlet` (`nullherz-traits/src/test_kit`) runs every registered processor through NaN ingestion, buffer-size oscillation, sub-block consistency, reset determinism, parameter reachability, and snapshot safety. Golden-hash render regression tests pin end-to-end DSP output (`nullherz-processors/src/golden_render_tests.rs`).
- **Kani proof harnesses** (8, behind the `kani-verify` feature): parallel stage execution has no hazards and stays in bounds (`audio-core/processors/graph/verification.rs`), compiled-plan hazard verification detects overlaps (`nullherz-topology`), PI clock-servo integral clamping (`nullherz-traits/src/clock.rs`), jitter-buffer panic freedom (`conductor/ipc_audio_bridge.rs`), and SHM ring, MPSC ring, and `ShmSignal` atomic-ordering safety (`ipc-layer`).
- **Warning-free**: `cargo check --workspace --all-targets` completes with zero warnings. The gate is `scripts/verify.sh` (check with `-D warnings` + full test suite; `--full` adds the advisory clippy count), enforceable as a pre-push hook via `git config core.hooksPath .githooks`. The GitHub Actions workflows (`ci.yml`, `kani.yml`) mirror it for whenever Actions is available.
- Integration/decoupling test suites live in `audio-core` (`integration_tests.rs`, `decoupling_tests.rs`, `engine_tests.rs`) and `conductor` (`mixing_test.rs`).
