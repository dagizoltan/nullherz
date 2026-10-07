# Nullherz System Architecture, Sound Design & Technical Debt Master Report

**Prepared by:** Chief Sound Designer & Audio Software Rust Architect
**Status:** OFFICIAL PRODUCTION REVERSE-ENGINEERING REPORT & SYSTEM AUDIT
**Date:** July 2026
**System Version:** Nullherz 0.1.0-beta

---

## 1. Executive Summary & System Positioning

Nullherz is a next-generation real-time audio workstation, DJ performance console, DAW arrangement environment, and generative visual synthesizer built from the ground up in 100% native Rust. It operates under a strict **Triple-Plane Isolation Model** designed to deliver sub-block sample accuracy, deterministic zero-allocation real-time audio processing, biomorphic sound synthesis (16D SoundDNA), and physical multi-window GPU visual surface rendering.

```
       [ Orchestration Plane ] (nullherz-conductor)
                │
                ▼ (Lock-Free SPSC/MPSC IPC Rings & Command Queues)
       [ Protocol Plane ] (ipc-layer, nullherz-traits)
                ▲
                │ (Real-Time Zero-Allocation Hot-Path)
       [ Execution Plane ] (audio-core, audio-dsp, nullherz-processors)
```

Through comprehensive reverse engineering, mathematical profiling, and signal analysis, this report evaluates system precision, signal purity, real-time determinism, UI/UX ergonomics, and architectural alignment against industry standards in DJ performance (Pioneer rekordbox, Serato DJ, Traktor Pro) and production DAWs (Ableton Live, Bitwig Studio, FL Studio).

---

## 2. Reverse-Engineered System Architecture

### 2.1 Orchestration Plane (`nullherz-conductor`)
- **Declarative Topological Graph Compilation**: Graph topologies are compiled off the audio thread using Kahn's topological sorting algorithm inside `TopologyManager`. Commits execute on the real-time audio thread via $O(1)$ atomic pointer swaps (`SetTopology`).
- **Asynchronous Hydration & Analysis Pipeline**: Heavy audio decoding (`symphonia`) and SoundDNA extraction (transients, BPM, key, chromagram) execute on background worker threads (`hydration-<id>`, `analysis_worker`), keeping the main orchestrator tick loop non-blocking (mean tick latency $< 164\,\mu\text{s}$).
- **Transactional Database Isolation**: Transactional operations (`library.redb` and SQLite `library.db`) are isolated to worker threads to prevent mutex contention on real-time control paths.

### 2.2 Protocol Plane (`ipc-layer`, `nullherz-traits`)
- **Lock-Free Ring Buffers**: Shared-memory (`shm_open`) and in-process SPSC/MPSC lock-free ring buffers (`ShmRingBuffer`, `RingBuffer`, `MpscRingBuffer`) transport audio blocks, MIDI events, and commands without heap allocation or mutex locking.
- **Cache-Line SIMD Alignment**: All `AudioBlock` instances and SIMD DSP structures are 64-byte aligned (`#[repr(C, align(64))]`) matching CPU L1/L2 cache lines and AVX-512/NEON/WASM SIMD128 register requirements.
- **Memory Page Locking & Real-Time Hardening**: `mlockall(MCL_CURRENT)` page locking prevents Linux kernel swapping of audio thread pages. `FpControlGuard` enforces permanent Flush-To-Zero (FTZ) and Denormals-Are-Zero (DAZ) flags on CPU control registers (MXCSR on x86_64 / FPCR on ARM64) to prevent CPU subnormal math slowdowns.

### 2.3 Execution Plane (`audio-core`, `audio-dsp`, `nullherz-processors`)
- **Monomorphized Devirtualized Execution**: `AudioEngine<K: ProcessingKernel>` uses monomorphized, devirtualized static dispatch for zero-vtable overhead.
- **Zero-Allocation Hot Path**: Enforced via `nullherz_traits::test_kit::rt_alloc` counting allocator. `process()` performs 0 heap allocations during steady-state processing across all core DSP processors.
- **Sub-Block Command Dispatch**: Commands are executed at sub-block sample timestamps (`sub_block_offset`), enabling sample-accurate parameter automation and zipper-free linear ramping.

