# Nullherz System Feature Matrix (Stage 6: Evolutionary Intelligence)

**Current State:** see [IMPLEMENTATION_ROADMAP_2026_07.md](../roadmap/IMPLEMENTATION_ROADMAP_2026_07.md) for current phase progress. A ✅ below means **a user can reach it in the application**, verified by `crates/nullherz-conductor/tests/reachability_gate_test.rs`.
**Last Updated:** 2026-10-08 — re-verified against the tree (`main` @ `9d4be33`). See [ARCHITECTURE.md](../system/ARCHITECTURE.md) and [REVERSE_ENGINEERING_SYSTEM_REPORT_2026.md](./REVERSE_ENGINEERING_SYSTEM_REPORT_2026.md) for the commands behind every number here.

> ⚠️ **Two caveats that apply to this whole matrix, both found on 2026-10-08.**
>
> 1. **The verification gate is RED.** `scripts/verify.sh` fails reproducibly
>    (5/5) on `test_long_track_does_not_stall_the_control_path`: ALSA device
>    enumeration costs 9–10 ms per call and runs synchronously on
>    `Conductor::tick()`, against a 5.805 ms budget. Debt §1.1. A ✅ below means
>    *reachable*; it does not mean the gate is green.
> 2. **✅ means reachable, not fully controllable** — still the right warning to
>    read these rows with, though the example that used to carry it is gone. The
>    deck FX rack *was* that example: one engine slot under an unbounded UI rack
>    whose remove, reorder and parameter controls emitted no commands. It is now
>    four positionally-bound slots per deck with every control wired (debt §4),
>    so the **⚠️ one-slot** tag no longer applies to any row. What remains in
>    that shape is the **pad subchannel strips** — see their row below and debt
>    §3.1; there the UI is finished and the audio subsystem does not exist.

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
| **Signal Transparency** | ✅ | Bit-exact identity at unity. **Re-measured 2026-10-08: THD+N 0.0000049% (-146.1 dB)** against a -153.2 dB analyser floor (FFT 16384). Response ripple ±0.056 dB, 40 Hz – 16 kHz, about a **-3.04 dB mean** — the constant-power crossfader law at centre (`1/√2`), by design and pinned by `test_curve_endpoints_are_linear_and_constant_power`. The earlier -107.1 dB figure was measured at a smaller FFT. |
| **16-Tap Sinc Resampler** | ✅ | 16-tap Kaiser **β=14** windowed sinc with **cubic** table interpolation. **Re-measured 2026-10-08: -129.2 dB THD+N at 10 kHz, -129.4 dB at 997 Hz — flat across frequency** at realistic tempo ratios; 100 dB better than the Catmull-Rom it replaced (-29.0 dB at 10 kHz). The previously published "-92.8 dB" was the superseded β=9/linear-table configuration; the shipped kernel is **36 dB better than was claimed**. Costs 296 ns/sample, 45.5% of a 256-frame budget at 32 voices — **the voice-count ceiling**. |
| **Pitch-Up Alias Suppression** | ⚠️ | The kernel scales its anti-alias cutoff by the stretch, but 16 taps give a finite transition width. 16 kHz source: **-17.7 dB fold at rate 1.6**, -63.0 dB at rate 2.0, gone (-136 dB) by 2.2. The published "-90.9 dB at rate 2.0" **does not reproduce**. Small on music (16 kHz energy sits ~40 dB below peak); real nonetheless. |
| **64-bit f64 Playhead** | ✅ | 64-bit float playhead tracking in `SamplerVoice`, eliminating the 25.4-minute f32 playback freeze. |
| **Exact Filter Math** | ✅ | Runtime Linkwitz-Riley coefficient generation for exact crossovers; bounded -0.115 dB isolator re-sum error. |
| **Soft Fallback & Recovery** | ✅ | Heartbeat-monitored instant swap to bypass node upon DSP failure; escalation to global Safe Mode. |
| **Spectral Processor** | ✅ | Hardened FFT overlap-add with exact COLA-normalized synthesis window (up to 1024 frames). |
| **KeySync Pitch Shift** | ⚠️ | Phase-vocoder pitch shifter with per-bin phase tracking, installed on demand into the deck pitch slot (zero latency when disengaged). Sample-rate agnostic by construction — frequencies are carried in **bin units**, not Hz. **Quality is the open problem:** worst partial suppression **-17.8 dB** at the shipped N=1024/hop 128, with **7.65 dB spread between partials** on a chord. The error is *timbral*, so no makeup gain fixes it; the specified replacement is time-stretch + resampling. 21.33 ms latency. Debt §2.5. |
| **Colored Waveforms** | ✅ | Per-window 3-band peaks + signed envelope (BandWaveform); filled per-vertex-colored GPU rendering. |
| **DJ Transport Semantics** | ✅ | Stop pauses (position held), play resumes in place, load clears voices; playhead reports held position. |
| **Planar Stereo Playback** | ✅ | Planar sample buffers end to end; frame-counted playhead; per-plane crop/stretch; deck strips stereo at every hop. |
| **OLA Time-Stretch** | ✅ | Overlap-add `time_stretch` kernel with corrected ratio semantics (`audio-dsp/util.rs`). |
| **Transient Detection** | ✅ | Spectral-flux + RMS onset/transient detectors powering editor chop and analysis. |
| **Parallel Graph Execution** | ✅ | Static stage assignment `TaskPool` with cost-gated threshold calibration. |
| **no_std Embedded Runtime** | ✅ | `#![no_std]` compilation support for `audio-dsp` (`--no-default-features`), enabling execution on embedded ARM/FPGA baremetal hardware. |

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
| **Deck FX Insert Rack** | ✅ | 4 positional slots per deck, each bound to a `deck_<x>_fx<n>` node: load/remove via `SwapProcessor` (empty = `BYPASS`), reorder as a real topology edit, per-slot ramped `SetParam`. Bounded by the graph — a full rack disables the load button rather than overwriting a slot. |
| **Pad Subchannel Strips** | ⚠️ | Display only. The graph has one `drum_machine_node` and no per-pad strip, so the pad rack, fader, GAIN, PITCH and EQ write UI state and reach no processor. Remove/reorder disabled; see debt §3.1. |

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
| **ALSA** | ✅ | Default direct hardware backend; configurable period and buffer sizes, direct hardware MMAP mode (`NULLHERZ_ALSA_MMAP`), `NO_PERIOD_WAKEUP` kernel bypass (`NULLHERZ_NO_PERIOD_WAKEUP`), and D-Bus device reservation (`org.freedesktop.ReserveDevice1`). |
| **PipeWire** | ✅ | PipeWire graph integration with native ALSA/JACK interop. |
| **JACK** | ✅ | Professional pro-audio server integration. |
| **Threaded** | ✅ | Software-clocked fallback backend for desktop/headless execution. |
| **Mock** | ✅ | Deterministic zero-I/O backend for automated test suites and CI. |

