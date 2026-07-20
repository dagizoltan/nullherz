# Nullherz System Architecture: Lead Architect's Comprehensive Report

**Author:** Senior Lead Audio & Rust Systems Architect
**Status:** PRODUCTION BETA
**Date:** 2026-07-20 (verified against code; see [ARCHITECTURE.md](../system/ARCHITECTURE.md) for the full reverse-engineered reference)

---

## 1. Architectural Overview: The Triple-Plane Model

The Nullherz engine is built upon a strict separation of concerns, ensuring that high-level management never interferes with real-time signal processing.

### 1.1 The Orchestration Plane (`nullherz-conductor`)
*   **Responsibility**: Lifecycle management, declarative topology reconciliation, and global resource coordination.
*   **Off-Thread Compilation**: [VERIFIED] `TopologyManager` performs expensive topological analysis (Kahn's algorithm) off the audio thread (`crates/nullherz-conductor/src/topology_manager.rs`). Off-thread compilation is enforced for all commit paths.
*   **Decoupling**: Interacts with the execution plane exclusively through `RenderingEngine` and `RenderingController` trait objects.

### 1.2 The Protocol Plane (`ipc-layer`, `nullherz-traits`)
*   **Responsibility**: Zero-allocation, lock-free communication between all planes.
*   **Broadcaster Telemetry**: `nullherz-gateway` utilizes a non-blocking broadcaster pattern, allowing multiple monitoring clients (Dashboards/Inspectors) to receive the same telemetry stream without frame competition.
*   **SIMD Foundation**: Enforces 64-byte alignment and provides the `AudioBlock` primitives used throughout the execution plane.

### 1.3 The Execution Plane (`audio-core`, `audio-dsp`)
*   **Responsibility**: Low-latency, bit-exact audio processing.
*   **Static Graph Execution**: [VERIFIED] The `ProcessorGraph` acts as a lightweight VM. Structural shifts are O(1) pointer swaps via `SetTopology`, ensuring zero-jitter transitions. A regression test `test_rt_topology_commit_is_no_op` ensures the RT thread cannot be stalled by accidental topology recalculations.

---

## 2. Advanced Core Invariants

### 2.1 Real-Time Safety & Performance
- **Lock-Free Sample Access**: [VERIFIED] `SampleRegistry` uses an atomic-swap pattern.
- **Zero Heap Allocation**: [VERIFIED] Audit of audio hot-paths indicates zero-allocation for core processors, verified via the gauntlet conformance test suite.
- **CPU Hardening**: [VERIFIED] FTZ/DAZ enabled for RT threads (`crates/ipc-layer/src/lib.rs` - `setup_rt_thread`).
- **Atomic Topology**: [VERIFIED] Structural shifts are buffered and committed via `TopologyManager` off-thread.

### 2.2 Fault Tolerance & Signal Stability
- **Sidecar Resilience**: `SidecarSupervisor` tracks `node_idx` state, ensuring failed DSP sidecars are restored to their correct topological position.
- **Automated Soft Fallback**: Stalled heartbeats (>200ms) trigger an instant swap to a zero-overhead `FallbackProcessor` (Bypass) to maintain audio continuity.
- **System-Wide Safe Mode**: Sidecar failures can now trigger a global "Safe Mode" via the command bus, allowing the engine to enter a known stable state.
- **RSS Limits**: Sidecar subprocesses are now constrained by real RSS memory limits using cgroups.
- **DSP Safety Pass**: All critical kernels (Gain, Biquad, Spectral) are hardened against non-finite float values.
- **Stereo Integrity (July 2026)**: Sample buffers are planar end to end (channel *c* at `buffer[c*frames..]`, playhead counts frames), fixing stereo files playing an octave high at double tempo. `MAX_BUFFERS = 128` decouples the buffer/edge address space from `MAX_NODES = 64` so a full stereo console cannot silently alias graph edges.

### 2.3 Hardware & Distributed Orchestration
- **Universal Backend Switching**: Real-time hot-swapping between ALSA, PipeWire, JACK, Threaded, and Mock backends without process restarts; automatic fallback to Threaded on boot failure. Period size is now configurable via `system_config.json` (`period_size`).
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

### 4.2 Song Builder Readiness [100%]
*   **Current State**: Sample-accurate parameter automation, modular "Transfusion" layer, and a global Pattern Manager are operational.
*   **Alpha Requirement**: [DONE] **Project Persistence**. Implement a serialized `ProjectState` that captures the entire topology, sequence grid, and parameter set, allowing for session save/load cycles.
*   **Alpha Requirement**: [DONE] **Pattern Orchestration**. Move beyond the 16-step `SequencerProcessor` toward a "Pattern Manager" that can schedule complex arrangements on the `Timeline`.
*   **Alpha Requirement**: [DONE] **Macro Modulation**. Introduce a "Modulation Matrix" that allows high-level controls (Macro Knobs) to broadcast commands to multiple downstream DSP nodes simultaneously.

---

## 5. Subsystem Readiness Matrix

| Subsystem | Readiness | Evidence |
|-----------|-----------|----------|
| **Core Engine** | Production Beta | Parallel execution covered by a Kani proof harness (`kani-verify` feature; graph, jitter buffer, and PI-servo harnesses). |
| **Clock Sync** | Active | Typed PTP protocol with Delay_Req/Delay_Resp measured path delay (offset-free, EMA-filtered, plausibility-clamped), PI clock servo (Kani-proved clamp). Remaining: SO_TIMESTAMPING integration, BMC election. |
| **Topology Manager** | Production Beta | Thread-safe, off-audio DAG compilation. |
| **Genetic Cloud** | Active | TCP Gossipsub-style DNA exchange with ed25519 `GOSSIP_SIGNED` payloads and TOFU peer-identity pinning (HANDSHAKE/IDENTITY, key-change rejection). |
| **DSP Kernels** | Production Beta | Wasm-SIMD128 optimized spectral kernels with exact COLA overlap-add reconstruction; KeySync is a real phase-vocoder pitch shifter; OLA time-stretch and transient detectors (July 2026). |
| **Mixer Console** | Active | 4-channel DJ topology with harmonic auto-sync; stereo deck strips with private L/R buffers and stereo cue bus. |
| **DJ Console UX** | Active | Frequency-colored asymmetric waveforms with beat grid + numbered hot cues; visible playhead + time readouts; pause/resume transport; persistent hot cues; per-deck command isolation; wrap-flow mixer strips (July 2026 UI campaign). |
| **Audio Editor** | Active | Waveform selection, OLA time-stretch, transient chop, non-destructive undo (July 2026). |
| **Composer** | Active | Endless-scroll step grid with sequencer routing and live per-step telemetry. |
| **Curation** | Active | Intelligent "Energy Match" smart crating logic. |

---

## 6. Conclusion

The Nullherz engine has transitioned to a **Production Beta** state. It features a Kani-covered execution plane, distributed clock discipline with measured path delay, a signed genetic DNA gossip protocol with peer-identity pinning, warning-free builds (`cargo check --workspace --all-targets -D warnings`), a CI gate, and a fully green test suite (**200/200**, 2026-07-20).

**Architecture Status:** PRODUCTION BETA.
