# DNS & DNA Architectural Glossary

**Document Version:** 1.0.0
**Status:** Architecture Specification & Reference Standard
**Date:** March 2026
**Crates & Modules:** `nullherz-traits`, `nullherz-dna`, `nullherz-processors`, `nullherz-conductor`, `audio-dsp`

---

## Overview

This document serves as the authoritative architectural glossary and terminology reference for the **Nullherz Digital Musical Signature (DNS)** and **Musical Digital/Descriptive Neural/Normalized Architecture Representation (DNA)** substrate.

It provides precise definitions to ensure consistent usage across specifications, source code comments, UI labels, and developer communications.

---

## Core Terms

### DNS — Digital / Descriptive Musical Signature

DNS is the **observational representation of actual audio**.

It answers:
> *What characteristics can we measure from this audio?*

DNS is derived from an audio signal through acoustic analysis (`AnalysisKernel`, `audio-dsp`).

It may contain:
* Spectral information (FFT magnitude/phase, spectral centroid, brightness, tilt)
* Temporal information (envelope, transients, decay rates)
* Rhythm observations (onset masks, swing ratio, micro-timing offset arrays)
* Transient observations (attack sharpness, crest factor)
* Energy and dynamics (RMS, peak, loudness profile)
* Harmonic observations (chroma vector, root key, tonal dissonance)
* Spatial characteristics (stereo width, early reflection density)
* Fingerprints, embeddings, and acoustic landmarks
* Observation confidence metrics per domain

DNS is primarily **descriptive and observational**. It is an acoustic measurement of physical audio waveforms and must not be treated as a unique or perfect representation of musical intent.

```text
AUDIO
  ↓
DNS ANALYSIS (AnalysisKernel)
  ↓
DNS
```

---

### DNA — Musical Digital/Descriptive Neural/Normalized Architecture Representation

**DNA** is the established project name for the structured, manipulable, and transformable musical intermediate representation (IR) derived from DNS, MIDI, procedural generators, and performance inputs.

DNA answers:
> *What musical characteristics exist, how are they related, and how can they be transformed?*

DNA is not merely a scalar feature vector or fingerprint. It is a **multidimensional, structured musical intermediate representation** that can be:
* Routed through engine pipelines
* Transformed and morphed
* Constrained via masks and invariants
* Compared and diffed
* Bred and mutated
* Interpolated and modulated
* Recorded as timeline automation
* Branched, merged, and version-controlled
* Compiled and rendered
* Closed-loop verified against DNS observations

```text
DNS / MIDI / Procedural / Controllers
                 ↓
                DNA
                 ↓
          Transformation
                 ↓
  DSP / Neural / MIDI / Synthesis Renderers
                 ↓
               Audio
```

*Note: DNA is not required to be audio-reversible. It represents characteristics and relationships that can be rendered back into audio only to the extent that appropriate renderers exist.*

---

## DNS vs DNA

The foundational distinction between DNS and DNA is summarized below:

| Concept | DNS | DNA |
| :--- | :--- | :--- |
| **Primary Role** | Observation | Manipulation |
| **Question** | What is present in this audio? | What should change, how, and where? |
| **Source** | Primarily audio signals | DNS, MIDI, audio, procedural & performance sources |
| **Nature** | Descriptive / Acoustic | Structured / Operational / Transformable |
| **Mutable** | Generally no (read-only measurement) | Yes (first-class transformable signal) |
| **Routable** | Limited / analysis-oriented | First-class graph signal |
| **Transformable**| Indirectly (via audio re-analysis) | Directly via domain-specific operators |
| **Renderable** | Not directly renderable | Through available DSP / Neural / Synthesis backends |
| **Confidence** | Fundamental (extraction uncertainty) | Propagated from sources through transformations |
| **Provenance** | Analysis algorithm & timestamp | Transformation lineage and donor ancestry |

### Conceptual Mental Model
```text
DNS = observation
DNA = musical state + structure + intent + transformability
```

---

## Detailed Glossary Index

### DNS Terms

