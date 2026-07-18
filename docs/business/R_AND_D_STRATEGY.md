# Nullherz R&D Strategy: Proprietary Audio Intelligence

**Focus**: Transitioning from a performance engine to a category-defining R&D platform for advanced audio technology.

---

## 1. R&D Pillars

*Status labels added July 18, 2026, per the [Strategic Assessment](./STRATEGIC_ASSESSMENT_2026_07.md): R&D spend follows the identity decision, not the other way around.*

### 1.1 True Neural Latent Space — [RESEARCH; current implementation is a placeholder]
- **Objective**: Move beyond frequency-bin averaging to true timbral embeddings derived from Variational Autoencoders (VAEs).
- **Goal**: Enable SoundDNA to "understand" texture, weight, and grit, rather than just spectral distribution.
- **Honest baseline**: today's latent-space projection matrix is explicitly mocked in code (`nullherz-dna`, tracked in the debt log). The 16-D latent schema and SIMD interpolation are real; the *embedding* is not yet neural. Any pitch language must reflect this.

### 1.2 Zero-Copy Distributed Audio (RDMA) — [PARKED]
- **Objective**: Transition the network audio return path from UDP/TCP to Remote Direct Memory Access (RDMA).
- **Goal**: Achieve sub-100 microsecond network latency with zero CPU overhead for multi-machine DSP clusters.
- **Parked because**: no current user needs sub-100µs network audio; the shipped measured-path-delay clock sync + UDP return path serves the installation tier that Identity 3 targets. Revisit only if Identity 3 wins the gate *and* customers hit the UDP ceiling.

### 1.3 Neurofunk-Specialized DSP Kernels — [ACTIVE, gated on Stranger test]
- **Objective**: Develop mathematically-hardened kernels specifically for high-density transients and complex FM/Phase modulation common in Eatbrain-style production.
- **Goal**: Create a "Technical Moat" of proprietary algorithms that define the Eatbrain sound.
- **Sequencing**: kernel investment follows evidence that the Breeder experience lands with artists (Stranger test) — the kernels are the *deepening* of Identity 2, not a hedge against it.

---

## 2. Competitive Positioning
By targeting the R&D category, Nullherz competes not with traditional DAWs, but with internal tools at companies like Teenage Engineering, Akai, or major DSP research labs.

- **Valuation Driver**: Intellectual Property (IP) of neural audio compression and synthesis.
- **Strategic Advantage**: Real-time safety combined with AI-driven sound evolution.