---

## 8. Workspace DSP Processors, Sidecars & Visual Engines Catalog

### 8.1 Native DSP Processors (`crates/nullherz-processors`)
| Processor Name | Type ID | Category | Description |
| :--- | :---: | :---: | :--- |
| `Bypass` | 0 | Utility | Zero-latency passthrough wire node |
| `Biquad` | 1 | Filter | Single-stage biquad filter (LPF, HPF, BPF, Notch) |
| `Gain` | 2 | Utility | High-performance smoothed gain with 2x oversampled soft-clipping |
| `Sampler` | 3 | Source | Multi-voice stereo sample playback engine with 64-bit playhead |
| `Crossfader` | 4 | Mixer | SIMD-optimized DJ crossfader with continuous power curve blending |
| `Summing` | 5 | Mixer | SIMD 16-to-1 bus mixing node |
| `Spectral` | 6 | FX | FFT overlap-add spectral resynthesis and window shaping |
| `Wavetable` | 7 | Source | Multi-oscillator wavetable synthesis engine |
| `Modulation` | 8 | Control | LFO and envelope modulation generator |
| `Sequencer` | 9 | Control | Step sequencer micro-timing controller |
| `EnvelopeFollower` | 10 | Analysis | Peak/RMS amplitude envelope tracking |
| `Granular` | 11 | Source/FX | Multi-grain buffer resynthesis engine |
| `SpectralMorph` | 12 | FX | Dual-spectrum interpolation and morphing engine |
| `Capture` | 13 | Utility | Real-time audio buffer capture and monitoring |
| `DjIsolator` | 14 | Mixer | 3-band Linkwitz-Riley DJ frequency isolator |
| `MasteringEq` | 15 | FX | Linear-phase 3-band tone stage |
| `SimdBiquad` | 16 | Filter | AVX-512/NEON parallel multi-channel biquad bank |
| `KeySync` | 17 | FX | Phase-vocoder pitch shifter with per-bin phase locking |
| `PersonalityInheritance` | 18 | DNA | DNA trait inheritance and spectral resynthesis filter |
| `DnaMorph` | 19 | DNA | Multidimensional DNA latent space interpolator |
| `Limiter` | 20 | Dynamics | Look-ahead peak limiter, brickwall ceiling held exactly, transparent below threshold (-150 dB). **Sample-peak only — not ITU-R BS.1770 true peak** (no oversampling in the tree). 1.2% THD on 60 Hz under 6 dB of limiting: single-stage release. Debt §2.2, §2.4 |
| `StreamingSampler` | 21 | Source | Double-buffered disk-to-SHM streaming sampler |
| `Delay` | 22 | FX | Multi-tap delay line with fractional Hermite interpolation |
| `NeuralSaturator` | 230 | Neural FX ⚠️ | Padé SIMD rational soft-clipping saturator. Memoryless at base rate with **no oversampling** → harmonics above Nyquist fold back in band. Applies to `TapeSaturator`, `TubePreamp` and `NeuralNam` equally. Debt §2.2 |
| `NeuralFilter` | 231 | Neural FX ⚠️ | State-variable hypernetwork dynamic filter. **Hardcodes `sample_rate = 48000.0` inside `process()`** — cutoff lands wrong at every other rate. One-line fix. Debt §2.3 |
| `NeuralTcn` | 232 | Neural FX | 4-layer dilated Temporal Convolutional Network (TCN) |
| `NeuralSsm` | 233 | Neural FX | 24-state Diagonal State-Space Model dynamic compressor |
| `NeuralNam` | 234 | Neural FX | Wave-shaping network for tube preamp and amp modeling |
| `HyperNetworkEq` | 235 | Neural FX | Hypernetwork-conditioned parametric equalizer |
| `TubePreamp` | 236 | Neural FX | Triode tube preamp with transformer hysteresis |
| `MultiBandCompressor` | 237 | Dynamics | 3-band State-Space Model dynamic compressor |
| `Reverb` | 238 | FX ⚠️ | **4+2** Freeverb comb/all-pass network (canonical is 8+4). **Delay lengths hardcoded to 44.1 kHz with no path to the session rate**; **identical impulse response in both channels → zero stereo width**; 384 KB struct for 1356 samples of delay. Debt §2.1 |
| `ModulationFx` | 239 | FX | Multi-mode LFO insert (Chorus, Flanger, Phaser) |
| `Compressor` | 240 | Dynamics | Peak/RMS dynamic range compressor |
| `StereoUtility` | 241 | Utility | Balance, width, and phase correlation utility |
| `Analysis` | 250 | Analysis | Multi-timescale audio perception and feature extraction |