#### DNS Analysis
The process of extracting observable acoustic characteristics from audio waveforms. DNS analysis may be real-time, control-rate, offline, multiresolution, or probabilistic. It exposes uncertainty where analysis is underdetermined or ambiguous.

#### DNS Landmark
A significant, measurable event or feature within DNS. Examples include transient onsets, downbeats, harmonic key shifts, spectral brightness transitions, and phrase boundaries. Landmarks establish temporal correspondence across different musical sources.

#### DNS Fingerprint
A compact, identification-oriented representation derived from audio. Optimized for acoustic matching or track recognition rather than full musical manipulation. A fingerprint is not equivalent to complete DNA.

#### DNS Embedding
A learned or engineered multidimensional latent vector representing audio or musical characteristics (e.g. 16-D spectral latent space). Useful as one component of DNS or DNA, though an embedding alone does not necessarily provide interpretable or controllable musical dimensions.

#### DNS Confidence
A explicit scalar or tensor value $[0.0, 1.0]$ representing extraction confidence in an observed DNS characteristic. Confidence remains distinct from the measured value itself.

---

### DNA Structural & Routing Terms

#### DNA Domain
A semantically meaningful category of DNA (e.g. `rhythm`, `groove`, `bass_motion`, `timbre`, `energy`, `density`, `harmony`, `structure`, `atmosphere`, `vocal_identity`). Each domain possesses its own representation, resolution, distance metric, and transformation semantics.

#### DNA Field
A spatial, temporal, or structural distribution of DNA values (e.g. $\text{DNA}[t, \text{domain}]$). Allows characteristics to vary across time, frequency bands, section boundaries, or instrument roles.

#### DNA Tensor
A multidimensional numerical representation of DNA (e.g. $\text{rhythm}[\text{phrase}, \text{bar}, \text{subdivision}, \text{voice}]$). A tensor is a mathematical representation mechanism, not the semantic definition of DNA itself.

#### DNA Graph
A directed acyclic or bounded-feedback graph representing operational relationships between DNA states, domains, transformations, sources, and destinations.

#### DNA Signal
A DNA value, field, event, tensor, or graph state traveling through the musical signal control plane. Distinct from audio sample frames.

#### DNA Bus
A routing mechanism for DNA signals. Conceptually analogous to an audio mix bus, but distributing musical characteristics to multiple downstream transformation consumers.

#### DNA Send / Return
A routing path allowing a DNA signal to be tapped, routed to an off-chain DNA processor, and returned to the primary graph.

#### DNA Source
Any entity capable of providing DNA or information from which DNA can be derived (audio files, DNS analysis, MIDI streams, controller inputs, procedural generators, or project snapshots).

#### DNA Target
The desired destination DNA state or characteristic toward which a transformation moves.

#### DNA Carrier
The primary musical material being transformed. The carrier provides the base audio foundation whose characteristics are selectively modified.

#### DNA Donor
A reference source whose characteristics influence the carrier without necessarily replacing the carrier's audible foundation.

---

### DNA Transformation & Operations