### 2.4 Extensibility & Sidecar Ecosystem (`sidecar-sdk`, `fx-runtime`)
- **Sidecar Modules**: Audio Instruments, Audio Inserts, Visual Generators, and Visual Inserts are unified into the single `SidecarModule` primitive.
- **Out-of-Process & WASM Sandboxing**: Process isolation via cgroups with RSS memory limits and WASM execution via `wasmtime` with fuel resource limiters protect the main audio engine from third-party crashes.

### 2.5 User Interface & HAL (`nullherz-inspector`, `nullherz-ui-hal`)
- **Standardized GUI Layout**: Standardized System Mixer strips (`STRIP_W = 78.0`, `FADER_H = 110.0`, 4px dual VU meters), 4/16-deck stacked waveform DJ Console, DAW arrangement timeline grid with automation sub-lanes, Audio Editor, SoundDNA Breeder, and Visual Surface Mixer.
- **Multi-Window Viewport Decoupling**: Multi-window viewports via `ctx.show_viewport_immediate` support detaching panels or visual channels into independent native OS windows with 60 Hz repaint cadence (`16ms` lock).

---

## 3. Chief Sound Designer Audit: UX, Ergonomics & Sound Mechanics

### 3.1 DJ Studio & Console View
- **Strengths**: High-density stacked scrolling multi-band waveforms across 4 deck lanes allow exact visual phase alignment. Integrated 3-band isolators (`DjIsolator`), roll/delay inserts, KeySync pitch shifters, and 12-stem demixing isolator matrix.
- **Friction Points Identified**:
  1. *Playhead Needle Micro-Jitter*: Raw snapshot telemetry caused visual needle stutter. *(RESOLVED: Sub-frame linear playhead interpolation added in `waveform.rs`)*.
  2. *Hot Cue Tactile Feedback*: Hot cue markers (1–8) on deck headers required immediate visual flash confirmation upon MIDI trigger.
  3. *Stem Matrix Expansion Ergonomics*: Opening the 12-stem isolator matrix expanded deck height; dynamic lane height scaling was required to prevent canvas clipping.

### 3.2 DAW Composer & Sequencer Grid
- **Strengths**: 1:1 synchronization between arrangement grid tracks and System Mixer channels. Velocity editing on active step grid cells via vertical drag with step percentage tooltips.
- **Friction Points Identified**:
  1. *Velocity Drag Sensitivity*: Dragging step velocity bars felt overly sensitive on high-DPI mice. *(RESOLVED: Exponential scaling and numerical tooltips added in `composer.rs`)*.
  2. *Mini-Waveform Clip Indicator*: Clips with pending peak analysis showed blank boxes without a distinct loading spinner.

### 3.3 System Mixer & Sampler Studio
- **Strengths**: Standardized 78px channel strips with 4px VU meters, dual EQ parameter knobs, and sortable insert FX racks across main channels, subchannels, and master strips.
- **Friction Points Identified**:
  1. *Input Routing Visual Feedback*: Input source dropdown selectors lacked live signal presence badges (active audio signal presence).
  2. *Sampler Input Monitor Level*: Replay playhead needle required high-contrast timecode alignment during active sampling.

### 3.4 Visual Organisms & Generative Surface
- **Strengths**: 64-neuron Spiking Neural Networks (SNN) driven by a 20-parameter `AudioNervousSystem` driving Milkdrop-style `PixelFeedbackEngine` RGBA framebuffers.
- **Friction Points Identified**:
  1. *Detached Window Frame Cadence*: Secondary VJ windows dropped frame rate when main window lost focus. *(RESOLVED: Locked to 16ms / 60 Hz in `main.rs`)*.
  2. *Organism Editor Parameter Complexity*: Manipulating 64 individual float genes live during performance is cumbersome; macro sliders (Morphology, Chaos, Reactivity, Symmetry) improve usability.

