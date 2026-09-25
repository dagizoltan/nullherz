# Advanced Musical Signal Graph & DNA Instrumentation Substrate

**Document Version:** 1.0.0
**Status:** Architecture Specification & System Design RFC
**Date:** March 2026
**Crates & Modules:** `nullherz-dna`, `nullherz-traits`, `nullherz-processors`, `nullherz-conductor`, `nullherz-topology`, `nullherz-inspector`, `audio-dsp`

---

## Executive Summary

This specification extends Nullherz's **Musical Transformation Engine** and **DNA Instrumentation & Tooling** framework. It details the long-term architectural evolution of DNA when treated as a **patchable, routable, modulatable, and performable musical signal substrate**.

> **Core Axiom:** DNA is a musical signal and control domain, not a conversational AI interface.
>
> The system operates as a DAW, modular synthesizer, scientific instrument, performance controller, and Musical Signal Graph—allowing musicians to manipulate musical characteristics directly rather than prompting an AI generator.

---

## Part 1: DNA Routing & Signal Substrate

### 1.1 DNA Buses
Analogous to audio summing buses, **DNA Buses** aggregate musical characteristic streams from multiple track or patch sources to expose composite reference DNA signals:

```text
TRACK A ──┐
TRACK B ──┼──► RHYTHM BUS    ──► Composite Micro-timing & Groove Reference
TRACK C ──┘

TRACK A ──┐
TRACK D ──┼──► TIMBRE BUS    ──► Composite Spectral Energy & Tilt Reference
TRACK E ──┘
```

#### Domain-Specific Aggregation Laws
Simple arithmetic averaging across multidimensional DNA vectors produces unmusical acoustic blur. Aggregation is governed by domain-specific rules:
* **Rhythm Domain**: Multi-point transient density peak-holding and dominant phase-locking (e.g. Euclidean union or envelope MAX).
* **Timbre Domain**: Octave band energy max-pooling combined with spectral centroid energy weighting.
* **Bass Domain**: Envelope peak-following with sub-bass frequency priority.
* **Spatial Domain**: Maximum stereo width envelope selection.

### 1.2 DNA Sends and Returns
DNA signals can be tapped from any node in the graph, routed via adjustable send levels, processed by dedicated DNA transformation modules, and returned to target track or engine parameters:

```text
                 ┌──► Track A (Groove Mod)
DNA SOURCE ──────┼──► Track B (Spectral Morph)
 (Send Bus)      └──► Track C (Saturation Conditioning)
```

### 1.3 Typed DNA Ports
To prevent invalid signal connections in complex graphs, DNA ports expose explicit static types and metadata:

```text
Port Categories:
  • RHYTHM     [Resolution: Beat/Event | Dimensions: 12-slot microtiming, swing]
  • TIMBRE     [Resolution: Control    | Dimensions: 8-band octave energy, centroid]
  • DENSITY    [Resolution: Bar        | Dimensions: Scalar events/sec]
  • ENERGY     [Resolution: Sub-block  | Dimensions: RMS, Crest Factor, Peak]
  • HARMONY    [Resolution: Phrase     | Dimensions: Root Key, Pitch Contour]
  • STRUCTURE  [Resolution: Section    | Dimensions: Phrase Length, State ID]
```

### 1.4 DNA Impedance Matching
When a DNA stream produced by one source is routed into a consumer requiring a different representation or temporal scale, an explicit **DNA Impedance Matcher** aligns the signals:

```text
DNA Source A (High-Res Event Onsets)
      │
      ▼
[ IMPEDANCE MATCH ] ──► Handles: Resolution, Domain Availability, Coordinate System,
      │                          Confidence, Temporal Scale, Renderer Capability
      ▼
DNA Consumer B (Control-Rate Spectral Filter)
```

### 1.5 DNA Converters and Adapters
Modular adapter units convert signals across different representation layers:
* **Rhythm-Event DNA ──► Rhythm-Density DNA**: Converts discrete onset timestamps into continuous event density curves.
* **Spectral DNA ──► Timbre-Control DNA**: Translates latent vectors into multiband equalizer shelf parameters.
* **Energy DNA ──► Mutation Rate**: Drives stochastic perturbation intensity from live RMS energy.

---

## Part 2: Dynamic Transformation Physics & Conservation Laws

### 2.1 DNA Attractors and Repulsion
DNA transformations do not always require explicit point targets. Instead, they operate within **Attraction and Repulsion Fields**:

```text
        Attractor Region (Target Zone)
             ╱       │       ╲
           ●─────────●─────────●  <── Transformation Trajectory
```

* **DNA Attractors**: Pull the current DNA state toward a target region (defined by a reference track, snapshot, or abstract genre manifold) governed by parameters: *attraction strength, radius, damping, domain weighting, and temporal response*.
* **DNA Repulsion**: Pushes the DNA state *away* from specified regions (e.g., avoiding repetitive rhythm, harsh spectral glare, or mud in the sub-bass).

