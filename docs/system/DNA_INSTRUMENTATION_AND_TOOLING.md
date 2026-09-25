# Architecture Specification: DNA Instrumentation & Tooling

**Document Version:** 1.0.0
**Status:** Conceptual & Architecture Specification
**Date:** March 2026
**Crates & Modules:** `nullherz-dna`, `nullherz-traits`, `nullherz-processors`, `nullherz-conductor`, `nullherz-inspector`, `audio-dsp`

---

## Executive Summary

This document formalizes the architectural thesis for **DNA Instrumentation & Tooling** within Nullherz, complementing the [Complete Musical DNA Architecture Specification](./MUSICAL_DNA_ARCHITECTURE_SPECIFICATION.md) and the [DNS & DNA Glossary](./DNS_DNA_GLOSSARY.md).

> **Core Axiom:** DNA is a musical instrumentation domain, not an AI conversation interface.

The system is designed to behave as a **DAW, modular synthesizer, signal-processing environment, scientific instrument, or performance controller** rather than a conversational prompt assistant or text-driven song generator. DNA represents an addressable, measurable, routable, modulatable, and transformable intermediate musical representation (IR) sitting between acoustic analysis and DSP/neural/MIDI execution backends.

---

## 1. DNA as a First-Class Signal Domain

In traditional digital audio workstations and audio software, data is divided into distinct signal domains:

```text
AUDIO       ────► Continuous sample-rate waveforms
MIDI        ────► Discrete event streams (Note On/Off, CC, Pitch Bend)
CONTROL     ────► Low-frequency control voltages and LFO signals
AUTOMATION  ────► Time-indexed parameter envelopes
ANALYSIS    ────► Time-frequency inspection data (FFT frames, RMS, Peak)
```

In Nullherz, **DNA is elevated to a first-class signal domain**:

```text
AUDIO
MIDI
CONTROL
AUTOMATION
ANALYSIS
DNA          ────► Multidimensional musical characteristic signal
```

### Signal Domain Invariants
1. **Routable Signal**: DNA is not merely static metadata attached to audio files on disk. It can be dynamically routed through the engine graph, tapped, processed, modulated, and sent to multiple target destinations.
2. **Addressable Characteristics**: Individual multidimensional domains of DNA (`rhythm`, `timbre`, `bass_motion`, `spatial_decay`, `harmonic_density`, `structure`) can be isolated and routed independently.
3. **Decoupled Carrier vs Reference**: Track A can provide the audio carrier foundation while Track B's live DNA signal acts as a modulation or transformation source.

### Topology Example
```text
Track A
 ├── AUDIO ───────────────► Mixer Bus
 ├── MIDI ────────────────► Synth Engine
 └── DNA ──► DNA Router ──► Processing Pipeline
                       ├── rhythm ─────► Micro-Timing Engine
                       ├── timbre ─────► Spectral Shelving
                       ├── dynamics ───► Saturation Envelope
                       └── structure ──► Pattern Mutation
```

---

## 2. DNA Modulation

Analogous to modulation routing in synthesizers (where LFOs modulate filter cutoffs, envelopes modulate amplitude, and sequencers modulate pitch), **DNA modulation** allows high-level musical characteristics to modulate target DSP, neural, and synth parameters across distinct temporal execution rates.

```text
Traditional Synthesis:
LFO           ──► Filter Cutoff
Envelope      ──► Amplitude / VCA
Sequencer     ──► Pitch / VCO

DNA Instrumentation:
DNA LFO       ──► Rhythmic Density
DNA Envelope  ──► Spectral Brightness / Tilt
Beat Phase    ──► Timbre Transfer Intensity
Bass Energy   ──► Saturation Character
Phrase Position──► Structural Mutation Rate
```

### Multi-Rate Execution Scales
DNA parameters update at rates appropriate to their musical domain. The system explicitly avoids assuming all DNA parameters update at sample rate:

| Execution Scale | Nominal Rate | Typical DNA Modulation Applications |
| :--- | :--- | :--- |
| **Sample Rate** | 44.1 / 48 kHz | Fast spectral phase alignment, sub-sample transient interpolation, DnaSideband audio processing. |
| **Control Rate** | ~100 Hz (sub-block) | Smooth parameter interpolation (`MixerCommand::SetParam`), Slerp curves, envelope-following. |
| **Beat Rate** | 1–4 Hz (transport-synced) | Groove micro-timing nudges, beat-quantized timbre morphs, slice-based saturation updates. |
| **Bar Rate** | ~0.1–0.5 Hz | Multi-bar spectral tilt automation, harmonic tension transitions, bass motion transfers. |
| **Phrase Rate** | ~0.02–0.1 Hz | Structural mutation, scene transitions, macro-scale energy curve morphing. |
| **Event / Discrete Rate** | Dynamic | Transient-triggered snapshot swaps, onset-driven mask mutations. |

---

## 3. DNA Automation and Recording

DNA transformations are fully recordable performance and automation signals on the arrangement timeline.

Rather than requiring a musician or producer to manually draw or record thousands of low-level DSP parameters (e.g. 32 band gain envelopes, biquad Q values, saturation thresholds), the timeline records high-level **DNA automation curves** that preserve semantic intent.

```text
Reference: Track B
Transfer Amount Curve:

100% │                                 ┌────────────────────────
     │                                /
     │                               /
  0% └──────────────────────────────┘
     0         Bar 8              Bar 16                    Bar 32
```

### Timeline DNA Automation Lanes
```text
Track A Timeline
 ├── Audio Clips
 └── DNA Automation Stack
      ├── Rhythm Transfer Amount  [0.0 ──► 1.0]
      ├── Bass Motion Transfer    [0.0 ──► 0.8]
      ├── Timbre Morph Ratio      [0.0 ──► 0.5]
      ├── Rhythmic Density        [0.2 ──► 0.9]
      └── Structural Mutation     [Mask: Frozen]
```

When recorded, the engine compiles high-level automation lanes into control-rate parameter streams, preserving semantic intent for offline editing, non-destructive tweaks, and session recall.

---

## 4. DNA as Modular Processing Objects

DNA operations are conceptualized as discrete modular processing primitives analogous to modular synthesizer hardware units (Eurorack modules):

```text
┌────────────┐     ┌─────────────────┐     ┌───────────────┐     ┌─────────────┐     ┌────────────┐
│ DNA SOURCE │ ──► │ RHYTHM TRANSFER │ ──► │ BASS TRANSFER │ ──► │ TIMBRE MORPH│ ──► │ DNA OUTPUT │
└────────────┘     └─────────────────┘     └───────────────┘     └─────────────┘     └────────────┘
```

### Processing Primitive Vocabulary

1. **DNA Source**: Generates or reads static/streaming `SoundDNA` from track analysis, preset snapshots, or live inputs.
2. **DNA Analyzer**: Converts incoming planar audio streams into real-time or block-level `SoundDNA` vectors.
3. **DNA Transfer**: Takes donor `SoundDNA` and carrier `SoundDNA` and applies domain-specific characteristic transfer.
4. **DNA Morph**: Performs multidimensional interpolation (Slerp / geodesic) between two or more DNA state vectors.
5. **DNA Breed**: Combines characteristics of two parent DNA strands using domain-specific crossover and harmonic masks.
6. **DNA Mutate**: Applies controlled, non-destructive chaotic or stochastic perturbations to specified DNA dimensions.
7. **DNA Mask**: Filters or protects specific DNA fields (e.g., vocal protection, melody lock, key lock).
8. **DNA Router**: Directs specific DNA dimensions (rhythm, timbre, spatial) to different downstream processing chains.
9. **DNA Diff**: Computes the multidimensional semantic delta between two DNA states.
10. **DNA Snapshot**: Captures, holds, and recalls complete multidimensional DNA state vectors.
11. **DNA Compare**: Evaluates structural, harmonic, and rhythmic compatibility between two DNA streams.
12. **DNA Follow**: Tracks live incoming audio DNA changes over time and drives slave parameters.
13. **DNA Freeze**: Holds the active DNA state indefinitely regardless of underlying audio progression.
14. **DNA Recorder**: Captures performed DNA transformations into timeline automation lanes.
15. **DNA Compiler**: Translates abstract DNA targets into concrete DSP parameter commands and routing graphs.

