# Nullherz Technical Debt & Stubs Report (Updated June 25, 2026)

This document tracks remaining stubs and prototype logic. Recent hardening has addressed several core issues.

---

## 1. Resolved Items (Hardened)

- **Orchestrator Calibration**: [RESOLVED] Prototype hardcoded 441 samples replaced with dynamic calculation based on engine sample rate.
- **Remote Audio Send**: [RESOLVED] Refactored from per-block `tokio::spawn` to efficient batching.
- **Isolator Filters**: [OPTIMIZED] Implemented 4x unrolled kernels and **exact Linkwitz-Riley coefficient generation**.
- **Offline Rendering**: [RESOLVED] Replaced `unsafe` pointer hack with safe mutable access in `bounce.rs`.
- **DNA Mutation Targeting**: [RESOLVED] Replaced first-ID heuristic with precise `resource_id` resolution in `orchestrator.rs`.
- **UI Placeholders**: [IMPROVED] Enhanced empty deck states in `dj_studio.rs`.
- **Waveform Rendering**: [OPTIMIZED] Implemented precise LOD selection in `waveform_renderer.rs`.

---

## 2. Orchestration Plane (`nullherz-conductor`)

### `src/orchestrator.rs`
- **Line 537**: `// For this prototype, we'll try to use the first registered ID if available.` - Heuristic-based DNA suggestion logic for master track identification. Needs robust `MasterDeck` state tracking.

---

## 3. Protocol & DNA Plane (`nullherz-dna`, `nullherz-traits`)

### `nullherz-dna/src/lib.rs`
- **Line 219**: `// Breeding logic or library integration here...` - Missing implementation for inherited DNA.
- **Line 261**: [RESOLVED] Skeleton UDP broadcast replaced with robust `mdns-sd` discovery service.

---

## 4. Execution Plane (`audio-dsp`, `nullherz-processors`)

### `nullherz-processors/src/spectral.rs`
- **Line 16**: [STRATEGY DEFINED] Handle mismatched buffer boundaries via zero-padding surrogate (prototype implemented). Needs hardening for arbitrary block sizes beyond 256 samples.

---

## 5. Sidecar & Runtime (`fx-runtime`, `sidecar-sdk`, `sidecar-macros`)

### `fx-runtime/src/wasm_runtime.rs`
- **Line 32**: [RESOLVED] `bincode::serialize(&cmd)` refactored to `serialize_into` with a stack-allocated buffer.

### `sidecar-macros/src/lib.rs`
- **Line 21**: `// Simplified parsing for macro prototype` - Sidecar initialization relies on manual CLI argument iteration. Needs a formal attribute parser for robust SHM and EventFD configuration.

### `sidecar-sdk/src/lib.rs`
- **Line 175**: [STRATEGY DEFINED] `apply_rhythmic_offset` utilizes `apply_rhythmic_grid` delay line logic. Needs verification against sub-sample accuracy requirements.

---

## 6. UI & Inspector Plane (`nullherz-inspector`, `nullherz-ui-hal`)

### `crates/nullherz-inspector/src/views/dj_studio.rs`
- **Monolithic Deck Rendering**: `render_deck_card` manages too many responsibilities (Transport, Performance, DNA, Mixer). Requires decomposition into sub-component functions for better testability and maintenance.
- **Ergonomic Inconsistency**: DNA/Personality traits use standard `egui::Slider` while frequency bands use industrial `knobs`.
- **Metadata Omission**: Deck headers do not currently display BPM or Root Key from `SampleMetadata`, forcing users to rely on the global telemetry header.

### `crates/nullherz-inspector/src/views/sampler.rs`
- **WGPU Callback Lifetime Safety**: Use of `std::mem::transmute` in `WaveformCallback` to satisfy WGPU 'a lifetimes is a potential UB risk if the renderer is dropped while the pass is active. Requires a safer resource management pattern.

### `crates/nullherz-inspector/src/views/composer.rs`
- **Monolithic Grid Loop**: Renders 16x64 (1024) interactive rectangles in a single flat loop. Needs a batched mesh approach or sub-grid culling to maintain UI performance at 60fps as session complexity grows.

### `crates/nullherz-inspector/src/views/topology.rs`
- **Simplified Cable Model**: Connection cables assume `buffer_idx = node_idx + 10`. This logic is brittle and breaks if the topology uses arbitrary buffer indices. Requires proper `GraphTopology` traversal.

### `crates/nullherz-inspector/src/views/breeder.rs`
- **Manual Command Packing**: `emit_dna_command` uses `unsafe` `ptr::copy_nonoverlapping` to pack SoundDNA into the 128-byte `DnaCommand` payload. Requires a zero-allocation `CommandBuilder` utility in `nullherz-traits` to eliminate `unsafe` and manual offsets.
- **Local Visualization Smoothing**: `smoothed_goniometer` is stored locally in `BreederView` instead of utilizing the shared `nullherz-ui-hal` ballistics, leading to inconsistent visual damping.

---

## 7. Strategic Technical Debt

### Persistence & Serialization
- **Zero-Copy Migration**: `ProjectState` currently uses Bincode/JSON which requires full deserialization. Transition to `rkyv` is required for zero-copy session loading on the audio thread.

### UI Architecture
- **Component Decomposition**: `dj_studio.rs` remains monolithic. Requires extraction of `DeckHeader`, `WaveformZone`, and `MixerStrip` into independent, testable components.

---

## 8. Strategic Documentation

### `NEXT_SESSION_PROMPT.md`
- **Line 18**: `- **InfiniBand/RDMA**: Research and prototype...` - RDMA networking remains a research task.
