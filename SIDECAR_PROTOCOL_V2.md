# SIDECAR_PROTOCOL_V2 Specification

This document details the binary protocol for inter-process and inter-machine communication between the `nullherz-conductor` and DSP Sidecars.

## Framing

All messages over TCP use a 4-byte Big-Endian length prefix, followed by the payload.

`[u32: length][u8: type][...payload...]`

## Message Types

### Type 1: TimestampedCommand (Bidirectional)
- **Header Type:** `1`
- **Payload:** Binary serialized `nullherz_traits::TimestampedCommand` using `bincode`.
- **Usage:** Orchestrating parameter changes, transport triggers, and DNA transfusion.

### Type 2: Sample Data Mirroring (Conductor -> Sidecar)
- **Header Type:** `2`
- **Payload:**
  - `[u64: sample_id]`
  - `[u32: sample_count]`
  - `[f32 * sample_count: data]`
- **Usage:** Transferring audio buffers from the central registry to remote nodes.

### Type 3: Audio Return Block (Sidecar -> Conductor)
- **Header Type:** `3`
- **Header Additional:** `[u32: node_idx]`
- **Payload:** `nullherz_traits::AudioBlock` (Binary/Pod).
- **Usage:** Returning processed audio blocks from remote nodes to the local engine via `IpcAudioBridge`.

### Type 4: Heartbeat / Telemetry (Sidecar -> Conductor)
- **Header Type:** `4`
- **Payload:**
  - `[f32: cpu_usage]`
  - `[f32: latency_ms]`
- **Usage:** Monitoring the health and performance of remote DSP nodes.

## Type-Safety & ABI Invariants

1. **Alignment:** All `AudioBlock` payloads MUST be 64-byte aligned and 1088 bytes in size (including padding).
2. **Serialization:** All non-Pod types MUST be serialized using `bincode` with standard configuration.
3. **Real-time Safety:** Message handlers in the Conductor MUST NOT allocate on the audio thread. Type 3 blocks are routed through pre-allocated SPSC queues.