---

## 5. DNA Scopes

Traditional audio scopes focus strictly on acoustic signal metrics:
* **Oscilloscope**: Raw waveform amplitude over time.
* **Spectrum Analyzer**: Frequency magnitude distribution (FFT).
* **Spectrogram**: Time-frequency energy heatmaps.
* **VU / PPM Meters**: Peak and RMS loudness.

**DNA Scopes** provide a dedicated visual observation layer for multidimensional musical characteristics over time:

```text
RHYTHM DENSITY
████████░░░░  [0.67]  (Groove: 16th Shuffle / Swing 58%)

SPECTRAL BRIGHTNESS
██████████░░  [0.82]  (Tilt: +3.2 dB/oct | Centroid: 3.4 kHz)

BASS MOVEMENT
█████░░░░░░░  [0.41]  (Syncopation: High | Envelope: Tight Decay)

HARMONIC TENSION
███░░░░░░░░░  [0.25]  (Key: D Minor | Dissonance Index: Low)

SPATIAL DECAY
███████░░░░░  [0.58]  (Stereo Width: 135% | ER Density: Mid)
```

### Key Visualization Features
* **Multidimensional Projections**: Visualizing high-dimensional DNA spaces down to 2D/3D manifold projections.
* **Temporal & Phrase-Level Evolution**: Tracking characteristic movement over 1-bar, 4-bar, and 16-bar phrase windows.
* **Region Comparison & Source Overlays**: Overlaying carrier DNA curves against reference DNA targets with confidence intervals.
* **Confidence & Artifact Visualization**: Rendering uncertainty bands where DNS analysis confidence is diminished (e.g. dense polyphonic mix regions).

---

## 6. DNA Diff

**DNA Diff** is a first-class analysis and transformation primitive. Given two musical entities $A$ and $B$, DNA Diff computes the multidimensional difference:

$$\Delta(A \to B) = B - A$$

Unlike scalar audio similarity scores (e.g., "78% similar"), a DNA Diff is a structured, multidimensional signal object preserving domain-specific deltas:

```text
rhythm       ++++++++    (+80% rhythmic density delta)
bass         +++++       (+50% sub-bass energy delta)
timbre       ++++        (+40% spectral brightness delta)
density      ++++++++    (+80% event density delta)
harmony      ++          (+20% harmonic complexity delta)
dynamics     +++++       (+50% dynamic range compression delta)
structure    +++++++     (+70% phrase variation delta)
```

### Diff-Driven Operations
Because the difference is itself a first-class DNA object, it enables algebraic, musically meaningful operations:

1. **Partial / Scaled Difference Application**:
   $$\text{Result} = A + \alpha \cdot (B - A) \quad (\text{where } 0.0 \le \alpha \le 1.0)$$
2. **Domain-Selective Delta Application**:
   $$\text{Result} = A + \text{rhythm}(B - A)$$
3. **Multi-Donor Composite Synthesis**:
   $$\text{Result} = A + \text{bass}(B - A) + \text{timbre}(C - A)$$

> **Domain Semantics Rule:** DNA algebraic operations use domain-specific manifold rules (e.g., geodesic rotation for spectral latent vectors, circular wrapping for phase/micro-timing) and must never assume linear vector addition is universally valid.

---

## 7. DNA Snapshots

A **DNA Snapshot** is a static or frozen musical-state object capturing a track's or scene's multidimensional characteristic profile:

