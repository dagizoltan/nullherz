# Architecture Specification: Sound Analysis / Perception System

**Document Version:** 1.0.0
**Status:** Architecture Specification
**Date:** March 2026
**Crates & Modules:** `audio-dsp`, `audio-core`, `nullherz-traits`, `nullherz-dna`, `nullherz-conductor`, `nullherz-processors`, `nullherz-inspector`

---

## Executive Summary

The **Nullherz Sound Analysis / Perception System** defines the architectural specification for real-time audio perception in the modular Rust DAW.

Rather than treating analysis as an isolated collection of visual meter widgets or third-party analyzer plugins, Nullherz establishes a **unified audio perception subsystem** that serves as a shared analytical foundation for the entire engine.

```text
AUDIO
  ↓
MEASUREMENT      (Raw, deterministic DSP signal processing)
  ↓
PERCEPTION       (Derived acoustic, musical, and timbre characteristics)
  ↓
CONTEXT          (High-level structural, trajectory, and DNA representations)
  ↓
UI / DSP / AI / CONTROL  (Shared Analysis Bus consumers)
```

By decoupling raw physical measurements from derived perceptual interpretations and high-level contextual awareness, the system enables real-time visual inspection, adaptive DSP modulation, neural model conditioning, and intelligent composition without duplicating analysis computation across subsystems.

---

## 1. Analysis Architecture

The analysis subsystem is structured into three strictly hierarchical analytical layers, maintaining clean boundaries between raw physical measurements, derived musical perceptions, and high-level context.

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                             CONTEXT LAYER                               │
│ Structural boundaries · Trajectories · DNA · Collision · Uncertainty     │
└────────────────────────────────────▲────────────────────────────────────┘
                                     │
┌────────────────────────────────────┴────────────────────────────────────┐
│                            PERCEPTION LAYER                             │
│ Rhythm · Pitch · Stems/Timbres · Brightness · Tension · Density         │
└────────────────────────────────────▲────────────────────────────────────┘
                                     │
┌────────────────────────────────────┴────────────────────────────────────┐
│                            MEASUREMENT LAYER                            │
│ FFT / STFT · Peak / RMS / LUFS · Centroid · ZCR · Phase · Envelopes     │
└────────────────────────────────────▲────────────────────────────────────┘
                                     │
                                   AUDIO
```

### 1.1 Measurement Layer
The Measurement Layer executes deterministic, mathematically exact DSP operations on raw audio sample frames. It produces non-abstract physical data without imposing musical or semantic assumptions:

* **Time-Frequency**: Fast Fourier Transform (FFT), Short-Time Fourier Transform (STFT), Linear / Logarithmic Magnitude Spectrum, Spectrogram frames.
* **Loudness & Dynamics**: Peak amplitude, True Peak (Oversampled), Root Mean Square (RMS), Integrated & Short-Term LUFS (EBU R128), Dynamic Range (DR), Crest Factor.
* **Spectral Shape**: Zero-Crossing Rate (ZCR), Spectral Centroid, Spectral Flux, Spectral Rolloff, Spectral Flatness / Tonality Index, Spectral Spread, Spectral Skewness.
* **Phase & Stereo**: Inter-channel Phase Correlation, Stereo Width / Side-to-Mid Ratio, Goniometer vector coordinates, Phase Coherence.
* **Envelopes & Time-Domain**: Fast / Slow Attack-Decay Envelopes, Hilbert Transform analytic signal envelopes, Sub-block Energy Derivatives.

### 1.2 Perception Layer
The Perception Layer ingests raw measurement streams and derives acoustic, timbre, and musical features. These features represent **derived interpretations** that carry confidence ratings rather than claiming absolute physical truth:

* **Stem & Timbre Classification**: Transient/Sustain decomposition, Kick, Snare, Clap, Hi-hats, Percussion, Vocals, Tonal vs. Noise ratio.
* **Harmonic & Pitch Content**: Fundamental Frequency ($f_0$), Harmonic Series distribution, Polyphonic Pitch Candidates, Pitch Class Profile (Chroma), Root/Key & Chord Candidates.
* **Rhythm & Timing**: Tempo (BPM), Beat Phase, Downbeat location, Pre-Beat / Pickup detection, Micro-timing offset, Swing ratio, Groove profile.
* **Perceptual Qualities**: Transient Density, Spectral Density, Perceptual Brightness, Perceptual Darkness, Sub-bass Weight, Perceptual Stereo Expansion, Audio Tension index.

### 1.3 Context Layer
The Context Layer evaluates perception streams across extended temporal windows (phrases, sections, full tracks) to derive high-level structural and relational information:

* **Structural Boundaries**: Intro, Build, Drop, Break, Outro, Verse, Chorus, Phrase transitions, Anomaly events.
* **Track DNA & Embeddings**: Compact 16D latent vectors (`SoundDNA`), 8-band spectral personality profiles, learned neural embeddings, multidimensional similarity graphs.
* **Cross-Track & Comparative Relationships**: Multi-track spectral collision maps, A/B differential profiles, perceptual trajectory curves, cross-deck mixability metrics.

All features in the Perception and Context layers explicitly preserve **confidence / uncertainty metrics** ($0.0 \le \mathcal{C} \le 1.0$) to indicate analysis reliability (e.g., polyphonic pitch extraction confidence in dense mix sections).

---

## 2. Multiple Analysis Timescales

Audio perception requires inspecting the signal across distinct temporal resolutions. A single window size cannot capture sub-millisecond transient attacks while simultaneously tracking 32-bar phrase structures.

```text
MICRO        ~1–10 ms          Attacks / Transients / Waveform Zero-Crossings
FAST         ~10–100 ms        Envelopes / Spectral Movement / Pitch Tracking
MUSICAL      ~100 ms–seconds   Beat / Rhythm / Energy / Key / Chords
STRUCTURAL   seconds–minutes   Phrases / Sections / Track DNA / Trajectories
```

### 2.1 Resolution Rationale
1. **Micro-Scale (~1–10 ms)**: High temporal resolution (e.g., 128–256 sample windows @ 48 kHz). Necessary for precise transient onset detection, phase alignment, true peak metering, and zero-crossing computation.
2. **Fast-Scale (~10–100 ms)**: Balanced time-frequency resolution (e.g., 1024–2048 sample STFT windows). Essential for tracking spectral centroid, envelope followers, pitch detection, and vocal formant movements.
3. **Musical Scale (~100 ms–seconds)**: High frequency resolution (e.g., 4096–8192 sample windows or multi-frame integration). Required for beat tracking, key identification, chord candidates, loudness integration (LUFS), and groove extraction.
4. **Structural Scale (seconds–minutes)**: Long-horizon temporal aggregation (e.g., 1–32 bar windows). Required for phrase boundary recognition, structural transition forecasting, energy progression, and track DNA fingerprinting.

### 2.2 Execution Rates
To prevent audio-thread overload, processing is strictly decoupled across three execution rates:

```text
AUDIO RATE (44.1 / 48 kHz)
    ↓  Sample-accurate windowing, peak detection, circular buffer writing
