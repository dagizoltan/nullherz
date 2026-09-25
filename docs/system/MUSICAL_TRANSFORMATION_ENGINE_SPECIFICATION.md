# Comprehensive Specification: Unified DNS/DNA Musical Transformation Engine

**Document Version:** 2.0.0
**Status:** Architecture Specification & System Benchmark
**Date:** March 2026
**Crates & Modules:** `nullherz-traits`, `nullherz-dna`, `nullherz-processors`, `nullherz-conductor`, `audio-dsp`

---

## Executive Summary

The **Nullherz Musical Transformation Engine** defines a unified, multi-layered architecture that treats **audio waveforms**, **acoustic observations (DNS)**, **musical representations (DNA)**, **DSP processors**, and **neural inference models** as composable, non-collapsing primitives.

Crucially, **traditional DJ waveform mixing remains a first-class, non-negotiable primitive**. DNA transformation does not replace conventional crossfading, channel gain control, or EQing; rather, it provides a parallel **musical transformation layer** ("Photoshop for Music") allowing tracks to borrow rhythm, groove, timbre, bass character, and atmosphere from reference tracks without requiring waveform mixing.

Furthermore, as detailed in the [DNA Instrumentation & Tooling Specification](./DNA_INSTRUMENTATION_AND_TOOLING.md), **DNA is a musical instrumentation domain, not an AI conversation interface**. The system operates as a tactile DAW, modular synthesizer, and signal-processing instrument rather than a text-driven or chatbot prompt generator.

---

## 1. Primary Operational Modes

Nullherz supports three distinct, fully composable operating modes across its DJ and Composer runtimes:

```text
                                 ┌─────────────────────────────────┐
                                 │       DECK A / DECK B INPUTS    │
                                 └────────────────┬────────────────┘
                                                  │
                 ┌────────────────────────────────┼────────────────────────────────┐
                 │                                │                                │
                 ▼                                ▼                                ▼
  ┌─────────────────────────────┐  ┌─────────────────────────────┐  ┌─────────────────────────────┐
  │  MODE 1: CONVENTIONAL MIX   │  │  MODE 2: CHARACTERISTIC     │  │    MODE 3: HYBRID MIX +     │
  │     (Waveform Crossfade)    │  │       TRANSFER (Audio-Less) │  │      DNA TRANSFORMATION     │
  ├─────────────────────────────┤  ├─────────────────────────────┤  ├─────────────────────────────┤
  │ Gain A / Gain B             │  │ Audio: Track A (Carrier)    │  │ Audio: Mix(A, B) Crossfade  │
  │ Equal-Power Crossfader      │  │ Reference: Track B DNA      │  │ DNA: Gradual A → B          │
  │ 3-Band Isolator EQ          │  │ Transform: Rhythm, Timbre,  │  │      Transformation         │
  │ Stem / Channel Summing      │  │            Bass, Transients │  │ Output: Hybrid Evolution    │
  └──────────────┬──────────────┘  └──────────────┬──────────────┘  └──────────────┬──────────────┘
                 │                                │                                │
                 └────────────────────────────────┼────────────────────────────────┘
                                                  │
                                                  ▼
                                       ┌─────────────────────┐
                                       │   MASTER AUDIO OUT  │
                                       └─────────────────────┘
```

### Mode 1 — Conventional Waveform Mixing (First-Class Primitive)
* **Equation**:
  $$\text{Output}[t] = A_{\text{audio}}[t] \cdot g_A(t) + B_{\text{audio}}[t] \cdot g_B(t)$$
* **Mechanisms**: Linear and equal-power crossfader curves, DJ channel gain faders, 3-band Isolator EQs (`HI`, `MID`, `LOW`), frequency-dependent crossovers, and stem summing.
* **Overhead**: Zero DNA overhead; pure real-time planar audio frame arithmetic.

### Mode 2 — Audio-Less Characteristic Transfer ("Photoshop for Music")
* **Concept**: Track A provides 100% of the audible waveform foundation. Track B is **never** mixed into the signal path; its `SoundDNA` acts purely as a musical control reference.
* **Level 1 DSP Execution**:
  * **Groove & Rhythm**: Aligns Track A's transient slices to Track B's 12-slot `micro_timing` profile and `swing_ratio`.
  * **Spectral Timbre**: Matches Track A's 8-band octave energy distribution to Track B's `feature_vector` via multiband shelving filters.
  * **Formants**: Reshapes Track A's resonant peaks using Track B's 5 `formant_peaks` $(f, Q, g)$.

### Mode 3 — Hybrid Mixing + DNA Transformation (Unified DJ Transition)
* **Concept**: Simultaneously crossfading audio waveforms while driving a parallel DNA transformation trajectory.
* **Example DJ Transition Timeline**:
  ```text
  Phase 1 (Bars 1–8):   100% Track A audio |   0% Track B audio | 20% Track B groove acquired
  Phase 2 (Bars 9–16):   75% Track A audio |  25% Track B audio | 60% Track B rhythm & timbre acquired
  Phase 3 (Bars 17–24):  25% Track A audio |  75% Track B audio | 90% Track B DNA convergence
  Phase 4 (Bars 25–32):   0% Track A audio | 100% Track B audio | 100% Track B DNA complete
  ```