### 2.2 Bounded DNA Feedback Networks
DNA output from a transformation chain can be fed back into upstream DNA inputs to create evolving musical systems:

```text
AUDIO ──► DNS ANALYSIS ──► DNA TRANSFORMATION ──► AUDIO
  ▲                                │
  └────────────── DNA FEEDBACK ────┘
```

#### Realtime Safety & Stability Limits
* **Maximum Divergence Limits**: Clamps maximum parameter shift relative to carrier baseline.
* **Domain Damping**: Exponential decay filters prevent infinite accumulation.
* **Deterministic Seeding**: Feedback loops support fixed random seeds for 100% reproducible execution.

### 2.3 Conservation Laws & Minimum-Change Optimization
Transformations can enforce strict conservation rules, acting as a constrained musical simulation:

```text
CONSERVE:
    melody_identity    (100% invariant)
    vocal_core        (100% invariant)
    tonal_center      (100% invariant)

MODIFY:
    groove_microtiming (Target: Track B)
    timbre_tilt        (Target: Track B)
```

#### Minimum-Change Transformation
The engine computes the optimal transformation vector that satisfies target criteria while minimizing total musical perturbation:

$$\min \Delta \text{DNA} \quad \text{subject to} \quad \begin{cases} \text{rhythm\_similarity}(\text{out}, B) \ge 0.85 \\ \text{melody\_deviation}(\text{out}, A) \le 0.05 \end{cases}$$

### 2.4 Domain-Specific Metrics & Topology
Distance in DNA space is not measured using simple Euclidean math across a uniform vector. Each domain employs domain-specific metrics:
* **Rhythm Metric**: Circular Earth Mover's Distance over phase-wrapped beat grids.
* **Timbre Metric**: Geodesic distance on the spectral latent manifold.
* **Topology & Landmarks**: Identifies topological clusters ("Groove Families") and section landmarks (`Intro`, `Drop`, `Break`).

---

## Part 3: Execution, Multi-Resolution & Control Infrastructure

### 3.1 Multi-Resolution DNA Architecture
DNA operates across multiple decoupled temporal resolutions:

```text
microtiming   ──► Event-level (Sub-sample / Transient timestamps)
groove        ──► Beat-level (1/16th grid offsets)
density       ──► Bar-level (Events per measure)
arrangement   ──► Phrase-level (8/16-bar energy trends)
structure     ──► Section-level (Intro, Chorus, Outro state)
```

### 3.2 Clock Domains, Triggers & Events
DNA operations subscribe to appropriate clock domains (`SampleClock`, `BlockClock`, `BeatClock`, `BarClock`, `PhraseClock`). Transformations can be fired dynamically via **DNA Triggers**:

```text
ON DOWNBEAT:
    trigger_snapshot_blend(Target: Snapshot_B, Duration: 1 Bar)

ON DROP:
    unlock_timbre_transfer()
    set_rhythm_mutation(Amount: 0.35)
```

### 3.3 DNA Inertia and Differential Control
* **DNA Inertia**: Imposes response ballistics (attack, decay, smoothing) on DNA target transitions to prevent sudden acoustic steps.
* **Differential DNA Control ($\Delta\text{DNA}$)**: Controls changes relative to the current state ($\Delta \text{timbre} = +10\%$) rather than forcing absolute scalar values, crucial for live performance.

### 3.4 Contextual Normalization & Anchors
* **Contextual Normalization**: Normalizes DNA values against the track's internal context (e.g. 50% density in an ambient track vs. a drum & bass track).
* **DNA Anchors**: Explicitly locks specific musical characteristics (e.g. kick identity, vocal melody) while allowing surrounding domains to morph.

---

## Part 4: Version Control, Branching & Performance Tools

### 4.1 DNA Inheritance & Lineage Graph
DNA inheritance supports domain-specific gene allocation across parent tracks, tracked in a immutable lineage tree:

```text
Root Track A
 ├── Branch A1 (Darker Timbre)
 │    ├── Branch A1a (Rhythm Transfer B)
 │    └── Branch A1b (Mutation Pass)
 └── Branch A2 (Hybrid B/C Breed)
```

### 4.2 Cheap Branching & DNA Merge
* **Cheap Branching**: Branches share immutable underlying audio and analysis data until divergence, minimizing memory overhead.
* **DNA Merge**: Recombines divergent branches with domain-specific conflict resolution (e.g. taking rhythm from Branch A1a and timbre from Branch A2).

### 4.3 DNA Probes, Laboratory Mode & Sampling
* **DNA Probes**: Inspection points attached to graph edges to monitor tensor shapes, confidence, latency, and CPU usage.
* **DNA Laboratory Mode**: An experimental non-timeline interface for testing multi-source DNA combinations.
* **DNA Sampling & Slicing**: Captures and recombines 4-bar characteristic profiles independently of raw audio data.

