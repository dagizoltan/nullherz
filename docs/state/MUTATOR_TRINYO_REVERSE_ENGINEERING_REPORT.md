# Reverse Engineering Analysis Report: MUTATOR / TRINYÓ FX Plugin

**Document ID:** `MUTATOR_TRINYO_REVERSE_ENGINEERING_REPORT`
**Target Package:** `MUTATOR_TRINYO_BUILD_PACK_v1`
**Version:** `0.1.0-scaffold`
**Architecture Context:** Max for Live / RNBO / Max V8 JavaScript / AnaWaves Ecosystem

---

## 1. Executive Summary & Product Mission

**MUTATOR / TRINYÓ** is a real-time audio sound deformation engine designed for advanced sound design within the AnaWaves nightmare aesthetic. Unlike traditional sound generators or synthesizers, MUTATOR does not produce audio *ex nihilo*. Instead, it deforms the structural and timbral identity of an incoming audio signal ($X$) into controlled mutations, ensuring that the source remains recognizable at low-to-medium settings while allowing complete identity collapse at extreme settings.

### Core Mathematical Identity
$$\mathbf{Y} = \text{Mix}(\mathbf{X}, \mathbf{M}(\mathbf{X}, \Theta, S), \text{wet})$$

Where:
* $\mathbf{X}$ = Dry incoming stereo audio signal
* $\mathbf{M}$ = Mutation processing kernel
* $\Theta$ = Mutation macro control vector ($\Theta = [\text{flesh}, \text{bone}, \text{teeth}, \text{parasite}, \text{asymmetry}, \text{abomination}]$)
* $S$ = Deterministic integer seed state
* $\mathbf{Y}$ = Output audio signal after dry/wet crossfading and safety limiting

---

## 2. System Architecture & Tech Stack

The architecture separates control logic, real-time signal processing, and host user interface into three distinct execution layers:

```
+-----------------------------------------------------------------------+
| MAX / MAX FOR LIVE SHELL                                              |
| Host Automation, Parameter States, UI Controls, Presets, Transport   |
+-----------------------------------------------------------------------+
                                   |
                         Control Parameters
                                   v
+-----------------------------------------------------------------------+
| MAX V8 JAVASCRIPT ENGINE (Control & Logic Layer)                     |
| - seeded_rng.js (mulberry32 32-bit PRNG)                             |
| - mutation_map.js (Character macro weights & interpolation)           |
| - max_adapter.js (UI / State parameter binding & validation)          |
| * Note: Zero sample-by-sample DSP executed in JS                       |
+-----------------------------------------------------------------------+
                                   |
                           Control Messages
                                   v
+-----------------------------------------------------------------------+
| RNBO DSP CORE (Real-Time Audio Execution Graph)                       |
| - Preconditioning & Input Trim                                       |
| - Parallel Mutation Branches (FLESH, BONE, TEETH, PARASITE, ASYM)     |
| - Character Mapping & ABOMINATION Trajectory Curve                   |
| - Output Trim, Dry/Wet Crossfader & Hard Peak Safety Protection       |
+-----------------------------------------------------------------------+
```

---

## 3. Signal Flow & Processing Pipeline

MUTATOR uses a **parallel recombination model** rather than a destructive serial FX chain. Parallel recombination preserves source invariants (envelope, fundamental frequency, transient timing) across extreme processing.

```
       [ Stereo Input X ]
               |
               v
      [ Preconditioning ] ---> Envelope / Onset Analysis
               |
    +----------+----------+-----------------+-------------------+
    |                     |                 |                   |
    v                     v                 v                   v
[ FLESH ]              [ BONE ]         [ TEETH ]          [ PARASITE ]
(Formant / Body)  (Harmonics/Res)  (Transient Sat)   (Input Noise/AM/FM)
    |                     |                 |                   |
    +----------+----------+-----------------+-------------------+
               |
               v
         [ ASYMMETRY ] (Stereo / Micro-delay / L/R Imbalance)
               |
               v
      [ CHARACTER MAP ] (Diseased / Mechanical / Possessed / Abomination)
               |
               v
      [ ABOMINATION ] (Meta-macro trajectory warping)
               |
               v
      [ DRY / WET MIX ] <--- (Dry Input X)
               |
               v
    [ OUTPUT PROTECTION ] (Peak limit, NaN/Inf protection)
               |
               v
      [ Stereo Output Y ]
```