---

## 2. Core Closed-Loop System Architecture

The ecosystem establishes a closed feedback loop where observation (DNS), representation (DNA), compilation, execution, and verification interlock cleanly:

```text
                    ┌─────────────────────┐
                    │   Musical Intent    │
                    └──────────┬──────────┘
                               ↓
                    ┌─────────────────────┐
                    │ DNA / Target State  │
                    └──────────┬──────────┘
                               ↓
              ┌────────────────┴────────────────┐
              ↓                                 ↓
       DNA Transformation                DNA Compiler
       breed/mutate/morph/etc.                 ↓
              ↓                         DSP / Neural Graph
              │                                 ↓
              └────────────────┬────────────────┘
                               ↓
                             AUDIO
                               ↓
                    ┌─────────────────────┐
                    │     DNS Analysis    │
                    └──────────┬──────────┘
                               ↓
                    Constraint Verification
                               │
                               └──── feedback ───→ DNA
```

1. **DNS (Observation / Measurement)**: Observed physical acoustics stored in `SampleMetadata` (planar buffers, peak envelopes, transients, frequency-colored `BandWaveform` pyramids, beat grid offset, Nyquist-bounded FFT spectrums).
2. **DNA (Multidimensional Intermediate Representation)**: Structured musical characteristics in `SoundDNA` (`SpectralPersonality`, `RhythmicDNA`, `ArtifactProfile`, `SpatialDNA`, 8-band `feature_vector`).
3. **DNA Target / Gravity Fields**: Desired destination state pulling current DNA across multidimensional attraction vectors.
4. **DNA Compiler**: Translates high-level musical intent ("Borrow B's groove while preserving A's melody") into precise DSP parameter configurations and graph routing choices.
5. **DSP / Neural Execution Machinery**: Real-time signal processors (`DnaMorpher`, `DnaAwareDistortion`, `DnaAwareDelay`, `NeuralTcnProcessor`).
6. **Audio Output**: Rendered planar audio frame output.
7. **Adversarial DNS Verification**: Closed-loop re-analysis via `AnalysisKernel` to verify beat alignment, phase stability, loudness bounds, and vocal identity before committing changes.

---

## 3. DNA Invariants (Musical Physics)

To prevent destructive transformations (e.g., mangling vocals or losing beat alignment during live DJing), transformations must pass an `InvariantMask` evaluation:

```rust
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct InvariantMask {
    pub preserve_melody: bool,     // Protect pitch contour and scale root
    pub preserve_vocal: bool,      // Protect formant frequency region (300 Hz - 3.4 kHz)
    pub preserve_tempo: bool,      // Lock BPM and transport grid
    pub preserve_key: bool,        // Lock root harmonic key
    pub mutable_rhythm: bool,      // Allow groove/micro-timing transformation
    pub mutable_timbre: bool,      // Allow octave band energy and tilt morphing
    pub mutable_spatial: bool,     // Allow stereo width and room size morphing
    pub loudness_tolerance_db: f32,// Maximum allowable RMS shift (e.g., ±1.5 dB)
}
```

If a candidate transformation violates an invariant during closed-loop DNS verification, the engine triggers an automatic **repair / adjust / reject** sequence:
$$\text{Target DNA} \xrightarrow[\text{Render}]{\text{Transform}} \text{Audio Out} \xrightarrow{\text{AnalysisKernel}} \text{Actual DNS} \xrightarrow{\text{Constraint Check}} \begin{cases} \text{Pass} \rightarrow \text{Commit} \\ \text{Fail} \rightarrow \text{Attenuate / Repair} \end{cases}$$

---

## 4. Multi-Rate Temporal Execution Architecture

To guarantee strict real-time safety on the audio execution path, Nullherz separates processing across 5 temporal execution rates:

```text
 ── Audio Callback Rate (Sample-rate / Block-rate, e.g. 48 kHz / 128 samples)
    └─ Zero allocations, zero locks, zero string operations.
    └─ Sample processing, biquad filtering, crossfading, DnaSideband inspection.

 ── Control Rate (~100 Hz, sub-block)
    └─ Parameter ramps (`MixerCommand::SetParam`), Slerp interpolation, LFOs.

 ── Rhythmic / Beat Rate (1–4 Hz, aligned to transport)
    └─ Quantized cue triggers, loop slicing, transient mask evaluations.

 ── Bar / Phrase Rate (~0.1–0.5 Hz)
    └─ Scene swaps, structural automation curves, invariant mask updates.

 ── Background Worker / Offline Thread
    └─ AnalysisKernel FFTs, closed-loop DNS verification, evolutionary track breeding, WAV disk encoding.
```

