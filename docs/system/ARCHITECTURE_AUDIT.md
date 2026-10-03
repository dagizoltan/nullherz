# Nullherz Architecture Audit, Systems Model & Discrepancy Matrix

**Date:** September 2026
**Role:** Senior Audio/DSP Architect, Rust Systems Architect, Real-Time Audio Engineer, UI/UX Architect

---

## 1. Executive Summary & Audit Mission

This document provides a comprehensive reverse-engineering audit of the entire Nullherz DJ DAW and audio engine codebase (~60,000 LOC Rust across 19 crates and 8 sidecars).

The objective is to establish the actual runtime architecture, map all data/control/audio planes, identify state boundaries, and document discrepancies between intended, documented, implemented, and runtime behaviors — specifically focusing on audio device discovery, configuration persistence, mixer/channel state, and UI state synchronization.

---

## 2. Current End-to-End Architecture Model

```text
┌───────────────────────────────────────────────────────────────────────────┐
│                                 UI PLANE                                  │
│                      nullherz-inspector (egui/eframe)                     │
│               UI-Local State & View Controllers (30/5 Hz)                │
└─────────────────────────────────────┬─────────────────────────────────────┘
                                      │ In-Process Command Ring Buffer /
                                      │ WebSocket :9001 (JSON / Binary)
                                      ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                           ORCHESTRATION PLANE                             │
│                      nullherz-conductor (Daemon Loop)                     │
│  Orchestrator Tick (100 Hz) • CommandHandler • MixerManager • Persistence │
└──────────────────────┬─────────────────────────────────────┬──────────────┘
                       │ Topology Ring                       │ Command Ring
                       ▼                                     ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                             EXECUTION PLANE                               │
│                         audio-core / audio-dsp                            │
│           AudioEngine<K> • ProcessorGraph VM (SCHED_FIFO, RT)             │
│            Planar Audio Blocks • Lock-Free Execution • PDC                │
└─────────────────────────────────────┬─────────────────────────────────────┘
                                      │ Interleaved / Planar Audio Frames
                                      ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                             BACKEND PLANE                                 │
│                         nullherz-backends                                 │
│          AlsaBackend | PipewireBackend | JackBackend | Threaded           │
└─────────────────────────────────────┬─────────────────────────────────────┘
                                      │ Hardware PCM Buffers (MMAP/RW)
                                      ▼
┌───────────────────────────────────────────────────────────────────────────┐
│                            HARDWARE DEVICE                                │
│                   ALSA Soundcard / PipeWire Server / DAC                  │
└───────────────────────────────────────────────────────────────────────────┘
```

### Layer-by-Layer Responsibilities

1. **UI Layer (`nullherz-inspector`)**:
   - Renders desktop UI (`egui`), handles user interaction, knobs, faders, waveforms, meters.
   - Resolves processor node IDs dynamically by **NAME** from telemetry `node_names` map (never hardcoded indices).
   - Manages UI-local state (`app.settings`, view filters, cached telemetry parameters).

2. **UI State Layer**:
   - Represents visual control state.
   - Derived or synchronized with Application State via telemetry snapshots received from the Conductor.

3. **Application / Orchestration State (`nullherz-conductor`)**:
   - Authoritative manager of system topology, active mixer deck mappings, library database connections (`redb`), track metadata, and persistence (`SystemConfig`, `ProjectState`).
   - Kahn's graph compilation runs off-thread in `TopologyManager`, producing lock-free `TopologyMutation` batches.
   - Performs command translation in `CommandHandler` and `MixerOrchestrator`.

4. **Audio / DJ Engine (`audio-core`)**:
   - Real-time execution plane (`AudioEngine<K: ProcessingKernel>`) running on a dedicated thread with `SCHED_FIFO` priority 80 and FTZ/DAZ denormal protection.
   - Zero heap allocations (`rt_alloc` enforced) and zero blocking syscalls during block processing.
   - Sample-accurate timestamped command execution (`TimestampedCommand`) via `StandardKernel`.

5. **Mixer / Channel Processing (`nullherz-mixer`)**:
   - Declarative console topology builders (`create_4channel_mixer`, `create_dj_deck`).
   - Each DJ deck strip processing graph:
     `Sampler -> [Pitch/KeySync Slot] -> [DNA Slot] -> Gain -> Biquad EQ -> StereoUtility -> [FX Insert Slot] -> DjIsolator`
   - Master bus: Stereo summing nodes (`master_sum_l`/`r`) -> `MasteringEq` -> `Limiter` -> Output Buffers.
   - Cue bus: Dedicated summing nodes (`cue_sum_l`/`r`) fed by per-deck private cue sends.