```text
┌──────────────────────────────────────────────────────────┐
│ DNA SNAPSHOT: "Deep Tech Groove - Peak Hour"             │
├──────────────────────────────────────────────────────────┤
│ rhythm.swing_ratio       = 0.58                          │
│ rhythm.micro_timing      = [0, +2, -1, +4, ...] (12-slot)│
│ timbre.spectral_tilt     = -1.8 dB/oct                   │
│ timbre.octave_energies   = [0.82, 0.74, 0.61, ...]       │
│ dynamics.transient_peak  = +4.2 dB                       │
│ structure.phrase_length  = 16 Bars                       │
└──────────────────────────────────────────────────────────┘
```

### Snapshot Applications
* **Instant Recall**: Instantly set reference targets for live performance or studio mixing.
* **Scene Morphing**: Smoothly interpolate between Snapshot A and Snapshot B across a 32-bar breakdown.
* **Performance Presets**: Store characteristic "recipes" independently of specific audio sample files.
* **Versioning & Branching**: Maintain non-destructive historical iterations of track characteristics during composition.

---

## 8. DNA Microscope (Observation-First Workflow)

Nullherz is an **observation-first system**. It provides immediate analytical value without requiring any audio modification or transformation.

A musician or mixing engineer can use the **DNA Microscope** to ask:

> *"What actually makes these two recordings sound different?"*

```text
Track A: "Analog Master 1978"
Track B: "Digital Remaster 2024"

Microscope Inspection:
├── Temporal Alignment: Exact 0.0 ms offset
├── Rhythm Domain: 100% identity match
├── Timbre Domain: +4.8 dB shelf > 8 kHz in Track B; -2.1 dBdip @ 400 Hz
├── Dynamic Domain: Track B RMS is +3.4 dB higher; Crest factor reduced by 4.1 dB
└── Spatial Domain: Track A stereo correlation = 0.88; Track B correlation = 0.62
```

### Observation Tools
* **Where they differ**: Identifying specific temporal markers or transient locations where tracks diverge.
* **When they differ**: Inspecting phrase-level or section-level variations (e.g., Chorus vs. Verse divergence).
* **Which domains differ**: Isolating whether divergence is driven by rhythm, timbre, dynamics, or spatiality.
* **Confidence Rating**: Reporting DNS extraction confidence per domain to highlight where audio masking might distort analysis.

---

## 9. Hard Separation Between Analysis and Transformation

A foundational architectural boundary is maintained between analysis (measurement) and transformation (execution):

```text
             ┌─────────────┐
AUDIO ──────►│   ANALYSIS  │ (DNS)
             └──────┬──────┘
                    │
                   DNA
                    │
        ┌───────────┴───────────┐
        │                       │
      OBSERVE                TRANSFORM
        │                       │
        ▼                       ▼
      SCOPES                 DNA OPS
                                │
                                ▼
                            RENDERERS (DSP / Neural / MIDI)
                                │
                                ▼
                              AUDIO
```

### The Triad Taxonomy
1. **OBSERVABLE DNA**: What can be extracted and measured from audio signals by `AnalysisKernel`.
2. **CONTROLLABLE DNA**: What can be parameterised, mapped, and modulated by the user or control plane.
3. **RENDERABLE DNA**: What the current DSP, neural model, or MIDI backend can actually reproduce without acoustic artifacts.

```text
Example Matrix:
Characteristic      | Observable | Controllable | Renderable
--------------------+------------+--------------+------------
Micro-timing        | 0.98       | 0.95         | 0.96 (DSP)
Octave Spectral Tilt| 0.95       | 0.90         | 0.94 (DSP)
Vocal Formant Core  | 0.88       | 0.60         | 0.70 (Neural)
Polyphonic Melody   | 0.82       | 0.40         | 0.50 (MIDI/Synth)
"Polyphonic Mood"   | 0.30       | 0.10         | 0.15 (None)
```