### Real-Time DNA Sideband (`DnaSideband`)
Passed inside `ProcessContext` during real-time audio callback execution:
```rust
#[derive(Debug, Clone, Copy)]
pub struct DnaSideband<'a> {
    pub source_dna: Option<&'a SoundDNA>,
    pub reference_dna: Option<&'a SoundDNA>,
    pub current_energy: f32,
    pub transient_density: f32,
}
```

---

## 5. Advanced Semantic Features

### DNA Gravity Fields
Represented as a multidimensional attraction tensor where multiple target tracks simultaneously pull different domains of the active DNA:
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GravityField {
    pub rhythm_target: Option<u64>,  // Track ID for rhythm attractor
    pub rhythm_strength: f32,
    pub timbre_target: Option<u64>,  // Track ID for timbre attractor
    pub timbre_strength: f32,
    pub bass_target: Option<u64>,    // Track ID for bass attractor
    pub bass_strength: f32,
    pub spatial_target: Option<u64>, // Track ID for spatial attractor
    pub spatial_strength: f32,
}
```

### DNA Deltas (`DnaDelta`)
Processors optionally report semantic changes to downstream inserts:
```rust
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, Default)]
pub struct DnaDelta {
    pub harmonic_density_delta: f32,
    pub spectral_brightness_delta: f32,
    pub transient_complexity_delta: f32,
    pub dynamic_range_delta: f32,
    pub spatial_decay_delta: f32,
}
```

### DNA Provenance / Musical Ancestry
Tracks maintain structured ancestry histories without audio-thread allocations:
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DnaProvenance {
    pub carrier_track_id: u64,
    pub donor_track_id: u64,
    pub transformation_spec: TransformationSpec,
    pub timestamp_unix_ms: u64,
    pub generator_version: String,
}
```

---

## 6. Three-Level Implementation Classification

| Capability | Classification | Execution Constraints |
| :--- | :--- | :--- |
| **Conventional Waveform Crossfading** | `LEVEL 1 — IMMEDIATELY PRACTICAL` | Real-time safe, 0% CPU overhead beyond sample math. |
| **Audio-Less Track Breeding (Multiband)** | `LEVEL 1 — IMMEDIATELY PRACTICAL` | Background worker thread, outputs persisted WAV. |
| **`DnaSideband` Pass-Through** | `LEVEL 1 — IMMEDIATELY PRACTICAL` | Real-time safe, zero heap allocations, zero locks. |
| **Multidimensional Matchmaking** | `LEVEL 1 — IMMEDIATELY PRACTICAL` | Synchronous or off-thread math on `SoundDNA`. |
| **DNA Invariants & Stencil Masks** | `LEVEL 1 — IMMEDIATELY PRACTICAL` | Control-rate boundary checks. |
| **Category B (DNA-Aware) Inserts** | `LEVEL 2 — ADVANCED ARCHITECTURE` | Real-time safe parameter modulation via `DnaSideband`. |
| **Category C (Transformation) Inserts** | `LEVEL 2 — ADVANCED ARCHITECTURE` | Real-time safe magnitude/band scaling. |
| **DNA Compiler & Conditioned Routing** | `LEVEL 2 — ADVANCED ARCHITECTURE` | Control-rate topology planning and graph swaps. |
| **Gravity Fields & Entropy Metrics** | `LEVEL 2 — ADVANCED ARCHITECTURE` | Control-rate tensor evaluations. |
| **Closed-Loop DNS Verification** | `LEVEL 3 — RESEARCH / EXPERIMENTAL` | Offline/precompute thread re-analysis via `AnalysisKernel`. |
| **Differentiable Neural Behaviour Transfer**| `LEVEL 3 — RESEARCH / EXPERIMENTAL` | Offloaded to background worker or SIMD neural sidecar. |

---

## 7. Audit & Capability Verification Matrix

1. **Current DNS capability**: Raw planar audio buffers, transient frame indices, peak envelopes, 3-band `BandWaveform` pyramids, beat grid offset, Nyquist-bounded FFT spectrums in `SampleMetadata`.
2. **Current DNA capability**: `SoundDNA` with 16D spectral latent space, 8 octave band feature vector, 256-bit onset mask, 12-slot micro-timing array, artifact profile, spatial early reflection gains/taps.
3. **Current Transfusion & Breeding**: SIMD Slerp on latent space (`NeuralTransfuser`), chaotic logistic map mutations, parent matchmaking sweet-spots ($0.4 \le \text{similarity} \le 0.7$).
4. **Current Inserts**: `DnaMorpher` (Type ID 228) and `PersonalityInheritanceProcessor` (Type ID 227) operating via FFT magnitude resynthesis.
5. **Genuinely Missing**: `DnaSideband` context passing, Category B/C DNA-aware inserts (`DnaAwareDistortion`, `DnaAwareDelay`, `DnaTransformationProcessor`), multidimensional `RegionalCompatibility` matchmaking, `TransformationSpec`, and audio-less multiband track breeding.
