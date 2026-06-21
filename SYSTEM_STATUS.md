# Nullherz System Architecture: Lead Architect's Comprehensive Report

**Author:** Senior Lead Audio & Rust Systems Architect
**Status:** PRODUCTION READY / HARDENED / DECOUPLED
**Date:** June 20, 2026

---

## 1. Architectural Overview: The Triple-Plane Model

The Nullherz engine is built upon a strict separation of concerns, ensuring that high-level management never interferes with real-time signal processing.

### 1.1 The Orchestration Plane (`nullherz-conductor`)
*   **Responsibility**: Lifecycle management, declarative topology reconciliation, and global resource coordination.
*   **Off-Thread Compilation**: `TopologyManager` now performs expensive topological analysis (Kahn's algorithm) off the audio thread, injecting pre-compiled `GraphTopology` structures via the `SetTopology` mutation.
*   **Decoupling**: Interacts with the execution plane exclusively through `RenderingEngine` and `RenderingController` trait objects.

### 1.2 The Protocol Plane (`ipc-layer`, `nullherz-traits`)
*   **Responsibility**: Zero-allocation, lock-free communication between all planes.
*   **Broadcaster Telemetry**: `nullherz-gateway` utilizes a non-blocking broadcaster pattern, allowing multiple monitoring clients (Dashboards/Inspectors) to receive the same telemetry stream without frame competition.
*   **SIMD Foundation**: Enforces 64-byte alignment and provides the `AudioBlock` primitives used throughout the execution plane.

### 1.3 The Execution Plane (`audio-core`, `audio-dsp`)
*   **Responsibility**: Low-latency, bit-exact audio processing.
*   **Static Graph Execution**: The `ProcessorGraph` acts as a lightweight VM. By utilizing `SetTopology`, structural shifts are O(1) pointer swaps, guaranteeing zero-jitter transitions even for complex graphs.

---

## 2. Advanced Core Invariants

### 2.1 Real-Time Safety & Performance
- **Lock-Free Sample Access**: `SampleRegistry` refactored to use an atomic-swap pattern, ensuring the RT thread never blocks on a `RwLock` during transfusion sourcing.
- **Zero Heap Allocation**: No `Vec::new`, `Vec::push`, or `Arc::new` occur on the audio thread.
- **CPU Hardening**: FTZ/DAZ enabled globally to prevent denormal-induced CPU spikes.
- **Atomic Topology**: Structural shifts (AddNode/SwapProcessor) are now fully buffered and committed atomically, preventing temporary signal graph inconsistencies.

### 2.2 Fault Tolerance & Signal Stability
- **Sidecar Resilience**: `SidecarSupervisor` tracks `node_idx` state, ensuring failed DSP sidecars are restored to their correct topological position.
- **System-Wide Safe Mode**: Sidecar failures can now trigger a global "Safe Mode" via the command bus, allowing the engine to enter a known stable state.
- **RSS Limits**: Sidecar subprocesses are now constrained by real RSS memory limits using cgroups.
- **DSP Safety Pass**: All critical kernels (Gain, Biquad, Spectral) are hardened against non-finite float values.

---

## 3. Testing & Verification Infrastructure

### 3.1 Nullherz Conformance Suite
The entire suite of registered processors (including `DjIsolator` and `SimdBiquad`) is validated against:
- **Sub-block Consistency**: bit-exact output regardless of sub-block boundaries.
- **Reset Determinism**: verified silence and state clearing on `reset()`.
- **Parameter Reachability**: verified that all published parameters are reachable via the global command bus.
- **Snapshot Safety**: verified that capture/pull operations are RT-safe and consistent.

### 3.2 Automated Reconciliation
- **Declarative Topology**: `nullherz-topology` provides verified reconciliation logic that calculates minimal atomic mutations to reach a desired graph state.

---

## 4. Conclusion

The Nullherz engine is now fully decoupled and real-time hardened. It achieves total isolation between hardware backends and processing logic, while ensuring that control-plane complexity never leaks into the high-priority signal path.

**Architecture Status:** PRODUCTION-READY.
