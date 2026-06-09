# nullherz Production Roadmap: Path to 1.0

This document outlines the evolutionary phases of the **nullherz** deterministic real-time audio engine, moving from its optimized core towards a production-ready ecosystem.

---

## ✅ Phase 1 & 2: Architectural Foundations (Completed)
*   **Explicit SIMD DSP**: Implementation of Biquad filters for AVX2, AVX-512, and ARM Neon.
*   **Dynamic Patching**: Sample-accurate rewiring with click-free crossfading and zero-allocation cycle detection.
*   **Multi-Core Scheduling**: Topological stage grouping and pre-allocated `TaskPool` for parallel node execution.
*   **Autonomous Sidecars**: `SidecarManager` with heartbeat watchdog and automated SHM/resource orchestration.
*   **Production State**: History-based undo/redo, command bundling, and manual JSON serialization.
*   **Developer Tooling**: `#[sidecar]` DSL macro and `nullherz-inspector` CLI/GUI foundations.
*   **Deep Linux Integration**: Automated RT-Cgroup migration, CPU affinity (pinning), and Hot-Swap safety.
*   **Hardened RT Core**: Race-free generation-based TaskPool, sample-accurate sidecar watchdogs, and RAII-aligned memory safety.

---

## 🚀 Phase 3: Native Linux Deep Integration (Current Focus)
*   **Native PipeWire SPA Backend**: Finalize the SPA protocol implementation to allow zero-copy buffer sharing directly with the PipeWire daemon.

---

## 📊 Phase 4: System Visibility & Telemetry
*   **Telemetry Dashboard**: Expand the graphical inspector to show real-time CPU micro-load per node, signal levels in all buffers, and xrun locations.
*   **Networked Control Plane**: Implement a lock-free gRPC or WebWS bridge for controlling the engine from remote devices or web interfaces.
*   **Topology Optimizer**: Automated graph refactoring based on micro-benchmarking data to minimize cache misses and buffer copies.

---

## 🎹 Phase 5: DSP Standard Library Expansion
*   **Spectral Engine**: SIMD-optimized FFT nodes for high-quality convolution reverb and spectral filtering.
*   **Advanced Modulation Matrix**: A high-level abstraction for mapping any engine buffer to node parameters via virtual patch cords.
*   **Wavetable Synthesis**: Production-grade SIMD wavetable oscillators with audio-rate FM/PM.

---

## 🛠 Phase 6: Stability & Hardening (Path to 1.0)
*   **Cross-Process Safety**: Hardened shared memory primitives with versioned heartbeat stall detection.
*   **Fuzz Testing**: Continuous fuzzing of the command processor and IPC layer to ensure absolute stability under adversarial input.
*   **Formal Verification**: Investigating formal verification for core synchronization primitives in the `ipc-layer`.
*   **Documentation & SDK**: Comprehensive API documentation and tutorial suite for building sidecar effects.

---

### Production Commitments
- **Zero Heap Allocation** in the real-time path.
- **Lock-Free** synchronization between execution domains.
- **Deterministic** behavior with sample-accurate automation.
- **Dependency-Minimal** core for maximum portability.