6. **DSP Graph (`audio-core`, `audio-dsp`, `nullherz-processors`)**:
   - Statically dispatched SIMD processing (`FloatX16` AVX-512 / WASM SIMD / scalar).
   - 24 registered processor factories using planar memory layout (`AudioBlock` aligned to 64 bytes).
   - Parallel stage execution with cost-gated worker pool (`TaskPool`).

7. **Audio Stream & Backend (`nullherz-backends`)**:
   - Abstraction driver layer (`AlsaBackend`, `PipewireBackend`, `JackBackend`, `ThreadedBackend`, `MockBackend`).
   - Formats, period sizes, sample rates, and ring buffer management.

8. **Device Layer**:
   - Physical audio interface (ALSA PCM `hw:X,Y`, PipeWire node, JACK client).

---

## 3. Plane Boundaries & Isolation

### Control Plane
- **Scope**: Configuration, device selection, project loading/saving, UI interaction, transport commands, BPM updates, MIDI mapping.
- **Timing Domain**: Non-RT (100 Hz conductor tick, 30 Hz UI repaint).
- **Primitives**: Standard Rust async/sync threads, `parking_lot::Mutex`, file I/O, `serde_json`, `redb`.
- **Invariants**: Must never block the Audio Plane. Commands are pushed to lock-free SPSC/MPSC command rings.

### Audio Plane
- **Scope**: RT audio callback (`process_block`), DSP node execution, buffer mixing, peak/meter metering.
- **Timing Domain**: Deterministic real-time callback (e.g. 128 frames @ 48 kHz = 2.67 ms deadline per block).
- **Primitives**: Lock-free SPSC/MPSC ring buffers, atomic variables (`AtomicBool`, `AtomicU64`), planar `AudioBlock` arrays.
- **Invariants**: Strictly ZERO heap allocations, ZERO Mutex locks, ZERO blocking syscalls, NO panics (`panic = "unwind"` with `catch_unwind`).

### Data Plane
- **Scope**: Audio samples (`SampleRegistry`), waveform MIPs, beat grids, DNA latent vectors, stem separation files.
- **Timing Domain**: Asynchronous background workers (`hydrate-<id>`, `analysis_worker`, `stem_worker`).
- **Primitives**: Zero-copy memory mapping (`MmapBuffer`), lock-free atomic `SampleRegistry` swaps, `redb` embedded KV store.
- **Invariants**: Decodes and file reads happen off the conductor tick thread and off the audio thread.

---

## 4. State Architecture Taxonomy

| State Category | Location | Mutability & Sync | Description |
| :--- | :--- | :--- | :--- |
| **Authoritative State** | `nullherz-conductor` (`MixerManager`, `TopologyManager`, `LibraryDatabase`) | Mutex / Conductor Thread | Ground truth for active decks, node mappings, track metadata, and routing. |
| **Execution State** | `audio-core` (`ProcessorGraph`, `AudioEngine`) | RT-safe lock-free SPSC queue | Live DSP processor instances and parameter arrays on the audio thread. |
| **Persisted System State** | `system_config.json` | Disk / `SystemConfig` struct | Persistent settings: audio backend, sample rate, block size, period size, MIDI ports, output/input device names. |
| **Persisted Project State**| `ProjectState` (bincode / JSON / rkyv) | Disk / Serialization | Graph topology snapshot, node parameters, modulation matrix, arrangements, active master deck. |
| **UI-Local State** | `nullherz-inspector` (`InspectorApp`, `state.rs`) | UI Thread (`eframe`) | View selections, active tab, UI control widget values, temporary strings. |
| **Derived State** | `nullherz-inspector` (from Telemetry) | Updated per Telemetry Snapshot | Deck play states, peak VU meters, active node names, CPU load indicators. |
| **Cached State** | `Conductor` / `InspectorApp` | Atomic / Snapshot | Cached audio device list (`cached_audio_devices`), cached track metadata. |

---

## 5. Audit of Critical Issue #1: Audio Device Discovery vs Configuration

### 5.1 Device Discovery Mechanism
- **Implementation**: `AlsaBackend::enumerate_devices()` invokes ALSA's C API (`snd_device_name_hint`) to discover hardware PCM devices.
- **Format Output**: Formats discovered entries as `"{pcm_name} — {description}"` (e.g., `"hw:0,0 — HDA Intel PCH, ALC269 Analog"`). `default` is inserted at index 0.
- **Telemetry Query**: `Orchestrator::tick()` caches device enumeration into `cached_audio_devices`, which is broadcast to the UI via telemetry.

