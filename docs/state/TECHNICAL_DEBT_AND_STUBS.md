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
- **Line 16**: `// For prototype, we ensure lengths match.` - Fixed-length processing assumes input/output alignment. Missing zero-padding or resampling for mismatched buffer boundaries.

---

## 5. Sidecar & Runtime (`fx-runtime`, `sidecar-sdk`, `sidecar-macros`)

### `fx-runtime/src/wasm_runtime.rs`
- **Line 32**: [RESOLVED] `bincode::serialize(&cmd)` refactored to `serialize_into` with a stack-allocated buffer.

### `sidecar-macros/src/lib.rs`
- **Line 21**: `// Simplified parsing for macro prototype` - Sidecar initialization relies on manual CLI argument iteration. Needs a formal attribute parser for robust SHM and EventFD configuration.

### `sidecar-sdk/src/lib.rs`
- **Line 175**: `// In a real kernel, this would involve a delay line or sample shift` - `apply_rhythmic_offset` is a stub.

---

## 6. UI & Inspector Plane (`nullherz-inspector`, `nullherz-ui-hal`)

### `crates/nullherz-inspector/src/views/dj_studio.rs`
- **Monolithic Deck Rendering**: `render_deck_card` manages too many responsibilities (Transport, Performance, DNA, Mixer). Requires decomposition into sub-component functions for better testability and maintenance.
- **Ergonomic Inconsistency**: DNA/Personality traits use standard `egui::Slider` while frequency bands use industrial `knobs`.
- **Metadata Omission**: Deck headers do not currently display BPM or Root Key from `SampleMetadata`, forcing users to rely on the global telemetry header.

---

## 7. Strategic Documentation

### `NEXT_SESSION_PROMPT.md`
- **Line 18**: `- **InfiniBand/RDMA**: Research and prototype...` - RDMA networking remains a research task.
