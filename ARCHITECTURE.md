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

## 3. High-Level System Architecture
`UI Layer` → `Control Plane` → `Real-Time Audio Kernel` → `DSP Sidecars` → `Linux Audio Backend`

---

## 4. Rust Crate Architecture
- `audio-core`: Core engine logic and RT loop.
- `audio-dsp`: Basic DSP primitives and traits.
- `fx-runtime`: Logic for running effects.
- `control-plane`: Command stream management and AI integration.
- `ipc-layer`: Zero-copy IPC mechanisms (Shared Memory).
- `sidecar-sdk`: SDK for building external DSP processes.

---

## 5. Performance Targets
- 2–6 ms latency.
- 32–128 sample buffers.
- Zero-drop RT execution.