### 8.2 Sidecar SDK Catalog (`crates/sidecar-sdk`)
| Sidecar ID | Type | Tags | Description |
| :--- | :---: | :--- | :--- |
| `neural-saturation` | NeuralProcessor | `neural`, `insert`, `real-time`, `saturation` | Padé SIMD neural analog saturation processor |
| `neural-ssm` | NeuralProcessor | `neural`, `insert`, `real-time`, `ssm`, `compressor` | 24-state State-Space Model for dynamic compression and hysteresis |
| `neural-nam` | NeuralProcessor | `neural`, `insert`, `real-time`, `nam`, `preamp` | Wave-shaping network for analog tube preamp and guitar amp emulation |
| `neural-filter` | NeuralProcessor | `neural`, `insert`, `real-time`, `filter`, `eq` | Hypernetwork dynamic filter with SIMD non-linearities |
| `neural-tcn` | NeuralProcessor | `neural`, `insert`, `real-time`, `tcn`, `saturation` | 4-layer dilated TCN with FloatX16 SIMD reduction |
| `hypernetwork-eq` | NeuralProcessor | `neural`, `insert`, `real-time`, `hypernetwork`, `eq` | HyperNetwork conditioned parametric EQ |
| `tube-preamp` | NeuralProcessor | `neural`, `insert`, `real-time`, `tube`, `saturation`, `preamp` | Asymmetric triode tube preamp with transformer hysteresis |
| `multiband-compressor` | NeuralProcessor | `neural`, `insert`, `real-time`, `ssm`, `compressor`, `multiband` | 3-Band crossover feeding parallel SSM compression cells |
| `algorithmic-delay` | Insert | `algorithmic`, `insert`, `real-time`, `delay` | Low-latency delay line with Hermite fractional interpolation |
| `algorithmic-eq` | Insert | `algorithmic`, `insert`, `real-time`, `eq`, `filter` | Multi-mode State-Variable Filter (LP, HP, BP, Notch) |
| `algorithmic-reverb` | Insert | `algorithmic`, `insert`, `real-time`, `reverb` | Schroeder/Freeverb comb and all-pass network. ⚠️ Resolves to the **in-process** `Reverb` type via `SwapProcessor`, not the sidecar — see debt §3.5. Carries the §2.1 defects |
| `algorithmic-modulation` | Insert | `algorithmic`, `insert`, `real-time`, `modulation`, `chorus` | Multi-mode LFO insert (Chorus, Flanger, Phaser) |
| `algorithmic-synth` | Instrument | `algorithmic`, `instrument`, `real-time` | Dual-oscillator MIDI synthesizer instrument |
| `neural-visuals` | NeuralProcessor | `visual`, `neural`, `insert`, `real-time`, `instrument` | Audio & telemetry input driven neural visual surface sidecar |
| `bioluminescent-fluid-flow` | NeuralProcessor | `visual`, `neural`, `real-time` | Organic bioluminescent fluid dynamics visual surface |
| `harmonic-arrangement-lattice` | NeuralProcessor | `visual`, `real-time` | Musical structure and harmonic chroma lattice visual sidecar |
| `abstract-quantum-swarm` | NeuralProcessor | `visual`, `real-time` | Multi-spectral colorful quantum particle swarm visual sidecar |
| `neural-floral-mycelium` | NeuralProcessor | `visual`, `neural`, `real-time` | Organic growing floral mycelium tendril visual surface |
| `neural-latent-manifold` | NeuralProcessor | `visual`, `neural`, `insert`, `real-time` | Neural network latent manifold visual generator |
| `phase-goniometer-2d` | NeuralProcessor | `visual`, `real-time`, `insert` | Real-time 2D phase goniometer stereo visual surface |
| `fft-spectrum-mesh` | NeuralProcessor | `visual`, `real-time`, `insert` | Real-time FFT frequency spectrum 3D mesh visual sidecar |
| `reaction-diffusion-nn` | NeuralProcessor | `visual`, `neural`, `real-time` | Neural network reaction diffusion pattern synthesis |
| `shader-particle-swarm` | NeuralProcessor | `visual`, `real-time` | Audio-reactive particle swarm shader visual generator |

