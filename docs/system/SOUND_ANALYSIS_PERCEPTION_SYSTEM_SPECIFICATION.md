# Sound Analysis & Real-Time Audio Perception System Specification

**Prepared by:** Lead Audio DSP, Music Perception & DAW Architecture Team
**Document Version:** 1.0.0
**Status:** ARCHITECTURAL SPECIFICATION
**Target:** Shared Real-Time Perception Subsystem across Audio, Modulation, Neural & UI Planes
**Crates & Modules:** `audio-dsp`, `nullherz-processors` (`analysis.rs`), `nullherz-conductor` (`analysis_kernel.rs`, `analysis_worker.rs`), `nullherz-traits`, `nullherz-dna`, `nullherz-inspector`

---

## Executive Summary & Core Axiom

> **Core Axiom:** The Sound Analysis Subsystem is an integrated, real-time **audio perception engine**—a shared analytical foundation serving the entire DAW (DSP, Neural Inserts, Modulation, AI, UI, and Automation)—not merely a collection of isolated visualization widgets or meter plugins.

Traditional DAWs treat analyzers as static display plugins placed at the end of an FX chain, recalculating FFTs independently in every plugin instance. In Nullherz, sound analysis is elevated to a first-class shared perception substrate. It transforms continuous audio streams into structured, multi-timescale analytical data that flows across all execution planes.

```text
AUDIO STREAM
     ↓
MEASUREMENT LAYER  (Deterministic DSP: FFT, STFT, RMS, LUFS, Centroid, Flux)
     ↓
PERCEPTION LAYER   (Derived Musical Features: Kick/Snare, Pitch, Beat, Groove)
     ↓
CONTEXT LAYER      (Macro Structure: Intro/Drop, DNA, Trajectories, Similarity)
     ↓
ANALYSIS BUS
  ┌──┴────────────┬──────────────────┬─────────────────┐
  ↓               ↓                  ↓                 ↓
 UI              DSP                 AI             CONTROL
Visuals     Modulation &         Embeddings &     Automation &
Scopes       Sidechains          Generators      Trigger Events
```

---

## 1. Analysis Architecture

The analysis architecture strictly segregates raw mathematical observation from derived musical interpretation and macro-level structural context.

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. MEASUREMENT LAYER                                                        │
│    Deterministic, objective acoustic DSP measurements computed per block   │
│    (FFT, STFT, RMS, True Peak, LUFS, Crest Factor, Spectral Centroid, Flux) │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. PERCEPTION LAYER                                                         │
│    Derived musical & acoustic features inferred from measurements           │
│    (Transients, Pitch/Harmonics, Stem Types, Beat Phase, Swing, Brightness) │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 3. CONTEXT LAYER                                                            │
│    Macro-scale contextual intelligence carrying confidence bounds          │
│    (Structure/Drop, Track DNA, Embeddings, Collision, Perceptual Trajectory) │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.1 Measurement Layer
The **Measurement Layer** provides objective, deterministic mathematical measurements directly derived from time-domain or frequency-domain audio blocks without musical bias or subjective heuristics:

* **Spectral Transforms**: FFT (Fast Fourier Transform), STFT (Short-Time Fourier Transform) with sliding Hanning/Blackman-Harris windows, 1/3-octave / log-frequency binning.
* **Level & Dynamics**: RMS (Root Mean Square), Peak Amplitude, True Peak (4x oversampled ISP detection), LUFS (EBU R128 Short-Term / Integrated Loudness), Crest Factor, Dynamic Range index.
* **Spectral Shape Descriptors**:
  * **Spectral Centroid**: Center of mass of the spectrum ($\sum f \cdot |X(f)| / \sum |X(f)|$).
  * **Spectral Flux**: Difference in spectral magnitude between consecutive frames.
  * **Spectral Rolloff**: Frequency below which 85% or 95% of spectral energy resides.
  * **Spectral Flatness**: Ratio of geometric mean to arithmetic mean (tonality vs. noise metric).
  * **Zero-Crossing Rate (ZCR)**: Rate of sign changes per sample buffer.
