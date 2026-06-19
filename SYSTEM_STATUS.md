# Nullherz System Architecture: Hardened Modular Engine & Transfusion DSP

**Author:** Senior Lead Audio & Rust Systems Architect
**Status:** PRODUCTION READY / HARDENED
**Date:** June 19, 2024

---

## 1. Architectural Overview

The Nullherz engine has reached its **Architectural Maturity Phase**. We have successfully decoupled the monolithic real-time processing loop from the high-level orchestration plane. The system is built upon a **Triple-Plane Model**:

1.  **The Orchestration Plane (`nullherz-conductor`)**: A hierarchical management layer responsible for lifecycle, topology, and global resource management (the "Blood Bank" of audio DNA).
2.  **The Protocol Plane (`ipc-layer`, `nullherz-traits`)**: A lock-free, zero-allocation communication interface that ensures sample-accurate synchronization between the host and processing units.
3.  **The Execution Plane (`audio-core`, `audio-dsp`)**: A strictly static, allocation-free processing kernel that executes the signal graph with SIMD-accelerated precision.

---

## 2. Hardened Core Modularization

### 2.1 Audio Engine: Delegated Static Execution
The `AudioEngine` (`crates/audio-core`) has been refactored into a coordinator of static, specialized handlers to ensure deterministic real-time behavior:

*   **`ProcessingKernel`**: The purely functional core. It implements sample-accurate sub-block splitting, ensuring that control commands (parameters, routing) are applied at the exact intended sample, enabling stable late-bound feedback loops.
*   **`EngineInputHandler`**: The system gatekeeper. It processes the command bus, MIDI, and topology mutations *synchronously* at the start of the block, protecting the processing loop from external jitter.
*   **`ResourceRecycler`**: A lock-free component that offloads the destruction of retired processors and command buffers to non-RT threads, eliminating "Deallocation Spikes."
*   **`SampleRegistry`**: A high-performance repository for shared audio buffers. It employs a **Single-Writer-Multiple-Reader (SWMR)** pattern where the Conductor (non-RT) registers new DNA, and the Engine (RT) performs atomic clones of `Arc<Vec<f32>>` for synthesis.

### 2.2 Conductor: Multi-Manager Orchestration
The `Conductor` (`crates/nullherz-conductor`) has been refactored into specialized delegates:
*   **`EngineCoordinator`**: Manages backend driver abstraction (JACK, PipeWire, ALSA).
*   **`TopologyManager`**: Maintains the logical graph and translates high-level mixer actions into atomic topology mutations.
*   **`SidecarSupervisor`**: Monitors out-of-process nodes with automated recovery and "Zombie Reaping" logic.

---

## 3. "Transfusion" DSP Layer Implementation

We have fully implemented the requested DSP theory layers using hardened, RT-safe patterns:

| Layer | Component | Implementation Note |
| :--- | :--- | :--- |
| **Granular Transfusion** | `GranularProcessor` | 32-voice granular engine with a **fixed-size 16-slot source pool**. Randomized grain scheduler with randomized position/pitch jitter and selectable (Hann/Triangle/Square) windowing. |
| **Spectral Transfusion** | `SpectralMorph` | Built on a reusable `SpectralPipeline` component. Performs magnitude-domain cross-synthesis. Modulation envelope is extracted via an optimized sliding-window average kernel. |
| **Cyclical Evolution** | `CaptureNode` | Circular write-buffer with a "Freeze" flag. The **Conductor pulls the snapshot non-RT**, wraps it in an `Arc`, and registers it globally as new DNA for the Granular pool. |
| **Plugin Ecosystem** | `Modulation` | The CV-to-Command bridge is now fully integrated. It leverages the engine's feedback command bus to enable cross-node modulation with 1-block deterministic delay. |
| **Rehabilitation of Errors**| Quality Dials | Kernels now expose creative `Quality` parameters, allowing users to toggle between raw/aliased (Linear) and high-fidelity (4-point Lagrange) interpolation. |

---

## 4. Real-Time Safety & Performance Audit

### 4.1 RT Invariants (Verified)
- **Zero Heap Allocation**: All `Vec::new`, `Vec::push`, and `Arc::new` operations are strictly forbidden on the audio thread.
- **Lock-Free Read Path**: Audio thread only acquires read-access or performs atomic pointer swaps. Writing is restricted to the non-RT Orchestration Plane.
- **SIMD-Aligned Processing**: All internal buffers utilize 64-byte alignment, supporting AVX-512 and NEON intrinsics.

### 4.2 Conformance Testing
The entire suite of 14 processors has passed the **Nullherz Conformance Suite**:
- **Sub-block Consistency**: Bit-exact output across varying block sizes.
- **Reset Determinism**: Correct state clearing for deterministic synthesis.
- **Topology Stability**: Hazardous mutations (cycles) are rejected; valid mutations are applied glitch-free.

---

## 5. Conclusion

The system is now stable, modular, and fulfills the "Transfusion" requirements with production-grade reliability. The architecture is specifically designed to allow future expansion (e.g., more complex spectral bin-manipulation or neural sidecars) without compromising the integrity of the core real-time engine.

**Architecture Status:** Commit-Ready / Production-Hardened.
