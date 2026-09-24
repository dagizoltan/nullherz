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

### Type 5: Remote Audio Send Block (Conductor -> Sidecar)
- **Header Type:** `5`
- **Header Additional:** `[u32: node_idx]`
- **Payload:** `nullherz_traits::AudioBlock` (Binary/Pod).
- **Usage:** Transmitting input audio blocks from local `NetworkProxySend` nodes to remote sidecars for processing.

### Type 6: UDP Audio Return (Sidecar -> Conductor)
- **Protocol:** UDP
- **Header Additional:** `[u32: node_idx]`
- **Payload:** `nullherz_traits::AudioBlock` (Binary/Pod).
- **Usage:** Low-jitter return path for processed audio blocks from remote nodes.

### Type 7: RDMA Zero-Copy Return (Experimental)
- **Protocol:** RoCE v2 / InfiniBand
- **Mechanism:** Direct Memory Access via `RdmaBridge`.
- **Payload:** Memory-mapped `AudioBlock` segments.
- **Usage:** Near-zero CPU overhead return path for extreme-density multi-machine DSP environments.

### Type 8: MIDI & Automation Fast-Path (Conductor -> Sidecar)
- **Mechanism:** Shared Memory (SHM) Ring Buffer.
- **Payload:** `nullherz_traits::MidiEvent`.
- **Usage:** Low-latency delivery of high-frequency MIDI events and parameter automation, bypassing the TCP/UDP stack for local sidecars.

## Side-Chain Input Support
Sidecars can now request additional physical buffer assignments during registration. The `sidecar-sdk` supports multi-input mappings, allowing for sophisticated side-chain compression and modular routing configurations within external DSP processes.

## SidecarStore & Composite SidecarChain Architecture

To enable rich instrument synthesis and multi-insert FX chains within sidecar processes without accumulating multi-hop IPC latency, the `sidecar-sdk` includes `SidecarStore` and `SidecarChain`.

### Eliminating Latency Accumulation
Normally, routing audio through separate IPC sidecars in series introduces a 1-block quantum delay per IPC hop. `SidecarStore` provides a composite container (`SidecarChain`) that encapsulates an optional instrument sidecar followed by sequential insert sidecars in a single execution loop within the same sidecar process.

```
HOST ENGINE ---> IPC Boundary ---> [ SidecarChain Container ]
                                     ├── Instrument (Synth / Generator)
                                     ├── Insert 1 (Neural Saturation)
                                     ├── Insert 2 (Neural Filter / EQ)
                                     └── Insert 3 (Algorithmic Tape Delay)
                 <--- IPC Boundary <--- Single Audio Return Block
```

By executing instrument generation and sequential insert DSP in-place within a single block quantum, `SidecarStore` presents a single IPC boundary to the audio host engine, maintaining deterministic <1ms real-time latency across arbitrarily deep effect chains.

### Metadata Schema & Tag-Based Query System
Every sidecar in `SidecarStore` is registered with a `SidecarDescriptor` containing searchable metadata and tags:

- **ID:** Unique string identifier (e.g., `"neural-saturation"`)
- **Name:** Human-readable label (e.g., `"Neural Saturation / Preamp"`)
- **Type:** `SidecarType::Instrument`, `SidecarType::Insert`, `SidecarType::NeuralAnalyzer`, or `SidecarType::NeuralProcessor`
- **Tags:** Array of string category tags (`"delay"`, `"neural"`, `"insert"`, `"real-time"`, `"instrument"`, `"eq"`, `"saturation"`, `"algorithmic"`)
- **Description:** Summary of DSP/neural modeling behavior
- **Latency Samples:** Algorithmic lookahead or processing delay

`SidecarStore` supports fast tag filtering:
- `store.filter_by_tag("neural")`
- `store.filter_by_tags(&["neural", "insert", "real-time"])`
- `store.filter_by_type(SidecarType::Insert)`

### Built-in Real-Time Sidecar Library
`SidecarStore` includes built-in real-time neural and algorithmic sidecars:
1. **`neural-saturation`** (`NeuralProcessor` / `Insert`): TCN / Padé approximant neural saturation with matrix wave-shaping.
2. **`neural-filter`** (`NeuralProcessor` / `Insert`): Hypernetwork dynamic filter with Padé SIMD non-linearities.
3. **`algorithmic-delay`** (`Insert`): Low-latency tape delay with Hermite fractional interpolation, feedback, and dampening.
4. **`algorithmic-eq`** (`Insert`): Multi-mode State-Variable EQ / Filter (low-pass, high-pass, band-pass, peak).
5. **`algorithmic-synth`** (`Instrument`): Real-time dual-oscillator synthesizer instrument.

## Type-Safety & ABI Invariants

1. **Alignment:** All `AudioBlock` payloads MUST be 64-byte aligned and 1088 bytes in size (including padding).
2. **Serialization:** All non-Pod types MUST be serialized using `bincode` with standard configuration.
3. **Real-time Safety:** Message handlers in the Conductor MUST NOT allocate on the audio thread. Type 3 blocks are routed through pre-allocated SPSC queues.

## Security & Isolation

- **P2P DNA Sync:** As of 2026-07-07, P2P DNA sync via `CloudPeerSync` is limited to trusted/paired peers in a closed studio LAN. Unauthenticated DNA pulls from unknown network nodes are prohibited by an allow-list in the `DiscoveryService`. Currently, only localhost (127.0.0.1:9003) is trusted by default for testing.
- **Remote Execution:** Sidecar processes are isolated via cgroups and restricted memory limits. Protocol level authentication for remote sidecars is planned for Stage 7.
