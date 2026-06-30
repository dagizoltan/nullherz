# RFC: AnaWaves Genetic Schema (Sound DNA Bit-Layout)

## 1. Goal
To define a compact, serializable, and DSP-efficient representation of "Sound DNA" to enable automated Transfusion and Evolutionary Synthesis.

## 2. Spectral Personality (1024 bits)
*   **Energy Map (512 bits):** 64 x 8-bit bins representing the normalized power spectrum (0-20kHz).
*   **Harmonicity (128 bits):** Ratio of periodic vs aperiodic energy across 8 octaves.
*   **Spectral Tilt (64 bits):** Float32 representing the spectral slope (brightness/darkness).
*   **Formant Peaks (320 bits):** Top 5 resonant peaks (Freq, Q, Gain).

## 3. Rhythmic DNA (512 bits)
*   **Onset Map (256 bits):** 64-step bitmask indicating significant transient density over a 4-bar window.
*   **Syncopation Index (64 bits):** Float32 measure of rhythmic complexity.
*   **Micro-Timing (192 bits):** Deviation profile from absolute grid (Early/Late bias).

## 4. Transfusion Logic (Pseudo-Code)
```rust
struct SoundDNA {
    spectral: SpectralPersonality,
    rhythmic: RhythmicDNA,
    artifacts: ArtifactProfile,
}

fn transfuse(parent_a: SoundDNA, parent_b: SoundDNA, bias: f32) -> SoundDNA {
    // Linear or non-linear interpolation of spectral bins
    // Probability-based merging of onset masks
    // Inheritance of noise profiles
}
```

## 5. Metadata Integration
This schema will be embedded into the `LibraryTrack` metadata in `nullherz-dna` and used by the `AnalysisWorker` to "tag" sounds as they are ingested or captured.
