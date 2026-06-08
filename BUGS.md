# nullherz: Known Issues, Performance Debt & Architectural Risks

This document tracks identified bugs, limitations, and areas requiring further hardening to maintain "high-end product" stability.

## 🔴 Critical Architectural Risks (Resolved)

### 1. Runtime Topology Validation (FIXED)
Implemented `verify_no_hazards_prod` in the graph commitment path to formally check for WAW hazards before any graph is activated in the RT thread.

### 2. Sidecar Resource Leakage (FIXED)
Implemented RAII `Drop` for `SidecarManager` and automated zombie reaping in the conductor to ensure external processes are reaped and SHM segments are unlinked.

## 🟡 Performance Debt

### 1. SIMD Coverage Gaps
Many DSP components (e.g., `BiquadFilter`, `Gain`) use manually unrolled loops but do not leverage true vector types (`f32x8`).
- **Debt**: This results in suboptimal instruction density and higher power consumption than necessary for a high-end engine.
- **Action**: Move to a unified SIMD abstraction (e.g., `wide` crate) for all core DSP primitives.

### 2. Spectral OLA Micro-jitter (FIXED)
Enforced power-of-two buffer sizes and moved to bitwise mask indexing in the Spectral Processor, eliminating cycle-expensive modulo operations in the hot path.

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
