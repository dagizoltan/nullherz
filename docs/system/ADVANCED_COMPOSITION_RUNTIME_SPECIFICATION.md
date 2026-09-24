# Advanced Audio Composition Runtime Specification

This specification details the advanced extensions to Nullherz's **Nested Composition, Event Graph, Typed Signal System, Feature Bus, and Adaptive Execution Engine**.

---

## 1. Encapsulated Processing Capsules & State Migration

An Insert is an encapsulated processing capsule. The parent composition consumes only its declared `CompositionContract`, latency, resource requirements, and quality modes without needing to inspect internal implementation graphs.

### State Migration Mechanism
When a composition or model undergoes a version upgrade, topology swap, or hot-swap mutation, its active runtime state (delay lines, filter memory, oscillator phase, neural hidden state) migrates gracefully via state migration descriptors:

$$\text{State}_{\text{v1}} \xrightarrow[\text{Migration Policy}]{\text{Transform}} \text{State}_{\text{v2}}$$

Unmigrated memory states fall back to smooth crossfaded zero-resets to prevent audible clicks or transients during live playback.

---

## 2. Multi-Rate Automation & Typed Signal System

Signal routing across compositions is strongly typed (`SignalType`):

- **`Audio`:** Sample-rate (48 kHz) audio signals.
- **`Feature`:** Frame-rate (50–200 Hz) spectral/rms descriptors.
- **`Control`:** Automation-rate parameter signals.
- **`Event`:** Discrete timestamped musical events.
- **`Spectrum` / `Envelope` / `Pitch` / `BeatPhase` / `SpatialField`:** Specialized domain signals.

### Multi-Rate Update Resolutions (`AutomationRate`)
- **`AudioRate`:** Evaluated per sample ($48,000\text{ Hz}$).
- **`ControlRate`:** Evaluated per control sub-block (~$100\text{ Hz}$).
- **`EventRate`:** Evaluated on discrete timestamp triggers (e.g. downbeats, bar boundaries).

---

## 3. Event Graph & Timestamped Trigger Domain

In addition to audio and control buses, compositions incorporate an **Event Graph** domain handling discrete timestamped events (`EventKind`):

- **Musical Triggers:** `Beat`, `Bar`, `Phrase`, `Drop`, `Break`
- **Performance Triggers:** `TrackStart`, `TrackEnd`, `SceneChange`, `DeckChange`
- **Engine Triggers:** `TopologyChange`, `ModelReady`

```text
Phrase Event (8 bars before Drop) ───► Event Router ───► Scene Transition (Neural Space -> Atmospheric)
```

---

## 4. Transactional Topology Changes & Model Lifecycle

Live graph mutations occur as atomic transactions (`CompositionTransaction`):

$$\text{REQUESTED} \longrightarrow \text{LOADING} \longrightarrow \text{INITIALIZING} \longrightarrow \text{WARMING} \longrightarrow \text{BENCHMARKING} \longrightarrow \text{READY}$$

1. **Staging:** A new topology or neural model variant is loaded and initialized off-thread.
2. **Warm-Up:** Ring buffers and SIMD state vectors are pre-warmed.
3. **Commit:** At a safe musical boundary (sample, beat, or bar), the engine atomically swaps the active pointer using `ShmSignal` / atomic swaps with zero xruns or audio dropouts.

---

## 5. Global Analysis Feature Bus & Spatial Domains

### Analysis Feature Bus
Instead of wiring individual point-to-point connections between decks, the engine provides an `AnalysisBus` where analyzers broadcast musical features (`Tempo`, `BeatPhase`, `Key`, `Energy`, `Transients`, `VocalPresence`). Compositions subscribe to authorized feature ports across decks without cross-mixing audio.

### Spatial Signal Domains (`SpatialDomain`)
Compositions declare supported spatial representations:
- `Mono`, `Stereo`, `MidSide`, `Multichannel`, `Ambisonic`

The compiler automatically inserts format conversion nodes when connecting mismatching spatial domains.

---

## 6. Priority-Aware Degradation & Quality Budget

Under severe CPU/GPU or thermal constraints, processing degrades gracefully based on `ProcessingPriority`:

1. **`Critical`:** Transport, Master Output, Limiter (Never dropped).
2. **`High`:** Primary Channel EQs and Compressors.
3. **`Medium`:** Neural Controllers and Advanced Feature Analyzers.
4. **`Low`:** Shadow neural processing, diagnostics, and non-essential FX branches.

### Quality Budget (`QualityBudget`)
Configurable quality mode ($0..100$) selects model sizes, FFT frame sizes, oversampling ratios, and branch complexity (`Live`: 40, `Performance`: 70, `Composer`: 85, `Offline`: 100).

---

## 7. Deterministic Performance Recording & Controlled Randomness

Generative and neural compositions consume controlled, seeded pseudo-randomness (`seed`, `variation_amount`, `variation_rate`). Performance sessions record:
- Composition version & graph snapshot
- Transactional scene changes
- Parameter automation & MIDI events
- Random seeds and model versions

Replaying a session with matching random seeds produces **100% deterministic audio playback**.