CONTROL RATE (~100 Hz / Sub-block)
    ↓  Continuous feature extraction (RMS, Centroid, Pitch, Envelopes)
EVENT RATE (Asynchronous / Discrete)
    ↓  Onsets, Beats, Downbeats, Phrase Boundaries, Structural Scene Events
```

---

## 3. Shared Analysis Bus

The **Analysis Bus** (`AnalysisBus`) is the central neural highway of the DAW. It collects analysis outputs from audio sources and makes them globally accessible to UI visualizations, DSP modulation, AI inference, and control systems.

```text
                                  ANALYSIS BUS
                                       │
       ┌───────────────────────────────┼───────────────────────────────┐
       ↓                               ↓                               ↓
      UI                              DSP                             AI
       │                               │                               │
 Visualizations                   Modulation                      Embeddings
 Analyzer / Scopes                Sidechain                       Classification
 Timeline Overlay                 Adaptive Routing                Generation / DNA
```

### 3.1 Core Architecture & Principles
* **Single Analysis Engine**: Eliminates redundant analysis calculations by replacing per-plugin analyzers with a shared, central perception hub.
* **Immutable Snapshots**: Consumers receive copy-on-write, atomic snapshots (`AnalysisBusSnapshot`) representing the state at a specific sample frame.
* **Streaming Telemetry**: High-frequency telemetry streams to the UI plane over lock-free ring buffers without blocking audio or DSP threads.
* **Clock & Sample Synchronization**: Every analysis frame carries precise audio clock timestamps (`sample_position`, `host_time_ns`, `beat_position`, `bar_index`).
* **Feature Provenance**: Analysis payloads track their source node ID, channel index, analysis window length, and feature extraction algorithm version.

### 3.2 Realtime Constraints & Thread Safety
* **Zero Allocations on RT Path**: Audio-rate analysis buffers operate on pre-allocated ring buffers and static memory arenas.
* **Lock-Free Communication**: Audio thread updates analysis ring buffers using single-producer multi-consumer (SPMC) atomic pointers.
* **Graceful Degradation**: Under heavy CPU load, control-rate and event-rate analysis frame rates drop automatically while preserving audio rendering integrity.

---

## 4. Analyzer UI & Central Spectral Field

The Analyzer UI is built around a unified, composable **Spectral Field**. Rather than scattering disjoint meters across the interface, it overlay-renders multiple analytical aspects onto a single visual field.

```text
┌────────────────────────────────────────────────────────────────────────┐
│ SPECTRAL FIELD                                                [120 BPM]│
├────────────────────────────────────────────────────────────────────────┤
│ 20kHz ┐                                              ░░▒▒▓▓██  [HIGH]  │
│       │               ▲            ▲                 ░░▒▒▓▓██          │
│ 1kHz  ┼───▓▓██▓▓─────/ \──────────/ \───────────────░░▒▒▓▓██   [MID]   │
│       │  ████████   /   \  ●     /   \   ●          ░░▒▒▓▓██           │
│ 20Hz  └──████████──┴─────┴─┼────┴─────┴──┼──────────░░▒▒▓▓██   [SUB]   │
│          01:12.0          01:12.5       01:13.0                        │
├────────────────────────────────────────────────────────────────────────┤
│ [RAW] [SPECTRAL] [RHYTHM] [HARMONIC] [TRANSIENT] [STEREO] [ENERGY]     │
│ [EVENTS] [DNA] [COLLISION] [EMBEDDING]                                 │
└────────────────────────────────────────────────────────────────────────┘
```

### 4.1 Composable Layer System
Users can toggle individual analytical layers on or off over the same visual coordinate space:

1. **`[RAW]`**: Oscilloscope time-domain waveform and envelope contours.
2. **`[SPECTRAL]`**: High-resolution FFT magnitude curve and colorized spectrogram background.
3. **`[RHYTHM]`**: Beat grid markers, subdivision ticks, swing offsets, and pre-beat indicators.
4. **`[HARMONIC]`**: Fundamental pitch tracks, harmonic series overlays, and key/chord annotations.
5. **`[TRANSIENT]`**: Onset markers, transient attack curves, and energy rise vectors.
6. **`[STEREO]`**: Mid/Side spectral balance, stereo width heatmaps, and correlation curves.
7. **`[ENERGY]`**: Integrated LUFS, dynamic range bounds, and crest factor indicators.
8. **`[EVENTS]`**: Classified event flags (kick, snare, vocal, fill, phrase change).
9. **`[DNA]`**: 16D latent space projection curves and spectral personality profiles.
10. **`[COLLISION]`**: Multi-track frequency overlap shading and phase cancellation hazard markers.
11. **`[EMBEDDING]`**: Sonic distance maps and vector similarity indicators against target reference tracks.

---

## 5. Musical Rhythm Analysis

The rhythm analysis module provides comprehensive, structural rhythm tracking going far beyond a static BPM readout.

```text
4 BARS (16 BEATS)

