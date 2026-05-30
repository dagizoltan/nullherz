# Nullherz Creative Ecosystem: Detailed System Design

This document provides the architectural specification for expanding the **nullherz** real-time audio engine into a modular Studio, DJ, and Radio platform.

---

## 1. Architectural Tiers

The system is organized into three distinct layers to maximize modularity and ensure that complex high-level features never interfere with the deterministic low-latency audio core.

### Tier 1: The RT-Kernel (Engine Room)
*   **Responsibility**: Sample-accurate audio processing, SIMD DSP, and lock-free graph execution.
*   **State**: Only knows about `Nodes`, `Buffers`, and `Atomic Parameters`.
*   **Determinism**: Zero heap allocation and zero syscalls in the `process_block` path.

### Tier 2: The Conductor (Logic & Timing)
*   **Responsibility**: Managing musical time, clock domains, and command scheduling.
*   **State**: Owns the project timeline (BPM, Meter, Transport status).
*   **Bridge**: Translates musical events (e.g., "Trigger Pad 1 on Beat 4") into engine commands with exact sample offsets.

### Tier 3: The Context Layer (Sidecars & UI)
*   **Responsibility**: External IO and state visualization.
*   **Sidecars**: Independent processes for disk-streaming (Sampler), network-streaming (Radio), and complex UI (Inspector/GUI).

---

## 2. The Switchboard: System Slab Memory Model

To simplify routing for global features (Radio, DJ CUE), the first 8 virtual buffers are reserved for system-wide buses.

| Buffers | Name | Description |
| :--- | :--- | :--- |
| **0-1** | `MASTER_OUT` | Final stereo mix intended for the main speakers. |
| **2-3** | `CUE_OUT` | Monitor bus for headphones (DJ pre-listening / Studio metronome). |
| **4-5** | `BROADCAST_OUT` | Dedicated siphon bus for the Radio/Encoder sidecar. |
| **6-7** | `PREVIEW_BUS` | Internal bus for auditioning samples without affecting Master. |
| **8-9** | `DJ_BUS_A` | Pre-summed output for Deck A (for crossfading). |
| **10-11** | `DJ_BUS_B` | Pre-summed output for Deck B (for crossfading). |
| **12-63** | `DYNAMIC_POOL` | Managed by the scratchpad allocator for internal routing. |

---

## 3. Mixing & Routing Logic (`nullherz-mixer`)

The "Lego" model (Option A) is implemented via a **Macro-Node** system. A "Channel" is a logical group of kernel nodes.

### Studio Channel Strip Template
1.  **Source Node**: Sampler or external input.
2.  **Gain Node**: Pre-fader trim and polarity.
3.  **Insert FX Chain**: A dynamic list of independent FX nodes (Biquad, Distortion, etc.) that process the signal sequentially.
4.  **Fader Node**: Smooth volume control and Stereo Panning.
5.  **Siphon Node**: Optional "tap" to send audio to FX groups or the Broadcast bus.

### DJ Deck Template
1.  **Resampling Node**: High-quality interpolation for pitch-shifting and time-stretching.
2.  **Insert FX Chain**: Independent deck effects (Echo, Reverb, etc.).
3.  **Isolator EQ**: Specialized 3-band "Kill" EQ using SIMD crossovers.
4.  **Output Assignment**: Routed either to `DJ_BUS_A`, `DJ_BUS_B`, or directly to `MASTER_OUT`.

### The Crossfader
*   **The Crossfader Node**: Resides on the Master Bus. It performs a SIMD-optimized blend between `DJ_BUS_A` and `DJ_BUS_B` based on a project-wide `CrossfaderPosition` parameter.

---

## 4. The Conductor: Multi-Clock Domain

Essential for the Studio/DJ hybrid, the Conductor supports multiple simultaneous timelines.

*   **Global Transport**: Primary clock for quantized MPC pads and Studio clip launching.
*   **Elastic Decks**: Each DJ Deck can have its own independent clock domain.
*   **Sync Engine**: A PID controller logic in the Conductor that nudges Deck Clocks to align their phase/tempo with the Global Transport or each other.

---

## 5. MIDI Mapping & External Control

To ensure low-latency physical control (MPC pads, DJ faders), MIDI is handled as a primary input stream.

*   **MIDI Sidecar**: A dedicated sidecar responsible for OS-level MIDI device discovery (ALSA/JACK MIDI).
*   **The Mapper (Conductor)**: Lives in the Conductor. It maintains a registry of `(MIDI_CC, Channel) -> (Node_ID, Param_ID)`.
*   **Late-Binding**: Parameters can be "learned" at runtime. The UI sends a `LearnParameter` request, and the next received MIDI CC is bound to that engine parameter.

---

## 6. Feature Implementation Strategy

### DAW & MPC Features
*   **Event Timeline**: A look-ahead queue in the Conductor that buffers events (MIDI-style) and dispatches them to the Engine exactly 128 samples before they are due.
*   **Quantization**: Configurable grid (1/16, 1/32) that snap UI-triggered events to the next transport tick.

### Radio & Streaming
*   **Broadcast Sidecar**: A standalone process using `libshout` or `ffmpeg`. It reads from `BROADCAST_OUT` SHM buffers.
*   **Metadata Bridge**: A lock-free SPSC ring-buffer that carries "Artist - Title" metadata from the Conductor to the Encoder sidecar for synchronized stream tagging.

---

## 6. Iterative Development Path

1.  **Iteration 1**: Implement `nullherz-mixer` and the **System Slab** (Basic Routing).
2.  **Iteration 2**: Implement the `Conductor` and **Sample-Accurate Clock** (Timing).
3.  **Iteration 3**: Implement the **SIMD Sampler Sidecar** (The Voice).
4.  **Iteration 4**: Implement the **Broadcast Sidecar** (The Radio).