*An acoustic field may be observable without being controllable; it may be controllable without a clean real-time renderer.*

---

## 10. DNA Instrument Rack

The system unifies these primitives into a cohesive **DNA Instrument Rack** vocabulary:

```text
┌────────────────────────────────────────────────────────────────────────┐
│ DNA INSTRUMENT RACK                                                    │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│  [ ANALYZE ]   [ TRANSFER ]   [ MORPH ]   [ BREED ]   [ MICROSCOPE ]   │
│                                                                        │
│  [ MUTATE ]    [ MASK ]       [ COMPARE ] [ SNAPSHOT ][ DIFF ]         │
│                                                                        │
│  [ ROUTE ]     [ FOLLOW ]     [ FREEZE ]  [ RECORD ]  [ SCOPE ]        │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

This interface vocabulary ensures the artist interacts with **musical tools, signals, and instruments**, rather than typing text prompts or managing conversational chatbots.

---

## 11. DNA as a Performance Signal

In live DJing and electronic performance, control interfaces map physical movements directly to audio parameters:

```text
Traditional DJ Mapping:
Crossfader Position  ──► Channel A / Channel B Audio Gain
Filter Knob          ──► Biquad Low-Pass Cutoff Frequency
Pitch Fader          ──► Playback Rate / Sample Resampling
```

**DNA performance mappings** elevate live control to direct musical characteristic manipulation:

```text
DNA Performance Mapping:
Crossfader Position   ──► DNA Transfer Amount (Track A Groove ──► Track B Groove)
Vinyl Platter Nudge   ──► DNA Micro-Timing Shift / Deformation
Filter Knob Position  ──► Timbre-Transfer Intensity & Spectral Tilt
Scratch Velocity      ──► Rhythmic Perturbation / Mutation Rate
Beat Phase Position   ──► DNA Interpolation / Morph Trajectory
Pad Pressure          ──► DNA Snapshot Recall / Transient Freeze
```

A DJ can crossfade the *groove and timbre* of Track B onto Track A two minutes before mixing the actual waveforms, producing seamless, progressive musical evolution.

---

## 12. DNA Performance Architecture

The overall system performance architecture is organized into clean functional tiers:

```text
                 ┌─────────────────┐
                 │    MUSICIAN     │
                 │   PERFORMANCE   │
                 └────────┬────────┘
                          │
               ┌──────────▼──────────┐
               │ DNA INSTRUMENTATION │ (Rack, Controls, Mappings)
               └──────────┬──────────┘
                          │
          ┌───────────────┼────────────────┐
          │               │                │
       ANALYSIS        MODULATION       TRANSFORM
       (Scopes,       (LFO, Beat Phase, (Transfer, Morph,
        Microscope)    Envelopes)        Breed, Mutate)
          │               │                │
          └───────────────┼────────────────┘
                          │
                    ┌─────▼─────┐
                    │ DNA GRAPH │ (Control Plane & Topology)
                    └─────┬─────┘
                          │
               ┌──────────┼──────────┐
               │          │          │
              DSP       NEURAL     MIDI (Execution Backends)
               │          │          │
               └──────────┼──────────┘
                          │
                        AUDIO