Bar 1                   Bar 2                   Bar 3                   Bar 4
1   .   2   .   3   .   4   .   1   .   2   .   3   .   4   .   1   .   2   .   3   .   4   .
●   ·   ●   ·   ●   ·   ●   ·   ●   ·   ●   ·   ●   ·   ●   ·   ●   ·   ●   ·   ●   ·   ●   ·
    ▲       ▲       ▲               ▲       ▲       ▲               ▲       ▲       ▲
   pre     pre     pre             pre     pre     pre             pre     pre     pre
[--- GROOVE: 16th Shuffle (58% Swing) ---] [--- TRANSIENT DENSITY: HIGH ---] [CONFIDENCE: 98%]
```

### 5.1 Rhythm Feature Set
* **BPM & Micro-BPM**: Multi-hypothesis tempo tracking (30–220 BPM) with sub-BPM precision ($0.01\text{ BPM}$).
* **Beat Phase**: Continuous phase accumulator ($0.0 \le \phi < 1.0$) tracking position within the active beat.
* **Downbeat & Bar Index**: Identification of Bar 1 beat 1 downbeats and bar counter tracking.
* **Phrase Boundaries**: Detection of 4-bar, 8-bar, 16-bar, and 32-bar structural phrase boundaries.
* **Pre-Beat / Pickup**: Detection of anticipatory transient attacks occurring immediately prior to downbeats.
* **Offbeat & Subdivision**: Tracking 8th, 16th, and triplet subdivision positions.
* **Groove & Swing**: Extraction of micro-timing offsets (`micro_timing: [i8; 12]`) and swing ratios ($0.50 \dots 0.75$).
* **Rhythmic Confidence**: Real-time evaluation of beat tracking stability and tempo certainty.

### 5.2 Beat-Grid Integration
Rhythm analysis outputs directly drive the DAW's primary beat-grid and beat-matching infrastructure:
$$\text{Sample Position} \xrightarrow{\text{Predictive Beat Tracker}} \text{Beat Phase } \phi(t) \xrightarrow{\text{Conductor}} \text{Deck Sync / Quantized Launch}$$

---

## 6. Transient & Event Analysis

The analysis system adopts a unified **Event Model** (`AnalysisEvent`). Audio events are detected, classified, and tagged with probabilistic confidence scores.

```text
                      AUDIO STREAM
                           │
                 Transient / Onset Detector
                           │
             ┌─────────────┴─────────────┐
             ▼                           ▼
     Spectral Feature            Temporal Feature
        Extraction                  Extraction
             │                           │
             └─────────────┬─────────────┘
                           ▼
                Classification Engine
                           │
                           ▼
                    AnalysisEvent
     ┌───────────────────────────────────────┐
     │ timestamp      : 01:32.441            │
     │ duration_ms    : 42.5 ms              │
     │ classification : KickDrum             │
     │ confidence     : 0.94                 │
     │ energy_db      : -1.2 dBFS            │
     │ freq_range     : 35 Hz - 180 Hz       │
     │ musical_pos    : Bar 12, Beat 3, +12ms │
     └───────────────────────────────────────┘
