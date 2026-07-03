# Nullherz Technical Optimization & Hardening Log

**Date:** June 22, 2026
**Focus:** Performance Maximization & RT-Hardenings

---

## 1. Real-Time (RT) Hardenings
*Ensuring the audio thread is 100% deterministic and jitter-free.*

| Task | Priority | Description |
| :--- | :---: | :--- |
| **Kernel Devirtualization** | VERIFIED | Replaced `Box<dyn ProcessingKernel>` with static dispatch (`AudioEngine<K: ProcessingKernel>`). Verified via `audio-core`. |
| **Pre-allocated Commands** | VERIFIED | Refactored `MixerBridge` to use a pool of pre-allocated `Vec<Command>` via `ResourceRecycler`. Verified via `nullherz-conductor`. |
| **Stack-Pinning** | High | Investigate stack-pinning for critical DSP nodes to improve cache locality and prevent accidental movement during block cycles. |
| **Denormal Safeguard v2**| Low | Add explicit `is_subnormal` checks in the `Gain` kernel as a secondary defense to FTZ/DAZ hardware flags. |

---

## 2. DSP & SIMD Optimizations
*Maximizing throughput and reducing per-sample CPU cycles.*

| Task | Priority | Description |
| :--- | :---: | :--- |
| **AVX-512 Pathway** | VERIFIED | Implemented 16-wide `FloatX16` path in `simd_vec.rs` and core kernels. Moved state to stack locals for register optimization. |
| **Zero-Copy Sampler** | VERIFIED | Refactored `SamplerVoice` Lagrange interpolation for direct SIMD pointer loads via `f32x4`. |
| **Biquad Unrolling** | Medium | Implement 4x unrolled biquad kernels for the `DjIsolator` to reduce the overhead of crossover filtering. |
| **FFT Twiddle Caching** | Low | Implement a global twiddle-factor cache in `audio-dsp` to avoid re-calculation during dynamic FFT size changes. |

---

## 3. Infrastructure & Throughput
*Reducing IPC overhead and orchestration latency.*

| Task | Priority | Description |
| :--- | :---: | :--- |
| **Ring Buffer Affinity** | High | Set CPU affinity for IPC ring buffers to pin producers/consumers to specific L3 cache domains. |
| **Telemetry Batching** | Medium | Batch `NodeMetrics` telemetry to reduce the frequency of atomic writes to the shared-memory status segment. |
| **Zero-Copy Serialization**| Medium | Move `ProjectState` toward a binary format (e.g., `rkyv`) to achieve near-instant project save/load. |

---

## 4. UI Rendering & Metrics
*Ensuring the Inspector remains fluid even under heavy DSP load.*

| Task | Priority | Description |
| :--- | :--- | :--- |
| **Widget Caching** | High | Cache `egui::Shape` primitives for knobs and faders to avoid recalculating geometry every frame. |
| **Waveform Downsampling**| Medium | Implement a multi-level waveform cache (MIP-maps) to reduce the number of points rendered in the rolling monitor. |
| **Telemetry Throttle** | Low | Decouple UI refresh rate (60Hz) from telemetry ingestion rate (RT-frequency) using a damping filter. |

---

## 5. Summary of Immediate Action Items
1.  **SIMD Pointer Access:** Refactor sampler kernels for direct aligned memory access.
2.  **UI Geometry Caching:** Optimize industrial-steel widget rendering.

---

**Audit Conducted by:** *Nullherz Systems Architecture Team*
