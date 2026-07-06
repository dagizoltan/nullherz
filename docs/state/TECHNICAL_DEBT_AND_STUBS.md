# Nullherz Technical Debt & Stubs Report (Updated June 25, 2026)

This document tracks remaining stubs and prototype logic. Recent hardening has addressed several core issues.

---

## 1. Resolved Items (Hardened)

- **Orchestrator Calibration**: [RESOLVED] Prototype hardcoded 441 samples replaced with dynamic calculation based on engine sample rate.
- **Remote Audio Send**: [RESOLVED] Refactored from per-block `tokio::spawn` to efficient batching.
- **Isolator Filters**: [OPTIMIZED] Implemented 4x unrolled kernels for `DjIsolator`.
- **UI Placeholders**: [IMPROVED] Enhanced empty deck states in `dj_studio.rs`.

---

## 2. Orchestration Plane (`nullherz-conductor`)

### `src/orchestrator.rs`
- **Line 522**: `// For this prototype, we'll try to use the first registered ID if available.` - Still using heuristic for DNA evolution targeting.

### `src/bounce.rs`
- **Line 53**: `// We'll use a hack for the prototype: we'll call process_block directly...` - Non-RT offline rendering hack still present.

---

## 3. Protocol & DNA Plane (`nullherz-dna`, `nullherz-traits`)

### `nullherz-dna/src/lib.rs`
- **Line 219**: `// Breeding logic or library integration here...` - Missing implementation for inherited DNA.
- **Line 244**: `// Stub for P2P discovery logic (libp2p/mdns)` - Placeholder for federated genetic cloud discovery.

---

## 4. Execution Plane (`audio-dsp`, `nullherz-processors`)

### `nullherz-processors/src/spectral.rs`
- **Line 16**: `// For prototype, we ensure lengths match.` - Simplification in spectral processing.

### `audio-dsp/src/filters.rs`
- **Note**: Linkwitz-Riley coefficients are still marked as "Approximate" in `DjIsolator::new()`.

---

## 5. Sidecar & Runtime (`fx-runtime`, `sidecar-sdk`, `sidecar-macros`)

### `fx-runtime/src/wasm_runtime.rs`
- **Line 26**: `// In a real implementation, we'd copy the command into WASM memory` - `pop_command` host function is a stub.

### `sidecar-macros/src/lib.rs`
- **Line 21**: `// Simplified parsing for macro prototype` - Macro DSL parsing is not robust.

### `sidecar-sdk/src/lib.rs`
- **Line 175**: `// In a real kernel, this would involve a delay line or sample shift` - `apply_rhythmic_offset` is a stub.

---

## 6. Strategic Documentation

### `NEXT_SESSION_PROMPT.md`
- **Line 18**: `- **InfiniBand/RDMA**: Research and prototype...` - RDMA networking remains a research task.
