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
- **Line 537**: [RESOLVED] DNA suggestion logic now strictly binds to the `active_master_deck` state.

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
- **Line 21**: [RESOLVED] CLI argument parsing refactored to a structured iterator-based matcher for robust SHM and EventFD extraction.

### `sidecar-sdk/src/lib.rs`
- **Line 175**: [RESOLVED] `apply_rhythmic_offset` implements sub-sample accurate jitter using linear interpolation.

---

## 6. UI & Inspector Plane (`nullherz-inspector`, `nullherz-ui-hal`)

### `crates/nullherz-inspector/src/views/dj_studio.rs`
- **Monolithic Deck Rendering**: [RESOLVED] Refactored into `render.rs`, `mixer.rs`, and `dna.rs` modules.
- **Ergonomic Inconsistency**: [RESOLVED] DNA traits migrated to industrial knobs.
- **Metadata Omission**: [RESOLVED] BPM and Key metadata now displayed in deck headers.

### `crates/nullherz-inspector/src/views/sampler.rs`
- **WGPU Callback Lifetime Safety**: [HARDENED] Added strict documentation and verified synchronous pointer-cast safety.

### `crates/nullherz-inspector/src/views/composer.rs`
- **Monolithic Grid Loop**: [OPTIMIZED] Implemented horizontal scrolling and viewport-based column culling for the sequencer grid.

### `crates/nullherz-inspector/src/views/topology.rs`
- **Simplified Cable Model**: [HARDENED] Cable rendering now uses deterministic routing data from `GraphTopology`.

### `crates/nullherz-inspector/src/views/breeder.rs`
- **Manual Command Packing**: `emit_dna_command` uses `unsafe` `ptr::copy_nonoverlapping` to pack SoundDNA into the 128-byte `DnaCommand` payload. Requires a zero-allocation `CommandBuilder` utility in `nullherz-traits` to eliminate `unsafe` and manual offsets.
- **Local Visualization Smoothing**: [RESOLVED] Breeder view now utilizes shared `damped_goniometer` from `InspectorApp`.

---

## 7. Strategic Technical Debt

### Persistence & Serialization
- **Zero-Copy Migration**: `ProjectState` currently uses Bincode/JSON which requires full deserialization. Transition to `rkyv` is required for zero-copy session loading on the audio thread.
- **Modular Persistence**: [RESOLVED] Persistence logic fully decoupled from the main Conductor orchestrator.

### UI Architecture
- **Component Decomposition**: [RESOLVED] Monolithic UI views (DJ Studio) refactored into modular sub-modules (`render`, `mixer`, `dna`, etc.) and specialized widgets.
- **Topology Routing**: [HARDENED] Cable rendering and buffer resolution now synchronized with `GraphTopology` routing, featuring real-time signal-aware coloring.
- **Grid Performance**: [OPTIMIZED] Sequencer grid now utilizes spatial culling and horizontal scrolling for high-density performance.
- **Dynamic Modulation**: [HARDENED] Modulation matrix now dynamically resolves targets from the active signal graph with per-macro scaling knobs.
- **Genetic Visualization**: [ENHANCED] Library view now features SoundDNA trait sparklines for rapid genetic profiling.
- **Interaction Hardening**: [RESOLVED] Topology view now supports interactive disconnects.

---

## 8. Strategic Technical Debt (Stage 2)

### Distributed Networking
- **Jitter Resilience**: [HARDENED] Jitter Buffer now implements aggressive clock recovery.
- **Service Isolation**: [RESOLVED] Sidecar discovery split into independent watcher and P2P sync tasks.

### Intelligence Plane
- **DNA Mutation Safety**: [HARDENED] `FeatureMutator` implemented with strict latent space clamping.

---

## 9. Strategic Documentation

### `NEXT_SESSION_PROMPT.md`
- **Line 18**: `- **InfiniBand/RDMA**: Research and prototype...` - RDMA networking remains a research task.