### 4.4 DNA Scenes, Morphing & Performance Choreography
* **DNA Scenes**: Store macro configurations of DNA transformations, switchable live.
* **DNA Scene Choreography**: Allows different domains to morph between scenes at different rates (e.g. fast rhythm transition, slow timbre fade).
* **DNA Layering**: Stack compositional layers (*Base + Performance + Reference + Automation + Mutation*) without destroying the original state.

---

## Part 5: Quality, Telemetry & Verification Infrastructure

### 5.1 Telemetry & Observation Tools
* **DNA Ghosts**: Retains previous DNA states as visual or control ghost references to allow moving toward or away from past states.
* **Differential Visualization**: Displays *Current*, *Target*, and *Delta ($\Delta$)* simultaneously in the UI.
* **"What Changed?" Telemetry**: Provides instant, explainable technical breakdowns of transformations.
* **DNA Rehearsal Mode & Transactions**: Audition transformations in lower resolution or preview buffers before committing atomic transactions.

### 5.2 Determinism & Invariance Testing
* **Deterministic Seeds**: All random mutations and chaotic maps take explicit seeds (`seed: u64`) for bit-exact reproducibility.
* **DNA Unit & Invariance Tests**: Closed-loop verification tests asserted as technical code contracts:
  ```rust
  assert!(rhythm_similarity(output, donor) > 0.80);
  assert!(melody_similarity(output, carrier) > 0.95);
  ```

### 5.3 DJ / Composition Bridge & UI Naming Taxonomy
* **DJ/Composition Bridge**: DNA scenes performed live in DJ Studio can be recorded, opened in Composer, edited as automation lanes, and exported as track presets.
* **UI Naming Taxonomy**: In user-facing controls, complex DNA vector math is presented in intuitive musical terms (*Character, Groove, Shape, Energy, Motion, Texture, Reference, Transform*), reserving technical DNA terms for advanced views.

---

## Part 6: Implementation Status & Architecture Classification

To ensure complete clarity across the engineering roadmap, all concepts are explicitly classified by implementation status:

### Tier 1: Currently Implemented Functionality (State of Workspace Tree)
* **`SoundDNA` & `SampleMetadata` Data Structures**: Planar buffers, peak envelopes, 3-band `BandWaveform`, beat-grid offsets, 16D latent space, 8-band feature vectors, 12-slot microtiming (`nullherz-traits`, `nullherz-dna`).
* **ed25519 Lineage Signing & redb Storage**: Signed DNA provenance and Smart Crate database (`nullherz-dna`).
* **Basic Transfusion & Morphing**: `NeuralTransfuser` (SIMD Slerp) and chaotic logistic map mutation (`nullherz-dna`).
* **Spectral DSP Inserts**: Real-time `DnaMorpher` and `PersonalityInheritanceProcessor` (`nullherz-processors`).
* **Sequencer Micro-Timing Sync**: Groove injection driven by `SetDeckSync` / `SetDeckKeySync` (`nullherz-conductor`).

### Tier 2: Previously Documented Architecture
* **Three Operational Modes**: Mode 1 (Waveform Mix), Mode 2 (Audio-Less Transfer), Mode 3 (Hybrid Mix) (`MUSICAL_TRANSFORMATION_ENGINE_SPECIFICATION.md`).
* **Triad Taxonomy & Three-Gate Rule**: Observable vs. Controllable vs. Renderable DNA and field capability metadata (`DNA_RISK_AND_CAPABILITY_FRAMEWORK.md`).
* **Real-time Zero-Allocation Neural Inserts**: TCN, SSM, HyperNetwork specifications (`NEURAL_DSP_INSERT_SPECIFICATION.md`).

### Tier 3: Proposed Architecture (Next System Design Horizon)
* **DNA Buses, Sends & Returns**: Multi-track aggregation and send/return routing in `ProcessorGraph`.
* **Typed DNA Ports & Impedance Matchers**: Type-checked DNA graph connections and resolution converters.
* **DNA Attractors & Repulsion Fields**: Attraction and repulsion field transformation operators.
* **Multi-Resolution Clock Domains**: Decoupled event, beat, bar, phrase execution clocks in `Conductor`.
* **DNA Timeline Automation & Scenes**: Timeline automation lanes and performance scenes in `nullherz-inspector`.
* **DNA Microscope, Diff & Scopes**: Dedicated egui visualization panels for differential inspection.
* **Closed-Loop Invariance Testing**: Automated verification gate executing `AnalysisKernel` post-render.

### Tier 4: Speculative / Future Research
* **Bounded DNA Feedback Networks**: Real-time closed-loop DNA feedback with stability limiters.
* **Minimum-Change Optimization Solvers**: Real-time constrained convex optimization for minimal transformation vectors.
* **Topological Manifold Learning**: Non-Euclidean topological distance metrics for groove families.
* **Direct Hardware DMA Control Mappings**: Ultra-low-latency FPGA/Baremetal DMA hardware mappings for DNA parameters.