### 8.3 Visual Engines & Organisms (`crates/nullherz-inspector`)
| Generator | Engine | Taxonomy / Sub-Styles | Description |
| :--- | :--- | :--- | :--- |
| `RadialMandala` | `RadialMandalaEngine` | 12 Polar Styles (Sacred Geometry, Neon Matrix, Celestial Pulse...) | Hyper-symmetric CPPN mandala generator |
| `LiquidSurface` | `LiquidSurfaceEngine` | Bio-Fluid & Cellular Warping | Two-pass latent domain fluid warper |
| `SpectralLandscape` | `SpectralLandscapeEngine` | Voxel Terrain, Waterfall Contour, Linear Equalizer | 3D instanced voxel waterfall terrain |
| `HyperAttractor` | `HyperAttractorEngine` | 100k GPU Particle Swarm | Neural chaos attractor particle generator |
| `ReactionDiffusion` | `ReactionDiffusionEngine` | Turing Morphogenesis | Turing pattern Gray-Scott pattern simulator |
| `NeuralRaymarcher` | `NeuralRaymarcherEngine` | Latent Signed Distance Fields (SDF) | Latent SDF raymarcher |
| `NeuralNcaMesh` | `NeuralNcaMeshEngine` | SphereMesh, TorusGrid, Voxel, CrystallineSprout | 3D Neural Cellular Automata mesh growth |

---

**Legend:**
- ✅ **Hardened**: implemented, reachability-verified (`reachability_gate_test.rs`), RT-safe (`conformance_gauntlet.rs`), green in the test suite.
- ⚠️ **Reachable with a measured limitation**: a user can get to it and it does something, but a named defect or quality ceiling applies. Every ⚠️ row cites the entry in [`TECHNICAL_DEBT_AND_STUBS.md`](./TECHNICAL_DEBT_AND_STUBS.md).
- 🔶 **Active**: functional implementation undergoing active refinement.
- 🧪 **Prototype**: research prototype or experimental hardware spec.

**What ✅ does not mean.** It does not mean *good*, and it does not mean *the
gate is green* — see the two caveats at the top of this file. Reachability and
quality are separate axes, and this matrix tracks reachability. For quality, read
[ARCHITECTURE.md §1.1.1](../system/ARCHITECTURE.md) (which rows of the DSP are
reference-grade and which are placeholders) and §4 of the
[master audit](./REVERSE_ENGINEERING_SYSTEM_REPORT_2026.md).

> **Not re-verified on 2026-10-08:** the `.clac` container, stem separation, the
> DNA network/consensus layers, and the 9 visual engines in §8.3. Their rows are
> carried forward from the July pass. Treat them as claims until a probe or a
> test is attached, per `AGENTS.md` §4.