---

## 4. The 6 Mutation Macro Families

| Macro | Name | DSP Mechanism & Behavioral Description |
| :--- | :--- | :--- |
| **$\theta_1$** | **FLESH** | Formant & spectral body warping engine. Implements parallel resonant filters with slow, seeded low-frequency modulation for organic, vocalic, and non-linear low/mid frequency coloration. |
| **$\theta_2$** | **BONE** | Harmonic & partial structure deformation engine. Utilizes tuned resonator banks with adjustable inharmonic offset to create metallic, skeletal, or bell-like harmonic framing. |
| **$\theta_3$** | **TEETH** | Dual fast/slow envelope-follower transient detector driving edge enhancement and transient-gated saturation. Focuses distortion strictly on attack transients while leaving sustain pristine. |
| **$\theta_4$** | **PARASITE** | Input-dependent secondary organism. Generates envelope-gated filtered noise, micro-AM, and micro-FM synthesis. Automatically mutes during input silence to prevent residual noise floors. |
| **$\theta_5$** | **ASYMMETRY**| Complementary left/right channel modulation imbalance and bounded micro-delay (<20ms). Features low-end mono compatibility safety to prevent phase cancellation under 200 Hz. |
| **$\theta_6$** | **ABOMINATION**| Curved non-linear meta-macro trajectory. Controls non-linear cross-coupling across all modules: Low (0–35%): FLESH/BONE; Mid (35–70%): TEETH/PARASITE; High (70–90%): ASYMMETRY + cross-modulation; Top 10%: Bounded intentional instability. |

---

## 5. Preset Characters & Deterministic Seed Engine

### Character Macro Mapping Matrix

The control adapter maps 4 signature characters onto the 5 core module weights:

$$\begin{pmatrix}
\text{Diseased} \\
\text{Mechanical} \\
\text{Possessed} \\
\text{Abomination}
\end{pmatrix} =
\begin{pmatrix}
1.00 & 0.65 & 0.45 & 0.75 & 0.35 \\
0.30 & 1.00 & 0.80 & 0.55 & 0.60 \\
0.65 & 0.45 & 0.55 & 1.00 & 0.85 \\
0.90 & 0.90 & 0.85 & 0.95 & 1.00
\end{pmatrix}
\begin{pmatrix}
\text{flesh} \\
\text{bone} \\
\text{teeth} \\
\text{parasite} \\
\text{asymmetry}
\end{pmatrix}$$

### Deterministic PRNG Engine (`mulberry32`)

To ensure bit-exact preset reproducibility and automation recall across studio sessions, stochastic modulations rely on `seeded_rng.js`:

```javascript
export function mulberry32(seed) {
  let a = seed >>> 0;
  return function () {
    a |= 0;
    a = (a + 0x6D2B79F5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
```

---

## 6. Safety, Normalization, & Output Protection Rules

1. **Transparent Bypass:** At dry/wet = 0% or mutation macros = 0, signal path output $\mathbf{Y}$ equals input $\mathbf{X}$ identically.
2. **Loudness Bias Prevention:** Perceived quality must not rely on hidden volume boosts. Processing levels are normalized before dry/wet recombination.
3. **Silence Safety:** All internal resonators, feedback loops, and noise generators gate immediately when input $X \approx 0$.
4. **Numerical Stability:** Hard limiting and denormal/NaN/Inf protection on output stages prevent runaway feedback or digital clipping.

---

## 7. Development Lifecycle & Build State Analysis

According to `BUILD_STATE.json` and `BUILD_REPORT.md`:

- **Current Status:** Phase 1 (`0.1.0-scaffold`) - I/O + deterministic scaffold scaffolded.
- **7-Stage Roadmap:**
  1. Phase 1: I/O + deterministic scaffold (*In Progress*)
  2. Phase 2: FLESH + BONE DSP implementation
  3. Phase 3: TEETH + PARASITE DSP implementation
  4. Phase 4: ASYMMETRY + Safety implementation
  5. Phase 5: A/B/C/D Character Engine
  6. Phase 6: ABOMINATION + Morph trajectory
  7. Phase 7: UI + RNBO Runtime Validation
- **Runtime State:** Static structure, JSON schema, and CRC ZIP integrity tests pass. Max/RNBO runtime verification is pending execution in a Max 8 / Ableton Live environment.
