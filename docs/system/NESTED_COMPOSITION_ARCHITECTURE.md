# Extended Nested Composition Architecture Specification

This specification documents Nullherz's core architectural principle and design for **Nested Compositions, Semantic Ports, Signal/Feature/Control Graphs, Memory, and Adaptive Execution**.

## Core Architectural Principle

> **Chain is the user-facing default. Composition is the underlying abstraction. Mesh is a capability of a composition, not the mandatory structure of the entire application.**

The system feels like a professional DJ / composition DAW at the top level (linear chains), while internally every processing element (Insert) is capable of hosting a complete, arbitrarily complex nested composition graph.

```text
Deck A / Track
  ├── EQ
  ├── Compressor
  ├── Hybrid Insert (Nested Composition)
  │    ├── Analyzer (RMS / Transients / FFT)
  │    ├── Feature Graph -> Semantic Ports
  │    ├── Neural Controller (Control Graph)
  │    ├── DSP / Neural Processor
  │    └── Mixer
  ├── Delay
  └── Limiter
```

---

## 1. Composition Hierarchy

A **Composition** is the fundamental processing graph abstraction. It can contain:
- DSP nodes
- Neural nodes (TCNs, SSMs)
- Analyzer nodes
- Generator nodes (Synthesizers / Samplers)
- Modulators
- Memory nodes
- Nested Compositions / Inserts

```text
Session
  └── Deck / Track
       └── Chain Composition
            ├── EQ
            ├── Compressor
            ├── Hybrid Insert
            │    └── Internal Composition
            │         ├── Analyzer
            │         ├── DSP
            │         ├── Neural
            │         ├── Nested Insert
            │         │    └── Composition
            │         └── Mixer
            ├── Delay
            └── Limiter
```

---

## 2. Separate Signal, Feature, and Control Graphs

Processing is decoupled into three interconnected domains:

### Signal Graph
Audio sample-rate path:
$$\text{Audio} \longrightarrow \text{DSP / Neural} \longrightarrow \text{Audio}$$

### Feature Graph
Frame-rate / event-rate analysis path:
$$\text{Audio} \longrightarrow \text{Analyzer / FFT} \longrightarrow \text{Features (RMS, LUFS, Centroid, Transients)}$$

### Control Graph
Parameter update & modulation path (~50–200 Hz):
$$\text{Features} \longrightarrow \text{Neural / Macro Controller} \longrightarrow \text{Parameters / Routing}$$

```text
Audio In ───► Analyzer ───► Features ───► Neural Controller ───► Parameters
  │                                                                   │
  └───────────────────────────► DSP / Neural DSP ◄────────────────────┘
                                      │
                                  Audio Out
```

---

## 3. Semantic / Musical Ports

Rather than routing raw audio everywhere, compositions exchange standardized semantic control and feature signals via `SemanticPort`:

- **Transport / Clock:** `Tempo`, `BeatPhase`, `BarPhase`, `PhrasePhase`
- **Tonal / Harmonic:** `Key`, `Pitch`, `HarmonicDensity`, `VocalPresence`
- **Dynamic / Spectral:** `Rms`, `Lufs`, `Peak`, `BassEnergy`, `MidEnergy`, `HighEnergy`, `SpectralCentroid`, `SpectralFlux`, `TransientDensity`, `RhythmicDensity`
- **Context:** `DeckId`, `MasterEnergy`

This enables cross-deck feature awareness (e.g., Deck A beat phase modulating Deck B filter cutoff) without mixing audio channels.

---

## 4. Latency Propagation & Zero-Copy Execution

- **Declared & Effective Latency:** Every `CompositionNode` declares its latency (`latency_samples()`). A parent `Composition` calculates its `effective_latency()` as the maximum or summed path delay across its internal topology and reports this contract to the parent graph.
- **Flattening & In-Place Execution:** The graph compiler flattens nested compositions into unified execution schedules using double-buffered ping-pong scratch buffers (`scratch_a`, `scratch_b`). This presents **exactly one IPC boundary** to `nullherz-conductor`, keeping end-to-end processing deterministic (<1ms latency) with zero heap allocations during real-time callbacks.

---

## 5. Shadow Mode & Confidence Blending

Compositions support **Shadow Execution Mode**, allowing new neural models or experimental DSP algorithms to run side-by-side with trusted deterministic DSP:

$$\text{Output} = (1 - \text{mix} \times \text{confidence}) \cdot \text{Traditional DSP} + (\text{mix} \times \text{confidence}) \cdot \text{Neural DSP}$$

If model confidence drops or an inference deadline is missed, the system non-blockingly falls back to the deterministic DSP path without audio interruption.

---

## 6. Execution Classes & Hardware Autonomy

Nodes declare their execution class budget:
- **`Realtime`:** <1ms deterministic audio path (zero allocations, zero locks).
- **`Fast`:** 1–3ms frame-rate feature extraction.
- **`Control`:** ~50–200Hz parameter automation.
- **`Async`:** Off-thread background neural inference workers.
- **`Offline`:** Bounce & pre-rendering.

Logical compositions are hardware-independent: the runtime selects between `tiny-cpu`, `medium-cpu`, `medium-gpu`, and `high-quality-gpu` model implementations based on available hardware without altering the logical graph.