```

DSP processors, neural models, and MIDI engines act purely as **execution/rendering backends**, receiving control parameters compiled from the DNA Graph.

---

## 13. AI/Neural Boundary

### Product Principle
> **AI is machinery, not the primary interface.**

Neural network models (TCNs, SSMs, HyperNetworks, ONNX/NAM runtimes) provide essential technical capabilities behind the scenes:
* High-accuracy onset and feature extraction.
* Neural source separation and vocal isolation.
* Non-linear analog saturation and preamp modeling.
* Neural formant-preserving pitch/timbre synthesis.

### Explicit Anti-Patterns (What Nullherz Will NOT Do)
* **No Chat-First Workflows**: The main screen is not a text box asking "What kind of track do you want to make today?".
* **No Prompt-First Composition**: Music creation is driven by direct physical interaction, sequencing, routing, and signal manipulation.
* **No Autonomous Song Generation**: The software does not render complete songs from scratch without human direction.
* **No Conversational Agents in the DAW**: The primary interaction model remains tactile, visual, and signal-oriented.

*If natural language assistance is ever introduced, it will strictly serve as an optional accessibility or shortcut macro layer over the underlying DNA Instrument Rack.*

---

## 14. Product Philosophy

> **The project is not primarily trying to make AI generate music.**
>
> **It is trying to make musical characteristics themselves addressable, measurable, routable, modulatable, and transformable.**

The software is explicitly designed to feel like:
* A next-generation DAW
* A modular synthesizer environment
* A high-precision scientific signal analyzer
* A musical microscope
* A tactile live performance instrument
* An experimental composition playground

It is explicitly **not** a chatbot, a prompt generator, or an autonomous AI music service.

---

## 15. Core Architectural Thesis

```text
Audio
MIDI
DSP
Neural
DNA
```

These five elements form a complementary, unified stack. DNA acts as the intermediate representation and signal domain sitting at the nexus of musical analysis and control:

```text
                 MUSICAL CHARACTER
                         │
              ┌──────────┴──────────┐
              │                     │
           ANALYSIS              CONTROL
              │                     │
              └──────────┬──────────┘
                         │
                        DNA (Intermediate Representation)
                         │
          ┌──────────────┼──────────────┐
          │              │              │
         DSP           NEURAL         MIDI (Execution Tiers)
          │              │              │
          └──────────────┼──────────────┘
                         │
                       AUDIO
```

DNA is a concrete, mathematical, and musical abstraction—not a marketing metaphor and not an AI prompt shortcut.

---

## 16. Implementation Status & Roadmap Classification

To ensure clarity, system components are categorized into **Current Implemented Functionality** vs. **Future Conceptual Architecture**:

### Currently Implemented Functionality (State of the Tree)
* **DNS Extraction & Metadata**: Planar sample buffers, peak envelopes, 3-band `BandWaveform` pyramids, beat-grid offsets, and FFT spectra stored in `SampleMetadata` (`nullherz-traits`).
* **DNA Schema & Lineage**: `SoundDNA` struct with 16D latent vector, 8-band feature vector, 12-slot micro-timing array, signed ed25519 lineages in `redb` (`nullherz-dna`).
* **Basic Morphing & Breeding**: `NeuralTransfuser` (SIMD Slerp) and chaotic logistic-map breeding operators in `nullherz-dna`.
* **DSP Processing Inserts**: Real-time spectral morphing inserts (`DnaMorpher`, `PersonalityInheritanceProcessor`) in `nullherz-processors`.
* **Basic Groove Transfusion**: Sequencer micro-timing injection driven by `PerformanceCommand::SetDeckSync` / `SetDeckKeySync` in `nullherz-conductor`.

### Future Conceptual Architecture (Documented Direction)
* **DNA as a Routable Signal**: Multi-channel DNA signal routing in `ProcessorGraph` alongside Audio and MIDI buffers.
* **DNA Modulation Matrix**: Control-rate and beat-rate LFOs/Envelopes directly modulating DNA vector parameters.
* **DNA Timeline Automation**: Dedicated high-level DNA automation lanes in `nullherz-conductor` pattern managers.
* **DNA Modular Processing Primitives**: Native graph nodes for `DnaDiff`, `DnaMask`, `DnaSnapshot`, `DnaRouter`, and `DnaFreeze`.
* **DNA Scopes & Microscope View**: Dedicated egui visualization panels for multidimensional DNA comparison and difference heatmaps in `nullherz-inspector`.
* **DNA Performance Mappings**: Hardware MIDI CC mappings for crossfader DNA transfer, scratch deformation, and phase morphing.