```

### 6.1 Event Taxonomy
* **Percussive Onsets**: Kick, Snare, Clap, Closed Hat, Open Hat, Tom, Percussion, Cymbal Crash.
* **Tonal / Harmonic Onsets**: Vocal onset, Bass note attack, Synth stab, Guitar pluck, Instrument onset.
* **Structural Events**: Beat, Downbeat, Pre-Beat Pickup, Drum Fill, Breakout, Drop, Phrase Boundary, Anomaly.

---

## 7. Spectral Collision Analysis

Collision analysis enables multi-track comparison to detect spectral masking, phase cancellation, and dynamic clutter across active mix decks or timeline tracks.

```text
TRACK A (Kick/Bass)  ──┐
                       ├──► SPECTRAL COLLISION ENGINE ──► OVERLAP MAP
TRACK B (Vocal/Keys) ──┘
```

```text
FREQUENCY BAND OVERLAP MAP
Band        Freq Range    Energy Overlap  Phase Risk   Actionable Insight
-------------------------------------------------------------------------------
SUB         20–60 Hz      88% [CRITICAL]  HIGH         High phase cancellation in sub
BASS        60–250 Hz     62% [MODERATE]  LOW          Masking kick transient
LOW-MID     250–500 Hz    35% [LOW]       NONE         Clear separation
MID         500–2 kHz     15% [CLEAR]     NONE         Vocal sitting clean
HIGH-MID    2–6 kHz       42% [MODERATE]  NONE         Key harmonics overlapping
HIGH        6–20 kHz      10% [CLEAR]     NONE         Air band open
```

### 7.1 Measurement Transparency
The system explicitly exposes raw physical measurements (overlap area, cross-correlation, phase cancellation indices) rather than reducing multi-track relationships to an unexplained "mixability score".

---

## 8. A/B and Before/After Differential Analysis

Differential analysis allows side-by-side comparison across any two signal points or presets to answer the fundamental question:

> **"What actually changed?"**

```text
INPUT (Dry Signal)   ──┐
                       ├──► DIFFERENTIAL ANALYSIS ──► "WHAT CHANGED?"
OUTPUT (Wet Signal)  ──┘
```

```text
DIFFERENTIAL COMPARISON: [Insert Before] vs [Insert After]
Dimension            Delta (Wet - Dry)         Insight
-------------------------------------------------------------------------------
Spectrum             +3.2 dB @ 8 kHz           High-shelf boost / Brightness added
Loudness (LUFS)      +1.8 LUFS                 Signal level increased
Dynamic Range        -2.4 dB                   Dynamic compression active
Stereo Width         +25% Expansion            Side content enhanced
Phase Correlation    0.98 ──► 0.82             Mild stereo decorrelation
Transient Density    -12%                      Transient softening (saturation)
Spectral Centroid    2.1 kHz ──► 3.4 kHz       Tonal balance shifted upward
Harmonic Distortion  +1.4% THD                 Odd-harmonic saturation added
```

### 8.1 Comparison Scenarios
* **Track A vs Track B**: DJ transition preview and mix compatibility.
* **Dry vs Wet / Pre vs Post Insert**: Inspecting plugin impact on dynamics and timbre.
* **Neural Insert In vs Out**: Evaluating neural saturation/filter coloration and latency.
* **Sample A vs Sample B**: Comparing drum sample transients and frequency content.

---

## 9. Perceptual Map

The **Perceptual Map** provides an experimental 2D/3D visual projection space where audio regions or full tracks are positioned according to psychoacoustic characteristics.

```text
                 BRIGHT (+Centroid / +Air)
                   ▲
                   │       ● Track C (Airy Vocal)
        harsh      │   ● Track B (Bright Lead)
                   │
 DENSE ────────────┼────────────── SPARSE (-Density / +DR)
 (+Flux / -DR)     │
       ● Track A   │
    (Heavy Industrial)
                   │       ● Track D (Deep Sub)
                 DARK (-Centroid / +Sub)