* **Phase & Stereo Spatialization**: Stereo Phase Correlation ($\cos \phi$), Stereo Width, Mid/Side energy ratio, Vector Scope locus points.
* **Time Envelope & Transients**: Fast peak decay envelopes, short-term energy derivatives ($dE/dt$).

*Invariant:* Measurement layer outputs are deterministic facts. Given identical sample inputs, measurements yield identical outputs across all platforms.

### 1.2 Perception Layer
The **Perception Layer** converts deterministic measurements into derived musical features. These represent acoustic interpretations rather than raw truth and are updated across sub-block control and event rates:

* **Stem & Percussive Candidates**: Inferred transient and spectral signatures for Kick, Snare, Clap, Hi-Hats, Vocal Onsets, and General Percussion.
* **Tonal & Harmonic Extraction**: Fundamental Frequency ($f_0$), Pitch Candidates (chroma vectors), Harmonics/Overtones series, Chord Candidates.
* **Rhythmic & Temporal Structure**: BPM (Beats Per Minute), Beat Phase $[0.0, 1.0)$, Downbeat markers, Pre-Beat / Pickup detection, Groove offset vectors, Swing percentage.
* **Perceptual Qualities**: Brightness, Darkness, Dynamic Tension, Perceptual Energy, Stereo Behavior, Transient Density.

*Invariant:* Perception outputs are derived inferences. They carry explicit confidence ratings ($C \in [0.0, 1.0]$) to communicate extraction certainty.

### 1.3 Context Layer
The **Context Layer** operates across musical bars, phrases, and entire track timelines, aggregating lower-level measurements and features into contextual intelligence:

* **Structural Segmentation**: Automated boundary tagging for Intro, Build-up, Drop, Breakdown, Outro, and Phrase transitions.
* **Track-Level DNA**: Multidimensional semantic representations (`SoundDNA`) capturing rhythmic, harmonic, timbral, and spatial fingerprints.
* **Sonic Relationships**: Cross-track spectral collision maps, A/B difference vectors, sonic similarity rankings.
* **Embeddings & Trajectories**: Dense neural latent embeddings (e.g. 128D/256D vectors) tracking perceptual trajectories across track timelines.

---

## 2. Multiple Analysis Timescales

Audio perception spans microsecond physical attacks up to multi-minute musical arrangements. The system organizes analysis across four distinct temporal resolutions and three execution rates.

```text
TIMESCALE RESOLUTIONS:

MICRO (~1–10 ms)       ──────► Transient attacks, zero-crossings, phase align
FAST (~10–100 ms)      ──────► Envelopes, spectral movement, pitch tracking
MUSICAL (~100 ms–sec)  ──────► Beats, rhythm, chord progression, energy curves
STRUCTURAL (sec–min)   ──────► Song sections, phrases, track DNA, trajectories
```

### 2.1 Multi-Timescale Resolution Specifications

| Layer | Window Size | Hop Size | Primary Features Target |
| :--- | :--- | :--- | :--- |
| **MICRO** | 64–512 samples (~1–10 ms) | 32–128 samples | Transient attack slope, sub-sample peak alignment, phase coherence, zero-crossing rate. |
| **FAST** | 1024–4096 samples (~20–100 ms) | 256–512 samples | STFT spectrum, spectral flux, pitch ($f_0$) tracking, RMS envelope, formants. |
| **MUSICAL** | 4096–192000 samples (~100 ms–4 s) | 1024–4800 samples | Beat grid tracking, downbeats, groove micro-timing, harmonic chroma, chord candidates. |
| **STRUCTURAL**| 16–128 bars (~10 s–5 min) | 1 bar (~0.5–2 s) | Section detection (Drop/Break), 16D DNA vectors, perceptual trajectories, embeddings. |

### 2.2 Execution Rate Domains

The analysis architecture operates across three distinct processing execution rates:

```text
AUDIO RATE (44.1 / 48 / 96 kHz)
  ↓ Sample-accurate RMS, peak, zero-crossings
CONTROL RATE (~100 Hz / Sub-block)
  ↓ Continuous envelope followers, spectral centroid, LUFS
EVENT RATE (Asynchronous / Quantized)
  ↓ Onsets, beat triggers, section transitions, anomalies
```

1. **Audio Rate**: Executes inside real-time DSP kernels without allocation, producing block-aligned measurements (peak, zero-crossings, raw FFT accumulators).
2. **Control Rate**: Updates continuous parameters (envelopes, centroid, loudness) at ~100 Hz intervals (`SubBlockIterator`) for smooth modulation routing.
3. **Event Rate**: Emits discrete, structured events (Beat, Downbeat, Transient, Drop, Anomaly) when feature detectors exceed dynamic thresholds.

---

## 3. Shared Analysis Bus

The **Analysis Bus** (`AnalysisBus`) is the central communication substrate making analysis data concurrently available to UI views, DSP processors, Neural Workers, and Control/Modulation networks without duplicating FFT or feature extraction logic.

```text
                                 ANALYSIS BUS
                                      │
       ┌──────────────────────────────┼──────────────────────────────┐
       ↓                              ↓                              ↓
    UI PLANE                      DSP PLANE                      AI / CONTROL
────────────────             ──────────────────             ────────────────────
• Spectral Field             • Sidechain Ducking            • Neural Conditioning
• Transient Timeline         • Spectral Masking             • Parameter Automation
• Vector Scope               • Dynamic EQ Tracking          • Similar Region Search
• Metric Meters              • Modulation Matrix            • Auto-Cue Triggering
```

### 3.1 Bus Architecture & Constraints

* **Lock-Free Thread Safety**: Real-time audio threads push measurement frames into a single-producer, multi-consumer lock-free ring buffer (`ShmRingBuffer` or SPSC lock-free queues) or update atomic snapshot pointers (`ArcSwap`).
* **Timestamp Alignment**: Every packet on the Analysis Bus is tagged with:
  * `sample_position: u64` (absolute audio frame counter)
  * `musical_position: MusicalTime` (Bar, Beat, Sub-beat, Tick)
  * `timestamp_ns: u64` (system PTP disciplined clock time)
* **Confidence & Provenance**: Every perceived feature includes `confidence: f32` $[0.0, 1.0]$ and `source_id: NodeId` identifying the generating node or channel.
* **Deterministic Audio-Clock Sync**: UI renderers and off-thread neural workers interpolate analysis streams using sample-position matching against active transport telemetry.

---

## 4. Analyzer UI & Composable Spectral Field

