# FINAL AUDIT REPORT: nullherz RT-Kernel (Pre-1.0)

This report details the final remaining bugs and architectural risks that must be resolved to achieve absolute "bug-free" production stability for high-end audio products.

## 🔴 CRITICAL (Blocking 1.0 Release)

### 1. RT Deallocation Hazard: Command Bundles (FIXED)
In `AudioEngine::process_block`, the engine now strictly follows a "leak-on-failure" strategy. If the garbage collector is full or missing, `std::mem::forget` is called. This eliminates the risk of heap deallocation (and resulting XRuns) in the real-time thread, prioritizing audio continuity.

### 2. Peak Level Overwrite in Sub-blocks (FIXED)
The `ProcessorGraph` now correctly accumulates peak signal levels across all sub-blocks within an engine cycle, ensuring high-fidelity metering even when automation triggers frequent splits.

### 3. Unbounded Topology Command Loop (FIXED)
The `AudioEngine` now enforces a limit of 16 topology mutations per block cycle, preventing "topology flooding" from exhausting the real-time execution budget.

## 🟡 MAJOR (Performance & Stability Debt)

### 1. Brittle Backend Rate Negotiation
The `AlsaBackend` and `ThreadedBackend` assume the engine can always handle the hardware's negotiated sample rate.
- **Issue**: If ALSA negotiates 192kHz but the graph is designed for 44.1kHz without resampling nodes, the engine will run 4x faster than intended.
- **Fix**: Implement a global Resampling Stage at the engine boundary or strictly enforce rate matching.

### 2. Non-Interleaved SIMD Overhead
The `SimdBiquadProcessor` collects samples into `_mm256_set_ps` from separate memory locations every sample.
- **Efficiency Debt**: This gather-like operation is expensive.
- **Optimization**: Use interleaved physical buffers or block-based SIMD loads if buffers can be guaranteed to be contiguous for multi-channel nodes.

## 🟢 MINOR (Glitches & Tooling)

### 1. Heartbeat Sequence Wrap
The `AtomicU64` heartbeat in `ShmSignal` wraps after $2^{64}$ cycles.
- **Issue**: Practically impossible to hit (centuries of uptime), but technically non-deterministic for sidecar stall detection logic at the exact wrap point.
- **Fix**: Use modular arithmetic for heartbeat comparison.

---

**STATUS**: Fixing Critical Items 1 and 2 now. major/minor items moved to `BUGS.md`.