```

### 9.1 Perceptual Dimension Axes
* **Brightness / Darkness**: Derived from Spectral Centroid, Spectral Rolloff, and High-Band Energy.
* **Density / Sparseness**: Derived from Spectral Flux, Transient Density, Event Rate, and Dynamic Range.
* **Weight / Airiness**: Derived from Sub-bass Ratio vs. Air Band Energy ($>10\text{ kHz}$).
* **Harshness / Softness**: Derived from High-Mid Energy ($2.5\text{–}5\text{ kHz}$) and Spectral Flatness.
* **Harmonicity / Noise**: Derived from Harmonic-to-Noise Ratio (HNR) and Tonality Index.

---

## 10. Perceptual Trajectories

Tracks evolve continuously over time. A **Perceptual Trajectory** maps a track's temporal progression through perceptual space rather than relying on a single static summary fingerprint.

```text
Track A (Prog House):  [Intro] ●───► [Build] ●──────► [Drop] ●──────► [Outro] ●
Track B (Techno Peak): [Intro] ●───────► [Build] ●───► [Drop] ●──────► [Outro] ●
```

```text
PERCEPTUAL TRAJECTORY EVOLUTION (Track A)
Time       Section    Brightness    Density    Weight    Tension    Trajectory Vector
-------------------------------------------------------------------------------------
00:00.000  Intro      0.22 [Dark]   0.15       0.30      0.12       Stable
01:00.000  Build      0.55 [Rising] 0.68       0.10      0.78 [↑↑]  Upward Brightness
02:00.000  Drop       0.82 [Bright] 0.92 [Dense] 0.95      0.35 [↓]   Maximum Energy
04:00.000  Outro      0.30 [Dark]   0.20       0.25      0.10       Decay
```

### 10.1 Applications
* **DJ Transition Planning**: Finding sweet spots where Track A's decaying trajectory matches Track B's rising trajectory.
* **Structural Analysis**: Automatically identifying builds, drops, and breakdowns from trajectory slopes.
* **Track Discovery & Matching**: Searching for tracks that share similar dynamic energy arcs.

---

## 11. Track DNA

`SoundDNA` serves as the compact, 16-dimensional analytical fingerprint for every audio file in the library. It integrates physical measurements, derived perceptual features, and learned embeddings into a reusable metadata object.

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackDna {
    pub track_id: u64,
    pub bpm: f32,
    pub key_root: u8,               // 0 = C, 1 = C#, ... 11 = B
    pub key_mode: u8,               // 0 = Minor, 1 = Major
    pub time_signature: (u8, u8),   // (4, 4)
    pub integrated_lufs: f32,
    pub dynamic_range_db: f32,
    pub octave_energies: [f32; 8],  // Sub, Bass, Low-Mid, Mid, High-Mid, High, Air, Upper
    pub tonal_noise_ratio: f32,
    pub transient_density: f32,
    pub kick_density: f32,
    pub vocal_presence: f32,
    pub swing_ratio: f32,
    pub micro_timing: [i8; 12],
    pub stereo_width: f32,
    pub harmonicity: f32,
    pub latent_vector: [f32; 16],   // 16D latent manifold vector
}
```

### 11.1 Feature Classification
* **Measured Features**: Integrated LUFS, Octave Energies, Dynamic Range, Stereo Width.
* **Inferred Features**: BPM, Key, Time Signature, Micro-timing, Transient Density, Swing.
* **Learned Features**: 16D Latent Vector, Vocal Presence, Tonal/Noise Ratio.

---

## 12. Embeddings and Similarity

The system supports selecting an arbitrary audio region on the timeline or editor waveform and performing instantaneous similarity matching across the track library.

```text
SELECTED REGION: [Deck A - 01:42.318 - 01:50.000] (8-Bar Vocal Break)

SIMILAR MOMENTS IN LIBRARY
Track Name              Timestamp    Similarity  Match Basis
-------------------------------------------------------------------------------
Track_A_Deep.wav        00:37.102    94%         Vocal presence & Spectral tilt
Track_B_Tech.wav        02:14.551    91%         Harmonic progression & Key (D min)
Track_C_Groove.wav      01:08.221    87%         Rhythmic micro-timing & Swing
Track_D_Ambient.wav     03:45.000    82%         Reverb spatial decay & Air band
```

### 12.1 Similarity Sources & Explanation
Matches are explicitly decomposed into contributing similarity factors:
* **Spectral Match**: Distance between octave band energy vectors.
* **Rhythmic Match**: Correlation of transient onset patterns and micro-timing profiles.
* **Harmonic Match**: Pitch class profile (Chroma) cosine distance.
* **Learned Embedding Match**: Euclidean distance in 16D neural latent manifold space.

---

## 13. Analysis Snapshots & History

Users can capture persistent **Analysis Snapshots** (`AnalysisSnapshot`) at specific timestamps or project states and compare them over time.

```text
┌────────────────────────────────────────────────────────────────────────┐
│ ANALYSIS SNAPSHOTS                                                     │
├────────────────────────────────────────────────────────────────────────┤
│ Snapshot A: [01:12.400] Intro Breakdown   (LUFS: -18.2 | Centroid: 1.2kHz)│
│ Snapshot B: [01:42.400] Main Drop         (LUFS: -8.4  | Centroid: 3.8kHz)│
│ Snapshot C: [02:12.400] Vocal Outro       (LUFS: -14.1 | Centroid: 2.4kHz)│
└────────────────────────────────────────────────────────────────────────┘
```

