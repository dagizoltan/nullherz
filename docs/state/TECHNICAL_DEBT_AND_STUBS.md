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
- **Decoupled Synchronization**: [RESOLVED] Replaced standard library blocking and lock-poisoning `std::sync::Mutex` with high-performance `parking_lot::Mutex` across UI and rendering components to comply with real-time safety lints and avoid poisoning states.
- **Gossip Protocol and Signatures**: [RESOLVED] Replaced unsigned gossip stub with secure cryptographic GOSSIP_SIGNED payload validation and local network provider sync test suite.

---

## 2. UI & Telemetry Gaps

- **Breeder View Visualization**: [PARTIAL] "Visual Transfusion Progress Bar" is tied to active DNA blend operations but could benefit from sub-block pipeline progress telemetry.

---

## 3. Orchestration Plane (`nullherz-conductor`)

### `src/orchestrator.rs`
- **Line 537**: [RESOLVED] DNA suggestion logic strictly binds to the `active_master_deck` state.

---

## 3. Protocol & DNA Plane (`nullherz-dna`, `nullherz-traits`)

### `nullherz-dna/src/lib.rs`
- **Gossip Protocol**: [RESOLVED] PeerSync gossip-overlay TCP network engine implemented.
- **Genetic Authority**: [RESOLVED] Cryptographically signed sound DNA lineages tracked with consensus checking.

---

## 4. Execution Plane (`audio-dsp`, `nullherz-processors`)

### `nullherz-processors/src/spectral.rs`
- **Boundary Handling**: [STUB] Still needs hardening for arbitrary, non-power-of-two block sizes in the spectral domain.

---

## 5. Sidecar & Runtime (`fx-runtime`, `sidecar-sdk`, `sidecar-macros`)

### `fx-runtime/src/wasm_runtime.rs`
- **SHM Host Exports**: [RESOLVED] Direct zero-copy memory-mapping getters (`get_shared_command_buffer_ptr`, etc.) integrated to satisfy guest plugin SDK standards.

---

## 6. Strategic Technical Debt

### Distributed Networking
- **Jitter Resilience**: [HARDENED] Jitter Buffer now implements aggressive clock recovery.
- **RDMA Path**: [RESEARCH] Zero-copy RDMA return path for distributed AudioBlocks remains a long-term research goal.

### Intelligence Plane
- **DNA-Aware Sequencing**: [PLANNED] Real-time mutation of MIDI patterns based on Rhythmic DNA is mapped out.