The Analyzer UI is built around a unified **Spectral Field** canvas capable of stacking multiple composable visual analytical layers, replacing legacy isolated meter widgets.

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ SPECTRAL FIELD CANVAS                                                      │
│ [RAW] [SPECTRAL] [RHYTHM] [HARMONIC] [TRANSIENT] [STEREO] [ENERGY] [EVENTS] │
├─────────────────────────────────────────────────────────────────────────────┤
│ 0 dB ───────────────────────────────────────────────────────── (Peak: -0.2) │
│      ┌───┐            ▲                       ★ Drop                │
│-12dB │   │  ░░▒▒▓▓██  │  ♩ Beat 3.1           │                     │
│      │   │  ░░▒▒▓▓██ ─── Fundamental 130Hz    │  ████ High Density  │
│-24dB └───┘  ░░▒▒▓▓██  │  [C# min Chord]       │  ▒▒▒▒ Mid Energy    │
│            Spectrogram   Pitch/Harmonics      │  ░░░░ Low Energy    │
├─────────────────────────────────────────────────────────────────────────────┤
│ ◄◄  01:24.150  │  BAR 32.1  │  128.0 BPM  │  CONFIDENCE: 98%  ►►          │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 4.1 Composable Visual Layers

The user can toggle and blend any combination of analytical layers over the main time-frequency canvas:

* `[RAW]` — Waveform peak/envelope body with True Peak overshoots.
* `[SPECTRAL]` — High-resolution FFT magnitude curve and waterfall spectrogram heatmap.
* `[RHYTHM]` — Beat grid lines, subdivision ticks, pre-beat/pickup markers, and swing phase indicators.
* `[HARMONIC]` — Fundamental pitch track ($f_0$), overtone spectrum series, and detected chord candidate bands.
* `[TRANSIENT]` — Transient onset spikes categorized by attack sharpness and frequency band.
* `[STEREO]` — Frequency-dependent stereo width map, phase correlation indicator, and goniometer overlay.
* `[ENERGY]` — Integrated LUFS, Short-Term LUFS, Crest Factor, and dynamic range history.
* `[EVENTS]` — Structural section tags (Intro, Build, Drop), anomaly alerts, and user cue points.
* `[DNA]` — 16D DNA radar chart and characteristic fingerprint overlay.
* `[COLLISION]` — Cross-track spectral overlap heatmap highlighting frequency masking.
* `[EMBEDDING]` — Dimensionality-reduced 2D/3D perceptual manifold trajectory points.

---

## 5. Musical Rhythm Analysis

Rhythm analysis extends beyond static BPM counter displays, extracting detailed micro-timing, swing, and phrase structure to feed the DAW's beat-grid engine (`audio-dsp::rhythm`, `RealtimePredictiveBeatTracker`).

```text
4-BAR RHYTHMIC PHASE MAP

 BAR 1               BAR 2               BAR 3               BAR 4
 1   2   3   4       1   2   3   4       1   2   3   4       1   2   3   4
 │   │   │   │       │   │   │   │       │   │   │   │       │   │   │   │
 ●   ·   ●   ·       ●   ·   ●   ·       ●   ·   ●   ·       ●   ·   ●   ·  (Downbeats)
     ▲       ▲           ▲       ▲           ▲       ▲           ▲
    pre     pre         pre     pre         pre     pre         pre         (Pre-beat Pickups)
```

### 5.1 Rhythm Feature Set
* **Tempo & Phase**: Multi-hypothesis BPM tracking ($30$–$220$ BPM), continuous beat phase $\theta \in [0.0, 1.0)$, downbeat index ($1..4$).
* **Micro-Timing & Pickup**: Pre-beat onset estimation ($\Delta t$ before beat grid), offbeat syncopation, fill probability.
* **Groove & Swing**: Quantized swing percentage ($50\%$ un-shifted to $75\%$ heavy shuffle), 12-slot micro-timing offset array (`[i8; 12]`).
* **Confidence & Stability**: Dynamic tracking confidence score ($0..100\%$) indicating rhythm regularity.

---

## 6. Transient and Event Analysis

The system unifies all short-term acoustic occurrences into a standardized **Analysis Event** model.

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ ANALYSIS EVENT MODEL                                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│ • Event Type     : [Kick | Snare | Clap | Hat | Vocal | Drop | Anomaly]    │
│ • Timestamp      : Sample Frame 4,128,960 (01:26.020)                      │
│ • Musical Pos    : Bar 43, Beat 1, Tick 0                                   │
│ • Duration       : 42.5 ms                                                  │
│ • Confidence     : 0.94                                                     │
│ • Peak Energy    : +3.2 dBFS                                                │
│ • Freq Range     : 40 Hz – 120 Hz (Low Band)                                │
│ • Spectral Form  : Transient Attack Slope = 0.88, Decay = 12.4 ms           │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 7. Spectral Collision Analysis

Spectral collision analysis provides multi-source comparative intelligence to identify frequency masking and phase cancellation between concurrent tracks (e.g., Kick vs. Bass, Vocals vs. Guitars).

```text
TRACK A (Kick)   ──┐
                  ├─► SPECTRAL & TEMPORAL COMPARATOR ─► FREQUENCY OVERLAP MAP
TRACK B (Bass)   ──┘

BANDS:
[ SUB ]     ████████████████  92% OVERLAP  (CRITICAL MASKING)
[ BASS ]    ████████░░░░░░░░  48% OVERLAP  (MODERATE)
[ LOW-MID ] ██░░░░░░░░░░░░░░  12% OVERLAP  (CLEAR)
[ MID ]     ░░░░░░░░░░░░░░░░   0% OVERLAP
```

### 7.1 Multi-Band Overlap Metrics
* **Frequency Bands**: Sub (20–60 Hz), Bass (60–250 Hz), Low-Mid (250–500 Hz), Mid (500–2 kHz), High-Mid (2–6 kHz), High (6–20 kHz).
* **Comparison Dimensions**: Transient Overlap (simultaneous attacks), Harmonic Overlap (competing pitch peaks), Stereo Overlap (competing spatial positions).
* **Transparent Measurements**: Exposes exact dB masking curves and frequency crossover points rather than an opaque "mix score".

---

## 8. A/B and Before/After Analysis

The A/B analysis framework allows objective differential comparison between two audio streams (e.g. Dry vs. Wet, Track A vs. Track B, Insert Input vs. Insert Output, Neural Model Input vs. Output).

```text
INPUT (Dry)   ──┐
                ├─► DIFFERENTIAL ANALYZER ─► "WHAT CHANGED?"
OUTPUT (Wet)  ──┘

CHANGES DETECTED:
• Spectrum          : +3.2 dB @ 3.4 kHz (Air Shelf) | -1.5 dB @ 250 Hz
• Dynamic Range     : Reduced by 2.8 dB (Compression applied)
• Loudness          : +1.4 LUFS (Integrated)
• Stereo Width      : Expanded by +18%
• Transient Density : Preserved (Peak attack slope unchanged)
• Phase Coherence   : -0.04 (Negligible phase shift)
```

---

## 9. Perceptual Map

The **Perceptual Map** projects complex multidimensional audio features into intuitive 2D/3D spatial spaces representing human acoustic perception.

```text
                 BRIGHT
                   ▲
                   │
        harsh      │      airy
                   │
 DENSE ────────────┼────────────── SPARSE
                   │
       heavy       │      soft
                   │
                   ▼
                 DARK
```

### 9.1 Perceptual Dimension Derivations
* **Bright / Dark Axis**: Derived from Spectral Centroid, High-Frequency Energy Ratio, and Spectral Rolloff.
* **Dense / Sparse Axis**: Derived from Event Density, Spectral Flatness, and RMS/Crest Factor.
* **Heavy / Soft Axis**: Derived from Low-Frequency Energy Ratio, Sub-Bass Weight, and Dynamic Compression.
* **Harsh / Airy Axis**: Derived from Upper-Mid Spectral Spikes (2–5 kHz) vs. High-Frequency Air (>10 kHz).

---

## 10. Perceptual Trajectories

A track is represented not merely as a static point, but as a continuous **perceptual trajectory** moving through analytical space across time.

```text
Track A (Tech House):  ●───●──────●─────────●────●  (Build → Drop → Outro)
Track B (Ambient):     ●───────●───────●─────────●  (Smooth, Low Dynamics)
```

### 10.1 Applications
* **Transition Planning**: Identifying optimal DJ crossfade points where two tracks share perceptual space.
* **Structural Match**: Comparing track energy curves to maintain dancefloor momentum.
* **Generative & Search**: Finding tracks with matching trajectory profiles in the track library.

---

## 11. Track DNA

Track DNA provides a compact, 128-byte analytical fingerprint capturing a track's total acoustic and musical identity.

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ TRACK DNA FINGERPRINT                                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│ MEASURED FEATURES (Deterministic DSP):                                      │
│ • BPM: 128.00         • Key: F# Minor        • Time Sig: 4/4               │
│ • Dynamic Range: 8.4dB• LUFS: -9.2           • Crest Factor: 3.1           │
│ • Bands (Sub/Bass/LowMid/Mid/HighMid/High/Air): [0.8, 0.9, 0.6, 0.5, 0.7...]│
│                                                                             │
│ INFERRED FEATURES (Perception Layer):                                       │
│ • Tonal/Noise Ratio: 0.72   • Transient Density: High (12.4 / sec)          │
│ • Vocal Presence: 0.12      • Groove/Swing Index: 54% Shuffle             │
│ • Stereo Width: 124%        • Harmonicity Index: 0.81                     │
│                                                                             │
│ LEARNED EMBEDDINGS (Neural Context Layer):                                  │
│ • Latent Space Vector: [f32; 16] (16D Style & Character Embedding)          │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 12. Embeddings and Similarity

By selecting any audio region on the DAW timeline, the user can query the similarity engine to locate analytically or sonically matching moments across the entire track library.

```text
SELECTED REGION: "Bar 33 - Vocal Chop & Percussion Fill" (01:42.318)

SIMILAR MOMENTS FOUND:
┌──────────┬───────────┬────────────┬─────────────────────────────────────────┐
│ Track    │ Timestamp │ Match Score│ Primary Basis                           │
├──────────┼───────────┼────────────┼─────────────────────────────────────────┤
│ Track A  │ 00:37.102 │    94%     │ Spectral Timbre & Vocal Chop Transient  │
│ Track B  │ 02:14.551 │    91%     │ Groove Micro-timing & Bass Energy       │
│ Track C  │ 01:08.221 │    87%     │ Harmonic Scale & Rhythm Subdivision     │
└──────────┴───────────┴────────────┴─────────────────────────────────────────┘
```

---

## 13. Analysis Snapshots and History

The system supports persistent **Analysis Snapshots** allowing engineers to capture and compare analytical states across different mixing iterations or song sections.

```text
AUDIO STREAM ──► ANALYSIS KERNEL ──► ANALYSIS RECORDER ──► ANALYTICAL TIMELINE
                                                                  │
                                   ┌──────────────────────────────┼──────────────────────────────┐
                                   ↓                              ↓                              ↓
                              Snapshot A                     Snapshot B                     Snapshot C
                             (Verse 1)                      (Drop 1)                       (Bridge)
```

Snapshots store full measurement matrices, perception frames, and DNA fingerprints for offline comparison and recall without re-rendering audio.

---

## 14. Selection-Based Analysis

Audio selections on the timeline are treated as first-class analytical objects (`AnalysisSelection`).

```text
TIMELINE SELECTION: [Track 1: Sample Frame 102400 .. 204800]

AVAILABLE SELECTION OPERATIONS:
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│ [ Analyze ]      │  │ [ Compare ]      │  │ [ Embed ]        │
└──────────────────┘  └──────────────────┘  └──────────────────┘
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│ [ Extract DNA ]  │  │ [ Find Similar ] │  │ [ Create Cue ]   │
└──────────────────┘  └──────────────────┘  └──────────────────┘
┌──────────────────┐  ┌──────────────────┐
│ [ Send to DSP ]  │  │ [ Send to Neural]│
└──────────────────┘  └──────────────────┘
```

---

## 15. Spectral Archaeology & Event Inspection

For sound design and deep audio inspection, **Spectral Archaeology** allows selecting any individual event (e.g. a snare hit in a full mix) and decomposing its constituent spectral components.

```text
INSPECTED EVENT @ 01:32.441 (Snare Onset)

TRANSIENT DECOMPOSITION:
  ├── Fundamental        : 185 Hz (Body Resonance)
  ├── Harmonic Overtones : 370 Hz, 555 Hz, 740 Hz
  ├── Spectral Noise     : 2.4 kHz – 12 kHz (Snare Wire Noise)
  ├── Attack Envelope    : 2.1 ms (Sharp Initial Impact)
  └── Decay Envelope     : 145 ms (Exponential Decay)
```

---

## 16. X-Ray Mode

**X-Ray Mode** provides an analytical breakdown of a full mix's perceptual contributions without requiring destructive source separation.

```text
FULL MIX
  ├── VOCAL SPECTRUM      (Extracted Formant Energy Mask)
  ├── DRUM TRANSIENTS     (High-Flux Onset Mask)
  ├── BASS FOUNDATION     (Low-Pass <200 Hz Phase-Locked Core)
  ├── HARMONIC BODY       (Mid-Frequency Tonal Comb)
  └── NOISE & ATMOSPHERE  (Spectral Flatness Residual)
```

---

## 17. Momentum & Change Analysis

Instead of observing static values, **Momentum Analysis** measures first and second mathematical derivatives ($d/dt, d^2/dt^2$) of acoustic features to show energy movement and trends.

```text
FEATURE TREND INDICATORS:
• ENERGY       : ↑↑ (+4.2 dB/sec - Accelerating Build)
• DENSITY      : ↑  (+12% Event Rate Increase)
• BRIGHTNESS   : ↑  (+320 Hz/sec Centroid Rise)
• STEREO WIDTH : →  (Stable 110%)
• BASS WEIGHT  : ↑  (+2.1 dB Sub Rise)
• TRANSIENTS   : ↑↑ (Dense Onset Cluster)
```

---

## 18. Tension Analysis

**Tension Analysis** computes an explainable, derived index ($0..100\%$) representing musical and acoustic tension built during song arrangements.

```text
CURRENT TENSION: 78%

CONTRIBUTING FACTOR BREAKDOWN:
• Spectral Rise (Centroid uplift)      : +24%
• Rhythmic Subdivision Density         : +19%
• Harmonic Instability (Dissonance)    : +17%
• Transient Density Increase           : +12%
• Short-Term Loudness Rise             : +6%
─────────────────────────────────────────────
  TOTAL CALCULATED TENSION             : 78%
```

---

## 19. Anomaly Detection

Real-time anomaly detection monitors the audio stream for unexpected technical or acoustic irregularities, emitting event flags with high confidence.

```text
ANOMALY DETECTED @ 02:14.892

TYPE: STEREO PHASE COLLAPSE / Unexpected Silence
• Phase Correlation dropped to -0.82 (Sub-bass Out-of-Phase)
• Instantaneous Peak: +0.4 dBFS (Inter-Sample Clipping Alert)
• Confidence: 0.98
```

*Monitored Conditions:* Clipping, unexpected spectral spikes, phase inversion, stereo collapse, sudden silence, tempo discontinuity, unusual transients, background noise floor jumps.

---

## 20. Analysis Graph

The analysis engine itself is structured as a modular, composable graph (`AnalysisGraph`) where specialized analysis nodes pipe measurements into downstream feature detectors.

```text
┌─────────────┐     ┌─────────────────────┐     ┌───────────────────┐     ┌───────────────────┐
│ Audio Input │ ──► │ Low-Freq Bandpass   │ ──► │ Envelope Follower │ ──► │ Dynamic Threshold │
└─────────────┘     └─────────────────────┘     └───────────────────┘     └─────────┬─────────┘
                                                                                    │
                                                                                    ▼
                                                                          ┌───────────────────┐
                                                                          │ Sidechain Event   │
                                                                          └───────────────────┘
```

---

## 21. Analysis → Control Modulation

Analysis outputs can be normalized and routed directly into the DAW's modulation matrix as real-time control signals.

```text
ANALYSIS SOURCE: Spectral Centroid
  ↓ Normalized [0.0, 1.0]
CONTROL SIGNAL
  ↓ Mapped via Modulation Matrix
DESTINATION: Synth Filter Cutoff / Neural Insert Condition Vector
```

---

## 22. Research & Developer Mode

For DSP engineers and system developers, **Research Mode** exposes low-level analysis diagnostics and engine execution metrics:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│ RESEARCH / DEVELOPER ANALYZER DIAGNOSTICS                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│ • FFT Window    : Blackman-Harris 7-Term | Size: 2048 | Hop: 256            │
│ • Analysis Lat  : 5.33 ms (1 Audio Block @ 48kHz)                           │
│ • CPU Cost      : 14.2 µs per block (0.26% core capacity)                     │
│ • Memory Alloc  : 0 bytes (RT Safe, Static Buffers)                         │
│ • Inference Time: 1.2 ms (Neural Worker Thread, Async Ring Buffer)         │
│ • Active Nodes  : FFT, STFT, OnsetDetector, PitchYin, BeatTracker, DnaKernel│
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 23. Realtime Constraints & Thread Safety

To guarantee that audio execution is never stalled by complex perception algorithms, the analysis subsystem enforces strict isolation between execution planes.

```text
AUDIO THREAD (Execution Plane)
  │ Reads audio blocks, updates atomic ring buffer accumulators
  │ Zero allocations, bounded O(1) DSP work, NO mutexes
  ▼
SHM RING BUFFER / ATOMIC SNAPSHOT
  │ Lock-free, single-producer multi-consumer
  ▼
ANALYSIS WORKER THREADS (Orchestration Plane)
  │ Executes heavy FFTs, Pitch Tracking, ML Inference, DNA Extraction
  ▼
ANALYSIS BUS (UI / Modulation / AI Consumption)
```

---

## 24. Conceptual Rust Data Model

```rust
// Core composable analysis structs

#[repr(C, align(64))]
pub struct MeasurementBlock {
    pub sample_position: u64,
    pub rms_db: [f32; 2],
    pub peak_db: [f32; 2],
    pub true_peak_db: [f32; 2],
    pub spectral_centroid_hz: f32,
    pub spectral_flux: f32,
    pub spectral_flatness: f32,
    pub zero_crossing_rate: f32,
    pub phase_correlation: f32,
    pub stereo_width: f32,
    pub fft_bins: [f32; 1024],
}

pub struct PerceptionFrame {
    pub timestamp_ns: u64,
    pub musical_position: MusicalTime,
    pub bpm: f32,
    pub beat_phase: f32,
    pub downbeat: bool,
    pub pitch_candidate_hz: f32,
    pub pitch_confidence: f32,
    pub detected_stem: StemClassification,
    pub brightness: f32,
    pub perceptual_energy: f32,
}

pub struct AnalysisEvent {
    pub id: u64,
    pub timestamp_sample: u64,
    pub event_type: EventKind,
    pub confidence: f32,
    pub energy_db: f32,
    pub freq_range_hz: (f32, f32),
    pub duration_ms: f32,
}

pub struct TrackDnaSignature {
    pub bpm: f32,
    pub key_root: u8,
    pub key_mode: u8,
    pub dynamic_range_db: f32,
    pub band_energies: [f32; 8],
    pub micro_timing_offsets: [i8; 12],
    pub latent_embedding: [f32; 16],
}
```

---

## 25. Relationship with Existing DAW Architecture

The Analysis Subsystem integrates across all Nullherz DAW components:

* **Four Decks & Mixer**: Taps per-deck pre/post-fader audio streams for Deck VU, Key Matching, and Beat Alignment.
* **DSP Graph & Modular Inserts**: Provides sidechain control signals, envelope followers, and dynamic EQ triggers.
* **Neural Inserts**: Supplies conditioning vectors for HyperNetwork EQs and TCN model parameters.
* **Beat Grid & Key Sync**: Feeds `audio-dsp::rhythm` and `AnalysisKernel` for zero-latency phase-locking.
* **Track Library (`redb`)**: Persists `TrackDnaSignature` for instant similarity searches and smart crates.
* **Inspector UI**: Powers the composable Spectral Field, DJ Studio waveforms, and Mastering scopes.

---

## 26. UI Philosophy

1. **Information-Dense, Non-Cluttered**: High data density with clean visual hierarchy.
2. **Visually Meaningful over Decorative**: Every pixel represents real audio telemetry.
3. **Raw Measurements Remain Accessible**: Toggle from high-level perception back to raw FFT data anytime.
4. **Visible Confidence Bounds**: Faint visual halos represent lower analysis confidence.
5. **Seamless Multiscale Zoom**: Zoom smoothly from 1 ms transient detail to a 5-minute song arrangement.

---

## 27. Future Extensibility

* Automated polyphonic chord progression estimation.
* Acoustic room impulse response (RIR) profiling.
* Neural source separation stem extraction (Vocal, Drums, Bass, Other).
* Real-time mastering diagnostic warnings (EBU R128 compliance).
* Automatic cue point and loop candidate generation.