### 13.1 Analytical Timeline Recording
Analysis data can be recorded continuously alongside audio rendering to create a time-indexed **Analytical Timeline**:

```text
AUDIO STREAM ──► ANALYSIS ENGINE ──► ANALYSIS RECORDER ──► ANALYTICAL TIMELINE
```

The recorded analytical timeline allows instant offline inspection, historical comparisons, and instant UI zooming without requiring re-computation of FFTs or feature extraction algorithms.

---

## 14. Selection-Based Analysis

Audio selections on the timeline or Audio Editor are treated as first-class analytical objects:

```text
SELECTION: [Deck A | 00:32.000 ──► 00:40.000]
  ├── Start Sample: 1,536,000
  ├── End Sample  : 1,920,000
  └── Duration    : 8.000 seconds (4 Bars @ 120 BPM)

AVAILABLE OPERATIONS
  ├── [ Analyze ]            ──► Open Spectral Field detail view
  ├── [ Compare ]            ──► Compare against reference selection
  ├── [ Embed ]              ──► Compute 16D neural latent embedding
  ├── [ Extract DNA ]        ──► Generate reusable SoundDNA snapshot
  ├── [ Find Similar ]       ──► Query library for matching audio regions
  ├── [ Create Cue ]         ──► Quantize and place hot-cue marker
  ├── [ Send to DSP ]        ──► Route selection to Granular/Sampler engine
  └── [ Send to Neural Insert]─► Feed selection into Neural TCN/SSM model
```

---

## 15. Spectral Archaeology / Event Inspection

Spectral Archaeology provides an exploratory inspection view allowing sound designers and research engineers to isolate an individual event and inspect its physical component composition.

```text
EVENT INSPECTION @ 01:32.441 (Snare Drum Hit)

TRANSIENT ATTACK (0.0 ms - 4.2 ms)
  ├── Peak Energy     : -0.4 dBFS
  ├── Spectral Spread : 20 Hz - 18 kHz (Broadband Noise)
  └── Zero Crossing   : 14.2 kHz equivalent

FUNDAMENTAL CORE (4.2 ms - 28.0 ms)
  ├── Fundamental $f_0$: 185 Hz (F#3)
  └── Harmonic Series : 370 Hz (2nd), 555 Hz (3rd), 740 Hz (4th)

BODY DECAY (28.0 ms - 120.0 ms)
  ├── Snare Wire Noise: Bandpassed 2.5 kHz - 8 kHz
  └── Exponential Decay: $\tau = 34.2\text{ ms}$
```

---

## 16. X-Ray Perceptual Decomposition

X-Ray Mode decomposes the perceptual contributions of a full mix across perceptual stems without asserting destructive or imperfect source separation.

```text
FULL MIX
  ├── VOCAL REGION     [300 Hz - 3.4 kHz | Formant Centroid: 1.8 kHz]
  ├── DRUMS / IMPACTS  [Transients > +6 dB Rise | Sub 40-90 Hz + High Noise]
  ├── BASS LINE        [Fundamental 40-180 Hz | High Harmonicity]
  ├── HARMONICS        [Mid/High Sustained Pitch Content]
  ├── TRANSIENTS       [Broadband High-Derivatives]
  └── NOISE / ATMOS    [Low Correlation | High Spectral Flatness]
```

### 16.1 Analytical Masks vs. Separation
X-Ray Mode distinguishes visual/analytical decomposition (applying time-frequency masks to highlight sound components on screen) from actual DSP stem isolation, avoiding artifact claims while giving producers visual clarity into mix components.

---

## 17. Change / Momentum Analysis

The system tracks first and second mathematical derivatives ($\frac{dx}{dt}, \frac{d^2x}{dt^2}$) of perceptual features to compute transparent **audio momentum**:

```text
CURRENT METRIC TRENDS & MOMENTUM
Energy         : ↑↑  (+4.2 dB/sec)  [RAPID RISING]
Density        : ↑   (+15%/sec)     [BUILDING]
Brightness     : ↑   (+320 Hz/sec)  [OPENING UP]
Stereo Width   : →   (0.0%/sec)     [STABLE]
Bass Energy    : ↑   (+2.1 dB/sec)  [ACCUMULATING]
Transients     : ↑↑  (+8 events/sec)[HIGH ACTIVITY]

PERCEPTUAL MOMENTUM INDEX: 84% [STRONG UPWARD BUILD]
```

---

## 18. Explainable Tension Analysis

Audio tension is derived transparently by aggregating weighted variations across measurable acoustic features:

$$\text{Tension} = w_1 \cdot \Delta\text{Energy} + w_2 \cdot \text{RhythmicDensity} + w_3 \cdot \text{HarmonicInstability} + w_4 \cdot \text{SpectralRise} + w_5 \cdot \text{SubdivisionDensity}$$