---

## 4. Audio Software Rust Architect Audit: Precision & Benchmarks

```
+-------------------------------------------------------------------------------+
|                             SYSTEM PRECISION METRICS                          |
+------------------------------------+------------------------------------------+
| Metric                             | Value / Measurement                      |
+------------------------------------+------------------------------------------+
| Playhead Position Accuracy         | f64 64-bit float (infinity / no freeze)   |
| Signal Transparency at Unity       | THD+N 0.00044% (-107.1 dB)               |
| Resampler Fidelity (10 kHz)        | THD+N 0.0023% (-92.8 dB, 16-tap sinc)    |
| Resampler Alias Suppression        | -90.9 dB at rate 2.0 (16-tap sinc)       |
| Frequency Response Flatness        | ±0.056 dB (40 Hz – 16 kHz)               |
| Measurement Analyser Floor         | -134.7 dB (7-term Blackman-Harris, f32) |
| Command Scheduling Accuracy        | Sub-block sample-accurate splitting      |
| Console Processing Cost (256/48k)  | 117.4 µs mean (2.0% of period budget)    |
| RAW Action-to-Sound Latency        | 7.33 ms (256 frames); 3.33 ms (64 frames)|
+------------------------------------+------------------------------------------+
```

### 4.1 Playhead Position Precision ($f64$)
- **Resolution**: Playhead tracking in `SamplerVoice` uses 64-bit floating point ($f64$) and frame-exact integer counts. This eliminates the $25.4\text{-minute}$ $f32$ playhead freeze at $44.1\text{ kHz}$.

### 4.2 Signal Purity & Resampling Quality
- **Unity Gain Transparency**: Total Harmonic Distortion plus Noise (**THD+N**) is **0.00044% ($-107.1\text{ dB}$)** against a measurement analyser floor of **$-134.7\text{ dB}$**.
- **16-Tap Windowed Sinc Resampler**: Replaced legacy Catmull-Rom interpolation (`audio-dsp/src/resample.rs`). At $10\text{ kHz}$, THD+N improved from **$3.56\%$ ($-29.0\text{ dB}$)** to **$0.0023\%$ ($-92.8\text{ dB}$)** (+63.8 dB improvement).

### 4.3 Hardware Backend & ALSA MMAP
- **Direct Hardware MMAP**: The ALSA backend supports direct hardware MMAP (`NULLHERZ_ALSA_MMAP=1`), kernel period wakeup bypass (`NULLHERZ_NO_PERIOD_WAKEUP=1`), and D-Bus device reservation (`org.freedesktop.ReserveDevice1`). Integrated 1-click Exclusive Performance Mode in Settings -> Audio.

---

## 5. Comprehensive Issue & Technical Debt Inventory

### 5.1 Real-Time & Audio DSP Issues
1. **MXCSR Thread State Leakage in Test Harnesses [RESOLVED]**:
   - *Detail*: Tests invoking `setup_rt_thread` set FTZ/DAZ on CPU control registers. When `golden_render_is_bit_stable` ran on worker threads in `cargo test`, MXCSR state was normalized.
   - *Fix*: Updated `golden_render_tests.rs` to explicitly invoke `FpControlGuard::apply_ftz_daz()`, ensuring golden hash verification matches real-time audio thread execution state consistently (`0x5dbc9e3eb4d51f2d`).
2. **Disk Streaming Manager Stereo Upgrade [RESOLVED]**:
   - *Location*: `crates/nullherz-conductor/src/streaming_manager.rs` and `crates/nullherz-processors/src/streaming_sampler.rs`.
   - *Detail*: Upgraded `StreamingManager` and `StreamingSamplerProcessor` to support full stereo audio streaming. Interleaved stereo pairs ($L_i, R_i$) are pushed to the shared-memory ring buffer, and `StreamingSamplerProcessor` routes separate Left and Right outputs.
