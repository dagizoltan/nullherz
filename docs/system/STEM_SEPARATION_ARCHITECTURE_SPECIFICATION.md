# Stem Separation & Multi-Tier Instrument Demixing System Specification

**Target:** Hierarchical Real-Time & Offline Stem Separation Architecture across Audio DSP, Neural Execution, DNA Schema, Conductor, and UI Planes
**Status:** Approved Architectural Standard

---

## 1. Executive Summary & Core Architectural Axiom

> **Core Axiom:** Stem separation in Nullherz is not a static 4-track audio splitter plugin, but a **multi-tier, hierarchical signal demixing engine** integrated directly into the `SoundDNA` intermediate representation, lock-free real-time DSP routing topology (`DeckStemMatrix`), and multi-donor genetic cross-breeding (`BreederView`).

Modern live DJ performance and sound transformation demand sample-accurate separation spanning basic 4-part performance stems up to granular 12-instrument demixing for DNA mutation, stem-level micro-timing extraction, and spectral re-synthesis.

---

## 2. Multi-Tier Hierarchical Stem Taxonomy

The Nullherz Stem Separation Architecture organizes isolated audio components into three distinct operational tiers:

### Tier 1: Performance DJ Mode (4 Stems)
Standard DJ performance mapping aligned with hardware controllers (rekordbox, Serato, Traktor):
1. **Drums** (`Kick`, `Snare`, `Hats`, `Percussion` combined)
2. **Bass** (Sub-bass, Synth Bass, Bass Guitar)
3. **Vocals** (Lead & Backing Vocals)
4. **Other** (Guitars, Keys, Synths, FX)

### Tier 2: Producer & Composition Mode (6 Stems)
Enhanced isolation for track remixing, arranger stem loading, and sampler slicing:
1. **Drums**
2. **Bass**
3. **Vocals**
4. **Guitar**
5. **Piano / Keys**
6. **Synths / Other**

### Tier 3: Master & DNA Transfusion Mode (8–12 Stems)
Micro-demixing for deep genetic SoundDNA extraction, per-instrument transient alignment, and multi-donor breeding:
- **Rhythmic & Percussive Group**:
  1. `Kick`
  2. `Snare / Clap`
  3. `Hi-Hats / Cymbals`
  4. `Percussion / Toms`
- **Tonal & Melodic Group**:
  5. `Lead Vocal`
  6. `Backing Vocal / Choir`
  7. `Bass`
  8. `Electric / Acoustic Guitar`
  9. `Piano / Keys`
  10. `Synths / Leads / Pads`
  11. `Strings / Brass`
  12. `FX / Textures / Atmospheric`

---

## 3. Two-Stage Cascading Neural Separation Pipeline

To eliminate phase cancellation artifacts and high-frequency smearing inherent in monolithic single-pass models, Nullherz employs a two-stage cascading neural demixing network:

```
                      ┌─────────────────────────────────────────┐
                      │            Full Mixed Track             │
                      └────────────────────┬────────────────────┘
                                           │
                                  ▼ Stage 1 Macro Model
                    ┌─────────────────────────────────────────────┐
                    │ Band-Split RNN / HTDemucs v4 Macro Model    │
                    └──────┬──────────┬──────────┬─────────┬──────┘
                           │          │          │         │
               ┌───────────┘          │          │         └──────────┐
               ▼                      ▼          ▼                    ▼
        ┌─────────────┐        ┌──────────┐  ┌────────┐        ┌─────────────┐
        │ Drums Stem  │        │ Bass Stem│  │ Vocals │        │ Other Stems │
        └──────┬──────┘        └──────────┘  └───┬────┘        └─────────────┘
               │                                 │
               ▼ Stage 2 Micro Demuxer           ▼ Stage 2 Micro Demuxer
      ┌─────────────────┐               ┌─────────────────┐
      │ Percussion      │               │ Vocal           │
      │ Demuxer         │               │ Demuxer         │
      └─┬───┬─────┬────┬┘               └────┬───────┬────┘
        │   │     │    │                     │       │
        ▼   ▼     ▼    ▼                     ▼       ▼
      Kick Snare Hat  Perc                 Lead     Backing
```

### Stage 1: Macro Demixing
- **Model**: HTDemucs v4 / Band-Split RNN (BSRNN).
- **Execution**: Off-thread background execution pool (`StemExtractionWorker`).
- **Output**: 6 main stem streams (`Drums`, `Bass`, `Vocals`, `Guitar`, `Keys`, `Other`).

### Stage 2: Micro Demixing
- **Percussion Demuxer**: Specialized lightweight TCN model operating on the `Drums` stem to extract `Kick`, `Snare`, `Hat`, and `Percussion`.
- **Vocal Demuxer**: Specialized U-Net model operating on the `Vocals` stem to extract `Lead Vocal` and `Backing Vocal`.

---

## 4. Zero-Copy On-Disk Storage & Data Schema

Stem audio files and metadata are stored in a structured zero-copy format under `library/stems/`:

```
library/stems/<track_id>/
├── metadata.json           # StemSetMetadata manifest
├── stem_01_drums.flac      # FLAC or 32-bit float WAV
├── stem_02_bass.flac
├── stem_03_vocals.flac
└── ...
```

### Rust Data Structures (`nullherz-dna`, `nullherz-traits`)

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct SingleStemMetadata {
    pub classification: StemClassification,
    pub relative_path: String,
    pub lufs_integrated: f32,
    pub peak_db: f32,
    pub dna: SoundDNA,
    pub mip_waveform: MipWaveform,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct StemSetMetadata {
    pub track_id: u64,
    pub tier: u8, // 4, 6, 8, or 12 stems
    pub stems: Vec<SingleStemMetadata>,
    pub created_at_timestamp: u64,
}
```

- **Memory-Mapped Buffers**: Loaded via `SampleBuffer::Mmap(Arc<MmapBuffer>)`, guaranteeing zero RAM duplication and sub-millisecond deck instantiation.

---

## 5. DSP Topology: `DeckStemMatrix` Processor

Each DJ Deck in `nullherz-processors` hosts a `DeckStemMatrix` DSP node:

- **Lock-Free Parameter Control**:
  - Per-stem Mute / Solo toggles.
  - Per-stem Gain (-inf dB to +12 dB) with linear ramp smoothing.
  - Per-stem Pan (-1.0 to +1.0).
  - Per-stem 3-Band Isolator EQ (`Low`, `Mid`, `High`).
- **Real-Time Analysis Routing**: Each active stem routes frames to `AnalysisKernel` for live per-stem feature extraction.

---

## 6. UI & Transfusion Integration

### DJ Console (`nullherz-inspector/src/views/dj_studio/`)
- Multi-lane colored stem waveform overlay.
- Interactive Mute/Solo buttons (`D`, `B`, `V`, `O`) per deck header.

### Breeder View (`nullherz-inspector/src/views/breeder.rs`)
- Multi-donor stem matrix cross-breeding:
  - Donor A: `Kick` & `Bass`
  - Donor B: `Vocals`
  - Donor C: `Guitars` & `Keys`
  - Donor D: `Micro-Timing / Groove`

---

## 7. Verification & Performance Benchmarks

- **Real-Time Memory Overhead**: < 1.2 MB per memory-mapped stem buffer.
- **DSP Block Execution**: < 0.15 ms total block budget for 12-stem matrix summing at 48 kHz / 128 frames.
- **Inference Fallback**: Instantaneous non-blocking switch to Causal Dilated TCN when offline separation is in progress.
