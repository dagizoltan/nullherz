# Nullherz System Architecture: Hardened Modular Engine & Transfusion DSP

**Author:** Senior Lead Audio & Rust Systems Architect
**Status:** PRODUCTION READY / HARDENED / MODULARIZED
**Date:** June 19, 2024

---

## 1. Architectural Overview

The Nullherz engine has reached its **Architectural Maturity Phase**. We have successfully decoupled the monolithic real-time processing loop from the high-level orchestration plane. The system is built upon a **Triple-Plane Model**:

1.  **The Orchestration Plane (`nullherz-conductor`)**: A hierarchical management layer responsible for lifecycle, topology, and global resource management.
2.  **The Protocol Plane (`ipc-layer`, `nullherz-traits`)**: A lock-free, zero-allocation communication interface.
3.  **The Execution Plane (`audio-core`, `audio-dsp`)**: A strictly static, allocation-free processing kernel that executes the signal graph with SIMD-accelerated precision.

---

## 2. Advanced Core Modularization

### 2.1 Audio Engine: Delegated Static Execution
The `AudioEngine` (`crates/audio-core`) is now a coordinator of specialized handlers:
*   **Trait-Based `ProcessingKernel`**: The processing logic is now polymorphic. The system defaults to `StandardKernel` but is architected to support `OverSamplingKernel` or `SafetyKernel` hot-swapping at runtime.
*   **`EngineInputHandler`**: Synchronously processes command, MIDI, and topology streams before each block.
*   **`ResourceRecycler`**: Lock-free offloading of object destruction.
*   **`nullherz-dna` Service**: The `SampleRegistry` is extracted into a dedicated crate, providing a SWMR (Single-Writer-Multiple-Reader) repository for shared audio DNA.

### 2.2 Conductor: Multi-Manager Orchestration
*   **`TopologyManager`**: Manages the signal graph. Foundation for **Declarative Topology** is established in `nullherz-topology` via the `GraphReconciler`.
*   **`Orchestrator`**: Includes non-RT cyclical evolution polling, safely pulling snapshots from the Engine and committing them to the DNA registry.

---

## 3. Transfusion & Evolution Layer Status

| Layer | Component | Status | Implementation Note |
| :--- | :--- | :--- | :--- |
| **Granular Transfusion** | `GranularProcessor` | **Hardened** | 32-voice granular engine with a fixed-size 16-slot source pool. Randomized jitter and selectable windowing. |
| **Spectral Transfusion** | `SpectralMorph` | **Hardened** | magnitude-domain cross-synthesis via reusable `SpectralPipeline`. |
| **Cyclical Evolution** | `CaptureNode` | **Hardened** | Circular write-buffer with polled non-RT snapshotting. |
| **Plugin Ecosystem** | `Modulation` | **Integrated** | CV-to-Command bridge with deterministic 1-block delay. |
| **Rehabilitation of Errors**| Quality Dials | **Operational** | creative control over interpolation order and window shapes. |

---

## 4. Real-Time Safety & Performance Audit

### 4.1 RT Invariants (Verified)
- **Zero Heap Allocation**: verified that no `Vec::new`, `Vec::push`, or `Arc::new` occur on the audio thread. `ProcessorGraph` uses fixed-size buffering for pending mutations.
- **Lock-Free Read Path**: Engine only performs atomic read operations. Registration is restricted to the Conductor.
- **Transport Agnostic**: `AudioEngine` is decoupled from concrete IPC types via `MidiConsumer`, `TopologyMutationConsumer`, and `CommandBundleConsumer` traits.
- **Conductor Decoupling**: Orchestration is decoupled from processor internals via the `pull_all_snapshots` interface.
- **SIMD Alignment**: All buffers utilize 64-byte alignment.

### 4.2 Conformance Audit
The entire suite of 14 processors passes the **Nullherz Conformance Suite**:
- **Sub-block Consistency**: bit-exact output regardless of block splitting.
- **Reset Determinism**: correct state clearing.
- **Parameter Stability**: verified resilience against NaN, Infinity, and extreme parameter values.
- **Snapshot Safety**: verified that `CaptureProcessor` and graph-wide snapshot pulling are thread-safe and RT-compliant.

---

## 5. Conclusion

The system is stable, modular, and fulfills the "Transfusion" requirements with production-grade reliability. The extraction of `nullherz-dna` and the trait-based kernel system prepare the engine for commercial-scale deployment.

**Architecture Status:** Commit-Ready / Production-Hardened.
