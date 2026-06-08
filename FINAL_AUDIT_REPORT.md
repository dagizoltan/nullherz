# FINAL AUDIT REPORT: nullherz RT-Kernel (Pre-1.0)

This report details the final remaining bugs and architectural risks that must be resolved to achieve absolute "bug-free" production stability for high-end audio products.

## 🔴 CRITICAL (Blocking 1.0 Release)

### 1. RT Deallocation Hazard: Command Bundles
In `AudioEngine::process_block`, if `bundle_garbage_producer` is `None` or its queue is full, the code either implicitly drops the `Vec<Command>` or explicitly "leaks" it via `std::mem::forget`.
- **Bug**: Implicit dropping causes a heap deallocation inside the RT thread, leading to high-jitter or XRuns. `mem::forget` causes a memory leak that will eventually crash the engine.
- **Fix**: Bundles must be stored in a pre-allocated "overflow" ring buffer or the control plane must be back-pressured.

### 2. Peak Level Overwrite in Sub-blocks
The `ProcessorGraph` currently overwrites `peak_levels` for every sub-block.
- **Bug**: If a block is split (e.g., at sample 10 out of 128), the peak level for samples 0-10 is lost, and the telemetry only shows the peak of samples 10-128. This makes metering inaccurate.
- **Fix**: Peak tracking must accumulate (max) across all sub-blocks in a single engine cycle.

### 3. Unbounded Topology Command Loop
The `AudioEngine` processes all pending `topology_consumer` commands in a single cycle.
- **Risk**: A flood of topology changes (e.g., adding 50 nodes at once) can exceed the cycle time budget.
- **Fix**: Limit the number of topology mutations per block, similar to the command limit.

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
