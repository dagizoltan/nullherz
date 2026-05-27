# Architecture Specification: Real-Time Rust Audio System (nullherz)

## 1. Project Goal
Design and implement a hardware-independent, Linux-first real-time audio system written in Rust.
The system is a deterministic audio execution engine, not a plugin-based DAW.

### Primary Objectives
- **Ultra-low latency**: 2–6 ms stable target.
- **Deterministic**: Real-time behavior under load.
- **AI-native control plane**: Non-blocking operations.
- **Sidecar-based DSP**: No plugin system, isolated processes.
- **Modular**: Fully modular Rust crate ecosystem.
- **Linux-first**: PipeWire/JACK/ALSA compatible.

---

## 2. Core Design Principles

### 2.1 Hard Separation of Execution Domains
1. **Real-Time Audio Kernel (RT Core)**: Executes audio graph. Strict real-time thread. No allocations, no locks, no syscalls.
2. **Control Plane**: UI, AI, automation. Converts actions into command streams.
3. **Sidecar DSP Services**: Independent processes/modules. Heavy/optional DSP tasks. IPC communication.

### 2.2 Deterministic Execution Model
- State changes as immutable, timestamped commands.
- Kernel applies commands at exact sample offsets.
- No direct state mutation in RT path.

### 2.3 No Plugin Architecture
- No VST/AU.
- DSP functionality is either compiled Rust crates or isolated sidecar services.

### 2.4 Zero Allocation RT Thread Rule
- No heap memory allocation.
- No I/O.
- No locks/mutexes.
- No OS syscalls.

---

## 3. Advanced Architectural Patterns

### 3.1 Atomic Graph Swapping
To allow dynamic reconfiguration of the audio engine without violating real-time constraints, we use `AtomicPtr` for swapping the entire processing graph. The Control Plane prepares a new `ProcessorChain` and performs an atomic swap. The old graph is then sent back to the Control Plane for safe deallocation outside the RT thread.

### 3.2 SIMD Alignment and Vectorization
Audio buffers and DSP structures are aligned to 64-byte boundaries (supporting AVX-512) to enable efficient auto-vectorization and manual SIMD optimizations. Fixed-size blocks are used to ensure predictable performance.

### 3.3 Sample-Accurate Automation
Commands are applied by splitting audio blocks into sub-blocks at the exact sample where the command is timestamped. This ensures that parameter changes happen at the precise intended moment, regardless of the system's buffer size.

---

## 4. Rust Crate Architecture
- `audio-core`: Core engine logic, RT loop, and backend abstractions.
- `audio-dsp`: Optimized DSP primitives, filters, and oscillators.
- `fx-runtime`: Logic for running complex effects.
- `control-plane`: Command stream management, AI integration, and graph management.
- `ipc-layer`: Zero-copy Shared Memory IPC and aligned data structures.
- `sidecar-sdk`: SDK for building external, isolated DSP processes.
