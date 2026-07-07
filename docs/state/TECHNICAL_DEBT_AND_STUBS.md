# Nullherz Technical Debt & Stubs Report (Updated July 7, 2026)

This document tracks remaining stubs and prototype logic. Recent hardening has addressed several core issues.

---

## 1. Resolved Items (Hardened)

- **Orchestrator Calibration**: [RESOLVED] Dynamic calculation based on engine sample rate implemented.
- **Remote Audio Send**: [RESOLVED] Refactored from per-block `tokio::spawn` to efficient batching.
- **Isolator Filters**: [OPTIMIZED] Implemented 4x unrolled kernels and **exact Linkwitz-Riley coefficient generation**.
- **Offline Rendering**: [RESOLVED] Replaced `unsafe` pointer hack with safe mutable access in `bounce.rs`.
- **DNA Mutation Targeting**: [RESOLVED] Replaced first-ID heuristic with precise `resource_id` resolution.
- **UI Placeholders**: [RESOLVED] Account and Metrics views now utilize live telemetry instead of mocks.
- **Waveform Rendering**: [OPTIMIZED] Implemented precise LOD selection in `waveform_renderer.rs`.
- **DNA Transfusion Builder**: [RESOLVED] Implemented `DnaCommand::pack_transfusion` to eliminate unsafe byte-packing in the Breeder view.

---

## 2. UI & Telemetry Gaps

- **Breeder View Visualization**: [PARTIAL] "Visual Transfusion Progress Bar" may not be tied to actual kernel progress if multi-block.
- **mDNS Discovery Feedback**: [STUB] `SYNC DNA` button in `account.rs` lacks the backend integration for mDNS/TCP sync triggers.

---

## 3. Orchestration Plane (`nullherz-conductor`)

### `src/orchestrator.rs`
- **Line 537**: [RESOLVED] DNA suggestion logic strictly binds to the `active_master_deck` state.

---

## 3. Protocol & DNA Plane (`nullherz-dna`, `nullherz-traits`)

### `nullherz-dna/src/lib.rs`
- **Gossip Protocol**: [PLANNED] `PeerSync` logic for real-time DNA template exchange is in placeholder status.
- **Genetic Authority**: [PLANNED] Lineage tracking for bred sounds requires consensus implementation.

---

## 4. Execution Plane (`audio-dsp`, `nullherz-processors`)

### `nullherz-processors/src/spectral.rs`
- **Boundary Handling**: [STUB] Still needs hardening for arbitrary block sizes beyond 256 samples in the spectral domain.

---

## 5. Sidecar & Runtime (`fx-runtime`, `sidecar-sdk`, `sidecar-macros`)

### `fx-runtime/src/wasm_runtime.rs`
- **SHM Host Exports**: [PARTIAL] Memory mapping for guest plugins is implemented but needs high-throughput verification.

---

## 6. Strategic Technical Debt

### Distributed Networking
- **Jitter Resilience**: [HARDENED] Jitter Buffer now implements aggressive clock recovery.
- **RDMA Path**: [RESEARCH] Zero-copy RDMA return path for distributed AudioBlocks remains a long-term research goal.

### Intelligence Plane
- **DNA-Aware Sequencing**: [PLANNED] Mutation of MIDI patterns based on Rhythmic DNA is not yet implemented.

---

## 7. Strategic Documentation

### `NEXT_SESSION_PROMPT.md`
- **Line 18**: `- **InfiniBand/RDMA**: Research and prototype...` - RDMA networking remains a research task.