```text
AUDIO TENSION INDEX: 78%

CONTRIBUTING FACTORS
  ├── Spectral Rise (+3.2 octave shift)     : +24%
  ├── Rhythmic Subdivision Density (16ths)   : +19%
  ├── Harmonic Instability / Dissonance      : +17%
  ├── Transient Density Acceleration         : +12%
  └── Loudness Build (+3.1 LUFS)             : +6%
  ------------------------------------------------
  TOTAL EXPLAINED TENSION                    : 78%
```

---

## 19. Real-Time Anomaly Detection

The analysis engine monitors live audio streams in real time to catch operational hazards and audio defects:

```text
ANOMALY DETECTED @ 02:14.882 [Severity: HIGH]
Type        : Phase Collapse / Mono Cancellation
Description : Inter-channel correlation dropped to -0.82 in Sub/Bass band (30-120 Hz).
Action      : AnalysisBus emitted AnomalyEvent #402. DSP sidechain warned.
```

### 19.1 Anomaly Event Types
* **Clipping / Inversion**: Digital overshoot above $0.0\text{ dBFS}$ or floating-point non-finite values (NaN/Inf).
* **Spectral Spikes**: High-energy narrow-band resonances threatening speaker drivers.
* **Phase Collapse**: Phase inversion causing low-frequency cancellation when summed to mono.
* **Sudden Silence**: Unexpected signal dropouts ($<-60\text{ dBFS}$ drop in $<10\text{ ms}$).
* **Tempo Discontinuity**: Unexplained phase jump in transport beat alignment.

---

## 20. Analysis Graph

Analysis itself is modular and composable via the **Analysis Graph** (`AnalysisGraph`). Users and developers can construct custom analysis pipelines analogous to the audio DSP graph.

```text
ANALYSIS GRAPH TOPOLOGY
[ Audio Input ]
       │
       ├─► [ Low Frequency Isolator (20-120 Hz) ] ──► [ RMS Envelope ] ──► [ Threshold ] ──► (Sidechain Event)
       │
       └─► [ Spectral Centroid Node ] ──────────────► [ Smooth (100ms) ] ──► [ Normalizer ] ──► (Control Output)
```

### 20.1 Analysis Node Types
`FftNode`, `StftNode`, `TransientDetectorNode`, `PitchDetectorNode`, `RhythmDetectorNode`, `HarmonicAnalyzerNode`, `EmbeddingNode`, `ClassifierNode`, `SmoothingNode`, `ThresholdNode`.

---

## 21. Analysis → Control Modulation

Analysis outputs can be converted directly into control-rate or audio-rate signals to modulate DSP inserts, filters, and neural models.

```text
SPECTRAL CENTROID ──► NORMALIZER ──► CONTROL SIGNAL ──► FILTER CUTOFF
TRANSIENT ENERGY  ──► ENVELOPE   ──► CONTROL SIGNAL ──► NEURAL SATURATION DRIVE
```

### 21.1 Control Signal Categories
* **Audio-Rate Control**: High-frequency envelopes fed into sidechain saturation or wave-folding inputs.
* **Control-Rate Modulation (~100 Hz)**: Smooth parameter modulation (`MixerCommand::SetParam`) driving EQ filters, delays, and spatial expansions.
* **Event-Driven Triggers**: Discrete events (beats, onsets, drops) triggering snapshot swaps or neural model re-conditioning.

---

## 22. Research & Developer Mode

For engine developers and DSP researchers, the system provides a dedicated low-level inspection mode exposing raw engine internals and performance benchmarks.

```text
┌────────────────────────────────────────────────────────────────────────┐
│ RESEARCH / DEVELOPER ANALYSIS MODE                                     │
├────────────────────────────────────────────────────────────────────────┤
│ FFT Size: 2048 | Hop Size: 512 | Window: Blackman-Harris (7-term)      │
│ Analysis Latency: 10.66 ms (512 samples @ 48 kHz)                      │
│ CPU Cost: 0.12 ms / block (2.25% of 5.33 ms block budget)              │
│ Memory Behavior: 0 bytes allocated on RT path (RingBuffer capacity: 64)│
│ Async ML Inference: TCN Model execution time: 1.42 ms (Worker Thread)  │
│ Active FFT Bins: 1024 bins | Nyquist: 24,000 Hz                        │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 23. Realtime Constraints & Safety Invariants

To maintain absolute system stability, analysis execution must never compromise real-time audio thread safety.

1. **Zero Allocations on Audio Thread**: No `Vec::push`, no string formatting, no heap allocations inside audio callbacks.
2. **Lock-Free Communication**: Inter-thread data transfers use single-producer lock-free ring buffers (`ShmRingBuffer` / `spsc`).
3. **Async ML Inference**: Neural network models (ONNX, TCN, SSM) execute strictly on background worker threads (`NeuralWorkerBridge`). If a neural inference deadline is missed, the engine non-blockingly retains or extrapolates control values without audio dropouts.
4. **Bounded Execution**: All analysis algorithms running on audio or control paths have bounded $\mathcal{O}(1)$ time complexity per sample block.

---

## 24. Core Rust Data Model

The analysis architecture is represented by composable, data-oriented Rust structures in `nullherz-traits` and `audio-dsp`:

```rust
pub struct MeasurementFrame {
    pub sample_position: u64,
    pub host_time_ns: u64,
    pub peak_amplitude: [f32; 2],
    pub true_peak_db: [f32; 2],
    pub rms_db: [f32; 2],
    pub short_term_lufs: f32,
    pub zero_crossing_rate: f32,
    pub spectral_centroid_hz: f32,
    pub spectral_flux: f32,
    pub spectral_rolloff_hz: f32,
    pub spectral_flatness: f32,
    pub phase_correlation: f32,
    pub stereo_width: f32,
}

