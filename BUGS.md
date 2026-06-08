# nullherz: Known Issues, Performance Debt & Architectural Risks

This document tracks identified bugs, limitations, and areas requiring further hardening to maintain "high-end product" stability.

## 🔴 Critical Architectural Risks

### 1. Static Topology Validation (Unverified)
The engine relies on Kahn's algorithm for topological sorting and Write-After-Write (WAW) hazard detection.
- **Risk**: A bug in the sorting logic or a malformed routing command from the control plane could cause multiple threads to write to the same physical buffer simultaneously.
- **Mitigation**: Implement a runtime "Hazard Checker" (debug-only) that verifies routing sanity before graph commitment.

### 2. Sidecar Memory Exhaustion
The `SidecarProcessor` uses shared memory ring buffers.
- **Risk**: If a sidecar process crashes or hangs without releasing its shared memory handles, the system may leak SHM segments or run out of file descriptors.
- **Mitigation**: Implement a robust cleanup mechanism in `nullherz-conductor` that forcibly unlinks SHM segments of zombie processes.

## 🟡 Performance Debt

### 1. SIMD Coverage Gaps
Many DSP components (e.g., `BiquadFilter`, `Gain`) use manually unrolled loops but do not leverage true vector types (`f32x8`).
- **Debt**: This results in suboptimal instruction density and higher power consumption than necessary for a high-end engine.
- **Action**: Move to a unified SIMD abstraction (e.g., `wide` crate) for all core DSP primitives.

### 2. Spectral OLA Micro-jitter
The `SpectralProcessor` overlap-add loop uses modulo operators (`% out_len`) which can be expensive in the hot path.
- **Debt**: Small but measurable jitter in the spectral processing stage.
- **Action**: Use power-of-two buffer sizes and bitwise masks to eliminate modulo divisions.

## 🟢 Known Bugs & Glitches (Resolved)

### 1. Sequence Wrap Precision (FIXED)
The `SequencerProcessor` now uses sample-absolute indexing, eliminating rounding errors and precision drift in long-duration performances.

### 2. Heartbeat Modular Wrap (FIXED)
Sidecar stall detection now uses wrapping modular arithmetic for heartbeat comparison, ensuring robustness across $2^{64}$ cycles.

### 3. RAII Sidecar Cleanup (FIXED)
Implemented `Drop` for `SidecarManager` to ensure zombie processes are killed and resources released.

## 🔵 Outstanding Debt

### 1. ALSA Descriptor Inheritance
`EventFd` is created with `CLOEXEC`, but child sidecar processes may require access to specific hardware descriptors in rare configurations.
- **Symptom**: Permission errors when spawning sidecars with hardware-direct access.
- **Action**: Implement explicit descriptor white-listing during sidecar `fork`/`exec`.
