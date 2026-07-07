# Nullherz System Architecture: Lead Architect's Comprehensive Report

**Author:** Senior Lead Audio & Rust Systems Architect
**Status:** HARDENED ALPHA (Active Development)
**Date:** 2026-07-07

---

## 1. Architectural Overview: The Triple-Plane Model

The Nullherz engine is built upon a strict separation of concerns, ensuring that high-level management never interferes with real-time signal processing.

### 1.1 The Orchestration Plane (`nullherz-conductor`)
*   **Responsibility**: Lifecycle management, declarative topology reconciliation, and global resource coordination.
*   **Off-Thread Compilation**: [VERIFIED] `TopologyManager` performs expensive topological analysis (Kahn's algorithm) off the audio thread (`crates/nullherz-conductor/src/topology_manager.rs:141`). Off-thread compilation is enforced for all commit paths; `ProcessorGraph::apply_command` in `audio-core` has been audited to remove the synchronous `calculate_stages` path.
*   **Decoupling**: Interacts with the execution plane exclusively through `RenderingEngine` and `RenderingController` trait objects.

### 1.2 The Protocol Plane (`ipc-layer`, `nullherz-traits`)
*   **Responsibility**: Zero-allocation, lock-free communication between all planes.
*   **Broadcaster Telemetry**: `nullherz-gateway` utilizes a non-blocking broadcaster pattern, allowing multiple monitoring clients (Dashboards/Inspectors) to receive the same telemetry stream without frame competition.
*   **SIMD Foundation**: Enforces 64-byte alignment and provides the `AudioBlock` primitives used throughout the execution plane.

### 1.3 The Execution Plane (`audio-core`, `audio-dsp`)
*   **Responsibility**: Low-latency, bit-exact audio processing.
*   **Static Graph Execution**: [VERIFIED] The `ProcessorGraph` acts as a lightweight VM. Structural shifts are O(1) pointer swaps via `SetTopology` (`crates/audio-core/src/processors/graph/mod.rs:114`), ensuring zero-jitter transitions. A regression test `test_rt_topology_commit_is_no_op` ensures the RT thread cannot be stalled by accidental topology recalculations.

---

## 2. Advanced Core Invariants

### 2.1 Real-Time Safety & Performance
- **Lock-Free Sample Access**: [VERIFIED] `SampleRegistry` uses an atomic-swap pattern (`crates/nullherz-dna/src/lib.rs:567`).
- **Zero Heap Allocation**: [PARTIAL] Audit of audio hot-paths indicates zero-allocation for core processors, but broad system-wide verification is pending a custom allocator/linter check.
- **CPU Hardening**: [VERIFIED] FTZ/DAZ enabled globally (`crates/ipc-layer/src/lib.rs:619`).
- **Atomic Topology**: [VERIFIED] Structural shifts are buffered and committed via `TopologyManager` off-thread.

### 2.2 Fault Tolerance & Signal Stability
- **Sidecar Resilience**: `SidecarSupervisor` tracks `node_idx` state, ensuring failed DSP sidecars are restored to their correct topological position.
- **Automated Soft Fallback**: Stalled heartbeats (>200ms) trigger an instant swap to a zero-overhead `FallbackProcessor` (Bypass) to maintain audio continuity.
- **System-Wide Safe Mode**: Sidecar failures can now trigger a global "Safe Mode" via the command bus, allowing the engine to enter a known stable state.
- **RSS Limits**: Sidecar subprocesses are now constrained by real RSS memory limits using cgroups.
- **DSP Safety Pass**: All critical kernels (Gain, Biquad, Spectral) are hardened against non-finite float values.

### 2.3 Hardware & Distributed Orchestration
- **Universal Backend Switching**: Real-time hot-swapping between ALSA, JACK, and Threaded backends without process restarts.
- **Latency Calibration**: Integrated RTL (Round Trip Latency) measurement routine to compensate for sidecar and hardware delays.
- **Targeted Distributed Routing**: Protocol Type 5 (Send) and Type 6 (UDP Return) enable efficient, low-jitter offloading of heavy DSP nodes to remote machines.
- **Thread Pinning**: RT threads are automatically pinned to performance cores via `sched_setaffinity` to maximize L3 cache locality.

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

## 4. DJ & Song Builder Roadmap: From Engine to Instrument

While the Nullherz engine is architecturally hardened, the transition to a "Valuable Instrument" requires bridging the gap between raw DSP and user-centric orchestration.

### 4.1 DJ Performance Readiness [100%]
*   **Current State**: MIXING READY. Logical deck addressing (A-D) and library-to-deck loading bridges are fully operational.
*   **Alpha Requirement**: [DONE] **Transient & BPM Analyzer**. Populates metadata for seamless "Sync" and "Snap".
*   **Alpha Requirement**: [DONE] **Library Database**. `redb` backend with Smart Crate trait-based filtering is now operational.

### 4.2 Song Builder Readiness [~85%]
*   **Current State**: Sample-accurate parameter automation, modular "Transfusion" layer, and a global Pattern Manager are operational.
*   **Alpha Requirement**: [DONE] **Project Persistence**. Implement a serialized `ProjectState` that captures the entire topology, sequence grid, and parameter set, allowing for session save/load cycles.
*   **Alpha Requirement**: [DONE] **Pattern Orchestration**. Move beyond the 16-step `SequencerProcessor` toward a "Pattern Manager" that can schedule complex arrangements on the `Timeline`.
*   **Alpha Requirement**: [DONE] **Macro Modulation**. Introduce a "Modulation Matrix" that allows high-level controls (Macro Knobs) to broadcast commands to multiple downstream DSP nodes simultaneously.

---

## 5. Subsystem Readiness Matrix

| Subsystem | Readiness | Evidence |
|-----------|-----------|----------|
| **Core Engine** | Hardened Alpha | `crates/audio-core/src/integration_tests.rs` |
| **Topology Manager** | Hardened Alpha | `crates/nullherz-conductor/src/topology_manager.rs` |
| **Mixer Console** | Active | `crates/nullherz-mixer/src/dj.rs` |
| **Genetic Cloud** | Prototype | [PARTIAL] Lacks cryptographic auth; limited to Studio LAN. |
| **Sequencer / Composer** | Active | `crates/nullherz-processors/src/sequencer.rs` |
| **Persistence** | Active | `crates/nullherz-conductor/src/persistence.rs` (rkyv verified) |
| **Modulation Matrix** | Active | `crates/nullherz-processors/src/modulation.rs` (addressable) |

---

## 6. Conclusion

The Nullherz engine is now in a **Hardened Alpha** state. It achieves significant isolation between management and execution planes. While core DSP and orchestration paths are real-time safe and tested, secondary systems (P2P Cloud) and verification coverage (Kani/Fuzzing) remain in active development.

**Architecture Status:** HARDENED ALPHA.
