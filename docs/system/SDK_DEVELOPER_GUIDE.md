# Nullherz SDK Developer Guide

Welcome to the Nullherz ecosystem. This guide provides instructions for building high-performance DSP plugins (Sidecars) that integrate seamlessly with the Nullherz distributed orchestration engine.

## Core Concepts: The Triple-Plane Model

1. **Orchestration Plane:** Managed by `nullherz-conductor`. It handles discovery, routing, and command dispatch.
2. **Protocol Plane:** The binary interface (`nullherz-traits`, `ipc-layer`) that links orchestration to execution.
3. **Execution Plane:** Where your Sidecar lives. It performs real-time audio processing in an isolated process.

## Getting Started: The Template

The fastest way to start is using the `sidecars/nullherz-template` project.

```rust
use sidecar_sdk::SidecarHost;
use nullherz_traits::{AudioProcessor, ProcessContext, SignalProcessor};

struct MyProcessor;

impl SignalProcessor for MyProcessor {
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext) {
        // Your RT-safe DSP logic here
    }
}

// Implement required traits...

fn main() {
    // Option A: Manual Host Initialization
    let mut host = unsafe { SidecarHost::new_from_args() };
    host.run(MyProcessor);
}

// Option B: Using the SidecarBuilder Macro (Recommended)
sidecar_builder!();

fn main() {
    SidecarApp::build_and_run("my-processor", MyProcessor::new());
}
```

## Composite Sidecar Stores (`SidecarStore`)

To chain an instrument sidecar with multiple insert sidecar effects without accumulating IPC block latency, use `SidecarStore`:

```rust
use sidecar_sdk::SidecarStore;

let mut store = SidecarStore::new();

// 1. Set Instrument Sidecar (source)
store.set_instrument(Box::new(MyInstrument::new()));

// 2. Add Insert Sidecars sequentially
store.add_insert(Box::new(MyCompressor::new()));
store.add_insert(Box::new(MyReverb::new()));

// The store processes instrument generation and insert DSP in-place within
// a single quantum block cycle, presenting a SINGLE IPC boundary to Nullherz!
```

## Latency Classes & Asynchronous Neural Workers

Nullherz processors expose explicit latency classification (`LatencyClass`):
- `UltraRealtime`: < 1 ms (< 48 samples at 48 kHz)
- `Realtime`: 1–3 ms (48–144 samples)
- `Performance`: 3–10 ms (144–480 samples)
- `Musical`: 10–30 ms (480–1440 samples)
- `Offline`: > 30 ms

For long-context or heavy neural models, integrate using `NeuralWorkerBridge`. The bridge streams audio frames to an off-thread neural worker via `ShmRingBuffer` and non-blockingly polls parameter updates (`NeuralControlMessage`), ensuring neural inference never blocks the real-time audio thread.

## DNA Transfusion Kernels

The SDK provides `DnaKernel` utilities to assist with genetic audio processing:

```rust
use sidecar_sdk::DnaKernel;

// Apply spectral shaping from SoundDNA
DnaKernel::apply_spectral_personality(&mut output, &input, &dna, 0.5);
```

## Real-time Safety Invariants

To ensure system stability, your `process()` loop MUST NOT:
- Allocate memory on the heap (`Vec::new()`, `Box::new()`, etc.)
- Use blocking synchronization (standard `Mutex`, `RwLock`)
- Execute syscalls (File I/O, Networking, `println!`)

**Use `audio_dsp::simd_vec::FloatX16` for high-performance math.**

## Distributed Protocol (V2)

Communication with the conductor happens over TCP (Inter-machine) or Shared Memory (Intra-machine) using binary framing:

`[u32: length][u8: type][payload]`

- **Type 1:** `TimestampedCommand` - Serialized with `bincode`.
- **Type 3:** `AudioBlock` - Fixed-size 1088-byte Pod structure.

## Building Spectral Processors

Nullherz provides a powerful spectral pipeline via `audio_dsp::SpectralPipeline`.

1. Define your frequency-domain kernel.
2. Use the `wide` crate for SIMD bin interpolation.
3. Integrate with the `SoundDNA` genetic schema to support real-time trait transfusion.

## Debugging

- Use `nullherz-bench` to flood your sidecar with commands and test robustness.
- Monitor `nullherz-inspector`'s System tab for CPU usage and heartbeat stalls.

---

**Happy Coding.** *The Nullherz Architecture Team*