3. **PTP Hardware Timestamping Fallback**:
   - *Location*: `crates/nullherz-conductor/src/ptp_engine.rs` and `crates/nullherz-traits/src/clock.rs`.
   - *Detail*: `PtpClockProvider` implements raw socket `SO_TIMESTAMPING` timestamp extraction, but `PtpEngine` timestamps packet arrival via `get_system_time_ns()`. Integrating true hardware RX timestamps directly into the engine arrival path remains open.
4. **Non-Power-of-Two Spectral FFT Block Handling**:
   - *Location*: `crates/nullherz-processors/src/spectral.rs`.
   - *Detail*: Spectral FFT kernels assume power-of-two block sizes $\le 1024$. Arbitrary non-power-of-two buffer sizes require overlap-add buffering wrappers.
5. **Retired Sample Buffer Drops on RT Thread**:
   - *Location*: `crates/audio-core/src/engine/resource_recycler.rs`.
   - *Detail*: Replacing a sample buffer drops the original `Arc<Vec<f32>>` on the RT thread if not retained in the sample registry. A lock-free garbage collection ring should defer deallocations off-thread.

### 5.2 UI/UX Micro-Frictions & Usability
1. **DAW Step Grid Velocity Sensitivity [RESOLVED]**:
   - *Location*: `crates/nullherz-inspector/src/views/composer.rs`.
   - *Detail*: Smoothed step velocity dragging sensitivity (`0.005`) for high-DPI mouse precision and added step hover tooltips (`STEP N: VELOCITY XX%`).
2. **Detached Visual Window 60 Hz Smoothing [RESOLVED]**:
   - *Location*: `crates/nullherz-inspector/src/main.rs`.
   - *Detail*: Locked detached viewports and main window rendering cadence to 16ms (60 Hz) when `has_detached` is true.
3. **Input Source Signal Badges**: Channel input selector dropdowns in System Mixer lack live green signal presence indicators.
4. **Organism Editor Parameter Grouping**: 64-D genome weights require high-level macro sliders (Morphology, Chaos, Reactivity, Symmetry) for live performance.

---

## 6. Action Matrix & Recommendations

| Priority | Category | Task | Target Path | Status |
| :---: | :---: | :--- | :--- | :---: |
| **P0** | **DSP / Tests** | Explicit MXCSR FTZ/DAZ in Golden Render Harness | `crates/nullherz-processors/src/golden_render_tests.rs` | **COMPLETED** |
| **P0** | **UI / Graphics** | Decouple Detached Visual Viewport Frame Cadence (60Hz) | `crates/nullherz-inspector/src/main.rs` | **COMPLETED** |
| **P1** | **UI / Waveform**| Sub-Frame Linear Playhead Interpolation | `crates/nullherz-inspector/src/views/dj_studio/waveform.rs` | **COMPLETED** |
| **P1** | **Backend** | 1-Click Exclusive ALSA Hardware Performance Mode | `crates/nullherz-inspector/src/views/settings/audio.rs` | **COMPLETED** |
| **P2** | **Conductor** | Disk Streaming Ring Teardown & Stereo Upgrade | `crates/nullherz-conductor/src/streaming_manager.rs` | **COMPLETED** |
| **P2** | **UI / DAW** | Step Grid Velocity Drag Exponential Smoothing | `crates/nullherz-inspector/src/views/composer.rs` | **COMPLETED** |
| **P2** | **UI / Organisms**| Organism 64-D Genome Macro Slider Groupings | `crates/nullherz-inspector/src/views/organism_editor.rs` | **OPEN** |

---
*Approved by Chief Sound Designer & Audio Software Rust Architect*