### 5.2 The Device Name Discrepancy & Sanitization Bug
- **Discrepancy**:
  When a user selected a device in the UI combo box (e.g., `"hw:0,0 — HDA Intel PCH"`), `SetAudioOutputDevice` received the full string.
  `command_handler.rs` previously attempted sanitization using `dev_name.split(" (").next()`.
  Because the device string used `" — "` (em-dash with spaces) instead of `" ("`, `clean_dev` remained `"hw:0,0 — HDA Intel PCH"`.
- **Runtime Failure**:
  `command_handler.rs` set `NULLHERZ_ALSA_DEVICE` to `"hw:0,0 — HDA Intel PCH"` and restarted `AlsaBackend`.
  `AlsaBackend` passed this raw string to `snd_pcm_open()`.
  ALSA's library rejected `"hw:0,0 — HDA Intel PCH"` with error code -2 (No such file or directory) because `" — ..."` is not a valid ALSA PCM identifier.
- **Resolution**:
  Use `nullherz_backends::alsa::device_id()` across the codebase. `device_id()` splits on `" — "` and extracts the exact ALSA device specifier (`"hw:0,0"`, `"default"`, `"hdmi:0"`).

### 5.3 Configuration Persistence Gap
- **Discrepancy**:
  `SystemConfig` in `persistence.rs` contained `audio_backend`, `midi_ports`, `sample_rate`, `block_size`, `calibration_samples`, and `period_size`, but was **missing fields for selected audio output and input devices**.
  As a result, any device selection made in the UI was lost when the application was closed and restarted, resetting to system defaults.
- **Resolution**:
  Extend `SystemConfig` with `audio_output_device: Option<String>` and `audio_input_device: Option<String>` using `#[serde(default)]`.
  Update `update_system_config()` in `orchestrator.rs` to write these fields to `system_config.json` whenever devices are changed via `SetAudioOutputDevice` or `SetAudioInputDevice`.

### 5.4 Hardware Preset Override Bug
- **Discrepancy**:
  In `views/settings/audio.rs`, applying 1-Click Hardware Presets (e.g., "⚡ Ultra-Low Latency Live") unconditionally set `NULLHERZ_ALSA_DEVICE` to `"hw:0,0"`.
  If a user was running an external USB audio interface (e.g. `hw:1,0`), applying a preset forcibly overrode their device selection.
- **Resolution**:
  Presets must preserve the user's configured `audio_output_device` if set, rather than hardcoding `"hw:0,0"`.

---

## 6. Discrepancy Matrix (Intended vs Documented vs Implemented)

| System Area | Intended Behavior | Documented Behavior | Implemented Code Behavior | Discrepancy & Status |
| :--- | :--- | :--- | :--- | :--- |
| **Audio Output Device Selection** | User selects device in UI; engine opens exact device; setting persists. | "Select audio output device from Settings -> Audio." | String passed to `snd_pcm_open` included human label `" — ..."`, causing ALSA open failure. | **FIXED**: Use `device_id()` sanitization and persist in `SystemConfig`. |
| **Device Selection Persistence** | Audio device choice persists across app restarts in `system_config.json`. | "Configuration persistence in `system_config.json`." | `SystemConfig` lacked `audio_output_device` / `audio_input_device` fields. | **FIXED**: Added serde-defaulted device fields to `SystemConfig`. |
| **Key Sync Default** | RAW playback by default; Key Sync engages only on explicit SYNC latch. | "RAW by default. Key shift off unless SYNC pressed." | Code transposed by -5 semitones towards hardcoded C if no master key was set. | **DOCUMENTED / FIXED**: Harmonic shift returns 0 if reference is unknown. |
| **Summing Processor Saturation** | Ceiling limiter on Master bus only. | "Master bus brickwall limiter." | `SummingProcessor` had `soft_clip: true` on ALL 7 summing nodes, causing 1.23% THD+N. | **DOCUMENTED / FIXED**: `soft_clip` disabled on channel summing nodes. |
| **Crossfader Curve** | Continuous blend from Linear (0.0) to Constant-Power (1.0). | "0.0 = Linear, 1.0 = Power." | Binary threshold `if curve > 0.5`. Default 0.5 sat right on boundary. | **DOCUMENTED / FIXED**: Continuous gain law blend implemented; default constant power. |

---