#### DNA Transfer
Moving selected characteristics from a donor toward a carrier (e.g. applying Track B's groove to Track A). Transfer does not imply literal waveform copying.

#### DNA Breeding
Combining characteristics from multiple DNA parent sources into a new offspring state using domain-specific crossover and mask rules.

#### DNA Mutation
Controlled, deterministic or stochastic modification of DNA dimensions without requiring an explicit donor reference.

#### DNA Morph
A continuous or structured transition between two or more DNA states across continuous parameter space or time.

#### DNA Transformation
Any operation that alters DNA state (transfer, breed, mutate, morph, follow, attract, repel, constrain).

#### DNA Constraint
A rule restricting how DNA may change during a transformation (e.g. locking melody root or preserving vocal formant regions).

#### DNA Mask
A selective constraint describing where, when, or on which domains/regions a transformation is allowed to operate.

#### DNA Invariant
A protected property intended to remain completely unchanged during transformation, acting as a musical conservation law.

#### DNA Protection
The enforcement mechanism preserving invariants and protected characteristics against destructive transformation.

#### DNA Distance
A domain-specific, non-Euclidean measure of difference between two DNA states.

#### DNA Compatibility
A multidimensional measure $[0.0, 1.0]$ evaluating how safely or harmoniously two DNA states can interact without producing acoustic artifacts.

---

### DNA Physics & Dynamics

#### DNA Pressure
The force or intensity with which one DNA state attempts to influence another.

#### DNA Momentum
The velocity and directional inertia of an ongoing DNA transformation across time or bars.

#### DNA Inertia
Resistance of a specific DNA domain to change. High-inertia domains (e.g. vocal identity) change slowly, while low-inertia domains (e.g. atmosphere) adapt rapidly.

#### DNA Friction
The conceptual difficulty, computational cost, or perceptual resistance associated with transforming a specific characteristic.

#### DNA Mass
A conceptual measure of the perceptual weight and identity significance of a DNA characteristic.

#### DNA Force
The effective driving influence of a transformation, conceptually modeled as:
$$\text{force} = \text{pressure} \times \text{compatibility} \times \text{confidence}$$

#### DNA Attractor
A target state toward which DNA is intentionally pulled across transformation space.

#### DNA Repulsor
A state or characteristic from which a transformation deliberately moves away to increase contrast or avoid repetition.

#### DNA Energy
A conceptual measure of the degree of structural and characteristic change introduced by a transformation. Distinct from audio RMS or acoustic volume.

#### DNA Entropy
A conceptual measure of diversity, randomness, or structural complexity within a DNA state.

#### DNA Novelty
A metric measuring how substantially a newly generated state differs from existing historical states in a project or library.

---

### DNA Versioning, State & Lineage

#### DNA Ghost
A preserved historical DNA state functioning as a non-destructive reference or target without directly outputting audio.

#### DNA Snapshot
A frozen multidimensional DNA state capturing a track or scene configuration at a specific moment.

#### DNA Scene
A snapshot combined with associated routing, modulation, and transformation parameters required to transition the system into that musical state.

#### DNA Branch
A non-destructive alternative evolutionary path derived from a parent DNA state.

#### DNA Merge
Recombining independently modified DNA branches back into a single primary lineage.

#### DNA Lineage
The complete ancestral history and transformation chain of a DNA state.

#### DNA Provenance
Detailed audit metadata recording the precise origin, donor tracks, algorithms, and parameter states that produced a DNA characteristic.

#### DNA Correspondence
The temporal, structural, or phrase-level mapping between related regions in different musical sources.

#### DNA Warping
Adjusting temporal or structural grids so that non-aligned sources can establish meaningful DNA correspondence.

#### DNA Topology
The relational structure and connectivity of DNA states within a neighborhood or transformation space.

#### DNA Neighbourhood
The local region of musically related states surrounding a given DNA point.

#### DNA Attractor Field
A region in DNA space where transformation vectors converge toward one or more target attractors.

#### DNA Phase Transition
A critical threshold where gradual numerical parameter movement produces a qualitative, perceptual shift in musical state.

#### DNA Collision
A conflict arising when competing transformations or mutually exclusive constraints target overlapping DNA characteristics.

#### DNA Arbitration
The deterministic or rule-based resolution of DNA collisions and constraint conflicts.

#### DNA Transaction
A group of DNA modifications executed as an atomic, reversible unit with explicit commit/rollback semantics.

#### DNA Rehearsal
A non-destructive, preview execution of a proposed transformation before committing it to the project state.

---

### DNA Compiler, Execution & Verification

#### DNA Compiler
The translation engine that converts high-level musical intent and DNA transformation graphs into concrete DSP, neural, MIDI, and routing parameter commands.

#### DNA Renderer
An execution backend (DSP insert, neural sidecar, wavetable synth, MIDI generator) capable of realizing a compiled DNA plan.

#### DNA Capability
A renderer's declared ability to observe, transform, or render specific DNA domains.

#### DNA Capability Negotiation
The pre-flight process where the compiler queries available renderers to match transformation intent against hardware and DSP capabilities.

#### DNA Fallback
An alternative execution path selected when a preferred renderer is unavailable, excessive in CPU cost, or uncertain in analysis.

#### DNA Graceful Degradation
Preserving as much of the requested musical transformation as possible when operating under constrained execution budgets.

#### DNA Confidence
The propagated confidence level associated with a DNA state or transformation result.

#### DNA Uncertainty
The degree to which a DNA representation or transformation is underdetermined, ambiguous, or subject to analysis limits.

#### DNA Uncertainty Propagation
Carrying uncertainty bounds explicitly through every stage of the transformation pipeline:
$$\text{DNS Confidence} \to \text{DNA Confidence} \to \text{Transform Confidence} \to \text{Renderer Confidence}$$

#### DNA Observability
The capacity to inspect, scope, diff, and audit DNA states, trajectories, and confidence metrics in real time.

#### DNA Diff
A structured, domain-specific comparison object detailing the exact dimensional differences between two DNA states.

---

### DNA Performance & Control

#### DNA Gesture
A physical user interaction (fader move, knob rotation, touch gesture, MIDI CC) controlling DNA transformations in real time.

#### DNA Performance Macro
A high-level performance control coordinating multiple underlying DNA dimensions simultaneously (e.g. `MORPH`, `ABSORB`, `DISSOLVE`, `HYBRIDIZE`).

#### DNA Crossfade
A multidimensional transition curve operating across DNA domains in parallel with, or independently of, conventional audio waveform crossfading.

#### DNA Clock
A timing source driving the execution rate of DNA events and modulations (sample, transient, beat, bar, phrase, section, MIDI clock).

#### DNA Event
A discrete trigger or state change occurring within the DNA control plane.

#### DNA Role
A functional musical classification (e.g. `kick`, `bass`, `vocal`, `lead`, `pad`, `percussion`, `atmosphere`) enabling role-specific transformation.

#### DNA Behaviour
A recurring, transferable pattern of musical change over time (e.g. tension/release curves, energy buildup) independent of literal pitch or waveform content.

#### DNA Motif
A recurring structural or behavioural pattern existing at the DNA level.

#### DNA Grammar
A set of rule systems describing valid evolutionary trajectories for musical characteristics across sections.

#### DNA Physics
The conceptual abstraction governing momentum, inertia, mass, friction, force, attraction, and collision during DNA transformations.

#### DNA Programming
The overarching paradigm of expressing musical composition and production intent by programming relationships between musical characteristics rather than automating individual low-level audio parameters.

---

## Terminology Rules & Standard Conventions

To prevent confusion across the codebase and documentation, adhere strictly to the following rules:

1. **DNS vs DNA**:
   * Use **DNS** strictly for observational measurements extracted from audio.
   * Use **DNA** for structured, transformable, and routable musical representations.
   * Never use DNS and DNA interchangeably.

2. **No Fingerprint Misnomer**:
   * Do not describe DNA as a "fingerprint". Fingerprints are compact, read-only identifiers; DNA is a rich, multidimensional intermediate representation.

3. **No Reversibility Assumption**:
   * Never imply that DNA is losslessly reversible back into original audio waveforms. Rendering DNA depends entirely on available synthesis and neural backends.

4. **No AI Equivalence**:
   * Do not use "AI" as a synonym for DNA. Neural networks are backends for analysis or rendering; DNA is the architectural signal domain and representation.

---

## Naming Principle

When a term could be confused with conventional audio or DSP concepts, **explicitly prefix or qualify it with `DNA`**:

* `DNA energy` (distinct from acoustic RMS/loudness)
* `DNA distance` (distinct from physical or spatial distance)
* `DNA pressure` (distinct from acoustic sound pressure / SPL)
* `DNA momentum` (distinct from mechanical momentum)
* `DNA field` (distinct from electromagnetic or acoustic field)
* `DNA bus` (distinct from audio mix bus)
* `DNA event` (distinct from MIDI event)
* `DNA renderer` (distinct from graphics or audio engine renderer)

This ensures complete clarity across technical specifications and source code implementation.