pub struct PerceptualFeatures {
    pub confidence: f32,
    pub pitch_candidate_hz: Option<f32>,
    pub fundamental_confidence: f32,
    pub chord_candidate: Option<u8>,
    pub bpm: f32,
    pub beat_phase: f32,
    pub is_downbeat: bool,
    pub is_prebeat: bool,
    pub transient_density: f32,
    pub spectral_density: f32,
    pub perceptual_brightness: f32,
    pub perceptual_darkness: f32,
    pub audio_tension: f32,
}

pub struct AnalysisEvent {
    pub event_id: u64,
    pub timestamp_samples: u64,
    pub duration_samples: u32,
    pub classification: EventKind,
    pub confidence: f32,
    pub energy_db: f32,
    pub frequency_range_hz: (f32, f32),
    pub musical_position: (u32, u32, f32), // (Bar, Beat, Sub-beat offset)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Kick,
    Snare,
    Clap,
    Hat,
    Percussion,
    VocalOnset,
    InstrumentOnset,
    Beat,
    Downbeat,
    Fill,
    PhraseBoundary,
    StructuralTransition,
    Anomaly,
}
```

---

## 25. Relationship with Existing DAW Architecture

The Sound Analysis / Perception System interfaces directly with every core component of the Nullherz engine:

* **Four DJ Decks & System Mixer**: Analysis nodes tap deck pre-fader and post-fader signals for real-time metering, needle waveform rendering, and cue bus monitoring.
* **DSP Graph & Modular Inserts**: Modulators consume `AnalysisBus` control signals to adjust parameters in real time (e.g., dynamic EQ, sidechain ducking).
* **Neural Inserts & Sidecars**: `NeuralWorkerBridge` feeds background ML inference workers with synchronized analysis frames.
* **Beat-Grid & Key Sync**: Analysis outputs feed `BeatGridInferenceEngine` and `KeySyncProcessor` for automatic tempo and pitch alignment.
* **Track Library & `SoundDNA`**: Background analysis workers extract track-level DNA fingerprints and persist them into `library.redb`.
* **UI Visualization**: `nullherz-inspector` consumes `AnalysisBusSnapshot` streams to render the Spectral Field, Vectorscope, and DNA Scopes.

---

## 26. UI Philosophy

1. **Information-Dense, Non-Cluttered**: Standardizing layout grids and composable layers so rich analytical data remains clear.
2. **Visually Meaningful over Decorative**: Every graphic element maps directly to physical or perceptual audio measurements.
3. **Raw Measurements Remain Accessible**: Users can always drill down from derived perceptual representations to raw FFT bins and time-domain samples.
4. **Visible Confidence**: Uncertain analysis (e.g. low pitch confidence in polyphonic noise) is rendered with subtle uncertainty bands rather than misleading hard values.
5. **Seamless Multiscale Zoom**: Smooth visual zooming from millisecond-level transient attacks out to 32-bar phrase overview maps.

---

## 27. Future Extensibility & Roadmap

```text
CURRENT IMPLEMENTATION
  ├── Real-Time FFT / STFT Spectrograms & Peak/RMS/LUFS Metering
  ├── Transient / Onset Detection & Peak Waveform Pyramids
  ├── Multi-Hypothesis Beat Grid & Predictive Beat Tracking
  └── 16D Latent SoundDNA & Redb Library Storage

PLANNED EXTENSIONS
  ├── Multi-Track Spectral Collision Overlays & Phase Hazard Map
  ├── X-Ray Perceptual Stem Decomposition & Visual Masking
  ├── Modular AnalysisGraph Engine & Custom Analysis Nodes
  ├── Analysis-to-Control Signal Routing & Sidechain Modulation
  └── Real-Time Anomaly Detection & Anomaly Event Stream
```

---

*“Perception is not the raw signal itself, but the structured understanding derived from observing it over time.”*
