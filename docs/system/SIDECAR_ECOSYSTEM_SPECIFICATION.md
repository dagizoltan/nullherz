# Unified Sidecar Ecosystem & Dual Signal-Chain Specification

## Executive Summary

The Nullherz Sidecar Ecosystem unifies all **Instruments**, **Audio Inserts**, **Visual Generators**, and **Visual Inserts** into a single modular sidecar primitive (`SidecarModule`). Rather than maintaining fragmented component abstractions, every sound source, audio effect, neural processor, and generative visual engine is built, registered, packaged, and distributed as a routable, hot-swappable **Sidecar**.

This specification formalizes the ecosystem architecture, the division between local assets (**Library**) and cloud marketplace (**Store**), and the dual signal-chain pipeline that separates real-time audio processing from visual surface generation.

---

## 1. Unified Sidecar Primitive (`SidecarType`)

Every sidecar in Nullherz declares a specific `SidecarType`:

| Category | Enum Variant | Target Pipeline | Description |
| :--- | :--- | :--- | :--- |
| **Audio Instrument** | `SidecarType::AudioInstrument` | Audio Engine (DSP) | Sound generators (Synths, Samplers, Physical Modeling, Neural Generators) |
| **Audio Insert** | `SidecarType::AudioInsert` | Audio Engine (DSP) | Audio processing FX (Analog Tape Saturation, Neural TCN/NAM Amps, Delays, Reverbs, EQs) |
| **Visual Generator** | `SidecarType::VisualGenerator` | Visual Compositor (Canvas) | Generative visual sources (3D NCA Morphogenesis, Particle Swarms, Fluid Dynamics) |
| **Visual Insert** | `SidecarType::VisualInsert` | Visual Compositor (Canvas) | Visual post-processing FX (Milkdrop Per-Pixel Warps, Chromatic Aberration, Bloom, Glitch) |

---

## 2. Dual Signal-Chain Architecture

To preserve strict audio-rate execution (<1.3 ms round-trip latency) while supporting frame-rate visual generation (60–120 FPS), Nullherz executes two parallel, decoupled signal chains:

```
                                 1. AUDIO PIPELINE
┌─────────────────────────┐     ┌─────────────────────────┐     ┌─────────────────────────┐
│   AUDIO INSTRUMENTS     │ ──> │      AUDIO INSERTS      │ ──> │     SYSTEM MIXER /      │
│  Synths, Samplers,      │     │ Analog Saturation,      │     │     AUDIO OUTPUT        │
│  Neural Tone Generators │     │ Delays, Reverbs, EQs    │     │   (Speakers / Lines)    │
└────────────┬────────────┘     └─────────────────────────┘     └─────────────────────────┘
             │
             │ Lock-Free Real-Time Telemetry (AnalysisBus: Spectral, RMS, Onset, Beat-Phase)
             ▼
                                 2. VISUAL PIPELINE
┌─────────────────────────┐     ┌─────────────────────────┐     ┌─────────────────────────┐
│    VISUAL GENERATORS    │ ──> │     VISUAL INSERTS      │ ──> │   VISUAL COMPOSITOR /   │
│  NCA Meshes, Shaders,   │     │ Per-Pixel Feedback Warps│     │     CANVAS OUTPUT       │
│  Particle Swarms, Rays  │     │ Chromatic Aberration,   │     │  (Main Screen/Detached) │
└─────────────────────────┘     │ Bloom, Color Grading    │     └─────────────────────────┘
                                └─────────────────────────┘
```

### Domain Guarantees
* **Audio Inserts** operate directly on audio sample buffers (`&[&[f32]]` at 48kHz) inside the zero-allocation audio engine thread.
* **Visual Inserts** operate on texture framebuffers / RGBA pixel buffers (`PixelFeedbackEngine`) driven by lock-free telemetry received over `AnalysisBus`.

---

## 3. Asset Taxonomy: Library vs. Store

```
                        ┌──────────────────────────────────────────────┐
                        │              STORE (Marketplace)             │
                        │   Browse, Buy, & Download Cloud Sidecars     │
                        └──────────────────────┬───────────────────────┘
                                               │ Download .sidecar bundle
                                               ▼
┌─────────────────────────────────────────────────────────────────────────────────────────────┐
│                                   LOCAL LIBRARY (Inventory)                                 │
├───────────────┬───────────────┬───────────────┬────────────────┬───────────────┬────────────┤
│    TRACKS     │    SAMPLES    │   SEQUENCES   │  INSTRUMENTS   │ AUDIO INSERTS │  VISUALS   │
│ Audio Tracks  │ Drum & Stem   │ MIDI Patterns │ Installed      │ Installed     │ Installed  │
│  + DNA IR     │   One-Shots   │ & DNA Motifs  │ Synth Sidecars │  FX Sidecars  │ Visuals    │
└───────────────┴───────────────┴───────────────┴────────────────┴───────────────┴────────────┘
```

### A. Local Library (`library/`)
Your local workspace inventory, organized into 6 tabs:
1. **Tracks**: Full audio tracks with extracted DNA signatures & peak metadata.
2. **Samples**: One-shots, drum samples, and stem slices with automatic tags.
3. **Sequences**: MIDI patterns, step sequences, and genetic blueprints.
4. **Instruments**: Installed `AudioInstrument` sidecars + patch presets.
5. **Audio Inserts**: Installed `AudioInsert` sidecars + custom rack preset chains.
6. **Visuals**: Installed `VisualGenerator` and `VisualInsert` sidecars + shader/NCA configs.

### B. Store (`View::Store`)
The cloud discovery marketplace where users can browse, purchase, and hot-install modules:
* **Filter Tabs**: `All`, `Audio Instruments`, `Audio Inserts`, `Visual Generators`, `Visual Inserts`.
* **Tag Search**: Search by tags (`#neural`, `#delay`, `#visual`, `#eq`, `#real-time`).
* **1-Click Hot Installation**: Bundles are verified, saved to `library/`, and instantly registered into the live `SidecarStore` without restarting the app.
