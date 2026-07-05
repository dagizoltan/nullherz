# Nullherz Technical Debt & Stubs Report

This document identifies existing stubs, placeholders, and prototype logic across the Nullherz codebase, categorized by crate.

---

## 1. Orchestration Plane (`nullherz-conductor`)

### `src/orchestrator.rs`
- **Line 336**: `// Prototype calibration: assume 10ms (441 samples)` - Hardcoded latency calibration for hardware.
- **Line 484**: `// --- STAGE 4 PHASE C: REMOTE AUDIO SEND PROTOTYPE ---` - Prototype logic for distributed audio.
- **Line 490**: `// For now, we prototype by pulling from the bridge if a block is waiting` - Distributed routing prototype.

### `src/bounce.rs`
- **Line 53**: `// We'll use a hack for the prototype: we'll call process_block directly...` - Non-RT offline rendering hack.

---

## 2. Protocol & DNA Plane (`nullherz-dna`, `nullherz-traits`)

### `nullherz-dna/src/lib.rs`
- **Line 219**: `// Breeding logic or library integration here...` - Missing implementation for inherited DNA.
- **Line 244**: `// Stub for P2P discovery logic (libp2p/mdns)` - Placeholder for federated genetic cloud discovery.

---

## 3. Execution Plane (`audio-dsp`, `nullherz-processors`)

### `nullherz-processors/src/spectral.rs`
- **Line 16**: `// For prototype, we ensure lengths match.` - Simplification in spectral processing.

### `audio-dsp/src/filters.rs` (via `DjIsolator`)
- **Note**: Linkwitz-Riley coefficients are marked as "Approximate".

---

## 4. Sidecar & Runtime (`fx-runtime`, `sidecar-sdk`, `sidecar-macros`)

### `fx-runtime/src/wasm_runtime.rs`
- **Line 26**: `// In a real implementation, we'd copy the command into WASM memory` - `pop_command` host function is a stub.
- **Whole module**: Marked as a "foundation" for Stage 6 Universal Extensibility.

### `sidecar-macros/src/lib.rs`
- **Line 21**: `// Simplified parsing for macro prototype` - Macro DSL parsing is not robust.

### `sidecar-sdk/src/lib.rs`
- **Line 175**: `// In a real kernel, this would involve a delay line or sample shift` - `apply_rhythmic_offset` is a stub.

---

## 5. UI & Integration (`nullherz-inspector`, `nullherz-backends`)

### `nullherz-inspector/src/views/dj_studio.rs`
- **Line 56**: `// Visuals Area (Waveform / Spectrum placeholder)` - Missing real-time waveform visualization in DJ view.

### `nullherz-backends/src/pipewire.rs`
- **Line 180**: `// size placeholder` - Pipewire buffer sizing logic incomplete.

---

## 6. Strategic Documentation

### `SOLUTION_DESIGN_OPTIMIZATION.md`
- **Line 50**: `2. **Prototype UI-HAL:** Refactor one widget...` - UI-HAL refactor is still in prototype stage.

### `NEXT_SESSION_PROMPT.md`
- **Line 18**: `- **InfiniBand/RDMA**: Research and prototype...` - RDMA networking remains a research task.
