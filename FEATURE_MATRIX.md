# nullherz Feature Matrix

This document tracks the implementation status of core architectural components and DSP features.

| Category | Feature | Status | Description |
| :--- | :--- | :---: | :--- |
| **Execution Engine** | Zero-Allocation RT Loop | ✅ | No heap usage in `process_block`. |
| | Sample-Accurate Automation | ✅ | Commands applied at exact sample offsets. |
| | 128-sample SIMD Chunking | ✅ | Transparent handling of arbitrary block sizes. |
| | Architecture-Specific Timing | ✅ | Cycle-accurate profiling (x86_64 / aarch64). |
| | Topology Crossfading | ✅ | Click-free re-wiring of graph edges. |
| | Parallel Node execution | ✅ | Multi-threaded TaskPool with stage grouping. |
| **DSP Library** | SIMD Biquad Filters | ✅ | Optimized for AVX-512 and ARM Neon. |
| | Fast Wavetable Oscillator | ✅ | Division-free phase wrapping, FM/PM support. |
| | Dj Isolator (SSE3) | ✅ | Parallel band processing in SIMD lanes. |
| | Spectral Engine (OLA) | ✅ | Robust Overlap-Add foundation with precomputed windows. |
| | Modulation Matrix | ✅ | Audio-rate CV-to-Parameter mapping functional. |
| | Convolution Reverb | ❌ | Requires full Partitioned Convolution implementation. |
| **System & IPC** | Process Isolation (Sidecars) | ✅ | Independent DSP processes via SHM. |
| | Zero-Copy SHM RingBuffer | ✅ | Lock-free SPSC communication. |
| | Sidecar Heartbeat Watchdog | ✅ | Automatic recovery of crashed DSP nodes. |
| | Cgroup / RT-Priority Management | ✅ | High-priority thread and resource isolation. |
| **Backends** | Improved ALSA Backend | ✅ | Low-level hw_params, float32 native path. |
| | Jack Backend | ✅ | Basic functionality verified. |
| | PipeWire Backend | ✅ | Native SPA protocol with dynamic 16-channel support. |
| | Backend Hot-swapping | ✅ | State-preserving transitions between backends. |
| **Tooling** | Inspector CLI/GUI | ✅ | Real-time graph visualization and telemetry. |
| | sidecar-sdk | ✅ | DSL for rapid process-isolated effect development. |
| | Performance Benchmarking | ✅ | Automated stress-testing suite. |
| | Fuzz Testing | ❌ | adversarial input stability testing pending. |

**Legend:**
- ✅ **Completed**: Production-ready implementation.
- 🛠 **In Progress**: Functional but requires refinement or expansion.
- ❌ **Planned**: Not yet implemented.
